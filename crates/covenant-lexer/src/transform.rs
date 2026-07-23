//! Pass 2: transform `RawToken`s into fully-resolved `Token`s with diagnostics.
//!
//! Responsibilities:
//! * Keyword resolution (identifier → `Kw*` / `True` / `False` / `Underscore`).
//! * Duration-literal combining (`<int> <unit>` → single `Duration` token).
//! * Literal decoding (integers, hex, text) with overflow / format diagnostics.
//! * Forbidden comment markers (`//`, `/* */`) → E003 / E004.
//! * Bare CR (`\r` not followed by `\n`) → E008.
//! * Newline collapsing (multiple consecutive newlines → one `Newline` token).

use covenant_diag::{Diagnostic, SourceId, Span};
use logos::Logos;

use crate::diag;
use crate::literals::{self, HexDecode, TextDecode};
use crate::raw::{RawLexError, RawToken};
use crate::token::{match_duration_unit, resolve_keyword, DurationUnit, Token, TokenKind};

/// Tokenize a single source buffer.
///
/// Always returns a non-empty `Vec<Token>` terminated with `TokenKind::Eof`.
/// Errors are collected into the returned `Vec<Diagnostic>` rather than
/// aborting; callers should check `diags.is_empty()` before proceeding.
pub fn tokenize(source: &str, source_id: SourceId) -> (Vec<Token>, Vec<Diagnostic>) {
    // ---- Pass 1: run `logos` and materialize the raw token stream so pass 2 can
    //               peek ahead (needed for `<int> <unit>` duration combining).
    let mut raw: Vec<(Result<RawToken, RawLexError>, std::ops::Range<usize>)> = Vec::new();
    let mut lex = RawToken::lexer(source);
    while let Some(tok) = lex.next() {
        raw.push((tok, lex.span()));
    }

    let mut out: Vec<Token> = Vec::with_capacity(raw.len() + 1);
    let mut diags: Vec<Diagnostic> = Vec::new();
    let mk_span = |r: &std::ops::Range<usize>| Span::new(source_id, r.start as u32, r.end as u32);

    let mut i = 0;
    while i < raw.len() {
        let (tok, rng) = &raw[i];
        let span = mk_span(rng);
        let slice = &source[rng.clone()];

        match tok {
            // ---- Error from logos (unrecognized byte, or callback-raised error) ----
            Err(RawLexError::Generic) => {
                diags.push(diag::unexpected(span, slice));
                push(&mut out, Token::new(TokenKind::Error, span));
                i += 1;
            }
            Err(RawLexError::UnterminatedNestedComment) => {
                diags.push(diag::unterminated_nested(span));
                i += 1;
            }

            // ---- Comments (silent) ----
            Ok(RawToken::LineComment) | Ok(RawToken::NestedComment) => {
                i += 1;
            }

            // ---- Reserved C-style comments → diagnostics ----
            Ok(RawToken::ReservedSlashSlash) => {
                diags.push(diag::c_line_comment(span));
                push(&mut out, Token::new(TokenKind::Error, span));
                i += 1;
            }
            Ok(RawToken::ReservedSlashStar) => {
                diags.push(diag::c_block_comment(span));
                push(&mut out, Token::new(TokenKind::Error, span));
                i += 1;
            }

            // ---- Bare CR ----
            Ok(RawToken::BareCr) => {
                diags.push(diag::bare_cr(span));
                i += 1;
            }

            // ---- Newlines (collapse runs) ----
            Ok(RawToken::Newline) => {
                // If the last emitted token was already a Newline, skip.
                if !matches!(out.last().map(|t| &t.kind), Some(TokenKind::Newline)) {
                    push(&mut out, Token::new(TokenKind::Newline, span));
                }
                i += 1;
            }

            // ---- Integer literals (possibly fusing with duration unit) ----
            Ok(RawToken::IntLit) => {
                let value = match literals::parse_integer(slice) {
                    Some(v) => v,
                    None => {
                        diags.push(diag::int_overflow(span));
                        push(&mut out, Token::new(TokenKind::Error, span));
                        i += 1;
                        continue;
                    }
                };

                // Try duration fusion: next raw token is Ident matching a unit word.
                // A duration must live on a single line: do NOT cross a Newline,
                // BareCr, or a comment (those all reset the `<int> <unit>` pairing).
                if let Some((Ok(RawToken::Ident), next_rng)) = raw.get(i + 1) {
                    let next_slice = &source[next_rng.clone()];
                    if let Some(unit) = match_duration_unit(next_slice) {
                        if let Some(n64) = u64_from_u128(value) {
                            let joined =
                                Span::new(source_id, rng.start as u32, next_rng.end as u32);
                            push(&mut out, Token::new(TokenKind::Duration(n64, unit), joined));
                            i += 2;
                            continue;
                        } else {
                            // Unit is present but the count exceeds u64. Still report
                            // overflow on the integer itself rather than silently
                            // downgrading to `Integer` + `Ident`.
                            diags.push(diag::int_overflow(span));
                            let _ = unit; // suppress unused warning if refactored
                            push(&mut out, Token::new(TokenKind::Error, span));
                            i += 1;
                            continue;
                        }
                    }
                }

                push(&mut out, Token::new(TokenKind::Integer(value), span));
                i += 1;
            }

            // ---- Hex literals ----
            Ok(RawToken::HexLit) => {
                match literals::parse_hex(slice) {
                    HexDecode::Ok(bytes) => {
                        push(&mut out, Token::new(TokenKind::HexBytes(bytes), span));
                    }
                    HexDecode::Empty => {
                        diags.push(diag::hex_empty(span));
                        push(&mut out, Token::new(TokenKind::Error, span));
                    }
                    HexDecode::Odd => {
                        diags.push(diag::hex_odd(span));
                        push(&mut out, Token::new(TokenKind::Error, span));
                    }
                }
                i += 1;
            }

            // ---- Text literals ----
            Ok(RawToken::TextLit) => {
                match literals::parse_text(slice) {
                    TextDecode::Ok(s) => {
                        push(&mut out, Token::new(TokenKind::Text(s), span));
                    }
                    TextDecode::BadEscape { offset, fragment } => {
                        let start = rng.start + offset;
                        let end = (start + fragment.len()).min(rng.end);
                        let esc_span = Span::new(source_id, start as u32, end as u32);
                        diags.push(diag::bad_escape(esc_span, &fragment));
                        push(&mut out, Token::new(TokenKind::Error, span));
                    }
                }
                i += 1;
            }

            Ok(RawToken::UnterminatedText) => {
                diags.push(diag::unterminated_string(span));
                push(&mut out, Token::new(TokenKind::Error, span));
                i += 1;
            }

            // ---- Identifier-shaped → keyword resolution ----
            Ok(RawToken::Ident) => {
                let kind = resolve_keyword(slice);
                push(&mut out, Token::new(kind, span));
                i += 1;
            }

            // ---- Direct 1:1 mappings ----
            Ok(raw_tok) => {
                let kind = map_punctuation(raw_tok);
                push(&mut out, Token::new(kind, span));
                i += 1;
            }
        }
    }

    // Append EOF.
    let eof_pos = source.len() as u32;
    out.push(Token::new(
        TokenKind::Eof,
        Span::new(source_id, eof_pos, eof_pos),
    ));

    (out, diags)
}

fn push(out: &mut Vec<Token>, tok: Token) {
    out.push(tok);
}

fn u64_from_u128(v: u128) -> Option<u64> {
    if v <= u64::MAX as u128 {
        Some(v as u64)
    } else {
        None
    }
}

/// Map raw punctuation/operator variants to `TokenKind`.
///
/// Literals, identifiers, comments, newlines, and errors are handled inline in
/// `tokenize`; this function covers the purely mechanical 1:1 cases.
fn map_punctuation(raw: &RawToken) -> TokenKind {
    match raw {
        RawToken::LParen => TokenKind::LParen,
        RawToken::RParen => TokenKind::RParen,
        RawToken::LBracket => TokenKind::LBracket,
        RawToken::RBracket => TokenKind::RBracket,
        RawToken::LBrace => TokenKind::LBrace,
        RawToken::RBrace => TokenKind::RBrace,
        RawToken::Comma => TokenKind::Comma,
        RawToken::Semicolon => TokenKind::Semicolon,
        RawToken::Colon => TokenKind::Colon,
        RawToken::ColonColon => TokenKind::ColonColon,
        RawToken::Dot => TokenKind::Dot,
        RawToken::DotDot => TokenKind::DotDot,
        RawToken::Arrow => TokenKind::Arrow,
        RawToken::FatArrow => TokenKind::FatArrow,
        RawToken::At => TokenKind::At,

        RawToken::Plus => TokenKind::Plus,
        RawToken::Minus => TokenKind::Minus,
        RawToken::Star => TokenKind::Star,
        RawToken::Slash => TokenKind::Slash,
        RawToken::Percent => TokenKind::Percent,
        RawToken::PlusEq => TokenKind::PlusEq,
        RawToken::MinusEq => TokenKind::MinusEq,
        RawToken::StarEq => TokenKind::StarEq,
        RawToken::SlashEq => TokenKind::SlashEq,
        RawToken::PercentEq => TokenKind::PercentEq,
        RawToken::Eq => TokenKind::Eq,
        RawToken::EqEq => TokenKind::EqEq,
        RawToken::NotEq => TokenKind::NotEq,
        RawToken::Lt => TokenKind::Lt,
        RawToken::LtEq => TokenKind::LtEq,
        RawToken::Gt => TokenKind::Gt,
        RawToken::GtEq => TokenKind::GtEq,
        RawToken::AmpAmp => TokenKind::AmpAmp,
        RawToken::PipePipe => TokenKind::PipePipe,
        RawToken::Bang => TokenKind::Bang,
        RawToken::Amp => TokenKind::Amp,
        RawToken::Pipe => TokenKind::Pipe,
        RawToken::Caret => TokenKind::Caret,
        RawToken::Tilde => TokenKind::Tilde,
        RawToken::ShiftLeft => TokenKind::ShiftLeft,
        RawToken::ShiftRight => TokenKind::ShiftRight,
        RawToken::PlusPlus => TokenKind::PlusPlus,

        // The variants below are handled inline in `tokenize` and should never
        // reach this function. Treat as a logic error via unreachable!().
        RawToken::IntLit
        | RawToken::HexLit
        | RawToken::TextLit
        | RawToken::UnterminatedText
        | RawToken::Ident
        | RawToken::LineComment
        | RawToken::NestedComment
        | RawToken::ReservedSlashSlash
        | RawToken::ReservedSlashStar
        | RawToken::Newline
        | RawToken::BareCr => unreachable!("raw variant {:?} handled by caller", raw),
    }
}

// Keep the DurationUnit type linked; silence the unused-import warning when it's
// only referenced in fused duration tokens above.
#[allow(dead_code)]
fn _unit_link(_u: DurationUnit) {}

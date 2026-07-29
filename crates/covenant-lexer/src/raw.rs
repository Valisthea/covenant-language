//! Pass-1 raw tokenizer using `logos`.
//!
//! Produces a stream of minimally-distinguished tokens (numbers, idents, punctuation,
//! comments, whitespace). Pass 2 (`crate::transform`) converts these into typed
//! `Token`s with keyword resolution and duration-literal combining.
//!
//! Design choices that matter:
//!
//! * `--` line comments, `(* ... *)` nesting comments, and `//` / `/* */` reserved
//!   C-style markers are all recognized here so the span information is precise.
//!   Whether a marker is a silently-consumed comment or an error-producing
//!   reserved form is decided in pass 2.
//! * Whitespace (spaces, tabs) is *skipped*; newlines (`\n`, `\r\n`) are *emitted*
//!   so the parser can use them as statement separators.
//! * A bare `\r` not followed by `\n` is emitted as a `BareCr` token so pass 2 can
//!   raise diagnostic E008.

use logos::{Lexer, Logos};

/// Errors that can be returned by a `logos` callback and converted to diagnostics
/// in pass 2.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RawLexError {
    #[default]
    Generic,
    UnterminatedNestedComment,
}

/// Pass-1 token produced by `logos`.
///
/// The enum is deliberately coarse: pass 2 does keyword mapping, duration-literal
/// combining, and diagnostic emission. Comments are retained so we can distinguish
/// "real" (`--`, `(* *)`) from "reserved" (`//`, `/* */`) forms.
#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(error = RawLexError)]
#[logos(skip r"[ \t]+")]
pub enum RawToken {
    // --- Literals ---
    #[regex(r"[0-9][0-9_]*")]
    IntLit,

    #[regex(r"0x[0-9a-fA-F_]*")]
    HexLit,

    #[regex(r#""([^"\\\r\n]|\\.)*""#)]
    TextLit,

    // Unterminated string: run of bytes starting with `"` that never closes before
    // end-of-line or end-of-input. Pass 2 emits E002.
    #[regex(r#""([^"\\\r\n]|\\.)*"#)]
    UnterminatedText,

    // --- Identifier-shaped (incl. bare `_`) ---
    #[regex(r"[A-Za-z_][A-Za-z0-9_]*")]
    Ident,

    // --- Comments ---
    // Line comment: `-- ...` to end of line (does NOT consume the newline).
    #[regex(r"--[^\r\n]*")]
    LineComment,

    // Nested `(* ... *)`; callback consumes the body.
    #[token("(*", consume_nested_comment)]
    NestedComment,

    // Reserved C-style single-line comment. Pass 2 emits E003.
    // Consumes through end-of-line (but not the terminator itself).
    #[regex(r"//[^\r\n]*")]
    ReservedSlashSlash,

    // Reserved C-style multi-line comment opener; callback skips body (up to */).
    // Pass 2 emits E004; unterminated variants also emit E009-ish.
    #[token("/*", consume_c_block_comment)]
    ReservedSlashStar,

    // --- Structural ---
    #[token("\n")]
    #[token("\r\n")]
    Newline,

    // Bare CR (not followed by LF). Pass 2 emits E008.
    #[token("\r")]
    BareCr,

    // --- Punctuation ---
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token(",")]
    Comma,
    #[token(";")]
    Semicolon,
    #[token("::")]
    ColonColon,
    #[token(":")]
    Colon,
    #[token("..")]
    DotDot,
    #[token(".")]
    Dot,
    #[token("->")]
    Arrow,
    #[token("=>")]
    FatArrow,
    #[token("@")]
    At,

    // --- Operators ---
    #[token("++")]
    PlusPlus,
    #[token("+=")]
    PlusEq,
    #[token("+")]
    Plus,
    #[token("-=")]
    MinusEq,
    #[token("-")]
    Minus,
    #[token("*=")]
    StarEq,
    #[token("*")]
    Star,
    #[token("/=")]
    SlashEq,
    #[token("/")]
    Slash,
    #[token("%=")]
    PercentEq,
    #[token("%")]
    Percent,
    #[token("==")]
    EqEq,
    #[token("=")]
    Eq,
    #[token("!=")]
    NotEq,
    #[token("!")]
    Bang,
    #[token("<=")]
    LtEq,
    #[token("<<")]
    ShiftLeft,
    #[token("<")]
    Lt,
    #[token(">=")]
    GtEq,
    #[token(">>")]
    ShiftRight,
    #[token(">")]
    Gt,
    #[token("&&")]
    AmpAmp,
    #[token("&")]
    Amp,
    #[token("||")]
    PipePipe,
    #[token("|")]
    Pipe,
    #[token("^")]
    Caret,
    #[token("~")]
    Tilde,
}

/// Callback for `(* ... *)` comments: walks the body tracking depth so that
/// `(* outer (* inner *) still outer *)` parses as one comment.
fn consume_nested_comment(lex: &mut Lexer<'_, RawToken>) -> Result<(), RawLexError> {
    let remainder = lex.remainder();
    let bytes = remainder.as_bytes();
    let mut depth: i32 = 1;
    let mut i: usize = 0;
    while i + 1 < bytes.len() && depth > 0 {
        match (bytes[i], bytes[i + 1]) {
            (b'(', b'*') => {
                depth += 1;
                i += 2;
            }
            (b'*', b')') => {
                depth -= 1;
                i += 2;
            }
            _ => i += 1,
        }
    }
    if depth > 0 {
        // Consume the rest so we don't re-enter the comment, but signal the error.
        lex.bump(remainder.len());
        return Err(RawLexError::UnterminatedNestedComment);
    }
    lex.bump(i);
    Ok(())
}

/// Callback for reserved `/* ... */` comments. We still consume the body (so the
/// lexer doesn't vomit further errors on the inside) but pass 2 raises a
/// diagnostic for the opening `/*`.
fn consume_c_block_comment(lex: &mut Lexer<'_, RawToken>) -> Result<(), RawLexError> {
    let remainder = lex.remainder();
    let bytes = remainder.as_bytes();
    let mut i: usize = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'*' && bytes[i + 1] == b'/' {
            lex.bump(i + 2);
            return Ok(());
        }
        i += 1;
    }
    // Unterminated: consume to end.
    lex.bump(remainder.len());
    Ok(())
}

//! Core parser struct and low-level token primitives.
//!
//! Every higher-level parse method (expressions, statements, declarations,
//! types) lives in a sibling module but hangs off `Parser<'a>` via its own
//! `impl` block. This file owns the *vocabulary*: how to peek, eat, expect, and
//! synchronize.

use covenant_diag::{Diagnostic, SourceId, Span};
use covenant_lexer::{Token, TokenKind};

use crate::ast::Ident;
use crate::diag;

/// Sentinel value returned by primitives when they fail to produce an AST node
/// and have already pushed a diagnostic. Methods should `?` on this to bail out
/// of the immediate construct and let a higher level synchronize.
#[derive(Debug)]
pub struct ParseError;

/// Maximum recursive-descent nesting depth for expressions and blocks.
///
/// OMEGA V6 (HGH-029 fix): a real crash threshold of ~140 nested parens was
/// observed against a fresh debug build on the default OS thread stack; this
/// is set well under that with margin for other stages' larger stack frames.
pub(crate) const MAX_PARSE_DEPTH: u32 = 96;

pub struct Parser<'a> {
    pub(crate) tokens: &'a [Token],
    pub(crate) pos: usize,
    /// Number of open `(` / `[` groups minus closes.
    /// When > 0, the parser silently skips `Newline` tokens.
    pub(crate) paren_depth: u32,
    /// Current recursive-descent nesting depth (expressions + blocks
    /// combined). Guarded by `enter_depth`/`exit_depth` to bound stack usage.
    pub(crate) nest_depth: u32,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) source_id: SourceId,
}

impl<'a> Parser<'a> {
    pub fn new(tokens: &'a [Token], source_id: SourceId) -> Self {
        Self {
            tokens,
            pos: 0,
            paren_depth: 0,
            nest_depth: 0,
            diagnostics: Vec::new(),
            source_id,
        }
    }

    /// Enter one level of recursive-descent nesting (expression or block).
    /// Returns `Err` without incrementing once the depth limit is reached,
    /// having already pushed a diagnostic -- callers should `?` this and
    /// bail out like any other parse error.
    pub(crate) fn enter_depth(&mut self) -> Result<(), ParseError> {
        if self.nest_depth >= MAX_PARSE_DEPTH {
            let span = self.current_span();
            self.push_diag(diag::too_deeply_nested(span));
            return Err(ParseError);
        }
        self.nest_depth += 1;
        Ok(())
    }

    /// Leave one level of recursive-descent nesting. Must be paired with a
    /// successful `enter_depth`.
    pub(crate) fn exit_depth(&mut self) {
        self.nest_depth = self.nest_depth.saturating_sub(1);
    }

    // -------- low-level cursor --------

    /// Return the current token, skipping newlines if we're inside a
    /// parenthesized/bracketed group.
    pub(crate) fn peek(&self) -> Option<&'a Token> {
        let mut i = self.pos;
        while i < self.tokens.len() {
            let t = &self.tokens[i];
            if self.paren_depth > 0 && matches!(t.kind, TokenKind::Newline) {
                i += 1;
                continue;
            }
            return Some(t);
        }
        None
    }

    pub(crate) fn peek_kind(&self) -> Option<&'a TokenKind> {
        self.peek().map(|t| &t.kind)
    }

    /// Peek at `offset` tokens ahead of the current position (offset 0 is the
    /// next token that `peek` would return). Skips newlines within groups the
    /// same way `peek` does.
    pub(crate) fn peek_ahead(&self, offset: usize) -> Option<&'a Token> {
        let mut i = self.pos;
        let mut seen = 0usize;
        while i < self.tokens.len() {
            let t = &self.tokens[i];
            if self.paren_depth > 0 && matches!(t.kind, TokenKind::Newline) {
                i += 1;
                continue;
            }
            if seen == offset {
                return Some(t);
            }
            seen += 1;
            i += 1;
        }
        None
    }

    /// Advance past the next non-skipped token, updating `paren_depth` when we
    /// cross grouping punctuation.
    pub(crate) fn advance(&mut self) -> Option<&'a Token> {
        // Skip newlines inside groups.
        while self.pos < self.tokens.len() {
            let t = &self.tokens[self.pos];
            if self.paren_depth > 0 && matches!(t.kind, TokenKind::Newline) {
                self.pos += 1;
                continue;
            }
            break;
        }
        if self.pos >= self.tokens.len() {
            return None;
        }
        let t = &self.tokens[self.pos];
        self.pos += 1;
        match t.kind {
            TokenKind::LParen | TokenKind::LBracket => {
                self.paren_depth = self.paren_depth.saturating_add(1);
            }
            TokenKind::RParen | TokenKind::RBracket => {
                self.paren_depth = self.paren_depth.saturating_sub(1);
            }
            _ => {}
        }
        Some(t)
    }

    /// Is the current token equal to `kind` (ignoring span/payload)?
    pub(crate) fn at(&self, kind: &TokenKind) -> bool {
        self.peek_kind().is_some_and(|k| token_kind_eq(k, kind))
    }

    /// Consume if the current kind matches.
    pub(crate) fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.at(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    /// Consume a specific token kind or push a diagnostic and return `Err`.
    pub(crate) fn expect(&mut self, kind: &TokenKind, what: &str) -> Result<&'a Token, ParseError> {
        if self.at(kind) {
            // `advance` is guaranteed to return Some because `at` succeeded.
            let t = self.advance().expect("at() implies a token is present");
            Ok(t)
        } else {
            let span = self.current_span();
            let got = self
                .peek()
                .map(|t| describe(&t.kind))
                .unwrap_or_else(|| "end of file".into());
            self.diagnostics.push(diag::expected(span, what, &got));
            Err(ParseError)
        }
    }

    /// Skip any run of `Newline` tokens at the current position (outside groups).
    pub(crate) fn skip_newlines(&mut self) {
        while matches!(self.peek_kind(), Some(TokenKind::Newline)) {
            self.pos += 1;
        }
    }

    /// Return the span of the current token, or a synthesized one-byte span at
    /// the last token position if we're already at EOF.
    pub(crate) fn current_span(&self) -> Span {
        if let Some(t) = self.peek() {
            return t.span;
        }
        if let Some(last) = self.tokens.last() {
            return last.span;
        }
        Span::new(self.source_id, 0, 0)
    }

    /// Expect an identifier token and return it as an `Ident` AST node.
    ///
    /// Also accepts type keywords (e.g. `duration`, `amount`) as identifiers,
    /// since the Basics fixtures use them in field-name and expression-reference
    /// positions. This matches Codex usage even though Doc 2 §1.5 lists them as
    /// reserved — flagged as a spec ambiguity.
    pub(crate) fn expect_ident(&mut self, what: &str) -> Result<Ident, ParseError> {
        match self.peek() {
            Some(t) => {
                if let TokenKind::Ident(name) = &t.kind {
                    let ident = Ident {
                        name: name.clone(),
                        span: t.span,
                    };
                    self.advance();
                    return Ok(ident);
                }
                if let Some(text) = keyword_as_ident_text(&t.kind) {
                    let ident = Ident {
                        name: text.into(),
                        span: t.span,
                    };
                    self.advance();
                    return Ok(ident);
                }
                let span = t.span;
                let got = describe(&t.kind);
                self.diagnostics.push(diag::expected(span, what, &got));
                Err(ParseError)
            }
            None => {
                let span = self.current_span();
                self.diagnostics
                    .push(diag::unexpected_eof(span, &format!("expecting {what}")));
                Err(ParseError)
            }
        }
    }

    /// Consume an optional statement terminator: `Newline`, `Semicolon`, or an
    /// implicit terminator before `}` / EOF. Does not consume the closing brace.
    pub(crate) fn expect_stmt_terminator(&mut self, _what: &str) -> Result<(), ParseError> {
        match self.peek_kind() {
            Some(TokenKind::Newline) | Some(TokenKind::Semicolon) => {
                self.advance();
                self.skip_newlines();
                Ok(())
            }
            Some(TokenKind::RBrace) | Some(TokenKind::Eof) | None => Ok(()),
            Some(other) => {
                let span = self.current_span();
                let got = describe(other);
                self.diagnostics.push(diag::bad_stmt_terminator(span, &got));
                Err(ParseError)
            }
        }
    }

    /// Push a diagnostic and skip until a sync point. Used during error recovery.
    pub(crate) fn recover_to(&mut self, sync: &[TokenKind]) {
        while let Some(k) = self.peek_kind() {
            if matches!(k, TokenKind::Eof) {
                return;
            }
            if sync.iter().any(|s| token_kind_eq(k, s)) {
                return;
            }
            self.pos += 1;
        }
    }

    pub(crate) fn push_diag(&mut self, d: Diagnostic) {
        self.diagnostics.push(d);
    }

    pub(crate) fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    /// Like `expect_ident` but also accepts ALL keyword tokens as identifiers.
    /// Used in `external contract` parsing where Solidity-conventional names
    /// (e.g. `transfer`, `token`, `approve`) coincide with Covenant keywords.
    pub(crate) fn expect_ident_or_any_keyword(&mut self, what: &str) -> Result<Ident, ParseError> {
        match self.peek() {
            Some(t) => {
                if let TokenKind::Ident(name) = &t.kind {
                    let ident = Ident {
                        name: name.clone(),
                        span: t.span,
                    };
                    self.advance();
                    return Ok(ident);
                }
                if let Some(text) = any_keyword_as_ident_text(&t.kind) {
                    let ident = Ident {
                        name: text.into(),
                        span: t.span,
                    };
                    self.advance();
                    return Ok(ident);
                }
                let span = t.span;
                let got = describe(&t.kind);
                self.diagnostics.push(diag::expected(span, what, &got));
                Err(ParseError)
            }
            None => {
                let span = self.current_span();
                self.diagnostics
                    .push(diag::unexpected_eof(span, &format!("expecting {what}")));
                Err(ParseError)
            }
        }
    }
}

/// If `k` is a type keyword that can also appear as an identifier in the Codex
/// Basics fixtures, return its canonical string form.
pub(crate) fn keyword_as_ident_text(k: &TokenKind) -> Option<&'static str> {
    use TokenKind::*;
    match k {
        KwAmount => Some("amount"),
        KwTime => Some("time"),
        KwDuration => Some("duration"),
        KwHash => Some("hash"),
        KwTextTy => Some("text"),
        KwAddress => Some("address"),
        KwChoice => Some("choice"),
        KwPqKey => Some("pq_key"),
        KwBytes => Some("bytes"),
        KwBool => Some("bool"),
        _ => None,
    }
}

/// Map ANY keyword to its source text. Used in contexts where keywords are
/// valid as identifiers (e.g., external contract function names).
pub(crate) fn any_keyword_as_ident_text(k: &TokenKind) -> Option<&'static str> {
    use TokenKind::*;
    // First try the standard whitelist.
    if let Some(s) = keyword_as_ident_text(k) {
        return Some(s);
    }
    // Then allow all remaining keywords.
    match k {
        KwModule => Some("module"),
        KwRecord => Some("record"),
        KwToken => Some("token"),
        KwBallot => Some("ballot"),
        KwCounter => Some("counter"),
        KwBoard => Some("board"),
        KwMarket => Some("market"),
        KwVault => Some("vault"),
        KwRegistry => Some("registry"),
        KwBridge => Some("bridge"),
        KwCeremony => Some("ceremony"),
        KwTest => Some("test"),
        KwPublic => Some("public"),
        KwPrivate => Some("private"),
        KwEncrypted => Some("encrypted"),
        KwSealed => Some("sealed"),
        KwHybrid => Some("hybrid"),
        KwConfidential => Some("confidential"),
        KwAction => Some("action"),
        KwView => Some("view"),
        KwReveal => Some("reveal"),
        KwWhen => Some("when"),
        KwOnly => Some("only"),
        KwGiven => Some("given"),
        KwMap => Some("map"),
        KwPriorityQueue => Some("priority_queue"),
        KwShares => Some("shares"),
        KwField => Some("field"),
        KwEvent => Some("event"),
        KwError => Some("error"),
        KwStruct => Some("struct"),
        KwCredential => Some("credential"),
        KwVersion => Some("version"),
        KwMatch => Some("match"),
        KwLet => Some("let"),
        KwReturn => Some("return"),
        KwFor => Some("for"),
        KwEach => Some("each"),
        KwIn => Some("in"),
        KwIf => Some("if"),
        KwElse => Some("else"),
        KwEncryptedWhen => Some("encrypted_when"),
        KwOtherwise => Some("otherwise"),
        KwTryAction => Some("try_action"),
        KwCatch => Some("catch"),
        KwRevertWith => Some("revert_with"),
        KwOnDestroy => Some("on_destroy"),
        KwMigrate => Some("migrate"),
        KwFrom => Some("from"),
        KwTo => Some("to"),
        KwAnchoredOn => Some("anchored_on"),
        KwUpgradeableBy => Some("upgradeable_by"),
        KwSelectiveDisclosure => Some("selective_disclosure"),
        KwUsing => Some("using"),
        KwAs => Some("as"),
        KwReturns => Some("returns"),
        KwImport => Some("import"),
        KwEmit => Some("emit"),
        KwTransfer => Some("transfer"),
        KwAppend => Some("append"),
        KwDiscard => Some("discard"),
        KwDelete => Some("delete"),
        KwVerifiedBy => Some("verified_by"),
        KwPqSigned => Some("pq_signed"),
        KwVdfLocked => Some("vdf_locked"),
        KwExternal => Some("external"),
        KwContract => Some("contract"),
        KwFunction => Some("function"),
        _ => None,
    }
}

/// Kind-equality that ignores payload for variants carrying data.
pub(crate) fn token_kind_eq(a: &TokenKind, b: &TokenKind) -> bool {
    use TokenKind::*;
    match (a, b) {
        (Integer(_), Integer(_))
        | (HexBytes(_), HexBytes(_))
        | (Text(_), Text(_))
        | (Duration(_, _), Duration(_, _))
        | (Ident(_), Ident(_)) => true,
        _ => std::mem::discriminant(a) == std::mem::discriminant(b),
    }
}

/// Human-readable description of a token kind for use in diagnostics.
pub(crate) fn describe(k: &TokenKind) -> String {
    use TokenKind::*;
    match k {
        Integer(_) => "an integer literal".to_string(),
        HexBytes(_) => "a hex literal".to_string(),
        Text(_) => "a text literal".to_string(),
        Duration(_, _) => "a duration literal".to_string(),
        Ident(s) => format!("identifier `{s}`"),
        True => "`true`".to_string(),
        False => "`false`".to_string(),
        Newline => "newline".to_string(),
        Eof => "end of file".to_string(),
        LParen => "`(`".to_string(),
        RParen => "`)`".to_string(),
        LBracket => "`[`".to_string(),
        RBracket => "`]`".to_string(),
        LBrace => "`{`".to_string(),
        RBrace => "`}`".to_string(),
        Comma => "`,`".to_string(),
        Semicolon => "`;`".to_string(),
        Colon => "`:`".to_string(),
        ColonColon => "`::`".to_string(),
        Dot => "`.`".to_string(),
        DotDot => "`..`".to_string(),
        Arrow => "`->`".to_string(),
        FatArrow => "`=>`".to_string(),
        At => "`@`".to_string(),
        Underscore => "`_`".to_string(),
        Plus => "`+`".to_string(),
        Minus => "`-`".to_string(),
        Star => "`*`".to_string(),
        Slash => "`/`".to_string(),
        Percent => "`%`".to_string(),
        PlusEq => "`+=`".to_string(),
        MinusEq => "`-=`".to_string(),
        StarEq => "`*=`".to_string(),
        SlashEq => "`/=`".to_string(),
        PercentEq => "`%=`".to_string(),
        Eq => "`=`".to_string(),
        EqEq => "`==`".to_string(),
        NotEq => "`!=`".to_string(),
        Lt => "`<`".to_string(),
        LtEq => "`<=`".to_string(),
        Gt => "`>`".to_string(),
        GtEq => "`>=`".to_string(),
        AmpAmp => "`&&`".to_string(),
        PipePipe => "`||`".to_string(),
        Bang => "`!`".to_string(),
        Amp => "`&`".to_string(),
        Pipe => "`|`".to_string(),
        Caret => "`^`".to_string(),
        Tilde => "`~`".to_string(),
        ShiftLeft => "`<<`".to_string(),
        ShiftRight => "`>>`".to_string(),
        PlusPlus => "`++`".to_string(),
        Error => "<error token>".to_string(),
        other => format!("{other:?}"),
    }
}

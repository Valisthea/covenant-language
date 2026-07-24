//! Type parsing.
//!
//! Covers primitive types, `encrypted T`, `[T]`, `map<K, V>`, `priority_queue<K, V, max|min>`,
//! `shares(n of k among expr)`, and user-defined type references.

use covenant_lexer::TokenKind;

use crate::ast::{ChoiceValue, Expr, Ident, Ordering, Type};
use crate::diag;
use crate::parser::{describe, ParseError, Parser};

impl<'a> Parser<'a> {
    /// Parse a type expression. Returns a synthesized `Type::User("<error>")`
    /// on failure (the diagnostic is already recorded).
    ///
    /// Wraps [`parse_type_body`] in a depth guard so a deeply nested type
    /// (`map<address, map<address, ...>>`) fails loud with E032 instead of
    /// recursing until the native stack overflows (F06).
    pub(crate) fn parse_type(&mut self) -> Result<Type, ParseError> {
        self.enter_type_depth()?;
        let result = self.parse_type_body();
        self.exit_type_depth();
        result
    }

    fn parse_type_body(&mut self) -> Result<Type, ParseError> {
        let t = match self.peek() {
            Some(t) => t,
            None => {
                let span = self.current_span();
                self.push_diag(diag::type_expected(span));
                return Err(ParseError);
            }
        };
        let start = t.span;
        match &t.kind {
            TokenKind::KwAmount => {
                self.advance();
                Ok(Type::Amount(start))
            }
            TokenKind::KwTime => {
                self.advance();
                Ok(Type::Time(start))
            }
            TokenKind::KwDuration => {
                self.advance();
                Ok(Type::Duration(start))
            }
            TokenKind::KwHash => {
                self.advance();
                Ok(Type::Hash(start))
            }
            TokenKind::KwTextTy => {
                self.advance();
                Ok(Type::Text(start))
            }
            TokenKind::KwAddress => {
                self.advance();
                Ok(Type::Address(start))
            }
            TokenKind::KwPqKey => {
                self.advance();
                Ok(Type::PqKey(start))
            }
            TokenKind::KwBytes => {
                self.advance();
                Ok(Type::Bytes(start))
            }
            TokenKind::KwBool => {
                self.advance();
                Ok(Type::Bool(start))
            }
            TokenKind::KwChoice => {
                self.advance();
                // Inline form `choice = [ "a", "b" ]` is handled by the field parser —
                // the type itself is just `choice` here.
                Ok(Type::Choice(start, Vec::new()))
            }
            TokenKind::KwEncrypted => {
                self.advance();
                let inner = self.parse_type()?;
                let span = start.join(inner_span(&inner));
                Ok(Type::Encrypted(Box::new(inner), span))
            }
            TokenKind::KwCiphertext => {
                // `ciphertext<T>`
                self.advance();
                self.expect(&TokenKind::Lt, "`<`")?;
                let inner = self.parse_type()?;
                self.expect(&TokenKind::Gt, "`>`")?;
                let span = start.join(inner_span(&inner));
                Ok(Type::Encrypted(Box::new(inner), span))
            }
            TokenKind::LBracket => {
                self.advance();
                let inner = self.parse_type()?;
                let rb = self.expect(&TokenKind::RBracket, "`]`")?;
                Ok(Type::List(Box::new(inner), start.join(rb.span)))
            }
            TokenKind::KwMap => {
                self.advance();
                self.expect(&TokenKind::Lt, "`<`")?;
                let k = self.parse_type()?;
                self.expect(&TokenKind::Comma, "`,`")?;
                let v = self.parse_type()?;
                let gt = self.expect(&TokenKind::Gt, "`>`")?;
                Ok(Type::Map(Box::new(k), Box::new(v), start.join(gt.span)))
            }
            TokenKind::KwPriorityQueue => {
                self.advance();
                self.expect(&TokenKind::Lt, "`<`")?;
                let k = self.parse_type()?;
                self.expect(&TokenKind::Comma, "`,`")?;
                let v = self.parse_type()?;
                self.expect(&TokenKind::Comma, "`,`")?;
                let ord = self.parse_ordering()?;
                let gt = self.expect(&TokenKind::Gt, "`>`")?;
                Ok(Type::PriorityQueue(
                    Box::new(k),
                    Box::new(v),
                    ord,
                    start.join(gt.span),
                ))
            }
            TokenKind::KwShares => {
                self.advance();
                self.parse_shares_after_kw(start)
            }
            TokenKind::Ident(name) => {
                let ident = Ident {
                    name: name.clone(),
                    span: t.span,
                };
                self.advance();
                Ok(Type::User(ident))
            }
            other => {
                let span = start;
                let desc = describe(other);
                self.push_diag(diag::expected(span, "a type", &desc));
                Err(ParseError)
            }
        }
    }

    fn parse_ordering(&mut self) -> Result<Ordering, ParseError> {
        match self.peek_kind() {
            Some(TokenKind::Ident(name)) if name.as_ref() == "max" => {
                self.advance();
                Ok(Ordering::Max)
            }
            Some(TokenKind::Ident(name)) if name.as_ref() == "min" => {
                self.advance();
                Ok(Ordering::Min)
            }
            _ => {
                let span = self.current_span();
                let got = self
                    .peek()
                    .map(|t| describe(&t.kind))
                    .unwrap_or_else(|| "end of file".into());
                self.push_diag(diag::expected(span, "`max` or `min`", &got));
                Err(ParseError)
            }
        }
    }

    /// Parse the rest of a `shares(n of k among expr)` type after the `shares`
    /// keyword has been consumed. `of` and `among` are contextual identifiers
    /// at the lexer level (not reserved words).
    fn parse_shares_after_kw(&mut self, start: covenant_diag::Span) -> Result<Type, ParseError> {
        self.expect(&TokenKind::LParen, "`(`")?;
        let n = self.expect_u32_literal("shares count `n`")?;
        self.expect_contextual_ident("of")?;
        let k = self.expect_u32_literal("shares threshold `k`")?;
        self.expect_contextual_ident("among")?;
        let parties = self.parse_expr()?;
        let rp = self.expect(&TokenKind::RParen, "`)`")?;
        Ok(Type::Shares {
            n,
            k,
            parties: Box::new(parties),
            span: start.join(rp.span),
        })
    }

    fn expect_u32_literal(&mut self, what: &str) -> Result<u32, ParseError> {
        match self.peek() {
            Some(t) => {
                if let TokenKind::Integer(v) = &t.kind {
                    if *v > u32::MAX as u128 {
                        self.push_diag(diag::bad_shares(t.span, "value exceeds u32 range"));
                        return Err(ParseError);
                    }
                    let out = *v as u32;
                    self.advance();
                    Ok(out)
                } else {
                    let span = t.span;
                    let got = describe(&t.kind);
                    self.push_diag(diag::expected(span, what, &got));
                    Err(ParseError)
                }
            }
            None => {
                let span = self.current_span();
                self.push_diag(diag::unexpected_eof(span, &format!("expecting {what}")));
                Err(ParseError)
            }
        }
    }

    /// Consume an `Ident` whose text equals `word`, producing a diagnostic
    /// otherwise. Used for contextual keywords like `of`, `among`, `indexed`.
    pub(crate) fn expect_contextual_ident(&mut self, word: &str) -> Result<(), ParseError> {
        match self.peek() {
            Some(t) => {
                if let TokenKind::Ident(name) = &t.kind {
                    if name.as_ref() == word {
                        self.advance();
                        return Ok(());
                    }
                }
                let span = t.span;
                let got = describe(&t.kind);
                self.push_diag(diag::expected(span, &format!("`{word}`"), &got));
                Err(ParseError)
            }
            None => {
                let span = self.current_span();
                self.push_diag(diag::unexpected_eof(span, &format!("expecting `{word}`")));
                Err(ParseError)
            }
        }
    }

    /// Parse inline choice values `[ "a", "b", "c" ]` that follow a `=` in a
    /// field with `choice` type. The `[` is expected to be current.
    pub(crate) fn parse_inline_choice_values(&mut self) -> Result<Vec<ChoiceValue>, ParseError> {
        self.expect(&TokenKind::LBracket, "`[`")?;
        let mut values = Vec::new();
        if !self.at(&TokenKind::RBracket) {
            loop {
                let t = match self.peek() {
                    Some(t) => t,
                    None => {
                        let span = self.current_span();
                        self.push_diag(diag::unexpected_eof(span, "parsing choice values"));
                        return Err(ParseError);
                    }
                };
                // Accept either a text literal ("approve") or an ident (approve).
                let ident = match &t.kind {
                    TokenKind::Text(s) => {
                        let ident = Ident {
                            name: s.clone(),
                            span: t.span,
                        };
                        self.advance();
                        ident
                    }
                    TokenKind::Ident(n) => {
                        let ident = Ident {
                            name: n.clone(),
                            span: t.span,
                        };
                        self.advance();
                        ident
                    }
                    other => {
                        let span = t.span;
                        let got = describe(other);
                        self.push_diag(diag::expected(
                            span,
                            "a choice value (text literal or identifier)",
                            &got,
                        ));
                        return Err(ParseError);
                    }
                };
                values.push(ChoiceValue { name: ident });
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(&TokenKind::RBracket, "`]`")?;
        Ok(values)
    }
}

fn inner_span(t: &Type) -> covenant_diag::Span {
    match t {
        Type::Amount(s)
        | Type::Time(s)
        | Type::Duration(s)
        | Type::Hash(s)
        | Type::Text(s)
        | Type::Address(s)
        | Type::Choice(s, _)
        | Type::PqKey(s)
        | Type::Bytes(s)
        | Type::Bool(s) => *s,
        Type::Encrypted(_, s)
        | Type::List(_, s)
        | Type::Map(_, _, s)
        | Type::PriorityQueue(_, _, _, s)
        | Type::Shares { span: s, .. } => *s,
        Type::User(id) => id.span,
    }
}

/// Peek helper: would the next tokens start a type expression?
///
/// Used by field-vs-metadata disambiguation.
pub(crate) fn looks_like_type(kind: &TokenKind) -> bool {
    use TokenKind::*;
    matches!(
        kind,
        KwAmount
            | KwTime
            | KwDuration
            | KwHash
            | KwTextTy
            | KwAddress
            | KwChoice
            | KwPqKey
            | KwBytes
            | KwBool
            | KwCiphertext
            | KwEncrypted
            | KwMap
            | KwPriorityQueue
            | KwShares
            | LBracket  // [T]
            | Ident(_) // user type
    )
}

/// Include the `Expr` import so rustc doesn't warn about a "private but unused"
/// re-export when the module is compiled independently.
#[allow(dead_code)]
fn _expr_link(_e: Expr) {}

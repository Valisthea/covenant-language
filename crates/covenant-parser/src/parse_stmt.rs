//! Statement parsing.
//!
//! Covers `let`, assignments, expression statements, `match`, `if`/`else`,
//! `encrypted_when`/`otherwise`, `for each`, `try_action`/`catch`, `emit`,
//! `transfer`, `append`, `discard`, `delete`, `return`, `revert_with`.

use covenant_lexer::TokenKind;

use crate::ast::{
    AssignOp, Expr, FieldAssignment, Ident, LValue, MatchArm, MatchBody, MatchPattern, Stmt, Type,
};
use crate::diag;
use crate::parser::{ParseError, Parser};

impl<'a> Parser<'a> {
    /// Parse the top-level `{ stmt* }` body of an executable declaration
    /// (`action`, `migrate`, `on_destroy`). Resets the per-body size budget:
    /// a file of many small actions is cheap, one action of many thousands of
    /// statements is not, so the budget is charged per body and not per file.
    pub(crate) fn parse_function_body(&mut self) -> Result<Vec<Stmt>, ParseError> {
        self.body_stmts = 0;
        self.parse_block()
    }

    /// Parse a `{ stmt* }` block, consuming the braces.
    pub(crate) fn parse_block(&mut self) -> Result<Vec<Stmt>, ParseError> {
        self.enter_depth()?;
        // A block is itself a unit of work for later stages, so charge it too:
        // otherwise a `match` with thousands of empty arms slips past the
        // budget with a statement count of zero.
        let result = match self.charge_body_stmt() {
            Ok(()) => self.parse_block_body(),
            Err(e) => Err(e),
        };
        self.exit_depth();
        result
    }

    fn parse_block_body(&mut self) -> Result<Vec<Stmt>, ParseError> {
        self.expect(&TokenKind::LBrace, "`{`")?;
        self.skip_newlines();
        let mut out = Vec::new();
        while !self.at(&TokenKind::RBrace) && !self.at(&TokenKind::Eof) {
            match self.parse_stmt() {
                Ok(s) => out.push(s),
                Err(_) => {
                    // An over-budget body is not recoverable: continuing would
                    // re-charge every remaining statement and bury the real
                    // diagnostic under thousands of follow-on errors.
                    if self.body_budget_blown() {
                        return Err(ParseError);
                    }
                    // recover to next newline or `}` and continue
                    self.recover_to(&[TokenKind::Newline, TokenKind::RBrace]);
                    let _ = self.eat(&TokenKind::Newline);
                }
            }
            self.skip_newlines();
        }
        self.expect(&TokenKind::RBrace, "`}`")?;
        Ok(out)
    }

    pub(crate) fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.charge_body_stmt()?;
        self.skip_newlines();
        let t = match self.peek() {
            Some(t) => t,
            None => {
                let span = self.current_span();
                self.push_diag(diag::unexpected_eof(span, "parsing a statement"));
                return Err(ParseError);
            }
        };
        let start = t.span;
        match &t.kind {
            TokenKind::KwLet => self.parse_let(),
            TokenKind::KwMatch => self.parse_match_stmt(start),
            TokenKind::KwIf => self.parse_if_stmt(start),
            TokenKind::KwEncryptedWhen => self.parse_encrypted_when_stmt(start),
            TokenKind::KwFor => self.parse_for_each_stmt(start),
            TokenKind::KwTryAction => self.parse_try_catch_stmt(start),
            TokenKind::KwEmit => self.parse_emit_stmt(start),
            TokenKind::KwTransfer => self.parse_transfer_stmt(start),
            TokenKind::KwAppend => self.parse_append_stmt(start),
            TokenKind::KwDiscard => self.parse_discard_stmt(start),
            TokenKind::KwDelete => self.parse_delete_stmt(start),
            TokenKind::KwReturn => self.parse_return_stmt(start),
            TokenKind::KwRevertWith => self.parse_revert_with_stmt(start),
            _ => self.parse_expr_or_assign_stmt(start),
        }
    }

    fn parse_let(&mut self) -> Result<Stmt, ParseError> {
        let start = self.current_span();
        self.expect(&TokenKind::KwLet, "`let`")?;
        let name = self.expect_ident("identifier after `let`")?;
        let ty = if self.eat(&TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(&TokenKind::Eq, "`=`")?;
        let value = self.parse_expr()?;
        let span = start.join(value.span());
        self.expect_stmt_terminator("after `let` binding")?;
        Ok(Stmt::Let {
            name,
            ty,
            value,
            span,
        })
    }

    fn parse_match_stmt(&mut self, start: covenant_diag::Span) -> Result<Stmt, ParseError> {
        self.expect(&TokenKind::KwMatch, "`match`")?;
        let scrutinee = self.parse_expr()?;
        self.expect(&TokenKind::LBrace, "`{`")?;
        self.skip_newlines();
        let mut arms = Vec::new();
        while !self.at(&TokenKind::RBrace) && !self.at(&TokenKind::Eof) {
            let pat_expr = self.parse_expr()?;
            let pat_span = pat_expr.span();
            self.expect(&TokenKind::FatArrow, "`=>`")?;
            let body = if self.at(&TokenKind::LBrace) {
                MatchBody::Block(self.parse_block()?)
            } else {
                MatchBody::Stmt(Box::new(self.parse_stmt()?))
            };
            let body_span = match &body {
                MatchBody::Stmt(s) => stmt_span(s),
                MatchBody::Block(_) => pat_span, // best-effort
            };
            arms.push(MatchArm {
                pattern: MatchPattern::Literal(pat_expr),
                body,
                span: pat_span.join(body_span),
            });
            let _ = self.eat(&TokenKind::Comma);
            self.skip_newlines();
        }
        let rb = self.expect(&TokenKind::RBrace, "`}`")?;
        Ok(Stmt::Match {
            scrutinee,
            arms,
            span: start.join(rb.span),
        })
    }

    fn parse_if_stmt(&mut self, start: covenant_diag::Span) -> Result<Stmt, ParseError> {
        self.expect(&TokenKind::KwIf, "`if`")?;
        let cond = self.parse_expr()?;
        let then_block = self.parse_block()?;
        let mut else_ifs = Vec::new();
        let mut else_block = None;
        while self.eat(&TokenKind::KwElse) {
            if self.eat(&TokenKind::KwIf) {
                let c = self.parse_expr()?;
                let b = self.parse_block()?;
                else_ifs.push((c, b));
            } else {
                else_block = Some(self.parse_block()?);
                break;
            }
        }
        let span = start; // rightmost span tracking not critical here
        Ok(Stmt::If {
            cond,
            then_block,
            else_ifs,
            else_block,
            span,
        })
    }

    fn parse_encrypted_when_stmt(
        &mut self,
        start: covenant_diag::Span,
    ) -> Result<Stmt, ParseError> {
        self.expect(&TokenKind::KwEncryptedWhen, "`encrypted_when`")?;
        let cond = self.parse_expr()?;
        let then_block = self.parse_block()?;
        let otherwise = if self.eat(&TokenKind::KwOtherwise) {
            Some(self.parse_block()?)
        } else {
            None
        };
        Ok(Stmt::EncryptedWhen {
            cond,
            then_block,
            otherwise,
            span: start,
        })
    }

    fn parse_for_each_stmt(&mut self, start: covenant_diag::Span) -> Result<Stmt, ParseError> {
        self.expect(&TokenKind::KwFor, "`for`")?;
        self.expect(&TokenKind::KwEach, "`each`")?;
        let binding = self.expect_ident("loop variable name")?;
        self.expect(&TokenKind::KwIn, "`in`")?;
        let iter = self.parse_expr()?;
        let body = self.parse_block()?;
        Ok(Stmt::ForEach {
            binding,
            iter,
            body,
            span: start,
        })
    }

    fn parse_try_catch_stmt(&mut self, start: covenant_diag::Span) -> Result<Stmt, ParseError> {
        self.expect(&TokenKind::KwTryAction, "`try_action`")?;
        let body = self.parse_block()?;
        self.expect(&TokenKind::KwCatch, "`catch`")?;
        let error_binding = if self.eat(&TokenKind::Underscore) {
            None
        } else {
            Some(self.expect_ident("catch binding name or `_`")?)
        };
        let catch_body = self.parse_block()?;
        Ok(Stmt::TryCatch {
            body,
            error_binding,
            catch_body,
            span: start,
        })
    }

    fn parse_emit_stmt(&mut self, start: covenant_diag::Span) -> Result<Stmt, ParseError> {
        self.expect(&TokenKind::KwEmit, "`emit`")?;
        let event = self.expect_ident("event name after `emit`")?;
        self.expect(&TokenKind::LParen, "`(`")?;
        let args = self.parse_expr_list_until(&TokenKind::RParen)?;
        let rp = self.expect(&TokenKind::RParen, "`)`")?;
        let span = start.join(rp.span);
        self.expect_stmt_terminator("after `emit`")?;
        Ok(Stmt::Emit { event, args, span })
    }

    fn parse_transfer_stmt(&mut self, start: covenant_diag::Span) -> Result<Stmt, ParseError> {
        self.expect(&TokenKind::KwTransfer, "`transfer`")?;
        let amount = self.parse_expr()?;
        let from = if self.eat(&TokenKind::KwFrom) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect(&TokenKind::KwTo, "`to`")?;
        let to = self.parse_expr()?;
        let span = start.join(to.span());
        self.expect_stmt_terminator("after `transfer`")?;
        Ok(Stmt::Transfer {
            amount,
            from,
            to,
            span,
        })
    }

    fn parse_append_stmt(&mut self, start: covenant_diag::Span) -> Result<Stmt, ParseError> {
        self.expect(&TokenKind::KwAppend, "`append`")?;
        let collection = self.expect_ident("collection name after `append`")?;
        self.expect(&TokenKind::LBrace, "`{`")?;
        self.skip_newlines();
        let mut struct_lit = Vec::new();
        while !self.at(&TokenKind::RBrace) && !self.at(&TokenKind::Eof) {
            let name = self.expect_ident("field name in `append { ... }`")?;
            self.expect(&TokenKind::Colon, "`:`")?;
            let value = self.parse_expr()?;
            let span = name.span.join(value.span());
            struct_lit.push(FieldAssignment { name, value, span });
            let _ = self.eat(&TokenKind::Comma);
            self.skip_newlines();
        }
        let rb = self.expect(&TokenKind::RBrace, "`}`")?;
        let span = start.join(rb.span);
        self.expect_stmt_terminator("after `append { ... }`")?;
        Ok(Stmt::Append {
            collection,
            struct_lit,
            span,
        })
    }

    fn parse_discard_stmt(&mut self, start: covenant_diag::Span) -> Result<Stmt, ParseError> {
        self.expect(&TokenKind::KwDiscard, "`discard`")?;
        let lv = self.parse_lvalue()?;
        let end = lvalue_span(&lv);
        self.expect_stmt_terminator("after `discard`")?;
        Ok(Stmt::Discard(lv, start.join(end)))
    }

    fn parse_delete_stmt(&mut self, start: covenant_diag::Span) -> Result<Stmt, ParseError> {
        self.expect(&TokenKind::KwDelete, "`delete`")?;
        let lv = self.parse_lvalue()?;
        let end = lvalue_span(&lv);
        self.expect_stmt_terminator("after `delete`")?;
        Ok(Stmt::Delete(lv, start.join(end)))
    }

    fn parse_return_stmt(&mut self, start: covenant_diag::Span) -> Result<Stmt, ParseError> {
        self.expect(&TokenKind::KwReturn, "`return`")?;
        let value = match self.peek_kind() {
            Some(TokenKind::Newline)
            | Some(TokenKind::Semicolon)
            | Some(TokenKind::RBrace)
            | Some(TokenKind::Eof)
            | None => None,
            _ => Some(self.parse_expr()?),
        };
        let span = value
            .as_ref()
            .map(|e| start.join(e.span()))
            .unwrap_or(start);
        self.expect_stmt_terminator("after `return`")?;
        Ok(Stmt::Return(value, span))
    }

    fn parse_revert_with_stmt(&mut self, start: covenant_diag::Span) -> Result<Stmt, ParseError> {
        self.expect(&TokenKind::KwRevertWith, "`revert_with`")?;
        let error = self.expect_ident("error name after `revert_with`")?;
        let mut args = Vec::new();
        let span_end;
        if self.eat(&TokenKind::LParen) {
            args = self.parse_expr_list_until(&TokenKind::RParen)?;
            let rp = self.expect(&TokenKind::RParen, "`)`")?;
            span_end = rp.span;
        } else {
            span_end = error.span;
        }
        self.expect_stmt_terminator("after `revert_with`")?;
        Ok(Stmt::RevertWith {
            error,
            args,
            span: start.join(span_end),
        })
    }

    /// Parse either an expression statement or an assignment. Distinguished by
    /// peeking at the token that follows a valid l-value expression.
    fn parse_expr_or_assign_stmt(
        &mut self,
        start: covenant_diag::Span,
    ) -> Result<Stmt, ParseError> {
        let expr = self.parse_expr()?;

        // Is the next token an assignment operator? If so, the expression is
        // the l-value side.
        let op = match self.peek_kind() {
            Some(TokenKind::Eq) => Some(AssignOp::Eq),
            Some(TokenKind::PlusEq) => Some(AssignOp::PlusEq),
            Some(TokenKind::MinusEq) => Some(AssignOp::MinusEq),
            Some(TokenKind::StarEq) => Some(AssignOp::StarEq),
            Some(TokenKind::SlashEq) => Some(AssignOp::SlashEq),
            Some(TokenKind::PercentEq) => Some(AssignOp::PercentEq),
            _ => None,
        };
        if let Some(op) = op {
            self.advance();
            let target = expr_to_lvalue(&expr).ok_or_else(|| {
                self.push_diag(diag::unexpected_token(
                    expr.span(),
                    "left-hand side of assignment is not an l-value".to_string(),
                ));
                ParseError
            })?;
            let value = self.parse_expr()?;
            let span = start.join(value.span());
            self.expect_stmt_terminator("after assignment")?;
            return Ok(Stmt::Assign {
                target,
                op,
                value,
                span,
            });
        }
        let span = expr.span();
        self.expect_stmt_terminator("after expression statement")?;
        Ok(Stmt::Expr(expr, span))
    }

    pub(crate) fn parse_expr_list_until(
        &mut self,
        end: &TokenKind,
    ) -> Result<Vec<Expr>, ParseError> {
        let mut out = Vec::new();
        if self.at(end) {
            return Ok(out);
        }
        loop {
            out.push(self.parse_expr()?);
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        Ok(out)
    }

    fn parse_lvalue(&mut self) -> Result<LValue, ParseError> {
        // Same shape as the Pratt loop: an iterative spine that has to be
        // charged, or `discard a.f.f.f...` builds an unbounded `LValue` chain
        // that later stages walk recursively.
        let base = self.nest_depth;
        let result = self.parse_lvalue_body();
        self.nest_depth = base;
        result
    }

    fn parse_lvalue_body(&mut self) -> Result<LValue, ParseError> {
        let id = self.expect_ident("identifier for l-value")?;
        let mut lv = LValue::Ident(id);
        loop {
            match self.peek_kind() {
                Some(TokenKind::Dot) => {
                    self.enter_chain_depth()?;
                    self.advance();
                    let f = self.expect_ident("field name after `.`")?;
                    lv = LValue::FieldAccess(Box::new(lv), f);
                }
                Some(TokenKind::LBracket) => {
                    self.enter_chain_depth()?;
                    self.advance();
                    let e = self.parse_expr()?;
                    self.expect(&TokenKind::RBracket, "`]`")?;
                    lv = LValue::Index(Box::new(lv), e);
                }
                _ => break,
            }
        }
        Ok(lv)
    }
}

/// Convert an expression into an l-value if possible (used when we've parsed
/// a full expression and then discover an `=` token afterward).
fn expr_to_lvalue(e: &Expr) -> Option<LValue> {
    match e {
        Expr::Ident(i) => Some(LValue::Ident(i.clone())),
        Expr::FieldAccess { base, field, .. } => {
            let base_lv = expr_to_lvalue(base)?;
            Some(LValue::FieldAccess(Box::new(base_lv), field.clone()))
        }
        Expr::Index { base, index, .. } => {
            let base_lv = expr_to_lvalue(base)?;
            Some(LValue::Index(Box::new(base_lv), (**index).clone()))
        }
        Expr::Paren(inner) => expr_to_lvalue(inner),
        _ => None,
    }
}

fn lvalue_span(lv: &LValue) -> covenant_diag::Span {
    match lv {
        LValue::Ident(i) => i.span,
        LValue::FieldAccess(_, i) => i.span,
        LValue::Index(base, _) => lvalue_span(base),
    }
}

fn stmt_span(s: &Stmt) -> covenant_diag::Span {
    match s {
        Stmt::Let { span, .. }
        | Stmt::Assign { span, .. }
        | Stmt::Match { span, .. }
        | Stmt::If { span, .. }
        | Stmt::EncryptedWhen { span, .. }
        | Stmt::ForEach { span, .. }
        | Stmt::TryCatch { span, .. }
        | Stmt::Emit { span, .. }
        | Stmt::Transfer { span, .. }
        | Stmt::Append { span, .. }
        | Stmt::RevertWith { span, .. } => *span,
        Stmt::Discard(_, s) | Stmt::Delete(_, s) => *s,
        Stmt::Return(_, s) => *s,
        Stmt::Expr(_, s) => *s,
    }
}

/// Silence unused-import warnings.
#[allow(dead_code)]
fn _type_link(_t: Type) {}
#[allow(dead_code)]
fn _ident_link(_i: Ident) {}

//! Expression parsing using Pratt (operator-precedence) style.
//!
//! Precedence follows Doc 2 §1.8 and the Phase 2 spec's binding-power table.
//! Higher binding power = tighter binding. The right-binding-power of each
//! infix operator determines associativity: if `r > l`, the operator is
//! left-associative; `r < l` right-associative.

use covenant_diag::Span;
use covenant_lexer::TokenKind;

use crate::ast::{BinaryOp, Expr, Ident, LiteralExpr, MatchExprArm, MatchPattern, UnaryOp};
use crate::diag;
use crate::parser::{describe, keyword_as_ident_text, ParseError, Parser};

const BP_UNARY: u8 = 23;

impl<'a> Parser<'a> {
    /// Public entry: parse an expression at the lowest binding power.
    pub(crate) fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_expr_bp(0)
    }

    fn parse_expr_bp(&mut self, min_bp: u8) -> Result<Expr, ParseError> {
        // Save and restore rather than pair a single `exit_depth`: the body
        // charges one level per node it appends to the iterative left spine
        // (`a + b + c`, `v.f.f.f`, `m[k][k]`) and those charges are only known
        // once it returns, including on the error path.
        let base = self.nest_depth;
        self.enter_depth()?;
        let result = self.parse_expr_bp_body(min_bp);
        self.nest_depth = base;
        result
    }

    fn parse_expr_bp_body(&mut self, min_bp: u8) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_prefix()?;

        loop {
            // Try lambda at the very top: `<ident> => expr`. We only rewrite a
            // bare `Ident` lhs here because lambdas in Covenant only appear in
            // single-parameter contexts (e.g. `.map(x => x.kind)`).
            if min_bp == 0 {
                if let Expr::Ident(ident) = &lhs {
                    if matches!(self.peek_kind(), Some(TokenKind::FatArrow)) {
                        let id = ident.clone();
                        self.enter_chain_depth()?;
                        self.advance();
                        let body = self.parse_expr_bp(0)?;
                        let body_span = body.span();
                        let span = id.span.join(body_span);
                        lhs = Expr::Lambda {
                            param: id,
                            body: Box::new(body),
                            span,
                        };
                        continue;
                    }
                }
            }

            let Some(op_info) = self.peek_infix_or_postfix() else {
                break;
            };
            match op_info {
                InfixOrPostfix::Binary(op, (l_bp, r_bp)) => {
                    if l_bp < min_bp {
                        break;
                    }
                    // Every iteration here deepens the tree by one level even
                    // though the parser itself does not recurse, so the node
                    // has to be charged explicitly or the depth guard never
                    // sees a chain of any length.
                    self.enter_chain_depth()?;
                    self.advance();
                    let rhs = self.parse_expr_bp(r_bp)?;
                    let span = lhs.span().join(rhs.span());
                    lhs = Expr::Binary {
                        op,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                        span,
                    };
                }
                InfixOrPostfix::Call => {
                    if BP_POSTFIX < min_bp {
                        break;
                    }
                    self.enter_chain_depth()?;
                    lhs = self.parse_call_tail(lhs)?;
                }
                InfixOrPostfix::Field => {
                    if BP_POSTFIX < min_bp {
                        break;
                    }
                    self.enter_chain_depth()?;
                    lhs = self.parse_field_tail(lhs)?;
                }
                InfixOrPostfix::Index => {
                    if BP_POSTFIX < min_bp {
                        break;
                    }
                    self.enter_chain_depth()?;
                    lhs = self.parse_index_tail(lhs)?;
                }
            }
        }

        Ok(lhs)
    }

    fn parse_prefix(&mut self) -> Result<Expr, ParseError> {
        let t = match self.peek() {
            Some(t) => t,
            None => {
                let span = self.current_span();
                self.push_diag(diag::unexpected_eof(span, "parsing an expression"));
                return Err(ParseError);
            }
        };
        match &t.kind {
            // Unary prefix operators.
            TokenKind::Bang => {
                let start = t.span;
                self.advance();
                let operand = self.parse_expr_bp(BP_UNARY)?;
                let span = start.join(operand.span());
                Ok(Expr::Unary {
                    op: UnaryOp::Not,
                    operand: Box::new(operand),
                    span,
                })
            }
            TokenKind::Minus => {
                let start = t.span;
                self.advance();
                let operand = self.parse_expr_bp(BP_UNARY)?;
                let span = start.join(operand.span());
                Ok(Expr::Unary {
                    op: UnaryOp::Neg,
                    operand: Box::new(operand),
                    span,
                })
            }
            TokenKind::Tilde => {
                let start = t.span;
                self.advance();
                let operand = self.parse_expr_bp(BP_UNARY)?;
                let span = start.join(operand.span());
                Ok(Expr::Unary {
                    op: UnaryOp::BitNot,
                    operand: Box::new(operand),
                    span,
                })
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        let t = match self.peek() {
            Some(t) => t,
            None => {
                let span = self.current_span();
                self.push_diag(diag::unexpected_eof(span, "parsing an expression"));
                return Err(ParseError);
            }
        };
        let start = t.span;
        match &t.kind {
            TokenKind::Integer(v) => {
                let out = Expr::Literal(LiteralExpr::Integer(*v, start));
                self.advance();
                Ok(out)
            }
            TokenKind::HexBytes(b) => {
                let out = Expr::Literal(LiteralExpr::Hex(b.clone(), start));
                self.advance();
                Ok(out)
            }
            TokenKind::Text(s) => {
                let out = Expr::Literal(LiteralExpr::Text(s.clone(), start));
                self.advance();
                Ok(out)
            }
            TokenKind::True => {
                self.advance();
                Ok(Expr::Literal(LiteralExpr::Bool(true, start)))
            }
            TokenKind::False => {
                self.advance();
                Ok(Expr::Literal(LiteralExpr::Bool(false, start)))
            }
            TokenKind::Duration(n, unit) => {
                let n = *n;
                let unit = *unit;
                self.advance();
                Ok(Expr::Literal(LiteralExpr::Duration(n, unit, start)))
            }
            TokenKind::Ident(name) => {
                let ident = Ident {
                    name: name.clone(),
                    span: start,
                };
                self.advance();
                Ok(Expr::Ident(ident))
            }
            TokenKind::LParen => {
                self.advance();
                let inner = self.parse_expr()?;
                self.expect(&TokenKind::RParen, "`)`")?;
                Ok(Expr::Paren(Box::new(inner)))
            }
            TokenKind::LBracket => {
                self.advance();
                let mut elements = Vec::new();
                if !self.at(&TokenKind::RBracket) {
                    loop {
                        elements.push(self.parse_expr()?);
                        if !self.eat(&TokenKind::Comma) {
                            break;
                        }
                    }
                }
                let rb = self.expect(&TokenKind::RBracket, "`]`")?;
                Ok(Expr::Array {
                    elements,
                    span: start.join(rb.span),
                })
            }
            TokenKind::KwEncrypted => {
                self.advance();
                self.expect(&TokenKind::LParen, "`(`")?;
                let inner = self.parse_expr()?;
                let rp = self.expect(&TokenKind::RParen, "`)`")?;
                Ok(Expr::EncryptedLit(Box::new(inner), start.join(rp.span)))
            }
            TokenKind::KwIf => self.parse_if_expr(),
            TokenKind::KwMatch => self.parse_match_expr(),
            other => {
                // Type keywords can appear as identifier references in
                // expressions (e.g. `opened_at + duration`). See spec-ambiguity
                // note in the Phase 2 report.
                if let Some(text) = keyword_as_ident_text(other) {
                    let ident = Ident {
                        name: text.into(),
                        span: start,
                    };
                    self.advance();
                    return Ok(Expr::Ident(ident));
                }
                let span = start;
                let desc = describe(other);
                self.push_diag(diag::expected(span, "an expression", &desc));
                Err(ParseError)
            }
        }
    }

    fn parse_if_expr(&mut self) -> Result<Expr, ParseError> {
        let start = self.current_span();
        self.expect(&TokenKind::KwIf, "`if`")?;
        let cond = self.parse_expr()?;
        self.expect(&TokenKind::LBrace, "`{`")?;
        let then_expr = self.parse_expr()?;
        self.expect(&TokenKind::RBrace, "`}`")?;
        self.expect(&TokenKind::KwElse, "`else`")?;
        self.expect(&TokenKind::LBrace, "`{`")?;
        let else_expr = self.parse_expr()?;
        let rb = self.expect(&TokenKind::RBrace, "`}`")?;
        Ok(Expr::If {
            cond: Box::new(cond),
            then_expr: Box::new(then_expr),
            else_expr: Box::new(else_expr),
            span: start.join(rb.span),
        })
    }

    fn parse_match_expr(&mut self) -> Result<Expr, ParseError> {
        let start = self.current_span();
        self.expect(&TokenKind::KwMatch, "`match`")?;
        let scrutinee = self.parse_expr()?;
        self.expect(&TokenKind::LBrace, "`{`")?;
        self.skip_newlines();
        let mut arms = Vec::new();
        while !self.at(&TokenKind::RBrace) {
            let pat_expr = self.parse_expr()?;
            let pat_span = pat_expr.span();
            let pattern = MatchPattern::Literal(pat_expr);
            self.expect(&TokenKind::FatArrow, "`=>`")?;
            let body = self.parse_expr()?;
            let body_span = body.span();
            arms.push(MatchExprArm {
                pattern,
                body: Box::new(body),
                span: pat_span.join(body_span),
            });
            let _ = self.eat(&TokenKind::Comma);
            self.skip_newlines();
        }
        let rb = self.expect(&TokenKind::RBrace, "`}`")?;
        Ok(Expr::Match {
            scrutinee: Box::new(scrutinee),
            arms,
            span: start.join(rb.span),
        })
    }

    fn parse_call_tail(&mut self, callee: Expr) -> Result<Expr, ParseError> {
        let start = callee.span();
        self.expect(&TokenKind::LParen, "`(`")?;
        let mut args = Vec::new();
        if !self.at(&TokenKind::RParen) {
            loop {
                args.push(self.parse_expr()?);
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
        }
        let rp = self.expect(&TokenKind::RParen, "`)`")?;
        Ok(Expr::Call {
            callee: Box::new(callee),
            args,
            span: start.join(rp.span),
        })
    }

    fn parse_field_tail(&mut self, base: Expr) -> Result<Expr, ParseError> {
        let start = base.span();
        self.expect(&TokenKind::Dot, "`.`")?;
        let field = self.expect_ident_or_any_keyword("a field name after `.`")?;
        let span = start.join(field.span);
        Ok(Expr::FieldAccess {
            base: Box::new(base),
            field,
            span,
        })
    }

    fn parse_index_tail(&mut self, base: Expr) -> Result<Expr, ParseError> {
        let start = base.span();
        self.expect(&TokenKind::LBracket, "`[`")?;
        let from = self.parse_expr()?;
        if self.eat(&TokenKind::DotDot) {
            let to = self.parse_expr()?;
            let rb = self.expect(&TokenKind::RBracket, "`]`")?;
            return Ok(Expr::Slice {
                base: Box::new(base),
                from: Box::new(from),
                to: Box::new(to),
                span: start.join(rb.span),
            });
        }
        let rb = self.expect(&TokenKind::RBracket, "`]`")?;
        Ok(Expr::Index {
            base: Box::new(base),
            index: Box::new(from),
            span: start.join(rb.span),
        })
    }

    fn peek_infix_or_postfix(&self) -> Option<InfixOrPostfix> {
        let k = self.peek_kind()?;
        match k {
            TokenKind::Plus => Some(InfixOrPostfix::Binary(BinaryOp::Add, bp(BinaryOp::Add))),
            TokenKind::Minus => Some(InfixOrPostfix::Binary(BinaryOp::Sub, bp(BinaryOp::Sub))),
            TokenKind::Star => Some(InfixOrPostfix::Binary(BinaryOp::Mul, bp(BinaryOp::Mul))),
            TokenKind::Slash => Some(InfixOrPostfix::Binary(BinaryOp::Div, bp(BinaryOp::Div))),
            TokenKind::Percent => Some(InfixOrPostfix::Binary(BinaryOp::Mod, bp(BinaryOp::Mod))),
            TokenKind::PlusPlus => Some(InfixOrPostfix::Binary(
                BinaryOp::Concat,
                bp(BinaryOp::Concat),
            )),
            TokenKind::EqEq => Some(InfixOrPostfix::Binary(BinaryOp::Eq, bp(BinaryOp::Eq))),
            TokenKind::NotEq => Some(InfixOrPostfix::Binary(BinaryOp::NotEq, bp(BinaryOp::NotEq))),
            TokenKind::Lt => Some(InfixOrPostfix::Binary(BinaryOp::Lt, bp(BinaryOp::Lt))),
            TokenKind::LtEq => Some(InfixOrPostfix::Binary(BinaryOp::LtEq, bp(BinaryOp::LtEq))),
            TokenKind::Gt => Some(InfixOrPostfix::Binary(BinaryOp::Gt, bp(BinaryOp::Gt))),
            TokenKind::GtEq => Some(InfixOrPostfix::Binary(BinaryOp::GtEq, bp(BinaryOp::GtEq))),
            TokenKind::AmpAmp => Some(InfixOrPostfix::Binary(BinaryOp::And, bp(BinaryOp::And))),
            TokenKind::PipePipe => Some(InfixOrPostfix::Binary(BinaryOp::Or, bp(BinaryOp::Or))),
            TokenKind::Amp => Some(InfixOrPostfix::Binary(
                BinaryOp::BitAnd,
                bp(BinaryOp::BitAnd),
            )),
            TokenKind::Pipe => Some(InfixOrPostfix::Binary(BinaryOp::BitOr, bp(BinaryOp::BitOr))),
            TokenKind::Caret => Some(InfixOrPostfix::Binary(
                BinaryOp::BitXor,
                bp(BinaryOp::BitXor),
            )),
            TokenKind::ShiftLeft => Some(InfixOrPostfix::Binary(BinaryOp::Shl, bp(BinaryOp::Shl))),
            TokenKind::ShiftRight => Some(InfixOrPostfix::Binary(BinaryOp::Shr, bp(BinaryOp::Shr))),
            TokenKind::KwIn => Some(InfixOrPostfix::Binary(BinaryOp::In, bp(BinaryOp::In))),

            TokenKind::LParen => Some(InfixOrPostfix::Call),
            TokenKind::Dot => Some(InfixOrPostfix::Field),
            TokenKind::LBracket => Some(InfixOrPostfix::Index),

            _ => None,
        }
    }
}

enum InfixOrPostfix {
    Binary(BinaryOp, (u8, u8)),
    Call,
    Field,
    Index,
}

const BP_POSTFIX: u8 = 24;

/// Left/right binding power per binary op.
fn bp(op: BinaryOp) -> (u8, u8) {
    use BinaryOp::*;
    match op {
        Or => (3, 4),
        And => (5, 6),
        Eq | NotEq => (7, 8),
        Lt | LtEq | Gt | GtEq | In => (9, 10),
        BitOr => (11, 12),
        BitXor => (13, 14),
        BitAnd => (15, 16),
        Shl | Shr => (17, 18),
        Add | Sub | Concat => (19, 20),
        Mul | Div | Mod => (21, 22),
    }
}

/// Silence an unused-import warning when `Span` is only referenced in doc.
#[allow(dead_code)]
fn _span_link(_s: Span) {}

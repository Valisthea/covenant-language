//! Declaration parsing.
//!
//! Handles everything that appears inside a top-level construct body:
//! metadata assignments, fields (implicit or explicit `field`), structs,
//! credentials, errors, events, actions (+ annotations, guards, qualifiers),
//! views, reveals (both forms), version/migrate/on_destroy blocks, and
//! selective_disclosure declarations.

use covenant_diag::Span;
use covenant_lexer::TokenKind;

use crate::ast::{
    ActionDecl, ActionQualifier, Annotation, AnnotationArg, Arg, CredentialDecl, ErrorDecl,
    ErrorField, EventArg, EventDecl, Expr, FieldDecl, Guard, Ident, MetadataAssignment,
    MetadataValue, MigrateBlock, OnDestroyBlock, PrivacyQualifier, RevealDecl, RevealTarget,
    SelectiveDisclosureDecl, StructDecl, TopLevelDecl, Type, VersionBlock, ViewDecl,
};
use crate::diag;
use crate::parse_ty::looks_like_type;
use crate::parser::{describe, keyword_as_ident_text, ParseError, Parser};

impl<'a> Parser<'a> {
    /// Parse the body of a top-level construct: `{` ... `}` with a sequence of
    /// `TopLevelDecl`s, each terminated by a newline or found at its natural
    /// boundary.
    pub(crate) fn parse_top_level_body(&mut self) -> Result<Vec<TopLevelDecl>, ParseError> {
        self.expect(&TokenKind::LBrace, "`{`")?;
        self.skip_newlines();
        let mut out = Vec::new();
        while !self.at(&TokenKind::RBrace) && !self.at(&TokenKind::Eof) {
            match self.parse_top_level_decl() {
                Ok(d) => out.push(d),
                Err(_) => {
                    // Recover to the next newline or end of construct.
                    self.recover_to(&[TokenKind::Newline, TokenKind::RBrace]);
                    let _ = self.eat(&TokenKind::Newline);
                }
            }
            self.skip_newlines();
        }
        self.expect(&TokenKind::RBrace, "`}`")?;
        Ok(out)
    }

    fn parse_top_level_decl(&mut self) -> Result<TopLevelDecl, ParseError> {
        // Collect any leading annotations (they must attach to an action or
        // selective_disclosure).
        let mut annotations = Vec::new();
        while self.at(&TokenKind::At) {
            annotations.push(self.parse_annotation()?);
            self.skip_newlines();
        }

        let t = match self.peek() {
            Some(t) => t,
            None => {
                let span = self.current_span();
                self.push_diag(diag::unexpected_eof(span, "parsing a declaration"));
                return Err(ParseError);
            }
        };
        let start = t.span;

        match &t.kind {
            TokenKind::KwAction => {
                let decl = self.parse_action(start, annotations)?;
                Ok(TopLevelDecl::Action(decl))
            }
            TokenKind::KwView => {
                if !annotations.is_empty() {
                    self.push_diag(diag::annotation_wrong_decl(annotations[0].span));
                }
                let decl = self.parse_view(start)?;
                Ok(TopLevelDecl::View(decl))
            }
            TokenKind::KwReveal => {
                if !annotations.is_empty() {
                    self.push_diag(diag::annotation_wrong_decl(annotations[0].span));
                }
                let decl = self.parse_reveal(start)?;
                Ok(TopLevelDecl::Reveal(decl))
            }
            TokenKind::KwField => {
                // Annotations on fields are allowed (KSR-CVN-021): `@slot(N)`.
                let decl = self.parse_field_with_keyword(start, annotations)?;
                Ok(TopLevelDecl::Field(decl))
            }
            TokenKind::KwStruct => {
                if !annotations.is_empty() {
                    self.push_diag(diag::annotation_wrong_decl(annotations[0].span));
                }
                let decl = self.parse_struct(start)?;
                Ok(TopLevelDecl::Struct(decl))
            }
            TokenKind::KwCredential => {
                if !annotations.is_empty() {
                    self.push_diag(diag::annotation_wrong_decl(annotations[0].span));
                }
                let decl = self.parse_credential(start)?;
                Ok(TopLevelDecl::Credential(decl))
            }
            TokenKind::KwError => {
                if !annotations.is_empty() {
                    self.push_diag(diag::annotation_wrong_decl(annotations[0].span));
                }
                let decl = self.parse_error_decl(start)?;
                Ok(TopLevelDecl::Error(decl))
            }
            TokenKind::KwEvent => {
                if !annotations.is_empty() {
                    self.push_diag(diag::annotation_wrong_decl(annotations[0].span));
                }
                let decl = self.parse_event_decl(start)?;
                Ok(TopLevelDecl::Event(decl))
            }
            TokenKind::KwVersion => {
                let decl = self.parse_version_block(start)?;
                Ok(TopLevelDecl::Version(decl))
            }
            TokenKind::KwMigrate => {
                let decl = self.parse_migrate_block(start)?;
                Ok(TopLevelDecl::Migrate(decl))
            }
            TokenKind::KwOnDestroy => {
                let decl = self.parse_on_destroy(start)?;
                Ok(TopLevelDecl::OnDestroy(decl))
            }
            TokenKind::KwSelectiveDisclosure => {
                let decl = self.parse_selective_disclosure(start, annotations)?;
                Ok(TopLevelDecl::SelectiveDisclosure(decl))
            }
            TokenKind::KwPublic | TokenKind::KwEncrypted => {
                // Per-field privacy override (inside `hybrid` constructs), e.g.
                // `encrypted count: amount`. Annotations forward to the field.
                let decl = self.parse_field_with_privacy(start, annotations)?;
                Ok(TopLevelDecl::Field(decl))
            }
            TokenKind::Ident(_) => self.parse_ident_starting_decl(start, annotations),
            other => {
                // Type keywords (e.g. `duration`) can appear in field-name
                // position; treat as implicit-field if followed by `:`.
                if keyword_as_ident_text(other).is_some()
                    && matches!(self.peek_ahead(1).map(|t| &t.kind), Some(TokenKind::Colon))
                {
                    return self.parse_ident_starting_decl(start, annotations);
                }
                if !annotations.is_empty() {
                    self.push_diag(diag::annotation_wrong_decl(annotations[0].span));
                }
                let span = start;
                let desc = describe(other);
                self.push_diag(diag::unexpected_token(span, desc));
                Err(ParseError)
            }
        }
    }

    /// Handle top-level declarations that begin with an identifier. The three
    /// shapes we can see here are:
    ///   1. `<ident> : <type>` → implicit-`field` declaration.
    ///   2. `<ident> : <value>` (not a type-leading token) → metadata assignment.
    ///   3. `<ident> { ... }` → anonymous struct-like block (used by `board`
    ///      constructs' `post { ... }` schema declaration).
    fn parse_ident_starting_decl(
        &mut self,
        start: Span,
        annotations: Vec<Annotation>,
    ) -> Result<TopLevelDecl, ParseError> {
        // Peek at the token after the identifier to disambiguate.
        let after_ident = self.peek_ahead(1).map(|t| &t.kind);
        match after_ident {
            Some(TokenKind::Colon) => {
                // Decide field vs metadata based on what follows the colon.
                let ahead2 = self.peek_ahead(2).map(|t| &t.kind);
                let looks_type = ahead2.is_some_and(looks_like_type);
                if looks_type {
                    let decl = self.parse_implicit_field(start, None, annotations)?;
                    Ok(TopLevelDecl::Field(decl))
                } else {
                    if !annotations.is_empty() {
                        self.push_diag(diag::annotation_wrong_decl(annotations[0].span));
                    }
                    let decl = self.parse_metadata(start)?;
                    Ok(TopLevelDecl::Metadata(decl))
                }
            }
            Some(TokenKind::LBrace) => {
                if !annotations.is_empty() {
                    self.push_diag(diag::annotation_wrong_decl(annotations[0].span));
                }
                // Anonymous struct-like block: `post { ... }`. We parse it as a
                // struct whose name is the identifier.
                let name = self.expect_ident("identifier")?;
                let fields = self.parse_struct_body()?;
                let span = start.join(self.current_span());
                Ok(TopLevelDecl::Struct(StructDecl { name, fields, span }))
            }
            _ => {
                if !annotations.is_empty() {
                    self.push_diag(diag::annotation_wrong_decl(annotations[0].span));
                }
                let span = self.current_span();
                let got = self
                    .peek()
                    .map(|t| describe(&t.kind))
                    .unwrap_or_else(|| "end of file".into());
                self.push_diag(diag::expected(span, "`:` or `{` after identifier", &got));
                Err(ParseError)
            }
        }
    }

    /// Only `@slot(...)` is accepted on field declarations (KSR-CVN-021).
    /// Any other annotation emits E021 (wrong-decl) so legacy tests like
    /// `@precompute(...)` on a field continue to error out, while preserving
    /// the new slot-pinning path.
    fn filter_field_annotations(&mut self, annotations: Vec<Annotation>) -> Vec<Annotation> {
        annotations
            .into_iter()
            .filter(|a| {
                if a.name.name.as_ref() == "slot" {
                    true
                } else {
                    self.push_diag(diag::annotation_wrong_decl(a.span));
                    false
                }
            })
            .collect()
    }

    fn parse_field_with_keyword(
        &mut self,
        start: Span,
        annotations: Vec<Annotation>,
    ) -> Result<FieldDecl, ParseError> {
        self.expect(&TokenKind::KwField, "`field`")?;
        // Optional per-field privacy override.
        let per_field_privacy = match self.peek_kind() {
            Some(TokenKind::KwPublic) => {
                self.advance();
                Some(PrivacyQualifier::Public)
            }
            Some(TokenKind::KwEncrypted) => {
                self.advance();
                Some(PrivacyQualifier::Encrypted)
            }
            _ => None,
        };
        self.finish_field(start, true, per_field_privacy, annotations)
    }

    fn parse_field_with_privacy(
        &mut self,
        start: Span,
        annotations: Vec<Annotation>,
    ) -> Result<FieldDecl, ParseError> {
        let pf = match self.peek_kind() {
            Some(TokenKind::KwPublic) => {
                self.advance();
                PrivacyQualifier::Public
            }
            Some(TokenKind::KwEncrypted) => {
                self.advance();
                PrivacyQualifier::Encrypted
            }
            _ => unreachable!("caller verified"),
        };
        // Either `<ident>: <type>` (implicit field) or explicit `field <ident>...`.
        if self.eat(&TokenKind::KwField) {
            self.finish_field(start, true, Some(pf), annotations)
        } else {
            self.finish_field(start, false, Some(pf), annotations)
        }
    }

    fn parse_implicit_field(
        &mut self,
        start: Span,
        per_field_privacy: Option<PrivacyQualifier>,
        annotations: Vec<Annotation>,
    ) -> Result<FieldDecl, ParseError> {
        self.finish_field(start, false, per_field_privacy, annotations)
    }

    fn finish_field(
        &mut self,
        start: Span,
        explicit_field_keyword: bool,
        per_field_privacy: Option<PrivacyQualifier>,
        annotations: Vec<Annotation>,
    ) -> Result<FieldDecl, ParseError> {
        let annotations = self.filter_field_annotations(annotations);
        let name = self.expect_ident("field name")?;
        self.expect(&TokenKind::Colon, "`:`")?;
        let ty = self.parse_type().inspect_err(|_e| {
            self.push_diag(diag::malformed_field(name.span, "could not parse type"));
        })?;
        // Optional initializer.
        let initializer = if self.eat(&TokenKind::Eq) {
            // Special case: choice type + inline values `= [...]`.
            if matches!(ty, Type::Choice(_, _)) && self.at(&TokenKind::LBracket) {
                // Peek further: an array of expressions looks the same as an
                // inline choice value list at this point. Parse as choice
                // values and rewrite the type; fall back to parsing as an expr
                // if that fails.
                let values = self.parse_inline_choice_values()?;
                // Rewrite the Type::Choice to carry the values we just parsed.
                let span_end = self.current_span();
                let span = start.join(span_end);
                self.expect_stmt_terminator("after field initializer")?;
                return Ok(FieldDecl {
                    span,
                    explicit_field_keyword,
                    per_field_privacy,
                    name,
                    ty: Type::Choice(start, values),
                    initializer: None,
                    annotations,
                });
            }
            Some(self.parse_expr()?)
        } else {
            None
        };
        let span_end = initializer
            .as_ref()
            .map(|e| e.span())
            .unwrap_or_else(|| self.current_span());
        let span = start.join(span_end);
        self.expect_stmt_terminator("after field declaration")?;
        Ok(FieldDecl {
            span,
            explicit_field_keyword,
            per_field_privacy,
            name,
            ty,
            initializer,
            annotations,
        })
    }

    fn parse_metadata(&mut self, start: Span) -> Result<MetadataAssignment, ParseError> {
        let name = self.expect_ident("metadata field name")?;
        self.expect(&TokenKind::Colon, "`:`")?;

        // Genesis-mint special case: `<integer> to <principal>`.
        if let Some(TokenKind::Integer(_)) = self.peek_kind() {
            let after = self.peek_ahead(1).map(|t| &t.kind);
            if matches!(after, Some(TokenKind::KwTo)) {
                let amount = self.parse_expr()?;
                self.expect(&TokenKind::KwTo, "`to`")?;
                let to = self.parse_principal()?;
                let span = start.join(self.current_span());
                self.expect_stmt_terminator("after metadata assignment")?;
                return Ok(MetadataAssignment {
                    name,
                    value: MetadataValue::GenesisMint { amount, to },
                    span,
                });
            }
        }

        let value = self.parse_expr()?;
        let span = start.join(value.span());
        self.expect_stmt_terminator("after metadata assignment")?;
        Ok(MetadataAssignment {
            name,
            value: MetadataValue::Literal(value),
            span,
        })
    }

    fn parse_struct(&mut self, start: Span) -> Result<StructDecl, ParseError> {
        self.expect(&TokenKind::KwStruct, "`struct`")?;
        let name = self.expect_ident("struct name")?;
        let fields = self.parse_struct_body()?;
        let span = start.join(self.current_span());
        Ok(StructDecl { name, fields, span })
    }

    fn parse_credential(&mut self, start: Span) -> Result<CredentialDecl, ParseError> {
        self.expect(&TokenKind::KwCredential, "`credential`")?;
        let name = self.expect_ident("credential name")?;
        let fields = self.parse_struct_body()?;
        let span = start.join(self.current_span());
        Ok(CredentialDecl { name, fields, span })
    }

    fn parse_struct_body(&mut self) -> Result<Vec<FieldDecl>, ParseError> {
        self.expect(&TokenKind::LBrace, "`{`")?;
        self.skip_newlines();
        let mut fields = Vec::new();
        while !self.at(&TokenKind::RBrace) && !self.at(&TokenKind::Eof) {
            let start = self.current_span();
            let name = self.expect_ident("field name")?;
            self.expect(&TokenKind::Colon, "`:`")?;
            let ty = self.parse_type()?;
            let initializer = if self.eat(&TokenKind::Eq) {
                Some(self.parse_expr()?)
            } else {
                None
            };
            let span_end = initializer.as_ref().map(|e| e.span()).unwrap_or(name.span);
            fields.push(FieldDecl {
                span: start.join(span_end),
                explicit_field_keyword: false,
                per_field_privacy: None,
                name,
                ty,
                initializer,
                annotations: Vec::new(),
            });
            let _ = self.eat(&TokenKind::Comma);
            self.skip_newlines();
        }
        self.expect(&TokenKind::RBrace, "`}`")?;
        Ok(fields)
    }

    fn parse_error_decl(&mut self, start: Span) -> Result<ErrorDecl, ParseError> {
        self.expect(&TokenKind::KwError, "`error`")?;
        let name = self.expect_ident("error name")?;
        self.expect(&TokenKind::LParen, "`(`")?;
        let mut fields = Vec::new();
        if !self.at(&TokenKind::RParen) {
            loop {
                let f_start = self.current_span();
                let fname = self.expect_ident("error field name")?;
                self.expect(&TokenKind::Colon, "`:`")?;
                let ty = self.parse_type()?;
                fields.push(ErrorField {
                    name: fname,
                    ty,
                    span: f_start.join(self.current_span()),
                });
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
        }
        let rp = self.expect(&TokenKind::RParen, "`)`")?;
        let span = start.join(rp.span);
        self.expect_stmt_terminator("after `error` decl")?;
        Ok(ErrorDecl { name, fields, span })
    }

    fn parse_event_decl(&mut self, start: Span) -> Result<EventDecl, ParseError> {
        self.expect(&TokenKind::KwEvent, "`event`")?;
        let name = self.expect_ident("event name")?;
        self.expect(&TokenKind::LParen, "`(`")?;
        let mut args = Vec::new();
        if !self.at(&TokenKind::RParen) {
            loop {
                let a_start = self.current_span();
                let aname = self.expect_ident("event arg name")?;
                self.expect(&TokenKind::Colon, "`:`")?;
                let ty = self.parse_type()?;
                // `indexed` is a contextual keyword here.
                let indexed = matches!(
                    self.peek_kind(),
                    Some(TokenKind::Ident(name)) if name.as_ref() == "indexed"
                );
                if indexed {
                    self.advance();
                }
                args.push(EventArg {
                    name: aname,
                    ty,
                    indexed,
                    span: a_start.join(self.current_span()),
                });
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
        }
        let rp = self.expect(&TokenKind::RParen, "`)`")?;
        let span = start.join(rp.span);
        self.expect_stmt_terminator("after `event` decl")?;
        Ok(EventDecl { name, args, span })
    }

    // ---------------- actions / views / reveals ----------------

    fn parse_action(
        &mut self,
        start: Span,
        annotations: Vec<Annotation>,
    ) -> Result<ActionDecl, ParseError> {
        self.expect(&TokenKind::KwAction, "`action`")?;
        let name = self.expect_ident("action name")?;
        let args = self.parse_arg_list()?;
        let (guards, qualifiers) = self.parse_guards_and_qualifiers()?;
        let body = self.parse_function_body()?;
        Ok(ActionDecl {
            span: start.join(self.current_span()),
            annotations,
            name,
            args,
            guards,
            qualifiers,
            body,
        })
    }

    fn parse_view(&mut self, start: Span) -> Result<ViewDecl, ParseError> {
        self.expect(&TokenKind::KwView, "`view`")?;
        let name = self.expect_ident("view name")?;
        let args = if self.at(&TokenKind::LParen) {
            self.parse_arg_list()?
        } else {
            Vec::new()
        };
        let returns = if self.eat(&TokenKind::KwReturns) {
            Some(self.parse_type()?)
        } else {
            None
        };
        let (guards, _q) = self.parse_guards_and_qualifiers()?;
        // View body is a single expression wrapped in braces.
        self.expect(&TokenKind::LBrace, "`{`")?;
        self.skip_newlines();
        let body = self.parse_expr()?;
        self.skip_newlines();
        let rb = self.expect(&TokenKind::RBrace, "`}`")?;
        Ok(ViewDecl {
            span: start.join(rb.span),
            name,
            args,
            returns,
            guards,
            body,
        })
    }

    fn parse_reveal(&mut self, start: Span) -> Result<RevealDecl, ParseError> {
        self.expect(&TokenKind::KwReveal, "`reveal`")?;
        let name = self.expect_ident("reveal name")?;

        // Arg list is optional (absent in the one-liner form).
        let args = if self.at(&TokenKind::LParen) {
            self.parse_arg_list()?
        } else {
            Vec::new()
        };
        let returns = if self.eat(&TokenKind::KwReturns) {
            Some(self.parse_type()?)
        } else {
            None
        };
        let target = if self.eat(&TokenKind::KwTo) {
            Some(self.parse_reveal_target()?)
        } else {
            None
        };
        // Look at the next significant token to pick one-liner vs block form.
        // Newlines between `reveal winner returns choice` and the `when` /
        // `{` are common; we have to skip them before the lookahead.
        let saved_pos = self.pos;
        self.skip_newlines();
        let has_guards = matches!(
            self.peek_kind(),
            Some(TokenKind::KwWhen)
                | Some(TokenKind::KwOnly)
                | Some(TokenKind::KwGiven)
                | Some(TokenKind::KwVerifiedBy)
                | Some(TokenKind::KwPqSigned)
                | Some(TokenKind::KwVdfLocked)
        );
        let has_brace = self.at(&TokenKind::LBrace);
        if !has_guards && !has_brace {
            // Restore so the one-liner terminator check still sees the newline.
            self.pos = saved_pos;
            // One-liner.
            let span = start.join(self.current_span());
            self.expect_stmt_terminator("after reveal one-liner")?;
            return Ok(RevealDecl {
                span,
                name,
                args,
                returns,
                target,
                guards: Vec::new(),
                body: None,
            });
        }
        // Block form — newlines were already skipped by saved_pos update.

        let (guards, _q) = self.parse_guards_and_qualifiers()?;
        self.expect(&TokenKind::LBrace, "`{`")?;
        self.skip_newlines();
        let body_expr = self.parse_expr()?;
        self.skip_newlines();
        let rb = self.expect(&TokenKind::RBrace, "`}`")?;
        Ok(RevealDecl {
            span: start.join(rb.span),
            name,
            args,
            returns,
            target,
            guards,
            body: Some(body_expr),
        })
    }

    fn parse_reveal_target(&mut self) -> Result<RevealTarget, ParseError> {
        match self.peek_kind() {
            Some(TokenKind::Ident(name)) => {
                let s = name.clone();
                let span = self.current_span();
                if s.as_ref() == "owner" {
                    self.advance();
                    return Ok(RevealTarget::Owner);
                }
                if s.as_ref() == "caller" {
                    self.advance();
                    return Ok(RevealTarget::Caller);
                }
                if s.as_ref() == "parties" {
                    self.advance();
                    return Ok(RevealTarget::Parties);
                }
                // Otherwise it's an ad-hoc address expression.
                let expr = self.parse_expr()?;
                let _ = span; // unused
                Ok(RevealTarget::Address(expr))
            }
            _ => {
                let expr = self.parse_expr()?;
                Ok(RevealTarget::Address(expr))
            }
        }
    }

    fn parse_arg_list(&mut self) -> Result<Vec<Arg>, ParseError> {
        self.expect(&TokenKind::LParen, "`(`")?;
        let mut out = Vec::new();
        if !self.at(&TokenKind::RParen) {
            loop {
                let a_start = self.current_span();
                let name = self.expect_ident("argument name")?;
                self.expect(&TokenKind::Colon, "`:`")?;
                let ty = self.parse_type()?;
                out.push(Arg {
                    name,
                    ty,
                    span: a_start.join(self.current_span()),
                });
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(&TokenKind::RParen, "`)`")?;
        Ok(out)
    }

    /// Parse a guard/qualifier list: mixed `when`/`only`/`given`/`verified_by`/
    /// `pq_signed`/`vdf_locked` clauses separated by commas. Terminates at the
    /// opening brace of the body.
    ///
    /// Newlines between guards are allowed: Covenant idiom is to put each guard
    /// on its own line for readability. We silently skip newlines at the top of
    /// the loop and after commas.
    fn parse_guards_and_qualifiers(
        &mut self,
    ) -> Result<(Vec<Guard>, Vec<ActionQualifier>), ParseError> {
        let mut guards = Vec::new();
        let mut quals = Vec::new();
        loop {
            self.skip_newlines();
            match self.peek_kind() {
                Some(TokenKind::KwWhen) => {
                    self.advance();
                    let e = self.parse_expr()?;
                    guards.push(Guard::When(e));
                }
                Some(TokenKind::KwOnly) => {
                    self.advance();
                    let p = self.parse_principal()?;
                    guards.push(Guard::Only(p));
                }
                Some(TokenKind::KwGiven) => {
                    self.advance();
                    let e = self.parse_expr()?;
                    guards.push(Guard::Given(e));
                }
                Some(TokenKind::KwVerifiedBy) => {
                    self.advance();
                    self.expect(&TokenKind::LParen, "`(`")?;
                    let e = self.parse_expr()?;
                    self.expect(&TokenKind::RParen, "`)`")?;
                    quals.push(ActionQualifier::VerifiedBy(e));
                }
                Some(TokenKind::KwPqSigned) => {
                    let start = self.current_span();
                    self.advance();
                    self.expect(&TokenKind::LParen, "`(`")?;
                    let msg = self.parse_expr()?;
                    self.expect(&TokenKind::Comma, "`,`")?;
                    let sig = self.parse_expr()?;
                    self.expect(&TokenKind::Comma, "`,`")?;
                    let pk = self.parse_expr()?;
                    let rp = self.expect(&TokenKind::RParen, "`)`")?;
                    quals.push(ActionQualifier::PqSigned {
                        msg,
                        sig,
                        pk,
                        span: start.join(rp.span),
                    });
                }
                Some(TokenKind::KwVdfLocked) => {
                    let start = self.current_span();
                    self.advance();
                    self.expect(&TokenKind::KwFor, "`for`")?;
                    let delay = self.parse_expr()?;
                    quals.push(ActionQualifier::VdfLocked {
                        span: start.join(delay.span()),
                        delay,
                    });
                }
                _ => break,
            }
            if !self.eat(&TokenKind::Comma) {
                break;
            }
            self.skip_newlines();
        }
        Ok((guards, quals))
    }

    // ---------------- annotations ----------------

    fn parse_annotation(&mut self) -> Result<Annotation, ParseError> {
        let start = self.current_span();
        self.expect(&TokenKind::At, "`@`")?;
        let name = self.expect_ident("annotation name after `@`")?;
        let mut args = Vec::new();
        if self.eat(&TokenKind::LParen) {
            if !self.at(&TokenKind::RParen) {
                loop {
                    // Named form: `<ident>: <expr>`.
                    let is_named = matches!(
                        (self.peek_kind(), self.peek_ahead(1).map(|t| &t.kind)),
                        (Some(TokenKind::Ident(_)), Some(TokenKind::Colon))
                    );
                    if is_named {
                        let n = self.expect_ident("annotation arg name")?;
                        self.expect(&TokenKind::Colon, "`:`")?;
                        let v = self.parse_expr()?;
                        args.push(AnnotationArg::Named { name: n, value: v });
                    } else {
                        args.push(AnnotationArg::Positional(self.parse_expr()?));
                    }
                    if !self.eat(&TokenKind::Comma) {
                        break;
                    }
                }
            }
            self.expect(&TokenKind::RParen, "`)`")?;
        }
        let span = start.join(self.current_span());
        Ok(Annotation { name, args, span })
    }

    // ---------------- version / migrate / on_destroy ----------------

    fn parse_version_block(&mut self, start: Span) -> Result<VersionBlock, ParseError> {
        self.expect(&TokenKind::KwVersion, "`version`")?;
        let n = self.expect_int_literal("version number")?;
        let body = self.parse_top_level_body()?;
        Ok(VersionBlock {
            number: n,
            body,
            span: start.join(self.current_span()),
        })
    }

    fn parse_migrate_block(&mut self, start: Span) -> Result<MigrateBlock, ParseError> {
        self.expect(&TokenKind::KwMigrate, "`migrate`")?;
        self.expect(&TokenKind::KwFrom, "`from`")?;
        let from = self.expect_int_literal("source version")?;
        self.expect(&TokenKind::KwTo, "`to`")?;
        let to = self.expect_int_literal("target version")?;
        let body = self.parse_function_body()?;
        Ok(MigrateBlock {
            from,
            to,
            body,
            span: start.join(self.current_span()),
        })
    }

    fn parse_on_destroy(&mut self, start: Span) -> Result<OnDestroyBlock, ParseError> {
        self.expect(&TokenKind::KwOnDestroy, "`on_destroy`")?;
        let body = self.parse_function_body()?;
        Ok(OnDestroyBlock {
            body,
            span: start.join(self.current_span()),
        })
    }

    fn parse_selective_disclosure(
        &mut self,
        start: Span,
        _annotations: Vec<Annotation>,
    ) -> Result<SelectiveDisclosureDecl, ParseError> {
        self.expect(&TokenKind::KwSelectiveDisclosure, "`selective_disclosure`")?;
        let name = self.expect_ident("disclosure name")?;
        let args = self.parse_arg_list()?;
        self.expect(&TokenKind::KwUsing, "`using`")?;
        let circuit_name = self.expect_ident("circuit name")?;
        self.expect(&TokenKind::LBrace, "`{`")?;
        self.skip_newlines();
        let body = self.parse_expr()?;
        self.skip_newlines();
        let rb = self.expect(&TokenKind::RBrace, "`}`")?;
        Ok(SelectiveDisclosureDecl {
            name,
            args,
            circuit_name,
            body,
            span: start.join(rb.span),
        })
    }

    // ---------------- principals ----------------

    pub(crate) fn parse_principal(&mut self) -> Result<crate::ast::Principal, ParseError> {
        use crate::ast::{Principal, PrincipalKind};
        match self.peek() {
            Some(t) => match &t.kind {
                TokenKind::Ident(name) => {
                    let name_s = name.as_ref();
                    let span = t.span;
                    // Named principals recognized by predicate keyword:
                    let kind = match name_s {
                        "owner" => Some(PrincipalKind::Owner),
                        "deployer" => Some(PrincipalKind::Deployer),
                        "admin" => Some(PrincipalKind::Admin),
                        "caller" => Some(PrincipalKind::Caller),
                        "guardians" => Some(PrincipalKind::Guardians),
                        "parties" => Some(PrincipalKind::Parties),
                        "holders" => Some(PrincipalKind::Holders),
                        _ => None,
                    };
                    if let Some(k) = kind {
                        self.advance();
                        return Ok(Principal::Named(k));
                    }
                    // Check for a call form: `<ident> ( ... )`.
                    let ident = Ident {
                        name: name.clone(),
                        span,
                    };
                    self.advance();
                    if self.eat(&TokenKind::LParen) {
                        let args = self.parse_expr_list_until(&TokenKind::RParen)?;
                        self.expect(&TokenKind::RParen, "`)`")?;
                        return Ok(Principal::Call { name: ident, args });
                    }
                    Ok(Principal::Predicate(ident))
                }
                _ => {
                    let e = self.parse_expr()?;
                    Ok(crate::ast::Principal::Address(e))
                }
            },
            None => {
                let span = self.current_span();
                self.push_diag(diag::unexpected_eof(span, "parsing a principal"));
                Err(ParseError)
            }
        }
    }

    pub(crate) fn expect_int_literal(&mut self, what: &str) -> Result<u32, ParseError> {
        match self.peek() {
            Some(t) => match &t.kind {
                TokenKind::Integer(v) => {
                    if *v > u32::MAX as u128 {
                        let span = t.span;
                        self.push_diag(diag::unexpected_token(
                            span,
                            format!("integer literal for {what} exceeds u32 range"),
                        ));
                        return Err(ParseError);
                    }
                    let out = *v as u32;
                    self.advance();
                    Ok(out)
                }
                _ => {
                    let span = t.span;
                    let got = describe(&t.kind);
                    self.push_diag(diag::expected(span, what, &got));
                    Err(ParseError)
                }
            },
            None => {
                let span = self.current_span();
                self.push_diag(diag::unexpected_eof(span, &format!("expecting {what}")));
                Err(ParseError)
            }
        }
    }
}

#[allow(dead_code)]
fn _suppress_unused(_e: Expr) {}

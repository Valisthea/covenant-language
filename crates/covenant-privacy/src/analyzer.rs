//! Single-pass privacy flow analyzer.
//!
//! See Doc 8 §3.1 (P1) and §4.2 (informal soundness argument). This file is
//! the executable realization of the informal proof's Steps 2-4.

use covenant_diag::{Diagnostic, Span};
use covenant_parser::ast::{
    ActionDecl, ActionQualifier, ConstructKind, Expr, LValue, OnDestroyBlock, RevealDecl, Stmt,
    TopLevelDecl, ViewDecl,
};
use covenant_resolver::{Binding, LangIdent, StdlibFn};
use covenant_types::{Ty, TypedFile};

use crate::diag;
use crate::domain::{domain_of, FlowContext, PrivacyDomain};
use crate::table::PrivacyTable;

pub struct Analyzer {
    pub(crate) typed: TypedFile,
    pub(crate) domains: PrivacyTable,
    pub(crate) diagnostics: Vec<Diagnostic>,
    /// Stack of flow contexts. Top is current.
    pub(crate) context: Vec<FlowContext>,
    /// Qualifiers of the currently-walked action (for R5 proof_payload).
    pub(crate) current_qualifiers: Vec<ActionQualifier>,
    /// Count of `Stmt::EncryptedWhen` encountered in the current action body.
    pub(crate) enc_when_count: usize,
    /// Span of the currently-walked action for W305 attribution.
    pub(crate) current_action_span: Option<Span>,
    /// The top-level construct kind. Retained for future per-construct
    /// privacy rules (e.g., `confidential token` ABI enforcement).
    #[allow(dead_code)]
    pub(crate) construct_kind: ConstructKind,
}

impl Analyzer {
    pub fn new(typed: TypedFile) -> Self {
        let construct_kind = typed.resolved.file.top_level.keyword;
        Self {
            typed,
            domains: PrivacyTable::new(),
            diagnostics: Vec::new(),
            context: Vec::new(),
            current_qualifiers: Vec::new(),
            enc_when_count: 0,
            current_action_span: None,
            construct_kind,
        }
    }

    pub fn run(mut self) -> (crate::PrivacyCheckedFile, Vec<Diagnostic>) {
        // Pass 1: derive every expr/local/decl domain from its type.
        self.populate_domains_from_types();

        // Pass 2: per-construct structural checks.
        self.check_construct_structure();

        // Pass 3: walk behaviors applying rules.
        let body = self.typed.resolved.file.top_level.body.clone();
        for d in &body {
            self.walk_decl(d);
        }

        let checked = crate::PrivacyCheckedFile {
            typed: self.typed,
            domains: self.domains,
        };
        (checked, self.diagnostics)
    }

    // -------------------------------------------------------------------
    // Pass 1: derive domains from types
    // -------------------------------------------------------------------

    fn populate_domains_from_types(&mut self) {
        for (span, ty) in &self.typed.types.expr_types {
            self.domains.expr_domains.insert(*span, domain_of(ty));
        }
        for (i, ty) in self.typed.types.local_types.iter().enumerate() {
            let id = covenant_resolver::LocalId(i as u32);
            self.domains.set_local(id, domain_of(ty));
        }
        for (i, ty) in self.typed.types.decl_types.iter().enumerate() {
            let id = covenant_resolver::DeclId(i as u32);
            self.domains.set_decl(id, domain_of(ty));
        }
    }

    // -------------------------------------------------------------------
    // Pass 2: per-construct structure
    // -------------------------------------------------------------------

    fn check_construct_structure(&mut self) {
        let tl = &self.typed.resolved.file.top_level;
        if tl.keyword == ConstructKind::Ceremony {
            let has_on_destroy = tl
                .body
                .iter()
                .any(|d| matches!(d, TopLevelDecl::OnDestroy(_)));
            if !has_on_destroy {
                self.diagnostics
                    .push(diag::ceremony_no_ondestroy(tl.span, &tl.name.name));
            } else {
                // W309: on_destroy must call destroy() or freeze() at least once.
                let on_destroy_blocks: Vec<&OnDestroyBlock> = tl
                    .body
                    .iter()
                    .filter_map(|d| match d {
                        TopLevelDecl::OnDestroy(od) => Some(od),
                        _ => None,
                    })
                    .collect();
                let any_destroy_call = on_destroy_blocks
                    .iter()
                    .any(|od| self.body_has_destroy_call(&od.body));
                if !any_destroy_call {
                    if let Some(od) = on_destroy_blocks.first() {
                        self.diagnostics
                            .push(diag::warn_ceremony_no_destroy_key(od.span));
                    }
                }
            }
        }
    }

    fn body_has_destroy_call(&self, body: &[Stmt]) -> bool {
        fn visit_expr(e: &Expr) -> bool {
            match e {
                Expr::Call { callee, .. } => matches!(
                    callee.as_ref(),
                    Expr::Ident(id) if id.name.as_ref() == "destroy" || id.name.as_ref() == "freeze"
                ),
                _ => false,
            }
        }
        fn visit_stmt(s: &Stmt) -> bool {
            match s {
                Stmt::Expr(e, _) => visit_expr(e),
                Stmt::Let { value, .. } => visit_expr(value),
                Stmt::If {
                    then_block,
                    else_ifs,
                    else_block,
                    ..
                } => {
                    then_block.iter().any(visit_stmt)
                        || else_ifs.iter().any(|(_, b)| b.iter().any(visit_stmt))
                        || else_block
                            .as_ref()
                            .is_some_and(|b| b.iter().any(visit_stmt))
                }
                _ => false,
            }
        }
        body.iter().any(visit_stmt)
    }

    // -------------------------------------------------------------------
    // Pass 3: declarations and behavior walking
    // -------------------------------------------------------------------

    fn walk_decl(&mut self, d: &TopLevelDecl) {
        match d {
            TopLevelDecl::Action(a) => self.walk_action(a),
            TopLevelDecl::View(v) => self.walk_view(v),
            TopLevelDecl::Reveal(r) => self.walk_reveal(r),
            TopLevelDecl::OnDestroy(od) => self.walk_on_destroy(od),
            TopLevelDecl::SelectiveDisclosure(sd) => {
                self.context.push(FlowContext::SelectiveDisclosure);
                self.walk_expr(&sd.body);
                // R R8 analog: selective_disclosure must yield Bool. Phase 4 already
                // enforces this; belt-and-suspenders here.
                if let Some(Ty::Unknown) | Some(Ty::Bool) =
                    self.typed.types.get_expr(&sd.body.span()).cloned()
                {
                    // ok
                } else if let Some(ty) = self.typed.types.get_expr(&sd.body.span()).cloned() {
                    if !matches!(ty, Ty::Bool | Ty::Unknown) {
                        self.diagnostics.push(diag::sd_non_bool(sd.body.span()));
                    }
                }
                self.context.pop();
            }
            TopLevelDecl::Version(v) => {
                for inner in &v.body {
                    self.walk_decl(inner);
                }
            }
            TopLevelDecl::Migrate(m) => {
                self.context.push(FlowContext::Normal);
                for s in &m.body {
                    self.walk_stmt(s);
                }
                self.context.pop();
            }
            _ => {}
        }
    }

    fn walk_action(&mut self, a: &ActionDecl) {
        self.current_qualifiers = a.qualifiers.clone();
        self.enc_when_count = 0;
        self.current_action_span = Some(a.span);
        self.context.push(FlowContext::Normal);

        for g in &a.guards {
            match g {
                covenant_parser::ast::Guard::When(e) | covenant_parser::ast::Guard::Given(e) => {
                    self.walk_expr(e)
                }
                covenant_parser::ast::Guard::Only(_) => {}
            }
        }
        for q in &a.qualifiers {
            match q {
                ActionQualifier::VerifiedBy(e) => self.walk_expr(e),
                ActionQualifier::PqSigned { msg, sig, pk, .. } => {
                    self.walk_expr(msg);
                    self.walk_expr(sig);
                    self.walk_expr(pk);
                }
                ActionQualifier::VdfLocked { delay, .. } => self.walk_expr(delay),
            }
        }

        for s in &a.body {
            self.walk_stmt(s);
        }

        // R10: cumulative encrypted-comparison warning.
        if self.enc_when_count > 3 {
            self.diagnostics
                .push(diag::warn_many_enc_comparisons(a.span, self.enc_when_count));
        }

        self.context.pop();
        self.current_qualifiers.clear();
        self.current_action_span = None;
    }

    fn walk_view(&mut self, v: &ViewDecl) {
        self.context.push(FlowContext::Normal);
        for g in &v.guards {
            if let covenant_parser::ast::Guard::When(e) | covenant_parser::ast::Guard::Given(e) = g
            {
                self.walk_expr(e);
            }
        }
        self.walk_expr(&v.body);

        // R2: view body must be plaintext.
        let body_dom = self.expr_domain(&v.body);
        if body_dom == PrivacyDomain::Encrypted {
            self.diagnostics
                .push(diag::view_encrypted_body(v.body.span()));
        }
        self.context.pop();
    }

    fn walk_reveal(&mut self, r: &RevealDecl) {
        self.context.push(FlowContext::Normal);

        for g in &r.guards {
            if let covenant_parser::ast::Guard::When(e) | covenant_parser::ast::Guard::Given(e) = g
            {
                self.walk_expr(e);
            }
        }

        if let Some(body) = &r.body {
            self.walk_expr(body);
            let body_dom = self.expr_domain(body);
            // R4: block-form reveal crossing the boundary must declare a target.
            if r.target.is_none() && body_dom == PrivacyDomain::Encrypted {
                self.diagnostics.push(diag::reveal_missing_target(r.span));
            }
        } else {
            // One-liner form `reveal <field> to <target>`.
            // R3: warn if the field is plaintext (reveal is no-op).
            let field_dom = match self.typed.resolved.bindings.get(&r.name.span) {
                Some(Binding::Field(d)) => self.domains.decl(*d),
                _ => PrivacyDomain::Unknown,
            };
            if field_dom == PrivacyDomain::Plaintext {
                self.diagnostics.push(diag::warn_reveal_plaintext(r.span));
            }
        }
        self.context.pop();
    }

    fn walk_on_destroy(&mut self, od: &OnDestroyBlock) {
        self.context.push(FlowContext::OnDestroy);
        for s in &od.body {
            self.walk_stmt(s);
        }
        self.context.pop();
    }

    // -------------------------------------------------------------------
    // Statement walker: where the bulk of the flow rules live
    // -------------------------------------------------------------------

    fn walk_stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Let { value, .. } => self.walk_expr(value),
            Stmt::Assign {
                target,
                value,
                span,
                ..
            } => {
                self.walk_lvalue(target);
                self.walk_expr(value);
                // R1: plaintext-target + encrypted-source.
                let target_dom = self.lvalue_domain(target);
                let value_dom = self.expr_domain(value);
                if target_dom == PrivacyDomain::Plaintext && value_dom == PrivacyDomain::Encrypted {
                    self.diagnostics.push(diag::plaintext_from_encrypted(*span));
                }
            }
            Stmt::Expr(e, _) => self.walk_expr(e),
            Stmt::Match {
                scrutinee, arms, ..
            } => {
                self.walk_expr(scrutinee);
                for arm in arms {
                    let covenant_parser::ast::MatchPattern::Literal(p) = &arm.pattern;
                    self.walk_expr(p);
                    match &arm.body {
                        covenant_parser::ast::MatchBody::Stmt(inner) => self.walk_stmt(inner),
                        covenant_parser::ast::MatchBody::Block(ss) => {
                            for s in ss {
                                self.walk_stmt(s);
                            }
                        }
                    }
                }
            }
            Stmt::If {
                cond,
                then_block,
                else_ifs,
                else_block,
                ..
            } => {
                self.walk_expr(cond);
                for s in then_block {
                    self.walk_stmt(s);
                }
                for (c, b) in else_ifs {
                    self.walk_expr(c);
                    for s in b {
                        self.walk_stmt(s);
                    }
                }
                if let Some(b) = else_block {
                    for s in b {
                        self.walk_stmt(s);
                    }
                }
            }
            Stmt::EncryptedWhen {
                cond,
                then_block,
                otherwise,
                ..
            } => {
                self.walk_expr(cond);
                self.enc_when_count += 1;
                // W306: cond should be ciphertext. If plaintext, suggest `if`.
                let cond_dom = self.expr_domain(cond);
                if cond_dom == PrivacyDomain::Plaintext {
                    self.diagnostics
                        .push(diag::warn_enc_when_plaintext_cond(cond.span()));
                }
                self.context.push(FlowContext::EncryptedThen);
                for s in then_block {
                    self.walk_stmt(s);
                }
                self.context.pop();
                if let Some(o) = otherwise {
                    self.context.push(FlowContext::EncryptedOtherwise);
                    for s in o {
                        self.walk_stmt(s);
                    }
                    self.context.pop();
                }
            }
            Stmt::ForEach { iter, body, .. } => {
                self.walk_expr(iter);
                for s in body {
                    self.walk_stmt(s);
                }
            }
            Stmt::TryCatch {
                body, catch_body, ..
            } => {
                for s in body {
                    self.walk_stmt(s);
                }
                for s in catch_body {
                    self.walk_stmt(s);
                }
            }
            Stmt::Emit { args, span, .. } => {
                for a in args {
                    self.walk_expr(a);
                }
                // R8: inside encrypted branch → warn (or elevate to error if any
                // arg has Encrypted domain).
                if self.in_encrypted_branch() {
                    let any_enc = args
                        .iter()
                        .any(|a| self.expr_domain(a) == PrivacyDomain::Encrypted);
                    if any_enc {
                        self.diagnostics.push(diag::emit_encrypted_arg(*span));
                    } else {
                        self.diagnostics.push(diag::warn_emit_in_enc(*span));
                    }
                }
            }
            Stmt::Transfer {
                amount,
                from,
                to,
                span,
            } => {
                self.walk_expr(amount);
                if let Some(f) = from {
                    self.walk_expr(f);
                }
                self.walk_expr(to);
                // R9: transfer inside encrypted branch.
                if self.in_encrypted_branch() {
                    self.diagnostics.push(diag::warn_transfer_in_enc(*span));
                }
            }
            Stmt::Append { struct_lit, .. } => {
                for fa in struct_lit {
                    self.walk_expr(&fa.value);
                }
            }
            Stmt::Discard(lv, _) | Stmt::Delete(lv, _) => self.walk_lvalue(lv),
            Stmt::Return(e, _) => {
                if let Some(e) = e {
                    self.walk_expr(e);
                }
            }
            Stmt::RevertWith { args, span, .. } => {
                for a in args {
                    self.walk_expr(a);
                }
                if self.in_encrypted_branch() {
                    let any_enc = args
                        .iter()
                        .any(|a| self.expr_domain(a) == PrivacyDomain::Encrypted);
                    if any_enc {
                        self.diagnostics.push(diag::revert_encrypted_arg(*span));
                    } else if !args.is_empty() {
                        self.diagnostics
                            .push(diag::warn_revert_params_in_enc(*span));
                    }
                }
            }
        }
    }

    fn walk_lvalue(&mut self, lv: &LValue) {
        match lv {
            LValue::Ident(_) => {}
            LValue::FieldAccess(base, _) => self.walk_lvalue(base),
            LValue::Index(base, idx) => {
                self.walk_lvalue(base);
                self.walk_expr(idx);
            }
        }
    }

    // -------------------------------------------------------------------
    // Expression walker: records domains and checks per-expression rules.
    // -------------------------------------------------------------------

    fn walk_expr(&mut self, e: &Expr) {
        // Per-expression rules:
        if let Expr::Ident(id) = e {
            // R5: proof_payload must be inside a verified_by action. Handles
            // both the "properly bound" case (Phase 3 seeded it in verified_by
            // scope) and the "spelled proof_payload but unresolved" case (Phase 3
            // did not seed it outside verified_by).
            let is_proof_payload_binding = matches!(
                self.typed.resolved.bindings.get(&id.span),
                Some(Binding::LangIdent(LangIdent::ProofPayload))
            );
            let is_proof_payload_by_name = id.name.as_ref() == "proof_payload";
            if is_proof_payload_binding || is_proof_payload_by_name {
                let in_verified = self
                    .current_qualifiers
                    .iter()
                    .any(|q| matches!(q, ActionQualifier::VerifiedBy(_)));
                if !in_verified {
                    self.diagnostics.push(diag::proof_payload_context(id.span));
                }
            }
        }
        if let Expr::Call { callee, .. } = e {
            if let Expr::Ident(id) = callee.as_ref() {
                if let Some(Binding::StdlibFn(f)) = self.typed.resolved.bindings.get(&id.span) {
                    // R6: destroy/freeze only inside on_destroy.
                    if matches!(f, StdlibFn::Destroy | StdlibFn::Freeze) {
                        let in_on_destroy = self
                            .context
                            .iter()
                            .any(|c| matches!(c, FlowContext::OnDestroy));
                        if !in_on_destroy {
                            self.diagnostics
                                .push(diag::destroy_context(id.span, &id.name));
                        }
                    }
                }
            }
        }

        // Recurse.
        match e {
            Expr::Literal(_) | Expr::Ident(_) => {}
            Expr::Binary { lhs, rhs, .. } => {
                self.walk_expr(lhs);
                self.walk_expr(rhs);
            }
            Expr::Unary { operand, .. } => self.walk_expr(operand),
            Expr::Call { callee, args, .. } => {
                self.walk_expr(callee);
                for a in args {
                    self.walk_expr(a);
                }
            }
            Expr::FieldAccess { base, .. } => self.walk_expr(base),
            Expr::Index { base, index, .. } => {
                self.walk_expr(base);
                self.walk_expr(index);
            }
            Expr::Slice { base, from, to, .. } => {
                self.walk_expr(base);
                self.walk_expr(from);
                self.walk_expr(to);
            }
            Expr::Array { elements, .. } => {
                for el in elements {
                    self.walk_expr(el);
                }
            }
            Expr::Paren(inner) | Expr::EncryptedLit(inner, _) => self.walk_expr(inner),
            Expr::If {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                self.walk_expr(cond);
                self.walk_expr(then_expr);
                self.walk_expr(else_expr);
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                self.walk_expr(scrutinee);
                for arm in arms {
                    let covenant_parser::ast::MatchPattern::Literal(p) = &arm.pattern;
                    self.walk_expr(p);
                    self.walk_expr(&arm.body);
                }
            }
            Expr::Lambda { body, .. } => self.walk_expr(body),
        }
    }

    // -------------------------------------------------------------------
    // Helpers
    // -------------------------------------------------------------------

    fn expr_domain(&self, e: &Expr) -> PrivacyDomain {
        self.domains.get_expr(&e.span())
    }

    fn lvalue_domain(&self, lv: &LValue) -> PrivacyDomain {
        match lv {
            LValue::Ident(id) => {
                // Resolve to Field or Local via the resolver.
                match self.typed.resolved.bindings.get(&id.span) {
                    Some(Binding::Field(d)) => self.domains.decl(*d),
                    Some(Binding::Local(l)) => self.domains.local(*l),
                    _ => PrivacyDomain::Unknown,
                }
            }
            LValue::FieldAccess(base, field) => {
                // OMEGA V6 (MED-001 fix): resolve the BASE's precise struct/
                // credential type (via the now-recursive `lvalue_type_from_ast`,
                // which reaches through `Index`/`FieldAccess` chains the same
                // way the type-checker's `field_access_ty`/`indexed_elem_ty`
                // already do) and look up the SPECIFIC field's own declared
                // type, instead of approximating from the base's aggregate
                // domain. The old fallback-to-Plaintext-unless-base-is-
                // Encrypted logic could never actually see Encrypted here
                // (a struct/list base is never itself `Ty::Ciphertext`), so
                // it always answered Plaintext regardless of whether the
                // named field was genuinely `ciphertext<T>` -- spuriously
                // rejecting (E301) legitimate encrypted-field assignments
                // into structs held inside `list`/`map` containers (e.g. a
                // confidential ballot/registry pattern).
                match self.lvalue_type_from_ast(base) {
                    Some(Ty::Struct(sid)) => self
                        .typed
                        .types
                        .struct_info(sid)
                        .and_then(|info| info.fields.iter().find(|f| f.name.name == field.name))
                        .map(|f| domain_of(&f.ty))
                        .unwrap_or(PrivacyDomain::Plaintext),
                    Some(Ty::Credential(cid)) => self
                        .typed
                        .types
                        .credential_info(cid)
                        .and_then(|info| info.fields.iter().find(|f| f.name.name == field.name))
                        .map(|f| domain_of(&f.ty))
                        .unwrap_or(PrivacyDomain::Plaintext),
                    _ => {
                        // Base type not precisely resolvable (unusual shape) --
                        // keep the historical conservative fallback.
                        if self.lvalue_domain(base) == PrivacyDomain::Encrypted {
                            PrivacyDomain::Encrypted
                        } else {
                            PrivacyDomain::Plaintext
                        }
                    }
                }
            }
            LValue::Index(base, _) => {
                // Indexing a map of ciphertext-valued entries yields Encrypted.
                // Check the inner element type via the TypeTable.
                let base_lv_ty = self.lvalue_type_from_ast(base);
                match base_lv_ty {
                    Some(Ty::Map(_, v)) => domain_of(&v),
                    Some(Ty::List(inner)) => domain_of(&inner),
                    _ => PrivacyDomain::Plaintext,
                }
            }
        }
    }

    fn lvalue_type_from_ast(&self, lv: &LValue) -> Option<Ty> {
        match lv {
            LValue::Ident(id) => match self.typed.resolved.bindings.get(&id.span) {
                Some(Binding::Field(d)) => Some(self.typed.types.decl_ty(*d)),
                Some(Binding::Local(l)) => Some(self.typed.types.local_ty(*l)),
                _ => None,
            },
            // OMEGA V6 (MED-001 fix): recurse into FieldAccess/Index bases
            // instead of giving up -- mirrors `covenant-types::checker`'s
            // `synth_lvalue`/`field_access_ty`/`indexed_elem_ty`, which
            // already do this correctly on the type-checking side.
            LValue::FieldAccess(base, field) => {
                let base_ty = self.lvalue_type_from_ast(base)?;
                match base_ty {
                    Ty::Struct(sid) => self
                        .typed
                        .types
                        .struct_info(sid)
                        .and_then(|info| info.fields.iter().find(|f| f.name.name == field.name))
                        .map(|f| f.ty.clone()),
                    Ty::Credential(cid) => self
                        .typed
                        .types
                        .credential_info(cid)
                        .and_then(|info| info.fields.iter().find(|f| f.name.name == field.name))
                        .map(|f| f.ty.clone()),
                    _ => None,
                }
            }
            LValue::Index(base, _) => {
                let base_ty = self.lvalue_type_from_ast(base)?;
                match base_ty {
                    Ty::Map(_, v) => Some(*v),
                    Ty::List(inner) => Some(*inner),
                    _ => None,
                }
            }
        }
    }

    fn in_encrypted_branch(&self) -> bool {
        self.context
            .last()
            .map(|c| c.is_encrypted_branch())
            .unwrap_or(false)
    }
}

//! The main bidirectional type checker.
//!
//! Organization:
//! 1. `Checker::new`: builds a fresh checker over a `ResolvedFile`.
//! 2. `run()`: orchestrates Pass 1 (lower AST type annotations → Ty),
//!    Pass 2 (register decl types), Pass 3 (typecheck expression bodies).
//! 3. `synth_expr` / `check_expr`: bidirectional core.
//! 4. Statement / behavior / principal / qualifier typecheck helpers.

use covenant_diag::{Diagnostic, Span};
use covenant_parser::ast::{
    ActionDecl, ActionQualifier, Annotation, AnnotationArg, Arg, BinaryOp, ConstructKind,
    CredentialDecl, ErrorDecl, Expr, ExternalContractDecl, FieldDecl, File, Ident, LValue,
    LiteralExpr, MatchBody, MatchPattern, MetadataValue, OnDestroyBlock, Principal,
    PrivacyQualifier, RevealDecl, RevealTarget, Stmt, StructDecl, TopLevelDecl, Type as AstType,
    ViewDecl,
};
use covenant_resolver::{
    Binding, BuiltinPredicate, DeclId, DeclInfo, DeclKind, LangIdent, LocalId, ResolvedFile,
    StdlibFn, StdlibModule,
};

use crate::diag;
use crate::operator::{binop_name, dispatch_binary, dispatch_unary, unop_name, OpResult};
use crate::stdlib::{
    module_from_tag, module_tag, stdlib_fn_signature, stdlib_method_signature, Signature,
    VariadicKind,
};
use crate::table::{
    ChoiceTypeInfo, CredentialInfo, LiftMarker, StructField, StructInfo, TypeTable,
};
use crate::ty::{Namespace, Ty};

/// Maximum `synth_expr`/`check_expr` recursion depth. See
/// `covenant_parser::parser::MAX_PARSE_DEPTH` for the empirical basis
/// (OMEGA V6 HGH-029).
const MAX_EXPR_DEPTH: u32 = 96;

pub struct Checker {
    pub(crate) file: File,
    pub(crate) resolved: ResolvedFile,
    pub(crate) table: TypeTable,
    pub(crate) diagnostics: Vec<Diagnostic>,
    /// Whether the currently visited declaration is inside a construct whose
    /// privacy qualifier is `encrypted`/`sealed`/`confidential`: in that case
    /// fields without per-field override default to `Ciphertext(T)`.
    pub(crate) construct_encrypted: bool,
    /// The ConstructKind currently being checked (for implicit-field types and
    /// return-type checks).
    pub(crate) construct_kind: ConstructKind,
    /// Return type expected by the currently-checked view/reveal.
    pub(crate) current_return_ty: Option<Ty>,
    /// True when the currently-checked behavior is an `action` (for `return`
    /// rejection).
    pub(crate) in_action: bool,
    /// Current `synth_expr` recursion depth (OMEGA V6 HGH-029 guard).
    expr_depth: u32,
}

impl Checker {
    pub fn new(resolved: ResolvedFile) -> Self {
        let file = resolved.file.clone();
        let construct_encrypted = matches!(
            file.top_level.privacy,
            Some(PrivacyQualifier::Encrypted)
                | Some(PrivacyQualifier::Sealed)
                | Some(PrivacyQualifier::Confidential)
        );
        let construct_kind = file.top_level.keyword;
        Self {
            file,
            resolved,
            table: TypeTable::new(),
            diagnostics: Vec::new(),
            construct_encrypted,
            construct_kind,
            current_return_ty: None,
            in_action: false,
            expr_depth: 0,
        }
    }

    pub fn run(mut self) -> (crate::TypedFile, Vec<Diagnostic>) {
        // Pass 1+2: register nominal types, lower decl annotations.
        self.register_nominal_types();
        self.register_decl_types();

        // Pass 3: typecheck expression bodies.
        let body = self.file.top_level.body.clone();
        for d in &body {
            self.check_decl(d);
        }

        let typed = crate::TypedFile {
            resolved: self.resolved,
            types: self.table,
        };
        (typed, self.diagnostics)
    }

    // ---------------------------------------------------------------
    // Pass 1: nominal type registration
    // ---------------------------------------------------------------

    fn register_nominal_types(&mut self) {
        let body = self.file.top_level.body.clone();
        for d in &body {
            match d {
                TopLevelDecl::Struct(s) => self.register_struct(s),
                TopLevelDecl::Credential(c) => self.register_credential(c),
                _ => {}
            }
        }
    }

    fn register_struct(&mut self, s: &StructDecl) {
        let fields = s
            .fields
            .iter()
            .map(|f| StructField {
                name: f.name.clone(),
                ty: self.lower_type(&f.ty),
                indexed: false,
            })
            .collect();
        let id = self.table.intern_struct(StructInfo {
            name: s.name.clone(),
            fields,
            span: s.span,
        });
        // Bind the struct's name at decl-level to `Struct(id)`.
        if let Some(decl_id) = self.find_decl_by_name(&s.name.name, DeclKind::Struct) {
            self.table.set_decl(decl_id, Ty::Struct(id));
        }
    }

    fn register_credential(&mut self, c: &CredentialDecl) {
        let fields = c
            .fields
            .iter()
            .map(|f| StructField {
                name: f.name.clone(),
                ty: self.lower_type(&f.ty),
                indexed: false,
            })
            .collect();
        let id = self.table.intern_credential(CredentialInfo {
            name: c.name.clone(),
            fields,
            span: c.span,
        });
        if let Some(decl_id) = self.find_decl_by_name(&c.name.name, DeclKind::Credential) {
            self.table.set_decl(decl_id, Ty::Credential(id));
        }
    }

    // ---------------------------------------------------------------
    // Pass 2: decl type registration
    // ---------------------------------------------------------------

    fn register_decl_types(&mut self) {
        let body = self.file.top_level.body.clone();
        for d in &body {
            self.register_one_decl(d);
        }
    }

    fn register_one_decl(&mut self, d: &TopLevelDecl) {
        match d {
            TopLevelDecl::Field(f) => self.register_field(f),
            TopLevelDecl::Error(e) => self.register_error(e),
            TopLevelDecl::Event(e) => self.register_event(e),
            TopLevelDecl::Action(a) => self.register_action_sig(a),
            TopLevelDecl::View(v) => self.register_view_sig(v),
            TopLevelDecl::Reveal(r) => self.register_reveal_sig(r),
            TopLevelDecl::SelectiveDisclosure(_)
            | TopLevelDecl::Version(_)
            | TopLevelDecl::Migrate(_)
            | TopLevelDecl::OnDestroy(_)
            | TopLevelDecl::Struct(_)
            | TopLevelDecl::Credential(_)
            | TopLevelDecl::Metadata(_) => {}
        }
    }

    fn register_field(&mut self, f: &FieldDecl) {
        let base = self.lower_type(&f.ty);
        let effective = self.apply_construct_privacy(&f.per_field_privacy, base);
        if let Some(id) = self.find_decl_by_name(&f.name.name, DeclKind::Field) {
            self.table.set_decl(id, effective);
        }
    }

    fn apply_construct_privacy(&self, per_field: &Option<PrivacyQualifier>, base: Ty) -> Ty {
        match per_field {
            Some(PrivacyQualifier::Public) => base,
            Some(PrivacyQualifier::Encrypted) => base.wrap_ciphertext(),
            None => {
                if self.construct_encrypted {
                    base.wrap_ciphertext()
                } else {
                    base
                }
            }
            _ => base,
        }
    }

    fn register_error(&mut self, e: &ErrorDecl) {
        let sig: Vec<Ty> = e.fields.iter().map(|f| self.lower_type(&f.ty)).collect();
        if let Some(id) = self.find_decl_by_name(&e.name.name, DeclKind::Error) {
            self.table.error_signatures.insert(id, sig);
        }
    }

    fn register_event(&mut self, e: &covenant_parser::ast::EventDecl) {
        let sig: Vec<Ty> = e.args.iter().map(|a| self.lower_type(&a.ty)).collect();
        if let Some(id) = self.find_decl_by_name(&e.name.name, DeclKind::Event) {
            self.table.event_signatures.insert(id, sig);
        }
    }

    fn register_action_sig(&mut self, a: &ActionDecl) {
        let params: Vec<Ty> = a.args.iter().map(|arg| self.lower_type(&arg.ty)).collect();
        if let Some(id) = self.find_decl_by_name(&a.name.name, DeclKind::Action) {
            self.table.set_decl(
                id,
                Ty::Fn {
                    params,
                    ret: Box::new(Ty::Unit),
                },
            );
        }
    }

    fn register_view_sig(&mut self, v: &ViewDecl) {
        let params: Vec<Ty> = v.args.iter().map(|arg| self.lower_type(&arg.ty)).collect();
        let ret = v
            .returns
            .as_ref()
            .map(|t| self.lower_type(t))
            .unwrap_or(Ty::Unit);
        if let Some(id) = self.find_decl_by_name(&v.name.name, DeclKind::View) {
            self.table.set_decl(
                id,
                Ty::Fn {
                    params,
                    ret: Box::new(ret),
                },
            );
        }
    }

    fn register_reveal_sig(&mut self, r: &RevealDecl) {
        let params: Vec<Ty> = r.args.iter().map(|arg| self.lower_type(&arg.ty)).collect();
        let ret = r
            .returns
            .as_ref()
            .map(|t| self.lower_type(t))
            .unwrap_or(Ty::Unit);
        if let Some(id) = self.find_decl_by_name(&r.name.name, DeclKind::Reveal) {
            self.table.set_decl(
                id,
                Ty::Fn {
                    params,
                    ret: Box::new(ret),
                },
            );
        }
    }

    fn find_decl_by_name(&self, name: &str, kind: DeclKind) -> Option<DeclId> {
        self.resolved
            .bindings
            .decls
            .iter()
            .find(|d: &&DeclInfo| d.kind == kind && d.name.name.as_ref() == name)
            .map(|d| d.id)
    }

    // ---------------------------------------------------------------
    // AST type lowering
    // ---------------------------------------------------------------

    pub(crate) fn lower_type(&mut self, t: &AstType) -> Ty {
        match t {
            AstType::Amount(_) => Ty::Amount,
            AstType::Time(_) => Ty::Time,
            AstType::Duration(_) => Ty::Duration,
            AstType::Hash(_) => Ty::Hash,
            AstType::Text(_) => Ty::Text,
            AstType::Address(_) => Ty::Address,
            AstType::PqKey(_) => Ty::PqKey,
            AstType::Bytes(_) => Ty::Bytes,
            AstType::Bool(_) => Ty::Bool,
            AstType::Choice(span, values) => {
                let members: Vec<Box<str>> = values.iter().map(|v| v.name.name.clone()).collect();
                let id = self.table.intern_choice(ChoiceTypeInfo {
                    members,
                    is_inline: true,
                    span: *span,
                    name: None,
                });
                Ty::Choice(id)
            }
            AstType::Encrypted(inner, _) => Ty::Ciphertext(Box::new(self.lower_type(inner))),
            AstType::List(inner, _) => Ty::List(Box::new(self.lower_type(inner))),
            AstType::Map(k, v, _) => {
                Ty::Map(Box::new(self.lower_type(k)), Box::new(self.lower_type(v)))
            }
            AstType::PriorityQueue(k, v, ord, _) => {
                let ordering = match ord {
                    covenant_parser::ast::Ordering::Max => crate::ty::Ordering::Max,
                    covenant_parser::ast::Ordering::Min => crate::ty::Ordering::Min,
                };
                Ty::PriorityQueue {
                    key: Box::new(self.lower_type(k)),
                    value: Box::new(self.lower_type(v)),
                    ordering,
                }
            }
            AstType::Shares { n, k, .. } => Ty::Shares {
                n: *n,
                k: *k,
                parties_ty: Box::new(Ty::List(Box::new(Ty::Address))),
            },
            AstType::User(ident) => {
                // Look up via the resolver's binding for this Ident's span.
                match self.resolved.bindings.get(&ident.span) {
                    Some(Binding::Struct(id)) => {
                        let decl_id = *id;
                        match self.table.decl_ty(decl_id) {
                            Ty::Struct(sid) => Ty::Struct(sid),
                            other => other,
                        }
                    }
                    Some(Binding::Credential(id)) => {
                        let decl_id = *id;
                        match self.table.decl_ty(decl_id) {
                            Ty::Credential(cid) => Ty::Credential(cid),
                            other => other,
                        }
                    }
                    // The resolver already reported an unresolved-identifier
                    // diagnostic for these two cases (unknown name, or the
                    // ident was never visited) -- don't double-diagnose.
                    None | Some(Binding::Unresolved) => Ty::Unknown,
                    // OMEGA V6 (HGH-026 fix): the ident resolved fine, but to
                    // something that isn't a type (a field, action, event,
                    // error, local, etc. sharing the flat top-level
                    // namespace). This used to silently fall back to
                    // `Ty::Unknown` with zero diagnostic anywhere in the
                    // pipeline. Refuse instead of guessing.
                    Some(other) => {
                        self.diagnostics.push(diag::not_a_type(
                            ident.span,
                            &ident.name,
                            binding_kind_name(other),
                        ));
                        Ty::Unknown
                    }
                }
            }
        }
    }

    // ---------------------------------------------------------------
    // Decl checking
    // ---------------------------------------------------------------

    fn check_decl(&mut self, d: &TopLevelDecl) {
        match d {
            TopLevelDecl::Field(f) => self.check_field_body(f),
            TopLevelDecl::Struct(s) => {
                for f in &s.fields {
                    if let Some(init) = &f.initializer {
                        let ty = self.lower_type(&f.ty);
                        self.check_expr(init, &ty);
                    }
                }
            }
            TopLevelDecl::Credential(c) => {
                for f in &c.fields {
                    if let Some(init) = &f.initializer {
                        let ty = self.lower_type(&f.ty);
                        self.check_expr(init, &ty);
                    }
                }
            }
            TopLevelDecl::Error(_) | TopLevelDecl::Event(_) | TopLevelDecl::Metadata(_) => {
                // Metadata values are checked leniently: synth the RHS.
                if let TopLevelDecl::Metadata(m) = d {
                    match &m.value {
                        MetadataValue::Literal(e) => {
                            self.synth_expr(e);
                        }
                        MetadataValue::GenesisMint { amount, .. } => {
                            self.check_expr(amount, &Ty::Amount);
                        }
                    }
                }
            }
            TopLevelDecl::Action(a) => self.check_action(a),
            TopLevelDecl::View(v) => self.check_view(v),
            TopLevelDecl::Reveal(r) => self.check_reveal(r),
            TopLevelDecl::SelectiveDisclosure(sd) => {
                for arg in &sd.args {
                    self.register_arg_type(arg);
                }
                self.check_expr(&sd.body, &Ty::Bool);
            }
            TopLevelDecl::Version(v) => {
                for inner in &v.body {
                    self.check_decl(inner);
                }
            }
            TopLevelDecl::Migrate(m) => {
                for s in &m.body {
                    self.check_stmt(s);
                }
            }
            TopLevelDecl::OnDestroy(od) => self.check_on_destroy(od),
        }
    }

    fn check_field_body(&mut self, f: &FieldDecl) {
        let base = self.lower_type(&f.ty);
        let effective = self.apply_construct_privacy(&f.per_field_privacy, base);
        if let Some(init) = &f.initializer {
            self.check_expr(init, &effective);
        }
    }

    fn check_action(&mut self, a: &ActionDecl) {
        self.in_action = true;
        self.current_return_ty = None;
        for arg in &a.args {
            self.register_arg_type(arg);
        }
        for ann in &a.annotations {
            self.check_annotation(ann);
        }
        let has_verified_by = a
            .qualifiers
            .iter()
            .any(|q| matches!(q, ActionQualifier::VerifiedBy(_)));
        let _ = has_verified_by;
        for g in &a.guards {
            self.check_guard(g);
        }
        for q in &a.qualifiers {
            self.check_qualifier(q);
        }
        for s in &a.body {
            self.check_stmt(s);
        }
        self.in_action = false;
    }

    fn check_view(&mut self, v: &ViewDecl) {
        self.in_action = false;
        let ret = v
            .returns
            .as_ref()
            .map(|t| self.lower_type(t))
            .unwrap_or(Ty::Unit);
        self.current_return_ty = Some(ret.clone());
        for arg in &v.args {
            self.register_arg_type(arg);
        }
        for g in &v.guards {
            self.check_guard(g);
        }
        self.check_expr(&v.body, &ret);
    }

    fn check_reveal(&mut self, r: &RevealDecl) {
        self.in_action = false;
        for arg in &r.args {
            self.register_arg_type(arg);
        }
        for g in &r.guards {
            self.check_guard(g);
        }
        if let Some(body) = &r.body {
            let ret = r
                .returns
                .as_ref()
                .map(|t| self.lower_type(t))
                .unwrap_or(Ty::Unit);
            self.current_return_ty = Some(ret.clone());
            self.check_expr(body, &ret);
        } else {
            // One-liner: `reveal <field_name> to <target>`. Resolve the field,
            // strip ciphertext if applicable; warn if the field is already
            // plaintext.
            let field_ty = self.resolve_ident_ty(&r.name);
            match &field_ty {
                Ty::Ciphertext(_inner) => {
                    // Valid: emits a plaintext version.
                }
                Ty::Unknown => {}
                _ => {
                    self.diagnostics.push(diag::warn_reveal_plaintext(r.span));
                }
            }
        }
        if let Some(target) = &r.target {
            self.check_reveal_target(target);
        }
    }

    fn check_on_destroy(&mut self, od: &OnDestroyBlock) {
        self.in_action = true;
        for s in &od.body {
            self.check_stmt(s);
        }
        self.in_action = false;
    }

    /// Resolve the `LocalId` a binding *definition site* introduced.
    ///
    /// `ident_bindings` only holds use-sites: the resolver's `introduce_local`
    /// writes the scope arena and the `locals` vector, never the ident map. So
    /// looking a definition span up in `ident_bindings` can never succeed, and
    /// the local silently keeps its `Ty::Unknown` default, which is compatible
    /// with every type and maps to `PrivacyDomain::Unknown`. That is how a
    /// single `let` used to erase both the type and the privacy domain of the
    /// value it bound. Every definition site must go through this helper.
    ///
    /// The use-site map is still consulted first so that any binding form which
    /// *is* recorded there keeps working unchanged.
    fn local_at_def(&self, span: Span) -> Option<LocalId> {
        if let Some(Binding::Local(id)) = self.resolved.bindings.get(&span) {
            return Some(*id);
        }
        self.resolved
            .bindings
            .locals
            .iter()
            .find(|l| l.def_span == span)
            .map(|l| l.id)
    }

    fn register_arg_type(&mut self, arg: &Arg) {
        let ty = self.lower_type(&arg.ty);
        if let Some(id) = self.local_at_def(arg.name.span) {
            self.table.set_local(id, ty);
        }
    }

    fn check_annotation(&mut self, ann: &Annotation) {
        for arg in &ann.args {
            match arg {
                AnnotationArg::Positional(e) => {
                    self.synth_expr(e);
                }
                AnnotationArg::Named { value, .. } => {
                    self.synth_expr(value);
                }
            }
        }
    }

    fn check_guard(&mut self, g: &covenant_parser::ast::Guard) {
        use covenant_parser::ast::Guard::*;
        match g {
            When(e) => {
                let ty = self.synth_expr(e);
                // Accept Bool or Ciphertext(Bool) or Unknown.
                if !matches!(ty, Ty::Bool | Ty::Unknown)
                    && !matches!(&ty, Ty::Ciphertext(b) if matches!(**b, Ty::Bool))
                {
                    let actual = ty.render(&self.table);
                    self.diagnostics
                        .push(diag::type_mismatch(e.span(), "bool", &actual));
                }
            }
            Given(e) => {
                let _ = self.check_expr(e, &Ty::Bool);
            }
            Only(p) => self.check_principal(p),
        }
    }

    fn check_qualifier(&mut self, q: &ActionQualifier) {
        match q {
            ActionQualifier::VerifiedBy(e) => {
                self.check_expr(e, &Ty::Bytes);
            }
            ActionQualifier::PqSigned { msg, sig, pk, .. } => {
                let msg_ty = self.synth_expr(msg);
                if !matches!(msg_ty, Ty::Bytes | Ty::Hash | Ty::Unknown) {
                    let a = msg_ty.render(&self.table);
                    self.diagnostics
                        .push(diag::type_mismatch(msg.span(), "bytes or hash", &a));
                }
                self.check_expr(sig, &Ty::Bytes);
                self.check_expr(pk, &Ty::PqKey);
            }
            ActionQualifier::VdfLocked { delay, .. } => {
                self.check_expr(delay, &Ty::Duration);
            }
        }
    }

    fn check_principal(&mut self, _p: &Principal) {
        // Principals are resolved by Phase 3; the typechecker has nothing
        // additional to do. Any expression branches (Principal::Address,
        // Principal::Call args) should be synth'd in Pass 3 via resolver-
        // visited walks: we take a best-effort approach and rely on the
        // resolver's traversal having already caught unresolved identifiers.
    }

    fn check_reveal_target(&mut self, t: &RevealTarget) {
        if let RevealTarget::Address(e) = t {
            self.check_expr(e, &Ty::Address);
        }
    }

    // ---------------------------------------------------------------
    // Statements
    // ---------------------------------------------------------------

    fn check_stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Let {
                name, ty, value, ..
            } => {
                let expected = ty.as_ref().map(|t| self.lower_type(t));
                let got = if let Some(ref exp) = expected {
                    self.check_expr(value, exp);
                    exp.clone()
                } else {
                    self.synth_expr(value)
                };
                if let Some(id) = self.local_at_def(name.span) {
                    self.table.set_local(id, got);
                }
            }
            Stmt::Assign {
                target, op, value, ..
            } => {
                let target_ty = self.synth_lvalue(target);
                // Compound ops: check RHS against a compatible type.
                use covenant_parser::ast::AssignOp;
                match op {
                    AssignOp::Eq => {
                        self.check_expr(value, &target_ty);
                    }
                    _ => {
                        // e.g. += : treat LHS op= RHS as LHS = LHS + RHS.
                        let rhs_ty = self.synth_expr(value);
                        let binop = match op {
                            AssignOp::PlusEq => BinaryOp::Add,
                            AssignOp::MinusEq => BinaryOp::Sub,
                            AssignOp::StarEq => BinaryOp::Mul,
                            AssignOp::SlashEq => BinaryOp::Div,
                            AssignOp::PercentEq => BinaryOp::Mod,
                            AssignOp::Eq => unreachable!(),
                        };
                        match dispatch_binary(binop, &target_ty, &rhs_ty) {
                            Some(OpResult {
                                ty,
                                lift_lhs: _,
                                lift_rhs,
                            }) => {
                                if lift_rhs {
                                    self.table.record_lift(LiftMarker {
                                        span: value.span(),
                                        from: rhs_ty.clone(),
                                        to: target_ty.clone(),
                                    });
                                }
                                if !ty.compatible(&target_ty) {
                                    let exp = target_ty.render(&self.table);
                                    let act = ty.render(&self.table);
                                    self.diagnostics.push(diag::type_mismatch(
                                        value.span(),
                                        &exp,
                                        &act,
                                    ));
                                }
                            }
                            None => {
                                let l = target_ty.render(&self.table);
                                let r = rhs_ty.render(&self.table);
                                self.diagnostics.push(diag::op_inapplicable(
                                    value.span(),
                                    binop_name(binop),
                                    &l,
                                    &r,
                                ));
                            }
                        }
                    }
                }
            }
            Stmt::Expr(e, _) => {
                self.synth_expr(e);
            }
            Stmt::Match {
                scrutinee, arms, ..
            } => {
                let scr_ty = self.synth_expr(scrutinee);
                for arm in arms {
                    let MatchPattern::Literal(p) = &arm.pattern;
                    let pat_ty = self.synth_expr(p);
                    if !pat_ty.compatible(&scr_ty) {
                        let exp = scr_ty.render(&self.table);
                        let act = pat_ty.render(&self.table);
                        self.diagnostics
                            .push(diag::match_pattern_mismatch(p.span(), &exp, &act));
                    }
                    match &arm.body {
                        MatchBody::Stmt(inner) => self.check_stmt(inner),
                        MatchBody::Block(ss) => {
                            for s in ss {
                                self.check_stmt(s);
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
                self.check_expr(cond, &Ty::Bool);
                for s in then_block {
                    self.check_stmt(s);
                }
                for (c, b) in else_ifs {
                    self.check_expr(c, &Ty::Bool);
                    for s in b {
                        self.check_stmt(s);
                    }
                }
                if let Some(b) = else_block {
                    for s in b {
                        self.check_stmt(s);
                    }
                }
            }
            Stmt::EncryptedWhen {
                cond,
                then_block,
                otherwise,
                ..
            } => {
                let cond_ty = self.synth_expr(cond);
                if !matches!(cond_ty, Ty::Unknown | Ty::Bool)
                    && !matches!(&cond_ty, Ty::Ciphertext(b) if matches!(**b, Ty::Bool))
                {
                    let a = cond_ty.render(&self.table);
                    self.diagnostics.push(diag::type_mismatch(
                        cond.span(),
                        "bool or ciphertext<bool>",
                        &a,
                    ));
                }
                for s in then_block {
                    self.check_stmt(s);
                }
                if let Some(o) = otherwise {
                    for s in o {
                        self.check_stmt(s);
                    }
                }
            }
            Stmt::ForEach {
                binding,
                iter,
                body,
                span,
            } => {
                let iter_ty = self.synth_expr(iter);
                let elem_ty = match &iter_ty {
                    Ty::List(e) => *e.clone(),
                    Ty::Unknown => Ty::Unknown,
                    other => {
                        let r = other.render(&self.table);
                        self.diagnostics.push(diag::foreach_not_list(*span, &r));
                        Ty::Unknown
                    }
                };
                if let Some(id) = self.local_at_def(binding.span) {
                    self.table.set_local(id, elem_ty);
                }
                for s in body {
                    self.check_stmt(s);
                }
            }
            Stmt::TryCatch {
                body,
                error_binding,
                catch_body,
                ..
            } => {
                for s in body {
                    self.check_stmt(s);
                }
                if let Some(id) = error_binding {
                    if let Some(lid) = self.local_at_def(id.span) {
                        self.table.set_local(lid, Ty::Text);
                    }
                }
                for s in catch_body {
                    self.check_stmt(s);
                }
            }
            Stmt::Emit {
                event, args, span, ..
            } => {
                let sig_opt = match self.resolved.bindings.get(&event.span) {
                    Some(Binding::Event(id)) => self.table.event_signatures.get(id).cloned(),
                    _ => None,
                };
                if let Some(sig) = sig_opt {
                    if sig.len() != args.len() {
                        self.diagnostics.push(diag::emit_arg_mismatch(
                            *span,
                            &event.name,
                            sig.len(),
                            args.len(),
                        ));
                    }
                    for (a, expected) in args.iter().zip(sig.iter()) {
                        self.check_expr(a, expected);
                    }
                } else {
                    // Unresolved event: synth args to fill the table but no check.
                    for a in args {
                        self.synth_expr(a);
                    }
                }
            }
            Stmt::Transfer {
                amount, from, to, ..
            } => {
                self.check_expr(amount, &Ty::Amount);
                if let Some(f) = from {
                    self.check_expr(f, &Ty::Address);
                }
                self.check_expr(to, &Ty::Address);
            }
            Stmt::Append {
                collection,
                struct_lit,
                span,
            } => {
                // Resolve the collection's element struct from its TYPE, not
                // from the shape of its binding. `append votes { ... }` on a
                // record field `votes: [Vote]` binds `votes` to
                // `Binding::Field`, never to `Binding::Struct`, so matching on
                // `Binding::Struct` alone missed every list-of-struct field and
                // fell through to the permissive branch: the element values
                // were synth'd but never checked, which let an `amount` land in
                // an `address` field, let a `ciphertext<T>` land in a plaintext
                // field, and skipped the plaintext-to-ciphertext auto-lift so a
                // field declared `encrypted amount` was stored in the clear.
                // `Ty::Struct` is still accepted because a `board`'s `post`
                // block resolves to the struct type itself rather than to a
                // list field.
                let coll_ty = self.resolve_ident_ty(collection);
                let struct_id_opt = match &coll_ty {
                    Ty::List(elem) => match elem.as_ref() {
                        Ty::Struct(sid) => Some(*sid),
                        _ => None,
                    },
                    Ty::Struct(sid) => Some(*sid),
                    _ => None,
                };
                if let Some(sid) = struct_id_opt {
                    // Check each field assignment against the struct's field types.
                    let fields = self.table.struct_info(sid).cloned();
                    if let Some(sinfo) = fields {
                        for fa in struct_lit {
                            if let Some(sf) =
                                sinfo.fields.iter().find(|f| f.name.name == fa.name.name)
                            {
                                let expected = sf.ty.clone();
                                self.check_expr(&fa.value, &expected);
                            } else {
                                // Not lowered by Phase 6 at all (it builds the
                                // element from the declared field order), so
                                // the value would be silently dropped.
                                let sname = sinfo.name.name.clone();
                                self.diagnostics.push(diag::append_unknown_field(
                                    fa.name.span,
                                    &sname,
                                    &fa.name.name,
                                ));
                                self.synth_expr(&fa.value);
                            }
                        }
                    }
                } else {
                    // `Unknown` (an unresolved or already-diagnosed collection)
                    // and `[unknown]` (the implicit `posts` list of a `board`,
                    // whose element struct is not resolvable here) stay
                    // permissive so this does not cascade. Anything else is a
                    // concrete type that cannot hold a struct literal.
                    let elem_unknown = matches!(&coll_ty, Ty::List(e) if **e == Ty::Unknown);
                    if !matches!(coll_ty, Ty::Unknown) && !elem_unknown {
                        let rendered = coll_ty.render(&self.table);
                        self.diagnostics.push(diag::append_not_list(
                            *span,
                            &format!("`{}` has type `{}`", collection.name, rendered),
                        ));
                    }
                    for fa in struct_lit {
                        self.synth_expr(&fa.value);
                    }
                }
            }
            Stmt::Discard(lv, _) | Stmt::Delete(lv, _) => {
                let _ = self.synth_lvalue(lv);
            }
            Stmt::Return(e, span) => {
                if self.in_action {
                    self.diagnostics.push(diag::return_in_action(*span));
                } else if let Some(ret) = self.current_return_ty.clone() {
                    if let Some(expr) = e {
                        self.check_expr(expr, &ret);
                    }
                }
            }
            Stmt::RevertWith {
                error, args, span, ..
            } => {
                let sig_opt = match self.resolved.bindings.get(&error.span) {
                    Some(Binding::Error(id)) => self.table.error_signatures.get(id).cloned(),
                    _ => None,
                };
                if let Some(sig) = sig_opt {
                    if sig.len() != args.len() {
                        self.diagnostics.push(diag::revert_arg_mismatch(
                            *span,
                            &error.name,
                            sig.len(),
                            args.len(),
                        ));
                    }
                    for (a, expected) in args.iter().zip(sig.iter()) {
                        self.check_expr(a, expected);
                    }
                } else {
                    for a in args {
                        self.synth_expr(a);
                    }
                }
            }
        }
    }

    fn synth_lvalue(&mut self, lv: &LValue) -> Ty {
        match lv {
            LValue::Ident(id) => self.resolve_ident_ty(id),
            LValue::FieldAccess(base, field) => {
                let base_ty = self.synth_lvalue(base);
                self.field_access_ty(&base_ty, field, field.span)
            }
            LValue::Index(base, idx) => {
                let base_ty = self.synth_lvalue(base);
                let _ = self.synth_expr(idx);
                self.indexed_elem_ty(&base_ty, idx.span())
            }
        }
    }

    // ---------------------------------------------------------------
    // Expression synth / check
    // ---------------------------------------------------------------

    pub(crate) fn synth_expr(&mut self, e: &Expr) -> Ty {
        // OMEGA V6 (HGH-029 fix): bound recursion depth so a long chain of
        // nested/binary expressions raises a normal diagnostic instead of
        // overflowing the native stack. `check_expr` always bottoms out
        // through this function, so guarding here covers both entry points.
        self.expr_depth += 1;
        if self.expr_depth > MAX_EXPR_DEPTH {
            if self.expr_depth == MAX_EXPR_DEPTH + 1 {
                self.diagnostics.push(diag::too_deeply_nested(e.span()));
            }
            self.expr_depth -= 1;
            return Ty::Unknown;
        }
        let ty = self.synth_expr_inner(e);
        self.expr_depth -= 1;
        self.table.record_expr(e.span(), ty.clone());
        ty
    }

    fn synth_expr_inner(&mut self, e: &Expr) -> Ty {
        match e {
            Expr::Literal(lit) => self.lit_ty(lit),
            Expr::Ident(id) => self.resolve_ident_ty(id),
            Expr::Binary { op, lhs, rhs, span } => {
                let l = self.synth_expr(lhs);
                let r = self.synth_expr(rhs);
                match dispatch_binary(*op, &l, &r) {
                    Some(OpResult {
                        ty,
                        lift_lhs,
                        lift_rhs,
                    }) => {
                        if lift_lhs {
                            self.table.record_lift(LiftMarker {
                                span: lhs.span(),
                                from: l,
                                to: ty.clone(),
                            });
                        }
                        if lift_rhs {
                            self.table.record_lift(LiftMarker {
                                span: rhs.span(),
                                from: r,
                                to: ty.clone(),
                            });
                        }
                        ty
                    }
                    None => {
                        let a = l.render(&self.table);
                        let b = r.render(&self.table);
                        self.diagnostics.push(diag::op_inapplicable(
                            *span,
                            binop_name(*op),
                            &a,
                            &b,
                        ));
                        Ty::Unknown
                    }
                }
            }
            Expr::Unary { op, operand, span } => {
                let o = self.synth_expr(operand);
                match dispatch_unary(*op, &o) {
                    Some(ty) => ty,
                    None => {
                        let a = o.render(&self.table);
                        self.diagnostics.push(diag::op_inapplicable_unary(
                            *span,
                            unop_name(*op),
                            &a,
                        ));
                        Ty::Unknown
                    }
                }
            }
            Expr::Call {
                callee, args, span, ..
            } => self.synth_call(callee, args, *span),
            Expr::FieldAccess { base, field, .. } => {
                let base_ty = self.synth_expr(base);
                self.field_access_ty(&base_ty, field, field.span)
            }
            Expr::Index {
                base, index, span, ..
            } => {
                let base_ty = self.synth_expr(base);
                let _ = self.synth_expr(index);
                self.indexed_elem_ty(&base_ty, *span)
            }
            Expr::Slice { base, from, to, .. } => {
                let t = self.synth_expr(base);
                self.check_expr(from, &Ty::Amount);
                self.check_expr(to, &Ty::Amount);
                t
            }
            Expr::Array { elements, span } => {
                if elements.is_empty() {
                    self.diagnostics.push(diag::empty_array_no_context(*span));
                    return Ty::Unknown;
                }
                let first_ty = self.synth_expr(&elements[0]);
                for el in &elements[1..] {
                    self.check_expr(el, &first_ty);
                }
                Ty::List(Box::new(first_ty))
            }
            Expr::Paren(inner) => self.synth_expr(inner),
            Expr::If {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                self.check_expr(cond, &Ty::Bool);
                let t = self.synth_expr(then_expr);
                self.check_expr(else_expr, &t);
                t
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                let s = self.synth_expr(scrutinee);
                if arms.is_empty() {
                    return Ty::Unknown;
                }
                for arm in arms {
                    let MatchPattern::Literal(p) = &arm.pattern;
                    let pt = self.synth_expr(p);
                    if !pt.compatible(&s) {
                        let exp = s.render(&self.table);
                        let act = pt.render(&self.table);
                        self.diagnostics
                            .push(diag::match_pattern_mismatch(p.span(), &exp, &act));
                    }
                }
                let first_ty = self.synth_expr(&arms[0].body);
                for arm in &arms[1..] {
                    self.check_expr(&arm.body, &first_ty);
                }
                first_ty
            }
            Expr::EncryptedLit(inner, span) => {
                let t = self.synth_expr(inner);
                if matches!(t, Ty::Ciphertext(_)) {
                    self.diagnostics.push(diag::double_encryption(*span));
                    return Ty::Unknown;
                }
                Ty::Ciphertext(Box::new(t))
            }
            Expr::Lambda { span, .. } => {
                self.diagnostics.push(diag::lambda_needs_context(*span));
                Ty::Unknown
            }
        }
    }

    pub(crate) fn check_expr(&mut self, e: &Expr, expected: &Ty) -> Ty {
        // Special handling for lambdas in check mode.
        if let Expr::Lambda { param, body, span } = e {
            if let Ty::Fn { params, ret } = expected {
                if params.len() == 1 {
                    // A lambda parameter is a definition site too, so it needs
                    // the same `locals` scan as `let` / `for each` / `catch`.
                    if let Some(id) = self.local_at_def(param.span) {
                        self.table.set_local(id, params[0].clone());
                    }
                    let got = self.synth_expr(body);
                    if !got.compatible(ret) {
                        let exp = ret.render(&self.table);
                        let act = got.render(&self.table);
                        self.diagnostics
                            .push(diag::type_mismatch(body.span(), &exp, &act));
                    }
                    let lam_ty = Ty::Fn {
                        params: params.clone(),
                        ret: ret.clone(),
                    };
                    self.table.record_expr(*span, lam_ty.clone());
                    return lam_ty;
                }
            }
            // Fall through: no useful expected type.
            self.diagnostics.push(diag::lambda_needs_context(*span));
            self.table.record_expr(*span, Ty::Unknown);
            return Ty::Unknown;
        }

        // Empty-array check: if expected is List(T), lift to a typed empty list.
        if let Expr::Array { elements, span } = e {
            if elements.is_empty() {
                if let Ty::List(_) = expected {
                    self.table.record_expr(*span, expected.clone());
                    return expected.clone();
                }
            }
            // Array of text literals initializing a List(Choice(cid)):
            // populate the choice members from the literal strings.
            if let Ty::List(inner) = expected {
                if let Ty::Choice(cid) = inner.as_ref() {
                    let all_text = elements
                        .iter()
                        .all(|el| matches!(el, Expr::Literal(LiteralExpr::Text(_, _))));
                    if all_text {
                        let mut members = Vec::new();
                        for el in elements {
                            if let Expr::Literal(LiteralExpr::Text(s, _)) = el {
                                members.push(s.clone());
                            }
                        }
                        if let Some(info) = self.table.choices.get_mut(cid.0 as usize) {
                            info.members = members;
                        }
                        self.table.record_expr(*span, expected.clone());
                        return expected.clone();
                    }
                }
            }
        }

        let got = self.synth_expr(e);
        if got.compatible(expected) {
            return got;
        }

        // Auto-lift plaintext → Ciphertext(T) if expected is ciphertext and got matches inner.
        if let Ty::Ciphertext(inner) = expected {
            if **inner == got {
                self.table.record_lift(LiftMarker {
                    span: e.span(),
                    from: got.clone(),
                    to: expected.clone(),
                });
                return expected.clone();
            }
        }

        let exp = expected.render(&self.table);
        let act = got.render(&self.table);
        self.diagnostics
            .push(diag::type_mismatch(e.span(), &exp, &act));
        Ty::Unknown
    }

    fn lit_ty(&mut self, lit: &LiteralExpr) -> Ty {
        match lit {
            LiteralExpr::Integer(_, _) => Ty::Amount,
            LiteralExpr::Bool(_, _) => Ty::Bool,
            LiteralExpr::Text(_, _) => Ty::Text,
            LiteralExpr::Hex(bytes, _) => match bytes.len() {
                20 => Ty::Address,
                32 => Ty::Hash,
                _ => Ty::Bytes,
            },
            LiteralExpr::Duration(_, _, _) => Ty::Duration,
        }
    }

    fn resolve_ident_ty(&mut self, id: &Ident) -> Ty {
        match self.resolved.bindings.get(&id.span) {
            Some(Binding::Field(d)) => self.table.decl_ty(*d),
            Some(Binding::Local(l)) => self.table.local_ty(*l),
            Some(Binding::Action(d))
            | Some(Binding::View(d))
            | Some(Binding::Reveal(d))
            | Some(Binding::SelectiveDisclosure(d)) => self.table.decl_ty(*d),
            Some(Binding::Event(_)) | Some(Binding::Error(_)) => Ty::Unknown,
            Some(Binding::Struct(d)) => self.table.decl_ty(*d),
            Some(Binding::Credential(d)) => self.table.decl_ty(*d),
            Some(Binding::LangIdent(li)) => lang_ident_ty(*li, self.construct_kind),
            Some(Binding::BuiltinPredicate(bp)) => builtin_predicate_ty(*bp),
            Some(Binding::StdlibModule(m)) => Ty::StdlibModule(module_tag(*m)),
            Some(Binding::StdlibFn(f)) => {
                let sig = stdlib_fn_signature(*f);
                Ty::Fn {
                    params: sig.params,
                    ret: Box::new(sig.ret),
                }
            }
            // V0.9.1: `IFoo.at(addr).method(...)` chains : the type
            // checker treats the interface name as an opaque module-like
            // identifier ; method-level resolution happens at IR build
            // time via the file.external_contracts list. Returning
            // Ty::Unknown here keeps the rest of the type checker
            // permissive on the chain (it will see `.at(addr)` returning
            // Unknown, and `.method(args)` returning Unknown, bytecode
            // is emitted regardless from the IR layer).
            Some(Binding::ExternalContract(_)) => Ty::Unknown,
            Some(Binding::Unresolved) | None => Ty::Unknown,
        }
    }

    fn field_access_ty(&mut self, base_ty: &Ty, field: &Ident, span: Span) -> Ty {
        match base_ty {
            Ty::Unknown => Ty::Unknown,
            Ty::Struct(sid) => {
                if let Some(info) = self.table.struct_info(*sid) {
                    if let Some(f) = info.fields.iter().find(|f| f.name.name == field.name) {
                        return f.ty.clone();
                    }
                }
                let ty_name = base_ty.render(&self.table);
                self.diagnostics
                    .push(diag::not_field_accessible(span, &ty_name, &field.name));
                Ty::Unknown
            }
            Ty::Credential(cid) => {
                if let Some(info) = self.table.credential_info(*cid) {
                    if let Some(f) = info.fields.iter().find(|f| f.name.name == field.name) {
                        return f.ty.clone();
                    }
                }
                let ty_name = base_ty.render(&self.table);
                self.diagnostics
                    .push(diag::not_field_accessible(span, &ty_name, &field.name));
                Ty::Unknown
            }
            Ty::Namespace(Namespace::Msg) => match field.name.as_ref() {
                "sender" => Ty::Address,
                "value" => Ty::Amount,
                "data" => Ty::Bytes,
                _ => {
                    self.diagnostics
                        .push(diag::not_field_accessible(span, "msg", &field.name));
                    Ty::Unknown
                }
            },
            Ty::Namespace(Namespace::Block) => match field.name.as_ref() {
                "timestamp" => Ty::Time,
                "number" => Ty::Amount,
                "hash" => Ty::Hash,
                _ => {
                    self.diagnostics
                        .push(diag::not_field_accessible(span, "block", &field.name));
                    Ty::Unknown
                }
            },
            Ty::List(elem) => match field.name.as_ref() {
                "length" | "len" => Ty::Amount,
                "first" | "last" | "argmax" | "argmin" => (**elem).clone(),
                _ => {
                    let ty_name = base_ty.render(&self.table);
                    self.diagnostics
                        .push(diag::not_field_accessible(span, &ty_name, &field.name));
                    Ty::Unknown
                }
            },
            Ty::Map(k, v) => match field.name.as_ref() {
                "length" | "len" => Ty::Amount,
                "keys" => Ty::List(k.clone()),
                "values" => Ty::List(v.clone()),
                "argmax" | "argmin" => (**k).clone(),
                _ => {
                    let ty_name = base_ty.render(&self.table);
                    self.diagnostics
                        .push(diag::not_field_accessible(span, &ty_name, &field.name));
                    Ty::Unknown
                }
            },
            Ty::PriorityQueue { key, value, .. } => match field.name.as_ref() {
                "length" | "len" => Ty::Amount,
                "top_key" | "top" => (**key).clone(),
                "top_value" => (**value).clone(),
                _ => {
                    let ty_name = base_ty.render(&self.table);
                    self.diagnostics
                        .push(diag::not_field_accessible(span, &ty_name, &field.name));
                    Ty::Unknown
                }
            },
            Ty::Ciphertext(_) => {
                self.diagnostics.push(diag::not_field_accessible(
                    span,
                    "ciphertext<...>",
                    &field.name,
                ));
                Ty::Unknown
            }
            Ty::Text | Ty::Bytes => match field.name.as_ref() {
                "len" | "length" => Ty::Amount,
                _ => {
                    let ty_name = base_ty.render(&self.table);
                    self.diagnostics
                        .push(diag::not_field_accessible(span, &ty_name, &field.name));
                    Ty::Unknown
                }
            },
            _ => {
                let ty_name = base_ty.render(&self.table);
                self.diagnostics
                    .push(diag::not_field_accessible(span, &ty_name, &field.name));
                Ty::Unknown
            }
        }
    }

    fn indexed_elem_ty(&mut self, base: &Ty, span: Span) -> Ty {
        match base {
            Ty::Unknown => Ty::Unknown,
            Ty::List(e) => (**e).clone(),
            Ty::Map(_k, v) => (**v).clone(),
            Ty::PriorityQueue { value, .. } => (**value).clone(),
            Ty::Bytes => Ty::Bytes, // slicing yields bytes; indexing single byte unsupported in V0 typer
            other => {
                let r = other.render(&self.table);
                self.diagnostics.push(diag::not_indexable(span, &r));
                Ty::Unknown
            }
        }
    }

    fn synth_call(&mut self, callee: &Expr, args: &[Expr], span: Span) -> Ty {
        // Stdlib free function call: dispatch via Signature to honor variadic
        // semantics (keccak / encode accept heterogeneous hashable args).
        if let Expr::Ident(id) = callee {
            if let Some(Binding::StdlibFn(f)) = self.resolved.bindings.get(&id.span) {
                let f = *f;
                let sig = stdlib_fn_signature(f);
                return self.check_signature_call(&sig, args, span, Some(id.name.as_ref()));
            }
        }
        // Special-case: `<ExternalContractIdent>.at(addr)`: the address
        // argument must be an `address`. Must be checked before the general
        // FieldAccess dispatch below, since `base` here is a bare
        // `Expr::Ident` whose synthesized type is (permissively) `Unknown`.
        if let Expr::FieldAccess { base, field, .. } = callee {
            if field.name.as_ref() == "at" {
                if let Expr::Ident(iface_id) = base.as_ref() {
                    if matches!(
                        self.resolved.bindings.get(&iface_id.span),
                        Some(Binding::ExternalContract(_))
                    ) {
                        for a in args {
                            self.check_expr(a, &Ty::Address);
                        }
                        return Ty::Unknown;
                    }
                }
            }
        }
        // Special-case: stdlib module method dispatch.
        if let Expr::FieldAccess { base, field, .. } = callee {
            // OMEGA V6 (HGH-027 fix): `IFoo.at(addr).method(args)` used to
            // hit the permissive `Ty::Unknown` fallback below unconditionally
            // -- typos in the method name, wrong arity, and wrong argument
            // types all silently passed type-check, with the mistake only
            // surfacing (if at all) as a runtime revert from a mismatched
            // selector. We know the interface statically (the resolver
            // already bound `IFoo` to `Binding::ExternalContract`), so check
            // the call against its declared signature before falling back.
            // `synth_expr(base)` recurses into `IFoo.at(addr)` (a nested
            // `Expr::Call`), which re-enters this function and hits the
            // `.at(addr)` special case above -- so the address argument gets
            // checked as a side effect of this call, even though its return
            // type (`Ty::Unknown`) isn't otherwise useful here.
            let base_ty = self.synth_expr(base);
            if let Some(iface) = self.external_contract_of(base) {
                return self.check_external_call(&iface, field, args, span);
            }
            if let Ty::StdlibModule(tag) = base_ty {
                let m = module_from_tag(tag);
                if let Some(sig) = stdlib_method_signature(m, &field.name) {
                    return self.check_signature_call(&sig, args, span, Some(&field.name));
                }
                self.diagnostics.push(diag::unknown_method(
                    field.span,
                    &format!("{tag:?}"),
                    &field.name,
                ));
                // Still synth args for completeness.
                for a in args {
                    self.synth_expr(a);
                }
                return Ty::Unknown;
            }
            // Other patterns where base_ty is Unknown (e.g. a field/param
            // typed `Unknown` upstream). Silently return Unknown so the
            // chain compiles rather than cascading unrelated errors.
            if matches!(base_ty, Ty::Unknown) {
                for a in args {
                    self.synth_expr(a);
                }
                return Ty::Unknown;
            }
            // Non-module base: fall through to general dispatch via the method shape.
            // We don't support user-defined methods in V0, so treat as error.
            // Still synth args.
            for a in args {
                self.synth_expr(a);
            }
            let ty_name = base_ty.render(&self.table);
            self.diagnostics.push(diag::not_field_accessible(
                field.span,
                &ty_name,
                &field.name,
            ));
            return Ty::Unknown;
        }

        // Regular callable expression.
        let callee_ty = self.synth_expr(callee);
        match callee_ty {
            Ty::Fn { params, ret } => self.check_call_params(&params, args, span, &ret),
            Ty::Unknown => {
                for a in args {
                    self.synth_expr(a);
                }
                Ty::Unknown
            }
            // StdlibModule shouldn't be directly callable, must go through method access.
            _ => {
                for a in args {
                    self.synth_expr(a);
                }
                // Record a generic "not callable" diag.
                let r = callee_ty.render(&self.table);
                self.diagnostics
                    .push(diag::op_inapplicable(span, "call", &r, ""));
                Ty::Unknown
            }
        }
    }

    fn check_signature_call(
        &mut self,
        sig: &Signature,
        args: &[Expr],
        span: Span,
        fn_name: Option<&str>,
    ) -> Ty {
        let name = fn_name.unwrap_or("<fn>");
        match sig.variadic {
            VariadicKind::Fixed => {
                if args.len() != sig.params.len() {
                    self.diagnostics.push(diag::arity_mismatch(
                        span,
                        name,
                        sig.params.len(),
                        args.len(),
                    ));
                }
                for (i, a) in args.iter().enumerate() {
                    let expected = sig.params.get(i).cloned().unwrap_or(Ty::Unknown);
                    self.check_expr(a, &expected);
                }
            }
            VariadicKind::LastParam => {
                if args.len() < sig.params.len() {
                    self.diagnostics.push(diag::arity_mismatch(
                        span,
                        name,
                        sig.params.len(),
                        args.len(),
                    ));
                }
                let last = sig.params.last().cloned().unwrap_or(Ty::Unknown);
                for (i, a) in args.iter().enumerate() {
                    let expected = sig.params.get(i).cloned().unwrap_or(last.clone());
                    self.check_expr(a, &expected);
                }
            }
            VariadicKind::AnyHashable => {
                // keccak / encode / etc.: accept any args.
                for a in args {
                    self.synth_expr(a);
                }
            }
        }
        sig.ret.clone()
    }

    fn check_call_params(&mut self, params: &[Ty], args: &[Expr], span: Span, ret: &Ty) -> Ty {
        if args.len() != params.len() {
            self.diagnostics
                .push(diag::arity_mismatch(span, "<fn>", params.len(), args.len()));
        }
        for (i, a) in args.iter().enumerate() {
            let expected = params.get(i).cloned().unwrap_or(Ty::Unknown);
            self.check_expr(a, &expected);
        }
        ret.clone()
    }

    /// If `e` is `<Iface>.at(addr)` where `Iface` is bound to a real
    /// `external contract` interface, returns a clone of that interface's
    /// declaration (cloned to avoid holding a borrow of `self.file` across
    /// the `&mut self` calls the caller makes next).
    fn external_contract_of(&self, e: &Expr) -> Option<ExternalContractDecl> {
        let Expr::Call { callee, .. } = e else {
            return None;
        };
        let Expr::FieldAccess { base, field, .. } = callee.as_ref() else {
            return None;
        };
        if field.name.as_ref() != "at" {
            return None;
        }
        let Expr::Ident(iface_id) = base.as_ref() else {
            return None;
        };
        if !matches!(
            self.resolved.bindings.get(&iface_id.span),
            Some(Binding::ExternalContract(_))
        ) {
            return None;
        }
        self.file
            .external_contracts
            .iter()
            .find(|ec| ec.name.name == iface_id.name)
            .cloned()
    }

    /// Type-checks `Iface.at(addr).method(args)` against `method`'s declared
    /// signature in `iface` (arity + per-argument type), returning the
    /// declared return type (`Ty::Unknown` for a void-returning function).
    fn check_external_call(
        &mut self,
        iface: &ExternalContractDecl,
        method: &Ident,
        args: &[Expr],
        span: Span,
    ) -> Ty {
        match iface.functions.iter().find(|f| f.name.name == method.name) {
            Some(f) => {
                if args.len() != f.params.len() {
                    self.diagnostics.push(diag::arity_mismatch(
                        span,
                        &method.name,
                        f.params.len(),
                        args.len(),
                    ));
                }
                for (i, a) in args.iter().enumerate() {
                    let expected = match f.params.get(i) {
                        Some(p) => self.lower_type(&p.ty),
                        None => Ty::Unknown,
                    };
                    self.check_expr(a, &expected);
                }
                match &f.returns {
                    Some(t) => self.lower_type(t),
                    None => Ty::Unknown,
                }
            }
            None => {
                self.diagnostics.push(diag::unknown_method(
                    method.span,
                    &iface.name.name,
                    &method.name,
                ));
                for a in args {
                    self.synth_expr(a);
                }
                Ty::Unknown
            }
        }
    }
}

fn lang_ident_ty(li: LangIdent, construct: ConstructKind) -> Ty {
    match li {
        LangIdent::Caller
        | LangIdent::Deployer
        | LangIdent::This
        | LangIdent::ZeroAddress
        | LangIdent::Owner
        | LangIdent::Party
        | LangIdent::Admin => Ty::Address,
        LangIdent::Now | LangIdent::OpensAt | LangIdent::ClosesAt | LangIdent::OpenedAt => Ty::Time,
        LangIdent::CurrentBlock => Ty::Amount,
        LangIdent::Block => Ty::Namespace(Namespace::Block),
        LangIdent::Msg => Ty::Namespace(Namespace::Msg),
        LangIdent::ThisChain | LangIdent::OtherChain => Ty::Text,
        LangIdent::Phase => Ty::Unknown,
        LangIdent::ProofPayload => Ty::Bytes,
        LangIdent::Parties | LangIdent::Guardians | LangIdent::Holders => {
            Ty::List(Box::new(Ty::Address))
        }
        LangIdent::Tally => {
            // `tally` in a ballot construct is an implicit Map(Choice, Amount).
            // We don't know the specific Choice type here without context, so
            // use `Map(Unknown, Amount)`. Indexing and `.argmax` both widen to
            // `Unknown`, which the caller's `compatible` will accept.
            let _ = construct;
            Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Amount))
        }
        LangIdent::Posts => {
            // Implicit `[post]`. We can't resolve the post struct here without a
            // scan; use `List(Unknown)` as a safe fallback.
            Ty::List(Box::new(Ty::Unknown))
        }
    }
}

fn builtin_predicate_ty(bp: BuiltinPredicate) -> Ty {
    use BuiltinPredicate::*;
    match bp {
        ValidatorMajority => Ty::Fn {
            params: vec![Ty::List(Box::new(Ty::Bytes))],
            ret: Box::new(Ty::Bool),
        },
        _ => Ty::Bool,
    }
}

/// Human-readable description of a `Binding` for "not a type" diagnostics.
fn binding_kind_name(b: &Binding) -> &'static str {
    match b {
        Binding::Field(_) => "a field",
        Binding::Struct(_) => "a struct",
        Binding::Credential(_) => "a credential",
        Binding::Error(_) => "an error",
        Binding::Event(_) => "an event",
        Binding::Action(_) => "an action",
        Binding::View(_) => "a view function",
        Binding::Reveal(_) => "a reveal function",
        Binding::SelectiveDisclosure(_) => "a selective-disclosure function",
        Binding::Local(_) => "a local binding",
        Binding::LangIdent(_) => "a language-provided identifier",
        Binding::BuiltinPredicate(_) => "a builtin predicate",
        Binding::StdlibModule(_) => "a stdlib module",
        Binding::StdlibFn(_) => "a stdlib function",
        Binding::ExternalContract(_) => "an external contract interface",
        Binding::Unresolved => "an unresolved identifier",
    }
}

// Keep these imports reachable for now-rare direct uses in helpers.
#[allow(dead_code)]
fn _pred_link(_bp: BuiltinPredicate, _li: LangIdent, _f: StdlibFn, _m: StdlibModule) {}

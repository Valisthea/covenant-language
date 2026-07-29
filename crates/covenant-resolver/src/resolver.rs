//! Two-pass name resolver.
//!
//! Pass 1 walks the AST top-to-bottom, registering every top-level and
//! nested declaration name into the appropriate scope. Mutual references are
//! supported because Pass 1 completes before Pass 2 begins.
//!
//! Pass 2 walks the AST a second time. For every identifier *use*, inside
//! expressions, statements, guards, principals, annotations, it performs
//! scope-chain lookup and records the result in the `BindingTable`. Unknown
//! names produce E102 with edit-distance suggestions.

use std::collections::HashSet;

use covenant_diag::{Diagnostic, SourceId, Span};
use covenant_parser::ast::{
    ActionDecl, ActionQualifier, AnchorClause, Annotation, AnnotationArg, Arg, ConstructKind,
    CredentialDecl, ErrorDecl, ErrorField, EventDecl, Expr, FieldDecl, File, Ident, Import, LValue,
    LiteralExpr, MatchBody, MetadataValue, MigrateBlock, OnDestroyBlock, Principal, PrincipalKind,
    RevealDecl, RevealTarget, SelectiveDisclosureDecl, Stmt, StructDecl, TextLit, TopLevel,
    TopLevelDecl, Type, UpgradeableClause, VersionBlock, ViewDecl,
};

use crate::binding::{Binding, BuiltinPredicate, DeclKind, LangIdent, LocalKind};
use crate::diag;
use crate::scope::{BehaviorKind, ScopeArena, ScopeId, ScopeKind};
use crate::suggest::suggest;
use crate::table::BindingTable;
use crate::ResolvedFile;

/// Shared state for the two resolution passes over a single `File`.
/// Maximum `resolve_expr` recursion depth. See `covenant_parser::parser::MAX_PARSE_DEPTH`
/// for the empirical basis (OMEGA V6 HGH-029).
const MAX_EXPR_DEPTH: u32 = 96;

pub struct Resolver<'a> {
    #[allow(dead_code)]
    pub(crate) source_id: SourceId,
    pub(crate) arena: ScopeArena,
    pub(crate) table: BindingTable,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) file: &'a File,
    pub(crate) construct_kind: ConstructKind,
    pub(crate) has_owner_field: bool,
    pub(crate) declared_events: HashSet<Box<str>>,
    pub(crate) declared_errors: HashSet<Box<str>>,
    expr_depth: u32,
}

impl<'a> Resolver<'a> {
    pub fn new(file: &'a File, source_id: SourceId) -> Self {
        let span = file.top_level.span;
        let mut arena = ScopeArena::new(span);
        seed_stdlib(&mut arena);
        Self {
            source_id,
            arena,
            table: BindingTable::new(),
            diagnostics: Vec::new(),
            file,
            construct_kind: file.top_level.keyword,
            has_owner_field: false,
            declared_events: HashSet::new(),
            declared_errors: HashSet::new(),
            expr_depth: 0,
        }
    }

    pub fn run(mut self) -> (ResolvedFile, Vec<Diagnostic>) {
        // File scope under stdlib.
        let file_scope = self.arena.push(
            Some(self.arena.stdlib_id),
            ScopeKind::File,
            self.file.top_level.span,
        );

        // Imports (all currently unsupported for user modules; stdlib `using` is absent).
        for imp in &self.file.imports {
            self.process_import(imp);
            let _ = file_scope;
        }

        // Construct scope.
        let construct_scope = self.arena.push(
            Some(file_scope),
            ScopeKind::Construct {
                kind: self.file.top_level.keyword,
            },
            self.file.top_level.span,
        );
        self.seed_construct_lang_idents(construct_scope, self.file.top_level.keyword);

        // V0.9.1: register external contract interfaces in the construct
        // scope BEFORE walking declarations, so any view/action body that
        // references `IFoo.at(addr).method(...)` can resolve `IFoo` to a
        // known binding. The IR builder + codegen already lower these
        // correctly ; this pass closes the previously-orphan resolver gap
        // that blocked M3 (cross-contract Covenant call) milestone.
        self.register_external_contracts(construct_scope);

        // Pass 1: register every declaration name under the construct scope.
        self.pass1_register_decls(construct_scope, &self.file.top_level.body);

        // Pass 2: walk expressions, statements, guards, principals.
        self.pass2_resolve(construct_scope, &self.file.top_level);

        let resolved = ResolvedFile {
            file: self.file.clone(),
            bindings: self.table,
            scopes: self.arena,
        };
        (resolved, self.diagnostics)
    }

    // ---------------------------------------------------------------
    // Pass 1: declaration registration
    // ---------------------------------------------------------------

    fn pass1_register_decls(&mut self, scope: ScopeId, body: &[TopLevelDecl]) {
        for d in body {
            match d {
                TopLevelDecl::Field(f) => self.register_field(scope, f),
                TopLevelDecl::Struct(s) => {
                    self.register_named(scope, &s.name, DeclKind::Struct, Binding::Struct)
                }
                TopLevelDecl::Credential(c) => {
                    self.register_named(scope, &c.name, DeclKind::Credential, Binding::Credential)
                }
                TopLevelDecl::Error(e) => {
                    self.declared_errors.insert(e.name.name.clone());
                    self.register_named(scope, &e.name, DeclKind::Error, Binding::Error);
                }
                TopLevelDecl::Event(e) => {
                    self.declared_events.insert(e.name.name.clone());
                    self.register_named(scope, &e.name, DeclKind::Event, Binding::Event);
                }
                TopLevelDecl::Action(a) => {
                    self.register_named(scope, &a.name, DeclKind::Action, Binding::Action)
                }
                TopLevelDecl::View(v) => {
                    self.register_named(scope, &v.name, DeclKind::View, Binding::View);
                }
                TopLevelDecl::Reveal(r) => {
                    // One-liner reveals (`reveal total to owner`) use the name
                    // as a field reference rather than introducing a new decl.
                    if r.body.is_some() {
                        self.register_named(scope, &r.name, DeclKind::Reveal, Binding::Reveal);
                    }
                }
                TopLevelDecl::SelectiveDisclosure(sd) => {
                    self.register_named(
                        scope,
                        &sd.name,
                        DeclKind::SelectiveDisclosure,
                        Binding::SelectiveDisclosure,
                    );
                }
                TopLevelDecl::Metadata(_) => {
                    // Metadata keys (`symbol`, `decimals`, ...) are not identifiers
                    // that bind new names; they are property assignments validated
                    // later by the type checker.
                }
                TopLevelDecl::Version(v) => {
                    // Register version's own declarations in a nested scope.
                    let version_scope = self.arena.push(
                        Some(scope),
                        ScopeKind::Version { number: v.number },
                        v.span,
                    );
                    self.pass1_register_decls(version_scope, &v.body);
                }
                TopLevelDecl::Migrate(_) | TopLevelDecl::OnDestroy(_) => {
                    // Bodies resolved in Pass 2.
                }
            }
        }
    }

    fn register_field(&mut self, scope: ScopeId, f: &FieldDecl) {
        if f.name.name.as_ref() == "owner" {
            self.has_owner_field = true;
        }
        let name_str = f.name.name.clone();
        if let Some(existing) = self.arena.get(scope).bindings.get(&name_str) {
            if !matches!(existing, Binding::LangIdent(_)) {
                self.diagnostics
                    .push(diag::duplicate_decl(f.name.span, &name_str));
                return;
            }
            // A seeded LangIdent (e.g. `owner`) is shadowed by an explicit field declaration.
        }
        let id = self
            .table
            .register_decl(f.name.clone(), DeclKind::Field, scope);
        self.arena
            .get_mut(scope)
            .bindings
            .insert(name_str, Binding::Field(id));
    }

    /// V0.9.1: register every `external contract IFoo { ... }` declaration's
    /// name in the construct scope so that body expressions like
    /// `IFoo.at(addr).method(args)` resolve `IFoo` to a known binding.
    /// Method-level type checking on the chain is best-effort (the type
    /// checker may or may not enforce signature/return-type matches today;
    /// the IR builder + codegen lower the call regardless).
    fn register_external_contracts(&mut self, scope: ScopeId) {
        let externals = self.file.external_contracts.clone();
        for ec in &externals {
            let key = ec.name.name.clone();
            if self.arena.get(scope).bindings.contains_key(&key) {
                self.diagnostics
                    .push(diag::duplicate_decl(ec.name.span, &key));
                continue;
            }
            let id = self
                .table
                .register_decl(ec.name.clone(), DeclKind::ExternalContract, scope);
            self.arena
                .get_mut(scope)
                .bindings
                .insert(key, Binding::ExternalContract(id));
        }
    }

    fn register_named<F>(&mut self, scope: ScopeId, name: &Ident, kind: DeclKind, bind: F)
    where
        F: FnOnce(crate::binding::DeclId) -> Binding,
    {
        let key = name.name.clone();
        if self.arena.get(scope).bindings.contains_key(&key) {
            self.diagnostics.push(diag::duplicate_decl(name.span, &key));
            return;
        }
        let id = self.table.register_decl(name.clone(), kind, scope);
        self.arena.get_mut(scope).bindings.insert(key, bind(id));
    }

    // ---------------------------------------------------------------
    // Pass 2: expression resolution
    // ---------------------------------------------------------------

    fn pass2_resolve(&mut self, construct_scope: ScopeId, tl: &TopLevel) {
        // Upgradeable clause principal.
        if let Some(up) = &tl.upgradeable {
            self.resolve_upgradeable(construct_scope, up);
        }

        // Anchor clause: pure text literals; nothing to resolve.
        if let Some(a) = &tl.anchor {
            self.touch_anchor(a);
        }

        for d in &tl.body {
            self.resolve_decl(construct_scope, d);
        }
    }

    fn resolve_decl(&mut self, scope: ScopeId, d: &TopLevelDecl) {
        match d {
            TopLevelDecl::Field(f) => self.resolve_field_body(scope, f),
            TopLevelDecl::Struct(s) => self.resolve_struct_body(scope, s),
            TopLevelDecl::Credential(c) => self.resolve_credential_body(scope, c),
            TopLevelDecl::Error(e) => self.resolve_error_decl(scope, e),
            TopLevelDecl::Event(e) => self.resolve_event_decl(scope, e),
            TopLevelDecl::Action(a) => self.resolve_action(scope, a),
            TopLevelDecl::View(v) => self.resolve_view(scope, v),
            TopLevelDecl::Reveal(r) => self.resolve_reveal(scope, r),
            TopLevelDecl::SelectiveDisclosure(sd) => self.resolve_selective_disclosure(scope, sd),
            TopLevelDecl::Metadata(m) => match &m.value {
                MetadataValue::Literal(e) => self.resolve_expr(scope, e),
                MetadataValue::GenesisMint { amount, to } => {
                    self.resolve_expr(scope, amount);
                    self.resolve_principal(scope, to, m.span);
                }
            },
            TopLevelDecl::Version(v) => self.resolve_version(scope, v),
            TopLevelDecl::Migrate(m) => self.resolve_migrate(scope, m),
            TopLevelDecl::OnDestroy(od) => self.resolve_on_destroy(scope, od),
        }
    }

    fn resolve_field_body(&mut self, scope: ScopeId, f: &FieldDecl) {
        self.walk_type(scope, &f.ty);
        if let Some(init) = &f.initializer {
            self.resolve_expr(scope, init);
        }
    }

    fn resolve_struct_body(&mut self, scope: ScopeId, s: &StructDecl) {
        for f in &s.fields {
            self.walk_type(scope, &f.ty);
            if let Some(init) = &f.initializer {
                self.resolve_expr(scope, init);
            }
        }
    }

    fn resolve_credential_body(&mut self, scope: ScopeId, c: &CredentialDecl) {
        for f in &c.fields {
            self.walk_type(scope, &f.ty);
            if let Some(init) = &f.initializer {
                self.resolve_expr(scope, init);
            }
        }
    }

    fn resolve_error_decl(&mut self, scope: ScopeId, e: &ErrorDecl) {
        for f in &e.fields {
            self.walk_error_field(scope, f);
        }
    }

    fn walk_error_field(&mut self, scope: ScopeId, f: &ErrorField) {
        self.walk_type(scope, &f.ty);
    }

    fn resolve_event_decl(&mut self, scope: ScopeId, e: &EventDecl) {
        for a in &e.args {
            self.walk_type(scope, &a.ty);
        }
    }

    fn resolve_action(&mut self, parent: ScopeId, a: &ActionDecl) {
        // Annotations are named `@precompute` etc.; validate against allowlist.
        for ann in &a.annotations {
            self.check_annotation(ann);
            for arg in &ann.args {
                match arg {
                    AnnotationArg::Positional(e) => self.resolve_expr(parent, e),
                    AnnotationArg::Named { value, .. } => self.resolve_expr(parent, value),
                }
            }
        }

        let behavior = self.arena.push(
            Some(parent),
            ScopeKind::Behavior {
                kind: BehaviorKind::Action,
            },
            a.span,
        );
        self.seed_behavior_idents(behavior, parent);
        let has_verified_by = a
            .qualifiers
            .iter()
            .any(|q| matches!(q, ActionQualifier::VerifiedBy(_)));
        if has_verified_by {
            self.insert_binding(
                behavior,
                "proof_payload",
                Binding::LangIdent(LangIdent::ProofPayload),
            );
        }

        self.register_args(behavior, &a.args);
        for arg in &a.args {
            self.walk_type(behavior, &arg.ty);
        }

        // Guards and qualifiers resolve in behavior scope (see args but not body locals).
        for g in &a.guards {
            match g {
                covenant_parser::ast::Guard::When(e) => self.resolve_expr(behavior, e),
                covenant_parser::ast::Guard::Given(e) => self.resolve_expr(behavior, e),
                covenant_parser::ast::Guard::Only(p) => self.resolve_principal(behavior, p, a.span),
            }
        }
        for q in &a.qualifiers {
            match q {
                ActionQualifier::VerifiedBy(e) => self.resolve_expr(behavior, e),
                ActionQualifier::PqSigned { msg, sig, pk, .. } => {
                    self.resolve_expr(behavior, msg);
                    self.resolve_expr(behavior, sig);
                    self.resolve_expr(behavior, pk);
                }
                ActionQualifier::VdfLocked { delay, .. } => self.resolve_expr(behavior, delay),
            }
        }

        self.resolve_stmts(behavior, &a.body);
    }

    fn resolve_view(&mut self, parent: ScopeId, v: &ViewDecl) {
        let behavior = self.arena.push(
            Some(parent),
            ScopeKind::Behavior {
                kind: BehaviorKind::View,
            },
            v.span,
        );
        self.seed_behavior_idents(behavior, parent);
        self.register_args(behavior, &v.args);
        for arg in &v.args {
            self.walk_type(behavior, &arg.ty);
        }
        if let Some(t) = &v.returns {
            self.walk_type(behavior, t);
        }
        for g in &v.guards {
            match g {
                covenant_parser::ast::Guard::When(e) | covenant_parser::ast::Guard::Given(e) => {
                    self.resolve_expr(behavior, e);
                }
                covenant_parser::ast::Guard::Only(p) => self.resolve_principal(behavior, p, v.span),
            }
        }
        self.resolve_expr(behavior, &v.body);
    }

    fn resolve_reveal(&mut self, parent: ScopeId, r: &RevealDecl) {
        let behavior = self.arena.push(
            Some(parent),
            ScopeKind::Behavior {
                kind: BehaviorKind::Reveal,
            },
            r.span,
        );
        self.seed_behavior_idents(behavior, parent);
        self.register_args(behavior, &r.args);
        for arg in &r.args {
            self.walk_type(behavior, &arg.ty);
        }
        if let Some(t) = &r.returns {
            self.walk_type(behavior, t);
        }
        if let Some(target) = &r.target {
            self.resolve_reveal_target(behavior, target, r.span);
        }

        // The reveal "name" in a one-liner (`reveal total to owner`) is a
        // field reference in the parent construct scope.
        if r.body.is_none() {
            self.resolve_ident(parent, &r.name);
        }

        for g in &r.guards {
            match g {
                covenant_parser::ast::Guard::When(e) | covenant_parser::ast::Guard::Given(e) => {
                    self.resolve_expr(behavior, e);
                }
                covenant_parser::ast::Guard::Only(p) => self.resolve_principal(behavior, p, r.span),
            }
        }
        if let Some(body) = &r.body {
            self.resolve_expr(behavior, body);
        }
    }

    fn resolve_selective_disclosure(&mut self, parent: ScopeId, sd: &SelectiveDisclosureDecl) {
        let scope = self.arena.push(
            Some(parent),
            ScopeKind::Behavior {
                kind: BehaviorKind::SelectiveDisclosure,
            },
            sd.span,
        );
        self.register_args(scope, &sd.args);
        for arg in &sd.args {
            self.walk_type(scope, &arg.ty);
        }
        self.resolve_expr(scope, &sd.body);
    }

    fn resolve_version(&mut self, parent: ScopeId, v: &VersionBlock) {
        // Version scope was created in Pass 1; we need to locate it. We stored
        // it only implicitly, so for Pass 2 we create a fresh version scope
        // layered under `parent`: duplication is harmless because Pass 1
        // already filled the declarations there too. Simpler: push a fresh
        // scope and re-run Pass 1 on its body before resolving.
        let vs = self.arena.push(
            Some(parent),
            ScopeKind::Version { number: v.number },
            v.span,
        );
        self.pass1_register_decls(vs, &v.body);
        for d in &v.body {
            self.resolve_decl(vs, d);
        }
    }

    fn resolve_migrate(&mut self, parent: ScopeId, m: &MigrateBlock) {
        let scope = self.arena.push(
            Some(parent),
            ScopeKind::Behavior {
                kind: BehaviorKind::Migrate,
            },
            m.span,
        );
        self.seed_behavior_idents(scope, parent);
        self.resolve_stmts(scope, &m.body);
    }

    fn resolve_on_destroy(&mut self, parent: ScopeId, od: &OnDestroyBlock) {
        let scope = self.arena.push(
            Some(parent),
            ScopeKind::Behavior {
                kind: BehaviorKind::OnDestroy,
            },
            od.span,
        );
        self.seed_behavior_idents(scope, parent);
        self.resolve_stmts(scope, &od.body);
    }

    fn register_args(&mut self, scope: ScopeId, args: &[Arg]) {
        for a in args {
            let name_str = a.name.name.clone();
            if self.arena.get(scope).bindings.contains_key(&name_str) {
                self.diagnostics
                    .push(diag::duplicate_decl(a.name.span, &name_str));
                continue;
            }
            let id = self
                .table
                .register_local(a.name.clone(), LocalKind::Argument, scope);
            self.arena
                .get_mut(scope)
                .bindings
                .insert(name_str, Binding::Local(id));
        }
    }

    fn resolve_stmts(&mut self, scope: ScopeId, stmts: &[Stmt]) {
        for s in stmts {
            self.resolve_stmt(scope, s);
        }
    }

    fn resolve_stmt(&mut self, scope: ScopeId, s: &Stmt) {
        match s {
            Stmt::Let {
                name, ty, value, ..
            } => {
                // Resolve initializer first (in outer scope), then introduce the binding.
                self.resolve_expr(scope, value);
                if let Some(t) = ty {
                    self.walk_type(scope, t);
                }
                self.introduce_local(scope, name, LocalKind::Let);
            }
            Stmt::Assign { target, value, .. } => {
                self.resolve_lvalue(scope, target);
                self.resolve_expr(scope, value);
            }
            Stmt::Expr(e, _) => self.resolve_expr(scope, e),
            Stmt::Match {
                scrutinee, arms, ..
            } => {
                self.resolve_expr(scope, scrutinee);
                for arm in arms {
                    let covenant_parser::ast::MatchPattern::Literal(p) = &arm.pattern;
                    self.resolve_expr(scope, p);
                    let block = self.arena.push(Some(scope), ScopeKind::Block, arm.span);
                    match &arm.body {
                        MatchBody::Stmt(s) => self.resolve_stmt(block, s),
                        MatchBody::Block(ss) => self.resolve_stmts(block, ss),
                    }
                }
            }
            Stmt::If {
                cond,
                then_block,
                else_ifs,
                else_block,
                span,
            } => {
                self.resolve_expr(scope, cond);
                let t = self.arena.push(Some(scope), ScopeKind::Block, *span);
                self.resolve_stmts(t, then_block);
                for (c, b) in else_ifs {
                    self.resolve_expr(scope, c);
                    let s = self.arena.push(Some(scope), ScopeKind::Block, *span);
                    self.resolve_stmts(s, b);
                }
                if let Some(b) = else_block {
                    let s = self.arena.push(Some(scope), ScopeKind::Block, *span);
                    self.resolve_stmts(s, b);
                }
            }
            Stmt::EncryptedWhen {
                cond,
                then_block,
                otherwise,
                span,
            } => {
                self.resolve_expr(scope, cond);
                let t = self.arena.push(Some(scope), ScopeKind::Block, *span);
                self.resolve_stmts(t, then_block);
                if let Some(o) = otherwise {
                    let s = self.arena.push(Some(scope), ScopeKind::Block, *span);
                    self.resolve_stmts(s, o);
                }
            }
            Stmt::ForEach {
                binding,
                iter,
                body,
                span,
            } => {
                self.resolve_expr(scope, iter);
                let block = self.arena.push(Some(scope), ScopeKind::Block, *span);
                self.introduce_local(block, binding, LocalKind::ForEachBinding);
                self.resolve_stmts(block, body);
            }
            Stmt::TryCatch {
                body,
                error_binding,
                catch_body,
                span,
            } => {
                let try_block = self.arena.push(Some(scope), ScopeKind::Block, *span);
                self.resolve_stmts(try_block, body);
                let catch_block = self.arena.push(Some(scope), ScopeKind::Block, *span);
                if let Some(id) = error_binding {
                    self.introduce_local(catch_block, id, LocalKind::TryCatchBinding);
                }
                self.resolve_stmts(catch_block, catch_body);
            }
            Stmt::Emit { event, args, .. } => {
                self.resolve_event(scope, event);
                for a in args {
                    self.resolve_expr(scope, a);
                }
            }
            Stmt::Transfer {
                amount, from, to, ..
            } => {
                self.resolve_expr(scope, amount);
                if let Some(f) = from {
                    self.resolve_expr(scope, f);
                }
                self.resolve_expr(scope, to);
            }
            Stmt::Append {
                collection,
                struct_lit,
                ..
            } => {
                self.resolve_ident(scope, collection);
                for fa in struct_lit {
                    self.resolve_expr(scope, &fa.value);
                }
            }
            Stmt::Discard(lv, _) | Stmt::Delete(lv, _) => {
                self.resolve_lvalue(scope, lv);
            }
            Stmt::Return(e, _) => {
                if let Some(e) = e {
                    self.resolve_expr(scope, e);
                }
            }
            Stmt::RevertWith { error, args, .. } => {
                self.resolve_error_type(scope, error);
                for a in args {
                    self.resolve_expr(scope, a);
                }
            }
        }
    }

    fn resolve_lvalue(&mut self, scope: ScopeId, lv: &LValue) {
        match lv {
            LValue::Ident(i) => self.resolve_ident(scope, i),
            LValue::FieldAccess(base, _field) => self.resolve_lvalue(scope, base),
            LValue::Index(base, idx) => {
                self.resolve_lvalue(scope, base);
                self.resolve_expr(scope, idx);
            }
        }
    }

    fn resolve_expr(&mut self, scope: ScopeId, e: &Expr) {
        // OMEGA V6 (HGH-029 fix): bound the recursion depth so a long chain
        // of nested/binary expressions raises a normal diagnostic instead of
        // overflowing the native stack (see `too_deeply_nested`).
        self.expr_depth += 1;
        if self.expr_depth > MAX_EXPR_DEPTH {
            if self.expr_depth == MAX_EXPR_DEPTH + 1 {
                self.diagnostics.push(diag::too_deeply_nested(e.span()));
            }
            self.expr_depth -= 1;
            return;
        }
        self.resolve_expr_inner(scope, e);
        self.expr_depth -= 1;
    }

    fn resolve_expr_inner(&mut self, scope: ScopeId, e: &Expr) {
        match e {
            Expr::Literal(_) => {}
            Expr::Ident(id) => self.resolve_ident(scope, id),
            Expr::Binary { lhs, rhs, .. } => {
                self.resolve_expr(scope, lhs);
                self.resolve_expr(scope, rhs);
            }
            Expr::Unary { operand, .. } => self.resolve_expr(scope, operand),
            Expr::Call { callee, args, .. } => {
                self.resolve_expr(scope, callee);
                for a in args {
                    self.resolve_expr(scope, a);
                }
            }
            // Field `.name` is NOT resolved here (per spec §Implementation notes).
            Expr::FieldAccess { base, .. } => self.resolve_expr(scope, base),
            Expr::Index { base, index, .. } => {
                self.resolve_expr(scope, base);
                self.resolve_expr(scope, index);
            }
            Expr::Slice { base, from, to, .. } => {
                self.resolve_expr(scope, base);
                self.resolve_expr(scope, from);
                self.resolve_expr(scope, to);
            }
            Expr::Array { elements, .. } => {
                for e in elements {
                    self.resolve_expr(scope, e);
                }
            }
            Expr::Paren(inner) => self.resolve_expr(scope, inner),
            Expr::If {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                self.resolve_expr(scope, cond);
                self.resolve_expr(scope, then_expr);
                self.resolve_expr(scope, else_expr);
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                self.resolve_expr(scope, scrutinee);
                for arm in arms {
                    let covenant_parser::ast::MatchPattern::Literal(p) = &arm.pattern;
                    self.resolve_expr(scope, p);
                    self.resolve_expr(scope, &arm.body);
                }
            }
            Expr::EncryptedLit(inner, _) => self.resolve_expr(scope, inner),
            Expr::Lambda { param, body, span } => {
                let block = self.arena.push(Some(scope), ScopeKind::Block, *span);
                self.introduce_local(block, param, LocalKind::Argument);
                self.resolve_expr(block, body);
            }
        }
    }

    fn resolve_ident(&mut self, scope: ScopeId, id: &Ident) {
        let b = match self.arena.lookup(&id.name, scope).cloned() {
            Some(b) => b,
            None => {
                let names: Vec<Box<str>> = self.arena.visible_names(scope);
                let name_refs: Vec<&str> = names.iter().map(|s| s.as_ref()).collect();
                let sugg = suggest(&id.name, name_refs.iter().copied());
                self.diagnostics
                    .push(diag::unresolved(id.span, &id.name, sugg));
                Binding::Unresolved
            }
        };
        self.table.record(id.span, b);
    }

    // ---------------------------------------------------------------
    // Principals
    // ---------------------------------------------------------------

    fn resolve_principal(&mut self, scope: ScopeId, p: &Principal, at_span: Span) {
        match p {
            Principal::Named(k) => self.resolve_named_principal(scope, *k, at_span),
            Principal::Predicate(id) => self.resolve_predicate(scope, id),
            Principal::Address(e) => self.resolve_expr(scope, e),
            Principal::Call { name, args } => {
                let b = match BuiltinPredicate::from_name(&name.name) {
                    Some(p) => Binding::BuiltinPredicate(p),
                    None => {
                        let names = self.arena.visible_names(scope);
                        let name_refs: Vec<&str> = names.iter().map(|s| s.as_ref()).collect();
                        let sugg = suggest(&name.name, name_refs.iter().copied());
                        self.diagnostics
                            .push(diag::unknown_predicate(name.span, &name.name, sugg));
                        Binding::Unresolved
                    }
                };
                self.table.record(name.span, b);
                for a in args {
                    self.resolve_expr(scope, a);
                }
            }
        }
    }

    fn resolve_named_principal(&mut self, scope: ScopeId, k: PrincipalKind, at_span: Span) {
        let (name, fallback) = match k {
            PrincipalKind::Owner => ("owner", Some(LangIdent::Owner)),
            PrincipalKind::Deployer => ("deployer", Some(LangIdent::Deployer)),
            PrincipalKind::Admin => ("admin", Some(LangIdent::Admin)),
            PrincipalKind::Caller => ("caller", Some(LangIdent::Caller)),
            PrincipalKind::Guardians => ("guardians", Some(LangIdent::Guardians)),
            PrincipalKind::Parties => ("parties", Some(LangIdent::Parties)),
            PrincipalKind::Holders => ("holders", Some(LangIdent::Holders)),
        };
        let binding = match self.arena.lookup(name, scope) {
            Some(b) => b.clone(),
            None => match fallback {
                Some(lang) => Binding::LangIdent(lang),
                None => {
                    self.diagnostics
                        .push(diag::principal_field_missing(at_span, name));
                    Binding::Unresolved
                }
            },
        };
        self.table.record(at_span, binding);
    }

    fn resolve_predicate(&mut self, scope: ScopeId, id: &Ident) {
        let binding = match BuiltinPredicate::from_name(&id.name) {
            Some(p) => Binding::BuiltinPredicate(p),
            None => {
                // Maybe the user wrote an ordinary field/ident, fall back to
                // scope lookup rather than hard-error, to keep advanced fixtures
                // compiling. Unresolved names still produce a diagnostic.
                match self.arena.lookup(&id.name, scope) {
                    Some(b) => b.clone(),
                    None => {
                        let names = self.arena.visible_names(scope);
                        let name_refs: Vec<&str> = names.iter().map(|s| s.as_ref()).collect();
                        let sugg = suggest(&id.name, name_refs.iter().copied());
                        self.diagnostics
                            .push(diag::unknown_predicate(id.span, &id.name, sugg));
                        Binding::Unresolved
                    }
                }
            }
        };
        self.table.record(id.span, binding);
    }

    fn resolve_reveal_target(&mut self, scope: ScopeId, t: &RevealTarget, _at: Span) {
        match t {
            RevealTarget::Owner => {
                let binding = self
                    .arena
                    .lookup("owner", scope)
                    .cloned()
                    .unwrap_or(Binding::LangIdent(LangIdent::Owner));
                self.table.record(_at, binding);
            }
            RevealTarget::Caller => {
                self.table
                    .record(_at, Binding::LangIdent(LangIdent::Caller));
            }
            RevealTarget::Parties => {
                let b = self
                    .arena
                    .lookup("parties", scope)
                    .cloned()
                    .unwrap_or(Binding::LangIdent(LangIdent::Parties));
                self.table.record(_at, b);
            }
            RevealTarget::Address(e) => self.resolve_expr(scope, e),
        }
    }

    // ---------------------------------------------------------------
    // Event / error lookups
    // ---------------------------------------------------------------

    fn resolve_event(&mut self, scope: ScopeId, id: &Ident) {
        match self.arena.lookup(&id.name, scope) {
            // KSR-CVN-008: bind the matched binding directly rather than
            // re-looking-up. Avoids the double-arena-probe and keeps the
            // match arm robust if `arena.lookup` ever becomes side-effecting.
            Some(b @ Binding::Event(_)) => {
                let b = b.clone();
                self.table.record(id.span, b);
            }
            Some(b) => {
                // Something else bound; keep as resolved to avoid cascading,
                // but no special diagnostic.
                let b = b.clone();
                self.table.record(id.span, b);
            }
            None => {
                // Strict about events only for constructs without stdlib-provided
                // event catalogs. See Phase 3 report note on Cast / KeyRegistered.
                if matches!(
                    self.construct_kind,
                    ConstructKind::Record | ConstructKind::Module
                ) {
                    let names: Vec<Box<str>> = self.declared_events.iter().cloned().collect();
                    let name_refs: Vec<&str> = names.iter().map(|s| s.as_ref()).collect();
                    let sugg = suggest(&id.name, name_refs.iter().copied());
                    self.diagnostics
                        .push(diag::unknown_event(id.span, &id.name, sugg));
                }
                self.table.record(id.span, Binding::Unresolved);
            }
        }
    }

    fn resolve_error_type(&mut self, scope: ScopeId, id: &Ident) {
        match self.arena.lookup(&id.name, scope) {
            // KSR-CVN-008: bind the matched binding directly (see `resolve_event`).
            Some(b @ Binding::Error(_)) => {
                let b = b.clone();
                self.table.record(id.span, b);
            }
            Some(b) => {
                let b = b.clone();
                self.table.record(id.span, b);
            }
            None => {
                if matches!(
                    self.construct_kind,
                    ConstructKind::Record | ConstructKind::Module
                ) {
                    let names: Vec<Box<str>> = self.declared_errors.iter().cloned().collect();
                    let name_refs: Vec<&str> = names.iter().map(|s| s.as_ref()).collect();
                    let sugg = suggest(&id.name, name_refs.iter().copied());
                    self.diagnostics
                        .push(diag::unknown_error(id.span, &id.name, sugg));
                }
                self.table.record(id.span, Binding::Unresolved);
            }
        }
    }

    // ---------------------------------------------------------------
    // Annotations
    // ---------------------------------------------------------------

    fn check_annotation(&mut self, ann: &Annotation) {
        const KNOWN: &[&str] = &["precompute", "batch_up_to", "prove_offchain", "gas_budget"];
        if !KNOWN.contains(&ann.name.name.as_ref()) {
            self.diagnostics
                .push(diag::unknown_annotation(ann.name.span, &ann.name.name));
        }
    }

    // ---------------------------------------------------------------
    // Imports
    // ---------------------------------------------------------------

    fn process_import(&mut self, imp: &Import) {
        // User imports are not yet supported. Stdlib `using ModuleName;` would
        // be an import per parser but wasn't emitted in Phase 2 for stdlib
        // modules. Treat all imports as user imports and reject.
        let rendered = imp
            .path
            .iter()
            .map(|p| p.name.as_ref())
            .collect::<Vec<_>>()
            .join(".");
        self.diagnostics
            .push(diag::user_imports_unsupported(imp.span, &rendered));
    }

    // ---------------------------------------------------------------
    // Helpers
    // ---------------------------------------------------------------

    fn introduce_local(&mut self, scope: ScopeId, id: &Ident, kind: LocalKind) {
        let key = id.name.clone();
        // Shadow warning: was there already a binding visible from this scope?
        if let Some(existing) = self.arena.lookup(&key, scope).cloned() {
            match existing {
                Binding::LangIdent(_) => {
                    self.diagnostics.push(diag::shadowing_lang(id.span, &key));
                }
                Binding::Field(_)
                | Binding::Local(_)
                | Binding::Action(_)
                | Binding::View(_)
                | Binding::Reveal(_)
                | Binding::Struct(_)
                | Binding::Credential(_)
                | Binding::Event(_)
                | Binding::Error(_)
                | Binding::StdlibFn(_)
                | Binding::StdlibModule(_) => {
                    self.diagnostics
                        .push(diag::shadowing(id.span, &key, self.file.top_level.span));
                }
                _ => {}
            }
        }
        let local_id = self.table.register_local(id.clone(), kind, scope);
        self.arena
            .get_mut(scope)
            .bindings
            .insert(key, Binding::Local(local_id));
    }

    fn insert_binding(&mut self, scope: ScopeId, name: &str, binding: Binding) {
        self.arena
            .get_mut(scope)
            .bindings
            .insert(name.into(), binding);
    }

    fn walk_type(&mut self, scope: ScopeId, ty: &Type) {
        match ty {
            Type::Amount(_)
            | Type::Time(_)
            | Type::Duration(_)
            | Type::Hash(_)
            | Type::Text(_)
            | Type::Address(_)
            | Type::PqKey(_)
            | Type::Bytes(_)
            | Type::Bool(_)
            | Type::Choice(_, _) => {}
            Type::Encrypted(t, _) | Type::List(t, _) => self.walk_type(scope, t),
            Type::Map(k, v, _) => {
                self.walk_type(scope, k);
                self.walk_type(scope, v);
            }
            Type::PriorityQueue(k, v, _, _) => {
                self.walk_type(scope, k);
                self.walk_type(scope, v);
            }
            Type::Shares { parties, .. } => self.resolve_expr(scope, parties),
            Type::User(id) => {
                // Resolve the user type name in the current scope. If it binds
                // to a struct or credential, record it.
                self.resolve_ident(scope, id);
            }
        }
    }

    fn resolve_upgradeable(&mut self, scope: ScopeId, up: &UpgradeableClause) {
        self.resolve_principal(scope, &up.principal, up.span);
    }

    fn touch_anchor(&mut self, a: &AnchorClause) {
        // Every chain is a text literal; no idents to resolve. Method exists
        // for symmetry / future extensions.
        for _c in &a.chains {
            let _: &TextLit = _c;
        }
    }

    // ---------------------------------------------------------------
    // Seeding
    // ---------------------------------------------------------------

    fn seed_construct_lang_idents(&mut self, scope: ScopeId, kind: ConstructKind) {
        // Always available inside any construct.
        let mut seeds = vec![
            ("deployer", Binding::LangIdent(LangIdent::Deployer)),
            ("this", Binding::LangIdent(LangIdent::This)),
            ("zero_address", Binding::LangIdent(LangIdent::ZeroAddress)),
        ];
        match kind {
            ConstructKind::Bridge => {
                seeds.push(("this_chain", Binding::LangIdent(LangIdent::ThisChain)));
                seeds.push(("other_chain", Binding::LangIdent(LangIdent::OtherChain)));
            }
            ConstructKind::Ceremony => {
                seeds.push(("phase", Binding::LangIdent(LangIdent::Phase)));
                seeds.push(("opens_at", Binding::LangIdent(LangIdent::OpensAt)));
                seeds.push(("closes_at", Binding::LangIdent(LangIdent::ClosesAt)));
            }
            ConstructKind::Ballot => {
                seeds.push(("opens_at", Binding::LangIdent(LangIdent::OpensAt)));
                seeds.push(("closes_at", Binding::LangIdent(LangIdent::ClosesAt)));
                seeds.push(("opened_at", Binding::LangIdent(LangIdent::OpenedAt)));
                seeds.push(("tally", Binding::LangIdent(LangIdent::Tally)));
            }
            ConstructKind::Market | ConstructKind::Vault => {
                seeds.push(("opens_at", Binding::LangIdent(LangIdent::OpensAt)));
                seeds.push(("closes_at", Binding::LangIdent(LangIdent::ClosesAt)));
            }
            ConstructKind::Board => {
                seeds.push(("posts", Binding::LangIdent(LangIdent::Posts)));
            }
            ConstructKind::Counter => {
                // Counters expose `owner` implicitly (the deployer, unless a
                // field overrides). `owner` is seeded below universally.
            }
            _ => {}
        }
        // `owner` is always available as a fallback; a real `owner: address`
        // field registered in Pass 1 will shadow it via scope ordering.
        seeds.push(("owner", Binding::LangIdent(LangIdent::Owner)));

        let s = self.arena.get_mut(scope);
        for (n, b) in seeds {
            s.bindings.entry(n.into()).or_insert(b);
        }
    }

    fn seed_behavior_idents(&mut self, scope: ScopeId, parent: ScopeId) {
        // OMEGA V6 CRT-006 fix: these 5 builtins are seeded into a FRESH
        // CHILD scope (`scope`), disjoint from the map `seed_construct_lang_idents`
        // seeds `owner`/`deployer`/`this`/`zero_address` into (the construct
        // scope, the SAME map fields are registered into -- which is why
        // those 4 names already correctly shadow via `entry().or_insert()`).
        // Because `caller`/`now`/`block`/`msg`/`current_block` lived in a
        // disjoint map, `ScopeArena::lookup`'s innermost-first walk found the
        // seed here before it ever reached a same-named field in `parent`,
        // silently and completely shadowing it with zero diagnostic -- most
        // dangerously for `caller` (msg.sender), the single most
        // safety-critical builtin in the language. Explicitly check `parent`
        // (and its own ancestors) for an existing binding of the same name
        // before seeding, so a real field always wins here too, exactly like
        // the 4 construct-level names already do.
        let seeds = [
            ("caller", Binding::LangIdent(LangIdent::Caller)),
            ("now", Binding::LangIdent(LangIdent::Now)),
            ("current_block", Binding::LangIdent(LangIdent::CurrentBlock)),
            ("block", Binding::LangIdent(LangIdent::Block)),
            ("msg", Binding::LangIdent(LangIdent::Msg)),
        ];
        for (n, b) in seeds {
            if self.arena.lookup(n, parent).is_some() {
                continue;
            }
            let s = self.arena.get_mut(scope);
            s.bindings.entry(n.into()).or_insert(b);
        }
    }
}

/// Seed the stdlib scope with every stdlib module and free function.
fn seed_stdlib(arena: &mut ScopeArena) {
    let stdlib_id = arena.stdlib_id;
    use crate::binding::{StdlibFn::*, StdlibModule::*};
    let stdlib = arena.get_mut(stdlib_id);
    // Modules.
    for m in [
        PQKeys,
        EncryptedTokens,
        FHEVerification,
        Amnesia,
        Math,
        Crypto,
        Encoding,
        Testing,
    ] {
        stdlib
            .bindings
            .insert(m.name().into(), Binding::StdlibModule(m));
    }
    // Free functions.
    for f in [
        Encrypted,
        CiphertextHashOf,
        Keccak,
        Encode,
        Decode,
        Destroy,
        Freeze,
        RandPq,
        Min,
        Max,
        Abs,
        Pow,
        Sqrt,
        DecryptInTest,
        MeasureGas,
        ExpectRevert,
        AsCaller,
        AdvanceTime,
        Deploy,
        RandomAddress,
    ] {
        stdlib
            .bindings
            .insert(f.name().into(), Binding::StdlibFn(f));
    }
}

/// Silence `LiteralExpr` unused-import warning when only reached through `Expr`.
#[allow(dead_code)]
fn _lit_link(_l: LiteralExpr) {}

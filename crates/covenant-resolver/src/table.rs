//! Binding table: maps every `Ident` span in the AST to its resolved `Binding`,
//! plus registration tables for locals and top-level declarations.

use std::collections::HashMap;

use covenant_diag::Span;
use covenant_parser::ast::Ident;

use crate::binding::{Binding, DeclId, DeclKind, LocalId, LocalKind};
use crate::scope::ScopeId;

/// The full binding result of Phase 3. Attached to a `ResolvedFile`.
#[derive(Debug, Clone, Default)]
pub struct BindingTable {
    /// Ident-site span → the binding it resolves to.
    pub ident_bindings: HashMap<Span, Binding>,

    /// Every local binding registered during resolution.
    pub locals: Vec<LocalInfo>,

    /// Every top-level declaration registered during resolution.
    pub decls: Vec<DeclInfo>,
}

impl BindingTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, span: Span, binding: Binding) {
        self.ident_bindings.insert(span, binding);
    }

    pub fn get(&self, span: &Span) -> Option<&Binding> {
        self.ident_bindings.get(span)
    }

    pub fn register_local(&mut self, name: Ident, kind: LocalKind, scope: ScopeId) -> LocalId {
        let id = LocalId(self.locals.len() as u32);
        let info = LocalInfo {
            id,
            def_span: name.span,
            name,
            kind,
            scope,
        };
        self.locals.push(info);
        id
    }

    pub fn register_decl(&mut self, name: Ident, kind: DeclKind, scope: ScopeId) -> DeclId {
        let id = DeclId(self.decls.len() as u32);
        let info = DeclInfo {
            id,
            def_span: name.span,
            name,
            kind,
            scope,
        };
        self.decls.push(info);
        id
    }

    pub fn decl(&self, id: DeclId) -> &DeclInfo {
        &self.decls[id.0 as usize]
    }

    pub fn local(&self, id: LocalId) -> &LocalInfo {
        &self.locals[id.0 as usize]
    }
}

#[derive(Debug, Clone)]
pub struct LocalInfo {
    pub id: LocalId,
    pub name: Ident,
    pub kind: LocalKind,
    pub scope: ScopeId,
    pub def_span: Span,
}

#[derive(Debug, Clone)]
pub struct DeclInfo {
    pub id: DeclId,
    pub name: Ident,
    pub kind: DeclKind,
    pub scope: ScopeId,
    pub def_span: Span,
}

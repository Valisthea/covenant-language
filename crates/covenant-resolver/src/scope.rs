//! Scope tree: arena-allocated scopes threaded via `parent` pointers.
//!
//! A `ScopeArena` owns every `Scope` produced during resolution. Scopes are
//! identified by an opaque `ScopeId`; lookup walks `parent` until it hits the
//! root (the synthetic stdlib scope).

use std::collections::BTreeMap;

use covenant_diag::Span;
use covenant_parser::ast::ConstructKind;

use crate::binding::Binding;

/// Index of a scope within a [`ScopeArena`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScopeId(pub u32);

/// What category of scope this is. Used for diagnostic messages and for rules
/// that depend on the surrounding context (e.g. `destroy()` only valid inside
/// `Behavior { kind: OnDestroy }`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    Stdlib,
    File,
    Construct { kind: ConstructKind },
    Version { number: u32 },
    Behavior { kind: BehaviorKind },
    Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BehaviorKind {
    Action,
    View,
    Reveal,
    OnDestroy,
    Migrate,
    Test,
    SelectiveDisclosure,
}

/// A single lexical scope.
#[derive(Debug, Clone)]
pub struct Scope {
    pub id: ScopeId,
    pub parent: Option<ScopeId>,
    pub kind: ScopeKind,
    pub bindings: BTreeMap<Box<str>, Binding>,
    pub span: Span,
}

/// Arena of scopes. Indexed by `ScopeId`.
#[derive(Debug, Clone)]
pub struct ScopeArena {
    pub scopes: Vec<Scope>,
    pub stdlib_id: ScopeId,
}

impl ScopeArena {
    pub fn new(stdlib_span: Span) -> Self {
        let stdlib_id = ScopeId(0);
        let stdlib = Scope {
            id: stdlib_id,
            parent: None,
            kind: ScopeKind::Stdlib,
            bindings: BTreeMap::new(),
            span: stdlib_span,
        };
        Self {
            scopes: vec![stdlib],
            stdlib_id,
        }
    }

    pub fn push(&mut self, parent: Option<ScopeId>, kind: ScopeKind, span: Span) -> ScopeId {
        let id = ScopeId(self.scopes.len() as u32);
        self.scopes.push(Scope {
            id,
            parent,
            kind,
            bindings: BTreeMap::new(),
            span,
        });
        id
    }

    pub fn get(&self, id: ScopeId) -> &Scope {
        &self.scopes[id.0 as usize]
    }

    pub fn get_mut(&mut self, id: ScopeId) -> &mut Scope {
        &mut self.scopes[id.0 as usize]
    }

    /// Walk the scope chain starting from `from`, returning the first scope
    /// that contains a binding for `name`.
    pub fn lookup(&self, name: &str, from: ScopeId) -> Option<&Binding> {
        let mut cur = Some(from);
        while let Some(id) = cur {
            let s = self.get(id);
            if let Some(b) = s.bindings.get(name) {
                return Some(b);
            }
            cur = s.parent;
        }
        None
    }

    /// Collect every binding name visible from `from` (inner-to-outer). Useful
    /// for edit-distance suggestions and debugging.
    pub fn visible_names(&self, from: ScopeId) -> Vec<Box<str>> {
        let mut seen = std::collections::BTreeSet::new();
        let mut out = Vec::new();
        let mut cur = Some(from);
        while let Some(id) = cur {
            let s = self.get(id);
            for k in s.bindings.keys() {
                if seen.insert(k.clone()) {
                    out.push(k.clone());
                }
            }
            cur = s.parent;
        }
        out
    }

    /// Return the nearest enclosing scope that matches the predicate, walking
    /// outward from `from` (inclusive).
    pub fn find_enclosing(
        &self,
        from: ScopeId,
        pred: impl Fn(&ScopeKind) -> bool,
    ) -> Option<&Scope> {
        let mut cur = Some(from);
        while let Some(id) = cur {
            let s = self.get(id);
            if pred(&s.kind) {
                return Some(s);
            }
            cur = s.parent;
        }
        None
    }
}

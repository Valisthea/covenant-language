//! Privacy domain table.

use std::collections::HashMap;

use covenant_diag::Span;
use covenant_resolver::{DeclId, LocalId};

use crate::domain::PrivacyDomain;

/// Domain annotations produced by the privacy analyzer.
#[derive(Debug, Clone, Default)]
pub struct PrivacyTable {
    pub expr_domains: HashMap<Span, PrivacyDomain>,
    pub local_domains: Vec<PrivacyDomain>,
    pub decl_domains: Vec<PrivacyDomain>,
}

impl PrivacyTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_expr(&mut self, span: Span, d: PrivacyDomain) {
        self.expr_domains.insert(span, d);
    }

    pub fn get_expr(&self, span: &Span) -> PrivacyDomain {
        self.expr_domains
            .get(span)
            .copied()
            .unwrap_or(PrivacyDomain::Unknown)
    }

    pub fn set_local(&mut self, id: LocalId, d: PrivacyDomain) {
        let i = id.0 as usize;
        if self.local_domains.len() <= i {
            self.local_domains.resize(i + 1, PrivacyDomain::Unknown);
        }
        self.local_domains[i] = d;
    }

    pub fn local(&self, id: LocalId) -> PrivacyDomain {
        self.local_domains
            .get(id.0 as usize)
            .copied()
            .unwrap_or(PrivacyDomain::Unknown)
    }

    pub fn set_decl(&mut self, id: DeclId, d: PrivacyDomain) {
        let i = id.0 as usize;
        if self.decl_domains.len() <= i {
            self.decl_domains.resize(i + 1, PrivacyDomain::Unknown);
        }
        self.decl_domains[i] = d;
    }

    pub fn decl(&self, id: DeclId) -> PrivacyDomain {
        self.decl_domains
            .get(id.0 as usize)
            .copied()
            .unwrap_or(PrivacyDomain::Unknown)
    }
}

//! Name resolution: binds identifiers to their definitions in the AST.
//!
//! Phase 3 of the compiler pipeline. The public entry point is [`resolve`],
//! which consumes a parsed `File` and returns a [`ResolvedFile`] containing
//! the original AST plus a [`BindingTable`] mapping every identifier span to
//! the declaration (or language-provided / stdlib binding) it refers to.

mod binding;
mod diag;
mod resolver;
mod scope;
mod suggest;
mod table;

use covenant_diag::{Diagnostic, SourceId};
use covenant_parser::ast::File;

pub use binding::{
    Binding, BuiltinPredicate, DeclId, DeclKind, LangIdent, LocalId, LocalKind, StdlibFn,
    StdlibModule,
};
pub use resolver::Resolver;
pub use scope::{BehaviorKind, Scope, ScopeArena, ScopeId, ScopeKind};
pub use table::{BindingTable, DeclInfo, LocalInfo};

/// The full output of Phase 3: the original AST plus a binding table and the
/// scope arena used to produce it.
#[derive(Debug, Clone)]
pub struct ResolvedFile {
    pub file: File,
    pub bindings: BindingTable,
    pub scopes: ScopeArena,
}

/// Run name resolution over a parsed AST. Always returns a `ResolvedFile` so
/// downstream phases can continue; diagnostics are reported out-of-band.
pub fn resolve(file: File, source_id: SourceId) -> (ResolvedFile, Vec<Diagnostic>) {
    let resolver = Resolver::new(&file, source_id);
    resolver.run()
}

/// Re-exported diagnostic codes (E101-E113, W201/W202, W301).
pub mod codes {
    pub use crate::diag::{
        E101_DUPLICATE_DECL, E102_UNRESOLVED_IDENT, E103_PROOF_PAYLOAD_CONTEXT,
        E104_DESTROY_CONTEXT, E105_PRINCIPAL_FIELD_MISSING, E106_UNKNOWN_PREDICATE,
        E107_USER_IMPORTS, E108_UNKNOWN_EVENT, E109_UNKNOWN_ERROR, E110_UNKNOWN_ANNOTATION,
        E111_PHASE_CONTEXT, E112_THIS_CHAIN_CONTEXT, E113_TOO_DEEPLY_NESTED, W201_SHADOWING,
        W202_SHADOWING_LANG, W301_REDUNDANT_USING,
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke() {}
}

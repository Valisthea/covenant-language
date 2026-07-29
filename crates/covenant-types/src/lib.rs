//! Type checking: infers and verifies types, including the privacy-aware ciphertext<T> domain.
//!
//! Phase 4 of the compiler pipeline. The public entry point is [`typecheck`],
//! which consumes a `ResolvedFile` from `covenant-resolver` and returns a
//! [`TypedFile`] containing the resolved AST plus a [`TypeTable`] mapping every
//! expression span to its inferred or checked [`Ty`].

mod checker;
mod diag;
mod operator;
mod stdlib;
mod table;
mod ty;

use covenant_diag::{Diagnostic, SourceId};
use covenant_resolver::ResolvedFile;

pub use checker::Checker;
pub use table::{ChoiceTypeInfo, CredentialInfo, LiftMarker, StructField, StructInfo, TypeTable};
pub use ty::{ChoiceTypeId, CredentialId, Namespace, Ordering, StdlibModuleTag, StructId, Ty};

/// The full output of Phase 4: the resolved AST plus per-expression types.
#[derive(Debug, Clone)]
pub struct TypedFile {
    pub resolved: ResolvedFile,
    pub types: TypeTable,
}

/// Run the type checker over a resolved AST.
///
/// Always returns a `TypedFile` even if there were errors so downstream phases
/// can continue. Unresolvable expressions get `Ty::Unknown`.
pub fn typecheck(resolved: ResolvedFile, _source_id: SourceId) -> (TypedFile, Vec<Diagnostic>) {
    let checker = Checker::new(resolved);
    checker.run()
}

/// Re-exported diagnostic codes (E201 to E232, E240, W303 to W305).
pub mod codes {
    pub use crate::diag::{
        E201_TYPE_MISMATCH, E202_OP_INAPPLICABLE, E203_UNKNOWN_METHOD, E204_AMBIGUOUS_OVERLOAD,
        E205_ARITY_MISMATCH, E206_NOT_INDEXABLE, E207_NOT_FIELD_ACCESSIBLE,
        E208_LAMBDA_NEEDS_CONTEXT, E209_INVALID_TYPE_ANNOTATION, E210_GENERIC_MISUSE,
        E211_DOUBLE_ENCRYPTION, E212_REVEAL_NOT_CIPHERTEXT, E213_REVERT_ARG_MISMATCH,
        E214_EMIT_ARG_MISMATCH, E215_PATTERN_TYPE_MISMATCH, E216_MATCH_NOT_EXHAUSTIVE,
        E217_OPERATOR_ARITY, E218_TEST_INTRINSIC, E219_REVEAL_TARGET_FIELD, E220_RETURN_IN_ACTION,
        E221_DESTRUCTIBLE_EXPECTED, E222_NOMINAL_MISMATCH, E223_CHOICE_NOT_MEMBER,
        E224_SHARES_MISMATCH, E225_LIFT_BLOCKED, E226_INFERENCE_FAILURE, E227_LAMBDA_MISPLACED,
        E228_FOREACH_NOT_LIST, E229_APPEND_NOT_LIST, E230_EMPTY_ARRAY_NO_CONTEXT, E231_NOT_A_TYPE,
        E232_TOO_DEEPLY_NESTED, E240_APPEND_UNKNOWN_FIELD, W303_TEST_INTRINSIC,
        W304_REVEAL_ON_PLAINTEXT, W305_MATCH_NOT_EXHAUSTIVE,
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke() {}
}

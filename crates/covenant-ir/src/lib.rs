//! Intermediate representation: the typed, privacy-annotated IR used by all backends.
//!
//! Phase 6 of the compiler pipeline. Consumes a `PrivacyCheckedFile` from
//! Phase 5 and emits an `IrModule` — SSA form with block parameters (no
//! explicit phi nodes). The IR is backend-agnostic: Phase 7 optimizes it,
//! Phase 8 (EVM) / Phase 9 (Aster) / Phase 10 (WASM) lower it to bytecode.

pub mod block;
pub mod builder;
pub mod diag;
pub mod function;
pub mod id;
pub mod instr;
pub mod module;
pub mod opcode;
pub mod validate;

use covenant_diag::{Diagnostic, SourceId};
use covenant_privacy::PrivacyCheckedFile;

pub use block::IrBlock;
pub use builder::Builder;
pub use function::{
    IrActionQualifier, IrAnnotation, IrFunction, IrFunctionKind, IrGuard, IrParam, IrPrincipal,
};
pub use id::{BlockId, FunctionId, GlobalId, StructTypeId, Value};
pub use instr::{Instr, InstrMetadata, IrConstant, StdlibCall, Terminator, ValueInfo};
pub use module::{
    AnchorInfo, IrChoice, IrError, IrEvent, IrExternalContract, IrExternalFunc, IrField,
    IrMetadataValue, IrModule, IrStruct, IrStructField, UpgradeableInfo,
};
pub use opcode::Opcode;
pub use validate::validate;

/// Run the IR builder over a privacy-checked file. Always returns a module,
/// even when the diagnostics list is non-empty.
pub fn build_ir(checked: PrivacyCheckedFile, _source_id: SourceId) -> (IrModule, Vec<Diagnostic>) {
    Builder::new(checked).run()
}

pub mod codes {
    pub use crate::diag::{
        E401_SSA_DOMINANCE, E402_BLOCK_NO_TERMINATOR, E403_USER_CALL, E404_LAMBDA_UNSUPPORTED,
        E405_BLOCK_ARG_MISMATCH, E406_UNKNOWN_EVENT, E407_UNKNOWN_ERROR, E408_FOREACH_NOT_LIST,
        E409_UNEXPECTED_STMT, E410_OPCODE_ARITY, E411_CIPHERTEXT_TO_PLAINTEXT,
        E412_UNLOWERABLE_EXPR, E413_UNKNOWN_STRUCT_FIELD, E414_MAP_ON_NON_MAP,
        E415_MISSING_STDLIB_CALL, E416_SELECTIVE_DISCLOSURE_DEFERRED, E417_ONDESTROY_UNKNOWN_FIELD,
        E418_MIGRATE_UNKNOWN_FIELD, E419_FHEBRANCH_PHI_MISSING, E420_FHEBRANCH_NO_MERGE,
        E421_GUARD_UNRESOLVED_PRINCIPAL, E422_SLOT_ANNOTATION_INVALID,
        E423_SLOT_ANNOTATION_CONFLICT, E424_STDLIB_MATH_UNIMPLEMENTED,
        E425_MAP_INTROSPECTION_UNIMPLEMENTED, E426_MEMBERSHIP_IN_UNIMPLEMENTED,
        E427_MAP_ARG_REDUCTION_UNIMPLEMENTED,
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke() {}
}

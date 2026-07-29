//! Optimizer passes: transforms the IR to reduce gas and pGas consumption.
//!
//! Phase 7 of the compiler pipeline. Six passes, each semantics-preserving and
//! privacy-preserving:
//!
//! | Pass | Purpose |
//! |------|---------|
//! | A (`precompute`) | Evaluate `@precompute(expr)` annotations at compile time |
//! | B (`constant_fold`) | Evaluate pure ops on constant operands |
//! | C (`dce`) | Dead code elimination and unreachable-block removal |
//! | D (`cse`) | Common subexpression elimination (block-local, V0) |
//! | E (`sload_coalesce`) | Merge redundant storage reads within a block |
//! | F (`fhe_fold`) | Coalesce trivial FHE encryptions of the same constant |
//!
//! Invariants preserved after every pass:
//! * SSA dominance still holds.
//! * Every block has exactly one terminator.
//! * Types are not changed.
//! * Privacy domains are not changed.
//! * `covenant_ir::validate(&module)` still passes.

mod config;
mod constant_fold;
mod cse;
mod dce;
mod diag;
mod effects;
mod fhe_fold;
mod helpers;
mod manager;
mod precompute;
mod sload_coalesce;

use covenant_diag::Diagnostic;
use covenant_ir::IrModule;

pub use config::OptimizerConfig;
pub use helpers::total_instructions;

/// Run the optimizer. Returns the (possibly mutated) module plus any
/// diagnostics. The module is never dropped; even on errors we return a valid
/// module so downstream phases continue.
pub fn optimize(mut module: IrModule, config: OptimizerConfig) -> (IrModule, Vec<Diagnostic>) {
    let mut diags = Vec::new();
    manager::run(&mut module, &config, &mut diags);
    (module, diags)
}

pub mod codes {
    pub use crate::diag::{
        E401_FIXPOINT, E403_INVARIANT_BROKEN, E404_TYPE_INCONSISTENCY, E405_CSE_TYPE_MISMATCH,
        W401_UNUSED_FIELD, W402_UNUSED_LOCAL, W403_UNREACHABLE, W404_UNUSED_EVENT,
        W405_PRECOMPUTE_RUNTIME, W406_BATCH_NOT_ACTIONABLE, W407_LARGE_SSA,
        W408_MANY_SLOAD_COALESCED, W409_MANY_CSE, W410_LARGE_FOLDING,
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke() {}
}

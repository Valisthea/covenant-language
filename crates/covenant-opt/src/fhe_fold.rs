//! Pass F: Trivial FHE folding.
//!
//! Coalesce multiple `FheEncryptTrivial(C)` instructions that share the same
//! constant operand into a single one. Lifts (`from_lift: true`) are kept
//! distinct from non-lifts to preserve the Phase 4/5 audit trail.

use std::collections::HashMap;

use covenant_ir::{IrFunction, Opcode, Value};

use crate::helpers::{apply_replacements, as_const};

pub fn run_function(func: &mut IrFunction) -> bool {
    let mut replacements: HashMap<Value, Value> = HashMap::new();

    // KSR-CVN-033: key = (constant_fingerprint, from_lift, ciphertext type,
    // privacy domain). Two FheEncryptTrivial whose plaintexts print
    // identically but target distinct FHE parameter sets / privacy domains
    // MUST NOT collapse: that would be a type-confusion bug across keys.
    let mut first: HashMap<(String, bool, String, String), Value> = HashMap::new();

    for block in &func.blocks {
        for instr in &block.instructions {
            if !matches!(instr.opcode, Opcode::FheEncryptTrivial) {
                continue;
            }
            let Some(result) = instr.result else { continue };
            let Some(operand) = instr.operands.first() else {
                continue;
            };
            let Some(c) = as_const(func, operand) else {
                continue;
            };
            let ty_fp = func
                .value_types
                .get(&result)
                .map(|t| format!("{t:?}"))
                .unwrap_or_else(|| "<no-ty>".to_string());
            let priv_fp = func
                .value_privacy
                .get(&result)
                .map(|p| format!("{p:?}"))
                .unwrap_or_else(|| "<no-priv>".to_string());
            let key = (format!("{c:?}"), instr.metadata.from_lift, ty_fp, priv_fp);
            match first.get(&key).copied() {
                Some(existing) if existing != result => {
                    replacements.insert(result, existing);
                }
                Some(_) => {}
                None => {
                    first.insert(key, result);
                }
            }
        }
    }

    apply_replacements(func, &replacements)
}

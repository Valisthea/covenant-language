//! PQ: Post-quantum invariant detectors.
//!
//! C700 `pq_verify_without_nonce` enforces P4 (PQ nonce uniqueness):
//! a `PqVerifyDilithium` / `PqHybridVerify` must be paired with a nullifier
//! `MapHas` *before* the verify and a `MapSet` *after*, both keyed by (or
//! derived from) one of the verify operands (msg / sig / pk).
//!
//! The pre-KSR-CVN-024 implementation accepted *any* `MapHas` or `MapSet`
//! anywhere in the function as proof of nonce handling. This version applies
//! a forward-taint pass over the verify's operand set to prove that the map
//! key actually derives from a verify input, and orders the check vs. the
//! verify position.

use std::collections::HashSet;

use covenant_ir::function::IrFunction;
use covenant_ir::{IrModule, Opcode, Value};

use crate::framework::{Category, Detector, Finding, Severity};

pub struct C700PqVerifyWithoutNonce;

impl Detector for C700PqVerifyWithoutNonce {
    fn code(&self) -> &'static str {
        "C700"
    }
    fn name(&self) -> &'static str {
        "pq_verify_without_nonce"
    }
    fn severity(&self) -> Severity {
        Severity::Critical
    }
    fn category(&self) -> Category {
        Category::PostQuantum
    }
    fn description(&self) -> &'static str {
        "PQ signature verification without a nullifier MapHas/MapSet bracket \
         keyed by the signature, public key, or message hash."
    }

    fn analyze(&self, ir: &IrModule, _source: &str) -> Vec<Finding> {
        let mut findings = Vec::new();
        for func in &ir.functions {
            for (bi, block) in func.blocks.iter().enumerate() {
                for (ii, instr) in block.instructions.iter().enumerate() {
                    if !matches!(
                        instr.opcode,
                        Opcode::PqVerifyDilithium | Opcode::PqHybridVerify
                    ) {
                        continue;
                    }
                    let pivot = (bi, ii);
                    let roots: HashSet<Value> = instr.operands.iter().copied().collect();
                    let tainted = forward_taint(func, &roots);

                    let preceding_check = find_keyed_map_op(
                        func,
                        pivot,
                        Order::Before,
                        |op| matches!(op, Opcode::MapHas),
                        &tainted,
                    );
                    let following_set = find_keyed_map_op(
                        func,
                        pivot,
                        Order::After,
                        |op| matches!(op, Opcode::MapSet),
                        &tainted,
                    );

                    if preceding_check.is_some() && following_set.is_some() {
                        continue;
                    }

                    let missing = match (preceding_check.is_some(), following_set.is_some()) {
                        (false, false) => {
                            "no nullifier `MapHas` precedes the verify and no \
                             `MapSet` keyed on the signature follows it"
                        }
                        (true, false) => {
                            "no `MapSet` keyed on the signature/key/hash follows \
                             a successful verify"
                        }
                        (false, true) => "no nullifier `MapHas` precedes the verify",
                        _ => unreachable!(),
                    };
                    findings.push(
                        Finding::new(
                            "C700",
                            instr.span,
                            format!(
                                "PQ verify without nullifier protection: {missing} \
: replayable signature"
                            ),
                            Severity::Critical,
                        )
                        .with_help(
                            "Bracket PQ verifies with a nullifier map: \
                             `assert(!nullifiers.has(sig))` before the verify, \
                             `nullifiers.set(sig, true)` after.",
                        ),
                    );
                }
            }
        }
        findings
    }
}

/// Forward-taint pass: starting from `roots`, propagate through every
/// instruction whose operand set intersects the tainted set.
fn forward_taint(func: &IrFunction, roots: &HashSet<Value>) -> HashSet<Value> {
    let mut tainted = roots.clone();
    let mut changed = true;
    while changed {
        changed = false;
        for block in &func.blocks {
            for instr in &block.instructions {
                if let Some(res) = instr.result {
                    if !tainted.contains(&res)
                        && instr.operands.iter().any(|op| tainted.contains(op))
                    {
                        tainted.insert(res);
                        changed = true;
                    }
                }
            }
        }
    }
    tainted
}

#[derive(Copy, Clone)]
enum Order {
    Before,
    After,
}

/// Find a Map op (filtered by `op_pred`) whose key (operands[1]) is in the
/// tainted set, and whose linear position satisfies `order` against `pivot`.
fn find_keyed_map_op<F>(
    func: &IrFunction,
    pivot: (usize, usize),
    order: Order,
    op_pred: F,
    tainted: &HashSet<Value>,
) -> Option<(usize, usize)>
where
    F: Fn(&Opcode) -> bool,
{
    for (bi, block) in func.blocks.iter().enumerate() {
        for (ii, instr) in block.instructions.iter().enumerate() {
            let pos = (bi, ii);
            let pos_ok = match order {
                Order::Before => pos < pivot,
                Order::After => pos > pivot,
            };
            if !pos_ok {
                continue;
            }
            if !op_pred(&instr.opcode) {
                continue;
            }
            // Map ops: operands[0] = map handle, operands[1] = key.
            if let Some(&k) = instr.operands.get(1) {
                if tainted.contains(&k) {
                    return Some(pos);
                }
            }
        }
    }
    None
}

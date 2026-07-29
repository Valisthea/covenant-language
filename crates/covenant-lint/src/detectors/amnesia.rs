//! Amnesia ceremony invariant detectors.
//!
//! C801 `ceremony_phase_backward_transition` enforces P5 (phase monotonicity):
//! a write `phase = N` inside an action guarded by `when phase == K` must
//! have N > K. The pre-KSR-CVN-025 implementation only checked that ceremony
//! actions had a `When` guard at all and accepted `action finalize() when
//! phase == 1 { phase = 0 }` as well-formed.
//!
//! The detector is conservative: it only fires when both the guard's expected
//! value and the written value are constant integers, identified by tracing
//! back through the SSA value graph.

use std::collections::{HashMap, HashSet};

use covenant_ir::function::{IrFunction, IrGuard};
use covenant_ir::id::GlobalId;
use covenant_ir::instr::{Instr, IrConstant, ValueInfo};
use covenant_ir::{IrModule, Opcode, Value};

use crate::framework::{Category, Detector, Finding, Severity};
use crate::ir_utils::{all_instrs, is_action};

pub struct C801CeremonyPhaseBackwardTransition;

impl Detector for C801CeremonyPhaseBackwardTransition {
    fn code(&self) -> &'static str {
        "C801"
    }
    fn name(&self) -> &'static str {
        "ceremony_phase_backward_transition"
    }
    fn severity(&self) -> Severity {
        Severity::Critical
    }
    fn category(&self) -> Category {
        Category::Amnesia
    }
    fn description(&self) -> &'static str {
        "Ceremony action gated by `when phase == K` writes `phase = N` with N <= K \
: phase regression breaks the ceremony state machine."
    }

    fn analyze(&self, ir: &IrModule, _source: &str) -> Vec<Finding> {
        let phase_fields: HashSet<GlobalId> = ir
            .fields
            .iter()
            .filter(|f| f.name.name.to_ascii_lowercase().contains("phase"))
            .map(|f| f.id)
            .collect();
        if phase_fields.is_empty() {
            return Vec::new();
        }

        let mut findings = Vec::new();
        for func in &ir.functions {
            if !is_action(func) {
                continue;
            }
            let values: HashMap<Value, ValueInfo> = func.values.iter().cloned().collect();
            let producers = build_producer_map(func);

            // Gather (phase_id -> expected K) from `when phase == K` guards.
            let mut expected: HashMap<GlobalId, u128> = HashMap::new();
            for guard in &func.guards {
                let IrGuard::When(g) = guard else { continue };
                if let Some((field_id, k)) =
                    guard_phase_eq_const(&producers, &values, *g, &phase_fields)
                {
                    // If multiple guards mention the same phase, keep the largest K
                    // (strictest backward bound: write must exceed all of them).
                    expected
                        .entry(field_id)
                        .and_modify(|e| {
                            if k > *e {
                                *e = k;
                            }
                        })
                        .or_insert(k);
                }
            }
            if expected.is_empty() {
                continue;
            }

            for instr in all_instrs(func) {
                let Opcode::SStore(fid) = instr.opcode else {
                    continue;
                };
                if !phase_fields.contains(&fid) {
                    continue;
                }
                let Some(&k) = expected.get(&fid) else {
                    continue;
                };
                let Some(&stored) = instr.operands.first() else {
                    continue;
                };
                let Some(n) = const_int(&values, stored) else {
                    continue;
                };
                if n <= k {
                    findings.push(
                        Finding::new(
                            "C801",
                            instr.span,
                            format!(
                                "ceremony phase regression: action expects \
                                 `phase == {k}` then writes `phase = {n}` (must be strictly greater)"
                            ),
                            Severity::Critical,
                        )
                        .with_help(
                            "Phase transitions must be monotonically increasing. \
                             Re-order the state machine so each action advances \
                             `phase` to a strictly larger value than its guard requires.",
                        ),
                    );
                }
            }
        }
        findings
    }
}

/// Map every SSA result Value to the instruction that produced it.
fn build_producer_map(func: &IrFunction) -> HashMap<Value, Instr> {
    let mut m = HashMap::new();
    for block in &func.blocks {
        for instr in &block.instructions {
            if let Some(v) = instr.result {
                m.insert(v, instr.clone());
            }
        }
    }
    m
}

/// If `guard_val` was produced by `Eq(SLoad(phase_id), const_K)` (in either
/// operand order) and `phase_id` is a known phase field, return `(phase_id, K)`.
fn guard_phase_eq_const(
    producers: &HashMap<Value, Instr>,
    values: &HashMap<Value, ValueInfo>,
    guard_val: Value,
    phase_fields: &HashSet<GlobalId>,
) -> Option<(GlobalId, u128)> {
    let eq_instr = producers.get(&guard_val)?;
    if !matches!(eq_instr.opcode, Opcode::Eq) {
        return None;
    }
    if eq_instr.operands.len() != 2 {
        return None;
    }
    let (a, b) = (eq_instr.operands[0], eq_instr.operands[1]);
    let try_pair = |sload_v: Value, const_v: Value| -> Option<(GlobalId, u128)> {
        let sload = producers.get(&sload_v)?;
        let Opcode::SLoad(fid) = sload.opcode else {
            return None;
        };
        if !phase_fields.contains(&fid) {
            return None;
        }
        let n = const_int(values, const_v)?;
        Some((fid, n))
    };
    try_pair(a, b).or_else(|| try_pair(b, a))
}

/// Extract `u128` literal from a Value if it is `ValueInfo::Const(Integer(_))`.
fn const_int(values: &HashMap<Value, ValueInfo>, v: Value) -> Option<u128> {
    match values.get(&v)? {
        ValueInfo::Const(IrConstant::Integer(n)) => Some(*n),
        _ => None,
    }
}

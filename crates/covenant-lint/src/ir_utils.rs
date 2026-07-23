//! Utility helpers for IR analysis used by detectors.

use std::collections::{HashMap, HashSet};

use covenant_ir::function::IrFunction;
use covenant_ir::instr::Instr;
use covenant_ir::{
    BlockId, IrActionQualifier, IrAnnotation, IrFunctionKind, IrGuard, Opcode, Terminator, Value,
};

/// All BlockIds targeted by a terminator.
pub fn terminator_targets(term: &Terminator) -> Vec<BlockId> {
    match term {
        Terminator::Jump { target, .. } => vec![*target],
        Terminator::Branch {
            then_target,
            else_target,
            ..
        } => {
            vec![*then_target, *else_target]
        }
        Terminator::FheBranch {
            then_target,
            else_target,
            merge_target,
            ..
        } => {
            vec![*then_target, *else_target, *merge_target]
        }
        Terminator::Return(_) | Terminator::Revert { .. } | Terminator::Unreachable => vec![],
    }
}

/// Set of block indices (into `func.blocks`) that lie inside a loop.
///
/// Uses the back-edge heuristic: if block at position `i` has a terminator
/// that targets a block at position `j ≤ i`, blocks `j..=i` are in a loop.
pub fn loop_blocks(func: &IrFunction) -> HashSet<usize> {
    let block_idx: HashMap<BlockId, usize> = func
        .blocks
        .iter()
        .enumerate()
        .map(|(i, b)| (b.id, i))
        .collect();

    let mut in_loop = HashSet::new();
    for (i, block) in func.blocks.iter().enumerate() {
        for target in terminator_targets(&block.terminator) {
            if let Some(&j) = block_idx.get(&target) {
                if j <= i {
                    for k in j..=i {
                        in_loop.insert(k);
                    }
                }
            }
        }
    }
    in_loop
}

/// Flat iterator over all instructions in a function (block by block, in order).
pub fn all_instrs(func: &IrFunction) -> impl Iterator<Item = &Instr> {
    func.blocks.iter().flat_map(|b| b.instructions.iter())
}

pub fn is_action(func: &IrFunction) -> bool {
    matches!(func.kind, IrFunctionKind::Action)
}

pub fn is_view(func: &IrFunction) -> bool {
    matches!(func.kind, IrFunctionKind::View)
}

/// True if any guard in `func` is `Only(principal)`.
pub fn has_only_guard(func: &IrFunction) -> bool {
    func.guards.iter().any(|g| matches!(g, IrGuard::Only(_)))
}

/// True if `func` has an `@name` annotation (matched as `IrAnnotation::Unknown("name")`).
pub fn has_annotation(func: &IrFunction, name: &str) -> bool {
    func.annotations
        .iter()
        .any(|a| matches!(a, IrAnnotation::Unknown(n) if n.as_ref() == name))
}

/// True if `func` has a `@batch_up_to(N)` annotation.
pub fn has_batch_up_to(func: &IrFunction) -> bool {
    func.annotations
        .iter()
        .any(|a| matches!(a, IrAnnotation::BatchUpTo(_)))
}

/// True if `func` has a `vdf_locked(...)` qualifier.
pub fn has_vdf_locked(func: &IrFunction) -> bool {
    func.qualifiers
        .iter()
        .any(|q| matches!(q, IrActionQualifier::VdfLocked(_)))
}

/// Total instruction count across all blocks.
pub fn total_instr_count(func: &IrFunction) -> usize {
    func.blocks.iter().map(|b| b.instructions.len()).sum()
}

/// Build a map from SSA `Value` to the `Opcode` that produced it.
pub fn value_sources(func: &IrFunction) -> HashMap<Value, Opcode> {
    let mut map = HashMap::new();
    for block in &func.blocks {
        for instr in &block.instructions {
            if let Some(v) = instr.result {
                map.insert(v, instr.opcode.clone());
            }
        }
    }
    map
}

/// True if `val` was directly produced by an opcode matching `pred`.
pub fn val_from_opcode<F>(sources: &HashMap<Value, Opcode>, val: Value, pred: &F) -> bool
where
    F: Fn(&Opcode) -> bool,
{
    sources.get(&val).is_some_and(pred)
}

/// True if `val` or any of the operands of the instruction that produced `val`
/// match `pred` (one-level operand chase through the SSA graph).
pub fn val_or_operand_from_opcode<F>(
    func: &IrFunction,
    sources: &HashMap<Value, Opcode>,
    val: Value,
    pred: &F,
) -> bool
where
    F: Fn(&Opcode) -> bool,
{
    if val_from_opcode(sources, val, pred) {
        return true;
    }
    for block in &func.blocks {
        for instr in &block.instructions {
            if instr.result == Some(val) {
                for &operand in &instr.operands {
                    if val_from_opcode(sources, operand, pred) {
                        return true;
                    }
                }
                return false;
            }
        }
    }
    false
}

/// True if `func` performs any collection-iteration opcode (MapKeys, MapValues,
/// ListGet over dynamic list, etc.) — a proxy for "this function has a loop".
pub fn has_collection_iter(func: &IrFunction) -> bool {
    all_instrs(func).any(|i| {
        matches!(
            i.opcode,
            Opcode::MapValues
                | Opcode::MapKeys
                | Opcode::ListSlice
                | Opcode::ListArgMax
                | Opcode::ListArgMin
                | Opcode::PqPop
                | Opcode::MapLength
                | Opcode::ListLength
        )
    })
}

/// Extract the `to` address value from a Transfer instruction (last operand).
pub fn transfer_to(instr: &Instr) -> Option<Value> {
    match instr.operands.len() {
        2 => Some(instr.operands[1]),
        3 => Some(instr.operands[2]),
        _ => None,
    }
}

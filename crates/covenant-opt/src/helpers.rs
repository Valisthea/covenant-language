//! Shared helpers used across optimizer passes.

use std::collections::HashMap;

use covenant_ir::{
    instr::{IrConstant, Terminator, ValueInfo},
    IrFunction, IrModule, Value,
};

/// Apply a `Value → Value` replacement map across every operand and terminator
/// use-site in the function. Returns `true` if any substitution happened.
pub fn apply_replacements(func: &mut IrFunction, map: &HashMap<Value, Value>) -> bool {
    if map.is_empty() {
        return false;
    }
    let mut changed = false;

    for block in &mut func.blocks {
        // Block parameters are definitions, never rewritten by replacement maps.
        // Instruction operands are uses.
        for instr in &mut block.instructions {
            for op in &mut instr.operands {
                if let Some(&new) = map.get(op) {
                    *op = new;
                    changed = true;
                }
            }
        }
        // Terminator operands.
        match &mut block.terminator {
            Terminator::Jump { args, .. } => {
                for v in args {
                    if let Some(&n) = map.get(v) {
                        *v = n;
                        changed = true;
                    }
                }
            }
            Terminator::Branch {
                cond,
                then_args,
                else_args,
                ..
            } => {
                if let Some(&n) = map.get(cond) {
                    *cond = n;
                    changed = true;
                }
                for v in then_args {
                    if let Some(&n) = map.get(v) {
                        *v = n;
                        changed = true;
                    }
                }
                for v in else_args {
                    if let Some(&n) = map.get(v) {
                        *v = n;
                        changed = true;
                    }
                }
            }
            Terminator::FheBranch {
                cond,
                then_args,
                else_args,
                ..
            } => {
                if let Some(&n) = map.get(cond) {
                    *cond = n;
                    changed = true;
                }
                for v in then_args {
                    if let Some(&n) = map.get(v) {
                        *v = n;
                        changed = true;
                    }
                }
                for v in else_args {
                    if let Some(&n) = map.get(v) {
                        *v = n;
                        changed = true;
                    }
                }
            }
            Terminator::Return(Some(v)) => {
                if let Some(&n) = map.get(v) {
                    *v = n;
                    changed = true;
                }
            }
            Terminator::Revert { args, .. } => {
                for v in args {
                    if let Some(&n) = map.get(v) {
                        *v = n;
                        changed = true;
                    }
                }
            }
            _ => {}
        }
    }
    changed
}

/// Look up a value's `ValueInfo` in the function's value registry.
pub fn value_info<'a>(func: &'a IrFunction, v: &Value) -> Option<&'a ValueInfo> {
    func.values.iter().find(|(k, _)| k == v).map(|(_, i)| i)
}

/// Return `Some(constant)` when `v` is a `ValueInfo::Const`.
pub fn as_const<'a>(func: &'a IrFunction, v: &Value) -> Option<&'a IrConstant> {
    match value_info(func, v)? {
        ValueInfo::Const(c) => Some(c),
        _ => None,
    }
}

/// Allocate a fresh SSA value in the function's registry and record its
/// `ValueInfo`. Returns the new `Value`.
pub fn fresh_const(
    func: &mut IrFunction,
    c: IrConstant,
    ty: covenant_types::Ty,
    privacy: covenant_privacy::PrivacyDomain,
    span: covenant_diag::Span,
) -> Value {
    let max = func.values.iter().map(|(v, _)| v.0).max().unwrap_or(0);
    let v = Value(max + 1);
    func.values.push((v, ValueInfo::Const(c)));
    func.value_types.insert(v, ty);
    func.value_privacy.insert(v, privacy);
    func.value_spans.insert(v, span);
    v
}

/// Total instruction count across all functions in a module, a cheap proxy
/// for "IR size".
pub fn total_instructions(module: &IrModule) -> usize {
    module
        .functions
        .iter()
        .flat_map(|f| f.blocks.iter())
        .map(|b| b.instructions.len())
        .sum()
}

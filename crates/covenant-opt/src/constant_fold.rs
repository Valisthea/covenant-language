//! Pass B: constant folding.
//!
//! Evaluates pure ops on constant operands at compile time. Creates a fresh
//! `Const` value for the folded result and redirects uses via a replacement
//! map. DCE (Pass C) cleans up the now-dead instructions.

use std::collections::HashMap;

use covenant_ir::{instr::IrConstant, IrFunction, Opcode, Value};

use crate::helpers::{apply_replacements, as_const, fresh_const};

pub fn run_function(func: &mut IrFunction) -> bool {
    let mut replacements: HashMap<Value, Value> = HashMap::new();

    // Gather the list of (block_idx, instr_idx, result_value, opcode, operand_consts)
    // upfront to avoid borrow issues, then process sequentially.
    let mut folds: Vec<(
        Value,
        IrConstant,
        covenant_types::Ty,
        covenant_privacy::PrivacyDomain,
        covenant_diag::Span,
    )> = Vec::new();

    for block in &func.blocks {
        for instr in &block.instructions {
            let Some(result) = instr.result else {
                continue;
            };
            // Skip if metadata marks this as a lift (preserve lift traceability).
            if instr.metadata.from_lift {
                continue;
            }
            let Some(folded) = try_fold(func, &instr.opcode, &instr.operands) else {
                continue;
            };
            let ty = func
                .value_types
                .get(&result)
                .cloned()
                .unwrap_or(covenant_types::Ty::Unknown);
            let dom = func
                .value_privacy
                .get(&result)
                .copied()
                .unwrap_or(covenant_privacy::PrivacyDomain::Plaintext);
            let span =
                func.value_spans.get(&result).copied().unwrap_or_else(|| {
                    covenant_diag::Span::new(covenant_diag::SourceId::new(0), 0, 0)
                });
            folds.push((result, folded, ty, dom, span));
        }
    }

    for (result, folded, ty, dom, span) in folds {
        let new_v = fresh_const(func, folded, ty, dom, span);
        replacements.insert(result, new_v);
    }

    apply_replacements(func, &replacements)
}

fn try_fold(func: &IrFunction, opcode: &Opcode, operands: &[Value]) -> Option<IrConstant> {
    // Only fold when every operand is a constant.
    let consts: Vec<&IrConstant> = operands
        .iter()
        .map(|v| as_const(func, v))
        .collect::<Option<Vec<_>>>()?;

    match opcode {
        Opcode::Add | Opcode::AddChecked => int2(consts, |a, b| a.checked_add(b)),
        Opcode::Sub | Opcode::SubChecked => int2(consts, |a, b| a.checked_sub(b)),
        Opcode::Mul | Opcode::MulChecked => int2(consts, |a, b| a.checked_mul(b)),
        Opcode::Div => int2(consts, |a, b| a.checked_div(b)),
        Opcode::Mod => int2(consts, |a, b| a.checked_rem(b)),
        Opcode::BitAnd => int2(consts, |a, b| Some(a & b)),
        Opcode::BitOr => int2(consts, |a, b| Some(a | b)),
        Opcode::BitXor => int2(consts, |a, b| Some(a ^ b)),
        Opcode::ShiftLeft => int2(consts, |a, b| if b >= 128 { None } else { Some(a << b) }),
        Opcode::ShiftRight => int2(consts, |a, b| if b >= 128 { None } else { Some(a >> b) }),

        Opcode::Eq => cmp2(consts, |a, b| a == b),
        Opcode::Ne => cmp2(consts, |a, b| a != b),
        Opcode::Lt => cmp2(consts, |a, b| a < b),
        Opcode::Le => cmp2(consts, |a, b| a <= b),
        Opcode::Gt => cmp2(consts, |a, b| a > b),
        Opcode::Ge => cmp2(consts, |a, b| a >= b),

        Opcode::LogicalAnd => bool2(consts, |a, b| a && b),
        Opcode::LogicalOr => bool2(consts, |a, b| a || b),
        Opcode::LogicalNot => {
            if let [IrConstant::Bool(b)] = consts.as_slice() {
                return Some(IrConstant::Bool(!*b));
            }
            None
        }

        Opcode::TextConcat => {
            if let [IrConstant::Text(a), IrConstant::Text(b)] = consts.as_slice() {
                let mut s = String::with_capacity(a.len() + b.len());
                s.push_str(a);
                s.push_str(b);
                return Some(IrConstant::Text(s.into_boxed_str()));
            }
            None
        }
        Opcode::BytesConcat => {
            if let [IrConstant::Hex(a), IrConstant::Hex(b)] = consts.as_slice() {
                let mut v = Vec::with_capacity(a.len() + b.len());
                v.extend_from_slice(a);
                v.extend_from_slice(b);
                return Some(IrConstant::Hex(v.into_boxed_slice()));
            }
            None
        }

        _ => None,
    }
}

fn int2<F>(consts: Vec<&IrConstant>, op: F) -> Option<IrConstant>
where
    F: Fn(u128, u128) -> Option<u128>,
{
    if let [IrConstant::Integer(a), IrConstant::Integer(b)] = consts.as_slice() {
        let r = op(*a, *b)?;
        return Some(IrConstant::Integer(r));
    }
    None
}

fn cmp2<F>(consts: Vec<&IrConstant>, op: F) -> Option<IrConstant>
where
    F: Fn(u128, u128) -> bool,
{
    if let [IrConstant::Integer(a), IrConstant::Integer(b)] = consts.as_slice() {
        return Some(IrConstant::Bool(op(*a, *b)));
    }
    None
}

fn bool2<F>(consts: Vec<&IrConstant>, op: F) -> Option<IrConstant>
where
    F: Fn(bool, bool) -> bool,
{
    if let [IrConstant::Bool(a), IrConstant::Bool(b)] = consts.as_slice() {
        return Some(IrConstant::Bool(op(*a, *b)));
    }
    None
}

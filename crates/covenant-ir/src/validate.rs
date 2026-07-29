//! Structural validator for an `IrModule`.
//!
//! Checks:
//! * every block has exactly one terminator (true by construction, but we
//!   explicitly reject `Terminator::Unreachable` as a bug unless the block is
//!   known to be unreachable: which we can't prove without a CFG pass, so we
//!   accept it as "intentional" placeholder);
//! * every function has a valid entry block;
//! * every Jump/Branch/FheBranch target exists;
//! * block-argument count matches target's parameter count;
//! * opcode operand counts match `Opcode::operand_count()`.

use std::collections::HashSet;

use covenant_diag::Diagnostic;

use crate::diag;
use crate::function::IrFunction;
use crate::instr::Terminator;
use crate::module::IrModule;

/// Run structural validation. Returns a list of diagnostics; empty means OK.
pub fn validate(module: &IrModule) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for f in &module.functions {
        validate_function(f, &mut out);
    }
    out
}

fn validate_function(f: &IrFunction, out: &mut Vec<Diagnostic>) {
    let block_ids: HashSet<u32> = f.blocks.iter().map(|b| b.id.0).collect();

    // Entry block exists.
    if !block_ids.contains(&f.entry.0) {
        out.push(diag::block_no_terminator(f.span));
    }

    for block in &f.blocks {
        // Opcode arity.
        for instr in &block.instructions {
            if let Some((lo, hi)) = instr.opcode.operand_count() {
                let n = instr.operands.len();
                let ok = n >= lo && hi.is_none_or(|h| n <= h);
                if !ok {
                    let name = format!("{:?}", instr.opcode);
                    out.push(diag::opcode_arity(instr.span, &name, lo, n));
                }
            }
        }
        // Terminator target validity.
        match &block.terminator {
            Terminator::Jump { target, args } => {
                validate_target(f, *target, args.len(), &block_ids, out, block.span);
            }
            Terminator::Branch {
                then_target,
                then_args,
                else_target,
                else_args,
                ..
            } => {
                validate_target(
                    f,
                    *then_target,
                    then_args.len(),
                    &block_ids,
                    out,
                    block.span,
                );
                validate_target(
                    f,
                    *else_target,
                    else_args.len(),
                    &block_ids,
                    out,
                    block.span,
                );
            }
            Terminator::FheBranch {
                then_target,
                then_args,
                else_target,
                else_args,
                merge_target,
                ..
            } => {
                validate_target(
                    f,
                    *then_target,
                    then_args.len(),
                    &block_ids,
                    out,
                    block.span,
                );
                validate_target(
                    f,
                    *else_target,
                    else_args.len(),
                    &block_ids,
                    out,
                    block.span,
                );
                if !block_ids.contains(&merge_target.0) {
                    out.push(covenant_diag::Diagnostic::error(
                        diag::E420_FHEBRANCH_NO_MERGE,
                        "FheBranch refers to an unknown merge block",
                        block.span,
                    ));
                }
            }
            Terminator::Return(_) | Terminator::Unreachable => {}
            Terminator::Revert { error, .. } => {
                // Caller can cross-check against the module's declared errors;
                // for now we accept any ident and let backend validate.
                let _ = error;
            }
        }
    }
}

fn validate_target(
    f: &IrFunction,
    target: crate::id::BlockId,
    arg_count: usize,
    block_ids: &HashSet<u32>,
    out: &mut Vec<Diagnostic>,
    span: covenant_diag::Span,
) {
    if !block_ids.contains(&target.0) {
        out.push(diag::block_arg_mismatch(span, 0, arg_count));
        return;
    }
    let target_block = &f.blocks[target.0 as usize];
    let expected = target_block.params.len();
    if expected != arg_count {
        out.push(diag::block_arg_mismatch(span, expected, arg_count));
    }
}

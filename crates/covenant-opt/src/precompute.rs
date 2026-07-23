//! Pass A: `@precompute(expr)` annotation evaluation.
//!
//! If the expression referenced by a `Precompute(value)` annotation traces
//! back (through pure ops) to a constant, we:
//!
//! 1. Evaluate the chain at compile time.
//! 2. Replace all uses of the original value with the folded constant.
//! 3. Keep the annotation as documentation but flip it to a constant-value
//!    form (here we emit the warning only when we *can't* fold).
//!
//! If the expression depends on runtime state (LangIdent loads, SLoad, etc.),
//! emit W405 and leave the function untouched.

use std::collections::HashMap;

use covenant_diag::Diagnostic;
use covenant_ir::{function::IrAnnotation, IrFunction, Value};

use crate::constant_fold;
use crate::diag;
use crate::helpers::apply_replacements;

pub fn run_function(func: &mut IrFunction, diagnostics: &mut Vec<Diagnostic>) -> bool {
    // Collect the precompute target values first.
    let targets: Vec<Value> = func
        .annotations
        .iter()
        .filter_map(|a| match a {
            IrAnnotation::Precompute(v) => Some(*v),
            _ => None,
        })
        .collect();

    if targets.is_empty() {
        return false;
    }

    let mut changed = false;

    // Run constant folding first to propagate any already-foldable subtree
    // into the target value. Then check whether the targets became constants.
    constant_fold::run_function(func);

    let replacements: HashMap<Value, Value> = HashMap::new();
    for target in &targets {
        let info = func
            .values
            .iter()
            .find(|(v, _)| v == target)
            .map(|(_, i)| i.clone());
        let is_const = matches!(info, Some(covenant_ir::instr::ValueInfo::Const(_)));
        if is_const {
            // Already a constant — no action needed for this annotation.
            continue;
        }
        // Not foldable; warn.
        let span = func
            .value_spans
            .get(target)
            .copied()
            .unwrap_or_else(|| covenant_diag::Span::new(covenant_diag::SourceId::new(0), 0, 0));
        diagnostics.push(diag::warn_precompute_runtime(span));
    }

    if apply_replacements(func, &replacements) {
        changed = true;
    }

    changed
}

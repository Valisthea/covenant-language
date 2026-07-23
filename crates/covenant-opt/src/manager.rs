//! Pass manager: runs passes in a fixed order to fixpoint, optionally
//! validating IR well-formedness after each pass.

use covenant_diag::{Diagnostic, Span};
use covenant_ir::IrModule;

use crate::config::OptimizerConfig;
use crate::diag;
use crate::{constant_fold, cse, dce, fhe_fold, precompute, sload_coalesce};

pub fn run(module: &mut IrModule, config: &OptimizerConfig, diags: &mut Vec<Diagnostic>) {
    let mut iteration = 0u32;
    loop {
        if iteration >= config.max_iterations {
            diags.push(diag::fixpoint_not_converged(module_span(module), iteration));
            break;
        }
        let mut changed = false;

        if config.enable_precompute {
            for f in &mut module.functions {
                changed |= precompute::run_function(f, diags);
            }
            maybe_validate(module, "precompute", config, diags);
        }
        if config.enable_constant_folding {
            for f in &mut module.functions {
                changed |= constant_fold::run_function(f);
            }
            maybe_validate(module, "constant_fold", config, diags);
        }
        if config.enable_trivial_fhe_folding {
            for f in &mut module.functions {
                changed |= fhe_fold::run_function(f);
            }
            maybe_validate(module, "fhe_fold", config, diags);
        }
        if config.enable_sload_coalescing {
            for f in &mut module.functions {
                changed |= sload_coalesce::run_function(f);
            }
            maybe_validate(module, "sload_coalesce", config, diags);
        }
        if config.enable_cse {
            for f in &mut module.functions {
                changed |= cse::run_function(f);
            }
            maybe_validate(module, "cse", config, diags);
        }
        if config.enable_dce {
            for f in &mut module.functions {
                changed |= dce::run_function(f);
            }
            maybe_validate(module, "dce", config, diags);
        }

        iteration += 1;
        if !changed {
            break;
        }
    }
}

fn maybe_validate(
    module: &IrModule,
    pass: &str,
    config: &OptimizerConfig,
    diags: &mut Vec<Diagnostic>,
) {
    if !config.validate_after_each_pass {
        return;
    }
    let issues = covenant_ir::validate(module);
    for issue in issues {
        diags.push(diag::invariant_broken(issue.span, pass, &issue.message));
    }
}

fn module_span(m: &IrModule) -> Span {
    m.functions
        .first()
        .map(|f| f.span)
        .unwrap_or_else(|| Span::new(m.source_id, 0, 0))
}

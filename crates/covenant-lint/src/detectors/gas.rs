//! GAS — Gas & DOS detectors (C1100, W1101, W1102, I1103).

use covenant_ir::{IrModule, Opcode};

use crate::framework::{Category, Detector, Finding, Severity};
use crate::ir_utils::{
    has_batch_up_to, has_collection_iter, is_view, loop_blocks, total_instr_count,
};

const EXPENSIVE_VIEW_THRESHOLD: usize = 150;
const HIGH_INSTR_THRESHOLD: usize = 500;

// ── C1100 ───────────────────────────────────────────────────────────────────

pub struct C1100UnboundedLoop;

impl Detector for C1100UnboundedLoop {
    fn code(&self) -> &'static str {
        "C1100"
    }
    fn name(&self) -> &'static str {
        "unbounded_loop"
    }
    fn severity(&self) -> Severity {
        Severity::Critical
    }
    fn category(&self) -> Category {
        Category::GasAndDOS
    }
    fn description(&self) -> &'static str {
        "Loop or collection iteration without a `@batch_up_to` bound — DOS via gas exhaustion."
    }

    fn analyze(&self, ir: &IrModule, _source: &str) -> Vec<Finding> {
        let mut findings = Vec::new();
        for func in &ir.functions {
            if has_batch_up_to(func) {
                continue;
            }
            let has_back_edge_loop = !loop_blocks(func).is_empty();
            let has_iter = has_collection_iter(func);
            if has_back_edge_loop || has_iter {
                findings.push(
                    Finding::new(
                        "C1100",
                        func.span,
                        format!(
                            "function `{}` has an unbounded iteration without `@batch_up_to`",
                            func.name.name
                        ),
                        Severity::Critical,
                    )
                    .with_help(
                        "Add `@batch_up_to(N)` annotation to bound the number of iterations",
                    ),
                );
            }
        }
        findings
    }
}

// ── W1101 ───────────────────────────────────────────────────────────────────

pub struct W1101ExpensiveView;

impl Detector for W1101ExpensiveView {
    fn code(&self) -> &'static str {
        "W1101"
    }
    fn name(&self) -> &'static str {
        "expensive_view"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn category(&self) -> Category {
        Category::GasAndDOS
    }
    fn description(&self) -> &'static str {
        "View function has a high instruction count — callers may hit gas limits."
    }

    fn analyze(&self, ir: &IrModule, _source: &str) -> Vec<Finding> {
        let mut findings = Vec::new();
        for func in &ir.functions {
            if !is_view(func) {
                continue;
            }
            let count = total_instr_count(func);
            if count > EXPENSIVE_VIEW_THRESHOLD {
                findings.push(
                    Finding::new(
                        "W1101",
                        func.span,
                        format!(
                            "view `{}` has {} instructions (threshold: {})",
                            func.name.name, count, EXPENSIVE_VIEW_THRESHOLD
                        ),
                        Severity::Warning,
                    )
                    .with_help("Split or cache results to reduce computation per call"),
                );
            }
        }
        findings
    }
}

// ── W1102 ───────────────────────────────────────────────────────────────────

pub struct W1102StoreInLoop;

impl Detector for W1102StoreInLoop {
    fn code(&self) -> &'static str {
        "W1102"
    }
    fn name(&self) -> &'static str {
        "store_in_loop"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn category(&self) -> Category {
        Category::GasAndDOS
    }
    fn description(&self) -> &'static str {
        "Storage write inside a loop — gas cost grows linearly with input size."
    }

    fn analyze(&self, ir: &IrModule, _source: &str) -> Vec<Finding> {
        let mut findings = Vec::new();
        for func in &ir.functions {
            let in_loop = loop_blocks(func);
            if in_loop.is_empty() {
                continue;
            }
            for (bi, block) in func.blocks.iter().enumerate() {
                if !in_loop.contains(&bi) {
                    continue;
                }
                for instr in &block.instructions {
                    if matches!(instr.opcode, Opcode::SStore(_) | Opcode::MapSet) {
                        findings.push(
                            Finding::new(
                                "W1102",
                                instr.span,
                                "storage write inside a loop — unbounded gas cost",
                                Severity::Warning,
                            )
                            .with_help(
                                "Accumulate changes in memory and write once after the loop",
                            ),
                        );
                    }
                }
            }
        }
        findings
    }
}

// ── I1103 ───────────────────────────────────────────────────────────────────

pub struct I1103HighInstructionCount;

impl Detector for I1103HighInstructionCount {
    fn code(&self) -> &'static str {
        "I1103"
    }
    fn name(&self) -> &'static str {
        "high_instruction_count"
    }
    fn severity(&self) -> Severity {
        Severity::Info
    }
    fn category(&self) -> Category {
        Category::GasAndDOS
    }
    fn description(&self) -> &'static str {
        "Function has an unusually high total instruction count — review for complexity."
    }

    fn analyze(&self, ir: &IrModule, _source: &str) -> Vec<Finding> {
        let mut findings = Vec::new();
        for func in &ir.functions {
            let count = total_instr_count(func);
            if count > HIGH_INSTR_THRESHOLD {
                findings.push(
                    Finding::new(
                        "I1103",
                        func.span,
                        format!(
                            "function `{}` has {} instructions (threshold: {})",
                            func.name.name, count, HIGH_INSTR_THRESHOLD
                        ),
                        Severity::Info,
                    )
                    .with_help("Consider decomposing into smaller functions"),
                );
            }
        }
        findings
    }
}

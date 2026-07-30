//! EXT: External Call detectors (C300, C301, W302, W303, I304).

use covenant_ir::{IrModule, Opcode};

use crate::framework::{Category, Detector, Finding, Severity};
use crate::ir_utils::{all_instrs, loop_blocks, transfer_to, val_from_opcode, value_sources};

// ── C300 ────────────────────────────────────────────────────────────────────

pub struct C300TransferToZero;

impl Detector for C300TransferToZero {
    fn code(&self) -> &'static str {
        "C300"
    }
    fn name(&self) -> &'static str {
        "transfer_to_zero_address"
    }
    fn severity(&self) -> Severity {
        Severity::Critical
    }
    fn category(&self) -> Category {
        Category::ExternalCalls
    }
    fn description(&self) -> &'static str {
        "Transfer destination is the zero address: funds will be permanently burned."
    }

    fn analyze(&self, ir: &IrModule, _source: &str) -> Vec<Finding> {
        let mut findings = Vec::new();
        for func in &ir.functions {
            let sources = value_sources(func);
            for instr in all_instrs(func) {
                if !matches!(instr.opcode, Opcode::Transfer) {
                    continue;
                }
                let Some(to_val) = transfer_to(instr) else {
                    continue;
                };
                if val_from_opcode(&sources, to_val, &|op| {
                    matches!(op, Opcode::LoadZeroAddress)
                }) {
                    findings.push(
                        Finding::new(
                            "C300",
                            instr.span,
                            "Transfer destination is the zero address: funds will be burned",
                            Severity::Critical,
                        )
                        .with_help("Validate that the recipient address is non-zero"),
                    );
                }
            }
        }
        findings
    }
}

// ── C301 ────────────────────────────────────────────────────────────────────

pub struct C301UncheckedTransferParam;

impl Detector for C301UncheckedTransferParam {
    fn code(&self) -> &'static str {
        "C301"
    }
    fn name(&self) -> &'static str {
        "unchecked_transfer_param"
    }
    fn severity(&self) -> Severity {
        Severity::Critical
    }
    fn category(&self) -> Category {
        Category::ExternalCalls
    }
    fn description(&self) -> &'static str {
        "Transfer destination comes directly from a caller-supplied parameter with no Assert."
    }

    fn analyze(&self, ir: &IrModule, _source: &str) -> Vec<Finding> {
        let mut findings = Vec::new();
        for func in &ir.functions {
            let param_values: std::collections::HashSet<_> =
                func.params.iter().map(|p| p.value).collect();
            if param_values.is_empty() {
                continue;
            }
            let has_assert = all_instrs(func).any(|i| matches!(i.opcode, Opcode::Assert));
            for instr in all_instrs(func) {
                if !matches!(instr.opcode, Opcode::Transfer) {
                    continue;
                }
                let Some(to_val) = transfer_to(instr) else {
                    continue;
                };
                if param_values.contains(&to_val) && !has_assert {
                    findings.push(
                        Finding::new(
                            "C301",
                            instr.span,
                            "Transfer to an unvalidated caller-supplied address",
                            Severity::Critical,
                        )
                        .with_help("Assert the recipient address is non-zero before transferring"),
                    );
                }
            }
        }
        findings
    }
}

// ── W302 ────────────────────────────────────────────────────────────────────

pub struct W302TransferInLoop;

impl Detector for W302TransferInLoop {
    fn code(&self) -> &'static str {
        "W302"
    }
    fn name(&self) -> &'static str {
        "transfer_in_loop_ext"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn category(&self) -> Category {
        Category::ExternalCalls
    }
    fn description(&self) -> &'static str {
        "Transfer inside a loop may fail mid-batch, leaving state inconsistent."
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
                    if matches!(instr.opcode, Opcode::Transfer) {
                        findings.push(
                            Finding::new(
                                "W302",
                                instr.span,
                                "Transfer inside a loop: partial failure risk",
                                Severity::Warning,
                            )
                            .with_help(
                                "Use pull-over-push or accumulate transfers outside the loop",
                            ),
                        );
                    }
                }
            }
        }
        findings
    }
}

// ── W303 ────────────────────────────────────────────────────────────────────

pub struct W303NoEnsureBeforeTransfer;

impl Detector for W303NoEnsureBeforeTransfer {
    fn code(&self) -> &'static str {
        "W303"
    }
    fn name(&self) -> &'static str {
        "no_ensure_before_transfer"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn category(&self) -> Category {
        Category::ExternalCalls
    }
    fn description(&self) -> &'static str {
        "Function performs a Transfer with no preceding `ensure`/`assert` check."
    }

    fn analyze(&self, ir: &IrModule, _source: &str) -> Vec<Finding> {
        let mut findings = Vec::new();
        for func in &ir.functions {
            let has_transfer = all_instrs(func).any(|i| matches!(i.opcode, Opcode::Transfer));
            if !has_transfer {
                continue;
            }
            let has_assert = all_instrs(func).any(|i| matches!(i.opcode, Opcode::Assert));
            if !has_assert {
                // Find the first transfer span for the finding location.
                if let Some(t) = all_instrs(func).find(|i| matches!(i.opcode, Opcode::Transfer)) {
                    findings.push(
                        Finding::new(
                            "W303",
                            t.span,
                            format!(
                                "function `{}` transfers funds without a preceding assertion",
                                func.name.name
                            ),
                            Severity::Warning,
                        )
                        .with_help(
                            "Validate before transferring with a `when` guard on the \
                             action, for example \
                             `action pay(v: amount) when balances[caller] >= v`. \
                             Covenant has no `ensure` or `assert` statement: both \
                             are E030.",
                        ),
                    );
                }
            }
        }
        findings
    }
}

// ── I304 ────────────────────────────────────────────────────────────────────

pub struct I304TransferWithNoLogging;

impl Detector for I304TransferWithNoLogging {
    fn code(&self) -> &'static str {
        "I304"
    }
    fn name(&self) -> &'static str {
        "transfer_with_no_logging"
    }
    fn severity(&self) -> Severity {
        Severity::Info
    }
    fn category(&self) -> Category {
        Category::ExternalCalls
    }
    fn description(&self) -> &'static str {
        "Transfer is performed without emitting an event, transfers become untrackable off-chain."
    }

    fn analyze(&self, ir: &IrModule, _source: &str) -> Vec<Finding> {
        let mut findings = Vec::new();
        for func in &ir.functions {
            let has_transfer = all_instrs(func).any(|i| matches!(i.opcode, Opcode::Transfer));
            if !has_transfer {
                continue;
            }
            let has_emit = all_instrs(func).any(|i| matches!(i.opcode, Opcode::Emit(_)));
            if !has_emit {
                if let Some(t) = all_instrs(func).find(|i| matches!(i.opcode, Opcode::Transfer)) {
                    findings.push(
                        Finding::new(
                            "I304",
                            t.span,
                            format!(
                                "function `{}` transfers funds without emitting an event",
                                func.name.name
                            ),
                            Severity::Info,
                        )
                        .with_help("Emit a Transfer event to enable off-chain tracking"),
                    );
                }
            }
        }
        findings
    }
}

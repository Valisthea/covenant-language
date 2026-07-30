//! REE: Reentrancy detectors (C001, C002, W003, I004).

use covenant_ir::{IrFunctionKind, IrModule, Opcode};

use crate::framework::{Category, Detector, Finding, Severity};
use crate::ir_utils::{all_instrs, has_annotation, is_action, loop_blocks};

// ── C001 ────────────────────────────────────────────────────────────────────

pub struct C001StateAfterTransfer;

impl Detector for C001StateAfterTransfer {
    fn code(&self) -> &'static str {
        "C001"
    }
    fn name(&self) -> &'static str {
        "state_modification_after_transfer"
    }
    fn severity(&self) -> Severity {
        Severity::Critical
    }
    fn category(&self) -> Category {
        Category::Reentrancy
    }
    fn description(&self) -> &'static str {
        "State is modified after an external Transfer, classic reentrancy pattern."
    }

    fn analyze(&self, ir: &IrModule, _source: &str) -> Vec<Finding> {
        let mut findings = Vec::new();
        for func in &ir.functions {
            if !is_action(func) {
                continue;
            }
            // Find the (block_idx, instr_idx) of the first Transfer.
            let mut first_transfer: Option<(usize, usize)> = None;
            'outer: for (bi, block) in func.blocks.iter().enumerate() {
                for (ii, instr) in block.instructions.iter().enumerate() {
                    if matches!(instr.opcode, Opcode::Transfer | Opcode::ExternalCall { .. }) {
                        first_transfer = Some((bi, ii));
                        break 'outer;
                    }
                }
            }
            let Some((t_bi, t_ii)) = first_transfer else {
                continue;
            };

            // Flag every SStore that comes after the first Transfer.
            for (bi, block) in func.blocks.iter().enumerate() {
                for (ii, instr) in block.instructions.iter().enumerate() {
                    if !matches!(instr.opcode, Opcode::SStore(_)) {
                        continue;
                    }
                    let after = bi > t_bi || (bi == t_bi && ii > t_ii);
                    if after {
                        findings.push(
                            Finding::new(
                                "C001",
                                instr.span,
                                "state modified after external Transfer: reentrancy risk",
                                Severity::Critical,
                            )
                            .with_help("Move all state updates before the Transfer call"),
                        );
                    }
                }
            }
        }
        findings
    }
}

// ── C002 ────────────────────────────────────────────────────────────────────

pub struct C002TransferInLoop;

impl Detector for C002TransferInLoop {
    fn code(&self) -> &'static str {
        "C002"
    }
    fn name(&self) -> &'static str {
        "transfer_in_loop"
    }
    fn severity(&self) -> Severity {
        Severity::Critical
    }
    fn category(&self) -> Category {
        Category::Reentrancy
    }
    fn description(&self) -> &'static str {
        "Transfer inside a loop amplifies reentrancy attack surface."
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
                    if matches!(instr.opcode, Opcode::Transfer | Opcode::ExternalCall { .. }) {
                        findings.push(
                            Finding::new(
                                "C002",
                                instr.span,
                                "Transfer inside a loop: reentrancy amplification risk",
                                Severity::Critical,
                            )
                            .with_help("Prefer pull-over-push: let recipients claim funds"),
                        );
                    }
                }
            }
        }
        findings
    }
}

// ── W003 ────────────────────────────────────────────────────────────────────

pub struct W003MissingNonReentrant;

impl Detector for W003MissingNonReentrant {
    fn code(&self) -> &'static str {
        "W003"
    }
    fn name(&self) -> &'static str {
        "missing_non_reentrant"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn category(&self) -> Category {
        Category::Reentrancy
    }
    fn description(&self) -> &'static str {
        "Action makes an external call, and this release has no reentrancy guard."
    }

    fn analyze(&self, ir: &IrModule, _source: &str) -> Vec<Finding> {
        let mut findings = Vec::new();
        for func in &ir.functions {
            if !is_action(func) {
                continue;
            }
            let has_external = all_instrs(func)
                .any(|i| matches!(i.opcode, Opcode::Transfer | Opcode::ExternalCall { .. }));
            if has_external && !has_annotation(func, "non_reentrant") {
                findings.push(
                    Finding::new(
                        "W003",
                        func.span,
                        format!(
                            "action `{}` makes an external call with no reentrancy \
                             protection, and this release has none to offer",
                            func.name.name
                        ),
                        Severity::Warning,
                    )
                    .with_help(
                        "Write every state change before the transfer. There is no \
                         reentrancy guard in the language at this release: \
                         `@non_reentrant` is not an annotation the compiler knows and \
                         writing it is E110, and `vault` adds no protection either, \
                         since the same body written as `module` compiles to \
                         byte-identical bytecode.",
                    ),
                );
            }
        }
        findings
    }
}

// ── I004 ────────────────────────────────────────────────────────────────────

pub struct I004TransferInInitializer;

impl Detector for I004TransferInInitializer {
    fn code(&self) -> &'static str {
        "I004"
    }
    fn name(&self) -> &'static str {
        "transfer_in_initializer"
    }
    fn severity(&self) -> Severity {
        Severity::Info
    }
    fn category(&self) -> Category {
        Category::Reentrancy
    }
    fn description(&self) -> &'static str {
        "A field initializer contains a Transfer, which executes at deployment time."
    }

    fn analyze(&self, ir: &IrModule, _source: &str) -> Vec<Finding> {
        let mut findings = Vec::new();
        for func in &ir.functions {
            if !matches!(func.kind, IrFunctionKind::FieldInitializer(_)) {
                continue;
            }
            for instr in all_instrs(func) {
                if matches!(instr.opcode, Opcode::Transfer | Opcode::ExternalCall { .. }) {
                    findings.push(
                        Finding::new(
                            "I004",
                            instr.span,
                            "Transfer in field initializer executes at deployment",
                            Severity::Info,
                        )
                        .with_help("Move transfers to a dedicated `init` action instead"),
                    );
                }
            }
        }
        findings
    }
}

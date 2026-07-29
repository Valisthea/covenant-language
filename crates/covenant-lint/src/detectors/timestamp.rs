//! TIM: Timestamp detectors (W1200, W1201, I1202).

use covenant_ir::{IrModule, Opcode, Terminator};

use crate::framework::{Category, Detector, Finding, Severity};
use crate::ir_utils::{all_instrs, is_action, val_or_operand_from_opcode, value_sources};

// ── W1200 ───────────────────────────────────────────────────────────────────

pub struct W1200TimestampForRandomness;

impl Detector for W1200TimestampForRandomness {
    fn code(&self) -> &'static str {
        "W1200"
    }
    fn name(&self) -> &'static str {
        "timestamp_for_randomness"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn category(&self) -> Category {
        Category::Timestamp
    }
    fn description(&self) -> &'static str {
        "Block timestamp used as input to a hash or random-looking operation, biasable by miners."
    }

    fn analyze(&self, ir: &IrModule, _source: &str) -> Vec<Finding> {
        let is_timestamp = |op: &Opcode| matches!(op, Opcode::LoadBlockTimestamp | Opcode::LoadNow);
        let is_hash = |op: &Opcode| {
            matches!(
                op,
                Opcode::Keccak | Opcode::Blake2 | Opcode::Hmac | Opcode::PqRand
            )
        };

        let mut findings = Vec::new();
        for func in &ir.functions {
            let sources = value_sources(func);
            for instr in all_instrs(func) {
                if !is_hash(&instr.opcode) {
                    continue;
                }
                for &operand in &instr.operands {
                    if val_or_operand_from_opcode(func, &sources, operand, &is_timestamp) {
                        findings.push(
                            Finding::new(
                                "W1200",
                                instr.span,
                                "block timestamp used in hash/randomness: \
                                 miners can bias the result",
                                Severity::Warning,
                            )
                            .with_help(
                                "Use a VDF, commit-reveal scheme, or on-chain randomness beacon",
                            ),
                        );
                        break;
                    }
                }
            }
        }
        findings
    }
}

// ── W1201 ───────────────────────────────────────────────────────────────────

pub struct W1201BlockNumberInBranch;

impl Detector for W1201BlockNumberInBranch {
    fn code(&self) -> &'static str {
        "W1201"
    }
    fn name(&self) -> &'static str {
        "block_number_in_branch"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn category(&self) -> Category {
        Category::Timestamp
    }
    fn description(&self) -> &'static str {
        "Block number used in a conditional branch: miners can influence block production timing."
    }

    fn analyze(&self, ir: &IrModule, _source: &str) -> Vec<Finding> {
        let is_block_number = |op: &Opcode| matches!(op, Opcode::LoadBlockNumber);

        let mut findings = Vec::new();
        for func in &ir.functions {
            let sources = value_sources(func);
            for block in &func.blocks {
                let cond = match &block.terminator {
                    Terminator::Branch { cond, .. } => *cond,
                    _ => continue,
                };
                if val_or_operand_from_opcode(func, &sources, cond, &is_block_number) {
                    findings.push(
                        Finding::new(
                            "W1201",
                            block.span,
                            "block number used in a conditional branch: \
                             timing can be influenced by miners",
                            Severity::Warning,
                        )
                        .with_help(
                            "Use block timestamps with a safety margin, or an oracle-verified deadline",
                        ),
                    );
                }
            }
        }
        findings
    }
}

// ── I1202 ───────────────────────────────────────────────────────────────────

pub struct I1202TimestampDependency;

impl Detector for I1202TimestampDependency {
    fn code(&self) -> &'static str {
        "I1202"
    }
    fn name(&self) -> &'static str {
        "timestamp_dependency"
    }
    fn severity(&self) -> Severity {
        Severity::Info
    }
    fn category(&self) -> Category {
        Category::Timestamp
    }
    fn description(&self) -> &'static str {
        "Action reads the block timestamp: document the acceptable manipulation window."
    }

    fn analyze(&self, ir: &IrModule, _source: &str) -> Vec<Finding> {
        let mut findings = Vec::new();
        for func in &ir.functions {
            if !is_action(func) {
                continue;
            }
            for instr in all_instrs(func) {
                if matches!(instr.opcode, Opcode::LoadBlockTimestamp | Opcode::LoadNow) {
                    findings.push(
                        Finding::new(
                            "I1202",
                            instr.span,
                            format!("action `{}` depends on block timestamp", func.name.name),
                            Severity::Info,
                        )
                        .with_help(
                            "Miners can shift the timestamp by ~15 s; \
                             ensure logic tolerates this drift",
                        ),
                    );
                    break; // one finding per function
                }
            }
        }
        findings
    }
}

//! ACC: Access Control detectors (C100, C101, W102, W103, I104).

use std::collections::HashSet;

use covenant_ir::{GlobalId, IrGuard, IrModule, IrPrincipal, Opcode, Terminator};

use crate::framework::{Category, Detector, Finding, Severity};
use crate::ir_utils::{
    all_instrs, has_vdf_locked, is_action, val_or_operand_from_opcode, value_sources,
};

// ── C100 ────────────────────────────────────────────────────────────────────

pub struct C100NoAccessGuard;

impl Detector for C100NoAccessGuard {
    fn code(&self) -> &'static str {
        "C100"
    }
    fn name(&self) -> &'static str {
        "no_access_guard"
    }
    fn severity(&self) -> Severity {
        Severity::Critical
    }
    fn category(&self) -> Category {
        Category::AccessControl
    }
    fn description(&self) -> &'static str {
        "Action modifies state but has no `only(principal)` access guard."
    }

    fn analyze(&self, ir: &IrModule, _source: &str) -> Vec<Finding> {
        let mut findings = Vec::new();
        for func in &ir.functions {
            if !is_action(func) {
                continue;
            }
            let has_sstore = all_instrs(func).any(|i| matches!(i.opcode, Opcode::SStore(_)));
            if !has_sstore {
                continue;
            }
            let has_only = func.guards.iter().any(|g| matches!(g, IrGuard::Only(_)));
            if !has_only {
                findings.push(
                    Finding::new(
                        "C100",
                        func.span,
                        format!(
                            "action `{}` modifies state with no access guard",
                            func.name.name
                        ),
                        Severity::Critical,
                    )
                    .with_help(
                        "Add a principal guard: `only owner`, `only deployer`, \
                         `only admin` backed by a field of that name, or an explicit \
                         `only 0x..` address. To accept a public action, put the \
                         suppression in a COMMENT, \
                         `-- @allow(C100, reason: \"intentionally public\")`, or set \
                         \"C100\": \"off\" in .covenantlint.json. Written as a bare \
                         annotation instead of a comment, `@allow(..)` fails the \
                         build with E110: the linter reads it from the source text, \
                         the parser does not know it.",
                    ),
                );
            }
        }
        findings
    }
}

// ── C101 ────────────────────────────────────────────────────────────────────

pub struct C101BlockConditionAsAuth;

impl Detector for C101BlockConditionAsAuth {
    fn code(&self) -> &'static str {
        "C101"
    }
    fn name(&self) -> &'static str {
        "block_condition_as_auth"
    }
    fn severity(&self) -> Severity {
        Severity::Critical
    }
    fn category(&self) -> Category {
        Category::AccessControl
    }
    fn description(&self) -> &'static str {
        "Block timestamp or block number used as an authorization condition."
    }

    fn analyze(&self, ir: &IrModule, _source: &str) -> Vec<Finding> {
        let is_block_env =
            |op: &Opcode| matches!(op, Opcode::LoadBlockTimestamp | Opcode::LoadBlockNumber);

        let mut findings = Vec::new();
        for func in &ir.functions {
            let sources = value_sources(func);
            for block in &func.blocks {
                let cond = match &block.terminator {
                    Terminator::Branch { cond, .. } => *cond,
                    _ => continue,
                };
                if val_or_operand_from_opcode(func, &sources, cond, &is_block_env) {
                    findings.push(
                        Finding::new(
                            "C101",
                            block.span,
                            "block timestamp/number used as authorization condition, \
                             miners can manipulate block values",
                            Severity::Critical,
                        )
                        .with_help("Use a principal-based guard (`only(...)`) instead"),
                    );
                }
            }
        }
        findings
    }
}

// ── W102 ────────────────────────────────────────────────────────────────────

pub struct W102AdminNoTimelock;

impl Detector for W102AdminNoTimelock {
    fn code(&self) -> &'static str {
        "W102"
    }
    fn name(&self) -> &'static str {
        "admin_no_timelock"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn category(&self) -> Category {
        Category::AccessControl
    }
    fn description(&self) -> &'static str {
        "Admin action has no VDF-locked timelock, allowing immediate privilege execution."
    }

    fn analyze(&self, ir: &IrModule, _source: &str) -> Vec<Finding> {
        let mut findings = Vec::new();
        for func in &ir.functions {
            let has_admin = func
                .guards
                .iter()
                .any(|g| matches!(g, IrGuard::Only(IrPrincipal::Admin(_))));
            if !has_admin {
                continue;
            }
            if !has_vdf_locked(func) {
                findings.push(
                    Finding::new(
                        "W102",
                        func.span,
                        format!(
                            "admin action `{}` executes without a timelock",
                            func.name.name
                        ),
                        Severity::Warning,
                    )
                    .with_help(
                        "No timelock qualifier compiles at this release. \
                         `vdf_locked(delay)` does not parse, and the form that does, \
                         `vdf_locked for <duration>`, is refused by the backend with \
                         E517 because it has no EVM lowering. Store the deadline in a \
                         `time` field and guard on it: \
                         `action claim() when now >= unlock_at`.",
                    ),
                );
            }
        }
        findings
    }
}

// ── W103 ────────────────────────────────────────────────────────────────────

pub struct W103OwnerZeroRisk;

impl Detector for W103OwnerZeroRisk {
    fn code(&self) -> &'static str {
        "W103"
    }
    fn name(&self) -> &'static str {
        "owner_zero_risk"
    }
    fn severity(&self) -> Severity {
        Severity::Warning
    }
    fn category(&self) -> Category {
        Category::AccessControl
    }
    fn description(&self) -> &'static str {
        "Action guarded by `only(owner)`: risk of bricking the contract if owner is zero."
    }

    fn analyze(&self, ir: &IrModule, _source: &str) -> Vec<Finding> {
        let mut findings = Vec::new();
        for func in &ir.functions {
            let has_owner = func
                .guards
                .iter()
                .any(|g| matches!(g, IrGuard::Only(IrPrincipal::Owner(_))));
            if has_owner {
                findings.push(
                    Finding::new(
                        "W103",
                        func.span,
                        format!(
                            "action `{}` uses `only(owner)`: zero-address owner risk",
                            func.name.name
                        ),
                        Severity::Warning,
                    )
                    .with_help("Assert the owner address is non-zero before assigning ownership"),
                );
            }
        }
        findings
    }
}

// ── I104 ────────────────────────────────────────────────────────────────────

pub struct I104SingleStepOwnerTransfer;

impl Detector for I104SingleStepOwnerTransfer {
    fn code(&self) -> &'static str {
        "I104"
    }
    fn name(&self) -> &'static str {
        "single_step_owner_transfer"
    }
    fn severity(&self) -> Severity {
        Severity::Info
    }
    fn category(&self) -> Category {
        Category::AccessControl
    }
    fn description(&self) -> &'static str {
        "Ownership transferred in one step with no confirmation from the new owner."
    }

    fn analyze(&self, ir: &IrModule, _source: &str) -> Vec<Finding> {
        let owner_fields: HashSet<GlobalId> = ir
            .fields
            .iter()
            .filter(|f| {
                let n = f.name.name.to_ascii_lowercase();
                n.contains("owner") || n.contains("admin")
            })
            .map(|f| f.id)
            .collect();

        if owner_fields.is_empty() {
            return vec![];
        }

        let mut findings = Vec::new();
        for func in &ir.functions {
            if !is_action(func) {
                continue;
            }
            for instr in all_instrs(func) {
                if let Opcode::SStore(gid) = instr.opcode {
                    if owner_fields.contains(&gid) {
                        findings.push(
                            Finding::new(
                                "I104",
                                instr.span,
                                "ownership transferred in a single step: new owner not confirmed",
                                Severity::Info,
                            )
                            .with_help("Use two-step transfer: propose new owner, then accept"),
                        );
                    }
                }
            }
        }
        findings
    }
}

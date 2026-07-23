//! The `Detector` trait — the contract every security detector implements.

use covenant_ir::IrModule;

use crate::framework::{Finding, Severity};

pub trait Detector: Send + Sync {
    /// Stable alphanumeric code, e.g. `"C001"`.
    fn code(&self) -> &'static str;

    /// Snake-case name, e.g. `"state_modification_after_external_call"`.
    fn name(&self) -> &'static str;

    /// Default severity (can be overridden via `--deny` / `--allow`).
    fn severity(&self) -> Severity;

    /// One-line description for `--help` and SARIF output.
    fn description(&self) -> &'static str;

    /// Logical category.
    fn category(&self) -> Category;

    /// If true the detector is skipped unless `--deep` is passed.
    fn requires_deep(&self) -> bool {
        false
    }

    /// Analyze `ir` and return all findings for this module.
    ///
    /// Called once per source file. Must not mutate any shared state — every
    /// detector is a pure function `IrModule → Vec<Finding>`.
    fn analyze(&self, ir: &IrModule, source: &str) -> Vec<Finding>;
}

/// Broad vulnerability category per Doc 15 §4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Reentrancy,
    AccessControl,
    Arithmetic,
    ExternalCalls,
    OraclePriceManipulation,
    MEV,
    FHE,
    PostQuantum,
    Amnesia,
    ZKDisclosure,
    Upgradeability,
    GasAndDOS,
    Timestamp,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reentrancy => "reentrancy",
            Self::AccessControl => "access-control",
            Self::Arithmetic => "arithmetic",
            Self::ExternalCalls => "external-calls",
            Self::OraclePriceManipulation => "oracle",
            Self::MEV => "mev",
            Self::FHE => "fhe",
            Self::PostQuantum => "post-quantum",
            Self::Amnesia => "amnesia",
            Self::ZKDisclosure => "zk-disclosure",
            Self::Upgradeability => "upgradeability",
            Self::GasAndDOS => "gas-dos",
            Self::Timestamp => "timestamp",
        }
    }
}

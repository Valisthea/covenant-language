//! Detector registry — owns all detector instances and handles filtering.

use std::collections::HashMap;
use std::sync::Arc;

use crate::detectors;
use crate::framework::{Category, Detector, Severity};

pub struct DetectorRegistry {
    detectors: Vec<Arc<dyn Detector>>,
    by_code: HashMap<&'static str, usize>,
}

impl DetectorRegistry {
    pub fn new() -> Self {
        let mut r = Self {
            detectors: Vec::new(),
            by_code: HashMap::new(),
        };
        r.register_all();
        r
    }

    fn register_all(&mut self) {
        // REE — Reentrancy
        self.push(Arc::new(detectors::reentrancy::C001StateAfterTransfer));
        self.push(Arc::new(detectors::reentrancy::C002TransferInLoop));
        self.push(Arc::new(detectors::reentrancy::W003MissingNonReentrant));
        self.push(Arc::new(detectors::reentrancy::I004TransferInInitializer));

        // ACC — Access Control
        self.push(Arc::new(detectors::access_control::C100NoAccessGuard));
        self.push(Arc::new(
            detectors::access_control::C101BlockConditionAsAuth,
        ));
        self.push(Arc::new(detectors::access_control::W102AdminNoTimelock));
        self.push(Arc::new(detectors::access_control::W103OwnerZeroRisk));
        self.push(Arc::new(
            detectors::access_control::I104SingleStepOwnerTransfer,
        ));

        // EXT — External calls
        self.push(Arc::new(detectors::external_calls::C300TransferToZero));
        self.push(Arc::new(
            detectors::external_calls::C301UncheckedTransferParam,
        ));
        self.push(Arc::new(detectors::external_calls::W302TransferInLoop));
        self.push(Arc::new(
            detectors::external_calls::W303NoEnsureBeforeTransfer,
        ));
        self.push(Arc::new(
            detectors::external_calls::I304TransferWithNoLogging,
        ));

        // GAS — Gas & DOS
        self.push(Arc::new(detectors::gas::C1100UnboundedLoop));
        self.push(Arc::new(detectors::gas::W1101ExpensiveView));
        self.push(Arc::new(detectors::gas::W1102StoreInLoop));
        self.push(Arc::new(detectors::gas::I1103HighInstructionCount));

        // TIM — Timestamp
        self.push(Arc::new(detectors::timestamp::W1200TimestampForRandomness));
        self.push(Arc::new(detectors::timestamp::W1201BlockNumberInBranch));
        self.push(Arc::new(detectors::timestamp::I1202TimestampDependency));

        // PQ — Post-quantum invariants (KSR-CVN-024)
        self.push(Arc::new(detectors::pq::C700PqVerifyWithoutNonce));

        // AMN — Amnesia ceremony invariants (KSR-CVN-025)
        self.push(Arc::new(
            detectors::amnesia::C801CeremonyPhaseBackwardTransition,
        ));
    }

    fn push(&mut self, d: Arc<dyn Detector>) {
        let code = d.code();
        let idx = self.detectors.len();
        self.by_code.insert(code, idx);
        self.detectors.push(d);
    }

    pub fn all(&self) -> &[Arc<dyn Detector>] {
        &self.detectors
    }

    pub fn get(&self, code: &str) -> Option<&Arc<dyn Detector>> {
        self.by_code.get(code).map(|&i| &self.detectors[i])
    }

    /// Return detectors that pass the given filters.
    pub fn filtered(
        &self,
        filter: Option<&str>,
        min_severity: Severity,
        deep: bool,
        category: Option<Category>,
    ) -> Vec<&Arc<dyn Detector>> {
        self.detectors
            .iter()
            .filter(|d| {
                if d.requires_deep() && !deep {
                    return false;
                }
                if d.severity() < min_severity {
                    return false;
                }
                if let Some(cat) = category {
                    if d.category() != cat {
                        return false;
                    }
                }
                if let Some(f) = filter {
                    let fl = f.to_ascii_lowercase();
                    return d.code().to_ascii_lowercase().contains(&fl)
                        || d.name().to_ascii_lowercase().contains(&fl)
                        || d.category().as_str().contains(&fl);
                }
                true
            })
            .collect()
    }
}

impl Default for DetectorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    //! KSR-CVN-005: registry self-test. Guarantees that a regression removing
    //! a scope-documented detector — or silently reducing the set below the
    //! 21-detector baseline — will fail CI rather than fly under the radar.

    use super::*;

    #[test]
    fn registry_has_at_least_baseline_count() {
        let r = DetectorRegistry::new();
        assert!(
            r.all().len() >= 21,
            "detector registry fell below the KSR-CVN-005 baseline of 21 (got {})",
            r.all().len()
        );
    }

    #[test]
    fn registry_contains_every_scope_documented_code() {
        // Scope-documented codes whose removal must fail CI. When adding a
        // new detector also document it in `docs/diagnostic-codes.md` and
        // extend this list.
        let required: &[&str] = &[
            // REE
            "C001", "C002", "W003", "I004", // ACC
            "C100", "C101", "W102", "W103", "I104", // EXT
            "C300", "C301", "W302", "W303", "I304", // GAS
            "C1100", "W1101", "W1102", "I1103", // TIM
            "W1200", "W1201", "I1202", // PQ (Session 2)
            "C700",  // AMN (Session 2)
            "C801",
        ];
        let r = DetectorRegistry::new();
        for code in required {
            assert!(
                r.get(code).is_some(),
                "detector `{code}` is scope-documented but not registered — \
                 see docs/diagnostic-codes.md"
            );
        }
    }

    #[test]
    fn registry_codes_have_no_duplicates() {
        let r = DetectorRegistry::new();
        let mut seen = std::collections::HashSet::new();
        for d in r.all() {
            assert!(
                seen.insert(d.code()),
                "detector code `{}` is registered twice",
                d.code()
            );
        }
    }
}

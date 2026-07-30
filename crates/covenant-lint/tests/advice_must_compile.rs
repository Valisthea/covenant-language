//! The linter may not prescribe syntax the compiler rejects.
//!
//! A security linter exists to move users toward safer code. Four of its
//! remedies moved them into a build failure instead:
//!
//!   - `@non_reentrant`, offered for W003, is E110: no such annotation.
//!   - `@allow(C100)`, offered as the suppression for C100, is E110 as a bare
//!     annotation. It works only inside a comment, which nothing said.
//!   - `ensure` and `assert`, offered before a transfer, are both E030: no
//!     such statement exists.
//!   - `vdf_locked(delay)` does not parse, and the form that does,
//!     `vdf_locked for <duration>`, is refused by the backend with E517.
//!
//! Following the tool's own advice broke the build, and nothing caught it
//! because no test ever read the help text back.

use covenant_diag::SourceId;
use covenant_lint::framework::registry::DetectorRegistry;

/// Spellings the compiler rejects, as a user would copy them out of a help
/// string. Each is paired with the diagnostic it produces, so a reader knows
/// the list was measured rather than assumed.
const PRESCRIBES_REJECTED_SYNTAX: &[(&str, &str)] = &[
    ("Add `@non_reentrant`", "E110, unknown annotation"),
    ("`ensure` or `assert` to", "E030, no such statement"),
    ("Add `vdf_locked(delay)`", "E020, does not parse"),
];

/// Exercises the transfer, access-control and reentrancy detectors at once.
const SOURCE: &str = "\
vault Treasury {
    field balances: map<address, amount>

    action withdraw(value: amount) {
        transfer(value) to caller
        balances[caller] -= value
    }
}
";

fn advice() -> Vec<(&'static str, &'static str)> {
    covenant_lint::lint_source(SOURCE, SourceId::new(0))
        .into_iter()
        .filter_map(|f| f.help.map(|h| (f.detector_code, h)))
        .collect()
}

#[test]
fn no_advice_prescribes_syntax_the_compiler_rejects() {
    let mut offences = Vec::new();
    for (code, help) in advice() {
        for (bad, why) in PRESCRIBES_REJECTED_SYNTAX {
            // Naming a rejected form in order to warn against it is fine, and
            // several help texts now do exactly that. Prescribing it is not,
            // and in these strings prescription always reads as "Add <thing>"
            // or "<thing> to ...", which is what these needles match.
            if help.contains(bad) {
                offences.push(format!("{code} prescribes {bad:?} ({why}):\n      {help}"));
            }
        }
    }
    assert!(
        offences.is_empty(),
        "the linter tells users to write code the compiler refuses:\n  {}",
        offences.join("\n  ")
    );
}

/// The detector descriptions are the other user-facing surface, and W003's
/// named the nonexistent annotation for as long as its help did.
#[test]
fn no_detector_description_prescribes_a_nonexistent_annotation() {
    let registry = DetectorRegistry::new();
    let offenders: Vec<_> = registry
        .all()
        .iter()
        .filter(|d| d.description().contains("without an `@non_reentrant`"))
        .map(|d| format!("{}: {}", d.code(), d.description()))
        .collect();
    assert!(
        offenders.is_empty(),
        "a detector describes itself in terms of an annotation that is E110:\n  {}",
        offenders.join("\n  ")
    );
}

/// A negative control. If `lint_source` stopped returning findings with help
/// attached, the test above would pass on an empty list and prove nothing.
#[test]
fn the_source_used_above_does_produce_advice() {
    assert!(
        !advice().is_empty(),
        "no finding carried help text, so the advice test is vacuous"
    );
}

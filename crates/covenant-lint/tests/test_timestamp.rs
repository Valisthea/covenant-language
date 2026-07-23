//! Tests for TIM detectors (W1200, W1201, I1202).

mod helpers;
use helpers::run;

use covenant_lint::detectors::timestamp::{
    I1202TimestampDependency, W1200TimestampForRandomness, W1201BlockNumberInBranch,
};

// ── W1200 ───────────────────────────────────────────────────────────────────

#[test]
fn w1200_no_finding_without_timestamp() {
    let src = r#"
record R {
    x: amount
    action go() only caller { x = 1 }
}
"#;
    let f = run(&W1200TimestampForRandomness, src);
    assert!(f.is_empty(), "unexpected W1200: {f:?}");
}

#[test]
fn w1200_no_finding_timestamp_not_hashed() {
    // Timestamp read but not fed into a hash — should not trigger W1200.
    let src = r#"
record R {
    deadline: time
    action go() when now < deadline {}
}
"#;
    let f = run(&W1200TimestampForRandomness, src);
    assert!(f.is_empty(), "unexpected W1200 on deadline check: {f:?}");
}

// ── W1201 ───────────────────────────────────────────────────────────────────

#[test]
fn w1201_no_finding_without_block_number() {
    let src = r#"
record R {
    x: amount
    action go() only caller { x = 1 }
}
"#;
    let f = run(&W1201BlockNumberInBranch, src);
    assert!(f.is_empty(), "unexpected W1201: {f:?}");
}

// ── I1202 ───────────────────────────────────────────────────────────────────

#[test]
fn i1202_fires_on_timestamp_in_action() {
    let src = r#"
record R {
    deadline: time
    action go() when now < deadline {}
}
"#;
    // `now` lowers to LoadBlockTimestamp — I1202 should fire.
    let f = run(&I1202TimestampDependency, src);
    assert!(!f.is_empty(), "expected I1202 for timestamp dependency");
    assert_eq!(f[0].detector_code, "I1202");
}

#[test]
fn i1202_no_finding_in_view() {
    // I1202 only applies to actions.
    let src = r#"
record R {
    deadline: time
    view is_expired returns bool { now >= deadline }
}
"#;
    let f = run(&I1202TimestampDependency, src);
    assert!(f.is_empty(), "I1202 should not fire on view: {f:?}");
}

#[test]
fn i1202_no_finding_without_timestamp() {
    let src = r#"
record R {
    x: amount
    action go() only caller { x = 1 }
}
"#;
    let f = run(&I1202TimestampDependency, src);
    assert!(f.is_empty());
}

#[test]
fn i1202_fires_only_once_per_function() {
    // Even if a function reads `now` multiple times, only one I1202 per function.
    let src = r#"
record R {
    deadline: time
    action go() when now < deadline {}
}
"#;
    let f = run(&I1202TimestampDependency, src);
    assert_eq!(f.len(), 1, "expected exactly one I1202 per function: {f:?}");
}

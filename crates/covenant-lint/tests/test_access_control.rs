//! Tests for ACC detectors (C100, C101, W102, W103, I104).

mod helpers;
use helpers::run;

use covenant_lint::detectors::access_control::{
    C100NoAccessGuard, C101BlockConditionAsAuth, I104SingleStepOwnerTransfer, W102AdminNoTimelock,
    W103OwnerZeroRisk,
};

// ── C100 ────────────────────────────────────────────────────────────────────

#[test]
fn c100_fires_action_with_sstore_no_guard() {
    let src = r#"
record R {
    x: amount
    action go() { x = 1 }
}
"#;
    let f = run(&C100NoAccessGuard, src);
    assert!(!f.is_empty(), "expected C100");
    assert_eq!(f[0].detector_code, "C100");
}

#[test]
fn c100_clean_with_only_guard() {
    let src = r#"
record R {
    x: amount
    action go() only caller { x = 1 }
}
"#;
    let f = run(&C100NoAccessGuard, src);
    assert!(f.is_empty(), "unexpected C100 with only guard: {f:?}");
}

#[test]
fn c100_no_sstore_no_finding() {
    let src = r#"
record R {
    action go() {}
}
"#;
    let f = run(&C100NoAccessGuard, src);
    assert!(f.is_empty());
}

#[test]
fn c100_view_not_flagged() {
    let src = r#"
record R {
    x: amount
    view get returns amount { x }
}
"#;
    let f = run(&C100NoAccessGuard, src);
    assert!(f.is_empty());
}

// ── W103 ────────────────────────────────────────────────────────────────────

#[test]
fn w103_fires_only_owner() {
    let src = r#"
record R {
    x: amount
    action go() only owner { x = 1 }
}
"#;
    let f = run(&W103OwnerZeroRisk, src);
    assert!(!f.is_empty(), "expected W103");
    assert_eq!(f[0].detector_code, "W103");
}

#[test]
fn w103_no_finding_only_caller() {
    let src = r#"
record R {
    x: amount
    action go() only caller { x = 1 }
}
"#;
    let f = run(&W103OwnerZeroRisk, src);
    assert!(f.is_empty(), "unexpected W103: {f:?}");
}

#[test]
fn w103_no_finding_no_guard() {
    let src = r#"
record R {
    action go() {}
}
"#;
    let f = run(&W103OwnerZeroRisk, src);
    assert!(f.is_empty());
}

// ── W102 ────────────────────────────────────────────────────────────────────

#[test]
fn w102_no_finding_without_admin_guard() {
    // W102 only fires on `only admin` functions: we don't use `admin` here.
    let src = r#"
record R {
    x: amount
    action go() only owner { x = 1 }
}
"#;
    let f = run(&W102AdminNoTimelock, src);
    // only(owner) is not only(admin), so no W102.
    assert!(f.is_empty(), "unexpected W102: {f:?}");
}

// ── I104 ────────────────────────────────────────────────────────────────────

#[test]
fn i104_fires_on_owner_field_write() {
    let src = r#"
record R {
    owner: address
    action set_owner(new_owner: address) only owner {
        owner = new_owner
    }
}
"#;
    let f = run(&I104SingleStepOwnerTransfer, src);
    assert!(!f.is_empty(), "expected I104");
    assert_eq!(f[0].detector_code, "I104");
}

#[test]
fn i104_no_finding_non_owner_field() {
    let src = r#"
record R {
    balance: amount
    action update(v: amount) only caller { balance = v }
}
"#;
    let f = run(&I104SingleStepOwnerTransfer, src);
    assert!(f.is_empty(), "unexpected I104: {f:?}");
}

#[test]
fn i104_no_finding_no_owner_field_exists() {
    let src = r#"
record R {
    action go() {}
}
"#;
    let f = run(&I104SingleStepOwnerTransfer, src);
    assert!(f.is_empty());
}

// ── C101 ────────────────────────────────────────────────────────────────────

#[test]
fn c101_no_finding_simple_action() {
    // Simple action without block timestamp in a branch condition.
    let src = r#"
record R {
    x: amount
    action go() only caller { x = 1 }
}
"#;
    let f = run(&C101BlockConditionAsAuth, src);
    assert!(f.is_empty(), "unexpected C101: {f:?}");
}

// ── multi-detector ──────────────────────────────────────────────────────────

#[test]
fn c100_multiple_actions_fires_once_per_unguarded() {
    let src = r#"
record R {
    x: amount
    y: amount
    action set_x() { x = 1 }
    action set_y() only caller { y = 2 }
}
"#;
    let f = run(&C100NoAccessGuard, src);
    // Only `set_x` lacks a guard: exactly one finding.
    assert_eq!(f.len(), 1, "expected exactly one C100: {f:?}");
}

#[test]
fn w103_multiple_only_owner_fires_per_function() {
    let src = r#"
record R {
    x: amount
    y: amount
    action a() only owner { x = 1 }
    action b() only owner { y = 2 }
}
"#;
    let f = run(&W103OwnerZeroRisk, src);
    assert_eq!(
        f.len(),
        2,
        "expected two W103 (one per only-owner action): {f:?}"
    );
}

#[test]
fn i104_fires_on_admin_field_write() {
    let src = r#"
record R {
    admin: address
    action set_admin(new_admin: address) only owner {
        admin = new_admin
    }
}
"#;
    let f = run(&I104SingleStepOwnerTransfer, src);
    assert!(!f.is_empty(), "expected I104 for admin field write");
}

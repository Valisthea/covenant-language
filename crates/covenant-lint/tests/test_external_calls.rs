//! Tests for EXT detectors (C300, C301, W302, W303, I304).

mod helpers;
use helpers::run;

use covenant_lint::detectors::external_calls::{
    C300TransferToZero, C301UncheckedTransferParam, I304TransferWithNoLogging, W302TransferInLoop,
    W303NoEnsureBeforeTransfer,
};

// ── C300 ────────────────────────────────────────────────────────────────────

#[test]
fn c300_no_finding_normal_transfer() {
    let src = r#"
record R {
    action pay(recipient: address) {
        transfer 10 to recipient
    }
}
"#;
    let f = run(&C300TransferToZero, src);
    // Parameter `recipient` is not LoadZeroAddress, so no finding.
    assert!(f.is_empty(), "unexpected C300: {f:?}");
}

#[test]
fn c300_no_transfer_no_finding() {
    let src = r#"
record R {
    x: amount
    action go() only caller { x = 1 }
}
"#;
    let f = run(&C300TransferToZero, src);
    assert!(f.is_empty());
}

// ── C301 ────────────────────────────────────────────────────────────────────

#[test]
fn c301_fires_unasserted_param_transfer() {
    let src = r#"
record R {
    action pay(recipient: address, amt: amount) {
        transfer amt to recipient
    }
}
"#;
    let f = run(&C301UncheckedTransferParam, src);
    assert!(!f.is_empty(), "expected C301");
    assert_eq!(f[0].detector_code, "C301");
}

#[test]
fn c301_clean_with_assert() {
    // When the function has a `when` guard (→ Assert), C301 should not fire.
    let src = r#"
record R {
    action pay(recipient: address, amt: amount) when amt > 0 {
        transfer amt to recipient
    }
}
"#;
    let f = run(&C301UncheckedTransferParam, src);
    assert!(f.is_empty(), "unexpected C301 with assert: {f:?}");
}

#[test]
fn c301_no_finding_no_transfer() {
    let src = r#"
record R {
    x: amount
    action go() only caller { x = 1 }
}
"#;
    let f = run(&C301UncheckedTransferParam, src);
    assert!(f.is_empty());
}

// ── W302 ────────────────────────────────────────────────────────────────────

#[test]
fn w302_no_finding_without_loop() {
    let src = r#"
record R {
    action pay(recipient: address) {
        transfer 1 to recipient
    }
}
"#;
    let f = run(&W302TransferInLoop, src);
    assert!(f.is_empty(), "unexpected W302 without loop: {f:?}");
}

// ── W303 ────────────────────────────────────────────────────────────────────

#[test]
fn w303_fires_transfer_no_assert() {
    let src = r#"
record R {
    action pay(recipient: address) {
        transfer 10 to recipient
    }
}
"#;
    let f = run(&W303NoEnsureBeforeTransfer, src);
    assert!(!f.is_empty(), "expected W303");
    assert_eq!(f[0].detector_code, "W303");
}

#[test]
fn w303_clean_with_when_guard() {
    // A `when` guard lowers to an Assert, making the transfer checked.
    let src = r#"
record R {
    action pay(recipient: address, amt: amount) when amt > 0 {
        transfer amt to recipient
    }
}
"#;
    let f = run(&W303NoEnsureBeforeTransfer, src);
    assert!(f.is_empty(), "unexpected W303 with when guard: {f:?}");
}

#[test]
fn w303_no_transfer_no_finding() {
    let src = r#"
record R {
    x: amount
    action go() only caller { x = 1 }
}
"#;
    let f = run(&W303NoEnsureBeforeTransfer, src);
    assert!(f.is_empty());
}

// ── I304 ────────────────────────────────────────────────────────────────────

#[test]
fn i304_fires_transfer_no_emit() {
    let src = r#"
record R {
    action pay(recipient: address) {
        transfer 10 to recipient
    }
}
"#;
    let f = run(&I304TransferWithNoLogging, src);
    assert!(!f.is_empty(), "expected I304");
    assert_eq!(f[0].detector_code, "I304");
}

#[test]
fn i304_clean_with_emit() {
    let src = r#"
record R {
    event Paid(recipient: address)
    action pay(recipient: address) {
        transfer 10 to recipient
        emit Paid(recipient)
    }
}
"#;
    let f = run(&I304TransferWithNoLogging, src);
    assert!(f.is_empty(), "unexpected I304 with emit: {f:?}");
}

#[test]
fn i304_no_finding_no_transfer() {
    let src = r#"
record R {
    x: amount
    action go() only caller { x = 1 }
}
"#;
    let f = run(&I304TransferWithNoLogging, src);
    assert!(f.is_empty());
}

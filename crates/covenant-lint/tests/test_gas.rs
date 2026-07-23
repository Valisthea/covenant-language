//! Tests for GAS detectors (C1100, W1101, W1102, I1103).

mod helpers;
use helpers::run;

use covenant_lint::detectors::gas::{
    C1100UnboundedLoop, I1103HighInstructionCount, W1101ExpensiveView, W1102StoreInLoop,
};

// ── C1100 ───────────────────────────────────────────────────────────────────

#[test]
fn c1100_fires_collection_iter_no_batch_up_to() {
    // ListLength triggers collection-iteration detection (see
    // ir_utils::has_collection_iter). This used to use `map.length`, but map
    // introspection is now refused at lowering (E425) because it silently
    // compiled to 0 — so the vehicle changed while the detector under test
    // did not.
    let src = r#"
record R {
    votes: [address] = []
    view tally returns amount {
        votes.length
    }
}
"#;
    let f = run(&C1100UnboundedLoop, src);
    assert!(!f.is_empty(), "expected C1100 for unbounded map iteration");
    assert_eq!(f[0].detector_code, "C1100");
}

#[test]
fn c1100_no_finding_without_iteration() {
    let src = r#"
record R {
    x: amount
    action go() only caller { x = 1 }
}
"#;
    let f = run(&C1100UnboundedLoop, src);
    assert!(f.is_empty(), "unexpected C1100: {f:?}");
}

#[test]
fn c1100_clean_with_batch_up_to() {
    // @batch_up_to is valid on action functions — should suppress C1100.
    let src = r#"
record R {
    votes: [address] = []
    @batch_up_to(100)
    action tally() only caller {
        votes.length
    }
}
"#;
    let f = run(&C1100UnboundedLoop, src);
    assert!(f.is_empty(), "unexpected C1100 with @batch_up_to: {f:?}");
}

// ── W1101 ───────────────────────────────────────────────────────────────────

#[test]
fn w1101_no_finding_simple_view() {
    let src = r#"
record R {
    x: amount
    view get returns amount { x }
}
"#;
    let f = run(&W1101ExpensiveView, src);
    assert!(f.is_empty(), "unexpected W1101 on simple view: {f:?}");
}

#[test]
fn w1101_no_finding_for_actions() {
    // W1101 only applies to view functions.
    let src = r#"
record R {
    x: amount
    action go() only caller { x = 1 }
}
"#;
    let f = run(&W1101ExpensiveView, src);
    assert!(f.is_empty());
}

// ── W1102 ───────────────────────────────────────────────────────────────────

#[test]
fn w1102_no_finding_without_loop() {
    let src = r#"
record R {
    x: amount
    action go() only caller { x = 1 }
}
"#;
    let f = run(&W1102StoreInLoop, src);
    assert!(f.is_empty(), "unexpected W1102 without loop: {f:?}");
}

// ── I1103 ───────────────────────────────────────────────────────────────────

#[test]
fn i1103_no_finding_small_function() {
    let src = r#"
record R {
    x: amount
    action go() only caller { x = 1 }
}
"#;
    let f = run(&I1103HighInstructionCount, src);
    assert!(f.is_empty(), "unexpected I1103 on tiny function: {f:?}");
}

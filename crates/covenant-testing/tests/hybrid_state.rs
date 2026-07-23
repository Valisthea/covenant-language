//! End-to-end scenarios for HybridState (hybrid module construct).
//!
//! Tests verify that `hybrid module` constructs compile correctly, that
//! per-field privacy qualifiers are respected, and that plain module actions
//! operate without stdlib synthesis side-effects.

use covenant_testing::{CovenantTestHarness, U256};

const HYBRID: &str =
    include_str!("../../covenant-lexer/tests/fixtures/example_10_hybrid_state.cov");

fn setup() -> (CovenantTestHarness, covenant_testing::Contract) {
    let mut h = CovenantTestHarness::new();
    let c = h
        .deploy(HYBRID, h.addrs.deployer)
        .expect("deploy HybridState");
    (h, c)
}

// ---------------------------------------------------------------------------
// Scenario 1: Deploy succeeds
// ---------------------------------------------------------------------------

#[test]
fn hybrid_state_deploys_successfully() {
    let (h, c) = setup();
    assert!(
        !h.artifact(c).runtime_bytecode.is_empty(),
        "runtime bytecode must be non-empty"
    );
}

// ---------------------------------------------------------------------------
// Scenario 2: Expected selectors present
// ---------------------------------------------------------------------------

#[test]
fn hybrid_state_has_expected_selectors() {
    let (h, c) = setup();
    let sels = &h.artifact(c).function_selectors;
    for name in [
        "set_treasury",
        "hire",
        "release",
        "count",
        "get_treasury",
        "get_locked",
    ] {
        assert!(
            sels.contains_key(name),
            "missing selector `{name}`; present: {:?}",
            sels.keys().collect::<Vec<_>>()
        );
    }
}

// ---------------------------------------------------------------------------
// Scenario 3: Initial values are zero
// ---------------------------------------------------------------------------

#[test]
fn hybrid_state_initial_values_are_zero() {
    let (mut h, c) = setup();
    let alice = h.addrs.alice;
    let count = h.view_u256(c, alice, "count()", &[]);
    assert_eq!(count, U256::ZERO, "initial headcount must be 0");
    let locked = h.view_u256(c, alice, "get_locked()", &[]);
    assert_eq!(locked, U256::ZERO, "initial locked must be 0");
}

// ---------------------------------------------------------------------------
// Scenario 4: hire() increments headcount
// ---------------------------------------------------------------------------

#[test]
fn hybrid_state_hire_increments_count() {
    let (mut h, c) = setup();
    let alice = h.addrs.alice;
    let _ = h.call_ok(c, alice, "hire()", &[]);
    let _ = h.call_ok(c, alice, "hire()", &[]);
    let count = h.view_u256(c, alice, "count()", &[]);
    assert_eq!(
        count,
        U256::from_u64(2),
        "headcount must be 2 after 2 hires"
    );
}

// ---------------------------------------------------------------------------
// Scenario 5: set_treasury + get_treasury round-trips address
// ---------------------------------------------------------------------------

#[test]
fn hybrid_state_set_treasury_roundtrips() {
    let (mut h, c) = setup();
    let alice = h.addrs.alice;
    let deployer = h.addrs.deployer;
    let _ = h.call_ok(c, deployer, "set_treasury(address)", &[alice.to_u256()]);
    let stored = h.view_u256(c, alice, "get_treasury()", &[]);
    assert_eq!(stored, alice.to_u256(), "get_treasury must return alice");
}

// ---------------------------------------------------------------------------
// Scenario 6: release() reverts when headcount is zero (when guard)
// ---------------------------------------------------------------------------

#[test]
fn hybrid_state_release_reverts_at_zero() {
    let (mut h, c) = setup();
    let alice = h.addrs.alice;
    let result = h.call(c, alice, "release()", &[]);
    assert!(
        result.is_revert(),
        "release() on empty headcount must revert"
    );
}

// ---------------------------------------------------------------------------
// Scenario 7: hire() then release() balances count
// ---------------------------------------------------------------------------

#[test]
fn hybrid_state_hire_then_release_balances() {
    let (mut h, c) = setup();
    let alice = h.addrs.alice;
    let _ = h.call_ok(c, alice, "hire()", &[]);
    let _ = h.call_ok(c, alice, "hire()", &[]);
    let _ = h.call_ok(c, alice, "release()", &[]);
    let count = h.view_u256(c, alice, "count()", &[]);
    assert_eq!(
        count,
        U256::from_u64(1),
        "headcount must be 1 after 2 hires and 1 release"
    );
}

// ---------------------------------------------------------------------------
// Scenario 8: unknown selector reverts
// ---------------------------------------------------------------------------

#[test]
fn hybrid_state_unknown_selector_reverts() {
    let (mut h, c) = setup();
    let result = h.raw_call(
        c,
        h.addrs.alice,
        vec![0xde, 0xad, 0xbe, 0xef],
        U256::ZERO,
        false,
    );
    assert!(result.is_revert(), "unknown selector must revert");
}

//! End-to-end scenarios for EncryptedBridge (cross-chain escrow module).
//!
//! Tests exercise locking assets, checking balances, and unlocking.
//! The `when deposits[caller] >= value` guard prevents over-withdrawal.

use covenant_testing::{CovenantTestHarness, U256};

const BRIDGE: &str =
    include_str!("../../covenant-lexer/tests/fixtures/example_09_encrypted_bridge.cov");

fn setup() -> (CovenantTestHarness, covenant_testing::Contract) {
    let mut h = CovenantTestHarness::new();
    let c = h
        .deploy(BRIDGE, h.addrs.deployer)
        .expect("deploy EncryptedBridge");
    (h, c)
}

// ---------------------------------------------------------------------------
// Scenario 1: Deploy succeeds
// ---------------------------------------------------------------------------

#[test]
fn bridge_deploys_successfully() {
    let (h, c) = setup();
    assert!(!h.artifact(c).runtime_bytecode.is_empty());
}

// ---------------------------------------------------------------------------
// Scenario 2: Expected selectors present
// ---------------------------------------------------------------------------

#[test]
fn bridge_has_expected_selectors() {
    let (h, c) = setup();
    let sels = &h.artifact(c).function_selectors;
    for name in ["lock", "unlock", "locked_by", "total_locked"] {
        assert!(
            sels.contains_key(name),
            "missing selector `{name}`; present: {:?}",
            sels.keys().collect::<Vec<_>>()
        );
    }
}

// ---------------------------------------------------------------------------
// Scenario 3: Initial locked balance is zero
// ---------------------------------------------------------------------------

#[test]
fn bridge_initial_balance_is_zero() {
    let (mut h, c) = setup();
    let alice = h.addrs.alice;
    let bal = h.view_u256(c, alice, "locked_by(address)", &[alice.to_u256()]);
    assert_eq!(bal, U256::ZERO, "initial locked balance must be zero");
}

// ---------------------------------------------------------------------------
// Scenario 4: lock() increases balance
// ---------------------------------------------------------------------------

#[test]
fn bridge_lock_increases_balance() {
    let (mut h, c) = setup();
    let alice = h.addrs.alice;
    let _ = h.call_ok(c, alice, "lock(uint256)", &[U256::from_u64(500)]);
    let bal = h.view_u256(c, alice, "locked_by(address)", &[alice.to_u256()]);
    assert_eq!(
        bal,
        U256::from_u64(500),
        "locked balance must be 500 after locking 500"
    );
}

// ---------------------------------------------------------------------------
// Scenario 5: total_locked tracks sum of all deposits
// ---------------------------------------------------------------------------

#[test]
fn bridge_total_locked_tracks_multiple_depositors() {
    let (mut h, c) = setup();
    let alice = h.addrs.alice;
    let bob = h.addrs.bob;
    let _ = h.call_ok(c, alice, "lock(uint256)", &[U256::from_u64(300)]);
    let _ = h.call_ok(c, bob, "lock(uint256)", &[U256::from_u64(700)]);
    let total = h.view_u256(c, alice, "total_locked()", &[]);
    assert_eq!(total, U256::from_u64(1000), "total locked must be 1000");
}

// ---------------------------------------------------------------------------
// Scenario 6: unlock() decreases balance
// ---------------------------------------------------------------------------

#[test]
fn bridge_unlock_decreases_balance() {
    let (mut h, c) = setup();
    let alice = h.addrs.alice;
    let _ = h.call_ok(c, alice, "lock(uint256)", &[U256::from_u64(1000)]);
    let _ = h.call_ok(c, alice, "unlock(uint256)", &[U256::from_u64(400)]);
    let bal = h.view_u256(c, alice, "locked_by(address)", &[alice.to_u256()]);
    assert_eq!(
        bal,
        U256::from_u64(600),
        "locked balance must be 600 after unlocking 400"
    );
}

// ---------------------------------------------------------------------------
// Scenario 7: unlock() reverts when balance is insufficient
// ---------------------------------------------------------------------------

#[test]
fn bridge_unlock_reverts_when_insufficient() {
    let (mut h, c) = setup();
    let alice = h.addrs.alice;
    let _ = h.call_ok(c, alice, "lock(uint256)", &[U256::from_u64(100)]);
    // Attempt to unlock more than locked — the `when` guard must revert.
    let result = h.call(c, alice, "unlock(uint256)", &[U256::from_u64(200)]);
    assert!(
        result.is_revert(),
        "unlock of 200 from balance 100 must revert"
    );
}

// ---------------------------------------------------------------------------
// Scenario 8: Multiple depositors are independent
// ---------------------------------------------------------------------------

#[test]
fn bridge_depositors_are_independent() {
    let (mut h, c) = setup();
    let alice = h.addrs.alice;
    let bob = h.addrs.bob;
    let _ = h.call_ok(c, alice, "lock(uint256)", &[U256::from_u64(200)]);
    let _ = h.call_ok(c, bob, "lock(uint256)", &[U256::from_u64(800)]);

    let a_bal = h.view_u256(c, alice, "locked_by(address)", &[alice.to_u256()]);
    let b_bal = h.view_u256(c, alice, "locked_by(address)", &[bob.to_u256()]);
    assert_eq!(a_bal, U256::from_u64(200), "alice's balance must be 200");
    assert_eq!(b_bal, U256::from_u64(800), "bob's balance must be 800");
}

// ---------------------------------------------------------------------------
// Scenario 9: Unknown selector reverts
// ---------------------------------------------------------------------------

#[test]
fn bridge_unknown_selector_reverts() {
    let (mut h, c) = setup();
    let result = h.raw_call(
        c,
        h.addrs.alice,
        vec![0x13, 0x37, 0x00, 0x00],
        U256::ZERO,
        false,
    );
    assert!(result.is_revert(), "unknown selector must revert");
}

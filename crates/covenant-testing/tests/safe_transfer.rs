//! End-to-end scenarios for SafeTransfer (typed errors + revert_with).
//!
//! Tests verify:
//!   - Error declarations compile and appear in the ABI
//!   - `revert_with ErrorName(args)` produces ABI-encoded revert data
//!   - The 4-byte error selector matches keccak256(signature)[0..4]
//!   - Args are ABI-encoded after the selector
//!   - Normal actions (deposit/withdraw) continue to work

use covenant_testing::{abi, CovenantTestHarness, U256};

const SAFE: &str = include_str!("../../covenant-lexer/tests/fixtures/example_11_safe_transfer.cov");

fn setup() -> (CovenantTestHarness, covenant_testing::Contract) {
    let mut h = CovenantTestHarness::new();
    let c = h
        .deploy(SAFE, h.addrs.deployer)
        .expect("deploy SafeTransfer");
    (h, c)
}

// ---------------------------------------------------------------------------
// Scenario 1: Deploy succeeds
// ---------------------------------------------------------------------------

#[test]
fn safe_transfer_deploys_successfully() {
    let (h, c) = setup();
    assert!(!h.artifact(c).runtime_bytecode.is_empty());
}

// ---------------------------------------------------------------------------
// Scenario 2: Expected selectors present
// ---------------------------------------------------------------------------

#[test]
fn safe_transfer_has_expected_selectors() {
    let (h, c) = setup();
    let sels = &h.artifact(c).function_selectors;
    for name in [
        "initialize",
        "deposit",
        "safe_withdraw",
        "force_insufficient",
        "force_unauthorized",
        "force_zero_amount",
        "balance_of",
        "get_owner",
        "get_total",
    ] {
        assert!(
            sels.contains_key(name),
            "missing selector `{name}`; present: {:?}",
            sels.keys().collect::<Vec<_>>()
        );
    }
}

// ---------------------------------------------------------------------------
// Scenario 3: Zero-argument error revert contains correct 4-byte selector
// ---------------------------------------------------------------------------

#[test]
fn safe_transfer_zero_arg_error_has_correct_selector() {
    let (mut h, c) = setup();
    let alice = h.addrs.alice;

    // ZeroAmount() selector
    let expected_sel = abi::selector("ZeroAmount()");

    let data = h.call_revert(c, alice, "force_zero_amount()", &[]);
    assert!(
        data.len() >= 4,
        "revert data must have at least 4 bytes for selector; got {}",
        data.len()
    );
    assert_eq!(
        &data[0..4],
        &expected_sel,
        "ZeroAmount() error selector mismatch: got {:?}, expected {:?}",
        &data[0..4],
        expected_sel
    );
}

// ---------------------------------------------------------------------------
// Scenario 4: Two-argument error encodes selector + args correctly
// ---------------------------------------------------------------------------

#[test]
fn safe_transfer_two_arg_error_encodes_selector_and_args() {
    let (mut h, c) = setup();
    let alice = h.addrs.alice;

    let required = U256::from_u64(500);
    let actual = U256::from_u64(100);

    // InsufficientBalance(uint256,uint256) selector
    let expected_sel = abi::selector("InsufficientBalance(uint256,uint256)");

    let data = h.call_revert(
        c,
        alice,
        "force_insufficient(uint256,uint256)",
        &[required, actual],
    );

    // Must be at least 4 + 32 + 32 = 68 bytes.
    assert!(
        data.len() >= 68,
        "revert data for InsufficientBalance must be ≥68 bytes; got {}",
        data.len()
    );

    // Selector check.
    assert_eq!(
        &data[0..4],
        &expected_sel,
        "InsufficientBalance selector mismatch: got {:?}",
        &data[0..4]
    );

    // First arg: required = 500 (as big-endian uint256 at bytes 4..36).
    let decoded_required = U256::from_be_bytes(data[4..36].try_into().unwrap());
    assert_eq!(
        decoded_required, required,
        "first arg (required) must be 500"
    );

    // Second arg: actual = 100 (as big-endian uint256 at bytes 36..68).
    let decoded_actual = U256::from_be_bytes(data[36..68].try_into().unwrap());
    assert_eq!(decoded_actual, actual, "second arg (actual) must be 100");
}

// ---------------------------------------------------------------------------
// Scenario 5: Address-typed error arg encodes the caller address
// ---------------------------------------------------------------------------

#[test]
fn safe_transfer_address_arg_error_encodes_caller() {
    let (mut h, c) = setup();
    let alice = h.addrs.alice;

    // Unauthorized(address) selector
    let expected_sel = abi::selector("Unauthorized(address)");

    let data = h.call_revert(c, alice, "force_unauthorized()", &[]);

    // Must be at least 4 + 32 = 36 bytes.
    assert!(
        data.len() >= 36,
        "revert data for Unauthorized must be ≥36 bytes; got {}",
        data.len()
    );

    // Selector check.
    assert_eq!(
        &data[0..4],
        &expected_sel,
        "Unauthorized selector mismatch: got {:?}",
        &data[0..4]
    );

    // Arg: caller address — ABI-encoded as 32 bytes (left-padded, 20 bytes right).
    // alice's address should be in the last 20 bytes of data[4..36].
    let alice_bytes = alice.to_u256().to_be_bytes();
    assert_eq!(
        &data[4..36],
        &alice_bytes,
        "Unauthorized arg must be alice's address"
    );
}

// ---------------------------------------------------------------------------
// Scenario 6: Normal deposit+withdraw flow still works
// ---------------------------------------------------------------------------

#[test]
fn safe_transfer_deposit_and_withdraw() {
    let (mut h, c) = setup();
    let alice = h.addrs.alice;
    let _ = h.call_ok(c, alice, "deposit(uint256)", &[U256::from_u64(1000)]);
    let bal = h.view_u256(c, alice, "balance_of(address)", &[alice.to_u256()]);
    assert_eq!(bal, U256::from_u64(1000));

    let _ = h.call_ok(c, alice, "safe_withdraw(uint256)", &[U256::from_u64(400)]);
    let bal2 = h.view_u256(c, alice, "balance_of(address)", &[alice.to_u256()]);
    assert_eq!(bal2, U256::from_u64(600));
}

// ---------------------------------------------------------------------------
// Scenario 7: safe_withdraw reverts when balance insufficient (when guard)
// ---------------------------------------------------------------------------

#[test]
fn safe_transfer_withdraw_reverts_when_insufficient() {
    let (mut h, c) = setup();
    let alice = h.addrs.alice;
    let _ = h.call_ok(c, alice, "deposit(uint256)", &[U256::from_u64(50)]);
    let result = h.call(c, alice, "safe_withdraw(uint256)", &[U256::from_u64(100)]);
    assert!(result.is_revert(), "withdraw beyond balance must revert");
}

// ---------------------------------------------------------------------------
// Scenario 8: get_total tracks sum of deposits
// ---------------------------------------------------------------------------

#[test]
fn safe_transfer_total_tracks_deposits() {
    let (mut h, c) = setup();
    let alice = h.addrs.alice;
    let bob = h.addrs.bob;
    let _ = h.call_ok(c, alice, "deposit(uint256)", &[U256::from_u64(300)]);
    let _ = h.call_ok(c, bob, "deposit(uint256)", &[U256::from_u64(700)]);
    let total = h.view_u256(c, alice, "get_total()", &[]);
    assert_eq!(total, U256::from_u64(1000));
}

// ---------------------------------------------------------------------------
// KSR-CVN-012: second initialize() call reverts
// ---------------------------------------------------------------------------
//
// The proxy hijack attack works by calling initialize() a second time to
// rebind owner to an attacker-controlled address. The re-init guard in
// emit_function (codegen.rs) must reject the second call.

#[test]
fn second_initialize_reverts_ksr_cvn_012() {
    let (mut h, c) = setup();
    let alice = h.addrs.alice;
    let bob = h.addrs.bob;

    // First initialize(alice) — expected to succeed and set owner = alice.
    let _ = h.call_ok(
        c,
        h.addrs.deployer,
        "initialize(address)",
        &[alice.to_u256()],
    );
    let owner_after_first = h.view_u256(c, alice, "get_owner()", &[]);
    assert_eq!(
        owner_after_first,
        alice.to_u256(),
        "first initialize must set owner to alice"
    );

    // Second initialize(bob) — the attack. Must revert, not rebind owner.
    let result = h.call(c, bob, "initialize(address)", &[bob.to_u256()]);
    assert!(
        result.is_revert(),
        "second call to initialize(...) must revert (KSR-CVN-012)"
    );

    // Owner must still be alice after the failed hijack attempt.
    let owner_after_second = h.view_u256(c, alice, "get_owner()", &[]);
    assert_eq!(
        owner_after_second,
        alice.to_u256(),
        "owner must be unchanged after the second initialize attempt"
    );
}

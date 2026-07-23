//! KSR-CVN-011 end-to-end scenarios — `only <principal>` runtime enforcement.
//!
//! Prior to this fix, an `only owner` / `only admin` / `only deployer` /
//! `only <address-literal>` guard compiled down to `Assert(true)`, i.e. a
//! no-op. Any EOA could invoke a guarded action. These tests deploy a small
//! contract and verify that unauthorized callers revert and authorized
//! callers succeed.

use covenant_testing::{CovenantTestHarness, U256};

const ONLY_OWNER: &str = r#"
module Vault {
    field owner: address
    field balance: amount

    action initialize(who: address) {
        owner = who
    }

    action set_balance(v: amount) only owner {
        balance = v
    }

    view get_owner() returns address { owner }
    view get_balance() returns amount { balance }
}
"#;

const ONLY_ADMIN: &str = r#"
module AdminVault {
    field admin: address
    field flag: amount

    action initialize(who: address) {
        admin = who
    }

    action set_flag(v: amount) only admin {
        flag = v
    }

    view get_admin() returns address { admin }
    view get_flag() returns amount { flag }
}
"#;

// ---------------------------------------------------------------------------
// `only owner`
// ---------------------------------------------------------------------------

#[test]
fn only_owner_rejects_non_owner_caller() {
    let mut h = CovenantTestHarness::new();
    let c = h.deploy(ONLY_OWNER, h.addrs.deployer).expect("deploy");

    let alice = h.addrs.alice;
    let bob = h.addrs.bob;

    // Initialize sets owner = alice.
    let _ = h.call_ok(
        c,
        h.addrs.deployer,
        "initialize(address)",
        &[alice.to_u256()],
    );

    // Bob (not owner) must be rejected by the `only owner` guard.
    let result = h.call(c, bob, "set_balance(uint256)", &[U256::from_u64(42)]);
    assert!(
        result.is_revert(),
        "`only owner` must revert when caller != owner (bob called, alice is owner)"
    );

    // Balance must remain unchanged at zero.
    let bal = h.view_u256(c, alice, "get_balance()", &[]);
    assert_eq!(
        bal,
        U256::ZERO,
        "unauthorised caller must not mutate state under `only owner`"
    );
}

#[test]
fn only_owner_allows_owner_caller() {
    let mut h = CovenantTestHarness::new();
    let c = h.deploy(ONLY_OWNER, h.addrs.deployer).expect("deploy");

    let alice = h.addrs.alice;

    let _ = h.call_ok(
        c,
        h.addrs.deployer,
        "initialize(address)",
        &[alice.to_u256()],
    );

    // Alice is owner — must succeed.
    let _ = h.call_ok(c, alice, "set_balance(uint256)", &[U256::from_u64(999)]);

    let bal = h.view_u256(c, alice, "get_balance()", &[]);
    assert_eq!(bal, U256::from_u64(999));
}

// ---------------------------------------------------------------------------
// `only admin`
// ---------------------------------------------------------------------------

#[test]
fn only_admin_rejects_non_admin_caller() {
    let mut h = CovenantTestHarness::new();
    let c = h.deploy(ONLY_ADMIN, h.addrs.deployer).expect("deploy");

    let alice = h.addrs.alice;
    let bob = h.addrs.bob;

    let _ = h.call_ok(
        c,
        h.addrs.deployer,
        "initialize(address)",
        &[alice.to_u256()],
    );

    let result = h.call(c, bob, "set_flag(uint256)", &[U256::from_u64(7)]);
    assert!(
        result.is_revert(),
        "`only admin` must revert when caller != admin"
    );

    let flag = h.view_u256(c, alice, "get_flag()", &[]);
    assert_eq!(flag, U256::ZERO, "unauthorised caller must not mutate flag");
}

#[test]
fn only_admin_allows_admin_caller() {
    let mut h = CovenantTestHarness::new();
    let c = h.deploy(ONLY_ADMIN, h.addrs.deployer).expect("deploy");

    let alice = h.addrs.alice;

    let _ = h.call_ok(
        c,
        h.addrs.deployer,
        "initialize(address)",
        &[alice.to_u256()],
    );
    let _ = h.call_ok(c, alice, "set_flag(uint256)", &[U256::from_u64(7)]);

    let flag = h.view_u256(c, alice, "get_flag()", &[]);
    assert_eq!(flag, U256::from_u64(7));
}

// ---------------------------------------------------------------------------
// Ownership rotation — new owner can call, old owner cannot
// ---------------------------------------------------------------------------
// Demonstrates that the guard reads the current SSTORE-backed field on every
// invocation rather than caching an early value.

#[test]
fn only_owner_follows_state_rotation() {
    let src = r#"
module Rotating {
    field owner: address
    field x: amount

    action initialize(who: address) { owner = who }
    action rotate(next: address) only owner { owner = next }
    action set_x(v: amount) only owner { x = v }
    view get_owner() returns address { owner }
}
"#;
    let mut h = CovenantTestHarness::new();
    let c = h.deploy(src, h.addrs.deployer).expect("deploy");

    let alice = h.addrs.alice;
    let bob = h.addrs.bob;

    // owner = alice
    let _ = h.call_ok(
        c,
        h.addrs.deployer,
        "initialize(address)",
        &[alice.to_u256()],
    );
    // alice can rotate ownership to bob
    let _ = h.call_ok(c, alice, "rotate(address)", &[bob.to_u256()]);

    // After rotation, alice (old owner) must be rejected.
    let result = h.call(c, alice, "set_x(uint256)", &[U256::from_u64(1)]);
    assert!(
        result.is_revert(),
        "after rotation, former owner must not retain authority"
    );

    // Bob (new owner) must succeed.
    let _ = h.call_ok(c, bob, "set_x(uint256)", &[U256::from_u64(2)]);
}

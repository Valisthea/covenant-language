//! End-to-end checked-arithmetic tests (V0.9.2, KSR-CVN-031).
//!
//! Covenant's `+` / `-` lower to AddChecked / SubChecked. Before V0.9.2 those
//! lowered to bare EVM ADD/SUB, which WRAP the 256-bit word on overflow /
//! underflow instead of reverting: so a `balance = balance - amount` would
//! silently wrap to ~type(uint256).max. These tests deploy a tiny record and
//! drive the runtime through the mini-EVM interpreter.

use covenant_testing::{CovenantTestHarness, U256};

const SRC: &str = r#"
record CheckedMath {
    n: amount

    action inc() { n = n + 1 }
    action dec() { n = n - 1 }

    view get returns amount { n }
}
"#;

#[test]
fn subtraction_underflow_reverts() {
    let mut h = CovenantTestHarness::new();
    let c = h.deploy(SRC, h.addrs.deployer).expect("deploy");
    let alice = h.addrs.alice;

    // n starts at 0; dec() computes 0 - 1, which must REVERT (not wrap to
    // type(uint256).max).
    h.call_revert(c, alice, "dec()", &[]);
    // State unchanged after the reverted call.
    assert_eq!(h.view_u256(c, alice, "get()", &[]), U256::ZERO);
}

#[test]
fn addition_and_subtraction_roundtrip() {
    let mut h = CovenantTestHarness::new();
    let c = h.deploy(SRC, h.addrs.deployer).expect("deploy");
    let alice = h.addrs.alice;

    h.call_ok(c, alice, "inc()", &[]);
    h.call_ok(c, alice, "inc()", &[]);
    h.call_ok(c, alice, "inc()", &[]);
    assert_eq!(h.view_u256(c, alice, "get()", &[]), U256::from_u64(3));

    h.call_ok(c, alice, "dec()", &[]);
    assert_eq!(h.view_u256(c, alice, "get()", &[]), U256::from_u64(2));

    // Drain back to zero, then the next dec() underflows and reverts.
    h.call_ok(c, alice, "dec()", &[]);
    h.call_ok(c, alice, "dec()", &[]);
    assert_eq!(h.view_u256(c, alice, "get()", &[]), U256::ZERO);
    h.call_revert(c, alice, "dec()", &[]);
}

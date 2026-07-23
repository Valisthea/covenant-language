//! End-to-end division-by-zero tests (fail-loud pass).
//!
//! `/` and `%` lower to `Opcode::Div` / `Opcode::Mod`, which used to become
//! bare EVM `DIV`/`MOD`. Those are TOTAL functions on the EVM: `x / 0` yields
//! `0` instead of trapping. So `payout = pot / participants` silently paid 0
//! to everybody when the participant set was empty — ordinary-looking source,
//! wrong on-chain result, no diagnostic. Solidity reverts here.
//!
//! These deploy a tiny record and drive the runtime through the mini-EVM
//! interpreter, whose `U256::div`/`rem` faithfully reproduce the EVM's
//! zero-divisor behaviour (covenant-evm-runtime/src/u256.rs) — so each test
//! observes the OLD wrong value before the fix and the revert after it.

use covenant_testing::{CovenantTestHarness, U256};

const SRC: &str = r#"
record Divider {
    n: amount
    d: amount

    action set(a: amount, b: amount) {
        n = a
        d = b
    }

    view quotient returns amount {
        n / d
    }

    view remainder returns amount {
        n % d
    }
}
"#;

#[test]
fn division_by_zero_reverts_instead_of_yielding_zero() {
    let mut h = CovenantTestHarness::new();
    let c = h.deploy(SRC, h.addrs.deployer).expect("deploy");
    let alice = h.addrs.alice;

    // 100 / 0 — before the fix this returned 0 with no error.
    h.call_ok(
        c,
        alice,
        "set(uint256,uint256)",
        &[U256::from_u64(100), U256::ZERO],
    );
    h.call_revert(c, alice, "quotient()", &[]);
}

#[test]
fn remainder_by_zero_reverts_instead_of_yielding_zero() {
    let mut h = CovenantTestHarness::new();
    let c = h.deploy(SRC, h.addrs.deployer).expect("deploy");
    let alice = h.addrs.alice;

    h.call_ok(
        c,
        alice,
        "set(uint256,uint256)",
        &[U256::from_u64(100), U256::ZERO],
    );
    h.call_revert(c, alice, "remainder()", &[]);
}

/// The guard must not change the result of ordinary, non-zero division.
#[test]
fn nonzero_division_is_unaffected() {
    let mut h = CovenantTestHarness::new();
    let c = h.deploy(SRC, h.addrs.deployer).expect("deploy");
    let alice = h.addrs.alice;

    h.call_ok(
        c,
        alice,
        "set(uint256,uint256)",
        &[U256::from_u64(100), U256::from_u64(7)],
    );
    assert_eq!(h.view_u256(c, alice, "quotient()", &[]), U256::from_u64(14));
    assert_eq!(h.view_u256(c, alice, "remainder()", &[]), U256::from_u64(2));
}

/// Exact-division boundary: no off-by-one introduced by the guard.
#[test]
fn exact_division_is_unaffected() {
    let mut h = CovenantTestHarness::new();
    let c = h.deploy(SRC, h.addrs.deployer).expect("deploy");
    let alice = h.addrs.alice;

    h.call_ok(
        c,
        alice,
        "set(uint256,uint256)",
        &[U256::from_u64(100), U256::from_u64(4)],
    );
    assert_eq!(h.view_u256(c, alice, "quotient()", &[]), U256::from_u64(25));
    assert_eq!(h.view_u256(c, alice, "remainder()", &[]), U256::ZERO);
}

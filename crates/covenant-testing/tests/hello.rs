//! End-to-end scenarios for the Hello Basics example, the minimal record.

use covenant_testing::{CovenantTestHarness, U256};

const HELLO: &str = include_str!("../../covenant-lexer/tests/fixtures/example_01_hello.cov");

#[test]
fn deploys_successfully() {
    let mut h = CovenantTestHarness::new();
    let c = h.deploy(HELLO, h.addrs.deployer).expect("deploy hello");
    assert!(!h.artifact(c).runtime_bytecode.is_empty());
}

#[test]
fn has_update_and_read_selectors() {
    let mut h = CovenantTestHarness::new();
    let c = h.deploy(HELLO, h.addrs.deployer).expect("deploy hello");
    let sels = &h.artifact(c).function_selectors;
    assert!(sels.contains_key("update"));
    assert!(sels.contains_key("read"));
}

#[test]
fn read_before_write_returns_zero() {
    let mut h = CovenantTestHarness::new();
    let c = h.deploy(HELLO, h.addrs.deployer).expect("deploy hello");
    // `greeting` is uninitialized → SLOAD returns zero.
    let v = h.view_u256(c, h.addrs.alice, "read()", &[]);
    assert_eq!(v, U256::ZERO);
}

#[test]
fn update_does_not_revert() {
    // V0.1: text parameters lower to placeholder zero, so this merely tests
    // that the action path compiles, dispatches, and returns successfully.
    let mut h = CovenantTestHarness::new();
    let c = h.deploy(HELLO, h.addrs.deployer).expect("deploy hello");
    let _ = h.call_ok(c, h.addrs.alice, "update(string)", &[U256::ZERO]);
}

#[test]
fn unknown_selector_reverts() {
    let mut h = CovenantTestHarness::new();
    let c = h.deploy(HELLO, h.addrs.deployer).expect("deploy hello");
    let result = h.raw_call(
        c,
        h.addrs.alice,
        vec![0xba, 0xd0, 0x00, 0x00],
        U256::ZERO,
        false,
    );
    assert!(result.is_revert());
}

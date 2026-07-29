//! End-to-end scenarios for the ShieldedCounter FHE Basics example.
//!
//! ShieldedCounter stores a `ciphertext<amount>` and exposes `bump(by)` +
//! `reveal total to owner`. `bump` must go through the FHE `add` precompile
//! rather than a plain ADD opcode: we verify that by watching the mock
//! precompile state for handle minting.

use covenant_testing::{CovenantTestHarness, U256};

const COUNTER: &str =
    include_str!("../../covenant-lexer/tests/fixtures/example_04_shielded_counter.cov");

#[test]
fn deploys_successfully() {
    let mut h = CovenantTestHarness::new();
    let c = h.deploy(COUNTER, h.addrs.deployer).expect("deploy counter");
    assert!(!h.artifact(c).runtime_bytecode.is_empty());
}

#[test]
fn has_bump_and_total_selectors() {
    let mut h = CovenantTestHarness::new();
    let c = h.deploy(COUNTER, h.addrs.deployer).expect("deploy counter");
    let sels = &h.artifact(c).function_selectors;
    assert!(
        sels.contains_key("bump"),
        "missing bump: keys={:?}",
        sels.keys().collect::<Vec<_>>()
    );
    // The reveal is named `total` (matches `reveal total`).
    assert!(sels.contains_key("total"));
}

#[test]
fn bump_invokes_fhe_precompile() {
    let mut h = CovenantTestHarness::new();
    let c = h.deploy(COUNTER, h.addrs.deployer).expect("deploy counter");
    let handles_before = h.precompiles.fhe_handles.len();
    let _ = h.call_ok(c, h.addrs.alice, "bump(uint256)", &[U256::from_u64(5)]);
    // `bump` should mint at least one fresh handle through the FHE add path.
    assert!(
        h.precompiles.fhe_handles.len() > handles_before,
        "expected FHE handles to grow; before={handles_before} after={}",
        h.precompiles.fhe_handles.len()
    );
}

#[test]
fn multiple_bumps_keep_growing_handle_table() {
    let mut h = CovenantTestHarness::new();
    let c = h.deploy(COUNTER, h.addrs.deployer).expect("deploy counter");
    let _ = h.call_ok(c, h.addrs.alice, "bump(uint256)", &[U256::from_u64(1)]);
    let n1 = h.precompiles.fhe_handles.len();
    let _ = h.call_ok(c, h.addrs.alice, "bump(uint256)", &[U256::from_u64(2)]);
    let n2 = h.precompiles.fhe_handles.len();
    assert!(n2 > n1);
}

#[test]
fn unknown_selector_reverts() {
    let mut h = CovenantTestHarness::new();
    let c = h.deploy(COUNTER, h.addrs.deployer).expect("deploy counter");
    let result = h.raw_call(c, h.addrs.alice, vec![0xff; 4], U256::ZERO, false);
    assert!(result.is_revert());
}

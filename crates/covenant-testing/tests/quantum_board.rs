//! End-to-end scenarios for the QuantumBoard PQ Basics example.
//!
//! QuantumBoard exercises the PQ precompile suite (ERC-8231). It accepts
//! Dilithium-signed `submit` posts and a classical `register` step.
//! The mock `verify_dilithium` precompile returns true unless
//! `precompiles.pq_force_fail` is set.
//!
//! OMEGA V6 CRT-004 fix (E518): this fixture's `submit` action is gated
//! `only registered_key`, a BuiltinPredicate guard that used to compile to
//! an unconditional `push 1` (a complete, undiagnosed authorization
//! bypass -- anyone could call `submit` without ever registering a key).
//! It now correctly refuses to compile until the predicate has a real EVM
//! lowering (E518, same pattern as E516/E517 for amnesia/VDF primitives),
//! so this fixture cannot deploy at all until that lands. The
//! deploy-dependent tests below were replaced with a single test asserting
//! the new, correct rejection.

use covenant_testing::CovenantTestHarness;

const BOARD: &str =
    include_str!("../../covenant-lexer/tests/fixtures/example_05_quantum_board.cov");

#[test]
fn deploy_rejects_unlowered_registered_key_predicate() {
    let mut h = CovenantTestHarness::new();
    let err = h
        .deploy(BOARD, h.addrs.deployer)
        .expect_err("registered_key is not yet lowered; deploy must fail");
    assert!(
        err.iter().any(|d| d.code.0 == 518),
        "expected E518 (unlowered BuiltinPredicate), got {err:?}"
    );
}

#[test]
fn pq_force_fail_flag_propagates() {
    // Direct-call the precompile to verify the mock respects `pq_force_fail`.
    // (Going through a full submit() path requires calldata-into-params,
    // which V0.1 codegen does not yet wire up.)
    use covenant_testing::precompiles::{addr, dispatch, MockPrecompileState};

    let mut st = MockPrecompileState::default();
    assert_eq!(
        dispatch(addr::PQ_VERIFY_DILITHIUM, &[], &mut st)
            .last()
            .copied(),
        Some(1u8),
        "default should verify OK"
    );
    st.pq_force_fail = true;
    assert_eq!(
        dispatch(addr::PQ_VERIFY_DILITHIUM, &[], &mut st)
            .last()
            .copied(),
        Some(0u8),
        "with force_fail, should reject"
    );
}

// `count_returns_zero_initially` and `unknown_selector_reverts` were removed
// -- both depended on BOARD deploying successfully, which it correctly no
// longer does (see `deploy_rejects_unlowered_registered_key_predicate`
// above).

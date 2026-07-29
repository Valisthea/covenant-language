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
//!
//! V0.9.6 F-13 then added an EARLIER refusal to the same fixture (E430/E431 for
//! the unbacked `posts` collection), which is raised during IR construction and
//! therefore preempts the codegen-stage E518. The predicate assertion moved to
//! `registered_key_predicate_is_refused`, which uses a source that reaches
//! codegen, so the CRT-004 protection is unaffected by that reordering.

use covenant_testing::CovenantTestHarness;

const BOARD: &str =
    include_str!("../../covenant-lexer/tests/fixtures/example_05_quantum_board.cov");

/// `only registered_key` with nothing else wrong: a construct whose state is a
/// plain backed field, so the pipeline reaches codegen and the predicate is the
/// only thing left to refuse. Deliberately not the QuantumBoard fixture, which
/// stops earlier (see `deploy_is_rejected` below).
const REGISTERED_KEY_ONLY: &str = r#"
record Guarded {
    field keys: map<address, bool>

    action ping()
            only registered_key {
        keys[caller] = true
    }
}
"#;

#[test]
fn registered_key_predicate_is_refused() {
    // The CRT-004 protection proper: `only registered_key` used to compile to
    // an unconditional `push 1`, so anyone could call the action without ever
    // registering a key, with no diagnostic anywhere. This is the test that
    // fails if that regresses. It carries the coverage that the QuantumBoard
    // fixture provided until F-13 gave that fixture an earlier refusal.
    let mut h = CovenantTestHarness::new();
    let err = h
        .deploy(REGISTERED_KEY_ONLY, h.addrs.deployer)
        .expect_err("registered_key is not yet lowered; deploy must fail");
    assert!(
        err.iter().any(|d| d.code.0 == 518),
        "expected E518 (unlowered BuiltinPredicate), got {err:?}"
    );
}

#[test]
fn deploy_is_rejected() {
    // Renamed from `deploy_rejects_unlowered_registered_key_predicate`: the
    // fixture is no longer refused for that reason. V0.9.6 F-13 made `append`
    // into, and reads from, a collection with no storage field an error
    // (E430/E431) after finding the append reported success and stored nothing,
    // and that `posts[i]` SLOADed slot 0 and returned the construct's first
    // declared field for every index. Both are raised in `build_ir`, upstream
    // of the codegen stage that raises E518, so the guard is never reached and
    // the old name described something that no longer happens.
    let mut h = CovenantTestHarness::new();
    let err = h
        .deploy(BOARD, h.addrs.deployer)
        .expect_err("posts has no storage field; deploy must fail");
    for code in [430, 431] {
        assert!(
            err.iter().any(|d| d.code.0 == code),
            "expected E{code} (unbacked collection), got {err:?}"
        );
    }
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

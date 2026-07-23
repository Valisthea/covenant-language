//! End-to-end scenarios for AmnesiaCeremony (ERC-8228 ceremony contract).
//!
//! Tests exercise the full amnesia ceremony lifecycle:
//! deploy → setup → submit_share → finalize → destroy
//!
//! The mock precompile suite (amnesia_nonce counter) ensures determinism.
//! Destruction emits `AmnesiaCeremonyDestroyed(uint256 indexed sessionId)`.

use covenant_testing::{CovenantTestHarness, U256};

const CEREMONY: &str =
    include_str!("../../covenant-lexer/tests/fixtures/example_08_amnesia_ceremony.cov");

fn setup() -> (CovenantTestHarness, covenant_testing::Contract) {
    let mut h = CovenantTestHarness::new();
    let c = h
        .deploy(CEREMONY, h.addrs.deployer)
        .expect("deploy AmnesiaCeremony");
    (h, c)
}

/// This fixture declares `threshold: 2`. OMEGA V6 (CRT-005 fix):
/// `finalize()` now asserts at least 2 DISTINCT callers have submitted a
/// share before it will even ask the (mocked) precompile -- previously
/// finalize trusted the precompile's boolean with zero on-chain
/// corroboration, so zero shares submitted could finalize successfully
/// (the exact bug the audit demonstrated). Tests that need to reach
/// `finalize()` successfully now submit 2 shares from 2 distinct callers
/// first.
fn submit_threshold_shares(h: &mut CovenantTestHarness, c: covenant_testing::Contract) {
    h.call_ok(
        c,
        h.addrs.alice,
        "submit_share(bytes32)",
        &[U256::from_u64(0xa11ce)],
    );
    h.call_ok(
        c,
        h.addrs.bob,
        "submit_share(bytes32)",
        &[U256::from_u64(0xb0b)],
    );
}

// ---------------------------------------------------------------------------
// Scenario 1: Deploy succeeds and produces non-empty bytecode
// ---------------------------------------------------------------------------

#[test]
fn ceremony_deploys_successfully() {
    let (h, c) = setup();
    assert!(
        !h.artifact(c).runtime_bytecode.is_empty(),
        "runtime bytecode must be non-empty"
    );
}

// ---------------------------------------------------------------------------
// Scenario 2: Expected selectors are present in artifact
// ---------------------------------------------------------------------------

#[test]
fn ceremony_has_eight_selectors() {
    let (h, c) = setup();
    let sels = &h.artifact(c).function_selectors;
    for name in [
        "setup",
        "submit_share",
        "finalize",
        "destroy",
        "phase",
        "session_id",
        "is_destroyed",
        "owner",
    ] {
        assert!(
            sels.contains_key(name),
            "missing selector `{name}`; present: {:?}",
            sels.keys().collect::<Vec<_>>()
        );
    }
}

// ---------------------------------------------------------------------------
// Scenario 3: Initial phase is 0 (Setup)
// ---------------------------------------------------------------------------

#[test]
fn ceremony_initial_phase_is_zero() {
    let (mut h, c) = setup();
    let ph = h.view_u256(c, h.addrs.alice, "phase()", &[]);
    assert_eq!(ph, U256::ZERO, "initial phase must be 0 (Setup)");
}

// ---------------------------------------------------------------------------
// Scenario 4: Initial session_id is 0
// ---------------------------------------------------------------------------

#[test]
fn ceremony_initial_session_id_is_zero() {
    let (mut h, c) = setup();
    let sid = h.view_u256(c, h.addrs.alice, "session_id()", &[]);
    assert_eq!(sid, U256::ZERO, "initial session_id must be 0");
}

// ---------------------------------------------------------------------------
// Scenario 5: setup() transitions phase to 1 and returns a session_id
// ---------------------------------------------------------------------------

#[test]
fn ceremony_setup_returns_session_id_and_sets_phase_one() {
    let (mut h, c) = setup();
    let result = h.call_ok(c, h.addrs.deployer, "setup()", &[]);
    // Return value is the session_id — must be non-zero (amnesia_nonce starts at 1).
    let sid = covenant_testing::abi::decode_u256(&result);
    assert!(
        sid > U256::ZERO,
        "setup() must return a non-zero session_id; got {sid:?}"
    );
    // Phase must now be 1.
    let ph = h.view_u256(c, h.addrs.alice, "phase()", &[]);
    assert_eq!(ph, U256::from_u64(1), "phase must be 1 after setup()");
}

// ---------------------------------------------------------------------------
// Scenario 6: session_id() matches the value returned by setup()
// ---------------------------------------------------------------------------

#[test]
fn ceremony_session_id_view_matches_setup_return() {
    let (mut h, c) = setup();
    let raw = h.call_ok(c, h.addrs.deployer, "setup()", &[]);
    let sid_from_setup = covenant_testing::abi::decode_u256(&raw);
    let sid_from_view = h.view_u256(c, h.addrs.alice, "session_id()", &[]);
    assert_eq!(
        sid_from_setup, sid_from_view,
        "session_id() must return the same value as setup()"
    );
}

// ---------------------------------------------------------------------------
// Scenario 7: submit_share succeeds after setup
// ---------------------------------------------------------------------------

#[test]
fn ceremony_submit_share_returns_true() {
    let (mut h, c) = setup();
    let _ = h.call_ok(c, h.addrs.deployer, "setup()", &[]);

    // Share is a bytes32 — use a deterministic test value.
    let share_word = U256::from_u64(0xdeadbeef);
    let result = h.call_ok(c, h.addrs.alice, "submit_share(bytes32)", &[share_word]);
    // Mock precompile always returns true (bool_word(true)).
    let ok = covenant_testing::abi::decode_u256(&result);
    assert_eq!(ok, U256::from_u64(1), "submit_share must return true");
}

// ---------------------------------------------------------------------------
// Scenario 8: finalize() sets phase to 2
// ---------------------------------------------------------------------------

#[test]
fn ceremony_finalize_sets_phase_two() {
    let (mut h, c) = setup();
    let _ = h.call_ok(c, h.addrs.deployer, "setup()", &[]);
    submit_threshold_shares(&mut h, c);
    let _ = h.call_ok(c, h.addrs.deployer, "finalize()", &[]);
    let ph = h.view_u256(c, h.addrs.alice, "phase()", &[]);
    assert_eq!(ph, U256::from_u64(2), "phase must be 2 after finalize()");
}

#[test]
fn ceremony_finalize_with_zero_shares_reverts_ksr_cvn_005() {
    // OMEGA V6 CRT-005 regression test: this exact sequence (setup then
    // immediately finalize with zero shares submitted) used to succeed --
    // the precise bug the audit demonstrated ("a ceremony's entire purpose
    // is threshold-of-N guardian consensus... this control is completely
    // absent").
    let (mut h, c) = setup();
    let _ = h.call_ok(c, h.addrs.deployer, "setup()", &[]);
    let result = h.call(c, h.addrs.deployer, "finalize()", &[]);
    assert!(
        result.is_revert(),
        "finalize() with zero shares submitted must revert (threshold: 2)"
    );
}

#[test]
fn ceremony_finalize_with_one_of_two_distinct_submitters_reverts_ksr_cvn_005() {
    let (mut h, c) = setup();
    let _ = h.call_ok(c, h.addrs.deployer, "setup()", &[]);
    let _ = h.call_ok(
        c,
        h.addrs.alice,
        "submit_share(bytes32)",
        &[U256::from_u64(0xa11ce)],
    );
    let result = h.call(c, h.addrs.deployer, "finalize()", &[]);
    assert!(
        result.is_revert(),
        "finalize() with only 1 of 2 required distinct submitters must revert"
    );
}

#[test]
fn ceremony_same_caller_submitting_twice_reverts_ksr_cvn_005() {
    // OMEGA V6 CRT-005 regression test: this exact sequence (one address
    // submitting the threshold's worth of shares) used to let a single
    // actor unilaterally fabricate guardian consensus.
    let (mut h, c) = setup();
    let _ = h.call_ok(c, h.addrs.deployer, "setup()", &[]);
    let _ = h.call_ok(
        c,
        h.addrs.alice,
        "submit_share(bytes32)",
        &[U256::from_u64(0xa11ce)],
    );
    let second_attempt = h.call(
        c,
        h.addrs.alice,
        "submit_share(bytes32)",
        &[U256::from_u64(0xa11ce2)],
    );
    assert!(
        second_attempt.is_revert(),
        "a second submit_share from the same caller must revert"
    );
    // Confirm the ceremony genuinely cannot finalize off the back of this
    // single (would-be double-counted) submitter.
    let result = h.call(c, h.addrs.deployer, "finalize()", &[]);
    assert!(
        result.is_revert(),
        "finalize() must still revert -- only 1 distinct submitter, threshold is 2"
    );
}

// ---------------------------------------------------------------------------
// Scenario 9: destroy() sets phase to 3, is_destroyed() returns true
// ---------------------------------------------------------------------------

#[test]
fn ceremony_destroy_sets_phase_three_and_is_destroyed() {
    let (mut h, c) = setup();
    let _ = h.call_ok(c, h.addrs.deployer, "setup()", &[]);
    submit_threshold_shares(&mut h, c);
    let _ = h.call_ok(c, h.addrs.deployer, "finalize()", &[]);
    let _ = h.call_ok(c, h.addrs.deployer, "destroy()", &[]);

    let ph = h.view_u256(c, h.addrs.alice, "phase()", &[]);
    assert_eq!(ph, U256::from_u64(3), "phase must be 3 after destroy()");

    let destroyed = h.view_u256(c, h.addrs.alice, "is_destroyed()", &[]);
    assert_eq!(
        destroyed,
        U256::from_u64(1),
        "is_destroyed() must return true"
    );
}

// ---------------------------------------------------------------------------
// Scenario 10: destroy() emits AmnesiaCeremonyDestroyed event
// ---------------------------------------------------------------------------

#[test]
fn ceremony_destroy_emits_event() {
    let (mut h, c) = setup();
    let _ = h.call_ok(c, h.addrs.deployer, "setup()", &[]);
    submit_threshold_shares(&mut h, c);
    let _ = h.call_ok(c, h.addrs.deployer, "finalize()", &[]);
    let _ = h.call_ok(c, h.addrs.deployer, "destroy()", &[]);

    let ev = h.find_event("AmnesiaCeremonyDestroyed(uint256)");
    assert!(
        ev.is_some(),
        "destroy() must emit AmnesiaCeremonyDestroyed event; logs: {:?}",
        h.host.logs
    );
}

// ---------------------------------------------------------------------------
// Scenario 11: owner() returns the deployer address
// ---------------------------------------------------------------------------

#[test]
fn ceremony_owner_returns_deployer() {
    let (mut h, c) = setup();
    let deployer = h.addrs.deployer;
    let owner_word = h.view_u256(c, h.addrs.alice, "owner()", &[]);
    // Deployer address is packed right in a 32-byte word.
    let expected = deployer.to_u256();
    assert_eq!(owner_word, expected, "owner() must return deployer address");
}

// ---------------------------------------------------------------------------
// Scenario 12: Unknown selector reverts
// ---------------------------------------------------------------------------

#[test]
fn ceremony_unknown_selector_reverts() {
    let (mut h, c) = setup();
    let result = h.raw_call(
        c,
        h.addrs.alice,
        vec![0xca, 0xfe, 0xba, 0xbe],
        U256::ZERO,
        false,
    );
    assert!(result.is_revert(), "unknown selector must revert");
}

// ---------------------------------------------------------------------------
// KSR-CVN-001: phase-transition guards
// ---------------------------------------------------------------------------
//
// The ceremony lifecycle is a strict state machine:
//     0 (Setup) -> 1 (Active) -> 2 (Finalized) -> 3 (Destroyed)
//
// Prior to this fix, every synthesized transition (setup/finalize/destroy)
// blindly SSTORE'd the next phase, regardless of the current phase or the
// caller. That allowed:
//     * anyone to re-`setup()` a live ceremony and rotate session_id;
//     * `finalize()` to be called before `setup()`;
//     * `destroy()` to be called before `finalize()`;
//     * `finalize()` / `destroy()` to advance the phase even when the
//       precompile proof returned false.
//
// The following scenarios cover each of those attack paths.

#[test]
fn ceremony_setup_rejects_non_deployer_caller_ksr_cvn_001() {
    // Only the deployer may initiate the ceremony.
    let (mut h, c) = setup();
    let result = h.call(c, h.addrs.alice, "setup()", &[]);
    assert!(
        result.is_revert(),
        "setup() must revert when caller != deployer (KSR-CVN-001)"
    );
    let ph = h.view_u256(c, h.addrs.alice, "phase()", &[]);
    assert_eq!(
        ph,
        U256::ZERO,
        "phase must remain 0 after rejected setup attempt"
    );
}

#[test]
fn ceremony_second_setup_reverts_ksr_cvn_001() {
    // Once phase > 0, setup() must revert — otherwise an attacker could
    // rotate session_id under a live ceremony.
    let (mut h, c) = setup();
    let _ = h.call_ok(c, h.addrs.deployer, "setup()", &[]);
    let result = h.call(c, h.addrs.deployer, "setup()", &[]);
    assert!(
        result.is_revert(),
        "second setup() must revert (phase already 1)"
    );
}

#[test]
fn ceremony_finalize_before_setup_reverts_ksr_cvn_001() {
    let (mut h, c) = setup();
    // No setup() yet — phase is 0. finalize requires phase == 1.
    let result = h.call(c, h.addrs.deployer, "finalize()", &[]);
    assert!(
        result.is_revert(),
        "finalize() must revert when phase != 1 (not yet set up)"
    );
}

#[test]
fn ceremony_destroy_before_finalize_reverts_ksr_cvn_001() {
    let (mut h, c) = setup();
    let _ = h.call_ok(c, h.addrs.deployer, "setup()", &[]);
    // Phase is 1, not 2 — destroy must revert.
    let result = h.call(c, h.addrs.deployer, "destroy()", &[]);
    assert!(
        result.is_revert(),
        "destroy() must revert when phase != 2 (not yet finalized)"
    );
}

#[test]
fn ceremony_double_destroy_reverts_ksr_cvn_001() {
    let (mut h, c) = setup();
    let _ = h.call_ok(c, h.addrs.deployer, "setup()", &[]);
    submit_threshold_shares(&mut h, c);
    let _ = h.call_ok(c, h.addrs.deployer, "finalize()", &[]);
    let _ = h.call_ok(c, h.addrs.deployer, "destroy()", &[]);
    // Second destroy must fail — phase is now 3, not 2.
    let result = h.call(c, h.addrs.deployer, "destroy()", &[]);
    assert!(
        result.is_revert(),
        "second destroy() must revert (already destroyed)"
    );
}

#[test]
fn ceremony_submit_share_requires_active_phase_ksr_cvn_001() {
    // Before setup, phase = 0 — submit_share must revert.
    let (mut h, c) = setup();
    let result_before = h.call(
        c,
        h.addrs.alice,
        "submit_share(bytes32)",
        &[U256::from_u64(0xdead)],
    );
    assert!(
        result_before.is_revert(),
        "submit_share() must revert before setup (phase 0)"
    );

    // After finalize, phase = 2 — submit_share must revert.
    let _ = h.call_ok(c, h.addrs.deployer, "setup()", &[]);
    submit_threshold_shares(&mut h, c);
    let _ = h.call_ok(c, h.addrs.deployer, "finalize()", &[]);
    let result_after = h.call(
        c,
        h.addrs.alice,
        "submit_share(bytes32)",
        &[U256::from_u64(0xdead)],
    );
    assert!(
        result_after.is_revert(),
        "submit_share() must revert after finalize (phase 2)"
    );
}

#[test]
fn ceremony_finalize_rejects_non_deployer_caller_ksr_cvn_001() {
    let (mut h, c) = setup();
    let _ = h.call_ok(c, h.addrs.deployer, "setup()", &[]);
    let result = h.call(c, h.addrs.alice, "finalize()", &[]);
    assert!(
        result.is_revert(),
        "finalize() must revert when caller != deployer"
    );
    let ph = h.view_u256(c, h.addrs.alice, "phase()", &[]);
    assert_eq!(
        ph,
        U256::from_u64(1),
        "phase must remain 1 after rejected finalize attempt"
    );
}

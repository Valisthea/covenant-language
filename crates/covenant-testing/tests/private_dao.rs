//! End-to-end scenarios for PrivateDAO (FHE-tally governance contract).
//!
//! PrivateDAO stores vote tallies as TFHE ciphertexts using `encrypted counter`.
//! Each `vote_yes()` / `vote_no()` call invokes the FHE-add precompile to
//! increment the respective tally homomorphically. The reveal functions use
//! `FheRevealDecrypt` to return the plaintext count.

use covenant_testing::{CovenantTestHarness, U256};

const DAO: &str = include_str!("../../covenant-lexer/tests/fixtures/example_07_private_dao.cov");

fn setup() -> (CovenantTestHarness, covenant_testing::Contract) {
    let mut h = CovenantTestHarness::new();
    let c = h.deploy(DAO, h.addrs.deployer).expect("deploy PrivateDAO");
    (h, c)
}

// ---------------------------------------------------------------------------
// Scenario 1: Deploy successfully
// ---------------------------------------------------------------------------

#[test]
fn privatedao_deploys_successfully() {
    let (h, c) = setup();
    assert!(!h.artifact(c).runtime_bytecode.is_empty());
}

// ---------------------------------------------------------------------------
// Scenario 2: Expected selectors present
// ---------------------------------------------------------------------------

#[test]
fn privatedao_has_expected_selectors() {
    let (h, c) = setup();
    let sels = &h.artifact(c).function_selectors;
    for name in ["vote_yes", "vote_no", "yes_votes", "no_votes"] {
        assert!(
            sels.contains_key(name),
            "missing selector `{name}`; present: {:?}",
            sels.keys().collect::<Vec<_>>()
        );
    }
}

// ---------------------------------------------------------------------------
// Scenario 3: vote_yes triggers FHE precompile (handle table grows)
// ---------------------------------------------------------------------------

#[test]
fn privatedao_vote_yes_triggers_fhe() {
    let (mut h, c) = setup();
    let before = h.precompiles.fhe_handles.len();
    let _ = h.call_ok(c, h.addrs.alice, "vote_yes()", &[]);
    assert!(
        h.precompiles.fhe_handles.len() > before,
        "vote_yes() must mint FHE handles (before={before}, after={})",
        h.precompiles.fhe_handles.len()
    );
}

// ---------------------------------------------------------------------------
// Scenario 4: vote_no triggers FHE precompile (handle table grows)
// ---------------------------------------------------------------------------

#[test]
fn privatedao_vote_no_triggers_fhe() {
    let (mut h, c) = setup();
    let before = h.precompiles.fhe_handles.len();
    let _ = h.call_ok(c, h.addrs.bob, "vote_no()", &[]);
    assert!(
        h.precompiles.fhe_handles.len() > before,
        "vote_no() must mint FHE handles (before={before}, after={})",
        h.precompiles.fhe_handles.len()
    );
}

// ---------------------------------------------------------------------------
// Scenario 5: Reveal after votes returns correct plaintext tally
// ---------------------------------------------------------------------------

#[test]
fn privatedao_reveal_returns_correct_tally() {
    let (mut h, c) = setup();
    // Cast two yes-votes and one no-vote.
    let _ = h.call_ok(c, h.addrs.alice, "vote_yes()", &[]);
    let _ = h.call_ok(c, h.addrs.bob, "vote_yes()", &[]);
    let _ = h.call_ok(c, h.addrs.carol, "vote_no()", &[]);

    // Reveal decrypts the ciphertext tallies (owner = deployer).
    let deployer = h.addrs.deployer;
    let yes = h.view_u256(c, deployer, "yes_votes()", &[]);
    let no = h.view_u256(c, deployer, "no_votes()", &[]);

    assert_eq!(yes, U256::from_u64(2), "expected 2 yes-votes");
    assert_eq!(no, U256::from_u64(1), "expected 1 no-vote");
}

// ---------------------------------------------------------------------------
// Scenario 6: Multiple votes accumulate (handle table grows monotonically)
// ---------------------------------------------------------------------------

#[test]
fn privatedao_repeated_votes_grow_handle_table() {
    let (mut h, c) = setup();
    let n0 = h.precompiles.fhe_handles.len();
    let _ = h.call_ok(c, h.addrs.alice, "vote_yes()", &[]);
    let n1 = h.precompiles.fhe_handles.len();
    let _ = h.call_ok(c, h.addrs.bob, "vote_yes()", &[]);
    let n2 = h.precompiles.fhe_handles.len();
    let _ = h.call_ok(c, h.addrs.carol, "vote_no()", &[]);
    let n3 = h.precompiles.fhe_handles.len();
    assert!(n1 > n0);
    assert!(n2 > n1);
    assert!(n3 > n2);
}

// ---------------------------------------------------------------------------
// Scenario 7: Unknown selector reverts
// ---------------------------------------------------------------------------

#[test]
fn privatedao_unknown_selector_reverts() {
    let (mut h, c) = setup();
    let result = h.raw_call(
        c,
        h.addrs.alice,
        vec![0xde, 0xad, 0xbe, 0xef],
        U256::ZERO,
        false,
    );
    assert!(result.is_revert(), "unknown selector must revert");
}

// ---------------------------------------------------------------------------
// Scenario 8: Initial tallies decrypt to zero before any vote
// ---------------------------------------------------------------------------

#[test]
fn privatedao_initial_tallies_are_zero() {
    let (mut h, c) = setup();
    let deployer = h.addrs.deployer;
    let yes = h.view_u256(c, deployer, "yes_votes()", &[]);
    let no = h.view_u256(c, deployer, "no_votes()", &[]);
    // No votes cast yet — tallies should decrypt to 0.
    // (The zero-bytes handle resolves to U256::ZERO in the mock.)
    assert_eq!(yes, U256::ZERO, "initial yes tally must be zero");
    assert_eq!(no, U256::ZERO, "initial no tally must be zero");
}

// ---------------------------------------------------------------------------
// Scenario 9: yes_votes and no_votes are independent tallies
// ---------------------------------------------------------------------------

#[test]
fn privatedao_yes_and_no_tallies_are_independent() {
    let (mut h, c) = setup();
    let _ = h.call_ok(c, h.addrs.alice, "vote_yes()", &[]);
    let _ = h.call_ok(c, h.addrs.alice, "vote_yes()", &[]);
    let _ = h.call_ok(c, h.addrs.bob, "vote_no()", &[]);

    let deployer = h.addrs.deployer;
    let yes = h.view_u256(c, deployer, "yes_votes()", &[]);
    let no = h.view_u256(c, deployer, "no_votes()", &[]);
    assert_eq!(yes, U256::from_u64(2), "yes tally should be 2");
    assert_eq!(no, U256::from_u64(1), "no tally should be 1");
}

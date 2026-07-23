//! Smoke test: every Codex Basics example deploys.
//!
//! OMEGA V6 CRT-004 fix (E518): ballot/board use BuiltinPredicate guards
//! (first_time_caller / registered_key) that now correctly refuse to
//! compile -- see `ballot_rejects_unlowered_predicate`/
//! `board_rejects_unlowered_predicate` below (previously `ballot_deploys`/
//! `board_deploys`, which asserted successful deploy).

use covenant_testing::CovenantTestHarness;

const HELLO: &str = include_str!("../../covenant-lexer/tests/fixtures/example_01_hello.cov");
const COIN: &str = include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov");
const BALLOT: &str = include_str!("../../covenant-lexer/tests/fixtures/example_03_open_ballot.cov");
const COUNTER: &str =
    include_str!("../../covenant-lexer/tests/fixtures/example_04_shielded_counter.cov");
const BOARD: &str =
    include_str!("../../covenant-lexer/tests/fixtures/example_05_quantum_board.cov");

#[test]
fn hello_deploys() {
    let mut h = CovenantTestHarness::new();
    let r = h.deploy(HELLO, h.addrs.deployer);
    if let Err(diags) = &r {
        for d in diags {
            eprintln!("{:?}: {}", d.level, d.message);
        }
    }
    assert!(r.is_ok());
}

#[test]
fn coin_deploys() {
    let mut h = CovenantTestHarness::new();
    let r = h.deploy(COIN, h.addrs.deployer);
    if let Err(diags) = &r {
        for d in diags {
            eprintln!("{:?}: {}", d.level, d.message);
        }
    }
    assert!(r.is_ok());
}

#[test]
fn ballot_rejects_unlowered_predicate() {
    let mut h = CovenantTestHarness::new();
    let r = h.deploy(BALLOT, h.addrs.deployer);
    let diags = r.expect_err("first_time_caller is not yet lowered; deploy must fail");
    assert!(
        diags.iter().any(|d| d.code.0 == 518),
        "expected E518 (unlowered BuiltinPredicate), got {diags:?}"
    );
}

#[test]
fn counter_deploys() {
    let mut h = CovenantTestHarness::new();
    let r = h.deploy(COUNTER, h.addrs.deployer);
    if let Err(diags) = &r {
        for d in diags {
            eprintln!("{:?}: {}", d.level, d.message);
        }
    }
    assert!(r.is_ok());
}

#[test]
fn board_rejects_unlowered_predicate() {
    let mut h = CovenantTestHarness::new();
    let r = h.deploy(BOARD, h.addrs.deployer);
    let diags = r.expect_err("registered_key is not yet lowered; deploy must fail");
    assert!(
        diags.iter().any(|d| d.code.0 == 518),
        "expected E518 (unlowered BuiltinPredicate), got {diags:?}"
    );
}

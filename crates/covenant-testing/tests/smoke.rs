//! Smoke test: every Codex Basics example deploys.
//!
//! OMEGA V6 CRT-004 fix (E518): ballot/board use BuiltinPredicate guards
//! (first_time_caller / registered_key) that now correctly refuse to
//! compile -- see `ballot_rejects_unlowered_predicate`/
//! `board_rejects_unlowered_predicate` below (previously `ballot_deploys`/
//! `board_deploys`, which asserted successful deploy).
//!
//! V0.9.6 F-13 added a second, earlier refusal to the board fixture (E430/E431,
//! an unbacked `posts` collection), so that test now pins those codes. See the
//! comment on it for where the `registered_key` E518 coverage moved.

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
    // The board fixture is refused by E430/E431, not by E518, and that is the
    // correct outcome rather than a regression. Besides `only registered_key`
    // it appends into, and reads from, `posts`, a collection with no storage
    // field: V0.9.6 F-13 found the append reported success and wrote nothing,
    // and `posts[i]` SLOADed slot 0, handing back the construct's first
    // declared field for every index. Those are raised in `build_ir`, upstream
    // of the codegen stage that raises E518, so the pipeline never reaches the
    // guard. Asserting E518 here would now be asserting that the earlier and
    // more serious refusal does NOT fire.
    //
    // The `registered_key` E518 protection this test used to provide lives in
    // quantum_board.rs's `registered_key_predicate_is_refused`, which isolates
    // the predicate in a construct that gets all the way to codegen.
    let mut h = CovenantTestHarness::new();
    let r = h.deploy(BOARD, h.addrs.deployer);
    let diags = r.expect_err("posts has no storage field; deploy must fail");
    for code in [430, 431] {
        assert!(
            diags.iter().any(|d| d.code.0 == code),
            "expected E{code} (unbacked collection), got {diags:?}"
        );
    }
}

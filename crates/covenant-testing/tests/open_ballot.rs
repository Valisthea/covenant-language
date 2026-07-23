//! End-to-end scenarios for the OpenBallot Basics example.
//!
//! OpenBallot exercises: (1) action dispatch with predicates, (2) reveal
//! function gated on time, (3) `duration` arithmetic, (4) the
//! `first_time_caller` predicate.
//!
//! OMEGA V6 CRT-004 fix (E518): `cast` is gated `only first_time_caller`, a
//! BuiltinPredicate guard that used to compile to an unconditional `push 1`
//! (a complete, undiagnosed authorization bypass -- `first_time_caller`
//! provided no actual first-time-only enforcement). It now correctly
//! refuses to compile until the predicate has a real EVM lowering (E518,
//! same pattern as E516/E517 for amnesia/VDF primitives), so this fixture
//! cannot deploy at all until that lands. The deploy-dependent tests below
//! were replaced with a single test asserting the new, correct rejection.

use covenant_testing::CovenantTestHarness;

const BALLOT: &str = include_str!("../../covenant-lexer/tests/fixtures/example_03_open_ballot.cov");

#[test]
fn deploy_rejects_unlowered_first_time_caller_predicate() {
    let mut h = CovenantTestHarness::new();
    let err = h
        .deploy(BALLOT, h.addrs.deployer)
        .expect_err("first_time_caller is not yet lowered; deploy must fail");
    assert!(
        err.iter().any(|d| d.code.0 == 518),
        "expected E518 (unlowered BuiltinPredicate), got {err:?}"
    );
}

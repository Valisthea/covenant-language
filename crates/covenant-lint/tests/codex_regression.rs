//! Codex regression tests: run all detectors on every example fixture and
//! pin the exact known critical-finding count so regressions are caught.
//!
//! Note: several fixtures intentionally use unguarded public actions (e.g.
//! hello.cov's `update`, shielded_counter.cov's `bump`) — these are real
//! security findings; the counts below are the current stable baseline.

use covenant_diag::SourceId;
use covenant_lint::framework::Severity;
use covenant_lint::lint_source;

fn critical_findings(src: &str) -> Vec<covenant_lint::framework::Finding> {
    lint_source(src, SourceId::new(0))
        .into_iter()
        .filter(|f| f.severity == Severity::Critical)
        .collect()
}

fn critical_count(src: &str) -> usize {
    critical_findings(src).len()
}

// ── Fixtures with 0 expected critical findings ──────────────────────────────

#[test]
fn coin_no_critical() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov");
    assert_eq!(
        critical_count(src),
        0,
        "coin.cov: unexpected critical findings"
    );
}

#[test]
fn open_ballot_no_critical() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_03_open_ballot.cov");
    assert_eq!(
        critical_count(src),
        0,
        "open_ballot.cov: unexpected critical findings"
    );
}

#[test]
fn secret_coin_no_critical() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_06_secret_coin.cov");
    assert_eq!(
        critical_count(src),
        0,
        "secret_coin.cov: unexpected critical findings"
    );
}

// ── Fixtures with known critical findings (stable baseline) ─────────────────

#[test]
fn hello_known_criticals() {
    // hello.cov has one public unguarded action (`update`) → C100.
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_01_hello.cov");
    assert_eq!(
        critical_count(src),
        1,
        "hello.cov: critical finding count changed"
    );
}

#[test]
fn shielded_counter_known_criticals() {
    // shielded_counter.cov has one public unguarded action (`bump`) → C100.
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_04_shielded_counter.cov");
    assert_eq!(
        critical_count(src),
        1,
        "shielded_counter.cov: critical count changed"
    );
}

#[test]
fn quantum_board_known_criticals() {
    // quantum_board.cov's `submit` uses `pq_signed(...)` without a nullifier
    // map — C700 (KSR-CVN-024) now correctly flags this as a replayable PQ
    // verify, raising the baseline from 3 → 4.
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_05_quantum_board.cov");
    assert_eq!(
        critical_count(src),
        4,
        "quantum_board.cov: critical count changed"
    );
}

#[test]
fn private_dao_known_criticals() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_07_private_dao.cov");
    assert_eq!(
        critical_count(src),
        2,
        "private_dao.cov: critical count changed"
    );
}

// ── Smoke test: linter does not panic on any fixture ────────────────────────

#[test]
fn amnesia_ceremony_does_not_panic() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_08_amnesia_ceremony.cov");
    let _ = lint_source(src, SourceId::new(0));
}

#[test]
fn encrypted_bridge_does_not_panic() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_09_encrypted_bridge.cov");
    let _ = lint_source(src, SourceId::new(0));
}

#[test]
fn hybrid_state_does_not_panic() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_10_hybrid_state.cov");
    let _ = lint_source(src, SourceId::new(0));
}

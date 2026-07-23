//! Integration tests: tokenize each Codex Basics example and assert the token
//! stream is well-formed (non-empty, ends with Eof, no Error variants, no diags).

use covenant_diag::SourceId;
use covenant_lexer::{tokenize, TokenKind};

fn assert_clean(name: &str, source: &str) -> usize {
    let (tokens, diags) = tokenize(source, SourceId::new(0));
    assert!(!tokens.is_empty(), "{name}: token stream is empty");
    assert_eq!(
        tokens.last().map(|t| &t.kind),
        Some(&TokenKind::Eof),
        "{name}: last token is not Eof"
    );
    assert!(
        !tokens.iter().any(|t| t.kind == TokenKind::Error),
        "{name}: stream contains Error tokens: {:?}",
        tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Error)
            .collect::<Vec<_>>()
    );
    assert!(
        diags.is_empty(),
        "{name}: unexpected diagnostics: {:?}",
        diags
    );
    tokens.len()
}

#[test]
fn example_01_hello() {
    let source = include_str!("fixtures/example_01_hello.cov");
    let count = assert_clean("example_01_hello", source);
    assert!(
        count > 10,
        "example 1 should produce more than 10 tokens (got {count})"
    );
}

#[test]
fn example_02_coin() {
    let source = include_str!("fixtures/example_02_coin.cov");
    assert_clean("example_02_coin", source);
}

#[test]
fn example_03_open_ballot() {
    let source = include_str!("fixtures/example_03_open_ballot.cov");
    assert_clean("example_03_open_ballot", source);
}

#[test]
fn example_04_shielded_counter() {
    let source = include_str!("fixtures/example_04_shielded_counter.cov");
    assert_clean("example_04_shielded_counter", source);
}

#[test]
fn example_05_quantum_board() {
    let source = include_str!("fixtures/example_05_quantum_board.cov");
    assert_clean("example_05_quantum_board", source);
}

/// Sanity check: the coin example contains a fused `Duration` if you rewrite it,
/// but the raw fixture does not. This test guards against accidental duration
/// fusion in the coin example by asserting Integer(1_000_000) survives.
#[test]
fn coin_integer_is_not_fused() {
    let source = include_str!("fixtures/example_02_coin.cov");
    let (tokens, _) = tokenize(source, SourceId::new(0));
    assert!(
        tokens
            .iter()
            .any(|t| t.kind == TokenKind::Integer(1_000_000)),
        "expected to see Integer(1_000_000) in coin example"
    );
}

/// `7 days` in the OpenBallot fixture must fuse into a single `Duration` token.
#[test]
fn open_ballot_fuses_duration() {
    use covenant_lexer::DurationUnit;
    let source = include_str!("fixtures/example_03_open_ballot.cov");
    let (tokens, _) = tokenize(source, SourceId::new(0));
    assert!(
        tokens
            .iter()
            .any(|t| matches!(t.kind, TokenKind::Duration(7, DurationUnit::Days))),
        "expected Duration(7, Days) in OpenBallot fixture"
    );
}

/// Sprint 35 Phase 1: the bare `nft NAME { }` fixture must lex cleanly,
/// producing a `KwNft` token. Full ERC-721 synthesis is Phase 2.
#[test]
fn example_13_nft_minimal_lexes_clean() {
    let source = include_str!("fixtures/example_13_nft_minimal.cov");
    assert_clean("example_13_nft_minimal", source);
    let (tokens, _) = tokenize(source, SourceId::new(0));
    assert!(
        tokens.iter().any(|t| t.kind == TokenKind::KwNft),
        "expected KwNft token in example_13_nft_minimal"
    );
}

/// Sprint 35 Phase 1: `registry NAME { }` was parsed since V0.6 but
/// synthesizer is stubbed. This fixture exists to anchor the Phase 2
/// (ERC-8231) work and to exercise the keyword end-to-end.
#[test]
fn example_14_registry_minimal_lexes_clean() {
    let source = include_str!("fixtures/example_14_registry_minimal.cov");
    assert_clean("example_14_registry_minimal", source);
    let (tokens, _) = tokenize(source, SourceId::new(0));
    assert!(
        tokens.iter().any(|t| t.kind == TokenKind::KwRegistry),
        "expected KwRegistry token in example_14_registry_minimal"
    );
}

//! Integration tests: privacy-check each Codex Basics fixture.

use covenant_diag::{Diagnostic, DiagnosticLevel, SourceId};
use covenant_lexer::tokenize;
use covenant_parser::parse;
use covenant_privacy::{analyze_privacy, PrivacyCheckedFile, PrivacyDomain};
use covenant_resolver::resolve;
use covenant_types::typecheck;

fn pipeline(src: &str) -> (PrivacyCheckedFile, Vec<Diagnostic>) {
    let (toks, lex) = tokenize(src, SourceId::new(0));
    assert!(lex.is_empty(), "lex: {lex:?}");
    let (file, pd) = parse(&toks, SourceId::new(0));
    assert!(pd.is_empty(), "parse: {pd:?}");
    let (res, rd) = resolve(file.expect("file"), SourceId::new(0));
    let rerrs: Vec<_> = rd
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Error)
        .collect();
    assert!(rerrs.is_empty(), "resolve: {rerrs:?}");
    let (typed, td) = typecheck(res, SourceId::new(0));
    let terrs: Vec<_> = td
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Error)
        .collect();
    assert!(terrs.is_empty(), "typecheck: {terrs:?}");
    analyze_privacy(typed, SourceId::new(0))
}

fn errors_only(d: &[Diagnostic]) -> Vec<&Diagnostic> {
    d.iter()
        .filter(|x| x.level == DiagnosticLevel::Error)
        .collect()
}

#[test]
fn example_1_hello_clean() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_01_hello.cov");
    let (_c, diags) = pipeline(src);
    assert!(diags.is_empty(), "unexpected: {diags:?}");
}

#[test]
fn example_2_coin_clean() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov");
    let (_c, diags) = pipeline(src);
    assert!(diags.is_empty(), "unexpected: {diags:?}");
}

#[test]
fn example_3_open_ballot_clean() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_03_open_ballot.cov");
    let (_c, diags) = pipeline(src);
    assert!(errors_only(&diags).is_empty(), "{diags:?}");
}

#[test]
fn example_4_shielded_counter_total_is_encrypted() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_04_shielded_counter.cov");
    let (checked, diags) = pipeline(src);
    assert!(errors_only(&diags).is_empty(), "{diags:?}");
    // The `total` field is declared `total: amount` under `encrypted counter`,
    // so the resolver-level decl type lowered to Ciphertext(Amount). The
    // analyzer must reflect that in `decl_domains`.
    assert!(
        checked
            .domains
            .decl_domains
            .contains(&PrivacyDomain::Encrypted),
        "expected at least one Encrypted decl domain, got {:?}",
        checked.domains.decl_domains
    );
}

#[test]
fn example_5_quantum_board_clean() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_05_quantum_board.cov");
    let (_c, diags) = pipeline(src);
    assert!(errors_only(&diags).is_empty(), "{diags:?}");
}

#[test]
fn every_typed_expression_has_domain() {
    // Any expression that Phase 4 typed must have a domain here.
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_05_quantum_board.cov");
    let (checked, _) = pipeline(src);
    for span in checked.typed.types.expr_types.keys() {
        assert!(
            checked.domains.expr_domains.contains_key(span),
            "expr at {:?} missing a domain",
            span
        );
    }
}

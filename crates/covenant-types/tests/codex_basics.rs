//! Integration tests: typecheck each Codex Basics fixture.

use covenant_diag::{Diagnostic, DiagnosticLevel, SourceId};
use covenant_lexer::tokenize;
use covenant_parser::parse;
use covenant_resolver::resolve;
use covenant_types::{typecheck, Ty, TypedFile};

fn pipeline(src: &str) -> (TypedFile, Vec<Diagnostic>) {
    let (toks, lex) = tokenize(src, SourceId::new(0));
    assert!(lex.is_empty(), "lex: {lex:?}");
    let (file, pd) = parse(&toks, SourceId::new(0));
    assert!(pd.is_empty(), "parse: {pd:?}");
    let (res, rd) = resolve(file.expect("file"), SourceId::new(0));
    let errs: Vec<_> = rd
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Error)
        .collect();
    assert!(errs.is_empty(), "resolve: {errs:?}");
    typecheck(res, SourceId::new(0))
}

fn errors_only(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
    diags
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Error)
        .collect()
}

#[test]
fn example_1_hello() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_01_hello.cov");
    let (_typed, diags) = pipeline(src);
    assert!(errors_only(&diags).is_empty(), "{diags:?}");
}

#[test]
fn example_2_coin() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov");
    let (_typed, diags) = pipeline(src);
    assert!(errors_only(&diags).is_empty(), "{diags:?}");
}

#[test]
fn example_3_open_ballot() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_03_open_ballot.cov");
    let (_typed, diags) = pipeline(src);
    assert!(errors_only(&diags).is_empty(), "{diags:?}");
}

#[test]
fn example_4_shielded_counter_records_lift() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_04_shielded_counter.cov");
    let (typed, diags) = pipeline(src);
    assert!(errors_only(&diags).is_empty(), "{diags:?}");
    // The `total += by` assignment must record at least one lift: plaintext
    // `by: Amount` auto-encrypted for the Ciphertext(Amount) LHS.
    assert!(
        !typed.types.lifts.is_empty(),
        "expected at least one lift marker in ShieldedCounter, got {:?}",
        typed.types.lifts
    );
    let lift = &typed.types.lifts[0];
    assert_eq!(lift.from, Ty::Amount);
    assert_eq!(lift.to, Ty::Ciphertext(Box::new(Ty::Amount)));
}

#[test]
fn example_5_quantum_board() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_05_quantum_board.cov");
    let (_typed, diags) = pipeline(src);
    assert!(errors_only(&diags).is_empty(), "{diags:?}");
}

#[test]
fn shielded_counter_total_field_is_ciphertext() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_04_shielded_counter.cov");
    let (typed, _) = pipeline(src);
    // `total: amount` under `encrypted counter` must lower to Ciphertext(Amount).
    let has_ct = typed
        .types
        .decl_types
        .iter()
        .any(|t| matches!(t, Ty::Ciphertext(inner) if **inner == Ty::Amount));
    assert!(has_ct, "expected Ciphertext(Amount) field type");
}

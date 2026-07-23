//! Integration tests: resolve each Codex Basics fixture and spot-check bindings.

use covenant_diag::SourceId;
use covenant_lexer::tokenize;
use covenant_parser::parse;
use covenant_resolver::{resolve, Binding, BuiltinPredicate, LangIdent};

fn resolve_or_panic(
    name: &str,
    src: &str,
) -> (
    covenant_resolver::ResolvedFile,
    Vec<covenant_diag::Diagnostic>,
) {
    let (toks, lex) = tokenize(src, SourceId::new(0));
    assert!(lex.is_empty(), "{name}: lex: {lex:?}");
    let (file, pd) = parse(&toks, SourceId::new(0));
    assert!(pd.is_empty(), "{name}: parse: {pd:?}");
    let (resolved, diags) = resolve(file.expect("parse"), SourceId::new(0));
    (resolved, diags)
}

#[test]
fn example_1_hello() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_01_hello.cov");
    let (res, diags) = resolve_or_panic("hello", src);
    assert!(diags.is_empty(), "diags: {diags:?}");
    // No unresolved bindings expected.
    assert!(!res
        .bindings
        .ident_bindings
        .values()
        .any(|b| matches!(b, Binding::Unresolved)));
}

#[test]
fn example_2_coin_deployer_is_lang_ident() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov");
    let (res, diags) = resolve_or_panic("coin", src);
    assert!(diags.is_empty(), "diags: {diags:?}");
    // Expect the single resolved ident to be LangIdent::Deployer.
    let kinds: Vec<_> = res.bindings.ident_bindings.values().collect();
    assert!(
        kinds
            .iter()
            .any(|b| matches!(b, Binding::LangIdent(LangIdent::Deployer))),
        "expected deployer binding, got {kinds:?}"
    );
}

#[test]
fn example_3_open_ballot() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_03_open_ballot.cov");
    let (res, diags) = resolve_or_panic("open_ballot", src);
    assert!(diags.is_empty(), "diags: {diags:?}");
    // `first_time_caller` must bind to BuiltinPredicate::FirstTimeCaller.
    assert!(res.bindings.ident_bindings.values().any(|b| matches!(
        b,
        Binding::BuiltinPredicate(BuiltinPredicate::FirstTimeCaller)
    )));
    // `now` must bind to LangIdent::Now.
    assert!(res
        .bindings
        .ident_bindings
        .values()
        .any(|b| matches!(b, Binding::LangIdent(LangIdent::Now))));
    // A field binding must be present for `options`.
    assert!(res
        .bindings
        .ident_bindings
        .values()
        .any(|b| matches!(b, Binding::Field(_))));
}

#[test]
fn example_4_shielded_counter_reveal_one_liner() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_04_shielded_counter.cov");
    let (res, diags) = resolve_or_panic("shielded_counter", src);
    assert!(diags.is_empty(), "diags: {diags:?}");
    // `total` should bind to a Field; `owner` target should fall back to LangIdent::Owner.
    let bindings: Vec<_> = res.bindings.ident_bindings.values().collect();
    assert!(bindings.iter().any(|b| matches!(b, Binding::Field(_))));
    assert!(bindings
        .iter()
        .any(|b| matches!(b, Binding::LangIdent(LangIdent::Owner))));
}

#[test]
fn example_5_quantum_board() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_05_quantum_board.cov");
    let (res, diags) = resolve_or_panic("quantum_board", src);
    assert!(diags.is_empty(), "diags: {diags:?}");
    // `registered_key` is a BuiltinPredicate.
    assert!(res.bindings.ident_bindings.values().any(|b| matches!(
        b,
        Binding::BuiltinPredicate(BuiltinPredicate::RegisteredKey)
    )));
    // `caller` as LangIdent appears multiple times.
    assert!(res
        .bindings
        .ident_bindings
        .values()
        .any(|b| matches!(b, Binding::LangIdent(LangIdent::Caller))));
}

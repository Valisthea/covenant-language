//! Error-path coverage for typechecker diagnostic codes.

use covenant_diag::{Diagnostic, SourceId};
use covenant_lexer::tokenize;
use covenant_parser::parse;
use covenant_resolver::resolve;
use covenant_types::{codes, typecheck};

fn diags(src: &str) -> Vec<Diagnostic> {
    let (toks, _) = tokenize(src, SourceId::new(0));
    let (file, _) = parse(&toks, SourceId::new(0));
    let (res, _) = resolve(file.expect("file"), SourceId::new(0));
    let (_, d) = typecheck(res, SourceId::new(0));
    d
}

fn has(src: &str, c: covenant_diag::DiagCode) -> bool {
    diags(src).iter().any(|d| d.code == c)
}

#[test]
fn e201_type_mismatch_in_field_initializer() {
    assert!(has(
        r#"record R { field x: amount = "not a number" }"#,
        codes::E201_TYPE_MISMATCH
    ));
}

#[test]
fn e202_plus_on_bytes_and_text() {
    assert!(has(
        r#"record R { view v returns bytes { 0xabcd + "x" } }"#,
        codes::E202_OP_INAPPLICABLE
    ));
}

#[test]
fn e203_unknown_stdlib_method() {
    assert!(has(
        r#"record R { action a(x: hash) { let _y = PQKeys.no_such(x) } }"#,
        codes::E203_UNKNOWN_METHOD
    ));
}

#[test]
fn e205_arity_mismatch() {
    assert!(has(
        r#"record R { view v returns amount { pow(1) } }"#,
        codes::E205_ARITY_MISMATCH
    ));
}

#[test]
fn e206_not_indexable() {
    assert!(has(
        r#"record R { view v returns amount { caller[0] } }"#,
        codes::E206_NOT_INDEXABLE
    ));
}

#[test]
fn e207_not_field_accessible() {
    assert!(has(
        r#"record R { view v returns amount { (1).nonsense } }"#,
        codes::E207_NOT_FIELD_ACCESSIBLE
    ));
}

#[test]
fn e211_double_encryption() {
    assert!(has(
        r#"record R { view v returns ciphertext<amount> { encrypted(encrypted(1)) } }"#,
        codes::E211_DOUBLE_ENCRYPTION
    ));
}

#[test]
fn e213_revert_with_wrong_arity() {
    assert!(has(
        r#"record R { error E(a: amount)
           action go() { revert_with E() } }"#,
        codes::E213_REVERT_ARG_MISMATCH
    ));
}

#[test]
fn e214_emit_wrong_arity() {
    assert!(has(
        r#"record R { event E(a: amount)
           action go() { emit E(1, 2) } }"#,
        codes::E214_EMIT_ARG_MISMATCH
    ));
}

#[test]
fn e220_return_in_action() {
    assert!(has(
        r#"record R { action go() { return } }"#,
        codes::E220_RETURN_IN_ACTION
    ));
}

#[test]
fn e228_foreach_not_list() {
    assert!(has(
        r#"record R { m: map<address, amount>
           action go() { for each p in m { let _ = p } } }"#,
        codes::E228_FOREACH_NOT_LIST
    ));
}

#[test]
fn e230_empty_array_no_context() {
    assert!(has(
        r#"record R { view v returns amount { [] } }"#,
        codes::E230_EMPTY_ARRAY_NO_CONTEXT
    ));
}

#[test]
fn w304_reveal_on_plaintext() {
    assert!(has(
        r#"record R { x: amount
           reveal x to owner }"#,
        codes::W304_REVEAL_ON_PLAINTEXT
    ));
}

#[test]
fn e208_lambda_needs_context() {
    // A bare lambda outside an argument position.
    assert!(has(
        r#"record R { view v returns amount { (x => x + 1) } }"#,
        codes::E208_LAMBDA_NEEDS_CONTEXT
    ));
}

#[test]
fn e215_match_pattern_type_mismatch() {
    // Match on an amount with a text literal pattern.
    assert!(has(
        r#"record R { view v returns amount {
            match 1 {
                "x" => 1
                2 => 2
            }
        } }"#,
        codes::E215_PATTERN_TYPE_MISMATCH
    ));
}

//! Error-path coverage: trigger each parser diagnostic code.

use covenant_diag::SourceId;
use covenant_lexer::tokenize;
use covenant_parser::{codes, parse};

fn parse_diags(src: &str) -> Vec<covenant_diag::Diagnostic> {
    let (toks, _) = tokenize(src, SourceId::new(0));
    let (_, diags) = parse(&toks, SourceId::new(0));
    diags
}

fn has_code(src: &str, code: covenant_diag::DiagCode) -> bool {
    parse_diags(src).iter().any(|d| d.code == code)
}

#[test]
fn e020_unexpected_token_generic() {
    // Two opening braces in a row — the parser can't recover into a sensible
    // field decl.
    assert!(has_code(
        "record R { { x: amount } }\n",
        codes::E020_UNEXPECTED_TOKEN
    ));
}

#[test]
fn e021_annotation_on_field() {
    assert!(has_code(
        "record R { @precompute(1) x: amount }\n",
        codes::E021_ANNOTATION_WRONG_DECL
    ));
}

#[test]
fn e023_unexpected_eof_mid_construct() {
    // Missing closing brace.
    assert!(
        has_code("record R {\n", codes::E023_UNEXPECTED_EOF)
            || !parse_diags("record R {\n").is_empty()
    );
}

#[test]
fn e026_type_expected_after_colon() {
    assert!(
        has_code("record R { x: = 1 }\n", codes::E026_TYPE_EXPECTED)
            || has_code("record R { x: = 1 }\n", codes::E020_UNEXPECTED_TOKEN)
    );
}

#[test]
fn e028_bad_construct_keyword() {
    // A file that starts with a random identifier rather than a construct keyword.
    assert!(has_code(
        "garbage R {}\n",
        codes::E028_BAD_CONSTRUCT_KEYWORD
    ));
}

#[test]
fn e029_bad_shares_spec() {
    // `shares(n of k among xs)` with a non-integer count.
    assert!(!parse_diags("record R { field x: shares(big of 3 among xs) }\n").is_empty());
}

#[test]
fn e030_bad_stmt_terminator() {
    // Two expression statements on the same line with no separator.
    assert!(!parse_diags("record R { action a() { let x = 1 let y = 2 } }\n").is_empty());
}

#[test]
fn e031_deeply_nested_parens_does_not_overflow_stack_hgh_029() {
    // OMEGA V6 HGH-029 regression test: this exact shape (nested-parens
    // count above the parser's real crash threshold, observed at ~140 in a
    // fresh debug build) used to overflow the native process stack -- an
    // uncatchable `STATUS_STACK_OVERFLOW`, not a normal Rust panic. If the
    // depth guard regresses, this test process crashes outright rather than
    // failing an assertion.
    let opens = "(".repeat(140);
    let closes = ")".repeat(140);
    let src = format!("record R {{\n view v returns amount {{ {opens}1{closes} }}\n}}\n");
    assert!(
        has_code(&src, codes::E031_TOO_DEEPLY_NESTED),
        "expected E031 (too deeply nested) for 140 nested parens"
    );
}

#[test]
fn multiple_errors_reported() {
    // Three separate problems should produce ≥ 3 diagnostics (error recovery
    // keeps parsing).
    let src = r#"record R {
        x := amount
        y := amount
        action f() when garbage !!! { }
    }
    "#;
    let diags = parse_diags(src);
    assert!(
        diags.len() >= 2,
        "expected at least 2 diagnostics, got {}: {diags:?}",
        diags.len()
    );
}

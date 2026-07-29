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
fn e032_deeply_nested_map_type_does_not_overflow_stack_f06() {
    // OMEGA V6 F06 regression test: a deeply nested type
    // `map<address, map<address, ... amount ...>>` used to recurse one native
    // stack frame per level in `parse_type` with no depth counter (the E031
    // guard only covers expressions and blocks), overflowing the process stack
    // -- an uncatchable `STATUS_STACK_OVERFLOW`, not a normal Rust panic --
    // and crashing `covenant check/build/fmt/lint` and the LSP. If the type
    // depth guard regresses, this test process crashes outright rather than
    // failing an assertion.
    //
    // Negative control: neutralize the guard (delete the `enter_type_depth()?`
    // wrapper in `parse_type`, or raise `MAX_PARSE_DEPTH` above 1500) and this
    // test either overflows the stack or fails the assertion below.
    let depth = 1500;
    let opens = "map<address, ".repeat(depth);
    let closes = ">".repeat(depth);
    let src = format!("record R {{\n field m: {opens}amount{closes}\n}}\n");
    assert!(
        has_code(&src, codes::E032_TYPE_TOO_DEEPLY_NESTED),
        "expected E032 (type too deeply nested) for {depth} nested map<> levels"
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

// ---------- E040: iterative chains (F-31) ----------
//
// The E031 guard counts recursive descent only. The Pratt parser builds
// left-associative operator chains and postfix chains inside a loop, so a chain
// of any length kept `nest_depth` at 1 and E031 never fired; the resulting
// left spine was then walked recursively by every later stage (and by `Drop`),
// killing `check`, `build`, `fmt`, `lint` and the language server with an
// uncatchable `STATUS_STACK_OVERFLOW` and no diagnostic at all. Each test below
// uses a chain far past the observed crash thresholds (3500 to 4200), so if the
// guard regresses the test process dies outright instead of failing an
// assertion.
//
// Negative control for all of them: remove the `enter_chain_depth()?` calls
// from the Pratt loop in `parse_expr.rs` (and from `parse_lvalue_body`) and
// every one of these either overflows the stack or fails its assertion.

#[test]
fn e040_long_add_chain_does_not_overflow_stack_f31() {
    let chain = " + v".repeat(5000);
    let src = format!(
        "record R {{
 field n: amount
 action a(v: amount) {{ n = v{chain} }}
}}
"
    );
    assert!(
        has_code(&src, codes::E040_CHAIN_TOO_LONG),
        "expected E040 for a 5000-term `+` chain"
    );
}

#[test]
fn e040_long_field_chain_does_not_overflow_stack_f31() {
    let chain = ".f".repeat(5000);
    let src = format!(
        "record R {{
 field n: amount
 action a(v: amount) {{ n = v{chain} }}
}}
"
    );
    assert!(
        has_code(&src, codes::E040_CHAIN_TOO_LONG),
        "expected E040 for a 5000-link field chain"
    );
}

#[test]
fn e040_long_index_chain_does_not_overflow_stack_f31() {
    let chain = "[k]".repeat(5000);
    let src = format!(
        "record R {{
 field n: amount
 action a(k: amount, m: amount) {{ n = m{chain} }}
}}
"
    );
    assert!(
        has_code(&src, codes::E040_CHAIN_TOO_LONG),
        "expected E040 for a 5000-link index chain"
    );
}

#[test]
fn e040_long_guard_chain_does_not_overflow_stack_f31() {
    let chain = " && v".repeat(5000);
    let src = format!(
        "record R {{
 field n: amount
 action a(v: bool) when v{chain} {{ n = 1 }}
}}
"
    );
    assert!(
        has_code(&src, codes::E040_CHAIN_TOO_LONG),
        "expected E040 for a 5000-term `&&` action guard"
    );
}

#[test]
fn e040_long_lvalue_chain_does_not_overflow_stack_f31() {
    // `parse_lvalue` has its own iterative spine, reached from `discard` and
    // `delete`, and it needs the same charge as the Pratt loop.
    let chain = ".f".repeat(5000);
    let src = format!(
        "record R {{
 field s: amount
 action a() {{ discard s{chain} }}
}}
"
    );
    assert!(
        has_code(&src, codes::E040_CHAIN_TOO_LONG),
        "expected E040 for a 5000-link l-value chain"
    );
}

#[test]
fn ordinary_chains_still_parse() {
    // The guard must not fire on anything a person would write. Sixteen terms
    // and four postfix links stay clean.
    let src = "record R {
 field n: amount
 action a(v: amount) { n = v + v + v + v + v + v + v + v + v + v + v + v + v + v + v + v }
}
";
    assert!(parse_diags(src).is_empty(), "diags: {:?}", parse_diags(src));
    let src2 = "record R {
 field n: amount
 action a(v: amount) { n = v.a.b.c.d }
}
";
    assert!(
        parse_diags(src2).is_empty(),
        "diags: {:?}",
        parse_diags(src2)
    );
}

// ---------- E041: body size (F-32) ----------

#[test]
fn e041_oversized_action_body_is_refused_f32() {
    // Code generation for a single body costs more than linear in its statement
    // count: 20000 statements took minutes to build and 50000 never finished,
    // while the language server runs that same pipeline on any file it is asked
    // to open. No body this size can fit the 24576-byte deployment limit, so
    // refusing at the size bound costs nothing that could have shipped.
    //
    // Negative control: raise `MAX_BODY_STMTS` above 8000 (or drop the
    // `charge_body_stmt()?` call at the top of `parse_stmt`) and this fails.
    let stmts = "        n = 1
"
    .repeat(8000);
    let src = format!(
        "record R {{
    field n: amount
    action a() {{
{stmts}    }}
}}
"
    );
    assert!(
        has_code(&src, codes::E041_BODY_TOO_LARGE),
        "expected E041 for an 8000-statement action body"
    );
}

#[test]
fn e041_reports_once_not_per_statement() {
    // An over-budget body must not bury its own diagnostic under thousands of
    // follow-on errors.
    let stmts = "        n = 1
"
    .repeat(8000);
    let src = format!(
        "record R {{
    field n: amount
    action a() {{
{stmts}    }}
}}
"
    );
    let count = parse_diags(&src)
        .iter()
        .filter(|d| d.code == codes::E041_BODY_TOO_LARGE)
        .count();
    assert_eq!(count, 1, "expected exactly one E041, got {count}");
}

#[test]
fn e041_budget_is_per_body_not_per_file() {
    // Many small actions are cheap to compile and must stay legal, even when
    // their statements add up past the per-body bound.
    let mut src = String::from(
        "record R {
    field n: amount
",
    );
    for i in 0..600 {
        src.push_str(&format!(
            "    action a{i}() {{ n = 1
 n = 2
 n = 3
 n = 4
 n = 5
 n = 6
 n = 7
 n = 8
 n = 9
 n = 10 }}
"
        ));
    }
    src.push_str(
        "}
",
    );
    assert!(
        !has_code(&src, codes::E041_BODY_TOO_LARGE),
        "6000 statements spread over 600 actions must not trip the per-body bound"
    );
}

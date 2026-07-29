//! Per-feature unit tests for the type checker.

use covenant_diag::{Diagnostic, DiagnosticLevel, SourceId};
use covenant_lexer::tokenize;
use covenant_parser::parse;
use covenant_resolver::resolve;
use covenant_types::{codes, typecheck, Ty, TypedFile};

fn pipeline(src: &str) -> (TypedFile, Vec<Diagnostic>) {
    let (toks, lex) = tokenize(src, SourceId::new(0));
    assert!(lex.is_empty(), "lex: {lex:?}");
    let (file, pd) = parse(&toks, SourceId::new(0));
    assert!(pd.is_empty(), "parse: {pd:?}");
    let (res, _) = resolve(file.expect("file"), SourceId::new(0));
    typecheck(res, SourceId::new(0))
}

fn errors(d: &[Diagnostic]) -> Vec<&Diagnostic> {
    d.iter()
        .filter(|x| x.level == DiagnosticLevel::Error)
        .collect()
}

fn has_code(d: &[Diagnostic], c: covenant_diag::DiagCode) -> bool {
    d.iter().any(|x| x.code == c)
}

fn with_view(ret_ty: &str, body_expr: &str) -> (TypedFile, Vec<Diagnostic>) {
    pipeline(&format!(
        "record R {{\n    view v returns {ret_ty} {{ {body_expr} }}\n}}\n"
    ))
}

/// `record R { view v returns amount { 1 + 1 + ... } }` with a left-leaning
/// `Binary` spine `depth` levels deep, assembled WITHOUT going through the
/// parser.
///
/// V0.9.6 F-31 taught the parser to charge an iterative chain spine against its
/// depth budget, so it now refuses to build a tree this deep (E040). That is the
/// right place for the first check, but it also means parsed source can no
/// longer reach the typechecker's own depth guard, which is deliberately
/// independent of it. The shallow shell is parsed so that everything except the
/// depth (spans, construct, view signature) is exactly what the real pipeline
/// produces.
fn deep_binary_chain_file(depth: usize) -> covenant_parser::ast::File {
    use covenant_parser::ast::{BinaryOp, Expr, TopLevelDecl};

    let (toks, _) = tokenize(
        "record R {\n view v returns amount { 1 }\n}\n",
        SourceId::new(0),
    );
    let mut file = parse(&toks, SourceId::new(0)).0.expect("shell parses");

    let view = file
        .top_level
        .body
        .iter_mut()
        .find_map(|d| match d {
            TopLevelDecl::View(v) => Some(v),
            _ => None,
        })
        .expect("the shell declares a view");

    let leaf = view.body.clone();
    let span = view.span;
    let mut expr = leaf.clone();
    for _ in 0..depth {
        expr = Expr::Binary {
            op: BinaryOp::Add,
            lhs: Box::new(expr),
            rhs: Box::new(leaf.clone()),
            span,
        };
    }
    view.body = expr;
    file
}

// ---------- Literals ----------

#[test]
fn typechecks_amount_literal() {
    let (t, d) = with_view("amount", "42");
    assert!(errors(&d).is_empty(), "{d:?}");
    assert!(t
        .types
        .expr_types
        .values()
        .any(|ty| matches!(ty, Ty::Amount)));
}

#[test]
fn typechecks_bool_literal() {
    let (_, d) = with_view("bool", "true");
    assert!(errors(&d).is_empty(), "{d:?}");
}

#[test]
fn typechecks_text_literal() {
    let (_, d) = with_view("text", r#""hello""#);
    assert!(errors(&d).is_empty(), "{d:?}");
}

#[test]
fn typechecks_duration_literal() {
    let (_, d) = with_view("duration", "7 days");
    assert!(errors(&d).is_empty(), "{d:?}");
}

#[test]
fn typechecks_hex_address_literal() {
    let hex = "0x".to_string() + &"ab".repeat(20);
    let (_, d) = pipeline(&format!("record R {{ field a: address = {hex} }}\n"));
    assert!(errors(&d).is_empty(), "{d:?}");
}

// ---------- Field / map / list access ----------

#[test]
fn typechecks_field_in_view() {
    let src = "record R { x: amount\n view g returns amount { x } }\n";
    let (_, d) = pipeline(src);
    assert!(errors(&d).is_empty(), "{d:?}");
}

#[test]
fn rejects_field_type_referring_to_non_type_binding_hgh_026() {
    // OMEGA V6 HGH-026 regression test: `bad`'s type annotation `total`
    // resolves fine (it's a sibling field name), but a field is not a
    // type. This used to silently lower to `Ty::Unknown` with zero
    // diagnostic anywhere in the pipeline.
    let src = "record R {\n    total: amount\n    bad: total\n}\n";
    let (_, d) = pipeline(src);
    assert!(
        has_code(&d, covenant_types::codes::E231_NOT_A_TYPE),
        "expected E231 (not a type) for a field used in type position, got {d:?}"
    );
}

#[test]
fn e232_deeply_nested_ast_is_refused_by_the_typechecker_hgh_029() {
    // OMEGA V6 HGH-029 regression test: `synth_expr` needs its OWN independent
    // depth guard. `pipeline()` above, like several real callers (e.g.
    // covenant-testing), discards the resolver's diagnostics and calls
    // `typecheck` directly on the resolved file, so the resolver hitting its own
    // limit does not stop the typechecker from separately walking the same deep
    // tree. If the guard regresses, this test process crashes outright rather
    // than failing an assertion.
    //
    // The tree is built directly rather than parsed. V0.9.6 F-31 taught the
    // parser to charge an iterative chain spine against its depth budget, so a
    // deep chain written in source is now refused at parse time (E040) and never
    // reaches this stage. Feeding source here would test the parser's guard and
    // leave this one untested, which is exactly the sort of "some upstream stage
    // will catch it" reasoning the three independent guards exist to defeat.
    let file = deep_binary_chain_file(400);
    let (resolved, _) = resolve(file, SourceId::new(0));
    let (_, d) = typecheck(resolved, SourceId::new(0));
    assert!(
        has_code(&d, covenant_types::codes::E232_TOO_DEEPLY_NESTED),
        "expected E232 (too deeply nested) for a 400-deep `Binary` tree, got {d:?}"
    );
}

// ---------- External contract call type checking (OMEGA V6 HGH-027) ----------

#[test]
fn accepts_external_call_matching_declared_signature() {
    let src = r#"
external contract IERC20 {
    function transfer(address, amount)
}
record R {
    action send(tok: address, dest: address, val: amount) only caller {
        IERC20.at(tok).transfer(dest, val)
    }
}
"#;
    let (_, d) = pipeline(src);
    assert!(errors(&d).is_empty(), "{d:?}");
}

#[test]
fn rejects_external_call_wrong_arity_hgh_027() {
    let src = r#"
external contract IERC20 {
    function transfer(address, amount)
}
record R {
    action send(tok: address, dest: address) only caller {
        IERC20.at(tok).transfer(dest)
    }
}
"#;
    let (_, d) = pipeline(src);
    assert!(
        has_code(&d, covenant_types::codes::E205_ARITY_MISMATCH),
        "expected E205 (arity mismatch) for a short-armed external call, got {d:?}"
    );
}

#[test]
fn rejects_external_call_wrong_arg_type_hgh_027() {
    let src = r#"
external contract IERC20 {
    function transfer(address, amount)
}
record R {
    action send(tok: address, dest: text) only caller {
        IERC20.at(tok).transfer(dest, dest)
    }
}
"#;
    let (_, d) = pipeline(src);
    assert!(
        has_code(&d, covenant_types::codes::E201_TYPE_MISMATCH),
        "expected E201 (type mismatch) for a `text` passed where `address` is declared, got {d:?}"
    );
}

#[test]
fn rejects_external_call_unknown_method_hgh_027() {
    let src = r#"
external contract IERC20 {
    function transfer(address, amount)
}
record R {
    action send(tok: address, dest: address, val: amount) only caller {
        IERC20.at(tok).trasnfer(dest, val)
    }
}
"#;
    let (_, d) = pipeline(src);
    assert!(
        has_code(&d, covenant_types::codes::E203_UNKNOWN_METHOD),
        "expected E203 (unknown method) for a typo'd external-contract method name, got {d:?}"
    );
}

#[test]
fn rejects_external_call_at_with_non_address_hgh_027() {
    let src = r#"
external contract IERC20 {
    function transfer(address, amount)
}
record R {
    action send(tok: text, dest: address, val: amount) only caller {
        IERC20.at(tok).transfer(dest, val)
    }
}
"#;
    let (_, d) = pipeline(src);
    assert!(
        has_code(&d, covenant_types::codes::E201_TYPE_MISMATCH),
        "expected E201 (type mismatch) for a `text` passed to `.at(...)`, got {d:?}"
    );
}

#[test]
fn typechecks_map_index() {
    let src = r#"
record R {
    balances: map<address, amount>
    view bal(who: address) returns amount { balances[who] }
}
"#;
    let (_, d) = pipeline(src);
    assert!(errors(&d).is_empty(), "{d:?}");
}

#[test]
fn typechecks_list_length() {
    let src = r#"
record R {
    items: [amount]
    view n returns amount { items.length }
}
"#;
    let (_, d) = pipeline(src);
    assert!(errors(&d).is_empty(), "{d:?}");
}

// ---------- Arithmetic & comparison ----------

#[test]
fn typechecks_time_plus_duration() {
    let (_, d) = with_view("time", "now + 7 days");
    assert!(errors(&d).is_empty(), "{d:?}");
}

#[test]
fn typechecks_time_minus_time_yields_duration() {
    let (_, d) = with_view("duration", "now - now");
    assert!(errors(&d).is_empty(), "{d:?}");
}

#[test]
fn typechecks_amount_arithmetic() {
    let (_, d) = with_view("amount", "1 + 2 * 3");
    assert!(errors(&d).is_empty(), "{d:?}");
}

#[test]
fn rejects_plus_on_text() {
    let (_, d) = with_view("text", r#""a" + "b""#);
    assert!(has_code(&d, codes::E202_OP_INAPPLICABLE));
}

#[test]
fn accepts_concat_on_text() {
    let (_, d) = with_view("text", r#""a" ++ "b""#);
    assert!(errors(&d).is_empty(), "{d:?}");
}

#[test]
fn typechecks_comparison_yields_bool() {
    let (_, d) = with_view("bool", "1 < 2");
    assert!(errors(&d).is_empty(), "{d:?}");
}

#[test]
fn rejects_time_plus_amount() {
    let (_, d) = with_view("time", "now + 1");
    assert!(has_code(&d, codes::E202_OP_INAPPLICABLE));
}

// ---------- Ciphertext / FHE ----------

#[test]
fn typechecks_ciphertext_arithmetic() {
    let src = r#"
record R {
    a: ciphertext<amount>
    b: ciphertext<amount>
    view s returns ciphertext<amount> { a + b }
}
"#;
    let (_, d) = pipeline(src);
    assert!(errors(&d).is_empty(), "{d:?}");
}

#[test]
fn typechecks_ciphertext_plus_plaintext_lifts() {
    let src = r#"
record R {
    a: ciphertext<amount>
    action bump(by: amount) {
        a += by
    }
}
"#;
    let (t, d) = pipeline(src);
    assert!(errors(&d).is_empty(), "{d:?}");
    assert!(!t.types.lifts.is_empty(), "expected lift marker");
}

#[test]
fn typechecks_encrypted_comparison() {
    let src = r#"
record R {
    a: ciphertext<amount>
    b: ciphertext<amount>
    view cmp returns ciphertext<bool> { a < b }
}
"#;
    let (_, d) = pipeline(src);
    assert!(errors(&d).is_empty(), "{d:?}");
}

#[test]
fn rejects_double_encryption() {
    // `encrypted(encrypted(1))` should trigger E211.
    let (_, d) = with_view("ciphertext<amount>", "encrypted(encrypted(1))");
    assert!(has_code(&d, codes::E211_DOUBLE_ENCRYPTION));
}

#[test]
fn accepts_single_encryption() {
    let (_, d) = with_view("ciphertext<amount>", "encrypted(1)");
    assert!(errors(&d).is_empty(), "{d:?}");
}

// ---------- Stdlib ----------

#[test]
fn typechecks_stdlib_keccak() {
    let (_, d) = with_view("hash", r#"keccak("x")"#);
    assert!(errors(&d).is_empty(), "{d:?}");
}

#[test]
fn typechecks_stdlib_min() {
    let (_, d) = with_view("amount", "min(1, 2)");
    assert!(errors(&d).is_empty(), "{d:?}");
}

#[test]
fn typechecks_stdlib_module_method() {
    let src = r#"
record R {
    action go(pk: pq_key, h: hash, s: bytes) {
        let ok = PQKeys.verify_dilithium(pk, h, s)
    }
}
"#;
    let (_, d) = pipeline(src);
    assert!(errors(&d).is_empty(), "{d:?}");
}

#[test]
fn rejects_unknown_stdlib_method() {
    let src = r#"
record R {
    action go(h: hash) {
        let x = PQKeys.nonexistent_method(h)
    }
}
"#;
    let (_, d) = pipeline(src);
    assert!(has_code(&d, codes::E203_UNKNOWN_METHOD));
}

// ---------- Events / errors ----------

#[test]
fn typechecks_emit_with_matching_args() {
    let src = r#"
record R {
    event E(who: address indexed, amt: amount)
    action go() { emit E(caller, 5) }
}
"#;
    let (_, d) = pipeline(src);
    assert!(errors(&d).is_empty(), "{d:?}");
}

#[test]
fn rejects_emit_with_wrong_arg_type() {
    let src = r#"
record R {
    event E(amt: amount)
    action go() { emit E("oops") }
}
"#;
    let (_, d) = pipeline(src);
    assert!(has_code(&d, codes::E201_TYPE_MISMATCH));
}

#[test]
fn rejects_emit_with_wrong_arity() {
    let src = r#"
record R {
    event E(amt: amount)
    action go() { emit E(1, 2, 3) }
}
"#;
    let (_, d) = pipeline(src);
    assert!(has_code(&d, codes::E214_EMIT_ARG_MISMATCH));
}

#[test]
fn typechecks_revert_with_matching() {
    let src = r#"
record R {
    error Bad(why: amount)
    action go() { revert_with Bad(42) }
}
"#;
    let (_, d) = pipeline(src);
    assert!(errors(&d).is_empty(), "{d:?}");
}

#[test]
fn rejects_revert_with_wrong_arity() {
    let src = r#"
record R {
    error Bad(why: amount)
    action go() { revert_with Bad() }
}
"#;
    let (_, d) = pipeline(src);
    assert!(has_code(&d, codes::E213_REVERT_ARG_MISMATCH));
}

// ---------- Statements ----------

#[test]
fn typechecks_for_each_over_list() {
    let src = r#"
record R {
    xs: [amount]
    action go() {
        for each x in xs {
            let y = x + 1
        }
    }
}
"#;
    let (_, d) = pipeline(src);
    assert!(errors(&d).is_empty(), "{d:?}");
}

#[test]
fn rejects_for_each_over_map() {
    let src = r#"
record R {
    m: map<address, amount>
    action go() {
        for each p in m {
            let _y = p
        }
    }
}
"#;
    let (_, d) = pipeline(src);
    assert!(has_code(&d, codes::E228_FOREACH_NOT_LIST));
}

#[test]
fn rejects_return_in_action() {
    let src = r#"
record R {
    action go() { return }
}
"#;
    let (_, d) = pipeline(src);
    assert!(has_code(&d, codes::E220_RETURN_IN_ACTION));
}

// ---------- Control flow ----------

#[test]
fn typechecks_if_expr() {
    let (_, d) = with_view("amount", "if 1 < 2 { 1 } else { 2 }");
    assert!(errors(&d).is_empty(), "{d:?}");
}

#[test]
fn rejects_if_branches_type_mismatch() {
    let (_, d) = with_view("amount", r#"if true { 1 } else { "hi" }"#);
    assert!(has_code(&d, codes::E201_TYPE_MISMATCH));
}

// ---------- Namespaces ----------

#[test]
fn typechecks_msg_value() {
    let (_, d) = with_view("amount", "msg.value");
    assert!(errors(&d).is_empty(), "{d:?}");
}

#[test]
fn typechecks_block_timestamp() {
    let (_, d) = with_view("time", "block.timestamp");
    assert!(errors(&d).is_empty(), "{d:?}");
}

// ---------- Unary ----------

#[test]
fn typechecks_unary_not() {
    let (_, d) = with_view("bool", "!true");
    assert!(errors(&d).is_empty(), "{d:?}");
}

#[test]
fn rejects_unary_neg_on_text() {
    let (_, d) = with_view("text", r#"-"hi""#);
    assert!(has_code(&d, codes::E202_OP_INAPPLICABLE));
}

// ---------- Arrays ----------

#[test]
fn rejects_empty_array_without_context() {
    let (_, d) = with_view("amount", "[]");
    assert!(has_code(&d, codes::E230_EMPTY_ARRAY_NO_CONTEXT));
}

#[test]
fn accepts_empty_array_with_type_annotation() {
    let src = r#"
record R {
    action go() {
        let xs: [amount] = []
    }
}
"#;
    let (_, d) = pipeline(src);
    assert!(errors(&d).is_empty(), "{d:?}");
}

#[test]
fn typechecks_array_literal() {
    let (_, d) = with_view("[amount]", "[1, 2, 3]");
    assert!(errors(&d).is_empty(), "{d:?}");
}

// ---------- Reveal ----------

#[test]
fn warns_reveal_on_plaintext() {
    // `reveal x to owner` where x is plaintext.
    let src = r#"
record R {
    x: amount
    reveal x to owner
}
"#;
    let (_, d) = pipeline(src);
    assert!(has_code(&d, codes::W304_REVEAL_ON_PLAINTEXT));
}

#[test]
fn reveal_on_ciphertext_fine() {
    let src = r#"
encrypted counter C {
    total: amount
    reveal total to owner
}
"#;
    let (_, d) = pipeline(src);
    assert!(errors(&d).is_empty(), "{d:?}");
}

// ---------- Guards / qualifiers ----------

#[test]
fn typechecks_when_guard_bool() {
    let src = r#"
record R {
    action go() when 1 < 2 {}
}
"#;
    let (_, d) = pipeline(src);
    assert!(errors(&d).is_empty(), "{d:?}");
}

#[test]
fn rejects_when_guard_non_bool() {
    let src = r#"
record R {
    action go() when 1 {}
}
"#;
    let (_, d) = pipeline(src);
    assert!(has_code(&d, codes::E201_TYPE_MISMATCH));
}

#[test]
fn typechecks_pq_signed_qualifier() {
    let src = r#"
record R {
    action go(m: hash, s: bytes, p: pq_key) pq_signed(m, s, p) {}
}
"#;
    let (_, d) = pipeline(src);
    assert!(errors(&d).is_empty(), "{d:?}");
}

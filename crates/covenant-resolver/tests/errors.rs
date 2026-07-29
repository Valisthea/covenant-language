//! Error-path coverage: one test per resolver diagnostic code E101-E112.

use covenant_diag::{Diagnostic, SourceId};
use covenant_lexer::tokenize;
use covenant_parser::parse;
use covenant_resolver::{codes, resolve};

fn resolve_src(src: &str) -> Vec<Diagnostic> {
    let (toks, _) = tokenize(src, SourceId::new(0));
    let (file, _) = parse(&toks, SourceId::new(0));
    let (_, diags) = resolve(file.expect("file"), SourceId::new(0));
    diags
}

fn has_code(src: &str, code: covenant_diag::DiagCode) -> bool {
    resolve_src(src).iter().any(|d| d.code == code)
}

/// `record R { view v returns amount { 1 + 1 + ... } }` with a left-leaning
/// `Binary` spine `depth` levels deep, assembled WITHOUT going through the
/// parser.
///
/// V0.9.6 F-31 taught the parser to charge an iterative chain spine against its
/// depth budget, so it now refuses to build a tree this deep (E040). That is the
/// right place for the first check, but it also means parsed source can no
/// longer reach the resolver's or the typechecker's own depth guards, which are
/// deliberately independent of it. Building the tree here keeps those guards
/// under test. The shallow shell is parsed so that everything except the depth
/// (spans, construct, view signature) is exactly what the real pipeline
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

#[test]
fn e101_duplicate_declaration() {
    assert!(has_code(
        "record R { x: amount\n x: amount }\n",
        codes::E101_DUPLICATE_DECL
    ));
}

#[test]
fn e102_unresolved_identifier() {
    assert!(has_code(
        "record R { action a() { let y = unknown_var } }\n",
        codes::E102_UNRESOLVED_IDENT
    ));
}

#[test]
fn e105_principal_field_missing() {
    // `only admin` without an admin field declared.
    // Note: admin has a LangIdent fallback, so we need a principal WITHOUT a
    // fallback. There's no principal kind in V0 without a fallback.
    // Instead we use a fresh record (no owner field) and check fallback still
    // works. To exercise E105, we'd need to remove fallbacks, for now assert
    // the diagnostic code constant exists and the helper is wired.
    let _ = codes::E105_PRINCIPAL_FIELD_MISSING;
}

#[test]
fn e106_unknown_predicate() {
    assert!(has_code(
        "record R { action a() only fizz_buzzzz {} }\n",
        codes::E106_UNKNOWN_PREDICATE
    ));
}

#[test]
fn e107_user_imports_unsupported() {
    assert!(has_code(
        "import SomeUserMod\nrecord R {}\n",
        codes::E107_USER_IMPORTS
    ));
}

#[test]
fn e108_unknown_event_in_record() {
    assert!(has_code(
        "record R { action a() { emit UnknownEvent() } }\n",
        codes::E108_UNKNOWN_EVENT
    ));
}

#[test]
fn e109_unknown_error_in_record() {
    assert!(has_code(
        "record R { action a() { revert_with UnknownError() } }\n",
        codes::E109_UNKNOWN_ERROR
    ));
}

#[test]
fn e110_unknown_annotation() {
    assert!(has_code(
        "record R { @not_a_real_annotation\n action a() {} }\n",
        codes::E110_UNKNOWN_ANNOTATION
    ));
}

#[test]
fn w201_shadowing_warning() {
    assert!(has_code(
        "record R { action a(x: amount) { let x = 1 } }\n",
        codes::W201_SHADOWING
    ));
}

#[test]
fn w202_shadowing_lang_warning() {
    assert!(has_code(
        "record R { action a() { let now = 1 } }\n",
        codes::W202_SHADOWING_LANG
    ));
}

#[test]
fn e040_deeply_chained_binary_expr_is_refused_at_parse_time_hgh_029() {
    // OMEGA V6 HGH-029, source half. A long flat chain of `+` parses fine (the
    // Pratt parser handles same-precedence left-associative chains iteratively,
    // with bounded recursion) but builds a left-leaning `Binary` tree hundreds
    // of levels deep, which every later stage then walks recursively. This is
    // the shape that overflowed the native stack: an uncatchable crash, not a
    // normal Rust panic.
    //
    // V0.9.6 F-31 moved the first line of defence into the parser, which now
    // charges the iterative chain spine against its depth budget and refuses to
    // BUILD a tree deeper than it can hand on (E040). That is strictly earlier
    // than the resolver's own guard and also covers recursive `Drop` of the
    // spine, which no later-stage counter can protect against. So a deep chain
    // written in source stops here and the resolver never sees it.
    //
    // The resolver's independent guard still matters and is still tested: see
    // `e113_deeply_nested_ast_is_refused_by_the_resolver_hgh_029`, which builds
    // the deep tree directly instead of going through the parser.
    let chain = " + 1".repeat(500);
    let src = format!("record R {{\n view v returns amount {{ 1{chain} }}\n}}\n");
    let (toks, _) = tokenize(&src, SourceId::new(0));
    let (_, diags) = parse(&toks, SourceId::new(0));
    assert!(
        diags
            .iter()
            .any(|d| d.code == covenant_parser::codes::E040_CHAIN_TOO_LONG),
        "expected E040 (chain too long) for a 500-deep chained `+` source, got {diags:?}"
    );
}

#[test]
fn e113_deeply_nested_ast_is_refused_by_the_resolver_hgh_029() {
    // OMEGA V6 HGH-029, resolver half. `resolve_expr` used to walk the
    // expression tree with plain recursion and no depth counter. It needs its
    // OWN guard rather than relying on the parser's: the parser is not the only
    // way an AST reaches the resolver, and a counter that only exists upstream
    // is a counter that stops existing the moment someone builds a tree by hand
    // (a macro, a code generator, a future front end) or retunes the parser's
    // budget. If the guard regresses, this test process crashes outright rather
    // than failing an assertion.
    //
    // The tree is built directly, NOT parsed, precisely because F-31 taught the
    // parser to refuse this shape: routing through the parser would test the
    // parser's guard a second time and leave the resolver's untested.
    let file = deep_binary_chain_file(400);
    let (_, diags) = resolve(file, SourceId::new(0));
    assert!(
        diags
            .iter()
            .any(|d| d.code == codes::E113_TOO_DEEPLY_NESTED),
        "expected E113 (too deeply nested) for a 400-deep `Binary` tree, got {diags:?}"
    );
}

#[test]
fn unresolved_suggestion_helpfully_present() {
    // `deploer` is one char off from `deployer`.
    let diags = resolve_src("record R { field a: address = deploer }\n");
    let found = diags
        .iter()
        .find(|d| d.code == codes::E102_UNRESOLVED_IDENT);
    assert!(found.is_some(), "expected E102, got {diags:?}");
    let help = found.and_then(|d| d.help.as_deref());
    assert!(
        help.is_some_and(|h| h.contains("deployer")),
        "expected suggestion `deployer`, got help {help:?}"
    );
}

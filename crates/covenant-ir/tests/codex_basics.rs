//! Integration tests: build IR for each Codex Basics fixture.

use covenant_diag::{Diagnostic, DiagnosticLevel, SourceId};
use covenant_ir::{build_ir, codes, validate, IrFunctionKind, IrModule, Opcode};
use covenant_lexer::tokenize;
use covenant_parser::parse;
use covenant_privacy::analyze_privacy;
use covenant_resolver::resolve;
use covenant_types::typecheck;

fn pipeline(src: &str) -> (IrModule, Vec<Diagnostic>) {
    let (toks, lex) = tokenize(src, SourceId::new(0));
    assert!(lex.is_empty(), "lex: {lex:?}");
    let (file, pd) = parse(&toks, SourceId::new(0));
    assert!(pd.is_empty(), "parse: {pd:?}");
    let (res, rd) = resolve(file.expect("file"), SourceId::new(0));
    assert!(
        rd.iter().all(|d| d.level != DiagnosticLevel::Error),
        "resolve: {rd:?}"
    );
    let (typed, td) = typecheck(res, SourceId::new(0));
    assert!(
        td.iter().all(|d| d.level != DiagnosticLevel::Error),
        "typecheck: {td:?}"
    );
    let (checked, ad) = analyze_privacy(typed, SourceId::new(0));
    assert!(
        ad.iter().all(|d| d.level != DiagnosticLevel::Error),
        "privacy: {ad:?}"
    );
    build_ir(checked, SourceId::new(0))
}

#[test]
fn example_1_hello_builds() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_01_hello.cov");
    let (m, diags) = pipeline(src);
    assert!(diags.is_empty(), "{diags:?}");
    let vdiags = validate(&m);
    assert!(vdiags.is_empty(), "validator: {vdiags:?}");
    assert_eq!(m.functions.len(), 2, "Hello has 1 action + 1 view");
    assert!(matches!(m.functions[0].kind, IrFunctionKind::Action));
    assert!(matches!(m.functions[1].kind, IrFunctionKind::View));
}

#[test]
fn example_2_coin_builds() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov");
    let (m, diags) = pipeline(src);
    assert!(diags.is_empty(), "{diags:?}");
    let vdiags = validate(&m);
    assert!(vdiags.is_empty(), "validator: {vdiags:?}");
    // Coin has no actions/views: only metadata.
    assert_eq!(m.functions.len(), 0);
    assert!(m.metadata.contains_key("symbol"));
    assert!(m.metadata.contains_key("decimals"));
    assert!(m.metadata.contains_key("supply"));
}

#[test]
fn example_3_open_ballot_builds() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_03_open_ballot.cov");
    let (m, diags) = pipeline(src);
    assert!(diags.is_empty(), "{diags:?}");
    let vdiags = validate(&m);
    assert!(vdiags.is_empty(), "validator: {vdiags:?}");
    // One action (cast) + one reveal (winner).
    assert_eq!(m.functions.len(), 2);
    let has_emit = m.functions.iter().any(|f| {
        f.blocks
            .iter()
            .flat_map(|b| b.instructions.iter())
            .any(|i| matches!(i.opcode, Opcode::Emit(_)))
    });
    assert!(has_emit, "expected an Emit opcode");
}

#[test]
fn example_4_shielded_counter_has_fhe_and_reveal() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_04_shielded_counter.cov");
    let (m, diags) = pipeline(src);
    assert!(diags.is_empty(), "{diags:?}");
    let vdiags = validate(&m);
    assert!(vdiags.is_empty(), "validator: {vdiags:?}");
    // Action `bump` performs total += by with auto-lift → FheEncryptTrivial + FheAdd.
    let has_lift = m.functions.iter().any(|f| {
        f.blocks
            .iter()
            .flat_map(|b| b.instructions.iter())
            .any(|i| matches!(i.opcode, Opcode::FheEncryptTrivial) && i.metadata.from_lift)
    });
    assert!(has_lift, "expected FheEncryptTrivial with from_lift");
    let has_fhe_add = m.functions.iter().any(|f| {
        f.blocks
            .iter()
            .flat_map(|b| b.instructions.iter())
            .any(|i| matches!(i.opcode, Opcode::FheAdd))
    });
    assert!(has_fhe_add, "expected FheAdd");
    // Reveal body should emit RevealDecrypt.
    let has_reveal = m.functions.iter().any(|f| {
        f.blocks
            .iter()
            .flat_map(|b| b.instructions.iter())
            .any(|i| matches!(i.opcode, Opcode::RevealDecrypt))
    });
    assert!(has_reveal, "expected RevealDecrypt in reveal body");
}

#[test]
fn example_5_quantum_board_has_pq_verify() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_05_quantum_board.cov");
    let (m, diags) = pipeline(src);
    // The board's `posts` collection has no storage field: nothing in the
    // compiler allocates one. `append post { .. }` therefore built the element
    // and dropped it (a successful, gas-burning, write-nothing call in a
    // construct whose entire point is being append-only), and `posts[i].<field>`
    // read storage slot 0, i.e. the construct's first declared field. Both are
    // now refused, so this fixture no longer builds clean. Restore
    // `assert!(diags.is_empty())` only once a board actually allocates `posts`.
    assert!(
        diags
            .iter()
            .any(|d| d.code == codes::E430_APPEND_UNBACKED_COLLECTION),
        "expected E430 for `append post` with no storage field, got: {diags:?}"
    );
    assert!(
        diags
            .iter()
            .any(|d| d.code == codes::E431_IMPLICIT_COLLECTION_UNBACKED),
        "expected E431 for reads of `posts`, got: {diags:?}"
    );
    let vdiags = validate(&m);
    assert!(vdiags.is_empty(), "validator: {vdiags:?}");
    let has_pq = m.functions.iter().any(|f| {
        f.blocks
            .iter()
            .flat_map(|b| b.instructions.iter())
            .any(|i| matches!(i.opcode, Opcode::PqVerifyDilithium))
    });
    assert!(
        has_pq,
        "expected PqVerifyDilithium from pq_signed qualifier"
    );
}

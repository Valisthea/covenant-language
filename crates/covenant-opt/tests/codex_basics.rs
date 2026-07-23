//! Integration tests: optimize each Codex Basics fixture.

use covenant_diag::SourceId;
use covenant_ir::{build_ir, validate, IrModule};
use covenant_lexer::tokenize;
use covenant_opt::{optimize, total_instructions, OptimizerConfig};
use covenant_parser::parse;
use covenant_privacy::analyze_privacy;
use covenant_resolver::resolve;
use covenant_types::typecheck;

fn pipeline(src: &str) -> IrModule {
    let (toks, _) = tokenize(src, SourceId::new(0));
    let (file, _) = parse(&toks, SourceId::new(0));
    let (res, _) = resolve(file.unwrap(), SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (checked, _) = analyze_privacy(typed, SourceId::new(0));
    let (module, _) = build_ir(checked, SourceId::new(0));
    module
}

fn check_basics(name: &str, src: &str) {
    let module = pipeline(src);
    let before = total_instructions(&module);
    let fns_before = module.functions.len();
    let fields_before = module.fields.len();

    let (optimized, diags) = optimize(module, OptimizerConfig::default());
    let after = total_instructions(&optimized);
    let errs: Vec<_> = diags
        .iter()
        .filter(|d| d.level == covenant_diag::DiagnosticLevel::Error)
        .collect();
    assert!(errs.is_empty(), "{name}: optimizer errors {errs:?}");

    let vdiags = validate(&optimized);
    assert!(vdiags.is_empty(), "{name}: validator post-opt: {vdiags:?}");

    assert!(
        after <= before,
        "{name}: optimizer inflated IR ({before} -> {after})"
    );
    assert_eq!(
        optimized.functions.len(),
        fns_before,
        "{name}: optimizer removed a function"
    );
    assert_eq!(
        optimized.fields.len(),
        fields_before,
        "{name}: optimizer removed a field"
    );
}

#[test]
fn example_1_hello_round_trip() {
    check_basics(
        "hello",
        include_str!("../../covenant-lexer/tests/fixtures/example_01_hello.cov"),
    );
}

#[test]
fn example_2_coin_round_trip() {
    check_basics(
        "coin",
        include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov"),
    );
}

#[test]
fn example_3_open_ballot_round_trip() {
    check_basics(
        "open_ballot",
        include_str!("../../covenant-lexer/tests/fixtures/example_03_open_ballot.cov"),
    );
}

#[test]
fn example_4_shielded_counter_round_trip() {
    check_basics(
        "shielded_counter",
        include_str!("../../covenant-lexer/tests/fixtures/example_04_shielded_counter.cov"),
    );
}

#[test]
fn example_5_quantum_board_round_trip() {
    check_basics(
        "quantum_board",
        include_str!("../../covenant-lexer/tests/fixtures/example_05_quantum_board.cov"),
    );
}

#[test]
fn all_disabled_preserves_module() {
    let module = pipeline(include_str!(
        "../../covenant-lexer/tests/fixtures/example_05_quantum_board.cov"
    ));
    let before = total_instructions(&module);
    let (out, _) = optimize(module, OptimizerConfig::none());
    assert_eq!(total_instructions(&out), before);
}

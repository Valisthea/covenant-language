//! Regression test for the first crash found by the cargo-fuzz
//! `compile_pipeline` target (weekly run 29708717546, artifact
//! `crash-e23701c003cf79fd2f50c0f9bc45efd0b6dd12cd`).
//!
//! `emit_text_return` asserted `len <= 32` and panicked otherwise, so the
//! whole compiler aborted with an ICE — "internal compiler error, please
//! report at ..." — on input as ordinary as a token whose `name:` runs past
//! 32 bytes. `covenant check` passed; only `covenant build` blew up.
//!
//! The fuzzer reached it by mutating the shipped `example_02_coin.cov` seed
//! into a 131-byte symbol. The human-sized case is a perfectly reasonable
//! token name, which is why this mattered: a compiler crash on valid-looking
//! user input is a launch-blocking bug, not an edge case.
//!
//! It is now E521, a normal diagnostic.

use covenant_diag::{Diagnostic, DiagnosticLevel, SourceId};
use covenant_evm_backend::{codegen_evm, EvmConfig};
use covenant_ir::build_ir;
use covenant_lexer::tokenize;
use covenant_opt::{optimize, OptimizerConfig};
use covenant_parser::parse;
use covenant_privacy::analyze_privacy;
use covenant_resolver::resolve;
use covenant_stdlib::{lower_stdlib, StdlibConfig};
use covenant_types::typecheck;

const E521: u32 = 521;

/// Runs the full pipeline. Must not panic for any input — that is the point.
fn compile(src: &str) -> Vec<Diagnostic> {
    let (toks, _) = tokenize(src, SourceId::new(0));
    let (file, _) = parse(&toks, SourceId::new(0));
    let Some(file) = file else { return vec![] };
    let (res, _) = resolve(file, SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (checked, _) = analyze_privacy(typed, SourceId::new(0));
    let (module, mut diags) = build_ir(checked, SourceId::new(0));
    let (module, std_diags) = lower_stdlib(module, StdlibConfig::default());
    diags.extend(std_diags);
    let (optimized, opt_diags) = optimize(module, OptimizerConfig::default());
    diags.extend(opt_diags);
    let (_, evm_diags) = codegen_evm(optimized, EvmConfig::default());
    diags.extend(evm_diags);
    diags
}

fn has_e521(diags: &[Diagnostic]) -> bool {
    diags
        .iter()
        .any(|d| d.level == DiagnosticLevel::Error && d.code.0 == E521)
}

/// The realistic shape of the bug: a token name a human would plausibly write.
#[test]
fn long_token_name_reports_e521_instead_of_panicking() {
    let src = r#"
token LongName {
    symbol: "COIN"
    name: "A perfectly reasonable token name that happens to exceed thirty-two bytes"
    decimals: 18
    supply: 1000 to deployer
}
"#;
    let diags = compile(src);
    assert!(
        has_e521(&diags),
        "expected E521 for an over-long text constant, got: {diags:?}"
    );
}

/// The fuzzer's own discovery: a very long symbol.
#[test]
fn long_symbol_reports_e521_instead_of_panicking() {
    let symbol = "C".repeat(131);
    let src = format!(
        r#"
token Coin {{
    symbol: "{symbol}"
    name: "Coin"
    decimals: 18
    supply: 1000 to deployer
}}
"#
    );
    let diags = compile(&src);
    assert!(
        has_e521(&diags),
        "expected E521 for a 131-byte symbol, got: {diags:?}"
    );
}

/// Exactly 32 bytes is the documented limit and must still compile — the
/// boundary the assert used to guard.
#[test]
fn exactly_32_bytes_still_compiles() {
    let name = "A".repeat(32);
    let src = format!(
        r#"
token Boundary {{
    symbol: "BND"
    name: "{name}"
    decimals: 18
    supply: 1000 to deployer
}}
"#
    );
    let diags = compile(&src);
    assert!(
        !has_e521(&diags),
        "32 bytes is within the encoder's range and must not be refused: {diags:?}"
    );
}

//! OMEGA V6 MED-003 regression test.
//!
//! Before this fix, a `view`/`reveal` returning a non-constant dynamic ABI
//! type (`text`/`bytes`/a list -- e.g. `view read returns text { greeting }`,
//! literally the language's own "Hello World" example) silently ABI-encoded
//! as a single raw word instead of the offset+length+data a spec-compliant
//! caller expects for a dynamic type, with zero diagnostic anywhere. This is
//! now flagged with W507 (a warning, not a hard error: unlike the narrower
//! CRT-007/pq_key case, this pattern is too common to hard-fail without a
//! real dynamic-ABI-encoding implementation, which is tracked separately in
//! DEBT.md) -- compilation still succeeds, bytecode is unchanged.

use covenant_diag::{Diagnostic, DiagnosticLevel, SourceId};
use covenant_evm_backend::{codegen_evm, codes::W507_DYNAMIC_RETURN_NOT_ENCODED, EvmConfig};
use covenant_ir::build_ir;
use covenant_lexer::tokenize;
use covenant_opt::{optimize, OptimizerConfig};
use covenant_parser::parse;
use covenant_privacy::analyze_privacy;
use covenant_resolver::resolve;
use covenant_types::typecheck;

const HELLO: &str = include_str!("../../covenant-lexer/tests/fixtures/example_01_hello.cov");

fn compile(src: &str) -> Vec<Diagnostic> {
    let (toks, _) = tokenize(src, SourceId::new(0));
    let (file, _) = parse(&toks, SourceId::new(0));
    let (res, _) = resolve(file.unwrap(), SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (checked, _) = analyze_privacy(typed, SourceId::new(0));
    let (module, _) = build_ir(checked, SourceId::new(0));
    let (optimized, _) = optimize(module, OptimizerConfig::default());
    let (_, diags) = codegen_evm(optimized, EvmConfig::default());
    diags
}

#[test]
fn non_constant_text_return_emits_w507_warning_not_error() {
    let diags = compile(HELLO);
    let w507 = diags
        .iter()
        .find(|d| d.code == W507_DYNAMIC_RETURN_NOT_ENCODED);
    assert!(
        w507.is_some(),
        "expected W507 for `view read returns text {{ greeting }}`, got {diags:?}"
    );
    assert_eq!(
        w507.unwrap().level,
        DiagnosticLevel::Warning,
        "W507 must be a warning, not a hard error -- this is the language's own \
         Hello World pattern and must keep compiling"
    );
    // No error-level diagnostics: compilation must succeed.
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "expected a clean compile, got {errors:?}"
    );
}

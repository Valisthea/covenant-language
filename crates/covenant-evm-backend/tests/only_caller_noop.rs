//! F05 regression — `only caller` is a no-op guard and must be diagnosed.
//!
//! `only caller` lowers to `msg.sender == msg.sender` (always true) and emits
//! ZERO caller checks. It was the one degenerate `only` principal that produced
//! NO diagnostic, while every other unenforceable principal already warns. The
//! fix surfaces it as W508 so an accidental no-op guard is no longer silent.
//!
//! It is intentionally a WARNING, not a hard error: the bytecode is not wrong
//! (it faithfully means "no restriction"), and `only caller` is used across the
//! examples/test-suite as an explicit "anyone" marker — a hard error would
//! break passing code for no correctness gain.

use covenant_diag::{Diagnostic, SourceId};
use covenant_evm_backend::codes::W508_ONLY_CALLER_NOOP;
use covenant_evm_backend::{codegen_evm, EvmConfig};
use covenant_ir::build_ir;
use covenant_lexer::tokenize;
use covenant_opt::{optimize, OptimizerConfig};
use covenant_parser::parse;
use covenant_privacy::analyze_privacy;
use covenant_resolver::resolve;
use covenant_stdlib::{lower_stdlib, StdlibConfig};
use covenant_types::typecheck;

fn codegen_diags(src: &str) -> Vec<Diagnostic> {
    let (toks, _) = tokenize(src, SourceId::new(0));
    let (file, _) = parse(&toks, SourceId::new(0));
    let (res, _) = resolve(file.unwrap(), SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (checked, _) = analyze_privacy(typed, SourceId::new(0));
    let (module, _) = build_ir(checked, SourceId::new(0));
    let (with_std, _) = lower_stdlib(module, StdlibConfig::default());
    let (optimized, _) = optimize(with_std, OptimizerConfig::default());
    let (_artifact, diags) = codegen_evm(optimized, EvmConfig::default());
    diags
}

#[test]
fn only_caller_guard_is_diagnosed() {
    let src = r#"
record G {
    field x: amount
    action set(v: amount) only caller { x = v }
}
"#;
    let diags = codegen_diags(src);
    let hit = diags
        .iter()
        .find(|d| d.code == W508_ONLY_CALLER_NOOP)
        .expect("`only caller` must raise W508_ONLY_CALLER_NOOP");
    assert_eq!(hit.level, covenant_diag::DiagnosticLevel::Warning);
}

#[test]
fn only_owner_guard_is_not_flagged() {
    // Negative control: a real principal guard must NOT trip the no-op warning.
    let src = r#"
record G {
    field owner: address
    field x: amount
    action init(who: address) { owner = who }
    action set(v: amount) only owner { x = v }
}
"#;
    let diags = codegen_diags(src);
    assert!(
        !diags.iter().any(|d| d.code == W508_ONLY_CALLER_NOOP),
        "`only owner` must NOT raise W508. diags: {diags:?}"
    );
}

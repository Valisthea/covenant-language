//! Pipeline orchestration: drives the full compilation pipeline from source to bytecode.
//!
//! Pipeline order:
//!
//! ```text
//! Source
//!   → Lex                (covenant-lexer)
//!   → Parse              (covenant-parser)
//!   → Resolve            (covenant-resolver)
//!   → Typecheck          (covenant-types)
//!   → Privacy analysis   (covenant-privacy)
//!   → IR construction    (covenant-ir)
//!   → Stdlib lowering    (covenant-stdlib, Phase 9)  [runs between IR and Optimizer]
//!   → Optimize           (covenant-opt)
//!   → EVM codegen        (covenant-evm-backend)
//! ```
//!
//! The `compile` function is the canonical "source → artifact" entry point.

use covenant_diag::{Diagnostic, DiagnosticLevel, SourceId};
use covenant_evm_backend::{codegen_evm, EvmArtifact, EvmConfig};
use covenant_ir::{build_ir, IrModule};
use covenant_lexer::tokenize;
use covenant_opt::{optimize, OptimizerConfig};
use covenant_parser::parse;
use covenant_privacy::analyze_privacy;
use covenant_resolver::resolve;
use covenant_stdlib::{lower_stdlib, StdlibConfig};
use covenant_types::typecheck;

/// Compile a source string into an EVM artifact.
///
/// Returns the artifact alongside any diagnostics collected along the way.
/// A non-empty error list doesn't prevent an artifact being returned — the
/// caller can inspect both.
pub fn compile(
    source: &str,
    source_id: SourceId,
    evm_config: EvmConfig,
    stdlib_config: StdlibConfig,
    opt_config: OptimizerConfig,
) -> (Option<EvmArtifact>, Vec<Diagnostic>) {
    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    let (tokens, lex_diags) = tokenize(source, source_id);
    diagnostics.extend(lex_diags);
    if has_errors(&diagnostics) {
        return (None, diagnostics);
    }

    let (file_opt, parse_diags) = parse(&tokens, source_id);
    diagnostics.extend(parse_diags);
    let Some(file) = file_opt else {
        return (None, diagnostics);
    };
    if has_errors(&diagnostics) {
        return (None, diagnostics);
    }

    let (resolved, res_diags) = resolve(file, source_id);
    diagnostics.extend(res_diags);
    if has_errors(&diagnostics) {
        return (None, diagnostics);
    }

    let (typed, ty_diags) = typecheck(resolved, source_id);
    diagnostics.extend(ty_diags);
    if has_errors(&diagnostics) {
        return (None, diagnostics);
    }

    let (analyzed, priv_diags) = analyze_privacy(typed, source_id);
    diagnostics.extend(priv_diags);
    if has_errors(&diagnostics) {
        return (None, diagnostics);
    }

    let (ir, ir_diags) = build_ir(analyzed, source_id);
    diagnostics.extend(ir_diags);
    if has_errors(&diagnostics) {
        return (None, diagnostics);
    }

    // Phase 9 — stdlib lowering — runs between IR construction and the
    // optimizer so synthesized functions benefit from the same passes.
    let (ir_with_std, std_diags) = lower_stdlib(ir, stdlib_config);
    diagnostics.extend(std_diags);

    let (optimized, opt_diags) = optimize(ir_with_std, opt_config);
    diagnostics.extend(opt_diags);

    let (artifact, evm_diags) = codegen_evm(optimized, evm_config);
    diagnostics.extend(evm_diags);

    (Some(artifact), diagnostics)
}

/// Run only the frontend pipeline (lex → parse → resolve → typecheck → privacy).
///
/// Stops before IR construction, optimization, and codegen. No artifact is
/// produced. Use this for fast validation without code generation.
pub fn check(source: &str, source_id: SourceId) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    let (tokens, lex_diags) = tokenize(source, source_id);
    diagnostics.extend(lex_diags);
    if has_errors(&diagnostics) {
        return diagnostics;
    }

    let (file_opt, parse_diags) = parse(&tokens, source_id);
    diagnostics.extend(parse_diags);
    let Some(file) = file_opt else {
        return diagnostics;
    };
    if has_errors(&diagnostics) {
        return diagnostics;
    }

    let (resolved, res_diags) = resolve(file, source_id);
    diagnostics.extend(res_diags);
    if has_errors(&diagnostics) {
        return diagnostics;
    }

    let (typed, ty_diags) = typecheck(resolved, source_id);
    diagnostics.extend(ty_diags);
    if has_errors(&diagnostics) {
        return diagnostics;
    }

    let (_analyzed, priv_diags) = analyze_privacy(typed, source_id);
    diagnostics.extend(priv_diags);

    diagnostics
}

/// Run the WHOLE pipeline (frontend → IR → stdlib → optimizer → codegen) and
/// return every diagnostic, discarding the artifact.
///
/// `check` stops at the frontend, so the editor never saw the fail-loud
/// diagnostics that live in IR lowering and codegen — a developer writing
/// `max(a, b)` (E424), `m.length` (E425), `x / 0` (E519) or a 33-byte token
/// name (E521) got a clean, green buffer that then refused to build. This is
/// what the language server should surface instead.
///
/// Uses the default (`MockChain`) target, whose runtime implements the crypto
/// opcodes natively, so the helper-only E520 does not fire in-editor — which is
/// correct: E520 is a property of a helper *deploy* target, not of the source.
pub fn check_deep(source: &str, source_id: SourceId) -> Vec<Diagnostic> {
    let (_artifact, diagnostics) = compile(
        source,
        source_id,
        EvmConfig::default(),
        StdlibConfig::default(),
        OptimizerConfig::default(),
    );
    diagnostics
}

/// Run the pipeline through IR construction (no backend, no stdlib synthesis).
///
/// Returns the IR module for linting and analysis tools. Fails if any error-level
/// diagnostic is produced before or during IR construction.
pub fn compile_to_ir(source: &str, source_id: SourceId) -> Result<IrModule, Vec<Diagnostic>> {
    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    let (tokens, lex_diags) = tokenize(source, source_id);
    diagnostics.extend(lex_diags);
    if has_errors(&diagnostics) {
        return Err(diagnostics);
    }

    let (file_opt, parse_diags) = parse(&tokens, source_id);
    diagnostics.extend(parse_diags);
    let Some(file) = file_opt else {
        return Err(diagnostics);
    };
    if has_errors(&diagnostics) {
        return Err(diagnostics);
    }

    let (resolved, res_diags) = resolve(file, source_id);
    diagnostics.extend(res_diags);
    if has_errors(&diagnostics) {
        return Err(diagnostics);
    }

    let (typed, ty_diags) = typecheck(resolved, source_id);
    diagnostics.extend(ty_diags);
    if has_errors(&diagnostics) {
        return Err(diagnostics);
    }

    let (analyzed, priv_diags) = analyze_privacy(typed, source_id);
    diagnostics.extend(priv_diags);
    if has_errors(&diagnostics) {
        return Err(diagnostics);
    }

    let (ir, ir_diags) = build_ir(analyzed, source_id);
    diagnostics.extend(ir_diags);
    if has_errors(&diagnostics) {
        return Err(diagnostics);
    }

    Ok(ir)
}

fn has_errors(diags: &[Diagnostic]) -> bool {
    diags.iter().any(|d| d.level == DiagnosticLevel::Error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_hello() {
        let src = include_str!("../../covenant-lexer/tests/fixtures/example_01_hello.cov");
        let (artifact, _diags) = compile(
            src,
            SourceId::new(0),
            EvmConfig::default(),
            StdlibConfig::default(),
            OptimizerConfig::default(),
        );
        assert!(artifact.is_some());
    }

    #[test]
    fn compiles_coin_with_erc20_synthesis() {
        let src = include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov");
        let (artifact, _diags) = compile(
            src,
            SourceId::new(0),
            EvmConfig::default(),
            StdlibConfig::default(),
            OptimizerConfig::default(),
        );
        let a = artifact.expect("artifact produced");
        // All 9 ERC-20 functions present.
        assert_eq!(a.function_selectors.len(), 9);
        assert!(a.function_selectors.contains_key("transfer"));
        assert!(a.function_selectors.contains_key("totalSupply"));
    }

    #[test]
    fn compiles_all_basics() {
        // OMEGA V6 CRT-004 fix (E518): `ballot` (example_03) uses `only
        // first_time_caller` and `board` (example_05) uses `only
        // registered_key` -- both BuiltinPredicate guards that used to
        // silently compile to an unconditional `push 1` (a complete,
        // undiagnosed authorization bypass). They now correctly refuse to
        // compile (E518) until the predicate has a real EVM lowering,
        // mirroring the existing E516/E517 "unlowered primitive" pattern
        // (see `amnesia_vdf_hardfail.rs`). hello/coin/counter don't use any
        // BuiltinPredicate and are unaffected.
        for (name, src, expect_e518) in [
            (
                "hello",
                include_str!("../../covenant-lexer/tests/fixtures/example_01_hello.cov"),
                false,
            ),
            (
                "coin",
                include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov"),
                false,
            ),
            (
                "ballot",
                include_str!("../../covenant-lexer/tests/fixtures/example_03_open_ballot.cov"),
                true,
            ),
            (
                "counter",
                include_str!("../../covenant-lexer/tests/fixtures/example_04_shielded_counter.cov"),
                false,
            ),
            (
                "board",
                include_str!("../../covenant-lexer/tests/fixtures/example_05_quantum_board.cov"),
                true,
            ),
        ] {
            let (artifact, diags) = compile(
                src,
                SourceId::new(0),
                EvmConfig::default(),
                StdlibConfig::default(),
                OptimizerConfig::default(),
            );
            let errs: Vec<_> = diags
                .iter()
                .filter(|d| d.level == DiagnosticLevel::Error)
                .collect();
            if expect_e518 {
                assert!(
                    errs.iter().any(|d| d.code.0 == 518),
                    "{name}: expected E518 (unlowered BuiltinPredicate), got {errs:?}"
                );
            } else {
                assert!(errs.is_empty(), "{name}: {errs:?}");
                assert!(artifact.is_some(), "{name}: artifact missing");
            }
        }
    }
}

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
/// A non-empty error list doesn't prevent an artifact being returned, the
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

    // Phase 9: stdlib lowering: runs between IR construction and the
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
/// diagnostics that live in IR lowering and codegen, a developer writing
/// `max(a, b)` (E424), `m.length` (E425), `x / 0` (E519) or a 33-byte token
/// name (E521) got a clean, green buffer that then refused to build. This is
/// what the language server should surface instead.
///
/// Uses the default (`MockChain`) target, whose runtime implements the crypto
/// opcodes natively, so the helper-only E520 does not fire in-editor, which is
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

/// Run the pipeline through IR construction and hand back the module even when
/// IR construction itself reported errors.
///
/// [`compile_to_ir`] drops the module as soon as any diagnostic is an error,
/// which is the right answer for codegen: never lower a module the builder
/// refused. It is the wrong answer for analysis tools. `build_ir` always
/// returns a module, so a linter built on `compile_to_ir` answers "no findings"
/// for every program the IR builder rejects, which is a fail-open: silence
/// reads as "clean" precisely when the code is at its least trustworthy.
///
/// V0.9.6 made that reachable for a whole class of ordinary programs. E430/E431
/// (append into, and read from, a collection with no storage field) are
/// error-level and raised inside `build_ir`, so any contract tripping them lost
/// its C100/C700/C1100 findings as a side effect of a diagnostic that has
/// nothing to do with them.
///
/// Frontend failures still yield `None`. A source that does not lex, parse,
/// resolve, typecheck or pass privacy analysis has no module to analyze at all,
/// and the linter's source-text scan already covers those cases.
pub fn compile_to_ir_for_analysis(
    source: &str,
    source_id: SourceId,
) -> (Option<IrModule>, Vec<Diagnostic>) {
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
    (Some(ir), diagnostics)
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
        // compile until the predicate has a real EVM lowering, mirroring the
        // existing E516/E517 "unlowered primitive" pattern (see
        // `amnesia_vdf_hardfail.rs`). hello/coin/counter don't use any
        // BuiltinPredicate and are unaffected.
        //
        // The expected codes are listed PER FIXTURE rather than as one shared
        // set, so that a fixture cannot start being refused for someone else's
        // reason and still pass. `ballot`'s E518 in particular stays pinned: it
        // is the only thing standing between that guard and the `push 1` bypass.
        //
        // `board` no longer reports E518 at all, and that is correct rather
        // than a regression. It also does `append post { .. }` into a
        // collection with no storage field, which V0.9.6 F-13 made an error
        // (E430/E431) after finding that the append reported success and wrote
        // nothing, and that reading it back SLOADed slot 0, handing out the
        // construct's first declared field for every index. Those live in
        // `build_ir`, which is upstream of codegen where E518 is raised, so the
        // pipeline stops before it ever looks at the guard. The `registered_key`
        // E518 protection the fixture used to carry now lives in
        // covenant-testing's `registered_key_predicate_is_refused`, which
        // isolates the predicate in a construct that reaches codegen.
        //
        // Empty slice = the fixture must compile clean.
        for (name, src, expect_codes) in [
            (
                "hello",
                include_str!("../../covenant-lexer/tests/fixtures/example_01_hello.cov"),
                &[][..],
            ),
            (
                "coin",
                include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov"),
                &[][..],
            ),
            (
                "ballot",
                include_str!("../../covenant-lexer/tests/fixtures/example_03_open_ballot.cov"),
                &[518][..],
            ),
            (
                "counter",
                include_str!("../../covenant-lexer/tests/fixtures/example_04_shielded_counter.cov"),
                &[][..],
            ),
            (
                "board",
                include_str!("../../covenant-lexer/tests/fixtures/example_05_quantum_board.cov"),
                &[430, 431][..],
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
            if expect_codes.is_empty() {
                assert!(errs.is_empty(), "{name}: {errs:?}");
                assert!(artifact.is_some(), "{name}: artifact missing");
            } else {
                // Every listed code must be present. A fixture that stops
                // tripping one of its refusals is a regression even if some
                // other error takes its place.
                for code in expect_codes {
                    assert!(
                        errs.iter().any(|d| d.code.0 == *code),
                        "{name}: expected E{code} among the refusals, got {errs:?}"
                    );
                }
                // No assertion on `artifact` here. `compile` documents that it
                // hands back the artifact alongside the diagnostics so the
                // caller can inspect both, and it only short-circuits to `None`
                // for failures up to and including `build_ir`. A codegen-stage
                // refusal like ballot's E518 therefore returns `Some`. Pinning
                // either shape would pin the pipeline stage the refusal happens
                // at, which is the implementation detail, not the contract.
                // What stops a refused build reaching disk is the caller:
                // `covenant build` bails on `error_count > 0` before writing,
                // and the wasm bindings set `ok = !has_errors`.
            }
        }
    }
}

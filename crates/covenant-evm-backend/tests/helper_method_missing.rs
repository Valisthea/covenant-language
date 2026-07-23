//! Fail-loud regression test: opcodes with no method on the helper contract.
//!
//! `helper_selector_for_opcode` (target.rs) maps 17 of the 31 opcodes that
//! reach `emit_precompile_call`. The other 14 used to fall back to the V0.8
//! namespaced selector `keccak("covenant.precompile.<Op>:v1")[0..4]`, which
//! matches no function on the deployed MockedFHEHelper / MockedZKVerifier /
//! MockedPQVerifier. Those helpers have no fallback function, so the CALL
//! could never dispatch: the contract compiled clean, deployed clean, and
//! bricked on first use — the primitive was neither real NOR mocked.
//!
//! The concrete case this caught: `confidential token` lowers its balance
//! check through `FheCmpGe`, which is NOT in the table. So every confidential
//! token ever compiled for Sepolia would have shipped a contract that reverts
//! on the first `transferEncrypted`. It now refuses to compile (E520).
//!
//! Native-precompile targets are deliberately unaffected — their runtime
//! implements these opcodes, so the namespaced selector is correct there.
//! That asymmetry is the point of the test: same source, two targets, one
//! compiles and one refuses.

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

const E520: u32 = 520;

/// A `confidential token`: its encrypted balance comparison lowers to
/// `FheCmpGe`, one of the opcodes with no helper method.
const SRC: &str = r#"
confidential token GapCoin {
    symbol: "GAP"
    name: "Gap Coin"
    decimals: 18
    supply: 1000 to deployer
}
"#;

fn compile_for(target: &str) -> Vec<Diagnostic> {
    let (toks, _) = tokenize(SRC, SourceId::new(0));
    let (file, _) = parse(&toks, SourceId::new(0));
    let (res, _) = resolve(file.unwrap(), SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (checked, _) = analyze_privacy(typed, SourceId::new(0));
    let (module, mut diags) = build_ir(checked, SourceId::new(0));
    let (module, std_diags) = lower_stdlib(module, StdlibConfig::default());
    diags.extend(std_diags);
    let (optimized, opt_diags) = optimize(module, OptimizerConfig::default());
    diags.extend(opt_diags);

    let config = EvmConfig::for_target(
        covenant_evm_backend::target::Target::parse(target).expect("known target"),
    );
    let (_, evm_diags) = codegen_evm(optimized, config);
    diags.extend(evm_diags);
    diags
}

fn has_e520(diags: &[Diagnostic]) -> bool {
    diags
        .iter()
        .any(|d| d.level == DiagnosticLevel::Error && d.code.0 == E520)
}

#[test]
fn helper_target_rejects_opcode_with_no_helper_method() {
    let diags = compile_for("sepolia");
    assert!(
        has_e520(&diags),
        "a helper target must refuse an opcode the helper cannot dispatch; got: {diags:?}"
    );
}

#[test]
fn native_precompile_target_still_compiles() {
    let diags = compile_for("mockchain");
    assert!(
        !has_e520(&diags),
        "mockchain implements these opcodes natively and must NOT be gated; got: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.level == DiagnosticLevel::Error),
        "mockchain build should be error-free; got: {diags:?}"
    );
}

//! OMEGA V6 CRT-007 regression test.
//!
//! Before this fix: the ERC-8231 `registry` construct's `register(bytes)` /
//! `key_of(address) returns bytes` ABI-declared the `pq_key` parameter/return
//! as dynamic `bytes`, but codegen's `is_static_abi_ty` treated it as a
//! plain 32-byte word -- so a spec-compliant caller's ABI encoding (which
//! places a dynamic-data OFFSET at that word) would be stored/returned
//! verbatim as if it were the key itself, silently corrupting the
//! registry with zero error signal anywhere in the pipeline.
//!
//! `registry` compilation now hard-fails (E505) instead.

use covenant_diag::{Diagnostic, DiagnosticLevel, SourceId};
use covenant_evm_backend::{codegen_evm, EvmArtifact, EvmConfig};
use covenant_ir::build_ir;
use covenant_lexer::tokenize;
use covenant_opt::{optimize, OptimizerConfig};
use covenant_parser::parse;
use covenant_privacy::analyze_privacy;
use covenant_resolver::resolve;
use covenant_stdlib::{lower_stdlib, StdlibConfig};
use covenant_types::typecheck;

const E505: u32 = 505;

fn pipeline(src: &str) -> (EvmArtifact, Vec<Diagnostic>) {
    let (toks, _) = tokenize(src, SourceId::new(0));
    let (file, _) = parse(&toks, SourceId::new(0));
    let (res, _) = resolve(file.unwrap(), SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (checked, _) = analyze_privacy(typed, SourceId::new(0));
    let (module, mut diags) = build_ir(checked, SourceId::new(0));
    let (module, std_diags) = lower_stdlib(module, StdlibConfig::default());
    diags.extend(std_diags);
    let (optimized, opt_diags) = optimize(module, OptimizerConfig::default());
    diags.extend(opt_diags);
    let (artifact, evm_diags) = codegen_evm(optimized, EvmConfig::default());
    diags.extend(evm_diags);
    (artifact, diags)
}

#[test]
fn plain_registry_construct_emits_e505_compile_error() {
    let (_artifact, diags) = pipeline("registry PqRegistry {\n}\n");
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Error)
        .collect();
    assert!(
        errors.iter().any(|d| d.code.0 == E505),
        "expected E505 (pq_key ABI static/dynamic mismatch), got {errors:?}"
    );
    // Both register(bytes) and key_of(...) returns bytes hit the mismatch.
    assert!(
        errors.iter().filter(|d| d.code.0 == E505).count() >= 2,
        "expected at least 2 E505 errors (register + key_of), got {errors:?}"
    );
}

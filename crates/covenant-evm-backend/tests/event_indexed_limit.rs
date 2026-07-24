//! F04 regression — non-anonymous event with >3 `indexed` params.
//!
//! Such an event compiled clean but `emit` lowered to an unconditional REVERT
//! (no LOG5 opcode exists) and the ABI advertised >3 indexed fields, which no
//! spec-compliant decoder accepts. The fix rejects it at compile time (E512).

use covenant_diag::{Diagnostic, SourceId};
use covenant_evm_backend::codes::E512_LOG_TOO_MANY_TOPICS;
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
fn event_with_four_indexed_params_is_rejected() {
    let src = r#"
record E {
    event Many(a: address indexed, b: address indexed, c: address indexed, d: address indexed)
    action go(a: address, b: address, c: address, d: address) {
        emit Many(a, b, c, d)
    }
}
"#;
    let diags = codegen_diags(src);
    let hit = diags
        .iter()
        .find(|d| d.code == E512_LOG_TOO_MANY_TOPICS)
        .expect("event with 4 indexed params must raise E512_LOG_TOO_MANY_TOPICS");
    assert_eq!(hit.level, covenant_diag::DiagnosticLevel::Error);
}

#[test]
fn event_with_three_indexed_params_is_allowed() {
    // Negative control: exactly 3 indexed is the EVM maximum and must compile.
    let src = r#"
record E {
    event Ok(a: address indexed, b: address indexed, c: address indexed, d: amount)
    action go(a: address, b: address, c: address, d: amount) {
        emit Ok(a, b, c, d)
    }
}
"#;
    let diags = codegen_diags(src);
    assert!(
        !diags.iter().any(|d| d.code == E512_LOG_TOO_MANY_TOPICS),
        "3 indexed params is legal; E512 must NOT fire. diags: {diags:?}"
    );
}

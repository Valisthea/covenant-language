//! Regression: the three-operand `transfer <amount> from <src> to <dst>` fails loud.
//!
//! The parser accepted the form and the IR builder lowered all three operands,
//! but `emit_transfer` destructured the operand list as `(operands[0],
//! operands[2])`. The `from` operand was therefore read, lowered, and then
//! silently discarded: the statement compiled clean, raised no diagnostic, and
//! emitted a plain `CALL` that paid `<dst>` out of the *contract's own* balance
//! while ignoring the source named in the source text. That is a silent
//! miscompile on a value path.
//!
//! There is no EVM primitive that spends the native balance of an account the
//! executing contract does not control, so the form has no faithful lowering and
//! is now refused at compile time (E523).

use covenant_diag::{Diagnostic, SourceId};
use covenant_evm_backend::codes::E523_TRANSFER_FROM_UNSUPPORTED;
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
fn transfer_with_from_is_rejected() {
    let src = r#"
vault Escrow {
    field balances: map<address, amount>

    action release(src: address, dst: address, value: amount)
            when balances[src] >= value {
        balances[src] -= value
        transfer(value) from src to dst
    }
}
"#;
    let diags = codegen_diags(src);
    let hit = diags
        .iter()
        .find(|d| d.code == E523_TRANSFER_FROM_UNSUPPORTED)
        .expect("`transfer ... from ... to ...` must raise E523_TRANSFER_FROM_UNSUPPORTED");
    assert_eq!(hit.level, covenant_diag::DiagnosticLevel::Error);
}

#[test]
fn plain_transfer_to_is_allowed() {
    // Negative control. Neutralise the fix in `emit_transfer` and
    // `transfer_with_from_is_rejected` fails; this test must keep passing either
    // way, proving the guard is scoped to the three-operand form and has not
    // simply banned every `transfer`.
    let src = r#"
vault Payout {
    field balances: map<address, amount>

    action withdraw(value: amount)
            when balances[caller] >= value {
        balances[caller] -= value
        transfer(value) to caller
    }
}
"#;
    let diags = codegen_diags(src);
    assert!(
        !diags
            .iter()
            .any(|d| d.code == E523_TRANSFER_FROM_UNSUPPORTED),
        "two-operand `transfer ... to ...` must NOT raise E523. diags: {diags:?}"
    );
}

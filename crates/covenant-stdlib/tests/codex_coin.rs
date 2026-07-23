//! Integration test: Example 2 Coin becomes an ERC-20 contract after Phase 9.

use covenant_diag::SourceId;
use covenant_evm_backend::{codegen_evm, EvmConfig};
use covenant_ir::{build_ir, IrModule};
use covenant_lexer::tokenize;
use covenant_opt::{optimize, OptimizerConfig};
use covenant_parser::parse;
use covenant_privacy::analyze_privacy;
use covenant_resolver::resolve;
use covenant_stdlib::{lower_stdlib, StdlibConfig, CANONICAL_SELECTORS};
use covenant_types::typecheck;

fn front_to_ir(src: &str) -> IrModule {
    let (toks, _) = tokenize(src, SourceId::new(0));
    let (file, _) = parse(&toks, SourceId::new(0));
    let (res, _) = resolve(file.unwrap(), SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (checked, _) = analyze_privacy(typed, SourceId::new(0));
    build_ir(checked, SourceId::new(0)).0
}

#[test]
fn coin_gets_9_erc20_functions() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov");
    let ir = front_to_ir(src);
    let (ir, _) = lower_stdlib(ir, StdlibConfig::default());
    let names: Vec<&str> = ir.functions.iter().map(|f| f.name.name.as_ref()).collect();
    for expected in &[
        "totalSupply",
        "balanceOf",
        "transfer",
        "transferFrom",
        "approve",
        "allowance",
        "decimals",
        "symbol",
        "name",
    ] {
        assert!(names.contains(expected), "missing {expected}");
    }
}

#[test]
fn coin_injects_required_fields() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov");
    let ir = front_to_ir(src);
    let (ir, _) = lower_stdlib(ir, StdlibConfig::default());
    let field_names: Vec<&str> = ir.fields.iter().map(|f| f.name.name.as_ref()).collect();
    assert!(field_names.contains(&"total_supply"));
    assert!(field_names.contains(&"balances"));
    assert!(field_names.contains(&"allowances"));
}

#[test]
fn coin_injects_standard_events() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov");
    let ir = front_to_ir(src);
    let (ir, _) = lower_stdlib(ir, StdlibConfig::default());
    let event_names: Vec<&str> = ir.events.iter().map(|e| e.name.name.as_ref()).collect();
    assert!(event_names.contains(&"Transfer"));
    assert!(event_names.contains(&"Approval"));
}

#[test]
fn coin_injects_standard_errors() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov");
    let ir = front_to_ir(src);
    let (ir, _) = lower_stdlib(ir, StdlibConfig::default());
    let err_names: Vec<&str> = ir.errors.iter().map(|e| e.name.name.as_ref()).collect();
    assert!(err_names.contains(&"InsufficientBalance"));
    assert!(err_names.contains(&"InsufficientAllowance"));
}

#[test]
fn coin_all_9_canonical_selectors_match() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov");
    let ir = front_to_ir(src);
    let (ir, _) = lower_stdlib(ir, StdlibConfig::default());
    let (opt, _) = optimize(ir, OptimizerConfig::default());
    let (artifact, _) = codegen_evm(opt, EvmConfig::default());
    for (name, expected) in CANONICAL_SELECTORS {
        let actual = artifact
            .function_selectors
            .get(*name)
            .unwrap_or_else(|| panic!("selector for {name} missing"));
        assert_eq!(
            actual, expected,
            "selector mismatch for `{name}`: got {actual:?}, expected {expected:?}"
        );
    }
}

#[test]
fn coin_produces_deployable_bytecode() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov");
    let ir = front_to_ir(src);
    let (ir, _) = lower_stdlib(ir, StdlibConfig::default());
    let (opt, _) = optimize(ir, OptimizerConfig::default());
    let (artifact, diags) = codegen_evm(opt, EvmConfig::default());
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.level == covenant_diag::DiagnosticLevel::Error)
        .collect();
    assert!(errors.is_empty(), "codegen errors: {errors:?}");
    assert!(!artifact.deploy_bytecode.is_empty());
    assert!(!artifact.runtime_bytecode.is_empty());
    assert!(artifact.runtime_bytecode.len() < 24 * 1024);
}

#[test]
fn coin_abi_contains_transfer_and_approval_events() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov");
    let ir = front_to_ir(src);
    let (ir, _) = lower_stdlib(ir, StdlibConfig::default());
    let (opt, _) = optimize(ir, OptimizerConfig::default());
    let (artifact, _) = codegen_evm(opt, EvmConfig::default());
    assert!(artifact.abi.contains("\"Transfer\""));
    assert!(artifact.abi.contains("\"Approval\""));
    assert!(artifact.abi.contains("\"type\":\"event\""));
}

#[test]
fn coin_abi_contains_standard_errors() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov");
    let ir = front_to_ir(src);
    let (ir, _) = lower_stdlib(ir, StdlibConfig::default());
    let (opt, _) = optimize(ir, OptimizerConfig::default());
    let (artifact, _) = codegen_evm(opt, EvmConfig::default());
    assert!(artifact.abi.contains("\"InsufficientBalance\""));
    assert!(artifact.abi.contains("\"InsufficientAllowance\""));
}

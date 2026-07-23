//! Smoke test: codegen EVM for all 5 Basics fixtures.

use covenant_diag::SourceId;
use covenant_evm_backend::{codegen_evm, EvmConfig};
use covenant_ir::build_ir;
use covenant_lexer::tokenize;
use covenant_opt::{optimize, OptimizerConfig};
use covenant_parser::parse;
use covenant_privacy::analyze_privacy;
use covenant_resolver::resolve;
use covenant_types::typecheck;

fn main() {
    for (name, src) in [
        (
            "example_01_hello",
            include_str!("../../covenant-lexer/tests/fixtures/example_01_hello.cov"),
        ),
        (
            "example_02_coin",
            include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov"),
        ),
        (
            "example_03_open_ballot",
            include_str!("../../covenant-lexer/tests/fixtures/example_03_open_ballot.cov"),
        ),
        (
            "example_04_shielded_counter",
            include_str!("../../covenant-lexer/tests/fixtures/example_04_shielded_counter.cov"),
        ),
        (
            "example_05_quantum_board",
            include_str!("../../covenant-lexer/tests/fixtures/example_05_quantum_board.cov"),
        ),
    ] {
        let (toks, _) = tokenize(src, SourceId::new(0));
        let (file, _) = parse(&toks, SourceId::new(0));
        let (res, _) = resolve(file.unwrap(), SourceId::new(0));
        let (typed, _) = typecheck(res, SourceId::new(0));
        let (checked, _) = analyze_privacy(typed, SourceId::new(0));
        let (module, _) = build_ir(checked, SourceId::new(0));
        let (optimized, _) = optimize(module, OptimizerConfig::default());
        let (artifact, diags) = codegen_evm(optimized, EvmConfig::default());
        println!(
            "{name:30}  deploy:{:5}B  runtime:{:5}B  fns:{:2}  storage:{:2}  diags:{}",
            artifact.deploy_size(),
            artifact.runtime_size(),
            artifact.function_selectors.len(),
            artifact.storage_layout.entries.len(),
            diags.len()
        );
        if !diags.is_empty() {
            for d in &diags {
                println!("  - {:?} [{:?}] {}", d.code, d.level, d.message);
            }
        }
    }
}

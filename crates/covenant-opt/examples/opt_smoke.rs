//! Smoke test: optimize each Basics fixture and show before/after counts.

use covenant_diag::SourceId;
use covenant_ir::{build_ir, validate};
use covenant_lexer::tokenize;
use covenant_opt::{optimize, total_instructions, OptimizerConfig};
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

        let before = total_instructions(&module);
        let (optimized, diags) = optimize(module.clone(), OptimizerConfig::default());
        let after = total_instructions(&optimized);
        let valid = validate(&optimized);
        println!(
            "{name:30}  before:{:3}  after:{:3}  diags:{:2}  validator:{}",
            before,
            after,
            diags.len(),
            if valid.is_empty() { "OK" } else { "FAIL" }
        );
        if !valid.is_empty() {
            for v in &valid {
                println!(
                    "    validator: {:?} @ {}..{}: {}",
                    v.code, v.span.start, v.span.end, v.message
                );
            }
        }
    }
}

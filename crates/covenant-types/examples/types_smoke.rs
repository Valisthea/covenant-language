//! Smoke test: typecheck all 5 Basics fixtures and report diagnostics + lift count.

use covenant_diag::SourceId;
use covenant_lexer::tokenize;
use covenant_parser::parse;
use covenant_resolver::resolve;
use covenant_types::typecheck;

fn main() {
    for (name, source) in [
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
        let (toks, _) = tokenize(source, SourceId::new(0));
        let (file, _) = parse(&toks, SourceId::new(0));
        let (res, _) = resolve(file.unwrap(), SourceId::new(0));
        let (typed, diags) = typecheck(res, SourceId::new(0));
        println!(
            "{name:30}  exprs: {:4}  lifts: {:3}  diags: {}",
            typed.types.expr_types.len(),
            typed.types.lifts.len(),
            diags.len()
        );
        for d in &diags {
            println!(
                "  - {:?} [{:?}] @ {}..{}: {}",
                d.code, d.level, d.span.start, d.span.end, d.message
            );
        }
    }
}

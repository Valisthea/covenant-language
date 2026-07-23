//! Smoke test: tokenize + parse each Codex Basics fixture and print diagnostics.
//! Run with `cargo run -p covenant-parser --example smoke`.

use covenant_diag::SourceId;
use covenant_lexer::tokenize;
use covenant_parser::parse;

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
        let (toks, lex_diags) = tokenize(source, SourceId::new(0));
        assert!(lex_diags.is_empty(), "{name}: lex diags: {lex_diags:?}");
        let (file, parse_diags) = parse(&toks, SourceId::new(0));
        let ok = parse_diags.is_empty() && file.is_some();
        println!(
            "{name:30}  tokens: {:4}   parsed: {}   parse_diags: {}",
            toks.len(),
            if ok { "ok " } else { "ERR" },
            parse_diags.len()
        );
        for d in &parse_diags {
            println!(
                "  - {:?} @ {}..{}: {}",
                d.code, d.span.start, d.span.end, d.message
            );
        }
    }
}

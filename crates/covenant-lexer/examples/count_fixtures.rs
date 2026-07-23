//! Utility: print token counts for the Codex Basics fixtures.
//!
//! Run with `cargo run -p covenant-lexer --example count_fixtures`.

use covenant_diag::SourceId;
use covenant_lexer::tokenize;

fn main() {
    for (name, source) in [
        (
            "example_01_hello",
            include_str!("../tests/fixtures/example_01_hello.cov"),
        ),
        (
            "example_02_coin",
            include_str!("../tests/fixtures/example_02_coin.cov"),
        ),
        (
            "example_03_open_ballot",
            include_str!("../tests/fixtures/example_03_open_ballot.cov"),
        ),
        (
            "example_04_shielded_counter",
            include_str!("../tests/fixtures/example_04_shielded_counter.cov"),
        ),
        (
            "example_05_quantum_board",
            include_str!("../tests/fixtures/example_05_quantum_board.cov"),
        ),
    ] {
        let (toks, diags) = tokenize(source, SourceId::new(0));
        println!(
            "{name:30}  tokens: {:4}   diags: {}",
            toks.len(),
            diags.len()
        );
    }
}

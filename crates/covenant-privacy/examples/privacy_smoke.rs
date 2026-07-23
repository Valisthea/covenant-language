//! Smoke test: run the privacy analyzer over all 5 Basics fixtures.

use std::collections::BTreeMap;

use covenant_diag::SourceId;
use covenant_lexer::tokenize;
use covenant_parser::parse;
use covenant_privacy::{analyze_privacy, PrivacyDomain};
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
        let (checked, diags) = analyze_privacy(typed, SourceId::new(0));
        let mut by_dom: BTreeMap<&'static str, usize> = BTreeMap::new();
        for d in checked.domains.expr_domains.values() {
            let k = match d {
                PrivacyDomain::Plaintext => "Plaintext",
                PrivacyDomain::Encrypted => "Encrypted",
                PrivacyDomain::Unknown => "Unknown",
            };
            *by_dom.entry(k).or_default() += 1;
        }
        println!("{name:30}  domains: {:?}  diags: {}", by_dom, diags.len());
        for d in &diags {
            println!(
                "  - {:?} [{:?}] @ {}..{}: {}",
                d.code, d.level, d.span.start, d.span.end, d.message
            );
        }
    }
}

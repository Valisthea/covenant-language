//! Smoke test: resolve all 5 Basics fixtures and print diagnostic counts.

use std::collections::BTreeMap;

use covenant_diag::SourceId;
use covenant_lexer::tokenize;
use covenant_parser::parse;
use covenant_resolver::{resolve, Binding};

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
        let file = file.unwrap();
        let (res, diags) = resolve(file, SourceId::new(0));
        let mut by_kind: BTreeMap<&'static str, usize> = BTreeMap::new();
        for b in res.bindings.ident_bindings.values() {
            let k = match b {
                Binding::Field(_) => "Field",
                Binding::Struct(_) => "Struct",
                Binding::Credential(_) => "Credential",
                Binding::Error(_) => "Error",
                Binding::Event(_) => "Event",
                Binding::Action(_) => "Action",
                Binding::View(_) => "View",
                Binding::Reveal(_) => "Reveal",
                Binding::SelectiveDisclosure(_) => "SelectiveDisclosure",
                Binding::Local(_) => "Local",
                Binding::LangIdent(_) => "LangIdent",
                Binding::BuiltinPredicate(_) => "BuiltinPredicate",
                Binding::StdlibModule(_) => "StdlibModule",
                Binding::StdlibFn(_) => "StdlibFn",
                Binding::ExternalContract(_) => "ExternalContract",
                Binding::Unresolved => "Unresolved",
            };
            *by_kind.entry(k).or_default() += 1;
        }
        println!(
            "{name:30}  idents: {:3}  diags: {}",
            res.bindings.ident_bindings.len(),
            diags.len()
        );
        for (k, v) in &by_kind {
            println!("    {k:<22} {v}");
        }
        for d in &diags {
            println!(
                "  - {:?} @ {}..{}: {}",
                d.code, d.span.start, d.span.end, d.message
            );
        }
    }
}

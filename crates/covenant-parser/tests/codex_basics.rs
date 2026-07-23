//! Integration tests: parse each Codex Basics fixture and assert its AST
//! structural shape.

use covenant_diag::SourceId;
use covenant_lexer::tokenize;
use covenant_parser::ast::{
    ConstructKind, MetadataValue, PrincipalKind, PrivacyQualifier, RevealTarget, TopLevelDecl, Type,
};
use covenant_parser::parse;

fn parse_or_panic(name: &str, source: &str) -> covenant_parser::ast::File {
    let (toks, lex_diags) = tokenize(source, SourceId::new(0));
    assert!(
        lex_diags.is_empty(),
        "{name}: lex diagnostics: {lex_diags:?}"
    );
    let (file, parse_diags) = parse(&toks, SourceId::new(0));
    assert!(
        parse_diags.is_empty(),
        "{name}: parse diagnostics: {parse_diags:?}"
    );
    file.expect("parser should produce an AST")
}

#[test]
fn parses_example_1_hello() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_01_hello.cov");
    let file = parse_or_panic("hello", src);
    let tl = &file.top_level;
    assert_eq!(tl.keyword, ConstructKind::Record);
    assert_eq!(tl.name.name.as_ref(), "Hello");
    assert!(tl.privacy.is_none());
    assert!(tl.anchor.is_none());
    assert!(tl.upgradeable.is_none());

    // Expect: one field (`greeting: text`), one action (`update`), one view (`read`).
    let mut fields = 0;
    let mut actions = 0;
    let mut views = 0;
    for d in &tl.body {
        match d {
            TopLevelDecl::Field(_) => fields += 1,
            TopLevelDecl::Action(_) => actions += 1,
            TopLevelDecl::View(_) => views += 1,
            _ => panic!("unexpected decl in Hello: {d:?}"),
        }
    }
    assert_eq!(fields, 1);
    assert_eq!(actions, 1);
    assert_eq!(views, 1);
}

#[test]
fn parses_example_2_coin() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov");
    let file = parse_or_panic("coin", src);
    let tl = &file.top_level;
    assert_eq!(tl.keyword, ConstructKind::Token);
    assert_eq!(tl.name.name.as_ref(), "Coin");

    let metadata: Vec<_> = tl
        .body
        .iter()
        .filter_map(|d| match d {
            TopLevelDecl::Metadata(m) => Some(m),
            _ => None,
        })
        .collect();
    // symbol, name, decimals, supply — 4 metadata entries.
    assert_eq!(metadata.len(), 4);
    let names: Vec<_> = metadata.iter().map(|m| m.name.name.as_ref()).collect();
    assert!(names.contains(&"symbol"));
    assert!(names.contains(&"name"));
    assert!(names.contains(&"decimals"));
    assert!(names.contains(&"supply"));

    // `supply` must parse as a GenesisMint targeting `deployer`.
    let supply = metadata
        .iter()
        .find(|m| m.name.name.as_ref() == "supply")
        .unwrap();
    match &supply.value {
        MetadataValue::GenesisMint { amount: _, to } => match to {
            covenant_parser::ast::Principal::Named(PrincipalKind::Deployer) => {}
            other => panic!("expected deployer, got {other:?}"),
        },
        other => panic!("expected GenesisMint, got {other:?}"),
    }
}

#[test]
fn parses_example_3_open_ballot() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_03_open_ballot.cov");
    let file = parse_or_panic("open_ballot", src);
    let tl = &file.top_level;
    assert_eq!(tl.keyword, ConstructKind::Ballot);
    assert_eq!(tl.name.name.as_ref(), "OpenBallot");

    let mut fields = 0;
    let mut actions = 0;
    let mut reveals = 0;
    for d in &tl.body {
        match d {
            TopLevelDecl::Field(_) => fields += 1,
            TopLevelDecl::Action(a) => {
                actions += 1;
                if a.name.name.as_ref() == "cast" {
                    // Action has three guards: when, only, given.
                    assert_eq!(a.guards.len(), 3);
                }
            }
            TopLevelDecl::Reveal(_) => reveals += 1,
            _ => {}
        }
    }
    // options + duration (two fields).
    assert_eq!(fields, 2);
    assert_eq!(actions, 1);
    assert_eq!(reveals, 1);
}

#[test]
fn parses_example_4_shielded_counter() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_04_shielded_counter.cov");
    let file = parse_or_panic("shielded_counter", src);
    let tl = &file.top_level;
    assert_eq!(tl.keyword, ConstructKind::Counter);
    assert_eq!(tl.name.name.as_ref(), "ShieldedCounter");
    assert_eq!(tl.privacy, Some(PrivacyQualifier::Encrypted));

    // Reveal must be the one-liner form with target Owner.
    let reveal = tl
        .body
        .iter()
        .find_map(|d| match d {
            TopLevelDecl::Reveal(r) => Some(r),
            _ => None,
        })
        .expect("reveal present");
    assert_eq!(reveal.name.name.as_ref(), "total");
    assert!(reveal.body.is_none(), "should be a one-liner");
    assert_eq!(reveal.target, Some(RevealTarget::Owner));
}

#[test]
fn parses_example_5_quantum_board() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_05_quantum_board.cov");
    let file = parse_or_panic("quantum_board", src);
    let tl = &file.top_level;
    assert_eq!(tl.keyword, ConstructKind::Board);
    assert_eq!(tl.name.name.as_ref(), "QuantumBoard");

    let mut post_struct_seen = false;
    let mut keys_field_seen = false;
    let mut register_seen = false;
    let mut submit_seen = false;
    let mut views = 0;

    for d in &tl.body {
        match d {
            TopLevelDecl::Struct(s) if s.name.name.as_ref() == "post" => {
                post_struct_seen = true;
                assert_eq!(s.fields.len(), 4);
            }
            TopLevelDecl::Field(f) if f.name.name.as_ref() == "keys" => {
                keys_field_seen = true;
                assert!(
                    matches!(&f.ty, Type::Map(_, _, _)),
                    "keys field must have map<_,_> type"
                );
            }
            TopLevelDecl::Action(a) if a.name.name.as_ref() == "register" => register_seen = true,
            TopLevelDecl::Action(a) if a.name.name.as_ref() == "submit" => {
                submit_seen = true;
                // submit should have one qualifier (pq_signed) and one guard (only).
                assert_eq!(a.qualifiers.len(), 1);
                assert_eq!(a.guards.len(), 1);
            }
            TopLevelDecl::View(_) => views += 1,
            _ => {}
        }
    }

    assert!(post_struct_seen, "post struct not found");
    assert!(keys_field_seen, "keys field not found");
    assert!(register_seen, "register action not found");
    assert!(submit_seen, "submit action not found");
    assert_eq!(views, 2);
}

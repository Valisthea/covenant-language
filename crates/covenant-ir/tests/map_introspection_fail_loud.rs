//! `map.length` / `.keys` / `.values` must refuse to compile.
//!
//! The EVM backend answered all three with `PUSH0`, so `.length` always read
//! 0 and `for each k in m.keys` always ran zero iterations, on source that
//! compiled without a single diagnostic. A silent "empty" is indistinguishable
//! from a genuinely empty map, which is the same false-negative shape as the
//! authorization bypass this pass exists to eliminate.
//!
//! There is nothing correct to emit: a Covenant map is a bare
//! `keccak(key ‖ slot)` mapping with no length word and no key array, so
//! cardinality and enumeration are simply not representable. Implementing them
//! needs an EnumerableSet-style storage convention. Until then, refuse.
//!
//! Note `map.has(k)` is deliberately NOT covered: it is unreachable from
//! source (the type checker rejects it with E207 before lowering), so the
//! `MapHas` arm in codegen is dead code rather than a live bug.

use covenant_diag::{Diagnostic, SourceId};
use covenant_ir::{build_ir, codes};
use covenant_lexer::tokenize;
use covenant_parser::parse;
use covenant_privacy::analyze_privacy;
use covenant_resolver::resolve;
use covenant_types::typecheck;

fn lower(src: &str) -> Vec<Diagnostic> {
    let (toks, lex) = tokenize(src, SourceId::new(0));
    assert!(lex.is_empty(), "lex: {lex:?}");
    let (file, pd) = parse(&toks, SourceId::new(0));
    assert!(pd.is_empty(), "parse: {pd:?}");
    let (res, _) = resolve(file.expect("file"), SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (checked, _) = analyze_privacy(typed, SourceId::new(0));
    let (_, diags) = build_ir(checked, SourceId::new(0));
    diags
}

fn assert_refused(member: &str) {
    let src = format!(
        r#"
record R {{
    votes: map<address, amount>

    view f returns amount {{
        votes.{member}
    }}
}}
"#
    );
    let diags = lower(&src);
    assert!(
        diags
            .iter()
            .any(|d| d.code == codes::E425_MAP_INTROSPECTION_UNIMPLEMENTED),
        "`map.{member}` should be refused with E425, got: {diags:?}"
    );
}

#[test]
fn map_length_is_refused() {
    assert_refused("length");
}

#[test]
fn map_len_alias_is_refused() {
    assert_refused("len");
}

#[test]
fn map_keys_is_refused() {
    assert_refused("keys");
}

#[test]
fn map_values_is_refused() {
    assert_refused("values");
}

/// A list still supports `.length`: the refusal is specific to maps, which
/// have no length word. Guards the blast radius of the fix.
#[test]
fn list_length_still_works() {
    let src = r#"
record R {
    votes: [address] = []

    view f returns amount {
        votes.length
    }
}
"#;
    let diags = lower(src);
    assert!(
        !diags
            .iter()
            .any(|d| d.code == codes::E425_MAP_INTROSPECTION_UNIMPLEMENTED),
        "list `.length` must NOT be caught by the map gate, got: {diags:?}"
    );
}

//! `map.argmax` / `map.argmin` must refuse to compile.
//!
//! They fell through the map `FieldAccess` arm to `Opcode::StructGet(0)`, so the
//! reduction never iterated: it read field 0 of the map handle and returned a
//! constant, never the key holding the maximum/minimum value. Clean-compiling
//! source, wrong key, no diagnostic — the same silent-miscompile class as E425.
//!
//! A Covenant map is a bare `keccak(key ‖ slot)` mapping with no key array to
//! iterate, so there is nothing correct to emit. List `.argmax` / `.argmin`
//! still lower (`ListArgMax` / `ListArgMin`) — the refusal is map-only.
//!
//! If someone implements an enumerable-map reduction, delete the map cases here
//! and add a semantic test. Do not "fix" this by restoring `StructGet(0)`.

use covenant_diag::{Diagnostic, SourceId};
use covenant_ir::{build_ir, codes};
use covenant_lexer::tokenize;
use covenant_parser::parse;
use covenant_privacy::analyze_privacy;
use covenant_resolver::resolve;
use covenant_types::typecheck;

fn lower(src: &str) -> (covenant_ir::IrModule, Vec<Diagnostic>) {
    let (toks, lex) = tokenize(src, SourceId::new(0));
    assert!(lex.is_empty(), "lex: {lex:?}");
    let (file, pd) = parse(&toks, SourceId::new(0));
    assert!(pd.is_empty(), "parse: {pd:?}");
    let (res, _) = resolve(file.expect("file"), SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (checked, _) = analyze_privacy(typed, SourceId::new(0));
    build_ir(checked, SourceId::new(0))
}

fn assert_map_refused(member: &str) {
    let src = format!(
        r#"
record R {{
    votes: map<address, amount>

    view f returns address {{
        votes.{member}
    }}
}}
"#
    );
    let (module, diags) = lower(&src);
    assert!(
        diags
            .iter()
            .any(|d| d.code == codes::E427_MAP_ARG_REDUCTION_UNIMPLEMENTED),
        "`map.{member}` should be refused with E427, got: {diags:?}"
    );
    // The old placeholder was `StructGet(0)`; this source has no struct field
    // read, so any StructGet is the placeholder returning silently.
    let has_struct_get = module.functions.iter().any(|f| {
        f.blocks
            .iter()
            .flat_map(|b| b.instructions.iter())
            .any(|i| matches!(i.opcode, covenant_ir::Opcode::StructGet(_)))
    });
    assert!(
        !has_struct_get,
        "`map.{member}` lowered to StructGet — the silent-miscompile placeholder is back"
    );
}

#[test]
fn map_argmax_is_refused() {
    assert_map_refused("argmax");
}

#[test]
fn map_argmin_is_refused() {
    assert_map_refused("argmin");
}

/// A list still supports `.argmax` / `.argmin` — the refusal is specific to
/// maps, which have no key array. Guards the blast radius of the fix.
#[test]
fn list_argmax_still_works() {
    let src = r#"
record R {
    nums: [amount] = []

    view f returns amount {
        nums.argmax
    }
}
"#;
    let (module, diags) = lower(src);
    assert!(
        !diags
            .iter()
            .any(|d| d.code == codes::E427_MAP_ARG_REDUCTION_UNIMPLEMENTED),
        "list `.argmax` must NOT be caught by the map gate, got: {diags:?}"
    );
    let has_list_argmax = module.functions.iter().any(|f| {
        f.blocks
            .iter()
            .flat_map(|b| b.instructions.iter())
            .any(|i| matches!(i.opcode, covenant_ir::Opcode::ListArgMax))
    });
    assert!(
        has_list_argmax,
        "list `.argmax` should still lower to ListArgMax"
    );
}

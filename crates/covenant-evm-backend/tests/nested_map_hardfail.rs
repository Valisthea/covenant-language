//! F09 regression — nested map (`map(_, map(...))`) fails loud.
//!
//! A nested-map write `ab[a][b] = v` was never lowered: the write emitted ZERO
//! SSTORE (the statement was silently dropped) and the read hashed against the
//! outer entry's always-zero stored value, returning 0. A well-typed program
//! therefore compiled to bytecode that silently discarded every write. The fix
//! rejects nested map fields at compile time (E522).

use covenant_diag::{Diagnostic, SourceId};
use covenant_evm_backend::codes::E522_NESTED_MAP_UNSUPPORTED;
use covenant_evm_backend::{codegen_evm, EvmConfig};
use covenant_ir::build_ir;
use covenant_lexer::tokenize;
use covenant_opt::{optimize, OptimizerConfig};
use covenant_parser::parse;
use covenant_privacy::analyze_privacy;
use covenant_resolver::resolve;
use covenant_stdlib::{lower_stdlib, StdlibConfig};
use covenant_types::typecheck;

fn codegen_diags(src: &str) -> Vec<Diagnostic> {
    let (toks, _) = tokenize(src, SourceId::new(0));
    let (file, _) = parse(&toks, SourceId::new(0));
    let (res, _) = resolve(file.unwrap(), SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (checked, _) = analyze_privacy(typed, SourceId::new(0));
    let (module, _) = build_ir(checked, SourceId::new(0));
    let (with_std, _) = lower_stdlib(module, StdlibConfig::default());
    let (optimized, _) = optimize(with_std, OptimizerConfig::default());
    let (_artifact, diags) = codegen_evm(optimized, EvmConfig::default());
    diags
}

#[test]
fn nested_map_field_is_rejected() {
    // Note the space before the final `>`: `map<..., map<...> >`. Without it the
    // lexer reads `>>` as a right-shift and the type fails to parse — nested
    // maps are only even *expressible* with the space.
    let src = r#"
record Nested {
    field ab: map<address, map<address, amount> >
    action setab(a: address, b: address, v: amount) { ab[a][b] = v }
    view getab(a: address, b: address) returns amount { ab[a][b] }
}
"#;
    let diags = codegen_diags(src);
    let hit = diags
        .iter()
        .find(|d| d.code == E522_NESTED_MAP_UNSUPPORTED)
        .expect("nested map field must raise E522_NESTED_MAP_UNSUPPORTED");
    assert_eq!(hit.level, covenant_diag::DiagnosticLevel::Error);
}

#[test]
fn single_level_map_is_allowed() {
    // Negative control: an ordinary single-level map must still compile clean.
    let src = r#"
record Single {
    field bal: map<address, amount>
    action set(a: address, v: amount) { bal[a] = v }
    view get(a: address) returns amount { bal[a] }
}
"#;
    let diags = codegen_diags(src);
    assert!(
        !diags.iter().any(|d| d.code == E522_NESTED_MAP_UNSUPPORTED),
        "single-level map must NOT raise E522. diags: {diags:?}"
    );
}

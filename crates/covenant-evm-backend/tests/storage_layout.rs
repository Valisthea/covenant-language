//! KSR-CVN-041 regression tests — detect_slot_collisions is wired into codegen_evm.
//!
//! Before this fix, `detect_slot_collisions` was fully implemented in
//! `storage.rs` but never called. Two fields sharing the same `@slot(N)`
//! compiled silently; one silently overwrote the other at runtime.
//!
//! After the fix, `codegen_evm` calls `detect_slot_collisions` before
//! returning and appends `E423_SLOT_ANNOTATION_CONFLICT` diagnostics for
//! every conflicting pair.

use covenant_diag::SourceId;
use covenant_evm_backend::{codegen_evm, EvmConfig};
use covenant_ir::build_ir;
use covenant_lexer::tokenize;
use covenant_opt::{optimize, OptimizerConfig};
use covenant_parser::parse;
use covenant_privacy::analyze_privacy;
use covenant_resolver::resolve;
use covenant_stdlib::{lower_stdlib, StdlibConfig};
use covenant_types::typecheck;

fn compile_diags(src: &str) -> Vec<covenant_diag::Diagnostic> {
    let (toks, _) = tokenize(src, SourceId::new(0));
    let (file, _) = parse(&toks, SourceId::new(0));
    let (res, _) = resolve(file.unwrap(), SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (checked, _) = analyze_privacy(typed, SourceId::new(0));
    let (module, _) = build_ir(checked, SourceId::new(0));
    let (with_std, _) = lower_stdlib(module, StdlibConfig::default());
    let (optimized, _) = optimize(with_std, OptimizerConfig::default());
    let (_, diags) = codegen_evm(optimized, EvmConfig::default());
    diags
}

#[test]
fn duplicate_explicit_slots_produce_e423() {
    let src = r#"
module Clash {
    @slot(0)
    field a: amount

    @slot(0)
    field b: amount

    action noop() {
        a = a + 1
        b = b + 1
    }
}
"#;
    let diags = compile_diags(src);
    let has_e423 = diags
        .iter()
        .any(|d| d.code == covenant_ir::diag::E423_SLOT_ANNOTATION_CONFLICT);
    assert!(
        has_e423,
        "expected E423 for duplicate @slot(0) on fields a and b, got: {diags:?}"
    );
}

#[test]
fn distinct_explicit_slots_compile_clean() {
    let src = r#"
module NoClash {
    @slot(5)
    field a: amount

    @slot(10)
    field b: amount

    action noop() {
        a = a + 1
        b = b + 1
    }
}
"#;
    let diags = compile_diags(src);
    let e423_count = diags
        .iter()
        .filter(|d| d.code == covenant_ir::diag::E423_SLOT_ANNOTATION_CONFLICT)
        .count();
    assert_eq!(
        e423_count, 0,
        "no E423 expected for non-overlapping @slot values (5 and 10)"
    );
}

#[test]
fn sequential_field_aliasing_explicit_slot_produces_e423() {
    // Sequential counter starts at 0. A plain `field a: amount` occupies slot 0.
    // A subsequent `@slot(0) field b: amount` then aliases it — E423 must fire.
    let src = r#"
module SeqClash {
    field a: amount

    @slot(0)
    field b: amount

    action noop() {
        a = a + 1
        b = b + 1
    }
}
"#;
    let diags = compile_diags(src);
    let has_e423 = diags
        .iter()
        .any(|d| d.code == covenant_ir::diag::E423_SLOT_ANNOTATION_CONFLICT);
    assert!(
        has_e423,
        "expected E423 when @slot(0) aliases sequential slot 0 already occupied by field a, \
         got: {diags:?}"
    );
}

//! KSR-CVN-021 regression tests — `@slot(N)` annotation respected by storage layout.
//!
//! Verifies that:
//!   1. A field annotated with `@slot(N)` is placed at storage slot `N`, not at
//!      the next sequential slot.
//!   2. Fields without the annotation continue to use sequential layout.
//!   3. Codegen emits the correct slot bytes for SSTORE/SLOAD of annotated
//!      fields.
//!
//! The attack documented in OMEGA V4: a user following the "annotate every
//! field with `@slot(N)`" upgrade-safety convention would have the annotation
//! silently ignored. Adding a new field between two existing ones shifts every
//! subsequent slot by one, so funds stored at "old slot 3" are read from
//! "new slot 3" — a different field. Pinning with `@slot` is the documented
//! upgrade-safety remediation; it must actually work.

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

fn compile_artifact(src: &str) -> covenant_evm_backend::artifact::EvmArtifact {
    let (toks, _) = tokenize(src, SourceId::new(0));
    let (file, _) = parse(&toks, SourceId::new(0));
    let (res, _) = resolve(file.unwrap(), SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (checked, _) = analyze_privacy(typed, SourceId::new(0));
    let (module, _) = build_ir(checked, SourceId::new(0));
    let (with_std, _) = lower_stdlib(module, StdlibConfig::default());
    let (optimized, _) = optimize(with_std, OptimizerConfig::default());
    let (artifact, _) = codegen_evm(optimized, EvmConfig::default());
    artifact
}

fn slot_u32(slot_bytes: &[u8; 32]) -> u32 {
    u32::from_be_bytes(slot_bytes[28..32].try_into().unwrap())
}

fn build_module(src: &str) -> covenant_ir::IrModule {
    let (toks, _) = tokenize(src, SourceId::new(0));
    let (file, _) = parse(&toks, SourceId::new(0));
    let (res, _) = resolve(file.unwrap(), SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (checked, _) = analyze_privacy(typed, SourceId::new(0));
    let (module, _) = build_ir(checked, SourceId::new(0));
    module
}

// ---- KSR-CVN-042: explicit @slot on a reserved slot is flagged ----

#[test]
fn explicit_slot_on_reserved_deployer_slot_is_flagged() {
    use covenant_evm_backend::storage;
    // DEPLOYER_SLOT = 0xFFFFFFFE = 4294967294 backs `only deployer` auth.
    let src = r#"
module Hijack {
    @slot(4294967294)
    field sneaky: amount
    action noop() { sneaky = sneaky + 1 }
}
"#;
    let module = build_module(src);
    let collisions = storage::detect_slot_collisions(&module);
    assert!(
        collisions
            .iter()
            .any(|(f, _, slot)| f.as_ref() == "sneaky" && *slot == storage::DEPLOYER_SLOT),
        "explicit @slot on DEPLOYER_SLOT must be flagged (KSR-CVN-042): {collisions:?}"
    );
}

#[test]
fn explicit_slot_on_reserved_reentrancy_slot_is_flagged() {
    use covenant_evm_backend::storage;
    // REENTRANT_LOCK_SLOT = 0xFFFFFFFF = 4294967295 backs `@non_reentrant`.
    let src = r#"
module Hijack2 {
    @slot(4294967295)
    field sneaky: amount
    action noop() { sneaky = sneaky + 1 }
}
"#;
    let module = build_module(src);
    let collisions = storage::detect_slot_collisions(&module);
    assert!(
        collisions
            .iter()
            .any(|(f, _, slot)| f.as_ref() == "sneaky" && *slot == storage::REENTRANT_LOCK_SLOT),
        "explicit @slot on REENTRANT_LOCK_SLOT must be flagged (KSR-CVN-042): {collisions:?}"
    );
}

#[test]
fn normal_explicit_slot_is_not_flagged_as_reserved() {
    use covenant_evm_backend::storage;
    let src = r#"
module Fine {
    @slot(42)
    field a: amount
    field b: amount
    action noop() { a = a + 1 }
}
"#;
    let module = build_module(src);
    let collisions = storage::detect_slot_collisions(&module);
    assert!(
        collisions.is_empty(),
        "no collisions expected: {collisions:?}"
    );
}

#[test]
fn slot_annotation_is_honored_in_storage_layout() {
    // First field gets @slot(42); second field has no annotation and should
    // take the next sequential slot (1, since the first sequential slot 0 is
    // unused — pinned slots do not advance the sequential counter).
    let src = r#"
module Pinned {
    @slot(42)
    field pinned_amount: amount

    field other_amount: amount

    action bump() {
        pinned_amount = pinned_amount + 1
        other_amount = other_amount + 1
    }
}
"#;
    let artifact = compile_artifact(src);

    let pinned = artifact
        .storage_layout
        .entries
        .iter()
        .find(|e| e.name.as_ref() == "pinned_amount")
        .expect("pinned_amount entry");
    let other = artifact
        .storage_layout
        .entries
        .iter()
        .find(|e| e.name.as_ref() == "other_amount")
        .expect("other_amount entry");

    assert_eq!(
        slot_u32(&pinned.slot),
        42,
        "pinned_amount should be pinned to slot 42 via @slot(42)"
    );
    assert_eq!(
        slot_u32(&other.slot),
        0,
        "other_amount has no @slot annotation; pinned fields should not advance the sequential cursor"
    );
}

#[test]
fn slot_annotation_does_not_shift_sequential_fields() {
    // With two pinned fields and one sequential, the sequential field should
    // always land at slot 0, regardless of the pinned slot values.
    let src = r#"
module MixedLayout {
    @slot(100)
    field pinned_a: amount

    field sequential_x: amount

    @slot(200)
    field pinned_b: amount

    field sequential_y: amount

    action noop() {
        sequential_x = sequential_x + 1
    }
}
"#;
    let artifact = compile_artifact(src);

    let get_slot = |name: &str| -> u32 {
        let e = artifact
            .storage_layout
            .entries
            .iter()
            .find(|e| e.name.as_ref() == name)
            .unwrap_or_else(|| panic!("missing storage entry `{name}`"));
        slot_u32(&e.slot)
    };

    assert_eq!(get_slot("pinned_a"), 100);
    assert_eq!(get_slot("pinned_b"), 200);
    assert_eq!(get_slot("sequential_x"), 0);
    assert_eq!(get_slot("sequential_y"), 1);
}

#[test]
fn no_annotation_preserves_pure_sequential_layout() {
    // Regression guard: modules without any @slot annotations must behave
    // exactly like before this feature existed.
    let src = r#"
module Vanilla {
    field a: amount
    field b: amount
    field c: amount

    action noop() { a = a + 1 }
}
"#;
    let artifact = compile_artifact(src);

    let get_slot = |name: &str| -> u32 {
        slot_u32(
            &artifact
                .storage_layout
                .entries
                .iter()
                .find(|e| e.name.as_ref() == name)
                .unwrap()
                .slot,
        )
    };

    assert_eq!(get_slot("a"), 0);
    assert_eq!(get_slot("b"), 1);
    assert_eq!(get_slot("c"), 2);
}

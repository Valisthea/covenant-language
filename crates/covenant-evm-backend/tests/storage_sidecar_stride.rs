//! Regression coverage for the storage sidecar's description of a
//! `list<Struct>` field.
//!
//! `rows[i]` lives at `keccak256(slot) + i * stride`, where the stride is the
//! element struct's field count, so growing `struct Row { a b }` to
//! `struct Row { a b c d }` relocates every element after index 0. That is a
//! breaking storage change by any definition.
//!
//! `render_ty` collapsed the element type to the literal string `_`, because
//! `IrField::ty` carries no resolution for nominal element types (a
//! `field rows: [Row]` arrives as `List(Unknown)`; struct names are bound to
//! `StructId`s only while typing expressions). Both versions therefore
//! rendered as `[_]`, the two sidecars were byte-identical, and
//! `covenant layout diff` (which compares name, slot and type string) printed
//! "no breaking storage-layout changes" and exited 0 for exactly the upgrade
//! that orphans every element. The only delta between the two runtimes was
//! `PUSH1 2 MUL` becoming `PUSH1 4 MUL`.
//!
//! The sidecar had no representation of the stride at all: even the
//! non-catch-all `Ty::Struct(_) => "struct"` branch carried no field count.

mod common;

use common::*;

const V1: &str = r#"
record P22 {
    struct Row { a: amount  b: amount }
    field rows: [Row]
    field tail: amount = 1
    action set_a(i: amount, v: amount) { rows[i].a = v }
    view get_a(i: amount) returns amount { rows[i].a }
}
"#;

const V2: &str = r#"
record P22 {
    struct Row { a: amount  b: amount  c: amount  d: amount }
    field rows: [Row]
    field tail: amount = 1
    action set_a(i: amount, v: amount) { rows[i].a = v }
    view get_a(i: amount) returns amount { rows[i].a }
}
"#;

fn ty_desc(src: &str, field: &str) -> String {
    let (artifact, _) = compile(src);
    artifact
        .storage_layout
        .entries
        .iter()
        .find(|e| e.name.as_ref() == field)
        .unwrap_or_else(|| panic!("no `{field}` in the layout"))
        .ty_desc
        .to_string()
}

#[test]
fn a_struct_element_size_change_changes_the_rendered_type() {
    let a = ty_desc(V1, "rows");
    let b = ty_desc(V2, "rows");
    assert_ne!(
        a, b,
        "a stride change must be visible in the sidecar; both rendered as `{a}`"
    );
}

#[test]
fn the_rendered_type_names_the_stride_the_bytecode_multiplies_by() {
    assert!(
        ty_desc(V1, "rows").contains("stride=2"),
        "a two-field element struct has stride 2, got `{}`",
        ty_desc(V1, "rows")
    );
    assert!(
        ty_desc(V2, "rows").contains("stride=4"),
        "a four-field element struct has stride 4, got `{}`",
        ty_desc(V2, "rows")
    );
}

#[test]
fn the_element_struct_is_named_not_collapsed_to_underscore() {
    let d = ty_desc(V1, "rows");
    assert!(
        d.contains("Row"),
        "the element type must be resolved, not rendered as `_`, got `{d}`"
    );
}

#[test]
fn scalar_lists_and_plain_fields_are_unchanged_in_kind() {
    // Control: a scalar element list has stride 1 and must still say so, and
    // an ordinary field's description is untouched.
    let src = r#"
record Plain {
    field xs: [amount]
    field n: amount = 1
    field who: address
    field m: map<address, amount>
    action bump(i: amount) { n = xs[i] }
}
"#;
    assert_eq!(ty_desc(src, "n"), "uint256");
    assert_eq!(ty_desc(src, "who"), "address");
    assert_eq!(ty_desc(src, "m"), "map<address, uint256>");
    assert!(
        ty_desc(src, "xs").contains("stride=1"),
        "{}",
        ty_desc(src, "xs")
    );
}

#[test]
fn slot_assignment_is_not_moved_by_the_resolution() {
    // Resolving the element type is a description fix. It must not shift a
    // single slot, or it would silently break every deployed contract.
    for src in [V1, V2] {
        let (artifact, _) = compile(src);
        let slots: Vec<(String, u32)> = artifact
            .storage_layout
            .entries
            .iter()
            .map(|e| {
                let mut n = [0u8; 4];
                n.copy_from_slice(&e.slot[28..32]);
                (e.name.to_string(), u32::from_be_bytes(n))
            })
            .collect();
        assert_eq!(
            slots,
            vec![("rows".to_string(), 0), ("tail".to_string(), 1)]
        );
    }
}

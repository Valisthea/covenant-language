//! Fail-loud refusals for three constructs whose lowering does not exist.
//!
//! E530, over-long hex literal. `push_n` computed the PUSH opcode as
//! `0x60 + (len - 1)` behind a `debug_assert!` that the shipped release binary
//! compiles out. A 33-byte literal emitted `0x80` (DUP1) followed by the
//! literal's own bytes laid down as instructions, and because the dispatcher
//! leaves the selector word on the stack the bogus DUP1 succeeded and
//! execution fell into the constant. A 256-byte literal truncated the length
//! byte to zero, emitting one PUSH0 while the size accounting still charged
//! 257 bytes, desynchronising every label after it. Representing a wider
//! constant needs multi-word constant support, so this refuses rather than
//! emits whatever `0x60 + (len - 1)` lands on.
//!
//! E531, bare struct-typed field. Neither direction is lowered: the write
//! produced no instruction at all and the read dereferenced the field's stored
//! word as a storage address, returning the next declared field's slot.
//!
//! E532, dynamic `indexed` event parameter. The ABI says the topic is
//! `keccak256(value)` and nothing hashes it, so every emit wrote a zero topic
//! and two logs with different values were indistinguishable.

mod common;

use common::*;
use covenant_diag::DiagnosticLevel;
use covenant_evm_backend::codes::{
    E530_HEX_CONSTANT_TOO_LONG, E531_BARE_STRUCT_FIELD, E532_DYNAMIC_INDEXED_EVENT_PARAM,
    W530_DYNAMIC_EVENT_DATA_NOT_ENCODED,
};

#[test]
fn a_hex_literal_wider_than_one_push_is_rejected() {
    // 33 bytes. The bytes spell CALLER ; PUSH4 0xfffffffe ; SSTORE ; PUSH0 ;
    // PUSH0 ; RETURN, the payload the finding used to rewrite the deployer
    // slot from an action whose source text only emits a log.
    let src = r#"
record OwnerProbe {
    field owner: address

    event Blob(tag: bytes, n: amount)

    action tag_it(n: amount) {
        emit Blob(0x3363fffffffe555f5ff30000000000000000000000000000000000000000000000, n)
    }
}
"#;
    let ds = diags(src);
    assert!(
        has_error(&ds, E530_HEX_CONSTANT_TOO_LONG),
        "a 33-byte hex literal must raise E530, got {ds:?}"
    );
}

#[test]
fn a_very_long_hex_literal_is_rejected_too() {
    // 64 bytes. Past 255 the length byte wrapped to zero, which is the label
    // desync case; well short of that the opcode arithmetic is already wrong.
    let long = "aa".repeat(64);
    let src = format!(
        r#"
record Big {{
    event Blob(tag: bytes)
    action fire() {{ emit Blob(0x{long}) }}
}}
"#
    );
    let ds = diags(&src);
    assert!(has_error(&ds, E530_HEX_CONSTANT_TOO_LONG), "{ds:?}");
}

#[test]
fn a_hex_literal_of_exactly_32_bytes_still_compiles() {
    // The control: 32 bytes is the widest a single PUSH carries, and it has
    // always worked.
    let src = r#"
record Fits {
    event Blob(tag: bytes)
    action fire() {
        emit Blob(0x1122334455667788990011223344556677889900112233445566778899001122)
    }
}
"#;
    let ds = diags(src);
    assert!(
        !has_error(&ds, E530_HEX_CONSTANT_TOO_LONG),
        "a 32-byte literal must NOT raise E530, got {ds:?}"
    );
    let errs: Vec<_> = ds
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Error)
        .collect();
    assert!(errs.is_empty(), "expected a clean compile, got {errs:?}");
}

#[test]
fn a_bare_struct_typed_field_is_rejected() {
    let src = r#"
record P14 {
    struct Cfg {
        w: amount
        x: amount
    }

    field cfg: Cfg
    field sentinel: amount = 555

    action set_w(v: amount) { cfg.w = v }
    view get_w returns amount { cfg.w }
    view get_x returns amount { cfg.x }
    view s returns amount { sentinel }
}
"#;
    let ds = diags(src);
    assert!(
        has_error(&ds, E531_BARE_STRUCT_FIELD),
        "a bare struct-typed field must raise E531, got {ds:?}"
    );
}

#[test]
fn a_struct_held_in_a_list_is_still_accepted() {
    // The control, and the remediation the diagnostic points at: a struct
    // reached through a list IS lowered, so E531 must not fire on it.
    let src = r#"
record Held {
    struct Cfg {
        w: amount
        x: amount
    }

    field cfgs: [Cfg]

    action add(w: amount, x: amount) { append cfgs { w: w, x: x } }
    action set_w(i: amount, v: amount) { cfgs[i].w = v }
    view get_w(i: amount) returns amount { cfgs[i].w }
}
"#;
    let ds = diags(src);
    assert!(
        !has_error(&ds, E531_BARE_STRUCT_FIELD),
        "a `[Struct]` field must NOT raise E531, got {ds:?}"
    );
}

#[test]
fn a_dynamic_indexed_event_parameter_is_rejected() {
    let src = r#"
record ConstTag {
    event Tagged(tag: text indexed, n: amount)

    action fire_alpha(n: amount) { emit Tagged("alpha", n) }
    action fire_beta(n: amount)  { emit Tagged("beta", n) }
}
"#;
    let ds = diags(src);
    assert!(
        has_error(&ds, E532_DYNAMIC_INDEXED_EVENT_PARAM),
        "`text indexed` must raise E532, got {ds:?}"
    );
}

#[test]
fn a_dynamic_non_indexed_event_parameter_only_warns() {
    // The same encoding gap in the log data, but putting text in an event is
    // an ordinary pattern, so it warns rather than blocking compilation. Same
    // call W507 makes for a dynamic return value.
    let src = r#"
record Marker {
    event Marked(label: text, qty: amount)
    action mark(label: text, qty: amount) { emit Marked(label, qty) }
}
"#;
    let ds = diags(src);
    assert!(
        has_warning(&ds, W530_DYNAMIC_EVENT_DATA_NOT_ENCODED),
        "expected W530, got {ds:?}"
    );
    assert!(
        !has_error(&ds, E532_DYNAMIC_INDEXED_EVENT_PARAM),
        "a non-indexed dynamic param is not E532"
    );
    let errs: Vec<_> = ds
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Error)
        .collect();
    assert!(errs.is_empty(), "must still compile, got {errs:?}");
}

#[test]
fn an_indexed_static_event_parameter_is_untouched() {
    // The control: `address indexed` is the ERC-20 shape and encodes fine.
    let src = r#"
record Xfer {
    event Sent(who: address indexed, amt: amount)
    action fire(who: address, amt: amount) { emit Sent(who, amt) }
}
"#;
    let ds = diags(src);
    assert!(
        !has_error(&ds, E532_DYNAMIC_INDEXED_EVENT_PARAM)
            && !has_warning(&ds, W530_DYNAMIC_EVENT_DATA_NOT_ENCODED),
        "a static indexed param must raise neither, got {ds:?}"
    );
}

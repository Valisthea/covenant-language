//! Regression coverage for `list<T>` element addressing.
//!
//! Two distinct defects lived in the same three lines of `Opcode::ListGet`
//! plus `emit_list_elem_addr`, and both handed an unprivileged caller an
//! arbitrary storage write.
//!
//! 1. Whether to dereference the computed element address was decided by
//!    `stride == 1`, meant as "the element is a scalar". A struct with exactly
//!    one field also has stride 1, so `xs[i].c` compiled to
//!    `SLOAD(SLOAD(elem))` and `xs[i].c = v` to `SSTORE(<element's stored
//!    value>, v)`. The caller chose the element value at `append` time and the
//!    written word at `edit` time, so an ordinary row-table setter was an
//!    arbitrary storage read and an arbitrary storage write. A two-field
//!    struct was unaffected, which is why the compiler's own `list<Struct>`
//!    fixture never caught it.
//!
//! 2. The address is `keccak256(slot) + index * stride` with a plain `ADD`,
//!    and nothing compared `index` against the list length. `ADD` wraps mod
//!    2^256, so a caller-chosen index selected any slot in the contract,
//!    including the compiler-reserved `DEPLOYER_SLOT` that backs every
//!    `only deployer` guard.

mod common;

use common::*;
use covenant_evm_backend::storage::DEPLOYER_SLOT;
use covenant_evm_runtime::U256;

/// A one-field struct in a list, beside an `only deployer` guarded field.
const ONE_FIELD: &str = r#"
record P9d {
    struct Item {
        w: amount
    }

    items: [Item] = []
    admin: address = 0x0000000000000000000000000000000000000000

    action add(w: amount) { append items { w: w } }
    action edit(i: amount, v: amount) { items[i].w = v }

    action set_admin(a: address) only deployer { admin = a }

    view get_admin returns address { admin }
    view peek(i: amount) returns amount { items[i].w }
    view n returns amount { items.length }
}
"#;

/// The same shape with a two-field struct: the case that always worked, kept
/// as a control so a fix cannot pass by breaking both.
const TWO_FIELD: &str = r#"
record P9e {
    struct Item {
        w: amount
        z: amount
    }

    items: [Item] = []
    admin: address = 0x0000000000000000000000000000000000000000

    action add(w: amount, z: amount) { append items { w: w, z: z } }
    action edit(i: amount, v: amount) { items[i].w = v }

    view get_admin returns address { admin }
    view peek(i: amount) returns amount { items[i].w }
    view peek_z(i: amount) returns amount { items[i].z }
}
"#;

#[test]
fn one_field_struct_element_round_trips() {
    let mut d = deploy(ONE_FIELD);
    let alice = d.alice;
    d.send_ok(alice, "add(uint256)", &[u(11)]);
    d.send_ok(alice, "add(uint256)", &[u(22)]);

    // Read half. The pre-fix double dereference returned 0 here, because the
    // element's stored value (11) was itself used as a storage address.
    assert_eq!(d.view_u256("peek(uint256)", &[u(0)]), u(11));
    assert_eq!(d.view_u256("peek(uint256)", &[u(1)]), u(22));

    // Write half.
    d.send_ok(alice, "edit(uint256,uint256)", &[u(0), u(99)]);
    assert_eq!(d.view_u256("peek(uint256)", &[u(0)]), u(99));
    assert_eq!(d.view_u256("peek(uint256)", &[u(1)]), u(22));
}

#[test]
fn one_field_struct_write_cannot_reach_a_guarded_field() {
    // The executed attack: a non-deployer appends an element whose value is
    // the slot number of `admin`, then "edits" element 0. Pre-fix that wrote
    // `SSTORE(1, attacker)` and the attacker owned `admin`, an address only
    // `set_admin ... only deployer` may write.
    let mut d = deploy(ONE_FIELD);
    let alice = d.alice;
    let admin_slot = 1u64; // items at slot 0, admin at slot 1

    d.send_ok(alice, "add(uint256)", &[u(admin_slot)]);
    d.send_ok(alice, "edit(uint256,uint256)", &[u(0), u(0xdead)]);

    assert_eq!(
        d.view_u256("get_admin()", &[]),
        U256::ZERO,
        "`edit` writes items[0].w, which cannot be the `admin` field"
    );
    assert_eq!(
        d.storage(u(admin_slot)),
        U256::ZERO,
        "slot 1 must be untouched by a write to a list element"
    );
    // And the write landed where the source says it should.
    assert_eq!(d.view_u256("peek(uint256)", &[u(0)]), u(0xdead));
}

#[test]
fn two_field_struct_control_still_works() {
    let mut d = deploy(TWO_FIELD);
    let alice = d.alice;
    d.send_ok(alice, "add(uint256,uint256)", &[u(55), u(77)]);
    assert_eq!(d.view_u256("peek(uint256)", &[u(0)]), u(55));
    assert_eq!(d.view_u256("peek_z(uint256)", &[u(0)]), u(77));

    d.send_ok(alice, "edit(uint256,uint256)", &[u(0), u(1)]);
    assert_eq!(d.view_u256("peek(uint256)", &[u(0)]), u(1));
    assert_eq!(
        d.view_u256("peek_z(uint256)", &[u(0)]),
        u(77),
        "editing field 0 must not disturb field 1"
    );
    assert_eq!(d.view_u256("get_admin()", &[]), U256::ZERO);
}

const ROWS: &str = r#"
record P10 {
    struct Row {
        a: amount
        b: amount
    }

    field rows: [Row]
    field owner_fee: amount = 1

    action add(a: amount, b: amount) { append rows { a: a, b: b } }
    action set_a(i: amount, v: amount) { rows[i].a = v }
    action set_b(i: amount, v: amount) { rows[i].b = v }

    action set_owner_fee(v: amount) only deployer { owner_fee = v }

    view fee returns amount { owner_fee }
    view get_a(i: amount) returns amount { rows[i].a }
}
"#;

#[test]
fn write_past_the_end_of_a_list_reverts() {
    let mut d = deploy(ROWS);
    let alice = d.alice;
    // Empty list: every index is out of bounds.
    d.send_reverts(alice, "set_b(uint256,uint256)", &[u(0), u(42)]);

    d.send_ok(alice, "add(uint256,uint256)", &[u(1), u(2)]);
    // Exactly one element, so index 0 is the only legal one.
    d.send_ok(alice, "set_b(uint256,uint256)", &[u(0), u(42)]);
    d.send_reverts(alice, "set_b(uint256,uint256)", &[u(1), u(42)]);
    d.send_reverts(alice, "set_b(uint256,uint256)", &[u(9999), u(42)]);
}

#[test]
fn read_past_the_end_of_a_list_reverts() {
    let mut d = deploy(ROWS);
    let alice = d.alice;
    let addr = d.addr;
    let data = raw_call("get_a(uint256)", &[word(0)]);
    let r = d.chain.call(alice, addr, &data, U256::ZERO);
    assert!(
        !matches!(r.status, covenant_evm_runtime::TxStatus::Success),
        "reading element 0 of an empty list must revert, not return slot garbage"
    );

    d.send_ok(alice, "add(uint256,uint256)", &[u(7), u(8)]);
    assert_eq!(d.view_u256("get_a(uint256)", &[u(0)]), u(7));
}

#[test]
fn a_crafted_index_cannot_reach_the_deployer_auth_slot() {
    // The executed escalation. `rows` sits at slot 0 with stride 2, so element
    // data begins at keccak256(0). Solve `base + i*2 + 1 == DEPLOYER_SLOT` mod
    // 2^256 for the `set_b` (field index 1) form, exactly as the finding did,
    // and confirm the write no longer lands.
    let mut d = deploy(ROWS);
    let alice = d.alice;

    let base = keccak_of_slot_zero();
    let target = U256::from_u64(DEPLOYER_SLOT as u64);
    // delta = target - base; set_b writes base + i*2 + 1, so i = (delta-1)/2
    // and the parity must be odd for the `b` field to be the one that lands.
    let delta = target.wrapping_sub(&base);
    let is_odd = delta.to_be_bytes()[31] & 1 == 1;
    assert!(
        is_odd,
        "fixture assumption: the `b` field is the odd offset"
    );
    let index = delta.wrapping_sub(&U256::ONE).div(&U256::from_u64(2));

    let deployer_before = d.storage(U256::from_u64(DEPLOYER_SLOT as u64));
    assert_ne!(
        deployer_before,
        U256::ZERO,
        "the constructor must have captured a deployer for this test to mean anything"
    );

    d.send_reverts(alice, "set_b(uint256,uint256)", &[index, u(0xbeef)]);
    assert_eq!(
        d.storage(U256::from_u64(DEPLOYER_SLOT as u64)),
        deployer_before,
        "the deployer-auth slot must be unreachable through a list index"
    );

    // The guard it protects still works in both directions.
    d.send_reverts(alice, "set_owner_fee(uint256)", &[u(9999)]);
    let deployer = d.deployer;
    d.send_ok(deployer, "set_owner_fee(uint256)", &[u(9999)]);
    assert_eq!(d.view_u256("fee()", &[]), u(9999));
}

/// `keccak256(bytes32(0))`, the start of the element data for a list at slot 0.
fn keccak_of_slot_zero() -> U256 {
    use sha3::{Digest, Keccak256};
    let mut h = Keccak256::new();
    h.update([0u8; 32]);
    let out: [u8; 32] = h.finalize().into();
    U256::from_be_bytes(out)
}

#[test]
fn in_bounds_iteration_and_append_are_unaffected() {
    // `append` writes at index == length, which is out of bounds by
    // definition, so the bounds check must not be on that path. `for each`
    // must still walk every element.
    let src = r#"
record Tally {
    struct Vote {
        weight: amount
        bonus: amount
    }

    votes: [Vote] = []
    total: amount = 0

    action cast(w: amount) { append votes { weight: w, bonus: 0 } }
    action tally() {
        total = 0
        for each v in votes {
            total = total + v.weight
        }
    }
    view get_total returns amount { total }
    view count returns amount { votes.length }
}
"#;
    let mut d = deploy(src);
    let alice = d.alice;
    d.send_ok(alice, "cast(uint256)", &[u(10)]);
    d.send_ok(alice, "cast(uint256)", &[u(20)]);
    d.send_ok(alice, "cast(uint256)", &[u(30)]);
    assert_eq!(d.view_u256("count()", &[]), u(3));
    d.send_ok(alice, "tally()", &[]);
    assert_eq!(d.view_u256("get_total()", &[]), u(60));
}

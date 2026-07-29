//! Constant field defaults must actually be written at deploy.
//!
//! `field a: amount = 42` emitted `PUSH1 0x2a` into the deploy bytecode but the
//! value was never SSTOREd, so the field read back as 0. Found 2026-07-23 while
//! building the Robinhood milestone token: `fee_bps = 100` deployed as 0,
//! and confirmed on anvil and on Robinhood Chain testnet. `covenant check`
//! passed and `build` succeeded; only the on-chain read exposed it. The
//! genesis-mint path (`supply: N to deployer`) was separate and worked, which
//! is exactly why the one initializer everyone tests was the one that worked.
//!
//! The IR builder now carries a constant default into `IrField::initializer_const`
//! (`field_default_const`), and the constructor SSTOREs it. Only the literal
//! types the backend stores in one word are carried, integers, bools, and
//! 20/32-byte hex: so these tests pin exactly those.

use covenant_testing::{CovenantTestHarness, U256};

#[test]
fn non_zero_integer_default_is_written() {
    let src = r#"
record Defaults {
    a: amount = 42
    view get_a returns amount { a }
}
"#;
    let mut h = CovenantTestHarness::new();
    let c = h.deploy(src, h.addrs.deployer).expect("deploy");
    // Was 0 before the fix.
    assert_eq!(
        h.view_u256(c, h.addrs.alice, "get_a()", &[]),
        U256::from_u64(42)
    );
}

#[test]
fn bool_true_default_is_written() {
    let src = r#"
record Flag {
    on: bool = true
    view is_on returns bool { on }
}
"#;
    let mut h = CovenantTestHarness::new();
    let c = h.deploy(src, h.addrs.deployer).expect("deploy");
    // `bool` reads back as a 0/1 word.
    assert_eq!(
        h.view_u256(c, h.addrs.alice, "is_on()", &[]),
        U256::from_u64(1)
    );
}

#[test]
fn zero_default_still_reads_zero() {
    // The boundary: an explicit `= 0` must remain 0 (no accidental double-write
    // or off-by-one from the new path).
    let src = r#"
record Zero {
    a: amount = 0
    view get_a returns amount { a }
}
"#;
    let mut h = CovenantTestHarness::new();
    let c = h.deploy(src, h.addrs.deployer).expect("deploy");
    assert_eq!(h.view_u256(c, h.addrs.alice, "get_a()", &[]), U256::ZERO);
}

#[test]
fn multiple_defaults_land_in_their_own_slots() {
    // Guards against a slot mix-up now that more than one field carries a
    // default.
    let src = r#"
record Multi {
    a: amount = 7
    b: amount = 9
    view get_a returns amount { a }
    view get_b returns amount { b }
}
"#;
    let mut h = CovenantTestHarness::new();
    let c = h.deploy(src, h.addrs.deployer).expect("deploy");
    assert_eq!(
        h.view_u256(c, h.addrs.alice, "get_a()", &[]),
        U256::from_u64(7)
    );
    assert_eq!(
        h.view_u256(c, h.addrs.alice, "get_b()", &[]),
        U256::from_u64(9)
    );
}

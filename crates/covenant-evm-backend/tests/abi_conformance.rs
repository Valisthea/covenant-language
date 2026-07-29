//! Regression coverage for what the runtime accepts through its own published
//! ABI.
//!
//! The parameter prelude was `PUSH off ; CALLDATALOAD ; PUSH slot ; MSTORE`
//! and nothing else, which broke three ways.
//!
//! No length check. `CALLDATALOAD` zero-pads past the end of calldata, so a
//! caller who sent only the 4-byte selector drove any action down its all-zero
//! argument path with status 1: an owner-setter truncated to its selector
//! wrote the zero address.
//!
//! No `bool` canonicalisation. `&&` and `||` are aliased onto the bitwise EVM
//! AND and OR, so the word `0x02` is truthy for `!`, for a bare `when b` and
//! for `||`, but `0x02 & 0x01 == 0` makes it false for `&&`. A guard written
//! `when !(a && b)` therefore FAILED OPEN for a caller who hand-encoded
//! `0x02`, while the honest `true, true` correctly reverted.
//!
//! No `address` canonicalisation. A recipient word with dirty high bits keys
//! `keccak(dirtyWord || slot)`, a slot no conformant caller can address again,
//! while the `Transfer` log emitted in the same transaction says the value was
//! delivered.
//!
//! Separately, `emit_abi` hardcoded `nonpayable` for every action while the
//! runtime emitted no CALLVALUE guard at all, so the published interface was a
//! false statement about the bytecode produced in the same invocation, in both
//! directions at once.

mod common;

use common::*;
use covenant_evm_runtime::{TxStatus, U256};

const BOOLZ: &str = r#"
record Boolz {
  field hit: amount
  field flag: bool

  view andb(a: bool, b: bool) returns bool { a && b }
  view orb(a: bool, b: bool) returns bool { a || b }
  view notb(a: bool) returns bool { !a }
  view notand(a: bool, b: bool) returns bool { !(a && b) }

  action store(b: bool) { flag = b }
  action gate(a: bool, b: bool) when !(a && b) { hit = 1 }

  view got returns amount { hit }
  view get_flag returns bool { flag }
}
"#;

#[test]
fn a_non_canonical_bool_argument_never_reaches_the_body() {
    let mut d = deploy(BOOLZ);
    let dep = d.deployer;

    // `store` carries no guard and no arithmetic, so the only thing that can
    // reject this call is the prelude's canonicality check. Pre-fix the raw
    // word was stored verbatim into a field the ABI declares `bool`, leaving
    // a value that is simultaneously not-equal-to-true and
    // not-equal-to-false, which a `when b` guard then treats as true.
    let data = raw_call("store(bool)", &[word(2)]);
    let r = d.send_raw(dep, &data);
    assert!(
        !matches!(r.status, TxStatus::Success),
        "a `bool` argument that is neither 0 nor 1 is not a valid encoding of \
         the type the ABI declares"
    );
    assert_eq!(d.view_u256("get_flag()", &[]), U256::ZERO);

    // And the guard shape the finding used, for the record.
    d.send_reverts(dep, "gate(bool,bool)", &[u(1), u(1)]);
    let data = raw_call("gate(bool,bool)", &[word(2), word(1)]);
    let r = d.send_raw(dep, &data);
    assert!(!matches!(r.status, TxStatus::Success));
    assert_eq!(d.view_u256("got()", &[]), U256::ZERO, "no state written");
}

#[test]
fn a_non_canonical_bool_cannot_be_returned_through_a_bool_view() {
    let mut d = deploy(BOOLZ);
    let dep = d.deployer;
    // `orb(0x02, false)` returned the raw word 0x02 in a slot the ABI
    // declares as `bool`.
    let data = raw_call("orb(bool,bool)", &[word(2), word(0)]);
    let r = d.send_raw(dep, &data);
    assert!(!matches!(r.status, TxStatus::Success));
}

#[test]
fn canonical_bools_still_work() {
    // The control. Every conformant encoder emits 0 or 1, so the ordinary
    // truth table must be untouched.
    let mut d = deploy(BOOLZ);
    let dep = d.deployer;
    assert_eq!(d.view_u256("andb(bool,bool)", &[u(1), u(1)]), U256::ONE);
    assert_eq!(d.view_u256("andb(bool,bool)", &[u(1), u(0)]), U256::ZERO);
    assert_eq!(d.view_u256("orb(bool,bool)", &[u(1), u(0)]), U256::ONE);
    assert_eq!(d.view_u256("orb(bool,bool)", &[u(0), u(0)]), U256::ZERO);
    assert_eq!(d.view_u256("notb(bool)", &[u(0)]), U256::ONE);
    assert_eq!(d.view_u256("notand(bool,bool)", &[u(1), u(1)]), U256::ZERO);
    d.send_ok(dep, "store(bool)", &[u(1)]);
    assert_eq!(d.view_u256("get_flag()", &[]), U256::ONE);
    d.send_ok(dep, "gate(bool,bool)", &[u(1), u(0)]);
    assert_eq!(d.view_u256("got()", &[]), U256::ONE);
}

const ADDRS: &str = r#"
record Book {
    field balances: map<address, amount>
    field last: address

    action credit(dest: address, v: amount) {
        balances[dest] = balances[dest] + v
        last = dest
    }
    view bal(of_addr: address) returns amount { balances[of_addr] }
    view get_last returns address { last }
}
"#;

#[test]
fn an_address_argument_with_dirty_high_bits_is_rejected() {
    let mut d = deploy(ADDRS);
    let dep = d.deployer;

    // 96 dirty high bits over a real 20-byte address. Pre-fix this credited
    // keccak(dirtyWord || slot), a permanently unreachable slot, and stored
    // the dirty word as `last` under an ABI that declares `address`.
    let mut dirty = [0u8; 32];
    dirty[..12].copy_from_slice(&[
        0xde, 0xad, 0xbe, 0xef, 0xde, 0xad, 0xbe, 0xef, 0xde, 0xad, 0xbe, 0xef,
    ]);
    dirty[12..].copy_from_slice(&[0x3c; 20]);
    let data = raw_call("credit(address,uint256)", &[dirty, word(1000)]);
    let r = d.send_raw(dep, &data);
    assert!(
        !matches!(r.status, TxStatus::Success),
        "an `address` word whose top 96 bits are set is not a valid encoding"
    );
    assert_eq!(d.view_u256("get_last()", &[]), U256::ZERO);
}

#[test]
fn clean_addresses_still_work() {
    let mut d = deploy(ADDRS);
    let dep = d.deployer;
    let mut clean = [0u8; 32];
    clean[12..].copy_from_slice(&[0x3c; 20]);
    let data = raw_call("credit(address,uint256)", &[clean, word(1000)]);
    let r = d.send_raw(dep, &data);
    assert!(matches!(r.status, TxStatus::Success));
    let who = U256::from_be_bytes(clean);
    assert_eq!(d.view_u256("bal(address)", &[who]), u(1000));
    assert_eq!(d.view_u256("get_last()", &[]), who);
}

const SET_ALL: &str = r#"
record AbiProbe {
    field n: amount
    field a: address
    field b: bool
    field h: hash

    action set_all(p_amount: amount, p_addr: address, p_bool: bool, p_hash: hash) {
        n = p_amount
        a = p_addr
        b = p_bool
        h = p_hash
    }

    view get_n returns amount { n }
    view get_a returns address { a }
}
"#;

#[test]
fn a_call_that_omits_its_arguments_reverts() {
    let mut d = deploy(SET_ALL);
    let dep = d.deployer;

    let mut addr = [0u8; 32];
    addr[12..].copy_from_slice(&[0x70; 20]);
    let full = raw_call(
        "set_all(uint256,address,bool,bytes32)",
        &[word(12345), addr, word(1), word(0x1122)],
    );
    let r = d.send_raw(dep, &full);
    assert!(matches!(r.status, TxStatus::Success));
    assert_eq!(d.view_u256("get_n()", &[]), u(12345));

    // Selector only. Pre-fix this succeeded and zeroed all four fields: an
    // owner-setter truncated to its selector wrote the zero address.
    let sel = raw_call("set_all(uint256,address,bool,bytes32)", &[]);
    let r = d.send_raw(dep, &sel);
    assert!(
        !matches!(r.status, TxStatus::Success),
        "a call carrying fewer argument bytes than the ABI declares must revert"
    );
    assert_eq!(
        d.view_u256("get_n()", &[]),
        u(12345),
        "the truncated call must not have written anything"
    );

    // Partially truncated: two words where four are declared.
    let partial = raw_call("set_all(uint256,address,bool,bytes32)", &[word(7), addr]);
    let r = d.send_raw(dep, &partial);
    assert!(!matches!(r.status, TxStatus::Success));
    assert_eq!(d.view_u256("get_n()", &[]), u(12345));
}

#[test]
fn trailing_junk_is_still_ignored_and_zero_arg_calls_still_work() {
    // Controls, both matching Solidity: extra calldata past the declared
    // arguments is ignored, and a function with no parameters is reachable
    // with a bare selector.
    let mut d = deploy(SET_ALL);
    let dep = d.deployer;
    let mut data = raw_call("get_n()", &[]);
    let r = d.send_raw(dep, &data);
    assert!(matches!(r.status, TxStatus::Success));

    data.extend_from_slice(&[0xab; 40]);
    let r = d.send_raw(dep, &data);
    assert!(
        matches!(r.status, TxStatus::Success),
        "trailing junk must not break a well-formed call"
    );
}

const VALUE_PROBE: &str = r#"
record ValueProbe {
    field last_value: amount
    field n: amount

    action deposit() { last_value = msg.value }
    action no_value(x: amount) { n = x }

    view get_last_value returns amount { last_value }
    view get_n returns amount { n }
}
"#;

#[test]
fn an_action_that_reads_msg_value_is_published_payable() {
    let (artifact, _) = compile(VALUE_PROBE);
    assert!(
        artifact
            .abi
            .contains(r#""name":"deposit","type":"function","inputs":[],"outputs":[],"stateMutability":"payable""#),
        "an action that reads msg.value must be published payable, got {}",
        artifact.abi
    );
    assert!(
        artifact.abi.contains(r#""name":"no_value""#)
            && artifact.abi.contains(r#""stateMutability":"nonpayable""#),
        "an action that never mentions value stays nonpayable, got {}",
        artifact.abi
    );
}

#[test]
fn a_nonpayable_action_rejects_attached_value() {
    let mut d = deploy(VALUE_PROBE);
    let dep = d.deployer;
    // Pre-fix this succeeded and left 5 wei in a contract whose source never
    // mentions value, while the ABI told wallets the function was unpayable.
    let r = d.send_value(dep, "no_value(uint256)", &[u(42)], u(5));
    assert!(
        !matches!(r.status, TxStatus::Success),
        "a function published `nonpayable` must reject Ether"
    );
    assert_eq!(d.view_u256("get_n()", &[]), U256::ZERO);

    // And it still works with no value attached.
    d.send_ok(dep, "no_value(uint256)", &[u(42)]);
    assert_eq!(d.view_u256("get_n()", &[]), u(42));
}

#[test]
fn a_payable_action_accepts_attached_value() {
    let mut d = deploy(VALUE_PROBE);
    let dep = d.deployer;
    let r = d.send_value(dep, "deposit()", &[], u(3));
    assert!(
        matches!(r.status, TxStatus::Success),
        "an action that reads msg.value must remain reachable WITH value"
    );
    assert_eq!(d.view_u256("get_last_value()", &[]), u(3));
}

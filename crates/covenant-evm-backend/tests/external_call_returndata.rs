//! Regression coverage for the external-call return-value guard.
//!
//! `when IGate.at(gate).allowed(caller)` is the canonical way to express an
//! off-board allowlist, role registry or KYC oracle, and the only way to
//! express `only <address held in a field>` at all. The call ran with
//! `retOffset = 0x00`, the same scratch buffer the outgoing calldata had just
//! been built in, and then branched on `RETURNDATASIZE != 0` alone.
//!
//! A gate returning 1 to 31 bytes overwrote only that many bytes of
//! `mem[0x00..0x20]`; the untouched tail still held the outgoing calldata word
//! (the selector shifted left by 224 bits), so `MLOAD(0x00)` read a non-zero
//! word and the guard PASSED on a gate that had answered false. A gate
//! returning more than 32 bytes failed the same way from the other side: the
//! head of any ABI dynamic type is the offset 0x20, likewise non-zero. Either
//! way the decision was made on attacker-influenced stale memory, and it
//! failed toward more authority.
//!
//! The precompile path in the same file already enforced a width check. This
//! is the same check, sized to the declared return type: exactly one word for
//! a static return, at least one for a dynamic one.
//!
//! The assertions here are on the emitted code rather than on execution
//! because `covenant-evm-runtime` treats a non-precompile CALL/STATICCALL as
//! an inert no-op with `RETURNDATASIZE == 0` (see its `OP_STATICCALL` arm), so
//! a gate with chosen behaviour cannot be run against it. The finding itself
//! was executed on anvil. What IS executable here, and is covered below, is
//! that a call whose result nobody decodes still works.

mod common;

use common::*;
use covenant_evm_runtime::U256;

const GUARD: &str = r#"
external contract IGate {
    function peek(address) view returns bool
}

record P29ExtGuard {
    gate: address
    n: amount

    action set_gate(g: address) { gate = g }
    action set_n_view(v: amount) when IGate.at(gate).peek(caller) { n = v }

    view get_n returns amount { n }
}
"#;

const VALUE_READ: &str = r#"
external contract IToken {
    function balanceOf(address) view returns amount
}

record Reader {
    view bal(tok: address, who: address) returns amount {
        IToken.at(tok).balanceOf(who)
    }
}
"#;

/// `RETURNDATASIZE ; PUSH1 32 ; EQ ; ISZERO ; PUSH2 <label> ; JUMPI`
/// i.e. revert unless the callee returned exactly one word.
const EXACT_WORD_CHECK: [u8; 6] = [0x3d, 0x60, 0x20, 0x14, 0x15, 0x61];
/// The shape this replaces: `RETURNDATASIZE ; PUSH2 <label> ; JUMPI`, which
/// accepts any non-zero width.
const NONZERO_ONLY_CHECK: [u8; 2] = [0x3d, 0x61];

fn count(hay: &[u8], needle: &[u8]) -> usize {
    hay.windows(needle.len()).filter(|w| *w == needle).count()
}

fn runtime(src: &str) -> Vec<u8> {
    compile(src).0.runtime_bytecode
}

#[test]
fn a_consumed_external_return_is_width_checked() {
    let code = runtime(GUARD);
    assert!(
        count(&code, &EXACT_WORD_CHECK) >= 1,
        "the guard's external call must reject a return that is not exactly \
         one word; RETURNDATASIZE ; PUSH1 32 ; EQ ; ISZERO ; JUMPI not found"
    );
}

#[test]
fn the_bare_nonzero_returndata_branch_is_gone() {
    // The exact pre-fix sequence, which let a 1-to-31-byte return through.
    // `PUSH2` is only ever a label push in this backend, so this pattern is
    // specific to the branch that was removed.
    for src in [GUARD, VALUE_READ] {
        let code = runtime(src);
        assert_eq!(
            count(&code, &NONZERO_ONLY_CHECK),
            0,
            "RETURNDATASIZE followed straight by a label push accepts any \
             non-empty return, including one that leaves the outgoing calldata \
             visible in mem[0x00]"
        );
    }
}

#[test]
fn a_value_read_is_width_checked_too() {
    // Not just guards: `IToken.at(t).balanceOf(who)` reads one word out of
    // mem[0x00] and must not accept a short or oversized return either.
    let code = runtime(VALUE_READ);
    assert!(count(&code, &EXACT_WORD_CHECK) >= 1);
}

#[test]
fn a_record_with_no_external_call_emits_no_such_check() {
    // Guards against the pattern above matching something unrelated, for
    // example the precompile path's own width check.
    let src = r#"
record Plain {
    field n: amount
    action set(v: amount) { n = v }
    view get returns amount { n }
}
"#;
    let code = runtime(src);
    assert_eq!(count(&code, &EXACT_WORD_CHECK), 0);
    assert_eq!(count(&code, &NONZERO_ONLY_CHECK), 0);
}

#[test]
fn a_call_whose_result_is_discarded_tolerates_an_empty_return() {
    // `IFoo.at(a).notify(x)` written as a statement decodes nothing, so the
    // width of a return nobody reads must not be checked. Solidity does not
    // check it either, and a callee with no code returns zero bytes. The IR
    // builder allocates an SSA result for every external call regardless of
    // whether the interface declares one, so "has a result" is not the same
    // question as "the result is used".
    let src = r#"
external contract IERC20 {
    function transfer(address, amount)
}
record Wallet {
    field sent: amount
    action send(tok: address, dest: address, val: amount) {
        IERC20.at(tok).transfer(dest, val)
        sent = sent + val
    }
    view get_sent returns amount { sent }
}
"#;
    let code = runtime(src);
    assert_eq!(
        count(&code, &EXACT_WORD_CHECK),
        0,
        "a discarded return must not be width-checked"
    );

    let mut d = deploy(src);
    let dep = d.deployer;
    let mut tok = [0u8; 32];
    tok[31] = 0x55;
    let mut dest = [0u8; 32];
    dest[31] = 0x66;
    let data = raw_call(
        "send(address,address,uint256)",
        &[tok, dest, U256::from_u64(100).to_be_bytes()],
    );
    let r = d.send_raw(dep, &data);
    assert!(
        matches!(r.status, covenant_evm_runtime::TxStatus::Success),
        "a value-less external call must not be gated on returndata width"
    );
    assert_eq!(d.view_u256("get_sent()", &[]), u(100));
}

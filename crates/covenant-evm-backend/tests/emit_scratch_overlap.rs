//! Regression coverage for `emit` overwriting the function's own SSA slots.
//!
//! Event data words were MSTOREd at `0x00 + 32*i` while SSA values live at
//! `SSA_MEMORY_BASE + 32*v` = 0x80 upward, so data word 4 landed exactly on
//! SSA value 0, word 5 on value 1, and so on. Everything read after the emit
//! saw the log payload instead of the parameter: the indexed topic operands
//! (loaded after the data words), later data operands, and the whole rest of
//! the function body. Four non-indexed parameters stayed under 0x80, which is
//! why the threshold looked arbitrary at exactly five.

mod common;

use common::*;
use covenant_evm_runtime::U256;

const PAY: &str = r#"
record PayProbe {
    field beneficiary: address
    field balances: map<address, amount>

    event Detail(a: amount, b: amount, c: amount, d: amount, e: amount)

    action pay(dest: address, a: amount, b: amount, c: amount, d: amount, e: amount) {
        emit Detail(a, b, c, d, e)
        beneficiary = dest
        balances[dest] = balances[dest] + a
    }

    action pay_no_emit(dest: address, a: amount, b: amount, c: amount, d: amount, e: amount) {
        beneficiary = dest
        balances[dest] = balances[dest] + a
    }

    view who returns address { beneficiary }
    view bal(of_addr: address) returns amount { balances[of_addr] }
}
"#;

const SIG: &str = "pay(address,uint256,uint256,uint256,uint256,uint256)";
const SIG_NO_EMIT: &str = "pay_no_emit(address,uint256,uint256,uint256,uint256,uint256)";

/// A clean 20-byte address word.
fn addr_word(low: u64) -> U256 {
    U256::from_u64(low)
}

#[test]
fn state_written_after_a_wide_emit_uses_the_real_parameters() {
    let mut d = deploy(PAY);
    let dep = d.deployer;
    let victim = addr_word(0x7099);
    let attacker = addr_word(0x3c44);

    // Control first: the identical call shape without the emit.
    d.send_ok(
        dep,
        SIG_NO_EMIT,
        &[victim, u(100), u(2), u(3), u(4), attacker],
    );
    assert_eq!(d.view_u256("who()", &[]), victim);
    assert_eq!(d.view_u256("bal(address)", &[victim]), u(100));

    // With the emit. Pre-fix `beneficiary` took the value of the LAST event
    // field, so the caller redirected the credit by choosing `e`.
    d.send_ok(dep, SIG, &[victim, u(500), u(2), u(3), u(4), attacker]);
    assert_eq!(
        d.view_u256("who()", &[]),
        victim,
        "`beneficiary = dest` must store `dest`, not an event data word"
    );
    assert_eq!(
        d.view_u256("bal(address)", &[victim]),
        u(600),
        "the credit must go to `dest`"
    );
    assert_eq!(
        d.view_u256("bal(address)", &[attacker]),
        U256::ZERO,
        "no balance may reach the address the caller smuggled in through `e`"
    );
}

#[test]
fn a_wide_emit_carries_the_values_the_source_named() {
    let mut d = deploy(PAY);
    let dep = d.deployer;
    let r = d.send_ok(
        dep,
        SIG,
        &[addr_word(0x7099), u(11), u(22), u(33), u(44), u(55)],
    );
    assert_eq!(r.logs.len(), 1);
    let data = hex::decode(r.logs[0].data.trim_start_matches("0x")).expect("hex");
    assert_eq!(data.len(), 5 * 32, "five non-indexed words");
    let words: Vec<u64> = data
        .chunks(32)
        .map(|c| {
            let mut w = [0u8; 32];
            w.copy_from_slice(c);
            U256::from_be_bytes(w).low_u64()
        })
        .collect();
    assert_eq!(words, vec![11, 22, 33, 44, 55]);
}

#[test]
fn an_indexed_topic_is_not_replaced_by_a_data_word() {
    // The indexed operands are loaded AFTER the data words are stored, so with
    // five or more data words the topic read a clobbered SSA slot: topic1 came
    // out as the last data field instead of `who`.
    let src = r#"
record Wide {
    event Big(who: address indexed, a: amount, b: amount, c: amount, d: amount, e: amount)

    action fire(who: address, a: amount, b: amount, c: amount, d: amount, e: amount) {
        emit Big(who, a, b, c, d, e)
    }
}
"#;
    let mut d = deploy(src);
    let dep = d.deployer;
    let who = addr_word(0x7099);
    let r = d.send_ok(
        dep,
        "fire(address,uint256,uint256,uint256,uint256,uint256)",
        &[who, u(11), u(22), u(33), u(44), u(55)],
    );
    assert_eq!(r.logs.len(), 1);
    assert_eq!(r.logs[0].topics.len(), 2, "topic0 + one indexed param");
    let topic1 = decode_word(&r.logs[0].topics[1]);
    assert_eq!(topic1, who, "topic1 must be `who`, not the last data word");
}

#[test]
fn four_word_events_still_work() {
    // Control: four non-indexed params never overlapped, so this shape must be
    // unchanged by the fix.
    let src = r#"
record Narrow {
    field marker: address
    event Four(a: amount, b: amount, c: amount, d: amount)
    action fire(m: address, a: amount, b: amount, c: amount, d: amount) {
        emit Four(a, b, c, d)
        marker = m
    }
    view get_marker returns address { marker }
}
"#;
    let mut d = deploy(src);
    let dep = d.deployer;
    let m = addr_word(0x7099);
    let r = d.send_ok(
        dep,
        "fire(address,uint256,uint256,uint256,uint256)",
        &[m, u(1), u(2), u(3), u(4)],
    );
    assert_eq!(r.logs.len(), 1);
    assert_eq!(d.view_u256("get_marker()", &[]), m);
}

#[test]
fn an_event_with_no_data_still_logs() {
    let src = r#"
record Ping {
    field n: amount
    event Pinged(who: address indexed)
    action ping() {
        emit Pinged(caller)
        n = n + 1
    }
    view get_n returns amount { n }
}
"#;
    let mut d = deploy(src);
    let dep = d.deployer;
    let r = d.send_ok(dep, "ping()", &[]);
    assert_eq!(r.logs.len(), 1);
    assert_eq!(r.logs[0].data, "0x", "no non-indexed params, no data");
    assert_eq!(d.view_u256("get_n()", &[]), u(1));
}

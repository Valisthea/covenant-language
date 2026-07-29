//! Regression coverage for four arithmetic lowerings that produced the wrong
//! number on chain with no diagnostic anywhere.
//!
//! * Shifts were routed through the generic `binop` helper, which puts the
//!   LEFT operand on top of the stack because that is what SUB/DIV/LT/GT want.
//!   `SHL`/`SHR` invert the convention: they pop the shift count first. So
//!   `a << b` computed `b << a`, and the permission-bitmap idiom `1 << role`
//!   produced `role << 1`.
//! * `SignedNeg` pushed the zero before the operand, so SUB computed `x - 0`:
//!   unary minus was the identity function.
//! * `DurationScale` was grouped with the two Add opcodes and emitted as ADD,
//!   so `base * 7` stored `base + 7`.
//! * `time`/`duration` add and subtract were raw ADD/SUB while the identical
//!   operators on `amount` have been checked since KSR-CVN-031, so a deadline
//!   that had passed produced a "time remaining" of 2^256-n instead of
//!   reverting.

mod common;

use common::*;
use covenant_evm_runtime::U256;

const SHIFTS: &str = r#"
record Shifts {
    view shl(a: amount, b: amount) returns amount { a << b }
    view shr(a: amount, b: amount) returns amount { a >> b }
    view band(a: amount, b: amount) returns amount { a & b }
    view sub_order(a: amount, b: amount) returns amount { a - b }
}
"#;

#[test]
fn shift_left_shifts_the_left_operand() {
    let mut d = deploy(SHIFTS);
    // The reversed lowering gave 16, 16 and 510 for these three.
    assert_eq!(d.view_u256("shl(uint256,uint256)", &[u(1), u(8)]), u(256));
    assert_eq!(d.view_u256("shl(uint256,uint256)", &[u(3), u(2)]), u(12));
    assert_eq!(
        d.view_u256("shl(uint256,uint256)", &[u(1), u(255)]),
        U256::ONE.shl(255)
    );
    // Shifting by zero is the identity, and shifting a zero yields zero.
    assert_eq!(d.view_u256("shl(uint256,uint256)", &[u(7), u(0)]), u(7));
    assert_eq!(d.view_u256("shl(uint256,uint256)", &[u(0), u(9)]), u(0));
}

#[test]
fn shift_right_shifts_the_left_operand() {
    let mut d = deploy(SHIFTS);
    // The reversed lowering annihilated to 0 here rather than merely swapping.
    assert_eq!(d.view_u256("shr(uint256,uint256)", &[u(256), u(4)]), u(16));
    assert_eq!(d.view_u256("shr(uint256,uint256)", &[u(1024), u(10)]), u(1));
    assert_eq!(d.view_u256("shr(uint256,uint256)", &[u(5), u(0)]), u(5));
}

#[test]
fn the_permission_bitmap_idiom_agrees_with_the_source() {
    // `1 << role` is the canonical way to build a role mask, and it was the
    // shape that made this Critical: the contract failed open for an attacker
    // and closed for the rightful admin at the same time.
    let src = r#"
record Roles {
    field granted: amount

    action grant(role: amount) { granted = granted | (1 << role) }
    view has_role(role: amount) returns bool { (granted & (1 << role)) != 0 }
    view mask(role: amount) returns amount { 1 << role }
}
"#;
    let mut d = deploy(src);
    let alice = d.alice;
    assert_eq!(d.view_u256("mask(uint256)", &[u(0)]), u(1));
    assert_eq!(d.view_u256("mask(uint256)", &[u(3)]), u(8));
    assert_eq!(d.view_u256("mask(uint256)", &[u(7)]), u(128));

    d.send_ok(alice, "grant(uint256)", &[u(3)]);
    assert_eq!(d.view_u256("has_role(uint256)", &[u(3)]), U256::ONE);
    assert_eq!(d.view_u256("has_role(uint256)", &[u(4)]), U256::ZERO);
}

#[test]
fn the_other_binary_operators_keep_their_operand_order() {
    // Control: `binop`'s convention is right for everything else, so the fix
    // must not have "corrected" the shared helper.
    let mut d = deploy(SHIFTS);
    assert_eq!(
        d.view_u256("sub_order(uint256,uint256)", &[u(10), u(3)]),
        u(7)
    );
    assert_eq!(d.view_u256("band(uint256,uint256)", &[u(12), u(10)]), u(8));
}

const NEG: &str = r#"
record Bounds {
    field r: amount

    action do_add(a: amount, b: amount) { r = a + b }
    action do_sub(a: amount, b: amount) { r = a - b }
    action do_neg(a: amount)            { r = -a }

    view get returns amount { r }
}
"#;

#[test]
fn unary_minus_negates() {
    let mut d = deploy(NEG);
    let dep = d.deployer;
    // Pre-fix these stored 5 and 1: `-x` was `x`.
    d.send_ok(dep, "do_neg(uint256)", &[u(5)]);
    assert_eq!(d.view_u256("get()", &[]), U256::ZERO.wrapping_sub(&u(5)));
    d.send_ok(dep, "do_neg(uint256)", &[u(1)]);
    assert_eq!(d.view_u256("get()", &[]), U256::MAX);
    // Negating zero is still zero.
    d.send_ok(dep, "do_neg(uint256)", &[u(0)]);
    assert_eq!(d.view_u256("get()", &[]), U256::ZERO);
}

#[test]
fn checked_amount_arithmetic_is_the_control() {
    let mut d = deploy(NEG);
    let dep = d.deployer;
    d.send_reverts(dep, "do_sub(uint256,uint256)", &[u(0), u(5)]);
    let max = raw_call(
        "do_add(uint256,uint256)",
        &[U256::MAX.to_be_bytes(), word(1)],
    );
    let r = d.send_raw(dep, &max);
    assert!(
        !matches!(r.status, covenant_evm_runtime::TxStatus::Success),
        "checked `amount` addition must still revert on overflow"
    );
}

const SCALE: &str = r#"
record Scale {
    field base: duration
    field scaled_r: duration
    field scaled_l: duration

    action set_base(t0: time, t1: time) { base = t1 - t0 }
    action scale_right(n: amount) { scaled_r = base * n }
    action scale_left(n: amount) { scaled_l = n * base }

    view get_base returns duration { base }
    view get_scaled_r returns duration { scaled_r }
    view get_scaled_l returns duration { scaled_l }
}
"#;

#[test]
fn duration_times_amount_multiplies() {
    let mut d = deploy(SCALE);
    let dep = d.deployer;
    d.send_ok(dep, "set_base(uint256,uint256)", &[u(1000), u(1100)]);
    assert_eq!(d.view_u256("get_base()", &[]), u(100));

    // Pre-fix all three of these stored `base + n`: 107, 101 and 100.
    d.send_ok(dep, "scale_right(uint256)", &[u(7)]);
    assert_eq!(d.view_u256("get_scaled_r()", &[]), u(700));
    d.send_ok(dep, "scale_right(uint256)", &[u(1)]);
    assert_eq!(d.view_u256("get_scaled_r()", &[]), u(100));
    d.send_ok(dep, "scale_right(uint256)", &[u(0)]);
    assert_eq!(d.view_u256("get_scaled_r()", &[]), u(0));

    // The type checker admits both operand orders and MUL is commutative.
    d.send_ok(dep, "scale_left(uint256)", &[u(7)]);
    assert_eq!(d.view_u256("get_scaled_l()", &[]), u(700));
}

const CLOCK: &str = r#"
record Clock {
    field deadline: time
    field remaining: duration
    field far: time
    field gap: duration

    action set_deadline(t: time) { deadline = t }
    action compute_remaining(n: time) { remaining = deadline - n }
    action extend(dd: duration) { far = deadline + dd }
    action shrink(a: duration, b: duration) { gap = a - b }

    view get_remaining returns duration { remaining }
    view get_far returns time { far }
    view get_gap returns duration { gap }
}
"#;

#[test]
fn time_and_duration_subtraction_is_checked() {
    let mut d = deploy(CLOCK);
    let dep = d.deployer;

    // Ordinary in-range arithmetic keeps working.
    d.send_ok(dep, "set_deadline(uint256)", &[u(1_000)]);
    d.send_ok(dep, "compute_remaining(uint256)", &[u(900)]);
    assert_eq!(d.view_u256("get_remaining()", &[]), u(100));
    d.send_ok(dep, "shrink(uint256,uint256)", &[u(9), u(4)]);
    assert_eq!(d.view_u256("get_gap()", &[]), u(5));

    // A deadline in the past used to yield 2^256-100 instead of reverting, so
    // "time remaining" became astronomically large the instant it went
    // negative.
    d.send_reverts(dep, "compute_remaining(uint256)", &[u(1_100)]);
    assert_eq!(
        d.view_u256("get_remaining()", &[]),
        u(100),
        "state unchanged"
    );
    d.send_reverts(dep, "shrink(uint256,uint256)", &[u(1), u(2)]);
    assert_eq!(d.view_u256("get_gap()", &[]), u(5), "state unchanged");
}

#[test]
fn time_and_duration_addition_is_checked() {
    let mut d = deploy(CLOCK);
    let dep = d.deployer;

    d.send_ok(dep, "set_deadline(uint256)", &[u(1_000)]);
    d.send_ok(dep, "extend(uint256)", &[u(50)]);
    assert_eq!(d.view_u256("get_far()", &[]), u(1_050));

    // `time MAX + duration 1` wrapped to 0, i.e. "extend the lock" set the
    // unlock instant to the Unix epoch, which every `now >= unlock` guard
    // then passed.
    let set_max = raw_call("set_deadline(uint256)", &[U256::MAX.to_be_bytes()]);
    d.send_raw(dep, &set_max);
    d.send_reverts(dep, "extend(uint256)", &[u(1)]);
    assert_eq!(d.view_u256("get_far()", &[]), u(1_050), "state unchanged");
}

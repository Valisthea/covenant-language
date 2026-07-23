//! OMEGA V6 HGH-028 regression test.
//!
//! `emit_external_call`'s non-view (CALL) path used to push the operands in
//! the order `addr, value, gas` before invoking CALL. The mini-EVM
//! interpreter's `OP_CALL` handler (and real EVM CALL) pops in the order
//! `gas, addr, value, argsOffset, argsSize, retOffset, retSize` (gas on top
//! of stack). With the old push order, `gas` landed correctly, but `addr`
//! and `value` were swapped: the interpreter bound `addr_w` to the constant
//! 0 (the codegen's real `value` push) and `value` to the real target
//! contract address (an always-large, always-nonzero word). Since a fresh
//! contract's mock ETH balance is 0, `cur(0) < value(huge address)` was
//! always true, so every non-view external call failed with "insufficient
//! balance" and reverted via the KSR-CVN-027 success-flag check --
//! unconditionally, for every `IFoo.at(addr).action(...)` call in the
//! language, on both the mock interpreter and (since the interpreter's
//! CALL implements real EVM pop semantics) a real chain.
//!
//! After the fix, `value` (always 0 -- Covenant's external-call syntax has
//! no ETH-forwarding form yet) is pushed first and `addr` second, so the
//! interpreter binds them correctly: `value` is genuinely 0, the balance
//! check is skipped entirely, and the call succeeds.

use covenant_testing::{CovenantTestHarness, U256};

const SOURCE: &str = r#"
external contract IERC20 {
    function transfer(address, amount)
}
record Wallet {
    action send(tok: address, dest: address, val: amount) only caller {
        IERC20.at(tok).transfer(dest, val)
    }
}
"#;

#[test]
fn non_view_external_call_succeeds_ksr_cvn_hgh_028() {
    let mut h = CovenantTestHarness::new();
    let wallet = h.deploy(SOURCE, h.addrs.deployer).expect("deploy Wallet");

    // Before the fix: this reverted unconditionally (CALL's swapped
    // addr/value made the interpreter see a huge fake `value` against the
    // freshly-deployed contract's 0 balance). After the fix: `value` is
    // genuinely 0, so the balance check never fires and the call succeeds.
    let _ = h.call_ok(
        wallet,
        h.addrs.alice,
        "send(address,address,uint256)",
        &[
            h.addrs.bob.to_u256(),
            h.addrs.carol.to_u256(),
            U256::from_u64(100),
        ],
    );
}

#[test]
fn non_view_external_call_does_not_touch_caller_balance_ksr_cvn_hgh_028() {
    // Cross-check the root cause directly: a correctly-zero `value` must
    // leave the deployed contract's mock balance untouched by the CALL
    // (the interpreter's non-precompile CALL branch only mutates balances
    // when `value` is nonzero).
    let mut h = CovenantTestHarness::new();
    let wallet = h.deploy(SOURCE, h.addrs.deployer).expect("deploy Wallet");
    let before = h
        .host
        .balances
        .get(&wallet.address)
        .copied()
        .unwrap_or(U256::ZERO);

    let _ = h.call_ok(
        wallet,
        h.addrs.alice,
        "send(address,address,uint256)",
        &[
            h.addrs.bob.to_u256(),
            h.addrs.carol.to_u256(),
            U256::from_u64(100),
        ],
    );

    let after = h
        .host
        .balances
        .get(&wallet.address)
        .copied()
        .unwrap_or(U256::ZERO);
    assert_eq!(
        before, after,
        "a zero-value external call must not move the mock ETH balance"
    );
}

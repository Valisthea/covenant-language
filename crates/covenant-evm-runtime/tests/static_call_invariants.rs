//! Regression tests for the EVM `is_static` enforcement on opcodes
//! that should abort in a STATICCALL context (per EIP-214).
//!
//! Sprint 26 audit (KSR-CVN-PRELIM-004) added the `is_static` check to
//! `OP_LOG{0..4}`. This file verifies the fix and documents adjacent
//! invariants so future contributors can't accidentally regress.
//!
//! Hand-crafted bytecode is used directly (no compile step) so the
//! tests exercise the interpreter in isolation from the rest of the
//! pipeline.

use covenant_evm_runtime::evm::execute;
use covenant_evm_runtime::{Address, CallEnv, CallResult, HostState, MockPrecompileState, U256};

const ALICE: [u8; 20] = [0xaa; 20];

fn alice() -> Address {
    Address(ALICE)
}

fn run(bytecode: &[u8], is_static: bool) -> CallResult {
    let env = CallEnv {
        caller: alice(),
        address: alice(),
        value: U256::ZERO,
        input: Vec::new(),
        timestamp: 0,
        block_number: 1,
        chain_id: 31337,
        is_static,
    };
    let mut host = HostState::default();
    let mut precompiles = MockPrecompileState::default();
    execute(bytecode, &env, &mut host, &mut precompiles)
}

// ─── PRELIM-004: LOG opcodes must respect is_static ─────────────────

/// LOG0 in non-static context: succeeds and emits a LogEvent.
#[test]
fn log0_in_normal_context_succeeds() {
    // PUSH1 0x00 (length) PUSH1 0x00 (offset) LOG0 STOP
    let bc = vec![0x60, 0x00, 0x60, 0x00, 0xa0, 0x00];
    let r = run(&bc, /* is_static */ false);
    assert!(
        matches!(r, CallResult::Ok(_)),
        "LOG0 normal must succeed, got {r:?}"
    );
}

/// LOG0 in static context: must abort per EIP-214.
/// PRE-FIX: this returned Ok(_) (silently allowed). POST-FIX: aborts.
#[test]
fn log0_in_static_context_aborts() {
    let bc = vec![0x60, 0x00, 0x60, 0x00, 0xa0, 0x00];
    let r = run(&bc, /* is_static */ true);
    match r {
        CallResult::Abort(reason) => {
            assert!(
                reason.contains("LOG") && reason.to_lowercase().contains("static"),
                "abort reason should mention LOG + static, got: {reason}"
            );
        }
        other => panic!("LOG0 in static must abort, got {other:?}"),
    }
}

/// LOG4 (max topics variant) in static context: also aborts.
/// Sanity check that the abort applies to the whole OP_LOG0..=OP_LOG4 range.
#[test]
fn log4_in_static_context_aborts() {
    // Stack needs (offset, length, t0, t1, t2, t3) before LOG4.
    // PUSH1 0 (4× for topics) PUSH1 0 (length) PUSH1 0 (offset) LOG4 STOP
    let bc = vec![
        0x60, 0x00, // PUSH1 0   (topic 3)
        0x60, 0x00, // PUSH1 0   (topic 2)
        0x60, 0x00, // PUSH1 0   (topic 1)
        0x60, 0x00, // PUSH1 0   (topic 0)
        0x60, 0x00, // PUSH1 0   (length)
        0x60, 0x00, // PUSH1 0   (offset)
        0xa4, // LOG4
        0x00, // STOP
    ];
    let r = run(&bc, /* is_static */ true);
    assert!(
        matches!(r, CallResult::Abort(_)),
        "LOG4 in static must abort, got {r:?}"
    );
}

// ─── Adjacent invariants: pre-existing checks that must stay green ──

/// SSTORE in static context: must abort. This was already enforced
/// pre-Sprint-26 (it's the pattern PRELIM-004 mirrored). Test exists
/// to prevent future regression that might inadvertently remove it.
#[test]
fn sstore_in_static_context_aborts() {
    // PUSH1 0x42 (value) PUSH1 0x00 (key) SSTORE STOP
    let bc = vec![0x60, 0x42, 0x60, 0x00, 0x55, 0x00];
    let r = run(&bc, /* is_static */ true);
    assert!(
        matches!(r, CallResult::Abort(_)),
        "SSTORE in static must abort, got {r:?}"
    );
}

/// SSTORE in normal context: succeeds. Sanity check of the negative case.
#[test]
fn sstore_in_normal_context_succeeds() {
    let bc = vec![0x60, 0x42, 0x60, 0x00, 0x55, 0x00];
    let r = run(&bc, /* is_static */ false);
    assert!(matches!(r, CallResult::Ok(_)));
}

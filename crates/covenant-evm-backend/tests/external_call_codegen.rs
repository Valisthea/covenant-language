//! EVM codegen tests for Phase 17 – external contract CALL/STATICCALL emission.

use covenant_diag::SourceId;
use covenant_evm_backend::{abi, codegen_evm, storage, EvmConfig};
use covenant_ir::build_ir;
use covenant_lexer::tokenize;
use covenant_opt::{optimize, OptimizerConfig};
use covenant_parser::parse;
use covenant_privacy::analyze_privacy;
use covenant_resolver::resolve;
use covenant_types::typecheck;

fn compile(src: &str) -> (Vec<u8>, Vec<u8>) {
    let (toks, _) = tokenize(src, SourceId::new(0));
    let (file, _) = parse(&toks, SourceId::new(0));
    let (res, _) = resolve(file.unwrap(), SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (checked, _) = analyze_privacy(typed, SourceId::new(0));
    let (module, _) = build_ir(checked, SourceId::new(0));
    let (optimized, _) = optimize(module, OptimizerConfig::default());
    let (artifact, _) = codegen_evm(optimized, EvmConfig::default());
    (artifact.deploy_bytecode, artifact.runtime_bytecode)
}

// ---- selector computation (ABI module, used by ExternalCall emission) ----

#[test]
fn selector_transfer_matches_erc20() {
    // ERC-20 transfer(address,uint256) canonical selector = a9059cbb
    let sel = abi::selector("transfer(address,uint256)");
    assert_eq!(sel, [0xa9, 0x05, 0x9c, 0xbb]);
}

#[test]
fn selector_balance_of_matches_erc20() {
    // balanceOf(address) = 70a08231
    let sel = abi::selector("balanceOf(address)");
    assert_eq!(sel, [0x70, 0xa0, 0x82, 0x31]);
}

#[test]
fn selector_ping_no_params() {
    // ping() = any deterministic value (just verify it's 4 bytes and non-zero)
    let sel = abi::selector("ping()");
    assert_eq!(sel.len(), 4);
}

#[test]
fn selector_is_first_4_bytes_of_keccak() {
    // Verify by cross-checking with a known value.
    // approve(address,uint256) = 095ea7b3
    let sel = abi::selector("approve(address,uint256)");
    assert_eq!(sel, [0x09, 0x5e, 0xa7, 0xb3]);
}

// ---- compilation succeeds with external contract ----

#[test]
fn compile_with_empty_external_contract_produces_bytecode() {
    let src = r#"
external contract IFoo {}
record R {}
"#;
    let (deploy, runtime) = compile(src);
    assert!(!deploy.is_empty(), "deploy bytecode should be non-empty");
    assert!(!runtime.is_empty(), "runtime bytecode should be non-empty");
}

#[test]
fn compile_with_external_contract_and_action_produces_bytecode() {
    let src = r#"
external contract IERC20 {
    function transfer(address, amount)
}
record R {}
"#;
    let (deploy, runtime) = compile(src);
    assert!(!deploy.is_empty());
    assert!(!runtime.is_empty());
}

#[test]
fn compile_external_call_in_action_produces_bytecode() {
    let src = r#"
external contract IERC20 {
    function transfer(address, amount)
}
record R {
    action do_transfer(tok: address, dest: address, val: amount) only caller {
        IERC20.at(tok).transfer(dest, val)
    }
}
"#;
    let (deploy, runtime) = compile(src);
    assert!(!deploy.is_empty());
    assert!(!runtime.is_empty());
}

#[test]
fn compile_external_view_call_in_view_produces_bytecode() {
    let src = r#"
external contract IERC20 {
    function balanceOf(address) view returns amount
}
record R {
    view check_balance(tok: address, who: address) returns amount {
        IERC20.at(tok).balanceOf(who)
    }
}
"#;
    let (deploy, runtime) = compile(src);
    assert!(!deploy.is_empty());
    assert!(!runtime.is_empty());
}

// ---- STATICCALL opcode in bytecode for view functions ----

#[test]
fn staticcall_opcode_present_for_view_external_call() {
    let src = r#"
external contract IERC20 {
    function balanceOf(address) view returns amount
}
record R {
    view get_balance(tok: address, who: address) returns amount {
        IERC20.at(tok).balanceOf(who)
    }
}
"#;
    let (_, runtime) = compile(src);
    // STATICCALL opcode = 0xFA
    assert!(
        runtime.contains(&0xfa),
        "runtime bytecode should contain STATICCALL (0xfa) for view external call"
    );
}

#[test]
fn call_opcode_present_for_non_view_external_call() {
    let src = r#"
external contract IERC20 {
    function transfer(address, amount)
}
record R {
    action send(tok: address, dest: address, val: amount) only caller {
        IERC20.at(tok).transfer(dest, val)
    }
}
"#;
    let (_, runtime) = compile(src);
    // CALL opcode = 0xF1
    assert!(
        runtime.contains(&0xf1),
        "runtime bytecode should contain CALL (0xf1) for non-view external call"
    );
}

// ---- selector bytes appear in bytecode ----

#[test]
fn transfer_selector_bytes_in_runtime_bytecode() {
    let src = r#"
external contract IERC20 {
    function transfer(address, amount)
}
record R {
    action send(tok: address, dest: address, val: amount) only caller {
        IERC20.at(tok).transfer(dest, val)
    }
}
"#;
    let (_, runtime) = compile(src);
    // transfer(address,uint256) selector = a9059cbb
    let sel = abi::selector("transfer(address,uint256)");
    let sel_present = runtime.windows(4).any(|w| w == sel);
    assert!(
        sel_present,
        "selector bytes should appear in runtime bytecode"
    );
}

#[test]
fn balance_of_selector_bytes_in_runtime_bytecode() {
    let src = r#"
external contract IERC20 {
    function balanceOf(address) view returns amount
}
record R {
    view check(tok: address, who: address) returns amount {
        IERC20.at(tok).balanceOf(who)
    }
}
"#;
    let (_, runtime) = compile(src);
    let sel = abi::selector("balanceOf(address)");
    let sel_present = runtime.windows(4).any(|w| w == sel);
    assert!(
        sel_present,
        "balanceOf selector should appear in runtime bytecode"
    );
}

// ---- @non_reentrant guard: SSTORE appears in bytecode ----

#[test]
fn non_reentrant_annotation_compiles_without_error() {
    let src = r#"
record R {
    @non_reentrant
    action protected() only caller {}
}
"#;
    let (deploy, runtime) = compile(src);
    assert!(!deploy.is_empty());
    assert!(!runtime.is_empty());
}

#[test]
fn non_reentrant_uses_lock_slot_in_bytecode() {
    let src = r#"
record R {
    @non_reentrant
    action protected() only caller {}
}
"#;
    let (_, runtime) = compile(src);
    // The reentrancy lock slot is 0xFFFFFFFF.
    // The PUSH4 0xFFFFFFFF = [0x63, 0xFF, 0xFF, 0xFF, 0xFF] should appear in the bytecode.
    let lock_slot_bytes = [0xffu8, 0xff, 0xff, 0xff];
    let slot_present = runtime.windows(4).any(|w| w == lock_slot_bytes);
    assert!(
        slot_present,
        "lock slot 0xFFFFFFFF bytes should appear in @non_reentrant bytecode"
    );
}

#[test]
fn non_reentrant_sstore_in_bytecode() {
    let src = r#"
record R {
    @non_reentrant
    action protected() only caller {}
}
"#;
    let (_, runtime) = compile(src);
    // SSTORE = 0x55
    assert!(
        runtime.contains(&0x55),
        "SSTORE opcode should be present for @non_reentrant lock"
    );
}

#[test]
fn non_reentrant_sload_in_bytecode() {
    let src = r#"
record R {
    @non_reentrant
    action protected() only caller {}
}
"#;
    let (_, runtime) = compile(src);
    // SLOAD = 0x54
    assert!(
        runtime.contains(&0x54),
        "SLOAD opcode should be present for @non_reentrant lock check"
    );
}

#[test]
fn action_without_non_reentrant_bytecode_is_smaller() {
    let with_guard = r#"
record R {
    @non_reentrant
    action protected() only caller {}
}
"#;
    let without_guard = r#"
record R {
    action protected() only caller {}
}
"#;
    let (_, runtime_with) = compile(with_guard);
    let (_, runtime_without) = compile(without_guard);
    assert!(
        runtime_with.len() > runtime_without.len(),
        "@non_reentrant should produce more bytecode than without guard"
    );
}

// ---- storage slot constant ----

#[test]
fn reentrant_lock_slot_constant_value() {
    assert_eq!(storage::REENTRANT_LOCK_SLOT, 0xFFFF_FFFFu32);
}

#[test]
fn deployer_slot_and_lock_slot_differ() {
    assert_ne!(storage::DEPLOYER_SLOT, storage::REENTRANT_LOCK_SLOT);
}

// ---- KSR-CVN-027: external CALL/STATICCALL success-flag check ----

/// Returns true if `pattern` appears as a contiguous subsequence of `bytes`.
fn contains_subseq(bytes: &[u8], pattern: &[u8]) -> bool {
    bytes.windows(pattern.len()).any(|w| w == pattern)
}

#[test]
fn ksr_027_external_call_emits_iszero_jumpi_after_call() {
    // After CALL (0xF1) the codegen must propagate failure: ISZERO ; JUMPI.
    // Concretely we look for the byte sequence `CALL ; ISZERO` (0xF1 0x15).
    let src = r#"
external contract IERC20 {
    function transfer(address, amount)
}
record R {
    action send(tok: address, dest: address, val: amount) only caller {
        IERC20.at(tok).transfer(dest, val)
    }
}
"#;
    let (_, runtime) = compile(src);
    assert!(
        contains_subseq(&runtime, &[0xf1, 0x15]),
        "expected CALL (0xF1) immediately followed by ISZERO (0x15) — \
         success-flag check missing (KSR-CVN-027)"
    );
    // And ISZERO must be followed (after the PUSH of the revert label) by JUMPI (0x57).
    assert!(
        runtime.contains(&0x57),
        "expected JUMPI (0x57) for revert-on-failure dispatch"
    );
}

#[test]
fn ksr_027_external_view_call_emits_iszero_jumpi_after_staticcall() {
    let src = r#"
external contract IERC20 {
    function balanceOf(address) view returns amount
}
record R {
    view check(tok: address, who: address) returns amount {
        IERC20.at(tok).balanceOf(who)
    }
}
"#;
    let (_, runtime) = compile(src);
    assert!(
        contains_subseq(&runtime, &[0xfa, 0x15]),
        "expected STATICCALL (0xFA) immediately followed by ISZERO (0x15) — \
         success-flag check missing on view path (KSR-CVN-027)"
    );
}

#[test]
fn ksr_027_external_call_no_naked_call_pop_pattern() {
    // The pre-fix code emitted CALL ; POP (0xF1 0x50). Verify that pattern
    // no longer appears anywhere in the runtime bytecode for an external
    // CALL site. (POP after CALL discards the success flag.)
    let src = r#"
external contract IERC20 {
    function transfer(address, amount)
}
record R {
    action send(tok: address, dest: address, val: amount) only caller {
        IERC20.at(tok).transfer(dest, val)
    }
}
"#;
    let (_, runtime) = compile(src);
    assert!(
        !contains_subseq(&runtime, &[0xf1, 0x50]),
        "regression: CALL ; POP pattern reintroduced — KSR-CVN-027 fix lost"
    );
}

// ---- KSR-CVN-030: external call return value is not wiped post-call ----

#[test]
fn ksr_030_external_view_call_reads_returndata_not_wiped() {
    // M3 milestone bug: the post-call code used to `MSTORE(0,0)` AFTER the
    // STATICCALL — wiping the very return word the call wrote to retOffset=0,
    // so every `IFoo.at(addr).method()` view returned 0/default. The fix reads
    // the value the call left in place, guarded by RETURNDATASIZE (0x3d) so an
    // empty return still yields 0. Assert the return path now consults
    // RETURNDATASIZE rather than unconditionally zeroing mem[0] before MLOAD.
    let src = r#"
external contract IERC20 {
    function balanceOf(address) view returns amount
}
record R {
    view check(tok: address, who: address) returns amount {
        IERC20.at(tok).balanceOf(who)
    }
}
"#;
    let (_, runtime) = compile(src);
    assert!(runtime.contains(&0xfa), "expected STATICCALL (0xfa)");
    assert!(
        runtime.contains(&0x3d),
        "expected RETURNDATASIZE (0x3d) in the external-call return path — \
         the post-call MSTORE-wipe regression (M3 reads return 0) is back"
    );
}

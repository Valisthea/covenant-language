//! Regression tests for Fix 1: calldata → param-slot prelude.
//!
//! Every public function's entry block must begin with a sequence that copies
//! each static-ABI parameter from its calldata offset into its SSA memory
//! slot. Without this prelude, the IR body reads parameters as zero — the
//! bug that defeated `approve / allowance` round-tripping in Phase 10.

use covenant_diag::SourceId;
use covenant_evm_backend::{codegen::value_memory_offset, codegen_evm, EvmConfig};
use covenant_ir::build_ir;
use covenant_lexer::tokenize;
use covenant_opt::{optimize, OptimizerConfig};
use covenant_parser::parse;
use covenant_privacy::analyze_privacy;
use covenant_resolver::resolve;
use covenant_stdlib::{lower_stdlib, StdlibConfig};
use covenant_types::typecheck;

const COIN: &str = include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov");

/// Compile Coin through the full pipeline, returning the runtime bytecode.
fn coin_runtime() -> Vec<u8> {
    let (toks, _) = tokenize(COIN, SourceId::new(0));
    let (file, _) = parse(&toks, SourceId::new(0));
    let (res, _) = resolve(file.unwrap(), SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (checked, _) = analyze_privacy(typed, SourceId::new(0));
    let (module, _) = build_ir(checked, SourceId::new(0));
    let (module, _) = lower_stdlib(module, StdlibConfig::default());
    let (optimized, _) = optimize(module, OptimizerConfig::default());
    codegen_evm(optimized, EvmConfig::default())
        .0
        .runtime_bytecode
}

/// Scan the runtime for the canonical param-load micro-sequence:
/// `PUSH1 <off> CALLDATALOAD PUSH1/PUSH2 <slot> MSTORE`. Returns a Vec of
/// (calldata_offset, memory_slot) pairs for every such pattern found. We skip
/// over PUSH data so we don't mistake bytes inside literals for opcodes.
fn find_param_copies(code: &[u8]) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    let mut i = 0;
    while i + 1 < code.len() {
        let op = code[i];
        // Recognize PUSH1 <off>, CALLDATALOAD, PUSH{n} <slot>, MSTORE.
        if op == 0x60 && i + 5 < code.len() && code[i + 2] == 0x35 {
            let off = code[i + 1] as u32;
            let j = i + 3;
            // PUSH1..PUSH2 for the slot value.
            if j >= code.len() {
                break;
            }
            let push = code[j];
            let (slot, next): (u32, usize) = if push == 0x60 && j + 2 < code.len() {
                (code[j + 1] as u32, j + 2)
            } else if push == 0x61 && j + 3 < code.len() {
                (((code[j + 1] as u32) << 8) | (code[j + 2] as u32), j + 3)
            } else {
                i += 1;
                continue;
            };
            if next < code.len() && code[next] == 0x52 {
                out.push((off, slot));
                i = next + 1;
                continue;
            }
        }
        // Skip over PUSH data regions to avoid false-positive matches.
        if (0x60..=0x7f).contains(&op) {
            let n = (op - 0x60 + 1) as usize;
            i += 1 + n;
        } else {
            i += 1;
        }
    }
    out
}

#[test]
fn coin_runtime_contains_param_loads() {
    let code = coin_runtime();
    let copies = find_param_copies(&code);
    // Synthesized functions with params:
    //   transfer(address,uint256)            — 2 params
    //   transferFrom(address,address,uint256) — 3 params
    //   approve(address,uint256)             — 2 params
    //   allowance(address,address)           — 2 params
    //   balanceOf(address)                   — 1 param
    // Total static params = 2+3+2+2+1 = 10.
    assert!(
        copies.len() >= 10,
        "expected ≥ 10 param-load sequences, found {}: {copies:?}",
        copies.len()
    );
}

#[test]
fn param_offsets_start_at_four_and_are_32_aligned() {
    let code = coin_runtime();
    let copies = find_param_copies(&code);
    // Valid calldata offsets for Covenant's V0.1 ABI: 4, 36, 68.
    let allowed: [u32; 3] = [4, 36, 68];
    for (off, _slot) in &copies {
        assert!(
            allowed.contains(off),
            "unexpected calldata offset {off}; expected one of {allowed:?}"
        );
    }
}

#[test]
fn param_slots_live_above_ssa_base() {
    // Memory layout: scratch [0, 0x80) / SSA values at 0x80 + id*32.
    // Param slots must land strictly above the scratch region.
    let code = coin_runtime();
    for (_, slot) in find_param_copies(&code) {
        assert!(
            slot >= 0x80,
            "param slot 0x{slot:x} collides with the keccak/precompile scratch region"
        );
    }
}

#[test]
fn value_memory_offset_matches_layout() {
    // Value::new(0) → 0x80, Value::new(1) → 0xA0, etc. This test pins the
    // invariant that the param-prelude and the rest of the codegen agree on
    // the same slot formula.
    use covenant_ir::id::Value;
    assert_eq!(value_memory_offset(Value(0)), 0x80);
    assert_eq!(value_memory_offset(Value(1)), 0xA0);
    assert_eq!(value_memory_offset(Value(2)), 0xC0);
}

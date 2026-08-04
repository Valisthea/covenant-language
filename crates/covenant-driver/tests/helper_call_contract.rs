//! A helper call site must obey the method it is calling, not the target.
//!
//! Before this, the backend applied one rule to every helper method on a
//! helper target: emit `CALL`, accept any returndata of at least 32 bytes,
//! read `MLOAD(0)`. Two things were wrong with that.
//!
//! Five of the seventeen dispatched methods are `view` or `pure` in the
//! deployed helper, and `CALL` gave them the authority to write, which
//! nothing in their semantics needs.
//!
//! And one method, `amnesiaDestroy`, returns Solidity dynamic `bytes`, whose
//! first returndata word is the ABI offset. The single-word reader handed back
//! `0x20`, the synthesized `destroy()` asserted on it under a comment claiming
//! the phase only advanced on success, and returned 32 from a function the ABI
//! declares `bool`.
//!
//! The method now carries its own return shape and call mode, and the call
//! site reads them.

use covenant_diag::{DiagCode, Diagnostic, DiagnosticLevel, SourceId};
use covenant_evm_backend::target::{helper_method_for_opcode, CallMode, ReturnShape};
use covenant_evm_backend::{EvmConfig, Target};
use covenant_opt::OptimizerConfig;
use covenant_stdlib::StdlibConfig;

const E535_HELPER_RETURN_UNDECODABLE: DiagCode = DiagCode(535);

const CEREMONY: &str = "\
-- ERC-8228: Cryptographic Amnesia Ceremony (Styx Protocol)
ceremony Wipe {
    guardians: 3
    threshold: 2

    on_destroy { }
}";

/// Uses an FHE write and a `reveal`, so it exercises both call modes.
const FHE: &str = "\
encrypted counter Shielded {
    total: amount
    action bump(by: amount) { total += by }
    reveal total to owner
}";

fn compile(source: &str, target: Target) -> (Option<Vec<u8>>, Vec<Diagnostic>) {
    let (artifact, diags) = covenant_driver::compile(
        source,
        SourceId::new(0),
        EvmConfig::for_target(target),
        StdlibConfig::default(),
        OptimizerConfig::default(),
    );
    (artifact.map(|a| a.runtime_bytecode), diags)
}

fn has_error(diags: &[Diagnostic], code: DiagCode) -> bool {
    diags
        .iter()
        .any(|d| d.level == DiagnosticLevel::Error && d.code == code)
}

/// Count `CALL` and `STATICCALL` while skipping PUSH payloads.
///
/// A plain byte scan is not enough: `0xf1` and `0xfa` occur constantly inside
/// pushed constants, so counting raw occurrences reports calls that are not
/// there.
fn count_calls(code: &[u8]) -> (usize, usize) {
    let (mut calls, mut staticcalls, mut i) = (0, 0, 0);
    while i < code.len() {
        let op = code[i];
        if (0x60..=0x7f).contains(&op) {
            i += 1 + (op as usize - 0x5f);
            continue;
        }
        match op {
            0xf1 => calls += 1,
            0xfa => staticcalls += 1,
            _ => {}
        }
        i += 1;
    }
    (calls, staticcalls)
}

#[test]
fn a_method_returning_dynamic_bytes_is_refused() {
    let (_, diags) = compile(CEREMONY, Target::Sepolia);
    assert!(
        has_error(&diags, E535_HELPER_RETURN_UNDECODABLE),
        "the destroy path compiled against a helper method returning dynamic \
         bytes, so its result is an ABI offset read as a value"
    );
}

/// The refusal is scoped to the shape, not to the construct. MockChain
/// implements the opcode as a native precompile returning a single word, so
/// there is nothing undecodable there.
#[test]
fn the_same_ceremony_still_builds_for_the_native_precompile_target() {
    let (artifact, diags) = compile(CEREMONY, Target::MockChain);
    assert!(
        !has_error(&diags, E535_HELPER_RETURN_UNDECODABLE),
        "mockchain was refused, but its precompile returns a word"
    );
    assert!(artifact.is_some(), "no artifact for mockchain: {diags:#?}");
}

/// And a helper-using contract that never reaches the dynamic-return method
/// must be unaffected, otherwise the refusal is a blanket ban on ceremonies
/// and FHE alike.
#[test]
fn a_helper_contract_without_the_dynamic_method_still_builds() {
    let (artifact, diags) = compile(FHE, Target::Sepolia);
    assert!(
        !has_error(&diags, E535_HELPER_RETURN_UNDECODABLE),
        "a contract that never calls amnesiaDestroy was refused: {diags:#?}"
    );
    assert!(artifact.is_some(), "no artifact: {diags:#?}");
}

#[test]
fn read_only_helper_methods_use_staticcall() {
    let (code, diags) = compile(FHE, Target::Sepolia);
    let code = code.unwrap_or_else(|| panic!("no artifact: {diags:#?}"));
    let (calls, staticcalls) = count_calls(&code);

    // `total += by` reaches the FHE arithmetic, which the deployed helper
    // declares state-changing. `reveal` reaches `decrypt`, which is `view`.
    assert!(
        staticcalls >= 1,
        "no STATICCALL emitted, so the read-only `decrypt` still has the \
         authority to write. Found {calls} CALL and {staticcalls} STATICCALL"
    );
    assert!(
        calls >= 1,
        "no CALL emitted, so a state-changing FHE method was demoted to \
         STATICCALL and would revert on write. Found {calls} CALL and \
         {staticcalls} STATICCALL"
    );
}

/// The table is the single source of truth for three properties at once, so
/// assert the classification directly. If a method is reclassified, this says
/// so in one place rather than through a bytecode diff.
#[test]
fn the_dispatch_table_classifies_every_method() {
    let expected: &[(&str, ReturnShape, CallMode)] = &[
        ("AmnesiaBegin", ReturnShape::Word, CallMode::Call),
        ("AmnesiaSubmitShare", ReturnShape::Bool, CallMode::Call),
        ("AmnesiaFinalize", ReturnShape::Bool, CallMode::Call),
        (
            "DestructionProof",
            ReturnShape::DynamicBytes,
            CallMode::Call,
        ),
        ("FheAdd", ReturnShape::Word, CallMode::Call),
        ("RevealDecrypt", ReturnShape::Word, CallMode::StaticCall),
        ("ZkVerify", ReturnShape::Bool, CallMode::StaticCall),
        ("ZkNullifier", ReturnShape::Word, CallMode::StaticCall),
        ("PqVerifyDilithium", ReturnShape::Bool, CallMode::StaticCall),
        ("PqRand", ReturnShape::Word, CallMode::StaticCall),
    ];
    for (opcode, shape, mode) in expected {
        let m = helper_method_for_opcode(opcode)
            .unwrap_or_else(|| panic!("`{opcode}` is not in the dispatch table"));
        assert_eq!(m.returns, *shape, "`{opcode}` return shape");
        assert_eq!(m.mode, *mode, "`{opcode}` call mode");
    }
}

/// A negative control for the disassembler. If `count_calls` were miscounting,
/// for instance by reading PUSH payloads as opcodes, the STATICCALL assertion
/// above could pass on a contract that emits none.
#[test]
fn the_call_counter_does_not_count_push_payloads() {
    // PUSH1 0xf1, PUSH1 0xfa, STOP. Two bytes that look like CALL and
    // STATICCALL, both inside push payloads, and no call at all.
    let code = [0x60u8, 0xf1, 0x60, 0xfa, 0x00];
    assert_eq!(
        count_calls(&code),
        (0, 0),
        "the counter read push payloads as opcodes, so its counts prove nothing"
    );
    // And it does find a real one.
    assert_eq!(count_calls(&[0xf1, 0xfa]), (1, 1));
}

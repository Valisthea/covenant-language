//! KSR-CVN-012 regression tests: proxy initializer re-init guard.
//!
//! Verifies that every public `action initialize(...)` emitted by the EVM
//! backend is wrapped in a re-initialization guard:
//!   - SLOAD on an EIP-7201-derived slot
//!   - ISZERO → JUMPI to an ok_label
//!   - fall-through revert
//!   - at ok_label: SSTORE 1 to the same slot
//!
//! The attack documented in OMEGA V4: a Covenant-compiled UUPS / Transparent
//! proxy implementation exposes `initialize(...)` as a permanently callable
//! function. A second call rebinds owner state to the attacker's address.
//! The guard fixes this by marking the flag at first call and reverting
//! every subsequent call.

use covenant_diag::SourceId;
use covenant_evm_backend::{codegen_evm, EvmConfig};
use covenant_ir::build_ir;
use covenant_lexer::tokenize;
use covenant_opt::{optimize, OptimizerConfig};
use covenant_parser::parse;
use covenant_privacy::analyze_privacy;
use covenant_resolver::resolve;
use covenant_stdlib::{lower_stdlib, StdlibConfig};
use covenant_types::typecheck;
use sha3::{Digest, Keccak256};

const OP_SLOAD: u8 = 0x54;
const OP_SSTORE: u8 = 0x55;
const OP_ISZERO: u8 = 0x15;
const OP_JUMPI: u8 = 0x57;
const OP_REVERT: u8 = 0xfd;
const OP_PUSH32: u8 = 0x7f;

/// Fixture: `example_11_safe_transfer.cov` declares `action initialize(who: address)`.
const SAFE_TRANSFER: &str =
    include_str!("../../covenant-lexer/tests/fixtures/example_11_safe_transfer.cov");

fn compile_runtime(src: &str) -> Vec<u8> {
    let (toks, _) = tokenize(src, SourceId::new(0));
    let (file, _) = parse(&toks, SourceId::new(0));
    let (res, _) = resolve(file.unwrap(), SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (checked, _) = analyze_privacy(typed, SourceId::new(0));
    let (module, _) = build_ir(checked, SourceId::new(0));
    let (with_std, _) = lower_stdlib(module, StdlibConfig::default());
    let (optimized, _) = optimize(with_std, OptimizerConfig::default());
    let (artifact, _) = codegen_evm(optimized, EvmConfig::default());
    artifact.runtime_bytecode
}

/// Compute the expected initializer flag slot bytes from a module name
/// (must match the helper in covenant-evm-backend/src/codegen.rs).
fn expected_flag_slot(module_name: &str) -> [u8; 32] {
    let mut hasher = Keccak256::new();
    hasher.update(b"covenant.proxy.initializer.");
    hasher.update(module_name.as_bytes());
    hasher.finalize().into()
}

/// Find all occurrences of a 32-byte PUSH32 payload in `code`. Returns the
/// byte offsets where the PUSH32 opcode sits.
fn find_push32_sites(code: &[u8], needle: &[u8; 32]) -> Vec<usize> {
    let mut hits = Vec::new();
    let mut i = 0;
    while i < code.len() {
        let op = code[i];
        if op == OP_PUSH32 && i + 33 <= code.len() && &code[i + 1..i + 33] == needle {
            hits.push(i);
            i += 33;
        } else if (0x60..=0x7f).contains(&op) {
            let n = (op - 0x60 + 1) as usize;
            i += 1 + n;
        } else {
            i += 1;
        }
    }
    hits
}

// ── The flag slot is referenced in the runtime bytecode ──────────────────────

#[test]
fn initializer_flag_slot_appears_in_runtime() {
    let runtime = compile_runtime(SAFE_TRANSFER);
    // The module declared by example_11 is named `SafeTransfer`.
    let slot = expected_flag_slot("SafeTransfer");
    let sites = find_push32_sites(&runtime, &slot);
    assert!(
        sites.len() >= 2,
        "expected at least two PUSH32 of the initializer flag slot \
         (one for SLOAD, one for SSTORE); got {}. \
         Slot bytes: 0x{}. Runtime size: {} bytes.",
        sites.len(),
        hex::encode(slot),
        runtime.len()
    );
}

// ── The guard's full structural pattern is present ───────────────────────────

#[test]
fn initializer_guard_has_sload_iszero_jumpi_revert_sstore() {
    let runtime = compile_runtime(SAFE_TRANSFER);
    let slot = expected_flag_slot("SafeTransfer");
    let sites = find_push32_sites(&runtime, &slot);
    assert!(
        !sites.is_empty(),
        "no initializer flag PUSH32 found: guard missing entirely"
    );

    // First site: PUSH32 slot → SLOAD → ISZERO → PUSH2 label → JUMPI → PUSH0 PUSH0 REVERT
    let first = sites[0];
    // Skip PUSH32 opcode + 32-byte payload.
    let mut pc = first + 33;
    assert_eq!(
        runtime[pc], OP_SLOAD,
        "byte after PUSH32 flag_slot must be SLOAD (guard loads flag)"
    );
    pc += 1;
    assert_eq!(
        runtime[pc], OP_ISZERO,
        "byte after SLOAD must be ISZERO (true if flag == 0, i.e. not yet initialized)"
    );
    pc += 1;
    // Skip PUSH2 ok_label (3 bytes).
    assert_eq!(
        runtime[pc], 0x61,
        "byte after ISZERO must be PUSH2 (ok_label push)"
    );
    pc += 3;
    assert_eq!(
        runtime[pc], OP_JUMPI,
        "byte after PUSH2 ok_label must be JUMPI (skip revert if flag clear)"
    );
    pc += 1;
    // Fall-through: PUSH0 PUSH0 REVERT (revert with no data if already initialized).
    assert_eq!(
        runtime[pc], 0x5f,
        "fall-through from JUMPI must start with PUSH0"
    );
    assert_eq!(
        runtime[pc + 1],
        0x5f,
        "PUSH0 must be followed by another PUSH0"
    );
    assert_eq!(
        runtime[pc + 2],
        OP_REVERT,
        "PUSH0 PUSH0 must be followed by REVERT"
    );
}

// ── SSTORE to the flag slot is present (guard sets the flag on success) ──────

#[test]
fn initializer_guard_sstores_to_flag_slot() {
    let runtime = compile_runtime(SAFE_TRANSFER);
    let slot = expected_flag_slot("SafeTransfer");
    let sites = find_push32_sites(&runtime, &slot);
    assert!(
        sites.len() >= 2,
        "expected PUSH32 flag_slot for both SLOAD and SSTORE, got {}",
        sites.len()
    );
    // The second PUSH32 flag_slot must be followed by SSTORE.
    let second = sites[1];
    let pc = second + 33;
    assert_eq!(
        runtime[pc], OP_SSTORE,
        "second PUSH32 flag_slot must be followed by SSTORE (guard sets flag = 1)"
    );
}

// ── The slot is derived (not zero, not a sequential user slot) ───────────────

#[test]
fn initializer_flag_slot_is_keccak_derived_not_sequential() {
    let slot = expected_flag_slot("SafeTransfer");
    // Sequential user slots are small (0..=0xFFFFFFFF as little-big-endian
    // zero-padded). A keccak-derived slot is overwhelmingly unlikely to
    // have 28 leading zero bytes.
    let is_sequential_shape = slot[0..28].iter().all(|&b| b == 0);
    assert!(
        !is_sequential_shape,
        "initializer flag slot must be keccak-derived (EIP-7201 style), \
         not a small sequential slot: else it can collide with user fields. \
         Got: 0x{}",
        hex::encode(slot)
    );
}

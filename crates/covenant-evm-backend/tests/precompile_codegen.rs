//! KSR-CVN-013 + KSR-CVN-014 regression tests — hardened precompile STATICCALL.
//!
//! Verifies that every precompile call sequence emitted by the EVM backend:
//!   - does NOT discard the STATICCALL success flag (fails closed, not open);
//!   - checks RETURNDATASIZE against the expected response size and reverts
//!     when the target address is absent (zero-length returndata);
//!   - zeroes the return slot before STATICCALL so a stale operand cannot
//!     be read back as the "verified" result.
//!
//! The regression targets two forgery primitives documented in OMEGA V4:
//!   1. A cross-chain replay where the target chain does not ship the
//!      Covenant precompile patchset: STATICCALL to an EOA / missing address
//!      returns success=1 with empty returndata, and the unhardened emitter
//!      would MLOAD operand-0 back as a non-zero "verified" result.
//!   2. A precompile revert (invalid signature, malformed ciphertext,
//!      out-of-gas) would previously be silently downgraded to result=0
//!      without reverting the caller.

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

const OP_STATICCALL: u8 = 0xfa;
const OP_ISZERO: u8 = 0x15;
const OP_JUMPI: u8 = 0x57;
const OP_RETURNDATASIZE: u8 = 0x3d;
const OP_EQ: u8 = 0x14;
const OP_POP: u8 = 0x50;
const OP_MSTORE: u8 = 0x52;
const OP_PUSH0: u8 = 0x5f;

/// Confidential token fixture — `confidential token` synthesises FHE actions
/// (transferEncrypted, approveEncrypted) that route through emit_precompile_call
/// at runtime, plus a genesis-mint STATICCALL in the constructor.
const CONFIDENTIAL_TOKEN: &str =
    include_str!("../../covenant-lexer/tests/fixtures/example_06_secret_coin.cov");

fn compile(src: &str) -> (Vec<u8>, Vec<u8>) {
    let (toks, _) = tokenize(src, SourceId::new(0));
    let (file, _) = parse(&toks, SourceId::new(0));
    let (res, _) = resolve(file.unwrap(), SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (checked, _) = analyze_privacy(typed, SourceId::new(0));
    let (module, _) = build_ir(checked, SourceId::new(0));
    let (with_std, _) = lower_stdlib(module, StdlibConfig::default());
    let (optimized, _) = optimize(with_std, OptimizerConfig::default());
    let (artifact, _) = codegen_evm(optimized, EvmConfig::default());
    (artifact.deploy_bytecode, artifact.runtime_bytecode)
}

/// Return the byte offsets of every STATICCALL opcode in `code`, ignoring
/// PUSH-data regions (so PUSH32 payload bytes that happen to equal 0xfa
/// don't produce false positives).
fn staticcall_offsets(code: &[u8]) -> Vec<usize> {
    let mut hits = Vec::new();
    let mut i = 0;
    while i < code.len() {
        let op = code[i];
        if op == OP_STATICCALL {
            hits.push(i);
            i += 1;
        } else if (0x60..=0x7f).contains(&op) {
            let n = (op - 0x60 + 1) as usize;
            i += 1 + n;
        } else {
            i += 1;
        }
    }
    hits
}

/// Assert every STATICCALL in `code` is followed (within `window` bytes) by
/// the hardened check sequence required by KSR-CVN-013 and KSR-CVN-014.
fn assert_all_staticcalls_are_hardened(code: &[u8], label: &str) -> usize {
    let sites = staticcall_offsets(code);
    for off in &sites {
        let end = (off + 24).min(code.len());
        let window = &code[*off + 1..end];

        // KSR-CVN-013: success flag must NOT be immediately POP'd.
        assert_ne!(
            window.first().copied().unwrap_or(0),
            OP_POP,
            "{label} STATICCALL at 0x{off:x}: success flag is immediately POP'd \
             (KSR-CVN-013 regression — discarding success flag)"
        );

        // KSR-CVN-013: ISZERO + JUMPI must appear in the window (success check).
        assert!(
            window.contains(&OP_ISZERO) && window.contains(&OP_JUMPI),
            "{label} STATICCALL at 0x{off:x}: no ISZERO/JUMPI after call \
             (KSR-CVN-013 — success flag not routed to revert)"
        );

        // KSR-CVN-014: RETURNDATASIZE must appear in the window.
        assert!(
            window.contains(&OP_RETURNDATASIZE),
            "{label} STATICCALL at 0x{off:x}: no RETURNDATASIZE check \
             (KSR-CVN-014 — stale-memory forgery primitive reachable)"
        );
    }
    sites.len()
}

// ── KSR-CVN-013 + KSR-CVN-014 : runtime precompile sites ─────────────────────

#[test]
fn runtime_staticcalls_are_hardened() {
    let (deploy, runtime) = compile(CONFIDENTIAL_TOKEN);
    let raw_deploy = deploy.iter().filter(|&&b| b == 0xfa).count();
    let raw_runtime = runtime.iter().filter(|&&b| b == 0xfa).count();
    let n = assert_all_staticcalls_are_hardened(&runtime, "runtime");
    assert!(
        n > 0,
        "confidential token runtime must emit at least one FHE precompile STATICCALL \
         (auto-generated transferEncrypted / approveEncrypted). \
         deploy len={}, runtime len={}, raw 0xfa in deploy={}, in runtime={}, scanned sites={}",
        deploy.len(),
        runtime.len(),
        raw_deploy,
        raw_runtime,
        n
    );
}

// ── KSR-CVN-013 + KSR-CVN-014 : constructor genesis-mint site ────────────────

#[test]
fn constructor_genesis_mint_staticcall_is_hardened() {
    let (deploy, _runtime) = compile(CONFIDENTIAL_TOKEN);
    let n = assert_all_staticcalls_are_hardened(&deploy, "deploy");
    assert!(
        n > 0,
        "`supply: N to deployer` in a confidential token must emit a \
         constructor-time FHE encrypt STATICCALL"
    );
}

// ── KSR-CVN-014 : return slot zeroing ────────────────────────────────────────

#[test]
fn return_slot_is_zeroed_before_runtime_staticcall() {
    // The hardened emit_precompile_call begins with PUSH0; PUSH0; MSTORE —
    // writing zero to mem[0x00] so a stale value cannot be MLOAD'd as the
    // "verified" result if the precompile returns empty data.
    let (_deploy, runtime) = compile(CONFIDENTIAL_TOKEN);
    let canary = [OP_PUSH0, OP_PUSH0, OP_MSTORE];
    let mut found = false;
    let mut i = 0;
    while i + 3 <= runtime.len() {
        if runtime[i..i + 3] == canary {
            found = true;
            break;
        }
        i += 1;
    }
    assert!(
        found,
        "runtime must contain the PUSH0 PUSH0 MSTORE canary (zero mem[0x00]) \
         before STATICCALL (KSR-CVN-014)"
    );
}

// ── KSR-CVN-013 : POP is never the first byte after STATICCALL ───────────────

#[test]
fn no_staticcall_followed_immediately_by_pop() {
    // Defensive structural check for the exact anti-pattern the audit called
    // out: `STATICCALL ; POP ; ...` in codegen.rs:1188 / :221 / :742.
    let (deploy, runtime) = compile(CONFIDENTIAL_TOKEN);
    for (name, code) in [("deploy", &deploy), ("runtime", &runtime)] {
        for off in staticcall_offsets(code) {
            let next = code.get(off + 1).copied().unwrap_or(0);
            assert_ne!(
                next, OP_POP,
                "{name} STATICCALL at 0x{off:x} is immediately followed by POP — \
                 success flag is being discarded (KSR-CVN-013)"
            );
        }
    }
}

// ── Structural counter: ensure the `EQ` comparator appears too ───────────────

#[test]
fn returndatasize_is_compared_before_mload() {
    // The hardened emitter pushes the expected size (32) after RETURNDATASIZE
    // and uses EQ → ISZERO → JUMPI. Confirm at least one EQ exists somewhere
    // following a RETURNDATASIZE (loose check — rules out "read but ignore").
    let (_deploy, runtime) = compile(CONFIDENTIAL_TOKEN);
    // Find first RETURNDATASIZE, then scan ahead for EQ or ISZERO within 16 bytes.
    let mut idx = 0;
    let mut found_any = false;
    while idx < runtime.len() {
        let op = runtime[idx];
        if op == OP_RETURNDATASIZE {
            let end = (idx + 1 + 16).min(runtime.len());
            let ahead = &runtime[idx + 1..end];
            if ahead.contains(&OP_EQ) || ahead.contains(&OP_ISZERO) {
                found_any = true;
                break;
            }
        }
        // Skip push payloads to avoid data-byte false positives.
        if (0x60..=0x7f).contains(&op) {
            let n = (op - 0x60 + 1) as usize;
            idx += 1 + n;
        } else {
            idx += 1;
        }
    }
    assert!(
        found_any,
        "RETURNDATASIZE must be compared (EQ or ISZERO) within 16 bytes — \
         otherwise the read value is discarded and KSR-CVN-014 still applies"
    );
}

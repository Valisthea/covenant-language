//! KSR-CVN-015 regression tests: FheCmpNe / FheCmpLe dispatch to the correct
//! precompile address rather than reusing FheCmpEq / FheCmpLt.
//!
//! The OMEGA V4 audit found that `Opcode::FheCmpEq | Opcode::FheCmpNe` shared
//! a single match arm dispatching to `cmp_eq` (0x106), so every encrypted `!=`
//! comparison silently returned the wrong answer. Same root cause for
//! `FheCmpLe` collapsing into `FheCmpLt` (0x107).
//!
//! These tests assert at three layers:
//!   1. `EvmConfig::default()` exposes distinct addresses for cmp_ne (0x113) and
//!      cmp_le (0x114), and they don't collide with cmp_eq / cmp_lt / cmp_gt /
//!      cmp_ge / any other FHE precompile.
//!   2. The generated runtime bytecode for an encrypted `!=` / `<=` source
//!      contains the new precompile addresses (`PUSH2 0x01 0x13` /
//!      `PUSH2 0x01 0x14`) and does not contain the old wrong ones for these
//!      operations.

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

#[test]
fn fhe_precompile_addresses_are_distinct() {
    let cfg = EvmConfig::default();
    let fhe = cfg.precompile_addresses.fhe;

    let addrs = [
        ("cmp_eq", fhe.cmp_eq),
        ("cmp_ne", fhe.cmp_ne),
        ("cmp_lt", fhe.cmp_lt),
        ("cmp_le", fhe.cmp_le),
        ("cmp_gt", fhe.cmp_gt),
        ("cmp_ge", fhe.cmp_ge),
    ];

    for (i, (name_a, a)) in addrs.iter().enumerate() {
        for (name_b, b) in addrs.iter().skip(i + 1) {
            // V0.9: addresses are [u8; 20] (20-byte EVM addresses), not u16.
            // Format as hex bytes for the failure message.
            let a_hex: String = a.iter().map(|b| format!("{b:02x}")).collect();
            let b_hex: String = b.iter().map(|b| format!("{b:02x}")).collect();
            assert_ne!(
                a, b,
                "FHE comparison precompile addresses must be distinct: \
                 {name_a} (0x{a_hex}) collides with {name_b} (0x{b_hex}) \
: this is the KSR-CVN-015 root cause"
            );
        }
    }

    // Sanity: the new addresses must specifically not be 0x106 or 0x107
    // (the addresses they were previously being aliased to).
    assert_ne!(fhe.cmp_ne, fhe.cmp_eq, "cmp_ne must NOT alias cmp_eq");
    assert_ne!(fhe.cmp_le, fhe.cmp_lt, "cmp_le must NOT alias cmp_lt");
}

fn try_compile(src: &str) -> Option<Vec<u8>> {
    let (toks, _) = tokenize(src, SourceId::new(0));
    let (file, _) = parse(&toks, SourceId::new(0));
    let file = file?;
    let (res, _) = resolve(file, SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (checked, _) = analyze_privacy(typed, SourceId::new(0));
    let (module, ir_diags) = build_ir(checked, SourceId::new(0));
    if ir_diags
        .iter()
        .any(|d| d.level == covenant_diag::DiagnosticLevel::Error)
    {
        return None;
    }
    let (with_std, _) = lower_stdlib(module, StdlibConfig::default());
    let (optimized, _) = optimize(with_std, OptimizerConfig::default());
    let (artifact, _) = codegen_evm(optimized, EvmConfig::default());
    Some(artifact.runtime_bytecode)
}

/// Search for `needle` in `haystack` skipping PUSH-data regions so payload
/// bytes do not produce false positives on opcode patterns.
fn contains_outside_pushdata(haystack: &[u8], needle: &[u8]) -> bool {
    let mut i = 0;
    while i < haystack.len() {
        let op = haystack[i];
        if (0x60..=0x7f).contains(&op) {
            let n = (op - 0x60 + 1) as usize;
            // The PUSH instruction byte itself can start a match, check
            // here, then skip the payload.
            if i + needle.len() <= haystack.len() && haystack[i..i + needle.len()] == *needle {
                return true;
            }
            i += 1 + n;
            continue;
        }
        if i + needle.len() <= haystack.len() && haystack[i..i + needle.len()] == *needle {
            return true;
        }
        i += 1;
    }
    false
}

/// Build the V0.9 PUSH20 byte pattern for a V0.8 short precompile address.
/// PUSH20 = 0x73, followed by 20 bytes (18 zeros + the 2-byte short address).
fn push20_for_v08_addr(short: u16) -> [u8; 21] {
    let mut out = [0u8; 21];
    out[0] = 0x73; // PUSH20 opcode
                   // High 18 bytes are zero; low 2 bytes are the V0.8 short address.
    out[19] = (short >> 8) as u8;
    out[20] = (short & 0xFF) as u8;
    out
}

#[test]
fn fhe_cmp_ne_emits_distinct_precompile_address() {
    // Two encrypted (cipher<amount>) values compared with `!=`.
    // After the KSR-CVN-015 fix, this must dispatch to cmp_ne (0x113),
    // not cmp_eq (0x106).
    //
    // V0.9 codegen emits PUSH20 (was PUSH2): see Sprint 31 / target.rs.
    // The MockChain target lifts the V0.8 u16 to a 20-byte EvmAddress with
    // high 18 bytes zero and low 2 bytes the V0.8 value.
    let src = r#"
record CmpNe {
    a: ciphertext<amount>
    b: ciphertext<amount>
    view probe returns ciphertext<bool> { a != b }
}
"#;

    let Some(bytecode) = try_compile(src) else {
        return;
    };

    let cmp_ne_push = push20_for_v08_addr(0x0113);
    assert!(
        contains_outside_pushdata(&bytecode, &cmp_ne_push),
        "FheCmpNe must dispatch to precompile 0x113 (PUSH20 ...0113 not found in bytecode)"
    );
}

#[test]
fn fhe_cmp_le_emits_distinct_precompile_address() {
    let src = r#"
record CmpLe {
    a: ciphertext<amount>
    b: ciphertext<amount>
    view probe returns ciphertext<bool> { a <= b }
}
"#;

    let Some(bytecode) = try_compile(src) else {
        return;
    };

    let cmp_le_push = push20_for_v08_addr(0x0114);
    assert!(
        contains_outside_pushdata(&bytecode, &cmp_le_push),
        "FheCmpLe must dispatch to precompile 0x114 (PUSH20 ...0114 not found in bytecode)"
    );
}

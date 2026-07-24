//! F07 / F08 regression — `reveal <field> to <target>` access control + ABI.
//!
//! F07 (CRITICAL): `reveal total to owner` used to compile with ZERO caller
//! check — the owner-only disclosure restriction was silently dropped at IR
//! lowering (`lower_reveal` ignored the `to <target>` clause). The fix lowers
//! the target into the SAME authorization assertion `only <principal>` guards
//! use, so a non-owner call reverts before anything is disclosed.
//!
//! F08 (MED): the reveal's ABI entry advertised `outputs:[]` +
//! `stateMutability:"nonpayable"` while the runtime is read-only and RETURNs
//! 32 bytes. The fix emits `view` + the real (decrypted) output type.

use covenant_diag::SourceId;
use covenant_evm_backend::{codegen_evm, EvmArtifact, EvmConfig};
use covenant_ir::build_ir;
use covenant_lexer::tokenize;
use covenant_opt::{optimize, OptimizerConfig};
use covenant_parser::parse;
use covenant_privacy::analyze_privacy;
use covenant_resolver::resolve;
use covenant_stdlib::{lower_stdlib, StdlibConfig};
use covenant_types::typecheck;

const OP_CALLER: u8 = 0x33;
const OP_EQ: u8 = 0x14;

fn compile(src: &str) -> EvmArtifact {
    let (toks, _) = tokenize(src, SourceId::new(0));
    let (file, _) = parse(&toks, SourceId::new(0));
    let (res, _) = resolve(file.unwrap(), SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (checked, _) = analyze_privacy(typed, SourceId::new(0));
    let (module, ir_diags) = build_ir(checked, SourceId::new(0));
    assert!(
        !ir_diags
            .iter()
            .any(|d| d.level == covenant_diag::DiagnosticLevel::Error),
        "IR build produced errors: {ir_diags:?}"
    );
    let (with_std, _) = lower_stdlib(module, StdlibConfig::default());
    let (optimized, _) = optimize(with_std, OptimizerConfig::default());
    let (artifact, _) = codegen_evm(optimized, EvmConfig::default());
    artifact
}

/// Count occurrences of a single opcode, skipping PUSH data regions so a
/// PUSH payload byte can't masquerade as an opcode.
fn count_op(code: &[u8], needle: u8) -> usize {
    let mut n = 0;
    let mut i = 0;
    while i < code.len() {
        let op = code[i];
        if (0x60..=0x7f).contains(&op) {
            i += 1 + (op - 0x60 + 1) as usize;
            continue;
        }
        if op == needle {
            n += 1;
        }
        i += 1;
    }
    n
}

// ── F07: `reveal ... to owner` emits a real caller check ────────────────────

#[test]
fn reveal_to_owner_emits_caller_gate() {
    // No `owner` field → the owner of the construct is the deployer, so the
    // gate must load the deployer and compare it against the caller.
    let to_owner = r#"
encrypted counter Shielded {
    total: amount
    action bump(by: amount) { total += by }
    reveal total to owner
}
"#;
    // `to caller` is a public reveal (caller == caller) — no gate. Baseline.
    let to_caller = r#"
encrypted counter Shielded {
    total: amount
    action bump(by: amount) { total += by }
    reveal total to caller
}
"#;
    let rt_owner = compile(to_owner).runtime_bytecode;
    let rt_caller = compile(to_caller).runtime_bytecode;

    // The owner-gated reveal must add a CALLER and an EQ that the public reveal
    // does not. (If the `to <target>` clause were dropped again, both counts
    // would be equal — the negative control.)
    assert!(
        count_op(&rt_owner, OP_CALLER) > count_op(&rt_caller, OP_CALLER),
        "`reveal ... to owner` must emit a CALLER check the public reveal does not: \
         owner={}, caller={}",
        count_op(&rt_owner, OP_CALLER),
        count_op(&rt_caller, OP_CALLER),
    );
    assert!(
        count_op(&rt_owner, OP_EQ) > count_op(&rt_caller, OP_EQ),
        "`reveal ... to owner` must emit an EQ (caller vs owner) the public reveal does not"
    );
}

#[test]
fn reveal_to_owner_with_owner_field_loads_the_field() {
    // With an explicit `owner` field the gate must SLOAD it (and still CALLER+EQ).
    let src = r#"
record Vault {
    field owner: address
    field secret: amount
    action init(who: address) { owner = who }
    reveal secret to owner
}
"#;
    let rt = compile(src).runtime_bytecode;
    assert!(
        count_op(&rt, OP_CALLER) >= 1 && count_op(&rt, OP_EQ) >= 1,
        "owner-field reveal must emit CALLER + EQ"
    );
}

// ── F08: reveal ABI is `view` with the real output type ─────────────────────

#[test]
fn reveal_abi_is_view_with_output_type() {
    let src = r#"
encrypted counter Shielded {
    total: amount
    action bump(by: amount) { total += by }
    reveal total to owner
}
"#;
    let abi = compile(src).abi;

    // Locate the `total` function entry.
    let entry = abi
        .split("{\"name\":\"total\"")
        .nth(1)
        .expect("ABI must contain the `total` reveal function");
    let entry = &entry[..entry.find('}').map(|_| entry.len()).unwrap_or(entry.len())];

    assert!(
        entry.contains("\"stateMutability\":\"view\""),
        "reveal `total` must be `view`, not nonpayable. ABI: {abi}"
    );
    assert!(
        entry.contains("\"outputs\":[{\"name\":\"\",\"type\":\"uint256\"}]"),
        "reveal `total` must advertise its real 32-byte output (uint256), not []. ABI: {abi}"
    );
}

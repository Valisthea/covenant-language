//! KSR-CVN-011 regression tests: `only(principal)` guard lowering.
//!
//! Verifies that every `action f() only <principal>` lowers to real
//! authorization bytecode rather than the pre-fix `Assert(true)` no-op.
//!
//! Attack documented in OMEGA V4: a Covenant contract declaring
//! `action mint() only owner` silently compiled to bytecode that never
//! checked the caller. Any address could invoke the action.
//!
//! These tests assert the bytecode-level signature of the emitted guard
//! for several principal shapes:
//!
//!   - `only owner` / `only admin` (field-backed)  → SLOAD + CALLER + EQ + ISZERO + JUMPI __revert__
//!   - `only deployer`                             → PUSH32 deployer_slot + SLOAD + CALLER + EQ + ISZERO + JUMPI
//!   - `only caller`                               → no extra check emitted (trivially true)
//!   - `only owner` with no `field owner`          → Assert(false) / fail-closed body

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

const OP_CALLER: u8 = 0x33;
const OP_EQ: u8 = 0x14;
const OP_ISZERO: u8 = 0x15;
const OP_JUMPI: u8 = 0x57;
const OP_SLOAD: u8 = 0x54;
const OP_REVERT: u8 = 0xfd;
const OP_PUSH0: u8 = 0x5f;

fn compile(src: &str) -> Vec<u8> {
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
    artifact.runtime_bytecode
}

/// Scan bytecode and return offsets where `needle` appears, skipping PUSH-data
/// regions so a PUSH32 payload byte can't masquerade as an opcode hit.
fn find_pattern(code: &[u8], needle: &[u8]) -> Vec<usize> {
    let mut hits = Vec::new();
    let mut i = 0;
    while i < code.len() {
        let op = code[i];
        if (0x60..=0x7f).contains(&op) {
            let n = (op - 0x60 + 1) as usize;
            if i + 1 + n > code.len() {
                break;
            }
            i += 1 + n;
            continue;
        }
        if i + needle.len() <= code.len() && code[i..i + needle.len()] == *needle {
            hits.push(i);
        }
        i += 1;
    }
    hits
}

// ── `only owner` with field owner: address ──────────────────────────────────

#[test]
fn only_owner_emits_sload_caller_eq_iszero_jumpi() {
    // Build a no-guard baseline and compare against the guarded version.
    let with_guard = r#"
module G {
    field owner: address
    field x: amount

    action init(who: address) { owner = who }
    action set(v: amount) only owner { x = v }
}
"#;
    let without_guard = r#"
module G {
    field owner: address
    field x: amount

    action init(who: address) { owner = who }
    action set(v: amount) only caller { x = v }
}
"#;
    let rt_guarded = compile(with_guard);
    let rt_plain = compile(without_guard);

    // The guard emits a new SLOAD (owner), a new CALLER, a new EQ, a new
    // ISZERO and a new JUMPI target. Each of these must appear at strictly
    // higher frequency than in the baseline (`only caller`).
    let sload_guarded = find_pattern(&rt_guarded, &[OP_SLOAD]).len();
    let sload_plain = find_pattern(&rt_plain, &[OP_SLOAD]).len();
    let caller_guarded = find_pattern(&rt_guarded, &[OP_CALLER]).len();
    let caller_plain = find_pattern(&rt_plain, &[OP_CALLER]).len();
    let eq_guarded = find_pattern(&rt_guarded, &[OP_EQ]).len();
    let eq_plain = find_pattern(&rt_plain, &[OP_EQ]).len();

    assert!(
        sload_guarded > sload_plain,
        "`only owner` must add an SLOAD (owner-field load); got guarded={sload_guarded}, \
         plain={sload_plain}"
    );
    assert!(
        caller_guarded > caller_plain,
        "`only owner` must add a CALLER op; got guarded={caller_guarded}, plain={caller_plain}"
    );
    assert!(
        eq_guarded > eq_plain,
        "`only owner` must add an EQ op (caller vs owner); got guarded={eq_guarded}, \
         plain={eq_plain}"
    );

    // Guard must route a failed check to the __revert__ label → ISZERO+JUMPI.
    assert!(
        find_pattern(&rt_guarded, &[OP_ISZERO]).len() > find_pattern(&rt_plain, &[OP_ISZERO]).len(),
        "`only owner` must add an ISZERO (Assert lowering)"
    );
    assert!(
        find_pattern(&rt_guarded, &[OP_JUMPI]).len() > find_pattern(&rt_plain, &[OP_JUMPI]).len(),
        "`only owner` must add a JUMPI (revert branch)"
    );
}

// ── `only admin` with field admin: address ──────────────────────────────────

#[test]
fn only_admin_emits_authorization_check() {
    let src = r#"
module G {
    field admin: address
    field x: amount

    action init(who: address) { admin = who }
    action set(v: amount) only admin { x = v }
}
"#;
    let runtime = compile(src);

    // Same pattern as owner: SLOAD, CALLER, EQ, Assert.
    let has_sload = runtime.contains(&OP_SLOAD);
    let has_caller = runtime.contains(&OP_CALLER);
    let has_eq = runtime.contains(&OP_EQ);
    assert!(has_sload, "runtime must contain SLOAD for admin field");
    assert!(has_caller, "runtime must contain CALLER opcode");
    assert!(has_eq, "runtime must contain EQ to compare admin == caller");

    // And at least one revert target referenced by a JUMPI.
    assert!(
        runtime.contains(&OP_JUMPI),
        "runtime must contain a JUMPI (conditional branch to revert)"
    );
}

// ── `only caller` is a no-op check ──────────────────────────────────────────

#[test]
fn only_caller_does_not_emit_extra_check() {
    let src_with = r#"
module G {
    field x: amount
    action set(v: amount) only caller { x = v }
}
"#;
    let src_without = r#"
module G {
    field x: amount
    action set(v: amount) { x = v }
}
"#;
    let rt_with = compile(src_with);
    let rt_without = compile(src_without);

    // `only caller` is trivially true (caller == caller). It MAY add metadata
    // but MUST NOT add extra CALLER+EQ+JUMPI checks relative to no guard.
    //
    // Count EQ opcodes outside PUSH payloads in both, the counts should match.
    let eq_with = find_pattern(&rt_with, &[OP_EQ]).len();
    let eq_without = find_pattern(&rt_without, &[OP_EQ]).len();
    assert_eq!(
        eq_with, eq_without,
        "`only caller` must not emit extra EQ checks: with={eq_with}, without={eq_without}"
    );
}

// ── unresolved principal fails closed (no `field owner` declared) ───────────

#[test]
fn only_owner_without_field_owner_fails_closed() {
    // Compile succeeds (warning-level diagnostic) but the emitted action must
    // revert on every call rather than silently allowing access.
    let src = r#"
module G {
    field x: amount
    action set(v: amount) only owner { x = v }
}
"#;
    let (toks, _) = tokenize(src, SourceId::new(0));
    let (file, _) = parse(&toks, SourceId::new(0));
    let (res, _) = resolve(file.unwrap(), SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (checked, _) = analyze_privacy(typed, SourceId::new(0));
    let (module, ir_diags) = build_ir(checked, SourceId::new(0));

    // KSR-CVN-011 warning must be emitted.
    assert!(
        ir_diags
            .iter()
            .any(|d| d.code == covenant_ir::diag::E421_GUARD_UNRESOLVED_PRINCIPAL),
        "expected E421 diagnostic for `only owner` without `field owner`, got: {ir_diags:?}"
    );

    // Compilation continues through to bytecode despite the warning.
    let (with_std, _) = lower_stdlib(module, StdlibConfig::default());
    let (optimized, _) = optimize(with_std, OptimizerConfig::default());
    let (artifact, _) = codegen_evm(optimized, EvmConfig::default());
    let runtime = artifact.runtime_bytecode;

    // Emitted bytecode must contain a PUSH0 PUSH0 REVERT sequence that the
    // Assert(false) → ISZERO→JUMPI(__revert__) path falls into. We at minimum
    // expect REVERT to be reachable.
    assert!(
        runtime.contains(&OP_REVERT),
        "fail-closed guard must leave a reachable REVERT in the runtime"
    );
    // And at least one ISZERO + JUMPI pair (the Assert lowering).
    assert!(
        runtime.contains(&OP_ISZERO) && runtime.contains(&OP_JUMPI),
        "Assert(false) must lower to ISZERO + JUMPI + revert"
    );
    // PUSH0 should appear (as the revert arg): Covenant uses the post-Shanghai PUSH0.
    assert!(
        runtime.contains(&OP_PUSH0),
        "PUSH0 must appear (constant 0 / revert-data offsets)"
    );
}

// ── Two `only owner` actions both emit their own guard ──────────────────────

#[test]
fn multiple_only_owner_actions_each_get_a_guard() {
    let src = r#"
module G {
    field owner: address
    field a: amount
    field b: amount

    action init(who: address) { owner = who }
    action set_a(v: amount) only owner { a = v }
    action set_b(v: amount) only owner { b = v }
}
"#;
    let rt_guarded = compile(src);

    // Baseline: same module but with `only caller` instead of `only owner` on
    // each setter. The delta in SLOAD/CALLER/EQ counts must reflect two guards.
    let baseline = r#"
module G {
    field owner: address
    field a: amount
    field b: amount

    action init(who: address) { owner = who }
    action set_a(v: amount) only caller { a = v }
    action set_b(v: amount) only caller { b = v }
}
"#;
    let rt_baseline = compile(baseline);

    let sload_delta = find_pattern(&rt_guarded, &[OP_SLOAD]).len()
        - find_pattern(&rt_baseline, &[OP_SLOAD]).len();
    let caller_delta = find_pattern(&rt_guarded, &[OP_CALLER]).len()
        - find_pattern(&rt_baseline, &[OP_CALLER]).len();
    let eq_delta =
        find_pattern(&rt_guarded, &[OP_EQ]).len() - find_pattern(&rt_baseline, &[OP_EQ]).len();

    assert!(
        sload_delta >= 2,
        "two `only owner` actions must add ≥2 SLOADs; got delta={sload_delta}"
    );
    assert!(
        caller_delta >= 2,
        "two `only owner` actions must add ≥2 CALLER ops; got delta={caller_delta}"
    );
    assert!(
        eq_delta >= 2,
        "two `only owner` actions must add ≥2 EQ ops; got delta={eq_delta}"
    );
}

// ── `only address(expr)` compares caller to an address expression ──────────

#[test]
fn only_address_expr_emits_caller_eq_expr_check() {
    // `only 0x1111...1111`: a literal address. The guard must compare caller
    // to the literal rather than no-op.
    let src = r#"
module G {
    field x: amount
    action set(v: amount) only 0x1111111111111111111111111111111111111111 { x = v }
}
"#;
    let runtime = compile(src);
    assert!(
        runtime.contains(&OP_CALLER) && runtime.contains(&OP_EQ),
        "`only <address>` must emit CALLER + EQ check"
    );
    assert!(
        runtime.contains(&OP_ISZERO) && runtime.contains(&OP_JUMPI),
        "check result must feed ISZERO + JUMPI (Assert lowering)"
    );
}

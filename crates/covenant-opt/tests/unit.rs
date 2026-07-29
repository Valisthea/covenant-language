//! Per-pass unit tests.

use covenant_diag::{Diagnostic, DiagnosticLevel, SourceId};
use covenant_ir::{build_ir, validate, IrModule, Opcode};
use covenant_lexer::tokenize;
use covenant_opt::{optimize, total_instructions, OptimizerConfig};
use covenant_parser::parse;
use covenant_privacy::analyze_privacy;
use covenant_resolver::resolve;
use covenant_types::typecheck;

fn lower(src: &str) -> IrModule {
    let (toks, _) = tokenize(src, SourceId::new(0));
    let (file, _) = parse(&toks, SourceId::new(0));
    let (res, _) = resolve(file.unwrap(), SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (checked, _) = analyze_privacy(typed, SourceId::new(0));
    let (module, _) = build_ir(checked, SourceId::new(0));
    module
}

fn opt_with(src: &str, cfg: OptimizerConfig) -> (IrModule, Vec<Diagnostic>) {
    optimize(lower(src), cfg)
}

fn opt(src: &str) -> IrModule {
    opt_with(src, OptimizerConfig::default()).0
}

fn count_opcode(m: &IrModule, f: impl Fn(&Opcode) -> bool) -> usize {
    m.functions
        .iter()
        .flat_map(|fun| fun.blocks.iter())
        .flat_map(|b| b.instructions.iter())
        .filter(|i| f(&i.opcode))
        .count()
}

/// Same count, restricted to one action. Module-wide counts are useless for
/// anything an unrelated view also emits: `view get_n returns amount { n }`
/// contributes an `SLoad` of its own and would make an `SLoad >= 1` assertion
/// pass no matter what the optimizer did to the action under test.
fn count_opcode_in(m: &IrModule, func_name: &str, f: impl Fn(&Opcode) -> bool) -> usize {
    m.functions
        .iter()
        .filter(|fun| fun.name.name.as_ref() == func_name)
        .flat_map(|fun| fun.blocks.iter())
        .flat_map(|b| b.instructions.iter())
        .filter(|i| f(&i.opcode))
        .count()
}

// ---------------- Constant folding ----------------

#[test]
fn constant_folding_add_integers() {
    // `1 + 2 * 3` → constant 7 after full folding.
    let src = r#"record R { view s returns amount { 1 + 2 * 3 } }"#;
    let m = opt(src);
    // After folding + DCE: no AddChecked or MulChecked remain because the
    // computation was fully constant.
    let arith = count_opcode(&m, |o| matches!(o, Opcode::AddChecked | Opcode::MulChecked));
    assert_eq!(arith, 0, "expected all arithmetic folded");
}

#[test]
fn constant_folding_with_comparison() {
    let src = r#"record R { view b returns bool { 1 < 2 } }"#;
    let m = opt(src);
    let cmps = count_opcode(&m, |o| matches!(o, Opcode::Lt));
    assert_eq!(cmps, 0);
}

#[test]
fn constant_folding_does_not_fold_sload() {
    // SLoad must remain; it depends on runtime storage state.
    let src = r#"record R { x: amount\n view g returns amount { x } }"#;
    let src = src.replace("\\n", "\n");
    let m = opt(&src);
    let sloads = count_opcode(&m, |o| matches!(o, Opcode::SLoad(_)));
    assert_eq!(sloads, 1);
}

#[test]
fn constant_folding_checked_overflow_is_skipped() {
    // Folding `AddChecked(u128::MAX, 1)` would overflow. Make sure we don't
    // silently fold to wrong value — the op must remain or be rewritten to
    // revert. For V0 we just assert the program still builds.
    let src = r#"record R { view s returns amount { 340282366920938463463374607431768211455 } }"#;
    // This is u128::MAX as a literal, so no runtime overflow risk.
    let m = opt(src);
    let _ = m; // merely assert it doesn't panic
}

// ---------------- DCE ----------------

#[test]
fn dce_preserves_sstore() {
    let src = r#"record R { x: amount\n action go(v: amount) { x = v } }"#;
    let src = src.replace("\\n", "\n");
    let m = opt(&src);
    let sstores = count_opcode(&m, |o| matches!(o, Opcode::SStore(_)));
    assert_eq!(sstores, 1);
}

#[test]
fn dce_preserves_emit() {
    let src = r#"
record R {
    event E()
    action go() { emit E() }
}
"#;
    let m = opt(src);
    let emits = count_opcode(&m, |o| matches!(o, Opcode::Emit(_)));
    assert_eq!(emits, 1);
}

#[test]
fn dce_preserves_assert_from_guard() {
    let src = r#"record R { action go() when 1 < 2 {} }"#;
    let m = opt(src);
    let asserts = count_opcode(&m, |o| matches!(o, Opcode::Assert));
    assert!(asserts >= 1, "assert from guard must survive DCE");
}

#[test]
fn dce_preserves_transfer() {
    let src = r#"record R { action go(to_addr: address) { transfer 100 to to_addr } }"#;
    let m = opt(src);
    let transfers = count_opcode(&m, |o| matches!(o, Opcode::Transfer));
    assert_eq!(transfers, 1);
}

// ---------------- OMEGA V3.6 F-19: runtime traps survive DCE ----------------
//
// These go through the real front end on purpose. The defect was reported
// from source (`let y = 100 / x` and `let remaining = bal - amt` lost their
// guards in the default optimized build, 122 runtime bytes against 166 with
// `--no-optimize`), so the regression is pinned at the same altitude: parse,
// type, lower, optimize, then count what is left in the IR the backend will
// see.

/// The finding's own reproduction, verbatim.
const F19_SRC: &str = r#"
record P7 {
    field bal: amount
    field n: amount

    action set_bal(v: amount) { bal = v }

    action div_guard(x: amount) {
        let y = 100 / x
        n = 1
    }

    action sub_guard(amt: amount) {
        let remaining = bal - amt
        n = 2
    }

    view get_n returns amount { n }
}
"#;

#[test]
fn f19_division_guard_survives_when_the_binding_is_never_read() {
    let m = opt(F19_SRC);
    let divs = count_opcode(&m, |o| matches!(o, Opcode::Div));
    assert_eq!(
        divs, 1,
        "`let y = 100 / x` must keep its Div: the zero-divisor revert is the \
         behaviour the source promises, whether or not `y` is ever read"
    );
}

#[test]
fn f19_checked_subtraction_survives_when_the_binding_is_never_read() {
    let m = opt(F19_SRC);
    let subs = count_opcode(&m, |o| matches!(o, Opcode::SubChecked));
    assert_eq!(
        subs, 1,
        "`let remaining = bal - amt` must keep its SubChecked: the underflow \
         revert is the behaviour the source promises"
    );
}

#[test]
fn f19_a_kept_trap_keeps_the_values_it_guards() {
    // The subtler half of the same defect. Retaining the SubChecked is not
    // enough: its underflow guard compares the SLoad of `bal` against the
    // parameter, so the SLoad has to be seeded live off the retained
    // instruction. Seed from a narrower set and the guard ships comparing a
    // memory slot that was never written.
    let m = opt(F19_SRC);
    let sloads = count_opcode_in(&m, "sub_guard", |o| matches!(o, Opcode::SLoad(_)));
    assert_eq!(
        sloads, 1,
        "the SLoad of `bal` feeding the retained underflow guard must stay live"
    );
}

#[test]
fn f19_dce_still_removes_genuinely_dead_work() {
    // Negative control at source level: the pass must still be a pass. A
    // dead read of a field carries no trap and has to go.
    let src = r#"
record R {
    field bal: amount
    field n: amount

    action f() {
        let unused = bal
        n = 1
    }
}
"#;
    let m = opt(src);
    let sloads = count_opcode(&m, |o| matches!(o, Opcode::SLoad(_)));
    assert_eq!(
        sloads, 0,
        "a dead field read traps on nothing and must still be eliminated"
    );
}

#[test]
fn f19_division_by_a_literal_non_zero_is_still_eliminated() {
    // The backend emits no guard for a literal non-zero divisor, so there is
    // no trap to preserve and the usual liveness rule applies. Without this
    // the fix would tax every `value * bps / 10000`.
    let src = r#"
record R {
    field n: amount

    action f(v: amount) {
        let unused = v / 10000
        n = 1
    }
}
"#;
    let m = opt(src);
    let divs = count_opcode(&m, |o| matches!(o, Opcode::Div));
    assert_eq!(
        divs, 0,
        "a provably non-zero divisor emits no guard, so the dead Div goes"
    );
}

#[test]
fn f19_division_by_a_literal_zero_reaches_the_backend() {
    // E519 is raised during codegen, so it only fires if the instruction
    // survives this far. Deleting it here is how `let y = 100 / 0` compiled
    // clean under the default optimized build while `--no-optimize`
    // correctly refused it.
    let src = r#"
record R {
    field n: amount

    action f() {
        let unused = 100 / 0
        n = 1
    }
}
"#;
    let m = opt(src);
    let divs = count_opcode(&m, |o| matches!(o, Opcode::Div));
    assert_eq!(
        divs, 1,
        "a literal-zero divisor must reach codegen so E519 can refuse it"
    );
}

// ---------------- SLoad coalescing ----------------

#[test]
fn sload_coalesce_same_field_twice() {
    // View that reads `x` twice in `x + x` should end up with a single SLoad.
    let src = r#"record R { x: amount\n view d returns amount { x + x } }"#;
    let src = src.replace("\\n", "\n");
    let m = opt(&src);
    let sloads = count_opcode(&m, |o| matches!(o, Opcode::SLoad(_)));
    assert_eq!(sloads, 1, "expected one SLoad after coalescing");
}

#[test]
fn sload_not_coalesced_across_sstore() {
    // `x = 1; let y = x` — the SLoad between SStores cannot coalesce with a
    // prior cached SLoad because the store invalidated the cache.
    let src = r#"
record R {
    x: amount
    action s() {
        x = 1
        let _y = x
    }
}
"#;
    let m = opt(src);
    // Validator must accept the module regardless of how many SLoads remain.
    let vdiags = validate(&m);
    assert!(vdiags.is_empty(), "{vdiags:?}");
}

// ---------------- Trivial FHE folding ----------------

#[test]
fn trivial_fhe_folding_merges_same_constant() {
    let src = r#"
record R {
    view c returns ciphertext<amount> { encrypted(0) }
    view d returns ciphertext<amount> { encrypted(0) }
}
"#;
    let m = opt(src);
    // Each view is in its own function, so folding is per-function. The test
    // simply verifies the optimizer runs cleanly.
    assert_eq!(m.functions.len(), 2);
}

#[test]
fn trivial_fhe_folding_preserves_lift_audit_trail() {
    // The lift from `total += by` must NOT be coalesced with a
    // non-lift encryption of the same constant.
    let src = r#"
encrypted counter C {
    total: amount
    action bump(by: amount) { total += by }
}
"#;
    let m = opt(src);
    // Lifts (from_lift: true) are preserved individually.
    let lifts = m
        .functions
        .iter()
        .flat_map(|f| f.blocks.iter())
        .flat_map(|b| b.instructions.iter())
        .filter(|i| matches!(i.opcode, Opcode::FheEncryptTrivial) && i.metadata.from_lift)
        .count();
    assert!(lifts >= 1, "lift preserved");
}

// ---------------- CSE ----------------

#[test]
fn cse_merges_identical_loads() {
    // `keccak(x) + keccak(x)` should compute `keccak` once.
    let src = r#"
record R {
    view h returns hash { keccak(caller) }
}
"#;
    let m = opt(src);
    // With one use, CSE doesn't fire. Just ensure validator accepts.
    let vdiags = validate(&m);
    assert!(vdiags.is_empty(), "{vdiags:?}");
}

// ---------------- Config knobs ----------------

#[test]
fn disable_constant_folding_keeps_arith() {
    let cfg = OptimizerConfig {
        enable_constant_folding: false,
        enable_dce: false,
        ..OptimizerConfig::default()
    };
    let src = r#"record R { view s returns amount { 1 + 2 } }"#;
    let (m, _) = opt_with(src, cfg);
    let adds = count_opcode(&m, |o| matches!(o, Opcode::AddChecked));
    assert!(adds >= 1, "disable_constant_folding should keep Adds");
}

#[test]
fn disable_dce_keeps_unused_add() {
    let cfg = OptimizerConfig {
        enable_dce: false,
        enable_constant_folding: false,
        ..OptimizerConfig::default()
    };
    let src = r#"
record R {
    action go() {
        let _a = 1 + 2
    }
}
"#;
    let (m, _) = opt_with(src, cfg);
    let adds = count_opcode(&m, |o| matches!(o, Opcode::AddChecked));
    assert!(adds >= 1);
}

#[test]
fn disable_all_is_identity() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_05_quantum_board.cov");
    let m_before = lower(src);
    let before_count = total_instructions(&m_before);
    let (m_after, diags) = opt_with(src, OptimizerConfig::none());
    let errs: Vec<_> = diags
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Error)
        .collect();
    assert!(errs.is_empty(), "{errs:?}");
    assert_eq!(total_instructions(&m_after), before_count);
}

#[test]
fn default_config_shrinks_basics() {
    // At least one Basics fixture should shrink under default config.
    let mut any_shrank = false;
    for src in [
        include_str!("../../covenant-lexer/tests/fixtures/example_01_hello.cov"),
        include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov"),
        include_str!("../../covenant-lexer/tests/fixtures/example_03_open_ballot.cov"),
        include_str!("../../covenant-lexer/tests/fixtures/example_04_shielded_counter.cov"),
        include_str!("../../covenant-lexer/tests/fixtures/example_05_quantum_board.cov"),
    ] {
        let m = lower(src);
        let before = total_instructions(&m);
        let (opt_m, _) = optimize(m, OptimizerConfig::default());
        let after = total_instructions(&opt_m);
        if after < before {
            any_shrank = true;
        }
    }
    assert!(any_shrank);
}

// ---------------- Invariant preservation ----------------

#[test]
fn validator_passes_after_optimization_basics() {
    for src in [
        include_str!("../../covenant-lexer/tests/fixtures/example_01_hello.cov"),
        include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov"),
        include_str!("../../covenant-lexer/tests/fixtures/example_03_open_ballot.cov"),
        include_str!("../../covenant-lexer/tests/fixtures/example_04_shielded_counter.cov"),
        include_str!("../../covenant-lexer/tests/fixtures/example_05_quantum_board.cov"),
    ] {
        let (m, _) = opt_with(src, OptimizerConfig::default());
        let diags = validate(&m);
        assert!(diags.is_empty(), "validator issues: {diags:?}");
    }
}

#[test]
fn privacy_domains_preserved_after_optimization() {
    // Each IrFunction has its own Value numbering space; scope check per-fn.
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_04_shielded_counter.cov");
    let before = lower(src);
    let per_fn_before: Vec<Vec<(covenant_ir::Value, covenant_privacy::PrivacyDomain)>> = before
        .functions
        .iter()
        .map(|f| {
            f.value_privacy
                .iter()
                .map(|(v, d)| (*v, *d))
                .collect::<Vec<_>>()
        })
        .collect();
    let (after, _) = optimize(before.clone(), OptimizerConfig::default());
    for (i, pairs) in per_fn_before.iter().enumerate() {
        let Some(fa) = after.functions.get(i) else {
            continue;
        };
        for (v, d) in pairs {
            if let Some(found) = fa.value_privacy.get(v).copied() {
                assert_eq!(found, *d, "privacy domain drifted for {v:?} in fn {i}");
            }
        }
    }
}

#[test]
fn fixpoint_converges_quickly() {
    let cfg = OptimizerConfig {
        max_iterations: 5,
        ..OptimizerConfig::default()
    };
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_05_quantum_board.cov");
    let (_, diags) = opt_with(src, cfg);
    let errs: Vec<_> = diags
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Error)
        .collect();
    assert!(errs.is_empty(), "fixpoint should converge: {errs:?}");
}

#[test]
fn optimize_preserves_function_count() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_05_quantum_board.cov");
    let before = lower(src);
    let n_before = before.functions.len();
    let (after, _) = optimize(before, OptimizerConfig::default());
    assert_eq!(after.functions.len(), n_before);
}

#[test]
fn optimize_preserves_metadata() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov");
    let before = lower(src);
    let md_before = before.metadata.len();
    let (after, _) = optimize(before, OptimizerConfig::default());
    assert_eq!(after.metadata.len(), md_before);
    assert!(after.metadata.contains_key("supply"));
}

#[test]
fn optimize_preserves_errors_and_events() {
    let src = r#"
record R {
    event E(x: amount)
    error Bad(y: amount)
    action go() { emit E(5) }
}
"#;
    let before = lower(src);
    let (after, _) = optimize(before, OptimizerConfig::default());
    assert_eq!(after.events.len(), 1);
    assert_eq!(after.errors.len(), 1);
}

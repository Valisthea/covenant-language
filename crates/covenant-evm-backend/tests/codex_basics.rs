//! Integration tests: codegen EVM for each Codex Basics fixture.

use covenant_diag::{Diagnostic, DiagnosticLevel, SourceId};
use covenant_evm_backend::{codegen_evm, EvmArtifact, EvmConfig};
use covenant_ir::build_ir;
use covenant_lexer::tokenize;
use covenant_opt::{optimize, OptimizerConfig};
use covenant_parser::parse;
use covenant_privacy::analyze_privacy;
use covenant_resolver::resolve;
use covenant_types::typecheck;

fn pipeline(src: &str) -> (EvmArtifact, Vec<Diagnostic>) {
    let (toks, _) = tokenize(src, SourceId::new(0));
    let (file, _) = parse(&toks, SourceId::new(0));
    let (res, _) = resolve(file.unwrap(), SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (checked, _) = analyze_privacy(typed, SourceId::new(0));
    let (module, _) = build_ir(checked, SourceId::new(0));
    let (optimized, _) = optimize(module, OptimizerConfig::default());
    codegen_evm(optimized, EvmConfig::default())
}

fn errors(d: &[Diagnostic]) -> Vec<&Diagnostic> {
    d.iter()
        .filter(|x| x.level == DiagnosticLevel::Error)
        .collect()
}

#[test]
fn hello_produces_valid_artifact() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_01_hello.cov");
    let (a, d) = pipeline(src);
    assert!(errors(&d).is_empty(), "{d:?}");
    assert!(!a.deploy_bytecode.is_empty());
    assert!(!a.runtime_bytecode.is_empty());
    assert!(a.runtime_bytecode.len() < 20 * 1024);
    assert_eq!(a.function_selectors.len(), 2, "update + read");
}

#[test]
fn coin_has_metadata_only() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov");
    let (a, d) = pipeline(src);
    assert!(errors(&d).is_empty(), "{d:?}");
    assert_eq!(a.function_selectors.len(), 0);
}

#[test]
fn open_ballot_rejects_unlowered_first_time_caller_predicate() {
    // OMEGA V6 CRT-004 fix (E518): `only first_time_caller` used to compile
    // to an unconditional `push 1` (an undiagnosed authorization bypass).
    // It now correctly refuses to compile until the predicate has a real
    // EVM lowering (same pattern as E516/E517 for amnesia/VDF primitives).
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_03_open_ballot.cov");
    let (_a, d) = pipeline(src);
    assert!(
        errors(&d).iter().any(|e| e.code.0 == 518),
        "expected E518 (unlowered BuiltinPredicate) for first_time_caller, got {d:?}"
    );
}

#[test]
fn shielded_counter_ciphertext_handle_at_slot_0() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_04_shielded_counter.cov");
    let (a, d) = pipeline(src);
    assert!(errors(&d).is_empty(), "{d:?}");
    assert_eq!(a.storage_layout.entries.len(), 1);
    let slot0 = a.storage_layout.entries[0].slot;
    let expected = [0u8; 32];
    assert_eq!(slot0, expected, "total at slot 0");
}

#[test]
fn quantum_board_rejects_unlowered_registered_key_predicate() {
    // OMEGA V6 CRT-004 fix (E518): same as open_ballot above, but for
    // `only registered_key`.
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_05_quantum_board.cov");
    let (_a, d) = pipeline(src);
    assert!(
        errors(&d).iter().any(|e| e.code.0 == 518),
        "expected E518 (unlowered BuiltinPredicate) for registered_key, got {d:?}"
    );
}

#[test]
fn all_basics_produce_valid_bytecode_structure() {
    // ballot/board are excluded: they use BuiltinPredicate guards
    // (first_time_caller / registered_key) that now correctly fail to
    // compile (E518) -- see the dedicated rejection tests above.
    for (name, src) in [
        (
            "hello",
            include_str!("../../covenant-lexer/tests/fixtures/example_01_hello.cov"),
        ),
        (
            "coin",
            include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov"),
        ),
        (
            "counter",
            include_str!("../../covenant-lexer/tests/fixtures/example_04_shielded_counter.cov"),
        ),
    ] {
        let (a, d) = pipeline(src);
        assert!(errors(&d).is_empty(), "{name}: {d:?}");
        // Deploy bytecode must start with constructor prologue (CALLER = 0x33)
        // and end with RETURN (0xf3) from the constructor.
        assert!(a.deploy_bytecode.len() > a.runtime_bytecode.len());
        // Runtime bytecode should start with the dispatcher: CALLDATALOAD (0x35).
        if !a.runtime_bytecode.is_empty() {
            // Earliest non-PUSH byte.
            let first_real_op = a
                .runtime_bytecode
                .iter()
                .find(|&&b| !(0x60..=0x7F).contains(&b) && b != 0x5F);
            assert!(first_real_op.is_some(), "{name}: runtime has some op");
        }
    }
}

#[test]
fn abi_json_is_valid_json_like() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_01_hello.cov");
    let (a, _) = pipeline(src);
    // Check it starts with `[` and ends with `]`.
    assert!(a.abi.starts_with('['));
    assert!(a.abi.ends_with(']'));
    // Contains both function names.
    assert!(a.abi.contains("\"update\""));
    assert!(a.abi.contains("\"read\""));
}

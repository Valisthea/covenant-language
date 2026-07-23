//! Integration tests for the `--target-chain aster` compilation path.
//!
//! Verifies that covenant-aster-backend produces valid V0.7 placeholder artifacts
//! for all Codex Basics fixtures, with correct magic bytes and metadata.

use covenant_aster_backend::{compile_module, ASTER_MAGIC};

const HELLO: &str = include_str!("../../covenant-lexer/tests/fixtures/example_01_hello.cov");
const COIN: &str = include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov");
const BALLOT: &str = include_str!("../../covenant-lexer/tests/fixtures/example_03_open_ballot.cov");

fn compile_to_aster(source: &str) -> covenant_aster_backend::AsterCompilationArtifact {
    use covenant_diag::SourceId;
    let module = covenant_driver::compile_to_ir(source, SourceId::new(0))
        .expect("IR compilation should succeed for valid source");
    compile_module(&module).expect("aster backend should produce artifact for valid IR")
}

#[test]
fn hello_produces_aster_artifact() {
    let artifact = compile_to_aster(HELLO);

    // Magic header must be present.
    assert!(
        artifact.bytecode.starts_with(ASTER_MAGIC),
        "bytecode must start with COV7 magic"
    );

    // Bytecode must be at least magic (5) + fn_count (4) bytes.
    assert!(
        artifact.bytecode.len() >= ASTER_MAGIC.len() + 4,
        "bytecode too short: {} bytes",
        artifact.bytecode.len()
    );

    // Metadata fields.
    assert_eq!(artifact.metadata.chain_id, 1996, "must target Aster Chain");
    assert_eq!(artifact.metadata.ir_version, 1);
    assert!(!artifact.metadata.sdk_lowering, "V0.7 has no SDK lowering");

    // Must emit the sdk-pending warning.
    assert!(
        artifact
            .warnings
            .iter()
            .any(|w| w.code == "aster-sdk-pending"),
        "expected aster-sdk-pending warning, got: {:?}",
        artifact.warnings
    );
}

#[test]
fn coin_produces_aster_artifact() {
    let artifact = compile_to_aster(COIN);
    assert!(artifact.bytecode.starts_with(ASTER_MAGIC));
    assert_eq!(artifact.metadata.chain_id, 1996);
}

#[test]
fn ballot_produces_aster_artifact() {
    let artifact = compile_to_aster(BALLOT);
    assert!(artifact.bytecode.starts_with(ASTER_MAGIC));
    assert_eq!(artifact.metadata.chain_id, 1996);
}

#[test]
fn function_count_encoded_in_bytecode() {
    let artifact = compile_to_aster(HELLO);
    // Bytes [5..9] are function count (little-endian u32).
    let fn_count = u32::from_le_bytes(artifact.bytecode[5..9].try_into().unwrap());
    // hello.cov has at least one function (update action).
    assert!(
        fn_count >= 1,
        "expected at least 1 function, got {fn_count}"
    );
}

#[test]
fn abi_field_is_present() {
    let artifact = compile_to_aster(HELLO);
    // V0.7 ABI is a serde_json::Value object with a "functions" key.
    assert!(
        artifact.abi.is_object(),
        "abi must be a JSON object, got: {:?}",
        artifact.abi
    );
    assert!(
        artifact.abi.get("functions").is_some(),
        "abi must have a 'functions' key"
    );
}

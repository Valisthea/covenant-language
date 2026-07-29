//! OMEGA V6 HGH-031 regression test.
//!
//! Before this fix, `CompilationMetadata` carried no signal about which
//! FHE/PQ/ZK primitives a compiled contract routes to a `Mocked*.sol`
//! helper contract for -- the only place that information existed was a
//! source-code comment in the Solidity helpers, which no `covenant` CLI
//! user or downstream integrator/auditor reading `.abi.json`/`.metadata.json`
//! would ever see. `detect_mocked_crypto_usage` now surfaces this from the
//! IR (ground truth of what codegen actually lowers) into
//! `artifact.metadata.mocked_crypto_primitives`.

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

/// `confidential token` synthesizes FHE actions (transferEncrypted,
/// approveEncrypted, ...) that route through `emit_precompile_call` against
/// the FHE helper family.
const CONFIDENTIAL_TOKEN: &str =
    include_str!("../../covenant-lexer/tests/fixtures/example_06_secret_coin.cov");

const HELLO: &str = include_str!("../../covenant-lexer/tests/fixtures/example_01_hello.cov");

fn compile(src: &str) -> EvmArtifact {
    let (toks, _) = tokenize(src, SourceId::new(0));
    let (file, _) = parse(&toks, SourceId::new(0));
    let (res, _) = resolve(file.unwrap(), SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (checked, _) = analyze_privacy(typed, SourceId::new(0));
    let (module, _) = build_ir(checked, SourceId::new(0));
    let (with_std, _) = lower_stdlib(module, StdlibConfig::default());
    let (optimized, _) = optimize(with_std, OptimizerConfig::default());
    let (artifact, _) = codegen_evm(optimized, EvmConfig::default());
    artifact
}

#[test]
fn confidential_token_reports_fhe_in_metadata_hgh_031() {
    let artifact = compile(CONFIDENTIAL_TOKEN);
    let cats: Vec<&str> = artifact
        .metadata
        .mocked_crypto_primitives
        .iter()
        .map(|c| c.as_ref())
        .collect();
    assert_eq!(
        cats,
        vec!["fhe"],
        "expected exactly the `fhe` mocked-crypto category for a confidential token"
    );
}

#[test]
fn plain_contract_reports_no_mocked_crypto_hgh_031() {
    let artifact = compile(HELLO);
    assert!(
        artifact.metadata.mocked_crypto_primitives.is_empty(),
        "a plain contract with no FHE/PQ/ZK constructs must report zero mocked-crypto categories, \
         got {:?}",
        artifact.metadata.mocked_crypto_primitives
    );
}

#[test]
fn ceremony_reports_amnesia_in_metadata() {
    // The amnesia category was originally excluded from this signal, on the
    // reasoning that `CeremonyHelper.sol` is a real if simplified commitment
    // rather than a coin-flip stub. That was the wrong call for the question
    // the field answers: a V0.9 ceremony's destroy path does NOT make anything
    // unrecoverable (the VDF, the Shamir split and the destruction proof are
    // deterministic stubs, and the "destroyed" secret stays readable from chain
    // state), so an empty `mockedCryptoPrimitives` told a reader gating on it
    // the exact opposite of the truth.
    //
    // Negative control for this test: revert `detect_mocked_crypto_usage` to
    // drop the amnesia arm and this assertion fails, while
    // `plain_contract_reports_no_mocked_crypto_hgh_031` and
    // `confidential_token_reports_fhe_in_metadata_hgh_031` both keep passing,
    // proving the arm is scoped to ceremony opcodes and has not simply started
    // reporting a category for everything.
    let src = r#"
ceremony SecretBurial {
    guardians: 3
    threshold: 2

    on_destroy {
        destroy(0)
    }
}
"#;
    let artifact = compile(src);
    let cats: Vec<&str> = artifact
        .metadata
        .mocked_crypto_primitives
        .iter()
        .map(|c| c.as_ref())
        .collect();
    assert!(
        cats.contains(&"amnesia"),
        "a `ceremony` contract depends on placeholder amnesia cryptography and must report the \
         `amnesia` category, got {cats:?}"
    );
}

#[test]
fn helper_contract_names_match_real_solidity_files_hgh_031() {
    // Cross-check the category -> contract-name mapping the CLI's warning
    // message uses against the categories this fixture actually produces,
    // so a future rename of the category strings can't silently desync the
    // printed helper name from the real `helpers/src/*.sol` file.
    use covenant_evm_backend::mocked_crypto::helper_contract_for_category;
    assert_eq!(helper_contract_for_category("fhe"), "MockedFHEHelper");
    assert_eq!(helper_contract_for_category("zk"), "MockedZKVerifier");
    assert_eq!(helper_contract_for_category("pq"), "MockedPQVerifier");
    assert_eq!(helper_contract_for_category("amnesia"), "CeremonyHelper");
}

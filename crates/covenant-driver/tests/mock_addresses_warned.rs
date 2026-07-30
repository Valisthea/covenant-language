//! Building mocked cryptography for the local mock chain must say so.
//!
//! `mockchain` is the default target, so it is what you get by typing nothing.
//! It bakes in the in-tab mock precompile addresses, which exist on no public
//! network, and until now the build printed `ok` with no warning at all. The
//! artifact on disk carries no target in its filename, so nothing between the
//! compiler and a deployment said the addresses were local.
//!
//! The gap was known and disclosed before it was closed: an artifact built
//! this way and deployed to a real chain reverts on the first action reaching
//! a helper. W534 makes the build say it.

use covenant_diag::{DiagCode, Diagnostic, DiagnosticLevel, SourceId};
use covenant_evm_backend::{EvmConfig, Target};
use covenant_opt::OptimizerConfig;
use covenant_stdlib::StdlibConfig;

const W534_MOCK_ADDRESSES_IN_ARTIFACT: DiagCode = DiagCode(534);

/// Reaches the FHE helper, so the target decides which addresses are baked in.
const USES_MOCKED_CRYPTO: &str = "\
encrypted counter Shielded {
    total: amount
    action bump(by: amount) { total += by }
    reveal total to owner
}
";

/// Touches no mocked primitive, so its bytecode is the same on every target
/// and there is nothing to warn about.
const USES_NONE: &str = "\
token DemoCoin {
    symbol:   \"DEMO\"
    name:     \"Demo Coin\"
    decimals: 18
    supply:   1_000_000 to deployer
}
";

fn diagnostics_for(source: &str, target: Target) -> Vec<Diagnostic> {
    covenant_driver::compile(
        source,
        SourceId::new(0),
        EvmConfig::for_target(target),
        StdlibConfig::default(),
        OptimizerConfig::default(),
    )
    .1
}

fn warned(source: &str, target: Target) -> bool {
    diagnostics_for(source, target)
        .iter()
        .any(|d| d.code == W534_MOCK_ADDRESSES_IN_ARTIFACT)
}

#[test]
fn the_mock_target_warns_when_it_bakes_mock_addresses() {
    assert!(
        warned(USES_MOCKED_CRYPTO, Target::MockChain),
        "a contract using mocked cryptography was built for the mock chain with \
         no warning, so nothing tells the user the baked addresses are local"
    );
}

/// The warning must not become noise. A contract that reaches no helper has
/// the same bytecode on every target, so there is nothing local about it.
#[test]
fn a_contract_without_mocked_crypto_is_not_warned() {
    assert!(
        !warned(USES_NONE, Target::MockChain),
        "warned about mock addresses on a contract that has none"
    );
}

/// The other half of the scoping. A helper target points at deployed
/// contracts, so the warning would be false there.
#[test]
fn a_helper_target_is_not_warned() {
    assert!(
        !warned(USES_MOCKED_CRYPTO, Target::Sepolia),
        "warned about mock addresses on a target that uses deployed helpers"
    );
}

/// It is a warning, not an error. Building for the mock chain is exactly right
/// when running the local harness, which is the common case, so the artifact
/// must still be produced.
#[test]
fn the_warning_does_not_block_the_build() {
    let (artifact, diagnostics) = covenant_driver::compile(
        USES_MOCKED_CRYPTO,
        SourceId::new(0),
        EvmConfig::for_target(Target::MockChain),
        StdlibConfig::default(),
        OptimizerConfig::default(),
    );
    assert!(artifact.is_some(), "the warning suppressed the artifact");
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.level == DiagnosticLevel::Error),
        "building mocked crypto for the mock chain raised an error: {diagnostics:#?}"
    );
}

/// A negative control. If the pipeline stopped producing diagnostics, the
/// three tests above would all pass on empty lists.
#[test]
fn the_pipeline_used_above_does_report_diagnostics() {
    assert!(
        !diagnostics_for("token Sample { this is not covenant }", Target::MockChain).is_empty(),
        "no diagnostics at all, so these tests prove nothing"
    );
}

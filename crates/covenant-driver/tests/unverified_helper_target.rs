//! A target may not carry helper calls to addresses nobody has verified.
//!
//! `aster_testnet` reuses the CREATE2 addresses predicted for Sepolia, on the
//! reasoning that the Arachnid factory is deterministic across EVM chains.
//! That holds only where the factory is itself deployed, and it was left to a
//! sprint that never ran: `config/helper-addresses-v0.9.0.json` still carries
//! `"helpers": null` for this target next to a status reading "Verify in
//! Sprint 42". There is no published Aster Chain testnet chain id and no
//! public EVM JSON-RPC endpoint to check against.
//!
//! Until that changes, a contract using any mocked primitive built for this
//! target ships calls into addresses that most likely hold no code. It
//! deploys, then reverts on first use, or a STATICCALL into an empty address
//! returns success with empty data and the primitive reads as a pass. That is
//! the failure this compiler exists to refuse.

use covenant_diag::{DiagCode, Diagnostic, DiagnosticLevel, SourceId};
use covenant_evm_backend::{EvmArtifact, EvmConfig, Target};
use covenant_opt::OptimizerConfig;
use covenant_stdlib::StdlibConfig;

/// Uses a mocked FHE primitive, so it must reach a helper contract.
const USES_A_HELPER: &str = "\
encrypted counter Shielded {
    total: amount

    action bump(by: amount) { total += by }

    reveal total to owner
}
";

/// Touches no mocked primitive, so its bytecode is the same on every target.
const USES_NO_HELPER: &str = "\
token DemoCoin {
    symbol:   \"DEMO\"
    name:     \"Demo Coin\"
    decimals: 18
    supply:   1_000_000 to deployer
}
";

const E533_UNVERIFIED_HELPER_TARGET: DiagCode = DiagCode(533);

fn build_for(source: &str, target: Target) -> (Option<EvmArtifact>, Vec<Diagnostic>) {
    covenant_driver::compile(
        source,
        SourceId::new(0),
        EvmConfig::for_target(target),
        StdlibConfig::default(),
        OptimizerConfig::default(),
    )
}

fn refusals(diagnostics: &[Diagnostic]) -> Vec<&Diagnostic> {
    diagnostics
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Error && d.code == E533_UNVERIFIED_HELPER_TARGET)
        .collect()
}

#[test]
fn a_helper_call_is_refused_for_a_target_with_unverified_helpers() {
    let (_artifact, diagnostics) = build_for(USES_A_HELPER, Target::AsterTestnet);
    assert!(
        !refusals(&diagnostics).is_empty(),
        "a helper call was emitted for aster_testnet, whose helper addresses \
         have never been confirmed deployed"
    );
    // `compile` returns the artifact alongside the diagnostics by design, so
    // a caller can inspect both. What matters is that the error is present:
    // the CLI gates on the error list and writes no files, verified by
    // `covenant build --target-chain aster_testnet` exiting 1 with an empty
    // output directory.
    assert!(
        diagnostics
            .iter()
            .any(|d| d.level == DiagnosticLevel::Error),
        "the refusal was not raised at error level, so the CLI would still \
         write the artifact"
    );
}

/// The negative control. Sepolia points at the same four addresses, and there
/// they are deployed and answer `eth_getCode`. If this started failing, the
/// refusal would be rejecting every helper target rather than the unverified
/// one, and the test above would prove nothing.
#[test]
fn the_same_program_builds_for_the_verified_helper_target() {
    let (artifact, diagnostics) = build_for(USES_A_HELPER, Target::Sepolia);
    assert!(
        refusals(&diagnostics).is_empty(),
        "sepolia was refused, but its helpers are deployed and verified"
    );
    assert!(artifact.is_some(), "no artifact for sepolia");
}

/// The other half of the control. MockChain implements these opcodes as native
/// precompiles, so there is no deployed helper to be missing.
#[test]
fn the_same_program_builds_for_the_native_precompile_target() {
    let (artifact, diagnostics) = build_for(USES_A_HELPER, Target::MockChain);
    assert!(
        refusals(&diagnostics).is_empty(),
        "mockchain was refused, but it needs no deployed helper at all"
    );
    assert!(artifact.is_some(), "no artifact for mockchain");
}

/// The refusal is scoped to programs that actually reach a helper. A contract
/// touching no mocked primitive compiles to the same bytes on every target, so
/// refusing it would block the ordinary case for no benefit.
#[test]
fn a_program_with_no_helper_call_still_builds_for_every_target() {
    for target in [Target::MockChain, Target::Sepolia, Target::AsterTestnet] {
        let (artifact, diagnostics) = build_for(USES_NO_HELPER, target);
        assert!(
            refusals(&diagnostics).is_empty(),
            "{target:?} refused a program that reaches no helper"
        );
        assert!(artifact.is_some(), "no artifact for {target:?}");
    }
}

/// The claim the refusal rests on, asserted directly so that flipping the flag
/// without deploying anything is a visible edit rather than a quiet one.
#[test]
fn only_the_unverified_target_is_marked_unverified() {
    assert!(Target::MockChain.helpers_verified_deployed());
    assert!(Target::Sepolia.helpers_verified_deployed());
    assert!(
        !Target::AsterTestnet.helpers_verified_deployed(),
        "aster_testnet is marked verified. If the helpers really were deployed \
         and checked on that chain, update the address manifest in \
         config/helper-addresses-v0.9.0.json, which still records \
         \"helpers\": null, in the same commit that changes this."
    );
}

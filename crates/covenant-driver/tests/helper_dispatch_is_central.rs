//! No helper selector is emitted outside the central gate.
//!
//! Two paths used to call `abi::precompile_selector` directly instead of going
//! through the dispatch table: the confidential-token genesis mint
//! (`FheEncryptTrivial`) and the encrypted balance check (`AssertEncrypted`).
//! On a helper target that emits the V0.8 namespaced selector rather than the
//! deployed method's ABI selector, and skips every fail-loud gate. So a
//! confidential token's genesis mint on Sepolia called a selector that matches
//! no deployed method, and `AssertEncrypted`, which has no helper method at
//! all, called a nonexistent function. Both compiled clean and reverted.
//!
//! Both now resolve through `resolve_precompile_selector`, so the selector is
//! target-aware and the gates fire. These tests prove it from the emitted
//! bytecode and from the diagnostics.

use covenant_diag::{DiagCode, Diagnostic, DiagnosticLevel, SourceId};
use covenant_evm_backend::{abi, EvmArtifact, EvmConfig, Target};
use covenant_opt::OptimizerConfig;
use covenant_stdlib::StdlibConfig;

const E520_HELPER_METHOD_MISSING: DiagCode = DiagCode(520);

/// A confidential token. Its genesis mint reaches `FheEncryptTrivial`, and its
/// synthesized `transferEncrypted` reaches `FheCmpGe`, which has no helper
/// method and is refused independently. We inspect the artifact regardless.
const CONFIDENTIAL: &str = "\
-- ERC-8227: Confidential Token Interface (Styx Protocol)
confidential token Secret {
    symbol: \"SEC\"
    name: \"Secret\"
    decimals: 18
    supply: 1000 to deployer
}";

fn compile(source: &str, target: Target) -> (Option<EvmArtifact>, Vec<Diagnostic>) {
    covenant_driver::compile(
        source,
        SourceId::new(0),
        EvmConfig::for_target(target),
        StdlibConfig::default(),
        OptimizerConfig::default(),
    )
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    needle.len() <= haystack.len() && haystack.windows(needle.len()).any(|w| w == needle)
}

/// The confidential-token genesis mint now embeds the ABI selector of
/// `encryptTrivial(uint256)` on a helper target, not the V0.8 namespaced form.
///
/// The artifact is produced even though the build has errors (`FheCmpGe` is
/// refused), because codegen stays total, so the genesis-mint selector is
/// observable in the deploy bytecode.
#[test]
fn the_genesis_mint_selector_is_target_aware() {
    let abi_selector = abi::selector("encryptTrivial(uint256)");
    let namespaced = abi::precompile_selector("FheEncryptTrivial");
    assert_ne!(
        abi_selector, namespaced,
        "the two selector forms are equal, so this test cannot tell them apart"
    );

    let (mock, _) = compile(CONFIDENTIAL, Target::MockChain);
    let mock = mock.expect("mockchain produced no artifact");
    assert!(
        contains(&mock.deploy_bytecode, &namespaced),
        "mockchain genesis mint does not use the namespaced precompile selector"
    );

    let (sepolia, _) = compile(CONFIDENTIAL, Target::Sepolia);
    let sepolia = sepolia.expect("sepolia produced no artifact");
    assert!(
        contains(&sepolia.deploy_bytecode, &abi_selector),
        "sepolia genesis mint does not use the ABI selector of encryptTrivial, so \
         it still calls a method that does not exist on the deployed helper"
    );
    assert!(
        !contains(&sepolia.deploy_bytecode, &namespaced),
        "sepolia genesis mint still carries the namespaced selector"
    );
}

/// `AssertEncrypted` has no deployed helper method, so on a helper target it
/// must now be refused. It used to bypass the gate entirely.
///
/// The confidential token reaches it, alongside `FheCmpGe`. Both are E520;
/// what matters is that `AssertEncrypted` is now among them rather than
/// silently emitting a call to a nonexistent function.
#[test]
fn assert_encrypted_is_gated_on_a_helper_target() {
    let (_, diags) = compile(CONFIDENTIAL, Target::Sepolia);
    let missing: Vec<&str> = diags
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Error && d.code == E520_HELPER_METHOD_MISSING)
        .map(|d| d.message.as_str())
        .collect();
    assert!(
        missing.iter().any(|m| m.contains("AssertEncrypted")),
        "AssertEncrypted was not gated on sepolia, so it still emits a call to a \
         method the deployed helper does not have. E520 messages seen: {missing:?}"
    );
}

/// The same construct on MockChain builds, because both opcodes are native
/// precompiles there. The gate is scoped to helper targets, not a blanket ban.
#[test]
fn the_same_construct_builds_on_the_native_precompile_target() {
    let (artifact, diags) = compile(CONFIDENTIAL, Target::MockChain);
    let e520: Vec<_> = diags
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Error && d.code == E520_HELPER_METHOD_MISSING)
        .collect();
    assert!(
        e520.is_empty(),
        "mockchain raised E520, but its precompiles implement these opcodes: {e520:#?}"
    );
    assert!(artifact.is_some(), "no mockchain artifact: {diags:#?}");
}

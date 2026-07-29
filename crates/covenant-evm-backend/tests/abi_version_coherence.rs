//! KSR-CVN-040 regression tests: PRECOMPILE_ABI_VERSION is a single source of truth.
//!
//! Before this fix, `abi.rs` and `artifact.rs` each defined an independent
//! `pub const PRECOMPILE_ABI_VERSION: u32 = 1;`. A maintainer bumping one
//! without the other would silently produce a bytecode marker at version N
//! while wire selectors remained at version 1.
//!
//! After the fix, `artifact::PRECOMPILE_ABI_VERSION` is a re-export of
//! `abi::PRECOMPILE_ABI_VERSION`. This test guards against accidental reversion.

use covenant_evm_backend::abi::PRECOMPILE_ABI_VERSION as ABI_VERSION;
use covenant_evm_backend::artifact::PRECOMPILE_ABI_VERSION as ARTIFACT_VERSION;

#[test]
fn precompile_abi_version_is_consolidated() {
    assert_eq!(
        ABI_VERSION, ARTIFACT_VERSION,
        "abi::PRECOMPILE_ABI_VERSION and artifact::PRECOMPILE_ABI_VERSION must be identical \
: artifact.rs must re-export from abi.rs (KSR-CVN-040)"
    );
}

#[test]
fn precompile_abi_version_matches_selector_format() {
    let selector_str = format!("covenant.precompile.test_op:v{ABI_VERSION}");
    assert!(
        selector_str.ends_with(":v1"),
        "selector string must contain version 1; got `{selector_str}`"
    );
}

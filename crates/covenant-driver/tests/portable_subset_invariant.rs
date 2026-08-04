//! The portable subset must compile to the same bytes on every EVM target.
//!
//! Constructs that touch no mocked cryptography emit no chain-specific
//! address, so their artifact is chain-agnostic. That is what lets a Covenant
//! token deploy on an Arbitrum Orbit chain the compiler has never heard of,
//! and it is the most valuable property the project has.
//!
//! It was also entirely untested. `EvmConfig::for_target` appeared in three
//! test files, none of which compared bytecode, so nothing would have noticed
//! a target-dependent byte creeping into the portable path.
//!
//! This is a metamorphic gate rather than a golden file. It does not assert
//! what the bytecode is, which would break on every legitimate codegen
//! change. It asserts that the bytecode does not depend on the target, which
//! must hold forever.

use std::collections::BTreeSet;

use covenant_diag::SourceId;
use covenant_evm_backend::target::{
    CEREMONY_HELPER_V090, FHE_HELPER_V090, PQ_HELPER_V090, ZK_HELPER_V090,
};
use covenant_evm_backend::{EvmArtifact, EvmConfig, Target};
use covenant_opt::OptimizerConfig;
use covenant_stdlib::StdlibConfig;

/// Every EVM target the compiler accepts.
const TARGETS: &[Target] = &[Target::MockChain, Target::Sepolia, Target::AsterTestnet];

/// Programs that must be byte-identical everywhere. One per construct family
/// that is supposed to be portable, so a regression names the construct that
/// broke rather than just failing.
const PORTABLE: &[(&str, &str)] = &[
    (
        "token",
        "token Coin {
    symbol:   \"COIN\"
    name:     \"Coin\"
    decimals: 18
    supply:   1_000_000 to deployer
}",
    ),
    (
        "record",
        "record Config {
    owner:   address
    enabled: bool
}",
    ),
    (
        "counter",
        "counter Tally {
    action bump() { }
}",
    ),
    (
        "module with a guard",
        "module Gate {
    field owner: address
    field n: amount

    action bump() only owner { n += 1 }
}",
    ),
    (
        "module with a value transfer",
        "module Payer {
    field balances: map<address, amount>

    action withdraw(v: amount) when balances[caller] >= v {
        balances[caller] -= v
        transfer(v) to caller
    }
}",
    ),
];

/// Reaches a helper, so its bytecode is target-dependent by construction.
/// Used as the negative control.
const HELPER_DEPENDENT: &str = "\
encrypted counter Shielded {
    total: amount
    action bump(by: amount) { total += by }
    reveal total to owner
}";

fn build(source: &str, target: Target) -> EvmArtifact {
    let (artifact, diagnostics) = covenant_driver::compile(
        source,
        SourceId::new(0),
        EvmConfig::for_target(target),
        StdlibConfig::default(),
        OptimizerConfig::default(),
    );
    artifact.unwrap_or_else(|| panic!("no artifact for {target:?}, diagnostics: {diagnostics:#?}"))
}

/// The four helper addresses, as they would appear inside emitted bytecode.
fn helper_address_bytes() -> Vec<[u8; 20]> {
    vec![
        CEREMONY_HELPER_V090,
        FHE_HELPER_V090,
        PQ_HELPER_V090,
        ZK_HELPER_V090,
    ]
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    needle.len() <= haystack.len() && haystack.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn portable_programs_emit_identical_bytecode_on_every_target() {
    for (name, source) in PORTABLE {
        let reference = build(source, TARGETS[0]);
        for target in &TARGETS[1..] {
            let other = build(source, *target);
            assert_eq!(
                reference.deploy_bytecode, other.deploy_bytecode,
                "`{name}` produced different deploy bytecode on {:?} and {target:?}, so it \
                 is not portable",
                TARGETS[0]
            );
            assert_eq!(
                reference.runtime_bytecode, other.runtime_bytecode,
                "`{name}` produced different runtime bytecode on {:?} and {target:?}",
                TARGETS[0]
            );
        }
    }
}

#[test]
fn portable_programs_have_identical_abi_and_storage_layout() {
    for (name, source) in PORTABLE {
        let reference = build(source, TARGETS[0]);
        let ref_slots: Vec<_> = reference
            .storage_layout
            .entries
            .iter()
            .map(|e| {
                (
                    e.name.clone(),
                    e.slot,
                    e.offset,
                    e.size_bytes,
                    e.ty_desc.clone(),
                )
            })
            .collect();
        let ref_selectors: BTreeSet<_> = reference.function_selectors.iter().collect();

        for target in &TARGETS[1..] {
            let other = build(source, *target);
            assert_eq!(
                reference.abi, other.abi,
                "`{name}`: ABI differs on {target:?}"
            );
            let other_slots: Vec<_> = other
                .storage_layout
                .entries
                .iter()
                .map(|e| {
                    (
                        e.name.clone(),
                        e.slot,
                        e.offset,
                        e.size_bytes,
                        e.ty_desc.clone(),
                    )
                })
                .collect();
            assert_eq!(
                ref_slots, other_slots,
                "`{name}`: storage layout differs on {target:?}"
            );
            let other_selectors: BTreeSet<_> = other.function_selectors.iter().collect();
            assert_eq!(
                ref_selectors, other_selectors,
                "`{name}`: selector table differs on {target:?}"
            );
        }
    }
}

#[test]
fn portable_programs_bake_in_no_helper_address() {
    let addresses = helper_address_bytes();
    for (name, source) in PORTABLE {
        for target in TARGETS {
            let artifact = build(source, *target);
            for addr in &addresses {
                assert!(
                    !contains(&artifact.runtime_bytecode, addr),
                    "`{name}` on {target:?} has a helper address in its runtime bytecode, \
                     so the artifact is not chain-agnostic"
                );
                assert!(
                    !contains(&artifact.deploy_bytecode, addr),
                    "`{name}` on {target:?} has a helper address in its deploy bytecode"
                );
            }
        }
    }
}

#[test]
fn portable_programs_declare_no_mocked_primitive() {
    for (name, source) in PORTABLE {
        for target in TARGETS {
            let artifact = build(source, *target);
            assert!(
                artifact.metadata.mocked_crypto_primitives.is_empty(),
                "`{name}` on {target:?} reports mocked primitives {:?}, so it does not \
                 belong in the portable set",
                artifact.metadata.mocked_crypto_primitives
            );
        }
    }
}

/// The negative control, and the reason the tests above are not vacuous.
///
/// A program that reaches a helper MUST differ between a native-precompile
/// target and a helper-contract target. If this ever stops holding, either
/// the helper addresses stopped being baked in, or the comparison above is
/// not comparing anything.
#[test]
fn a_helper_dependent_program_is_not_portable() {
    let mock = build(HELPER_DEPENDENT, Target::MockChain);
    let sepolia = build(HELPER_DEPENDENT, Target::Sepolia);

    assert_ne!(
        mock.runtime_bytecode, sepolia.runtime_bytecode,
        "a program using mocked cryptography produced identical bytecode on the mock \
         chain and on a helper target. Either the helper address is no longer baked in, \
         or the portable-subset tests are comparing nothing"
    );

    assert!(
        !mock.metadata.mocked_crypto_primitives.is_empty(),
        "the control program reports no mocked primitives, so it is not exercising the \
         helper path this test depends on"
    );

    let addresses = helper_address_bytes();
    assert!(
        addresses
            .iter()
            .any(|a| contains(&sepolia.runtime_bytecode, a)),
        "no helper address found in the sepolia build of the control program, so the \
         address search used by the portable tests would never find one either"
    );
}

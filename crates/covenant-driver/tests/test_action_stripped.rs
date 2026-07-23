//! `test_*` actions must NOT ship in a release build.
//!
//! The V0.9 test pattern is a plain `action test_X() when <assert> {}`, so
//! before this fix every test compiled into the deployed contract as a public
//! function — confirmed on-chain 2026-07-23 on the M6 token, where all five
//! test_* selectors were live and callable. A test that MUTATES is then a
//! public, unguarded state mutator (the repo's own test_isolation_demo.cov
//! contains `action test_mutate_n_to_5() { n = 5 }`).
//!
//! A `test_*` action (or one carrying `@test`) is now classified as
//! `IrFunctionKind::Test` and stripped from the ABI, selector table and
//! dispatcher unless `EvmConfig::include_test_actions` is set — which only the
//! `covenant test` harness does.

use covenant_diag::SourceId;
use covenant_driver::compile;
use covenant_evm_backend::EvmConfig;
use covenant_opt::OptimizerConfig;
use covenant_stdlib::StdlibConfig;

const SRC: &str = r#"
record Bank {
    balance: amount

    action deposit(n: amount) { balance = balance + n }

    view get returns amount { balance }

    action test_starts_empty() when balance == 0 {}

    action test_mutate() { balance = 999 }
}
"#;

fn abi_for(include_test_actions: bool) -> String {
    let config = EvmConfig {
        include_test_actions,
        ..EvmConfig::default()
    };
    let (artifact, diags) = compile(
        SRC,
        SourceId::new(0),
        config,
        StdlibConfig::default(),
        OptimizerConfig::default(),
    );
    assert!(
        artifact.is_some(),
        "compile should succeed; got diags: {diags:?}"
    );
    artifact.unwrap().abi
}

#[test]
fn release_build_omits_test_actions_from_abi() {
    let abi = abi_for(false);
    assert!(
        !abi.contains("test_starts_empty") && !abi.contains("test_mutate"),
        "a release build must not expose test_* actions in the ABI:\n{abi}"
    );
    // The real interface is still there.
    assert!(abi.contains("deposit") && abi.contains("get"));
}

#[test]
fn release_build_omits_test_actions_from_selectors() {
    let config = EvmConfig::default(); // include_test_actions = false
    let (artifact, _) = compile(
        SRC,
        SourceId::new(0),
        config,
        StdlibConfig::default(),
        OptimizerConfig::default(),
    );
    let selectors = artifact.unwrap().function_selectors;
    let names: Vec<&str> = selectors.keys().map(|k| k.as_ref()).collect();
    assert!(
        !names.iter().any(|n| n.starts_with("test_")),
        "release selector table must not contain any test_* entry: {names:?}"
    );
    assert!(names.contains(&"deposit"));
}

#[test]
fn harness_config_keeps_test_actions() {
    // What the `covenant test` harness does — tests must remain callable.
    let abi = abi_for(true);
    assert!(
        abi.contains("test_starts_empty") && abi.contains("test_mutate"),
        "with include_test_actions the harness must still see the tests:\n{abi}"
    );
}

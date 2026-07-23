//! Native smoke tests for the wasm-bindings adapter layer.
//!
//! Runs with `cargo test -p covenant-wasm-bindings` — no wasm-pack,
//! no headless browser. The adapter functions are the same code path
//! the WASM build will execute, just minus the `JsValue` ser step.

use covenant_wasm_bindings::adapt::{check_only, compile_evm, compile_ir};
use covenant_wasm_bindings::result::JsLevel;

const HELLO: &str = include_str!("../../covenant-lexer/tests/fixtures/example_01_hello.cov");
const COIN: &str = include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov");

#[test]
fn hello_compiles_to_evm_with_artifact_and_no_errors() {
    let r = compile_evm(HELLO);
    let errs: Vec<_> = r
        .diagnostics
        .iter()
        .filter(|d| d.level == JsLevel::Error)
        .collect();
    assert!(
        errs.is_empty(),
        "Hello.cov must compile cleanly, got errors: {errs:?}"
    );
    assert!(r.ok, "ok must be true when no errors");
    assert!(
        r.deploy_bytecode.is_some(),
        "deploy bytecode must be present"
    );
    assert!(
        r.runtime_bytecode.is_some(),
        "runtime bytecode must be present"
    );
    assert!(r.abi.is_some(), "abi must be present");

    let deploy = r.deploy_bytecode.unwrap();
    let runtime = r.runtime_bytecode.unwrap();
    assert!(
        deploy.starts_with("0x"),
        "deploy bytecode must be 0x-prefixed"
    );
    assert!(
        runtime.starts_with("0x"),
        "runtime bytecode must be 0x-prefixed"
    );
    // Deploy bytecode is constructor + runtime, so it's strictly longer.
    assert!(deploy.len() > runtime.len(), "deploy must include runtime");

    // Metadata round-trips.
    let meta = r.metadata.expect("metadata must be present");
    assert_eq!(meta.covenant_version, env!("CARGO_PKG_VERSION"));
    assert!(meta.precompile_abi_version >= 1);
}

#[test]
fn coin_produces_erc20_selectors() {
    let r = compile_evm(COIN);
    assert!(
        r.ok,
        "coin example must compile, diags: {:?}",
        r.diagnostics
    );

    let names: std::collections::BTreeSet<_> = r
        .function_selectors
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    // ERC-20 mandatory surface.
    for required in &[
        "transfer",
        "totalSupply",
        "balanceOf",
        "allowance",
        "approve",
        "transferFrom",
    ] {
        assert!(
            names.contains(required),
            "coin must expose ERC-20 `{required}`, got {names:?}"
        );
    }

    // Each selector is 0x + 8 hex chars.
    for sel in &r.function_selectors {
        assert_eq!(sel.selector.len(), 10, "selector `{}` malformed", sel.name);
        assert!(sel.selector.starts_with("0x"));
    }
}

#[test]
fn syntax_error_is_reported_with_line_col() {
    // Missing closing brace inside `set` — parser-level error.
    let src = "record Bad {\n    field x: int\n    action set(v: int) {\n        x = v\n    \n}\n";
    let r = compile_evm(src);
    assert!(!r.ok, "broken source must report ok = false");
    assert!(
        !r.diagnostics.is_empty(),
        "must produce at least one diagnostic"
    );

    let first_err = r
        .diagnostics
        .iter()
        .find(|d| d.level == JsLevel::Error)
        .expect("must surface an error-level diagnostic");
    assert!(first_err.line >= 1, "line must be 1-indexed");
    assert!(first_err.column >= 1, "column must be 1-indexed");
    assert!(!first_err.code.is_empty(), "diagnostic code must be set");
    assert!(
        first_err.code.starts_with('E'),
        "code must format as E<num>, got {}",
        first_err.code
    );
}

#[test]
fn check_is_cheaper_than_full_compile() {
    // Run each twice to warm caches; the cheap path must still win.
    let _ = compile_evm(HELLO);
    let _ = check_only(HELLO);

    let c0 = compile_evm(HELLO);
    let k0 = check_only(HELLO);

    // Compile must do strictly more work than check, so its `total`
    // must be ≥ check's. Allow a 20% wiggle because system clock
    // resolution on Windows can flatten to 0.5-1ms increments.
    assert!(
        c0.timing.total + 1.0 >= k0.timing.total,
        "compile ({:.2}ms) must be >= check ({:.2}ms)",
        c0.timing.total,
        k0.timing.total
    );
}

#[test]
fn ir_text_renders_for_hello() {
    let r = compile_ir(HELLO);
    assert!(r.ok, "Hello.cov must produce IR cleanly");
    let ir = r.ir_text.expect("ir_text must be Some");
    // Pretty-printed Debug always begins with the type name.
    assert!(
        ir.starts_with("IrModule"),
        "ir_text must start with IrModule, got: {}",
        ir.chars().take(100).collect::<String>()
    );
    // The Hello record exposes `set` and `read` actions in the fixture.
    assert!(ir.contains("set") || ir.contains("read") || ir.contains("Hello"));
}

#[test]
fn five_basic_examples_all_compile() {
    // Mirrors the existing covenant-driver test_compiles_all_basics test —
    // proves the bindings adapter doesn't drop any artifact or mangle
    // any selector for the canonical fixture set.
    //
    // OMEGA V6 CRT-004 fix (E518): ballot/board use BuiltinPredicate guards
    // (first_time_caller / registered_key) that now correctly fail to
    // compile -- see the `_expect_e518` flag below.
    for (name, src, expect_e518) in [
        (
            "hello",
            include_str!("../../covenant-lexer/tests/fixtures/example_01_hello.cov"),
            false,
        ),
        (
            "coin",
            include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov"),
            false,
        ),
        (
            "ballot",
            include_str!("../../covenant-lexer/tests/fixtures/example_03_open_ballot.cov"),
            true,
        ),
        (
            "counter",
            include_str!("../../covenant-lexer/tests/fixtures/example_04_shielded_counter.cov"),
            false,
        ),
        (
            "board",
            include_str!("../../covenant-lexer/tests/fixtures/example_05_quantum_board.cov"),
            true,
        ),
    ] {
        let r = compile_evm(src);
        let errs: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| d.level == JsLevel::Error)
            .collect();
        if expect_e518 {
            assert!(
                errs.iter().any(|d| d.code == "E518"),
                "{name}: expected E518 (unlowered BuiltinPredicate), got {errs:?}"
            );
        } else {
            assert!(errs.is_empty(), "{name}: must compile clean, got {errs:?}");
            assert!(r.ok, "{name}: ok must be true");
            assert!(r.deploy_bytecode.is_some(), "{name}: missing bytecode");
            assert!(r.abi.is_some(), "{name}: missing ABI");
        }
    }
}

//! Integration test for `covenant build`.
//!
//! Invokes the release binary on the Coin fixture and verifies the four
//! expected artifact files are produced with plausible structure.

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR points to crates/covenant-cli; the workspace root is
    // two levels up.
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p
}

fn binary() -> PathBuf {
    // Prefer the release binary if it exists (matches the acceptance spec);
    // fall back to debug for `cargo test` runs that haven't built release.
    let root = workspace_root();
    let rel = root.join("target/release/covenant.exe");
    if rel.exists() {
        return rel;
    }
    let rel_unix = root.join("target/release/covenant");
    if rel_unix.exists() {
        return rel_unix;
    }
    let dbg = root.join("target/debug/covenant.exe");
    if dbg.exists() {
        return dbg;
    }
    root.join("target/debug/covenant")
}

#[test]
fn build_coin_produces_four_artifacts() {
    let root = workspace_root();
    let src = root.join("crates/covenant-lexer/tests/fixtures/example_02_coin.cov");
    assert!(src.exists(), "fixture missing: {}", src.display());

    let out = std::env::temp_dir().join("covenant_cli_test_coin");
    let _ = std::fs::remove_dir_all(&out);

    let bin = binary();
    assert!(
        bin.exists(),
        "run `cargo build -p covenant-cli` first; expected {}",
        bin.display()
    );

    let status = Command::new(&bin)
        .arg("build")
        .arg(&src)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("spawn covenant");
    assert!(status.success(), "covenant build exited non-zero");

    for f in [
        "Coin.bin",
        "Coin.runtime.bin",
        "Coin.abi.json",
        "Coin.storage.json",
        "Coin.metadata.json",
    ] {
        let p = out.join(f);
        assert!(p.exists(), "missing artifact: {}", p.display());
        let size = std::fs::metadata(&p).unwrap().len();
        assert!(size > 0, "empty artifact: {}", p.display());
    }

    // Validate the metadata JSON mentions canonical ERC-20 selectors.
    let meta = std::fs::read_to_string(out.join("Coin.metadata.json")).unwrap();
    assert!(meta.contains("0xa9059cbb"), "transfer selector absent");
    assert!(meta.contains("0x18160ddd"), "totalSupply selector absent");

    // Validate the ABI JSON contains the expected function names.
    let abi = std::fs::read_to_string(out.join("Coin.abi.json")).unwrap();
    assert!(abi.contains("\"totalSupply\""));
    assert!(abi.contains("\"balanceOf\""));
    assert!(abi.contains("\"transfer\""));

    // Validate the deploy bytecode is hex and non-trivial.
    let bin_hex = std::fs::read_to_string(out.join("Coin.bin")).unwrap();
    assert!(bin_hex.chars().all(|c| c.is_ascii_hexdigit()));
    assert!(bin_hex.len() > 200, "deploy bytecode suspiciously short");
}

#[test]
fn build_missing_file_exits_nonzero() {
    let bin = binary();
    if !bin.exists() {
        return;
    }
    let out = std::env::temp_dir().join("covenant_cli_test_missing");
    let status = Command::new(&bin)
        .arg("build")
        .arg("/does/not/exist.cov")
        .arg("--out")
        .arg(&out)
        .status()
        .expect("spawn covenant");
    assert!(!status.success(), "expected non-zero exit on missing input");
}

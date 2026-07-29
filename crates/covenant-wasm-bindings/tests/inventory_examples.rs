//! Inventory test: try compiling every .cov file in the playground's
//! Examples Gallery. Surfaces which examples are real working Covenant
//! and which are pseudo-Covenant (Solidity-flavored aspirational
//! pseudo-code that never compiled).
//!
//! Sprint 26 audit Phase 5 finding KSR-CVN-PRELIM-009, the operator's
//! browser validation discovered that ~10+ examples use `require()`,
//! `map<...>`, `encrypted<...>`, etc.: none of which exist in real
//! Covenant per the working compiler fixtures.
//!
//! Run with:
//!
//!   cargo test -p covenant-wasm-bindings --test inventory_examples \
//!     -- --nocapture --ignored
//!
//! `--ignored` because this test reads files outside the workspace
//! (the playground repo) so it shouldn't run by default in CI.

use covenant_wasm_bindings::adapt::compile_evm;
use covenant_wasm_bindings::result::JsLevel;

use std::fs;
use std::path::PathBuf;

fn examples_dir() -> Option<PathBuf> {
    // Try a few likely paths from this crate's root.
    let candidates = [
        "../../../covenant-playground/public/examples",
        "../../covenant-playground/public/examples",
        "../covenant-playground/public/examples",
    ];
    for c in &candidates {
        let p = PathBuf::from(c);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

#[test]
#[ignore = "reads files from sibling covenant-playground repo; run explicitly"]
fn inventory_playground_examples() {
    let dir = match examples_dir() {
        Some(d) => d,
        None => {
            eprintln!("playground examples dir not found, run from covenant repo root");
            eprintln!("expected at ../covenant-playground/public/examples");
            return;
        }
    };

    let mut entries: Vec<_> = fs::read_dir(&dir)
        .expect("read examples dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("cov"))
        .collect();
    entries.sort();

    let mut pass = Vec::new();
    let mut fail = Vec::new();

    for path in &entries {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let source = fs::read_to_string(path).expect("read .cov");
        let r = compile_evm(&source);
        let errs: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| d.level == JsLevel::Error)
            .collect();
        if r.ok && errs.is_empty() {
            pass.push(name.clone());
        } else {
            let first = errs
                .first()
                .map(|d| format!("L{}:{} {}: {}", d.line, d.column, d.code, d.message))
                .unwrap_or_else(|| "(no specific error)".into());
            fail.push((name.clone(), first));
        }
    }

    println!("\n=== EXAMPLES INVENTORY ({} total) ===", entries.len());
    println!("\n✅ COMPILES CLEAN ({}):", pass.len());
    for n in &pass {
        println!("   ✓ {n}");
    }
    println!("\n❌ FAILS TO COMPILE ({}):", fail.len());
    for (n, e) in &fail {
        println!("   ✗ {n}");
        println!("      {e}");
    }
    println!(
        "\n=== TOTAL: {}/{} compile ===\n",
        pass.len(),
        entries.len()
    );

    // Hotfix 27.0 gate: every .cov shipped in public/examples/ MUST compile.
    // Sprint 27 deleted the 19 broken legacy files and rewrote 10 against the
    // verified compiler fixtures. From here on, anyone adding a new file to
    // public/examples/ has to make sure it parses + lowers cleanly.
    //
    // If you intentionally want to ship a "preview / not-yet-compiling"
    // example, do NOT put it in public/examples/: keep it in src/examples/
    // metadata only and gate visibility via a feature flag.
    assert!(
        fail.is_empty(),
        "{} example(s) in public/examples/ failed to compile, see stdout",
        fail.len()
    );
}

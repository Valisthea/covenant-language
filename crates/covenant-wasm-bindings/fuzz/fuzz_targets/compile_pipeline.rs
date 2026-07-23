//! Fuzz target: full compile pipeline.
//!
//! Sprint 26 audit Phase 3 (KSR-AUD-V0.8). Run with:
//!
//! ```bash
//! cd crates/covenant-wasm-bindings
//! cargo +nightly fuzz run compile_pipeline -- -max_total_time=14400  # 4 hours
//! ```
//!
//! Pass criterion: zero panics across the full run. Any panic = a
//! finding (PoC reproducible from fuzz/artifacts/compile_pipeline/).
//!
//! Coverage targets (informal): the lexer + parser + resolver + types
//! + privacy + IR + opt + EVM codegen. Anything that triggers a panic
//! in any of those is a real bug — the compiler should always either
//! succeed or return diagnostics, never crash.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Only feed valid UTF-8. The lexer rejects non-UTF-8 with a
    // diagnostic; we want to fuzz the post-lex stages too.
    if let Ok(s) = std::str::from_utf8(data) {
        // We deliberately discard the result; we only care that the
        // call returns (no panic, no infinite loop).
        let _ = covenant_wasm_bindings::adapt::compile_evm(s);
    }
});

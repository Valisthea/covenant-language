//! Fuzz target: frontend-only check (no codegen).
//!
//! ```bash
//! cargo +nightly fuzz run check_only -- -max_total_time=3600  # 1 hour
//! ```
//!
//! Cheaper per-iteration than the full pipeline (no IR, no codegen) so
//! we cover more of the lex/parse/resolve/typecheck/privacy surface
//! per CPU-second.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = covenant_wasm_bindings::adapt::check_only(s);
    }
});

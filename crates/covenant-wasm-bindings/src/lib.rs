//! WASM bindings for the Covenant compiler.
//!
//! Bridge between the Rust `covenant-driver` API and the JavaScript
//! playground at <https://playground.covenant-lang.org>.
//!
//! ## Build
//!
//! ```bash
//! wasm-pack build --target web --release \
//!   --out-dir ../../../covenant-playground/public/covenant-wasm \
//!   --out-name covenant_wasm_bindings \
//!   crates/covenant-wasm-bindings
//! ```
//!
//! ## Public surface
//!
//! All `#[wasm_bindgen]` functions return a `JsValue` produced by
//! `serde_wasm_bindgen::to_value`. The JS side gets idiomatic objects
//! (no JSON.parse round-trip). Native callers (`cargo test`) consume
//! the same shapes via [`adapt`] directly — see `tests/smoke.rs`.

#![deny(rust_2018_idioms)]

pub mod adapt;
pub mod result;

pub use result::{
    CompileTarget, JsCheckResult, JsCompileResult, JsDiagExplanation, JsDiagnostic, JsIrResult,
    JsLevel, JsMapping, JsMetadata, JsSelector, JsSourceMap, JsStorageEntry, JsTiming,
};

// ─── WASM-only entry points ───────────────────────────────────────────
//
// Everything below this line is gated to wasm32 because:
//
// - `wasm_bindgen::prelude::*` injects shims that only resolve when
//   building for `wasm32-unknown-unknown`.
// - `js_sys` likewise.
// - The native test suite calls `adapt::*` directly and never hits
//   these wrappers, so they're pure dead weight on `cargo test`.

#[cfg(target_arch = "wasm32")]
mod chain_bindings;

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use crate::adapt;
    // Re-export the chain entry points so they appear in the wasm-pack
    // output bundle alongside the compiler ones.
    #[allow(unused_imports)]
    pub use crate::chain_bindings::{
        chain_advance_time, chain_call, chain_deploy, chain_get_accounts, chain_get_contracts,
        chain_get_state, chain_get_storage, chain_get_tx_log, chain_init, chain_mine_blocks,
        chain_reset, chain_static_call,
    };

    /// Initialize the WASM module. Wires up the panic hook (when the
    /// `panic-hook` feature is enabled) so that Rust panics surface as
    /// readable JS exceptions in the browser console.
    #[wasm_bindgen(start)]
    pub fn init() {
        #[cfg(feature = "panic-hook")]
        console_error_panic_hook::set_once();
    }

    /// Compiler version, e.g. `"0.8.2"`.
    #[wasm_bindgen]
    pub fn version() -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    /// Compile a Covenant source string targeting EVM bytecode.
    ///
    /// Backward-compatible V0.8 entry point: defaults to `mockchain` target.
    /// New code should call [`compile_to_evm_for_target`] explicitly.
    ///
    /// Returns a JS object matching the `JsCompileResult` schema.
    /// Panics are caught by `console_error_panic_hook` and surface as
    /// JS exceptions; the playground catches those and shows a generic
    /// "internal compiler error" diagnostic.
    #[wasm_bindgen]
    pub fn compile_to_evm(source: &str) -> JsValue {
        let r = adapt::compile_evm(source);
        serde_wasm_bindgen::to_value(&r).unwrap_or(JsValue::NULL)
    }

    /// Compile a Covenant source string targeting EVM bytecode with an
    /// explicit chain target (V0.9, Sprint 31).
    ///
    /// `target` accepts:
    ///   - `"mockchain"` / `"mock"` / `"evm"` → V0.8 in-tab MockChain
    ///   - `"sepolia"`                        → V0.9 helpers on Sepolia
    ///   - `"aster_testnet"`                  → V0.9 helpers on Aster Testnet
    ///   - `"mainnet"`                        → REJECTED (single E601 diag)
    ///
    /// The playground's Chain Target dropdown should call this for any
    /// non-MockChain target; MockChain can use the parameterless
    /// [`compile_to_evm`] for backward compat.
    #[wasm_bindgen]
    pub fn compile_to_evm_for_target(source: &str, target: &str) -> JsValue {
        let r = adapt::compile_evm_for_target(source, target);
        serde_wasm_bindgen::to_value(&r).unwrap_or(JsValue::NULL)
    }

    /// Run only frontend stages (lex → parse → resolve → typecheck →
    /// privacy). Cheap enough for keystroke-rate calls; used by Monaco's
    /// live diagnostics.
    #[wasm_bindgen]
    pub fn check(source: &str) -> JsValue {
        let r = adapt::check_only(source);
        serde_wasm_bindgen::to_value(&r).unwrap_or(JsValue::NULL)
    }

    /// Compile up to IR construction and return the IR as printable text.
    /// Used by the Inspector and Layer Explorer panes.
    #[wasm_bindgen]
    pub fn compile_to_ir_text(source: &str) -> JsValue {
        let r = adapt::compile_ir(source);
        serde_wasm_bindgen::to_value(&r).unwrap_or(JsValue::NULL)
    }

    /// Diagnostic-code → prose-explanation table. Reserved surface;
    /// returns `[]` until the compiler's diagnostic registry exposes
    /// long-form explanations (filed in DEBT.md as "diagnostic prose
    /// registry").
    #[wasm_bindgen]
    pub fn diagnostic_explanations() -> JsValue {
        let entries = adapt::all_diagnostic_explanations();
        serde_wasm_bindgen::to_value(&entries).unwrap_or(JsValue::NULL)
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

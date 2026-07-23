//! JS-facing result types.
//!
//! These are pure `serde` shapes. Nothing wasm-bindgen-flavoured here —
//! the [`crate::lib`] entry points serialize them to `JsValue` via
//! `serde_wasm_bindgen::to_value`. That separation means every type in
//! this module is also returned directly from native `cargo test` runs,
//! so the test suite verifies the same shapes the browser sees.
//!
//! Naming convention: every public type is prefixed `Js` so a reader of
//! `crate::adapt` can tell at a glance which side of the bridge they're
//! on.

use serde::{Deserialize, Serialize};

/// Backend the playground asked us to compile against.
///
/// Today only `Evm` is supported. `Aster` and `Wasm` slots are reserved
/// for Sprint 25+ when those backends mature past placeholder.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CompileTarget {
    Evm,
}

/// Severity of a single diagnostic.
///
/// Mirrors `covenant_diag::DiagnosticLevel` but with a `Note → Info`
/// rename so the playground UI can use Monaco's standard severity
/// vocabulary without an extra mapping step.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum JsLevel {
    Error,
    Warning,
    Info,
}

/// One diagnostic, with line/column already resolved from the byte span.
///
/// The compiler's `Diagnostic` carries a half-open byte range
/// (`Span { start, end }`) — Monaco wants 1-indexed line/column, so the
/// adapter pre-computes both and passes them across the bridge already
/// resolved. The raw byte offsets are kept too for callers (e.g. the
/// Inspector) that want byte-precise highlights.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct JsDiagnostic {
    pub level: JsLevel,
    pub code: String, // e.g. "E042"
    pub message: String,
    pub help: Option<String>,
    pub line: u32,   // 1-indexed
    pub column: u32, // 1-indexed (UTF-8 chars, not bytes)
    pub end_line: u32,
    pub end_column: u32,
    pub span_start: u32, // raw byte offset, half-open
    pub span_end: u32,
}

/// Wall-clock breakdown of a single compile call.
///
/// Today we only expose `total`; future work can split it per-phase
/// (lex / parse / resolve / typecheck / privacy / ir / lower / opt / codegen)
/// without breaking the playground because new fields default to zero.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct JsTiming {
    pub total: f64, // milliseconds, browser high-res clock
}

/// Function selector entry — useful for the Output panel's "Functions"
/// tab so the user can see the 4-byte selector for each public action.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JsSelector {
    pub name: String,
    pub selector: String, // 0x-prefixed 8-hex-char string
}

/// One row of the contract storage layout.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JsStorageEntry {
    pub name: String,
    pub slot: String, // 0x-prefixed 32-byte slot
    pub offset: u8,
    pub size_bytes: u32,
    pub ty_desc: String,
}

/// Compilation metadata — the same fields the CLI prints in `metadata.json`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JsMetadata {
    pub covenant_version: String,
    pub optimizer_config: String,
    pub evm_version: String,
    pub erc_versions: serde_json::Map<String, serde_json::Value>,
    pub precompile_abi_version: u32,
}

/// One PC → source-line mapping. The Inspector renders one row per
/// entry in the disassembly pane and uses `(source_line, source_column)`
/// to drive its hover-bridge to Monaco.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JsMapping {
    pub pc: u32,
    pub source_line: u32,
    pub source_column: u32,
    pub instr_kind: String,
}

/// Full source map carried alongside an artifact.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct JsSourceMap {
    pub mappings: Vec<JsMapping>,
}

/// Result of a `compile_to_evm` call.
///
/// `ok = true` ⇔ the artifact compiled with no error-level diagnostics.
/// Warnings/info diagnostics are still surfaced. When `ok = false`,
/// `bytecode/abi/...` MAY still be `Some` if the compiler produced a
/// partial artifact (today it doesn't, but the shape is future-proof).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JsCompileResult {
    pub ok: bool,
    pub target: CompileTarget,

    // Artifact (None when compilation bailed before codegen).
    pub deploy_bytecode: Option<String>,  // 0x-prefixed hex
    pub runtime_bytecode: Option<String>, // 0x-prefixed hex
    pub abi: Option<String>,              // JSON-encoded by evm-backend
    pub function_selectors: Vec<JsSelector>,
    pub storage_layout: Vec<JsStorageEntry>,
    pub metadata: Option<JsMetadata>,
    pub source_map: Option<JsSourceMap>,

    // Always present.
    pub diagnostics: Vec<JsDiagnostic>,
    pub timing: JsTiming,
}

/// Result of `check()` — frontend-only pass.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JsCheckResult {
    pub diagnostics: Vec<JsDiagnostic>,
    pub timing: JsTiming,
}

/// Result of `compile_to_ir_text()`.
///
/// `ir_text` uses the IR module's `Debug` impl for v1 — the playground
/// shows it monospaced. A proper `Display` impl is a follow-up in the
/// IR crate (filed as DEBT — not Sprint 22 scope).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JsIrResult {
    pub ok: bool,
    pub ir_text: Option<String>,
    pub diagnostics: Vec<JsDiagnostic>,
    pub timing: JsTiming,
}

/// One row in the diagnostic explanations table — used by the
/// Inspector's "Why this error?" feature. Today the long form is
/// the same as the short form (the compiler doesn't yet ship prose
/// explanations); the field is reserved so the playground UI can
/// already render the panel.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JsDiagExplanation {
    pub code: String,
    pub short: String,
    pub long: String,
}

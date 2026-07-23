//! Adapter between the strongly-typed compiler pipeline and the
//! `serde`-friendly shapes the playground consumes.
//!
//! Everything here is `cfg`-agnostic — it builds and runs on native
//! (so `cargo test` exercises the same code path the WASM binary will)
//! and on `wasm32-unknown-unknown`. The only WASM-specific behaviour
//! lives in [`crate`] (`lib.rs`), which wraps these functions in
//! `#[wasm_bindgen]` exports.

use covenant_diag::{DiagCode, Diagnostic, DiagnosticLevel, SourceId};
use covenant_driver::{check as driver_check, compile as driver_compile, compile_to_ir};
use covenant_evm_backend::{EvmArtifact, EvmConfig, SourceMap as EvmSourceMap, Target};
use covenant_opt::OptimizerConfig;
use covenant_stdlib::StdlibConfig;

use crate::result::{
    CompileTarget, JsCheckResult, JsCompileResult, JsDiagExplanation, JsDiagnostic, JsIrResult,
    JsLevel, JsMapping, JsMetadata, JsSelector, JsSourceMap, JsStorageEntry, JsTiming,
};

// ─── Public entry points ──────────────────────────────────────────────

/// Run the full pipeline targeting EVM. Always returns a result —
/// success is conveyed via the `ok` field, never via panic.
///
/// Defaults to the V0.8-compatible MockChain target. Use [`compile_evm_for_target`]
/// to compile for Sepolia or AsterTestnet (V0.9 helper-contract addresses).
pub fn compile_evm(source: &str) -> JsCompileResult {
    compile_evm_for_target(source, "mockchain")
}

/// Run the full pipeline targeting EVM with an explicit chain target.
///
/// `target_str` accepts the same values as the CLI `--target-chain` flag:
///   - `mockchain` / `mock` / `evm`  → V0.8 MockChain precompile addresses
///   - `sepolia`                     → V0.9 helper contracts on Sepolia
///   - `aster_testnet`               → V0.9 helper contracts on Aster
///   - `mainnet`                     → REJECTED (returns a diagnostic)
///
/// Unknown target names produce a single Error-level diagnostic with code
/// `E601` and `ok = false`.
pub fn compile_evm_for_target(source: &str, target_str: &str) -> JsCompileResult {
    let started = now_ms();
    let source_id = SourceId::new(0);

    // Resolve target. Bad target → return a single diagnostic, no compile.
    let evm_config = match Target::parse(target_str) {
        Ok(t) => EvmConfig::for_target(t),
        Err(e) => {
            return JsCompileResult {
                ok: false,
                target: CompileTarget::Evm,
                deploy_bytecode: None,
                runtime_bytecode: None,
                abi: None,
                function_selectors: vec![],
                storage_layout: vec![],
                metadata: None,
                source_map: None,
                diagnostics: vec![JsDiagnostic {
                    level: JsLevel::Error,
                    code: "E601".to_string(),
                    message: format!("invalid target '{target_str}': {e}"),
                    help: Some("valid targets: mockchain, sepolia, aster_testnet".to_string()),
                    line: 1,
                    column: 1,
                    end_line: 1,
                    end_column: 1,
                    span_start: 0,
                    span_end: 0,
                }],
                timing: JsTiming {
                    total: elapsed_ms(started),
                },
            };
        }
    };

    let (artifact_opt, diags) = driver_compile(
        source,
        source_id,
        evm_config,
        StdlibConfig::default(),
        OptimizerConfig::default(),
    );

    let line_idx = LineIndex::new(source);
    let diagnostics = diags.iter().map(|d| adapt_diag(d, &line_idx)).collect();
    let ok = !has_errors(&diags);

    let elapsed_ms = elapsed_ms(started);
    let timing = JsTiming { total: elapsed_ms };

    match artifact_opt {
        Some(art) => package_artifact(art, diagnostics, timing, ok),
        None => JsCompileResult {
            ok: false,
            target: CompileTarget::Evm,
            deploy_bytecode: None,
            runtime_bytecode: None,
            abi: None,
            function_selectors: vec![],
            storage_layout: vec![],
            metadata: None,
            source_map: None,
            diagnostics,
            timing,
        },
    }
}

/// Frontend-only check (lex → parse → resolve → typecheck → privacy).
/// No backend, no codegen — cheap enough for keystroke-rate calls.
pub fn check_only(source: &str) -> JsCheckResult {
    let started = now_ms();
    let line_idx = LineIndex::new(source);
    let diags = driver_check(source, SourceId::new(0));

    JsCheckResult {
        diagnostics: diags.iter().map(|d| adapt_diag(d, &line_idx)).collect(),
        timing: JsTiming {
            total: elapsed_ms(started),
        },
    }
}

/// Compile through IR construction and return a printable representation.
pub fn compile_ir(source: &str) -> JsIrResult {
    let started = now_ms();
    let line_idx = LineIndex::new(source);

    match compile_to_ir(source, SourceId::new(0)) {
        Ok(ir) => JsIrResult {
            ok: true,
            // The IR crate doesn't yet ship a `Display` impl; using
            // `{:#?}` for v1. Filed in DEBT.md as "IR Display impl" —
            // when it lands, swap to `{}` here without changing the
            // playground.
            ir_text: Some(format!("{ir:#?}")),
            diagnostics: vec![],
            timing: JsTiming {
                total: elapsed_ms(started),
            },
        },
        Err(diags) => JsIrResult {
            ok: false,
            ir_text: None,
            diagnostics: diags.iter().map(|d| adapt_diag(d, &line_idx)).collect(),
            timing: JsTiming {
                total: elapsed_ms(started),
            },
        },
    }
}

/// Diagnostic explanations table.
///
/// The compiler doesn't yet ship per-code prose explanations, so we
/// surface only the codes that have actually appeared in any diagnostic
/// emitted during this process's lifetime — populated lazily as a
/// best-effort. The Inspector renders a "click for full explanation"
/// link that gracefully degrades to "no extra info available yet".
pub fn all_diagnostic_explanations() -> Vec<JsDiagExplanation> {
    // Reserved surface — the registry crate would supply these once
    // CR-DIAG-001 lands. Returning an empty vec keeps the playground
    // happy (it falls back to the inline `message + help` shown in the
    // diagnostic itself).
    vec![]
}

// ─── Internals ────────────────────────────────────────────────────────

fn package_artifact(
    art: EvmArtifact,
    diagnostics: Vec<JsDiagnostic>,
    timing: JsTiming,
    ok: bool,
) -> JsCompileResult {
    JsCompileResult {
        ok,
        target: CompileTarget::Evm,
        deploy_bytecode: Some(hex_encode(&art.deploy_bytecode)),
        runtime_bytecode: Some(hex_encode(&art.runtime_bytecode)),
        abi: Some(art.abi),
        function_selectors: adapt_selectors(&art.function_selectors),
        storage_layout: adapt_storage(&art.storage_layout),
        metadata: Some(adapt_metadata(&art.metadata)),
        source_map: art.source_map.as_ref().map(adapt_source_map),
        diagnostics,
        timing,
    }
}

fn adapt_diag(d: &Diagnostic, line_idx: &LineIndex) -> JsDiagnostic {
    let (line, column) = line_idx.line_col(d.span.start);
    let (end_line, end_column) = line_idx.line_col(d.span.end.saturating_sub(1).max(d.span.start));

    JsDiagnostic {
        level: match d.level {
            DiagnosticLevel::Error => JsLevel::Error,
            DiagnosticLevel::Warning => JsLevel::Warning,
            DiagnosticLevel::Note => JsLevel::Info,
        },
        code: format_code(d.code),
        message: d.message.clone(),
        help: d.help.clone(),
        line,
        column,
        end_line,
        end_column,
        span_start: d.span.start,
        span_end: d.span.end,
    }
}

fn adapt_selectors(selectors: &std::collections::BTreeMap<Box<str>, [u8; 4]>) -> Vec<JsSelector> {
    selectors
        .iter()
        .map(|(name, bytes)| JsSelector {
            name: name.to_string(),
            selector: format!(
                "0x{:02x}{:02x}{:02x}{:02x}",
                bytes[0], bytes[1], bytes[2], bytes[3]
            ),
        })
        .collect()
}

fn adapt_storage(layout: &covenant_evm_backend::StorageLayout) -> Vec<JsStorageEntry> {
    layout
        .entries
        .iter()
        .map(|e| JsStorageEntry {
            name: e.name.to_string(),
            slot: format!("0x{}", hex_encode_raw(&e.slot)),
            offset: e.offset,
            size_bytes: e.size_bytes,
            ty_desc: e.ty_desc.to_string(),
        })
        .collect()
}

fn adapt_metadata(m: &covenant_evm_backend::CompilationMetadata) -> JsMetadata {
    JsMetadata {
        covenant_version: m.covenant_version.to_string(),
        optimizer_config: m.optimizer_config.to_string(),
        evm_version: format!("{:?}", m.evm_version),
        erc_versions: m
            .erc_versions
            .iter()
            .map(|(k, v)| (k.to_string(), serde_json::Value::String(v.to_string())))
            .collect(),
        precompile_abi_version: m.precompile_abi_version,
    }
}

fn adapt_source_map(sm: &EvmSourceMap) -> JsSourceMap {
    // The compiler emits one entry per generated instruction. We have
    // no per-source line index here because spans already point into a
    // single buffer (SourceId::new(0)), so we recompute inline using a
    // local LineIndex over the source — but the source isn't carried
    // in the artifact. Workaround: we expose `source_line/column = 0`
    // when we can't resolve, and the playground synthesises with
    // its own line index from the source string the user typed.
    //
    // This is intentional — it means the bridge stays stateless and
    // the playground retains the canonical source text.
    JsSourceMap {
        mappings: sm
            .entries
            .iter()
            .map(|e| JsMapping {
                pc: e.offset,
                source_line: 0,
                source_column: 0,
                instr_kind: e.instr_kind.to_string(),
            })
            .collect(),
    }
}

fn has_errors(diags: &[Diagnostic]) -> bool {
    diags.iter().any(|d| d.level == DiagnosticLevel::Error)
}

fn format_code(code: DiagCode) -> String {
    format!("{code}")
}

/// Wall-clock milliseconds since epoch.
///
/// On native (`cargo test`), uses `std::time::SystemTime` which gives
/// us microsecond precision. On wasm32, falls back to `js_sys::Date::now`
/// because `std::time::Instant` is unsupported on
/// `wasm32-unknown-unknown` without a custom getrandom backend.
fn now_ms() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64() * 1000.0)
            .unwrap_or(0.0)
    }
}

fn elapsed_ms(started: f64) -> f64 {
    (now_ms() - started).max(0.0)
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(2 + bytes.len() * 2);
    out.push_str("0x");
    out.push_str(&hex_encode_raw(bytes));
    out
}

fn hex_encode_raw(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        // SAFETY: hex digits are always valid ASCII.
        const HEX: &[u8; 16] = b"0123456789abcdef";
        out.push(HEX[(*b >> 4) as usize] as char);
        out.push(HEX[(*b & 0x0f) as usize] as char);
    }
    out
}

// ─── Line index ───────────────────────────────────────────────────────

/// O(log N) byte-offset → 1-indexed (line, column) lookup.
///
/// Built once per compile call. A `Vec<u32>` of newline byte positions
/// plus a binary search beats a per-diagnostic linear scan when a contract
/// emits many diagnostics (hello-world: 0, broken contracts can hit dozens).
/// Column is 1-indexed in UTF-8 chars (not bytes), matching Monaco's API;
/// the conversion respects multibyte UTF-8.
struct LineIndex {
    /// Byte offset of the start of each line. `line_starts[0] = 0`,
    /// `line_starts[N]` is just past the last byte (sentinel).
    line_starts: Vec<u32>,
    source: String,
}

impl LineIndex {
    fn new(source: &str) -> Self {
        let mut line_starts = vec![0u32];
        for (i, b) in source.bytes().enumerate() {
            if b == b'\n' {
                let next = (i as u32).saturating_add(1);
                line_starts.push(next);
            }
        }
        // Sentinel so `line_col(byte_offset)` past EOF still yields a
        // sensible answer (last line, last column).
        line_starts.push(source.len() as u32);
        Self {
            line_starts,
            source: source.to_string(),
        }
    }

    /// Returns 1-indexed `(line, column)` for the byte offset.
    /// Column is in UTF-8 *characters*, not bytes.
    fn line_col(&self, byte_offset: u32) -> (u32, u32) {
        // Binary search the largest line_starts[i] <= byte_offset.
        let line_idx = match self.line_starts.binary_search(&byte_offset) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let line_start = self.line_starts[line_idx] as usize;
        let offset = (byte_offset as usize).min(self.source.len());

        // Count UTF-8 chars between line_start and offset.
        let col_chars = self
            .source
            .get(line_start..offset)
            .map(|s| s.chars().count())
            .unwrap_or(0);

        // 1-indexed line and column.
        ((line_idx as u32) + 1, (col_chars as u32) + 1)
    }
}

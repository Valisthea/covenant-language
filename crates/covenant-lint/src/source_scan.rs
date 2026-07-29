//! Source-level anti-pattern scanner (V0.9 Sprint 39 Phase 39.1).
//!
//! The existing `framework::Detector` trait operates on `IrModule`:
//! which means the source must compile through lex → parse → resolve →
//! typecheck → privacy → IR-build before any detector runs. That's the
//! right place for security analyses (you need types and control-flow),
//! but it's the WRONG place for catching pseudo-Solidity / migration
//! antipatterns: those typically *fail compilation* at the parser, so
//! the IR is never produced and the detector never runs.
//!
//! `source_scan` fills the gap. It runs on raw source text BEFORE the
//! parser, finds anti-patterns via simple substring matches (no regex
//! dep), and emits `Finding`s with a quick-fix hint suitable for an
//! LSP code action.
//!
//! Sprint 39 ships 6 high-frequency anti-patterns (the PRELIM-009
//! greatest hits). The set is append-only: adding a new rule is safe;
//! removing one risks breaking outstanding LSP code-action references.

use covenant_diag::{SourceId, Span};

const SRC: SourceId = SourceId::new(0);
const fn span_at(start: u32, end: u32) -> Span {
    Span::new(SRC, start, end)
}

use crate::framework::{Finding, Severity};

/// Lint codes for the source-scan rules. The `L` prefix distinguishes
/// from compiler error codes (E***) and security codes (C***).
pub const L001_SLASH_SLASH_COMMENT: &str = "L001";
pub const L002_MAPPING_KEYWORD: &str = "L002";
pub const L003_FUNCTION_KEYWORD: &str = "L003";
pub const L004_REQUIRE_FUNCTION: &str = "L004";
pub const L005_UINT256_TYPE: &str = "L005";
pub const L006_STRING_TYPE: &str = "L006";

/// Run the source-scan pass on raw text. Returns one `Finding` per
/// anti-pattern occurrence. Findings are produced even when the source
/// is unparseable: that's the whole point of this pass.
pub fn scan(source: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    out.extend(scan_slash_slash_comments(source));
    out.extend(scan_mapping_keyword(source));
    out.extend(scan_function_keyword(source));
    out.extend(scan_require_function(source));
    out.extend(scan_solidity_types(
        source,
        "uint256",
        L005_UINT256_TYPE,
        "Solidity `uint256` is not a Covenant type. Use `amount` instead, \
         it's the canonical 256-bit unsigned integer in Covenant.",
        "rename `uint256` → `amount`",
    ));
    out.extend(scan_solidity_types(
        source,
        "string",
        L006_STRING_TYPE,
        "Solidity `string` is not a Covenant type. Use `text` instead, \
         the same UTF-8 semantics, renamed for consistency with Covenant's \
         semantic-type discipline.",
        "rename `string` → `text`",
    ));
    out
}

// ─── L001: `//` line comments ──────────────────────────────────────────
//
// Covenant uses `--` for line comments, not `//`. Same root issue as the
// lexer's E003 but with quick-fix metadata for IDE code actions.

fn scan_slash_slash_comments(source: &str) -> Vec<Finding> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        // Skip strings and char literals to avoid false positives on a
        // `//` that's inside a string.
        if bytes[i] == b'"' {
            i = skip_string(source, i);
            continue;
        }
        if bytes[i] == b'/' && bytes[i + 1] == b'/' {
            // Find end of line.
            let mut j = i + 2;
            while j < bytes.len() && bytes[j] != b'\n' {
                j += 1;
            }
            out.push(
                Finding::new(
                    L001_SLASH_SLASH_COMMENT,
                    span_at(i as u32, j as u32),
                    "Covenant uses `--` for line comments, not `//`. \
                     This is intentional: the language deliberately \
                     diverges from C-family syntax.",
                    Severity::Warning,
                )
                .with_help("replace `//` with `--`"),
            );
            i = j;
            continue;
        }
        i += 1;
    }
    out
}

fn skip_string(source: &str, start: usize) -> usize {
    let bytes = source.as_bytes();
    if start >= bytes.len() || bytes[start] != b'"' {
        return start + 1;
    }
    let mut j = start + 1;
    while j < bytes.len() {
        if bytes[j] == b'\\' && j + 1 < bytes.len() {
            j += 2;
            continue;
        }
        if bytes[j] == b'"' {
            return j + 1;
        }
        j += 1;
    }
    j
}

// ─── L002: `mapping(K => V)` Solidity-ism ─────────────────────────────

fn scan_mapping_keyword(source: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    for occ in find_word_occurrences(source, "mapping") {
        // Require it to be followed by `(` (with optional whitespace) to
        // distinguish from an identifier coincidentally named "mapping".
        let after = &source[occ.end..];
        let after_trim = after.trim_start();
        if !after_trim.starts_with('(') {
            continue;
        }
        out.push(
            Finding::new(
                L002_MAPPING_KEYWORD,
                span_at(occ.start as u32, occ.end as u32),
                "Solidity `mapping(K => V)` is not Covenant syntax. \
                 Use `map<K, V>` instead: same semantics, generics-style.",
                Severity::Warning,
            )
            .with_help("replace with `field name: map<K, V>`"),
        );
    }
    out
}

// ─── L003: `function name() { … }` Solidity-ism ───────────────────────

fn scan_function_keyword(source: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    for occ in find_word_occurrences(source, "function") {
        out.push(
            Finding::new(
                L003_FUNCTION_KEYWORD,
                span_at(occ.start as u32, occ.end as u32),
                "Solidity `function` is not a Covenant keyword. Use \
                 `action` for state-mutating functions or `view` for \
                 read-only queries: the type system tracks them \
                 separately.",
                Severity::Warning,
            )
            .with_help("replace `function` with `action` (mutating) or `view` (read-only)"),
        );
    }
    out
}

// ─── L004: `require(cond, "msg")` Solidity-ism ────────────────────────

fn scan_require_function(source: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    for occ in find_word_occurrences(source, "require") {
        let after = &source[occ.end..];
        let after_trim = after.trim_start();
        if !after_trim.starts_with('(') {
            continue;
        }
        out.push(
            Finding::new(
                L004_REQUIRE_FUNCTION,
                span_at(occ.start as u32, occ.end as u32),
                "Solidity `require(cond, \"msg\")` is not Covenant. Use \
                 a `when cond` guard on the action signature, or call \
                 `revert_with ErrorName(args)` inside the body.",
                Severity::Warning,
            )
            .with_help("replace with `when cond` guard or `revert_with ErrorName()`"),
        );
    }
    out
}

// ─── L005/L006: `uint256` / `string` Solidity types ────────────────────

fn scan_solidity_types(
    source: &str,
    needle: &'static str,
    code: &'static str,
    msg: &'static str,
    help: &'static str,
) -> Vec<Finding> {
    let mut out = Vec::new();
    for occ in find_word_occurrences(source, needle) {
        out.push(
            Finding::new(
                code,
                span_at(occ.start as u32, occ.end as u32),
                msg,
                Severity::Warning,
            )
            .with_help(help),
        );
    }
    out
}

// ─── Helpers ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
struct Occurrence {
    start: usize,
    end: usize,
}

/// Find all occurrences of `needle` as a complete word in `source`.
/// Word boundary = non-(alphanumeric + underscore). Skips occurrences
/// inside string literals (rough heuristic).
fn find_word_occurrences(source: &str, needle: &str) -> Vec<Occurrence> {
    let mut out = Vec::new();
    let mut search_from = 0usize;
    let bytes = source.as_bytes();
    let in_string = |idx: usize| -> bool {
        // Quick scan: count unescaped `"` before idx; odd = inside string.
        let mut count = 0u32;
        let mut k = 0usize;
        while k < idx {
            if bytes[k] == b'\\' {
                k += 2;
                continue;
            }
            if bytes[k] == b'"' {
                count += 1;
            }
            k += 1;
        }
        count % 2 == 1
    };

    while let Some(rel) = source[search_from..].find(needle) {
        let start = search_from + rel;
        let end = start + needle.len();

        // Word-boundary check: must not be preceded/followed by ID char.
        let pre_ok = start == 0 || !is_id_char(bytes[start - 1]);
        let post_ok = end >= bytes.len() || !is_id_char(bytes[end]);
        if pre_ok && post_ok && !in_string(start) {
            out.push(Occurrence { start, end });
        }

        search_from = start + 1;
    }
    out
}

fn is_id_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_slash_slash_comment() {
        let src = "// this is a Solidity comment\nrecord Hello {}";
        let findings = scan(src);
        assert!(findings
            .iter()
            .any(|f| f.detector_code == L001_SLASH_SLASH_COMMENT));
    }

    #[test]
    fn does_not_detect_slash_slash_inside_string() {
        let src = r#"record Hello { greeting: text = "see // for examples" }"#;
        let findings = scan(src);
        assert!(!findings
            .iter()
            .any(|f| f.detector_code == L001_SLASH_SLASH_COMMENT));
    }

    #[test]
    fn detects_mapping_keyword() {
        let src = "module M { field bal: mapping(address => amount) }";
        let findings = scan(src);
        assert!(findings
            .iter()
            .any(|f| f.detector_code == L002_MAPPING_KEYWORD));
    }

    #[test]
    fn does_not_detect_mapping_as_identifier() {
        let src = "record M { mapping_status: bool }";
        let findings = scan(src);
        assert!(!findings
            .iter()
            .any(|f| f.detector_code == L002_MAPPING_KEYWORD));
    }

    #[test]
    fn detects_function_keyword() {
        let src = "module M { function transfer() {} }";
        let findings = scan(src);
        assert!(findings
            .iter()
            .any(|f| f.detector_code == L003_FUNCTION_KEYWORD));
    }

    #[test]
    fn detects_require_function_call() {
        let src = "action transfer() { require(value > 0, \"zero\") }";
        let findings = scan(src);
        assert!(findings
            .iter()
            .any(|f| f.detector_code == L004_REQUIRE_FUNCTION));
    }

    #[test]
    fn detects_uint256_type() {
        let src = "field bal: uint256";
        let findings = scan(src);
        assert!(findings
            .iter()
            .any(|f| f.detector_code == L005_UINT256_TYPE));
    }

    #[test]
    fn detects_string_type() {
        let src = "field name: string = \"foo\"";
        let findings = scan(src);
        assert!(findings.iter().any(|f| f.detector_code == L006_STRING_TYPE));
    }

    #[test]
    fn clean_covenant_source_produces_no_findings() {
        let src = "record Hello { greeting: text\n action update(x: text) { greeting = x } \
                   view read returns text { greeting } }";
        let findings = scan(src);
        assert!(
            findings.is_empty(),
            "clean source should not trip source-scan: {:?}",
            findings.iter().map(|f| f.detector_code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn each_finding_has_help() {
        let src = "// bad\nfield m: mapping(a => b)\nfield bal: uint256";
        let findings = scan(src);
        for f in &findings {
            assert!(
                f.help.is_some(),
                "{} should have a quick-fix help",
                f.detector_code
            );
        }
    }
}

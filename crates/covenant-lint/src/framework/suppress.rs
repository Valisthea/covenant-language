//! Suppression annotation parsing.
//!
//! `@allow(C001, reason: "...")` in Covenant source suppresses finding C001 on the
//! annotated item. Because `IrAnnotation::Unknown` discards annotation arguments,
//! we do a lightweight text scan of the raw source to discover suppression codes.

use std::collections::HashSet;

/// Parse all `@allow(CODE, ...)` annotations from raw source and return the set
/// of suppressed detector codes.
///
/// This is intentionally global (not position-sensitive) for Session 1 simplicity.
/// Session 3 will add position-sensitive suppression.
pub fn parse_suppressions(source: &str) -> HashSet<String> {
    let mut codes = HashSet::new();
    let mut rest = source;

    while let Some(pos) = rest.find("@allow(") {
        let after = &rest[pos + 7..];
        let end = after.find(')').unwrap_or(after.len());
        let inside = &after[..end];
        // inside looks like: "C001, reason: \"...\""
        // The first token before ',' or ')' is the code.
        let code_end = inside.find([',', ')']).unwrap_or(inside.len());
        let code = inside[..code_end].trim();
        if !code.is_empty() {
            codes.insert(code.to_string());
        }
        rest = &rest[pos + 7..];
    }

    codes
}

/// Filter findings, removing any whose detector code appears in `suppressions`.
pub fn apply_suppressions(
    findings: Vec<crate::framework::Finding>,
    suppressions: &HashSet<String>,
) -> Vec<crate::framework::Finding> {
    findings
        .into_iter()
        .filter(|f| !suppressions.contains(f.detector_code))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_allow() {
        let src = r#"@allow(C001, reason: "intentional")"#;
        let s = parse_suppressions(src);
        assert!(s.contains("C001"));
    }

    #[test]
    fn parse_multiple_allows() {
        let src = "@allow(C001, reason: \"r1\")\n@allow(W003, reason: \"r2\")";
        let s = parse_suppressions(src);
        assert!(s.contains("C001"));
        assert!(s.contains("W003"));
    }

    #[test]
    fn no_allow_empty_set() {
        let src = "module Foo { field x: amount }";
        let s = parse_suppressions(src);
        assert!(s.is_empty());
    }
}

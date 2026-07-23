//! Tests for the @allow suppression mechanism.

use covenant_diag::SourceId;
use covenant_lint::framework::{apply_suppressions, parse_suppressions};
use covenant_lint::lint_source;

// ── parse_suppressions ───────────────────────────────────────────────────────

#[test]
fn suppress_single_code() {
    let src = r#"@allow(C001, reason: "intentional")"#;
    let s = parse_suppressions(src);
    assert!(s.contains("C001"));
}

#[test]
fn suppress_multiple_codes() {
    let src = "@allow(C001)\n@allow(W003, reason: \"safe\")";
    let s = parse_suppressions(src);
    assert!(s.contains("C001"));
    assert!(s.contains("W003"));
}

#[test]
fn suppress_empty_source() {
    let s = parse_suppressions("record R {}");
    assert!(s.is_empty());
}

#[test]
fn suppress_code_without_reason() {
    let src = "@allow(C100)";
    let s = parse_suppressions(src);
    assert!(s.contains("C100"));
}

// ── apply_suppressions ───────────────────────────────────────────────────────

#[test]
fn apply_removes_suppressed_findings() {
    use covenant_diag::Span;
    use covenant_lint::framework::{Finding, Severity};
    use std::collections::HashSet;

    let sid = SourceId::new(0);
    let span = Span::new(sid, 0, 1);

    let findings = vec![
        Finding::new("C001", span, "reentrancy", Severity::Critical),
        Finding::new("W003", span, "missing guard", Severity::Warning),
    ];

    let mut suppressed = HashSet::new();
    suppressed.insert("C001".to_string());

    let after = apply_suppressions(findings, &suppressed);
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].detector_code, "W003");
}

#[test]
fn apply_keeps_all_when_no_suppression() {
    use covenant_diag::Span;
    use covenant_lint::framework::{Finding, Severity};
    use std::collections::HashSet;

    let sid = SourceId::new(0);
    let span = Span::new(sid, 0, 1);
    let findings = vec![Finding::new("C001", span, "msg", Severity::Critical)];
    let after = apply_suppressions(findings, &HashSet::new());
    assert_eq!(after.len(), 1);
}

// ── lint_source integration ──────────────────────────────────────────────────

#[test]
fn lint_source_suppresses_with_allow() {
    // W003 would fire (transfer without @non_reentrant), but @allow(W003) suppresses it.
    let src = r#"
@allow(W003, reason: "safe")
record R {
    action pay(to: address) {
        transfer 10 to to
    }
}
"#;
    let findings = lint_source(src, SourceId::new(0));
    assert!(
        findings.iter().all(|f| f.detector_code != "W003"),
        "W003 should be suppressed: {findings:?}"
    );
}

#[test]
fn lint_source_clean_contract_no_findings() {
    let src = r#"
record R {
    x: amount
    view get returns amount { x }
}
"#;
    let findings = lint_source(src, SourceId::new(0));
    // A simple read-only contract should produce no Critical/Warning findings.
    let high = findings
        .iter()
        .filter(|f| {
            f.severity == covenant_lint::framework::Severity::Critical
                || f.severity == covenant_lint::framework::Severity::Warning
        })
        .count();
    assert_eq!(high, 0, "unexpected high-severity findings: {findings:?}");
}

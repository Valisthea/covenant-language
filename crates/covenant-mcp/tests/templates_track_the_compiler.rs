//! Every scaffold template must be accepted by the compiler it ships beside.
//!
//! The MCP server was built against V0.8 and kept shipping while the language
//! moved. Three of its fourteen templates taught syntax the compiler no longer
//! parsed, and nothing noticed: `scaffold` handed back a program that the
//! server's own `check_syntax` tool then rejected. It surfaced only when
//! someone used the two tools in sequence.
//!
//! These tests are the missing link. A template that stops parsing fails the
//! build instead of reaching a user.

use covenant_diag::{DiagnosticLevel, SourceId};
use covenant_mcp::templates;

/// Every construct keyword `scaffold` advertises. Kept as a literal list
/// rather than read from the directory so that deleting a template file is a
/// test failure rather than a silently shorter run.
const CONSTRUCTS: &[&str] = &[
    "record",
    "token",
    "confidential token",
    "ballot",
    "counter",
    "encrypted counter",
    "board",
    "market",
    "vault",
    "registry",
    "bridge",
    "ceremony",
    "module",
    "hybrid module",
];

fn errors_in(source: &str) -> Vec<String> {
    covenant_driver::check(source, SourceId::new(0))
        .into_iter()
        .filter(|d| d.level == DiagnosticLevel::Error)
        .map(|d| format!("[E{}] {}", d.code.0, d.message))
        .collect()
}

#[test]
fn every_advertised_construct_has_a_template() {
    let missing: Vec<_> = CONSTRUCTS
        .iter()
        .filter(|c| templates::render(c, "Sample").is_none())
        .collect();
    assert!(
        missing.is_empty(),
        "scaffold advertises constructs with no template: {missing:?}"
    );
}

#[test]
fn every_template_is_accepted_by_this_compiler() {
    let mut broken = Vec::new();
    for construct in CONSTRUCTS {
        let source = templates::render(construct, "Sample")
            .unwrap_or_else(|| panic!("no template for `{construct}`"));
        let errors = errors_in(&source);
        if !errors.is_empty() {
            broken.push(format!("`{construct}`:\n    {}", errors.join("\n    ")));
        }
    }
    assert!(
        broken.is_empty(),
        "scaffold would hand the user a program this compiler rejects:\n  {}",
        broken.join("\n  ")
    );
}

/// The placeholder has to actually be substituted. A template that spells it
/// `{{ NAME }}` or `{{Name}}` renders into a program containing braces, which
/// fails to parse for a reason that reads like a language bug.
#[test]
fn no_template_leaks_its_placeholder() {
    for construct in CONSTRUCTS {
        let rendered = templates::render(construct, "Sample").unwrap();
        assert!(
            !rendered.contains("{{"),
            "`{construct}` still contains an unsubstituted placeholder after rendering"
        );
        assert!(
            rendered.contains("Sample"),
            "`{construct}` never used the name it was given"
        );
    }
}

/// A negative control. If `check` stopped reporting errors the two tests above
/// would pass on anything, including the templates that were actually broken.
#[test]
fn the_check_used_above_does_reject_bad_input() {
    let errors = errors_in("token Sample { this is not covenant }");
    assert!(
        !errors.is_empty(),
        "check accepted nonsense, so the template tests prove nothing"
    );
}

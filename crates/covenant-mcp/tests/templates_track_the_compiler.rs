//! Every scaffold template must survive the compiler it ships beside.
//!
//! The MCP server was built against V0.8 and kept shipping while the language
//! moved. Three of its fourteen templates taught syntax the compiler no longer
//! parsed, and nothing noticed: `scaffold` handed back a program that the
//! server's own `check_syntax` tool then rejected.
//!
//! The first version of this file checked the templates with
//! `covenant_driver::check`, which stops after the type checker. That is the
//! same mistake one level down. Three templates passed `check` and failed
//! `build`, because the diagnostics that reject them live in the IR and the
//! backend: `ballot` on E518, `board` on E430 and E431, `registry` on E505.
//! A scaffold that type-checks and then refuses to produce bytecode is not a
//! working scaffold. These tests run the whole pipeline.

use covenant_diag::{DiagnosticLevel, SourceId};
use covenant_evm_backend::EvmConfig;
use covenant_mcp::templates;
use covenant_opt::OptimizerConfig;
use covenant_stdlib::StdlibConfig;

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

/// Constructs that cannot be compiled at this release, whatever you write.
///
/// This is a ratchet, not an excuse list. Adding an entry means stating the
/// diagnostic and having proved the construct is unbuildable in every form,
/// and `an_unbuildable_construct_is_really_unbuildable` re-proves it on every
/// run so an entry cannot outlive the limitation that justified it.
const UNBUILDABLE: &[(&str, &str)] = &[(
    "registry",
    "the ERC-8231 synthesizer injects register/key_of over pq_key, whose ABI \
     type is dynamic bytes, and the backend refuses it with E505",
)];

fn is_unbuildable(construct: &str) -> bool {
    UNBUILDABLE.iter().any(|(c, _)| *c == construct)
}

/// Run the full pipeline, lexer through EVM backend, and return the errors.
fn build_errors(source: &str) -> Vec<String> {
    let (_artifact, diagnostics) = covenant_driver::compile(
        source,
        SourceId::new(0),
        EvmConfig::default(),
        StdlibConfig::default(),
        OptimizerConfig::default(),
    );
    diagnostics
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
fn every_buildable_template_actually_builds() {
    let mut broken = Vec::new();
    for construct in CONSTRUCTS {
        if is_unbuildable(construct) {
            continue;
        }
        let source = templates::render(construct, "Sample")
            .unwrap_or_else(|| panic!("no template for `{construct}`"));
        let errors = build_errors(&source);
        if !errors.is_empty() {
            broken.push(format!("`{construct}`:\n    {}", errors.join("\n    ")));
        }
    }
    assert!(
        broken.is_empty(),
        "scaffold would hand the user a program this compiler refuses to \
         build:\n  {}",
        broken.join("\n  ")
    );
}

/// The ratchet's other half. If a construct on the unbuildable list starts
/// building, the list is stale and the template should be promoted back into
/// the checked set rather than left excused forever.
#[test]
fn an_unbuildable_construct_is_really_unbuildable() {
    for (construct, reason) in UNBUILDABLE {
        let source = templates::render(construct, "Sample")
            .unwrap_or_else(|| panic!("no template for `{construct}`"));
        let errors = build_errors(&source);
        assert!(
            !errors.is_empty(),
            "`{construct}` is on the unbuildable list ({reason}) but it now \
             builds. Remove the entry so it is checked like the others."
        );
    }
}

/// A template that cannot build must say so in its own text, because the user
/// reads the scaffold output long before they read this test.
#[test]
fn an_unbuildable_template_warns_in_its_own_body() {
    for (construct, _) in UNBUILDABLE {
        let rendered = templates::render(construct, "Sample").unwrap();
        let head: String = rendered.lines().take(12).collect::<Vec<_>>().join("\n");
        assert!(
            head.to_ascii_uppercase().contains("DOES NOT BUILD"),
            "`{construct}` does not build, but its template header never says \
             so, so scaffold hands over a broken file with no warning"
        );
    }
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

/// A negative control. If the pipeline stopped reporting errors, the tests
/// above would pass on anything, including the templates that were broken.
#[test]
fn the_build_used_above_does_reject_bad_input() {
    assert!(
        !build_errors("token Sample { this is not covenant }").is_empty(),
        "the pipeline accepted nonsense, so the template tests prove nothing"
    );
    // And specifically at the backend, which is the stage `check` skipped.
    let backend_only = "\
market Sample {
    field bids: priority_queue<amount, address, max>
    action place(price: amount) { bids.push(price, caller) }
}";
    assert!(
        !build_errors(backend_only).is_empty(),
        "a program that only fails past the type checker was accepted, so \
         these tests are still only checking the frontend"
    );
}

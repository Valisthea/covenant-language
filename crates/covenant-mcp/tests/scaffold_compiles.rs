use covenant_diag::{DiagnosticLevel, SourceId};
use covenant_driver::check;
use covenant_mcp::templates;

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

#[test]
fn all_templates_render() {
    for &construct in CONSTRUCTS {
        let source = templates::render(construct, "TestContract")
            .unwrap_or_else(|| panic!("template missing for '{construct}'"));
        assert!(
            source.contains("TestContract"),
            "template for '{construct}' does not contain the substituted name"
        );
    }
}

#[test]
fn simple_templates_pass_check() {
    for &construct in &[
        "record",
        "token",
        "confidential token",
        "ballot",
        "counter",
        "module",
    ] {
        let source = templates::render(construct, "Tc")
            .unwrap_or_else(|| panic!("template missing for '{construct}'"));
        let diags = check(&source, SourceId::new(0));
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.level == DiagnosticLevel::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "template '{construct}' has compiler errors: {errors:?}"
        );
    }
}

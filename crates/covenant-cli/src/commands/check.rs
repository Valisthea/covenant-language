//! `covenant check`: frontend-only compilation (no codegen).

use std::path::PathBuf;

use clap::Args;
use covenant_diag::{DiagnosticLevel, SourceId};
use covenant_driver::check as driver_check;

use crate::{diagnostics::emit_diagnostics, error::CliError, output::OutputFormat};

#[derive(Args, Debug, Clone)]
pub struct CheckArgs {
    /// Source file to check, or omit for project mode.
    #[arg(value_name = "TARGET")]
    pub target: Option<PathBuf>,
}

pub fn run(
    args: &CheckArgs,
    manifest_path: Option<&PathBuf>,
    format: OutputFormat,
    use_color: bool,
) -> Result<(), CliError> {
    let files = resolve_targets(&args.target, manifest_path)?;

    let mut has_errors = false;
    for file in &files {
        let source = std::fs::read_to_string(file).map_err(|e| {
            CliError::Io(std::io::Error::new(
                e.kind(),
                format!("cannot read `{}`: {e}", file.display()),
            ))
        })?;

        let diags = driver_check(&source, SourceId::new(0));
        let errors_here = diags.iter().any(|d| d.level == DiagnosticLevel::Error);
        has_errors = has_errors || errors_here;

        let path_str = file.to_string_lossy();
        emit_diagnostics(&diags, &path_str, &source, format, use_color);

        if format == OutputFormat::Json {
            let error_count = diags
                .iter()
                .filter(|d| d.level == DiagnosticLevel::Error)
                .count();
            let warn_count = diags
                .iter()
                .filter(|d| d.level == DiagnosticLevel::Warning)
                .count();
            println!(
                "{}",
                serde_json::json!({
                    "file": path_str,
                    "status": if errors_here { "error" } else { "ok" },
                    "errors": error_count,
                    "warnings": warn_count,
                })
            );
        } else if !errors_here {
            let warn_count = diags
                .iter()
                .filter(|d| d.level == DiagnosticLevel::Warning)
                .count();
            if warn_count == 0 {
                println!("ok: {}", file.display());
            } else {
                println!("ok ({}): {} warning(s)", file.display(), warn_count);
            }
        }
    }

    if has_errors {
        Err(CliError::CompileError)
    } else {
        Ok(())
    }
}

/// Resolve what files to check.
///
/// If a target file is given, use that. Otherwise look for a manifest and use
/// the entrypoints declared there.
fn resolve_targets(
    target: &Option<PathBuf>,
    manifest_path: Option<&PathBuf>,
) -> Result<Vec<PathBuf>, CliError> {
    // Single-file mode
    if let Some(t) = target {
        if t.extension().is_some_and(|e| e == "cov") {
            return Ok(vec![t.clone()]);
        }
    }

    // Project mode
    let manifest_file = manifest_path
        .cloned()
        .or_else(|| covenant_manifest::Manifest::find_upward(&std::env::current_dir().unwrap()))
        .ok_or(CliError::ManifestNotFound)?;

    let manifest = covenant_manifest::Manifest::read(&manifest_file)?;
    let project_root = manifest_file.parent().unwrap().to_path_buf();
    let src_root = manifest
        .project
        .source
        .root
        .clone()
        .unwrap_or_else(|| PathBuf::from("src"));
    let entrypoints = manifest
        .project
        .source
        .entrypoints
        .clone()
        .unwrap_or_else(|| vec![PathBuf::from("main.cov")]);

    Ok(entrypoints
        .into_iter()
        .map(|ep| project_root.join(&src_root).join(ep))
        .collect())
}

/// Check source text directly (for use in unit tests).
#[allow(dead_code)]
pub fn check_source(source: &str, path: &str) -> CheckResult {
    let diags = driver_check(source, SourceId::new(0));
    let errors = diags
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Error)
        .count();
    let warnings = diags
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Warning)
        .count();
    CheckResult {
        errors,
        warnings,
        path: path.to_owned(),
        source: source.to_owned(),
        diags,
    }
}

#[allow(dead_code)]
pub struct CheckResult {
    pub errors: usize,
    pub warnings: usize,
    pub path: String,
    pub source: String,
    pub diags: Vec<covenant_diag::Diagnostic>,
}

#[allow(dead_code)]
impl CheckResult {
    pub fn has_errors(&self) -> bool {
        self.errors > 0
    }
    pub fn emit(&self, format: OutputFormat, use_color: bool) {
        emit_diagnostics(&self.diags, &self.path, &self.source, format, use_color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD_SOURCE: &str =
        include_str!("../../../covenant-lexer/tests/fixtures/example_01_hello.cov");

    const BAD_SOURCE: &str = r#"
module Bad {
    field x: amount
    action bork() { x = "not_an_amount" }
}
"#;

    #[test]
    fn check_good_source_no_errors() {
        let r = check_source(GOOD_SOURCE, "hello.cov");
        assert_eq!(r.errors, 0, "good source should have no errors");
    }

    #[test]
    fn check_bad_source_has_errors() {
        let r = check_source(BAD_SOURCE, "bad.cov");
        assert!(r.has_errors(), "bad source should have errors");
    }

    #[test]
    fn resolve_targets_with_explicit_file() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/covenant-lexer/tests/fixtures/example_01_hello.cov");
        let result = resolve_targets(&Some(fixture.clone()), None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), vec![fixture]);
    }
}

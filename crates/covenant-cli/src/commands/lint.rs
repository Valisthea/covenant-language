//! `covenant lint` — run the security linter on .cov source files.

use std::path::PathBuf;

use clap::Args;
use covenant_lint::cli::Cli as LintCli;

use crate::error::CliError;

#[derive(Args, Debug, Clone)]
pub struct LintArgs {
    /// Source files to lint (.cov).
    pub paths: Vec<PathBuf>,

    /// Enable deep / expensive detectors.
    #[arg(long)]
    pub deep: bool,

    /// Filter detectors by code, name, or category prefix.
    #[arg(long, value_name = "PATTERN")]
    pub filter: Option<String>,

    /// Minimum severity to report: info, warning, critical.
    #[arg(long, value_name = "LEVEL", default_value = "info")]
    pub severity: String,

    /// Suppress a detector code globally (may be repeated).
    #[arg(long = "allow", value_name = "CODE")]
    pub allow: Vec<String>,

    /// Elevate a detector code to Critical (may be repeated).
    #[arg(long = "deny", value_name = "CODE")]
    pub deny: Vec<String>,
}

pub fn run(
    args: &LintArgs,
    format: crate::output::OutputFormat,
    use_color: bool,
) -> Result<(), CliError> {
    let fmt_str = match format {
        crate::output::OutputFormat::Human => "human",
        crate::output::OutputFormat::Json => "json",
    };

    let lint_args = LintCli {
        paths: args.paths.clone(),
        deep: args.deep,
        filter: args.filter.clone(),
        severity: args.severity.clone(),
        format: fmt_str.to_string(),
        allow: args.allow.clone(),
        deny: args.deny.clone(),
        color: if use_color {
            "always".to_string()
        } else {
            "never".to_string()
        },
    };

    match covenant_lint::run(&lint_args) {
        Ok(0) => Ok(()),
        Ok(_) => Err(CliError::LintError),
        Err(e) => {
            eprintln!("covenant lint: {e:#}");
            Err(CliError::LintError)
        }
    }
}

/// Run the linter on a single source file for `covenant build --release`.
/// Returns `true` if any Critical findings were found (build should be blocked).
pub fn lint_for_release(source_path: &std::path::Path) -> bool {
    let lint_args = LintCli {
        paths: vec![source_path.to_path_buf()],
        deep: false,
        filter: None,
        severity: "critical".to_string(),
        format: "human".to_string(),
        allow: vec![],
        deny: vec![],
        color: "auto".to_string(),
    };
    matches!(covenant_lint::run(&lint_args), Ok(code) if code > 0)
}

//! Command-line interface definition for `covenant-lint`.

use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "covenant-lint",
    about = "Security linter for Covenant smart contracts",
    version
)]
pub struct Cli {
    /// Source files to lint (`.cov`).
    pub paths: Vec<std::path::PathBuf>,

    /// Enable deep / expensive detectors.
    #[arg(long)]
    pub deep: bool,

    /// Filter detectors by code, name, or category prefix (case-insensitive).
    #[arg(long, value_name = "PATTERN")]
    pub filter: Option<String>,

    /// Minimum severity to report: info, warning, critical.
    #[arg(long, value_name = "LEVEL", default_value = "info")]
    pub severity: String,

    /// Output format: human (default) or json.
    #[arg(long, value_name = "FORMAT", default_value = "human")]
    pub format: String,

    /// Suppress a specific detector code globally (may be repeated).
    #[arg(long = "allow", value_name = "CODE")]
    pub allow: Vec<String>,

    /// Elevate a specific detector code to Critical (may be repeated).
    #[arg(long = "deny", value_name = "CODE")]
    pub deny: Vec<String>,

    /// Color mode: auto (default), always, never.
    #[arg(long, value_name = "MODE", default_value = "auto")]
    pub color: String,
}

impl Cli {
    pub fn use_color(&self) -> bool {
        match self.color.as_str() {
            "always" => true,
            "never" => false,
            _ => std::io::IsTerminal::is_terminal(&std::io::stderr()),
        }
    }
}

//! `covenant completions` — generate shell completion scripts.

use clap::Args;
use clap_complete::Shell;

use crate::error::CliError;

#[derive(Args, Debug, Clone)]
pub struct CompletionsArgs {
    /// Shell to generate completions for (bash, fish, zsh, powershell).
    #[arg(value_enum)]
    pub shell: Shell,
}

/// Generate completions to stdout.
///
/// The caller must pass the `clap::Command` that was built from the top-level
/// `Cli` struct — we receive it via a closure to avoid circular module deps.
pub fn run(args: CompletionsArgs, cmd: &mut clap::Command) -> Result<(), CliError> {
    use clap_complete::generate;
    generate(args.shell, cmd, "covenant", &mut std::io::stdout());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_values_parse() {
        // clap_complete::Shell implements ValueEnum — just verify we can reference them
        let _bash = Shell::Bash;
        let _zsh = Shell::Zsh;
        let _fish = Shell::Fish;
        let _ps = Shell::PowerShell;
    }
}

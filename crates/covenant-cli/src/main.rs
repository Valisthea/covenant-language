//! Covenant compiler CLI: V0.5 (Phase 14, Sessions 1-3).
//!
//! Subcommands:
//!   init: scaffold a new project from a template
//!   build: compile to EVM bytecode + ABI
//!   check: frontend-only validation (no codegen)
//!   test: run inline test blocks via the MockChain harness
//!   fmt: format source files canonically
//!   inspect: dump AST/IR/bytecode/ABI/storage
//!   lint: run the security linter
//!   clean: remove build artifacts
//!   completions: generate shell completion scripts

mod color;
mod commands;
mod diagnostics;
mod error;
mod output;

use std::path::PathBuf;

use clap::{CommandFactory, Parser, Subcommand};

use color::ColorMode;
use output::OutputFormat;

// ---------------------------------------------------------------------------
// Top-level CLI
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(
    name = "covenant",
    version,
    about = "Covenant compiler and tools",
    long_about = None
)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Increase verbosity (-v info, -vv debug, -vvv trace).
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// Suppress all non-error output.
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Output format.
    #[arg(long, value_enum, default_value = "human", global = true)]
    format: OutputFormat,

    /// Color output.
    #[arg(long, value_enum, default_value = "auto", global = true)]
    color: ColorMode,

    /// Path to Covenant.toml manifest (overrides upward search).
    #[arg(long, global = true)]
    manifest: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Scaffold a new Covenant project from a template.
    Init(commands::init::InitArgs),

    /// Compile .cov source files to EVM bytecode and ABI artifacts.
    Build(commands::build::BuildArgs),

    /// Run the frontend pipeline only (no code generation).
    Check(commands::check::CheckArgs),

    /// Run inline test blocks via the MockChain harness.
    Test(commands::test::TestArgs),

    /// Format Covenant source files in canonical style.
    Fmt(commands::fmt::FmtArgs),

    /// Dump AST, IR, bytecode, ABI, or storage layout.
    Inspect(commands::inspect::InspectArgs),

    /// Compare two storage-layout sidecars to catch breaking upgrades.
    Layout(commands::layout::LayoutArgs),

    /// Run the security linter on .cov source files.
    Lint(commands::lint::LintArgs),

    /// Remove build artifacts.
    Clean(commands::clean::CleanArgs),

    /// Generate shell completion scripts.
    Completions(commands::completions::CompletionsArgs),

    /// Print the long-form explanation for a diagnostic code.
    /// `covenant explain E421` or `covenant explain --list`.
    Explain(commands::explain::ExplainArgs),

    /// Diagnose the local development environment.
    /// Probes rustc, cargo, Foundry, env vars, and config files;
    /// prints a green ✓ / yellow ⚠ / red ✗ report with action items.
    Doctor(commands::doctor::DoctorArgs),
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    // Clean panic hook: shows ICE message, not a Rust stack trace by default.
    // With RUST_BACKTRACE set, surfaces the actual panic payload + location
    // (not just the PanicHookInfo Debug, which only shows the closure site).
    std::panic::set_hook(Box::new(|info| {
        eprintln!("error[ICE]: internal compiler error");
        eprintln!("Please report at https://github.com/Valisthea/covenant-language/issues");
        if std::env::var("RUST_BACKTRACE").is_ok() {
            // Extract the panic payload (string or &str).
            let msg = info
                .payload()
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
                .unwrap_or("<unknown panic payload>");
            if let Some(loc) = info.location() {
                eprintln!(
                    "panicked at '{}', {}:{}:{}",
                    msg,
                    loc.file(),
                    loc.line(),
                    loc.column()
                );
            } else {
                eprintln!("panicked at '{msg}' (no location)");
            }
        }
    }));

    let cli = Cli::parse();
    let use_color = cli.color.should_color();
    let format = cli.format;
    let verbose = cli.verbose;
    let manifest = cli.manifest.as_ref();

    let result = match cli.command {
        Commands::Init(args) => commands::init::run(&args),
        Commands::Build(args) => commands::build::run(&args, manifest, format, use_color),
        Commands::Check(args) => commands::check::run(&args, manifest, format, use_color),
        Commands::Test(args) => commands::test::run(args, verbose),
        Commands::Fmt(args) => commands::fmt::run(args, verbose),
        Commands::Inspect(args) => commands::inspect::run(args, verbose),
        Commands::Layout(args) => commands::layout::run(&args),
        Commands::Lint(args) => commands::lint::run(&args, format, use_color),
        Commands::Clean(args) => commands::clean::run(&args, manifest),
        Commands::Completions(args) => {
            let mut cmd = Cli::command();
            commands::completions::run(args, &mut cmd)
        }
        Commands::Explain(args) => commands::explain::run(args),
        Commands::Doctor(args) => commands::doctor::run(args),
    };

    match result {
        Ok(()) => std::process::exit(0),
        Err(e) => {
            let code = e.exit_code();
            if !cli.quiet || code != 1 {
                eprintln!("error: {e}");
            }
            std::process::exit(code);
        }
    }
}

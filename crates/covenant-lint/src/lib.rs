//! `covenant-lint`: security linter for Covenant smart contracts.
//!
//! The library exposes `run()` for use by the CLI binary and by the
//! `covenant-cli` `lint` / `build --release` subcommands.

pub mod cli;
pub mod config;
pub mod detectors;
pub mod framework;
pub mod ir_utils;
pub mod output;
pub mod source_scan;

use std::collections::HashSet;

use anyhow::Context;
use covenant_diag::SourceId;

use crate::cli::Cli;
use crate::framework::{apply_suppressions, parse_suppressions, DetectorRegistry, Severity};
use crate::output::OutputFormat;

/// Run the linter according to `args`.
///
/// Returns `Ok(exit_code)` where:
/// - `0` = no findings at or above the min severity
/// - `1` = one or more findings at or above the min severity
pub fn run(args: &Cli) -> anyhow::Result<i32> {
    let format = OutputFormat::from_str(&args.format)
        .ok_or_else(|| anyhow::anyhow!("unknown output format {:?}", args.format))?;

    let min_severity = Severity::from_str(&args.severity)
        .ok_or_else(|| anyhow::anyhow!("unknown severity level {:?}", args.severity))?;

    let use_color = args.use_color();
    let registry = DetectorRegistry::new();

    let category = None; // future: parse from args
    let detectors = registry.filtered(args.filter.as_deref(), min_severity, args.deep, category);

    // Globally suppressed codes from --allow flags.
    let global_allow: HashSet<String> = args.allow.iter().cloned().collect();
    let deny_set: HashSet<String> = args.deny.iter().cloned().collect();

    let mut any_findings = false;

    for path in &args.paths {
        let source_text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;

        let source_path = path.display().to_string();

        // Compile to IR; skip file on compile error.
        let ir = match covenant_driver::compile_to_ir(&source_text, SourceId::new(0)) {
            Ok(ir) => ir,
            Err(diags) => {
                eprintln!(
                    "covenant-lint: {} compile error(s) in {source_path}: skipping",
                    diags.len()
                );
                continue;
            }
        };

        // Run all selected detectors.
        let mut findings: Vec<_> = detectors
            .iter()
            .flat_map(|d| d.analyze(&ir, &source_text))
            .collect();

        // Apply --deny elevation.
        for f in &mut findings {
            if deny_set.contains(f.detector_code) {
                f.severity = Severity::Critical;
            }
        }

        // Merge source-level @allow suppressions with CLI --allow.
        let source_suppressions = parse_suppressions(&source_text);
        let mut combined_allow = global_allow.clone();
        combined_allow.extend(source_suppressions);

        let findings = apply_suppressions(findings, &combined_allow);

        // Filter by min severity.
        let findings: Vec<_> = findings
            .into_iter()
            .filter(|f| f.severity >= min_severity)
            .collect();

        if findings.is_empty() {
            continue;
        }

        any_findings = true;

        match format {
            OutputFormat::Human => {
                output::human::emit_findings(&findings, &source_path, &source_text, use_color);
            }
            OutputFormat::Json => {
                output::json::emit_findings(&findings, &source_path, &source_text);
            }
        }
    }

    Ok(if any_findings { 1 } else { 0 })
}

/// Lint a source string directly and return all findings (used by integration tests and LSP).
///
/// V0.9 Sprint 39: runs the source-scan pass FIRST (catches pseudo-Solidity /
/// migration anti-patterns even when the source fails to parse), then runs the
/// IR-based security detectors against whatever module the pipeline can build.
/// Both sets of findings are merged.
pub fn lint_source(source: &str, source_id: SourceId) -> Vec<framework::Finding> {
    // Always-on: source-text scan for anti-patterns. Works on raw text,
    // doesn't require successful IR build.
    let mut findings: Vec<_> = source_scan::scan(source);

    // IR-dependent detectors. These run whenever there is a module to run them
    // against, INCLUDING when IR construction itself raised errors: `build_ir`
    // still returns the module, and refusing to report findings because some
    // unrelated diagnostic fired is a fail-open. A linter that goes quiet on
    // code the compiler just rejected is silent exactly when the code is least
    // trustworthy, and "no findings" is indistinguishable from "clean" to the
    // caller. E430/E431 made this concrete: an unbacked collection anywhere in
    // the file used to suppress every C100/C700/C1100 finding in it.
    //
    // Only a frontend failure yields no module, and the source-text scan above
    // already covers the common reasons for that (`//` comments, `mapping()`,
    // the `function` keyword, and the rest of the migration anti-patterns).
    let (ir_opt, _) = covenant_driver::compile_to_ir_for_analysis(source, source_id);
    if let Some(ir) = ir_opt {
        let registry = DetectorRegistry::new();
        let detectors = registry.filtered(None, Severity::Info, false, None);
        findings.extend(detectors.iter().flat_map(|d| d.analyze(&ir, source)));
    }

    let suppressions = parse_suppressions(source);
    apply_suppressions(findings, &suppressions)
}

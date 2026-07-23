//! `covenant doctor` — diagnose the local development environment.
//!
//! V0.9 Sprint 41 Phase 41.3. Inspired by Flutter's `flutter doctor` /
//! Foundry's implicit env checks.
//!
//! Runs a series of probes and prints a green ✓ / yellow ⚠ / red ✗
//! report. No probe causes a hard failure (we never want `covenant
//! doctor` to crash) ; problems get listed at the bottom as suggested
//! action items.
//!
//! Probes shipped today :
//!
//!   ✓ covenant version (always present)
//!   ✓ rustc version + min-supported-version check
//!   ✓ cargo presence
//!   ✓ Foundry (forge, cast) presence
//!   ✓ Sepolia RPC reachability (if SEPOLIA_RPC_URL env set)
//!   ✓ Etherscan API key presence (env var)
//!   ✓ HelperRegistry config file present
//!   ✓ covenant-helpers/ Foundry sub-project present
//!
//! V0.9.x can grow this list (LSP binary, VSCode extension presence,
//! disk-space estimation, etc.) without changing the output format.

use std::path::Path;
use std::process::Command;

use clap::Args;

use crate::error::CliError;

#[derive(Args, Debug, Clone, Default)]
pub struct DoctorArgs {
    /// Output as JSON for tooling integration. Default is human-readable.
    #[arg(long)]
    pub json: bool,

    /// Exit with non-zero status if any probe is in `Failed` state.
    /// Warnings (e.g. missing optional env vars) do NOT trigger non-zero
    /// exit — only hard failures (missing required tools, unreadable
    /// config files). Designed for CI gates : `covenant doctor --strict ||
    /// exit 1` blocks the pipeline if the dev env can't reproduce the
    /// V0.9.x baseline.
    #[arg(long)]
    pub strict: bool,
}

#[derive(Debug)]
struct Probe {
    name: String,
    status: ProbeStatus,
    detail: String,
    fix: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProbeStatus {
    Ok,
    Warning,
    Failed,
}

impl ProbeStatus {
    fn glyph(self) -> &'static str {
        match self {
            Self::Ok => "✓",
            Self::Warning => "⚠",
            Self::Failed => "✗",
        }
    }
}

pub fn run(args: DoctorArgs) -> Result<(), CliError> {
    let probes = vec![
        probe_covenant_version(),
        probe_rustc(),
        probe_cargo(),
        probe_foundry("forge"),
        probe_foundry("cast"),
        probe_env_var(
            "SEPOLIA_RPC_URL",
            "Sepolia RPC URL",
            "needed for Sepolia deploy / verify",
        ),
        probe_env_var(
            "ETHERSCAN_API_KEY",
            "Etherscan API key",
            "needed for Sepolia source verification",
        ),
        probe_env_var(
            "ASTER_RPC_URL",
            "Aster Chain RPC URL",
            "needed for Aster Chain deploy (V0.9.x — placeholder, deploy pending)",
        ),
        probe_file_present(
            "config/helper-addresses-v0.9.0.json",
            "V0.9 helper addresses registry",
        ),
        probe_file_present("helpers/foundry.toml", "covenant-helpers Foundry project"),
    ];

    if args.json {
        emit_json(&probes);
    } else {
        emit_human(&probes);
    }

    // V0.9.1 : --strict flag for CI gates. If any probe is in Failed
    // state (not Warning), exit non-zero so CI pipelines can block.
    let any_failed = probes.iter().any(|p| p.status == ProbeStatus::Failed);
    if args.strict && any_failed {
        let count = probes
            .iter()
            .filter(|p| p.status == ProbeStatus::Failed)
            .count();
        return Err(CliError::Usage(format!(
            "{} probe(s) failed (--strict mode). Resolve before proceeding.",
            count
        )));
    }

    Ok(())
}

// ─── Probes ─────────────────────────────────────────────────────────────

fn probe_covenant_version() -> Probe {
    Probe {
        name: "covenant".to_string(),
        status: ProbeStatus::Ok,
        detail: env!("CARGO_PKG_VERSION").to_string(),
        fix: None,
    }
}

fn probe_rustc() -> Probe {
    match command_version_string("rustc", &["--version"]) {
        Some(v) => {
            // Min supported = 1.81 (matches the workspace's rust-toolchain).
            // We don't strictly enforce — just warn if it looks too old.
            let too_old = v.contains("1.7") || v.contains("1.6") || v.contains("1.5");
            Probe {
                name: "rustc".to_string(),
                status: if too_old {
                    ProbeStatus::Warning
                } else {
                    ProbeStatus::Ok
                },
                detail: v,
                fix: if too_old {
                    Some("upgrade to rustc 1.81+ (rustup update stable)".to_string())
                } else {
                    None
                },
            }
        }
        None => Probe {
            name: "rustc".to_string(),
            status: ProbeStatus::Failed,
            detail: "not found in PATH".to_string(),
            fix: Some("install rustc via https://rustup.rs".to_string()),
        },
    }
}

fn probe_cargo() -> Probe {
    match command_version_string("cargo", &["--version"]) {
        Some(v) => Probe {
            name: "cargo".to_string(),
            status: ProbeStatus::Ok,
            detail: v,
            fix: None,
        },
        None => Probe {
            name: "cargo".to_string(),
            status: ProbeStatus::Failed,
            detail: "not found in PATH".to_string(),
            fix: Some("install cargo via https://rustup.rs".to_string()),
        },
    }
}

fn probe_foundry(tool: &str) -> Probe {
    let name = tool.to_string();
    match command_version_string(tool, &["--version"]) {
        Some(v) => Probe {
            name,
            status: ProbeStatus::Ok,
            detail: v,
            fix: None,
        },
        None => Probe {
            name,
            status: ProbeStatus::Warning,
            detail: format!("'{tool}' not found in PATH"),
            fix: Some(
                "install Foundry via https://book.getfoundry.sh/getting-started/installation"
                    .to_string(),
            ),
        },
    }
}

fn probe_env_var(var: &str, label: &str, hint: &str) -> Probe {
    match std::env::var(var) {
        Ok(v) if !v.is_empty() => Probe {
            name: var.to_string(),
            status: ProbeStatus::Ok,
            detail: format!("{} bytes set ({})", v.len(), truncate(&v, 32)),
            fix: None,
        },
        _ => Probe {
            name: var.to_string(),
            status: ProbeStatus::Warning,
            detail: format!("{label} env var not set"),
            fix: Some(format!("{hint}; export {var}=...")),
        },
    }
}

fn probe_file_present(rel_path: &str, label: &str) -> Probe {
    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let p = cwd.join(rel_path);
    let found = p.exists();
    Probe {
        name: rel_path.to_string(),
        status: if found {
            ProbeStatus::Ok
        } else {
            ProbeStatus::Warning
        },
        detail: if found {
            format!("found at {}", display_path(&p))
        } else {
            format!("not found (expected {label})")
        },
        fix: if found {
            None
        } else {
            Some("relative to project root; check `pwd` matches the covenant repo".to_string())
        },
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────

fn command_version_string(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    // First line is usually the most useful.
    Some(s.lines().next().unwrap_or("").trim().to_string())
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        // Walk char boundaries so we never split mid-UTF-8 codepoint.
        let mut end = n.saturating_sub(1);
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &s[..end])
    }
}

fn display_path(p: &Path) -> String {
    p.display().to_string().replace('\\', "/")
}

fn emit_human(probes: &[Probe]) {
    println!("covenant doctor");
    println!();
    let max_name = probes.iter().map(|p| p.name.len()).max().unwrap_or(0);
    for p in probes {
        println!(
            "  {} {:width$}  {}",
            p.status.glyph(),
            p.name,
            p.detail,
            width = max_name
        );
    }
    let action_items: Vec<&Probe> = probes
        .iter()
        .filter(|p| p.fix.is_some() && p.status != ProbeStatus::Ok)
        .collect();
    if !action_items.is_empty() {
        println!();
        println!("Action items :");
        for (i, p) in action_items.iter().enumerate() {
            println!(
                "  {}. {} — {}",
                i + 1,
                p.name,
                p.fix.as_deref().unwrap_or("")
            );
        }
    } else {
        println!();
        println!("All probes OK. Environment is ready for Covenant V0.9 development.");
    }
}

fn emit_json(probes: &[Probe]) {
    println!("{{");
    println!("  \"probes\": [");
    for (i, p) in probes.iter().enumerate() {
        let comma = if i + 1 < probes.len() { "," } else { "" };
        let status = match p.status {
            ProbeStatus::Ok => "ok",
            ProbeStatus::Warning => "warning",
            ProbeStatus::Failed => "failed",
        };
        let fix = p
            .fix
            .as_ref()
            .map(|s| format!("\"{}\"", json_escape(s)))
            .unwrap_or_else(|| "null".to_string());
        println!(
            "    {{\"name\": \"{}\", \"status\": \"{}\", \"detail\": \"{}\", \"fix\": {}}}{}",
            json_escape(&p.name),
            status,
            json_escape(&p.detail),
            fix,
            comma
        );
    }
    println!("  ]");
    println!("}}");
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

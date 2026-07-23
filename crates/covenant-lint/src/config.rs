//! `.covenantlint.json` configuration (V0.9 Sprint 39 Phase 39.4).
//!
//! Per-project linter configuration loaded from a JSON file at the
//! repository root. Lets users tune severity per detector, exclude
//! files via glob patterns, and override per-project defaults without
//! editing source.
//!
//! Schema (all fields optional) :
//!
//! ```json
//! {
//!   "rules": {
//!     "L001": "warn",
//!     "C100": "error",
//!     "L004": "off"
//!   },
//!   "exclude": [
//!     "fixtures/**",
//!     "test/legacy/*.cov"
//!   ]
//! }
//! ```
//!
//! Severity values : `"off"` | `"info"` | `"warn"` | `"error"`.
//! Unknown rule codes are ignored (forward-compat — V0.9.x can ship new
//! detectors without breaking V0.9.0 config files).
//!
//! Loader is permissive : missing file = empty config = use defaults.
//! Malformed JSON = error to user, lint exits non-zero.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// User-supplied lint configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LintConfig {
    /// Per-detector overrides. Keys are detector codes (`"L001"`,
    /// `"C100"`, etc.); values are severity strings.
    #[serde(default)]
    pub rules: HashMap<String, String>,

    /// Glob patterns of paths to exclude from linting (relative to the
    /// project root). Matched via `glob::Pattern`-compatible syntax.
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// Per-rule severity override.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleSeverity {
    Off,
    Info,
    Warn,
    Error,
}

impl RuleSeverity {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "off" | "allow" => Some(Self::Off),
            "info" | "hint" => Some(Self::Info),
            "warn" | "warning" => Some(Self::Warn),
            "error" | "deny" => Some(Self::Error),
            _ => None,
        }
    }
}

impl LintConfig {
    /// Load `.covenantlint.json` from `dir` (or any ancestor). Returns
    /// `Ok(None)` if no config file is found anywhere up to filesystem
    /// root — this is the "use defaults" path. Returns `Err` only on
    /// malformed JSON.
    pub fn find_and_load(start_dir: &Path) -> Result<Option<Self>, ConfigError> {
        let mut current = Some(start_dir.to_path_buf());
        while let Some(dir) = current {
            let candidate = dir.join(".covenantlint.json");
            if candidate.exists() {
                let text = fs::read_to_string(&candidate).map_err(|e| ConfigError::Read {
                    path: candidate.clone(),
                    source: e,
                })?;
                let config: LintConfig =
                    serde_json::from_str(&text).map_err(|e| ConfigError::Parse {
                        path: candidate,
                        source: e,
                    })?;
                return Ok(Some(config));
            }
            current = dir.parent().map(|p| p.to_path_buf());
        }
        Ok(None)
    }

    /// Load directly from a JSON string (for tests / custom paths).
    pub fn from_json(json: &str) -> Result<Self, ConfigError> {
        serde_json::from_str(json).map_err(|e| ConfigError::Parse {
            path: std::path::PathBuf::from("<inline>"),
            source: e,
        })
    }

    /// Look up the configured severity for a detector code. Returns
    /// `None` if the user did not override (caller falls back to the
    /// detector's default severity).
    pub fn severity_for(&self, code: &str) -> Option<RuleSeverity> {
        self.rules.get(code).and_then(|s| RuleSeverity::parse(s))
    }
}

/// Errors loading or parsing `.covenantlint.json`.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("could not read config at {path:?}: {source}")]
    Read {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    #[error("invalid JSON in {path:?}: {source}")]
    Parse {
        path: std::path::PathBuf,
        source: serde_json::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_severity_keywords() {
        assert_eq!(RuleSeverity::parse("off"), Some(RuleSeverity::Off));
        assert_eq!(RuleSeverity::parse("ERROR"), Some(RuleSeverity::Error));
        assert_eq!(RuleSeverity::parse("Warn"), Some(RuleSeverity::Warn));
        assert_eq!(RuleSeverity::parse("nope"), None);
    }

    #[test]
    fn loads_from_json_string() {
        let json = r#"{
            "rules": { "L001": "warn", "C100": "off" },
            "exclude": ["fixtures/**"]
        }"#;
        let config = LintConfig::from_json(json).expect("parses");
        assert_eq!(config.severity_for("L001"), Some(RuleSeverity::Warn));
        assert_eq!(config.severity_for("C100"), Some(RuleSeverity::Off));
        assert_eq!(config.severity_for("UNSET"), None);
        assert_eq!(config.exclude, vec!["fixtures/**"]);
    }

    #[test]
    fn empty_config_returns_no_overrides() {
        let config = LintConfig::default();
        assert_eq!(config.severity_for("L001"), None);
        assert!(config.exclude.is_empty());
    }

    #[test]
    fn malformed_json_errors() {
        let bad = "{ this is not valid json";
        assert!(matches!(
            LintConfig::from_json(bad),
            Err(ConfigError::Parse { .. })
        ));
    }
}

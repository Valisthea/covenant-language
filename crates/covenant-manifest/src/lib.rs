//! Covenant.toml manifest parsing.
//!
//! Reads project configuration, validates required fields, and checks
//! compiler version constraints.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("I/O error reading {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("manifest parse error in {path}: {source}")]
    Toml {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("required manifest key missing: {0}")]
    MissingKey(String),
    #[error("malformed compiler version constraint '{constraint}': {reason}")]
    BadVersionConstraint { constraint: String, reason: String },
    #[error(
        "compiler version mismatch: project requires {required}, current compiler is {current}"
    )]
    VersionMismatch { required: String, current: String },
}

// ---------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub project: ProjectSection,
}

#[derive(Debug, Deserialize)]
pub struct ProjectSection {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,

    #[serde(default)]
    pub source: SourceSection,
    #[serde(default)]
    pub build: BuildSection,
    #[serde(default)]
    pub test: TestSection,
    #[serde(default)]
    pub compiler: CompilerSection,
}

#[derive(Debug, Deserialize, Default)]
pub struct SourceSection {
    pub root: Option<PathBuf>,
    pub entrypoints: Option<Vec<PathBuf>>,
}

#[derive(Debug, Deserialize, Default)]
pub struct BuildSection {
    pub target: Option<String>,
    pub optimizer: Option<bool>,
    pub emit_source_map: Option<bool>,
    pub output: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Default)]
pub struct TestSection {
    pub directory: Option<PathBuf>,
    pub mock_chain: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct CompilerSection {
    pub version: Option<String>,
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

impl Manifest {
    /// Read and parse a `Covenant.toml` at the given path.
    pub fn read(path: &Path) -> Result<Self, ManifestError> {
        let text = std::fs::read_to_string(path).map_err(|e| ManifestError::Io {
            path: path.to_owned(),
            source: e,
        })?;
        let manifest: Manifest = toml::from_str(&text).map_err(|e| ManifestError::Toml {
            path: path.to_owned(),
            source: e,
        })?;
        Ok(manifest)
    }

    /// Walk upward from `start` looking for a `Covenant.toml`.
    pub fn find_upward(start: &Path) -> Option<PathBuf> {
        let mut current = if start.is_file() {
            start.parent()?.to_path_buf()
        } else {
            start.to_path_buf()
        };
        loop {
            let candidate = current.join("Covenant.toml");
            if candidate.exists() {
                return Some(candidate);
            }
            if !current.pop() {
                return None;
            }
        }
    }

    /// If a compiler version constraint is set, verify `current` satisfies it.
    ///
    /// `current` should be the value of `env!("CARGO_PKG_VERSION")`.
    pub fn check_compiler_version(&self, current: &str) -> Result<(), ManifestError> {
        let Some(ref constraint) = self.project.compiler.version else {
            return Ok(());
        };
        let req = semver::VersionReq::parse(constraint).map_err(|e| {
            ManifestError::BadVersionConstraint {
                constraint: constraint.clone(),
                reason: e.to_string(),
            }
        })?;
        let cur = semver::Version::parse(current)
            .unwrap_or_else(|_| semver::Version::parse("0.0.1").unwrap());
        if !req.matches(&cur) {
            return Err(ManifestError::VersionMismatch {
                required: constraint.clone(),
                current: current.to_owned(),
            });
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const BASIC_TOML: &str = r#"
[project]
name = "hello"
version = "0.1.0"
"#;

    const FULL_TOML: &str = r#"
[project]
name = "my-project"
version = "1.2.3"
authors = ["Alice <alice@example.com>"]
description = "A test project"
license = "MIT"
repository = "https://github.com/example/repo"

[project.source]
root = "src/"
entrypoints = ["main.cov", "lib.cov"]

[project.build]
target = "evm"
optimizer = true
output = "build/"

[project.test]
directory = "tests/"
mock_chain = "default"

[project.compiler]
version = ">=0.0.1"
"#;

    #[test]
    fn parses_minimal_manifest() {
        let m: Manifest = toml::from_str(BASIC_TOML).unwrap();
        assert_eq!(m.project.name, "hello");
        assert_eq!(m.project.version, "0.1.0");
        assert!(m.project.description.is_none());
        assert!(m.project.source.root.is_none());
    }

    #[test]
    fn parses_full_manifest() {
        let m: Manifest = toml::from_str(FULL_TOML).unwrap();
        assert_eq!(m.project.name, "my-project");
        assert_eq!(m.project.authors, vec!["Alice <alice@example.com>"]);
        assert_eq!(
            m.project.source.entrypoints,
            Some(vec![PathBuf::from("main.cov"), PathBuf::from("lib.cov")])
        );
        assert_eq!(m.project.build.output, Some(PathBuf::from("build/")));
        assert_eq!(m.project.compiler.version, Some(">=0.0.1".to_string()));
    }

    #[test]
    fn version_constraint_passes() {
        let m: Manifest = toml::from_str(FULL_TOML).unwrap();
        assert!(m.check_compiler_version("0.0.1").is_ok());
    }

    #[test]
    fn version_constraint_fails() {
        let toml_str = r#"
[project]
name = "x"
version = "1.0.0"
[project.compiler]
version = ">=9.0.0"
"#;
        let m: Manifest = toml::from_str(toml_str).unwrap();
        assert!(m.check_compiler_version("0.0.1").is_err());
    }

    #[test]
    fn find_upward_returns_none_for_root() {
        // Searching from filesystem root should find nothing (assuming no Covenant.toml there).
        // We just verify it terminates and doesn't panic.
        let _ = Manifest::find_upward(Path::new("/"));
    }
}

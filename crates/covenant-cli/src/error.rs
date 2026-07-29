//! CLI error type with exit-code semantics per Doc 9 §3.4.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CliError {
    #[error("Covenant.toml not found (searched upward from current directory)")]
    ManifestNotFound,

    #[error("compilation failed")]
    CompileError,

    #[allow(dead_code)]
    #[error("internal compiler error: {0}")]
    Internal(String),

    #[error("{0}")]
    Io(#[from] std::io::Error),

    #[error("manifest error: {0}")]
    Manifest(#[from] covenant_manifest::ManifestError),

    #[allow(dead_code)]
    #[error("compiler version mismatch: {0}")]
    CompilerVersionMismatch(String),

    #[error("unknown template '{0}': available: basics, token, ballot, ceremony, module")]
    UnknownTemplate(String),

    #[error("target directory is not empty; use --force to overwrite: {0}")]
    NonEmptyTarget(String),

    #[error("usage error: {0}")]
    Usage(String),

    #[error("linter found critical security findings, release build blocked")]
    LintError,

    #[error("{0} test(s) failed")]
    TestFailed(usize),

    #[error("source file(s) are not formatted: run `covenant fmt` to fix")]
    FmtCheckFailed,
}

impl CliError {
    /// Per Doc 9 §3.4.
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::CompileError | Self::CompilerVersionMismatch(_) | Self::Manifest(_) => 1,
            Self::Usage(_) | Self::UnknownTemplate(_) => 2,
            Self::Io(_) | Self::ManifestNotFound | Self::NonEmptyTarget(_) => 3,
            Self::Internal(_) => 4,
            Self::LintError | Self::TestFailed(_) | Self::FmtCheckFailed => 1,
        }
    }
}

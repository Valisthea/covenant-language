//! Diagnostics infrastructure: error, warning, and note reporting for the Covenant compiler.
//!
//! This crate provides the shared `Span`, `SourceId`, `Diagnostic`, and related types
//! used throughout the compiler. Rendering (via `ariadne`) is deferred to later phases;
//! this phase only collects and exposes diagnostic values.
//!
//! V0.9 Sprint 38 adds the [`explanations`] module which carries
//! long-form prose for high-frequency error codes. Consumed by the
//! `covenant explain <code>` CLI subcommand and by the LSP code
//! actions.

pub mod explanations;

use std::fmt;
use std::ops::Range;

/// Identifier for a source file within a `SourceMap`.
///
/// Phase 1 uses bare `SourceId`s without a map; the map is introduced in Phase 4+.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceId(u32);

impl SourceId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

/// A half-open byte range `[start, end)` into a specific source buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub source: SourceId,
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub const fn new(source: SourceId, start: u32, end: u32) -> Self {
        Self { source, start, end }
    }

    pub const fn len(&self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    pub const fn is_empty(&self) -> bool {
        self.end <= self.start
    }

    pub fn range(&self) -> Range<usize> {
        (self.start as usize)..(self.end as usize)
    }

    /// Returns a span covering both `self` and `other`, assuming they belong to the same source.
    pub fn join(self, other: Span) -> Span {
        debug_assert_eq!(
            self.source, other.source,
            "cannot join spans from different sources"
        );
        Span {
            source: self.source,
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// The severity level of a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticLevel {
    Error,
    Warning,
    Note,
}

/// A numeric diagnostic code, e.g. `E003` or `W101`.
///
/// See Doc 5 §17.2 for the code-range assignments per compiler stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DiagCode(pub u32);

impl fmt::Display for DiagCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "E{:03}", self.0)
    }
}

/// A diagnostic emitted by the compiler.
///
/// Phase 1 exposes the minimal surface needed by the lexer. Later phases will add
/// secondary labels, notes, and suggested-fix support (cf. Doc 5 §17.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub code: DiagCode,
    pub message: String,
    pub span: Span,
    pub help: Option<String>,
}

impl Diagnostic {
    pub fn error(code: DiagCode, message: impl Into<String>, span: Span) -> Self {
        Self {
            level: DiagnosticLevel::Error,
            code,
            message: message.into(),
            span,
            help: None,
        }
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

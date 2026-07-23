//! The `Finding` type: a single linter result with location, message, and optional fix.

use covenant_diag::Span;

use crate::framework::Severity;

/// A single linter finding.
#[derive(Debug, Clone)]
pub struct Finding {
    /// Stable detector code (e.g., "C001").
    pub detector_code: &'static str,
    /// Primary source location of the finding.
    pub span: Span,
    /// Additional related spans with explanatory labels.
    pub secondary_spans: Vec<(Span, &'static str)>,
    /// Main human-readable message.
    pub message: String,
    /// Suggestion / fix hint shown in the help line.
    pub help: Option<&'static str>,
    /// Severity of this finding.
    pub severity: Severity,
}

impl Finding {
    pub fn new(
        detector_code: &'static str,
        span: Span,
        message: impl Into<String>,
        severity: Severity,
    ) -> Self {
        Self {
            detector_code,
            span,
            secondary_spans: Vec::new(),
            message: message.into(),
            help: None,
            severity,
        }
    }

    pub fn with_help(mut self, help: &'static str) -> Self {
        self.help = Some(help);
        self
    }

    pub fn with_secondary(mut self, span: Span, label: &'static str) -> Self {
        self.secondary_spans.push((span, label));
        self
    }
}

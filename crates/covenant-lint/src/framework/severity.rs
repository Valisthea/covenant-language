//! Three-tier severity model per Doc 15 §2.1.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Critical => "critical",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "info" | "i" => Some(Self::Info),
            "warning" | "warn" | "w" => Some(Self::Warning),
            "critical" | "c" => Some(Self::Critical),
            _ => None,
        }
    }
}

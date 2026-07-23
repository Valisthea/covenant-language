use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsterDiagnostic {
    pub code: String,
    pub severity: Severity,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl AsterDiagnostic {
    pub fn error(code: &str, message: &str) -> Self {
        Self {
            code: code.to_string(),
            severity: Severity::Error,
            message: message.to_string(),
        }
    }

    pub fn warning(code: &str, message: &str) -> Self {
        Self {
            code: code.to_string(),
            severity: Severity::Warning,
            message: message.to_string(),
        }
    }

    pub fn info(code: &str, message: &str) -> Self {
        Self {
            code: code.to_string(),
            severity: Severity::Info,
            message: message.to_string(),
        }
    }
}

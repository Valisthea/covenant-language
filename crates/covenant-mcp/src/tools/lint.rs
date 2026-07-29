use covenant_diag::SourceId;
use covenant_lint::{framework::Severity, lint_source};
use rmcp::model::{CallToolResult, Tool};
use serde_json::{json, Map, Value};

use crate::diagnostics_index;
use crate::server::{error_result, schema, text_result};

pub fn definition() -> Tool {
    Tool {
        name: "lint".into(),
        description: "Run the Covenant security linter on source code. Returns findings with \
             detector codes, severity (info/warning/critical), span offsets, messages, and help text. \
             The linter requires code that compiles to IR; syntax errors produce an empty finding list."
            .into(),
        input_schema: schema(json!({
            "type": "object",
            "required": ["source"],
            "properties": {
                "source": {
                    "type": "string",
                    "description": "Covenant source code to lint"
                },
                "min_severity": {
                    "type": "string",
                    "enum": ["info", "warning", "critical"],
                    "description": "Minimum severity to include (default: info)"
                }
            }
        })),
    }
}

pub fn run(params: &Map<String, Value>) -> CallToolResult {
    let source = match params.get("source").and_then(Value::as_str) {
        Some(s) => s,
        None => return error_result("missing required parameter: source"),
    };

    let min_sev = params
        .get("min_severity")
        .and_then(Value::as_str)
        .and_then(Severity::from_str)
        .unwrap_or(Severity::Info);

    let all_findings = lint_source(source, SourceId::new(0));
    let findings: Vec<_> = all_findings
        .into_iter()
        .filter(|f| f.severity >= min_sev)
        .collect();

    let finding_json: Vec<Value> = findings
        .iter()
        .map(|f| {
            let doc = diagnostics_index::lookup(f.detector_code);
            json!({
                "code":     f.detector_code,
                "severity": f.severity.as_str(),
                "message":  f.message,
                "span":     { "start": f.span.start, "end": f.span.end },
                "help":     f.help,
                "doc":      doc,
            })
        })
        .collect();

    let counts = json!({
        "critical": findings.iter().filter(|f| f.severity == Severity::Critical).count(),
        "warning":  findings.iter().filter(|f| f.severity == Severity::Warning).count(),
        "info":     findings.iter().filter(|f| f.severity == Severity::Info).count(),
        "total":    findings.len(),
    });

    text_result(json!({
        "findings": finding_json,
        "counts":   counts,
    }))
}

use covenant_diag::{DiagnosticLevel, SourceId};
use covenant_driver::check;
use rmcp::model::{CallToolResult, Tool};
use serde_json::{json, Map, Value};

use crate::server::{error_result, schema, text_result};
use crate::tools::compile::format_diagnostics;

pub fn definition() -> Tool {
    Tool {
        name: "check_syntax".into(),
        description: "Fast frontend-only validation of Covenant source: lex → parse → \
             resolve → typecheck → privacy analysis. No codegen. Returns diagnostics with codes and spans."
            .into(),
        input_schema: schema(json!({
            "type": "object",
            "required": ["source"],
            "properties": {
                "source": {
                    "type": "string",
                    "description": "Covenant source code to validate"
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

    let diagnostics = check(source, SourceId::new(0));
    let error_count = diagnostics
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Error)
        .count();
    let warning_count = diagnostics
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Warning)
        .count();

    text_result(json!({
        "valid": error_count == 0,
        "error_count": error_count,
        "warning_count": warning_count,
        "diagnostics": format_diagnostics(&diagnostics),
    }))
}

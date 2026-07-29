use rmcp::model::{CallToolResult, Tool};
use serde_json::{json, Map, Value};

use crate::server::{error_result, schema, text_result};
use crate::templates;

pub fn definition() -> Tool {
    Tool {
        name: "scaffold".into(),
        description: "Generate a minimally-compiling Covenant .cov file for any of the 14 \
             top-level constructs. Templates are verified against compiler fixtures. \
             Returns the rendered source code ready to save as <name>.cov."
            .into(),
        input_schema: schema(json!({
            "type": "object",
            "required": ["construct", "name"],
            "properties": {
                "construct": {
                    "type": "string",
                    "description": "One of: record, token, confidential token, ballot, counter, \
                                    encrypted counter, board, market, vault, registry, bridge, \
                                    ceremony, module, hybrid module"
                },
                "name": {
                    "type": "string",
                    "description": "PascalCase contract name, e.g. MyVault"
                }
            }
        })),
    }
}

pub fn run(params: &Map<String, Value>) -> CallToolResult {
    let construct = match params.get("construct").and_then(Value::as_str) {
        Some(s) => s.trim().to_ascii_lowercase(),
        None => return error_result("missing required parameter: construct"),
    };
    let name = match params.get("name").and_then(Value::as_str) {
        Some(s) => s.trim().to_string(),
        None => return error_result("missing required parameter: name"),
    };
    if name.is_empty() {
        return error_result("name must not be empty");
    }

    match templates::render(&construct, &name) {
        Some(source) => text_result(json!({
            "construct": construct,
            "name":      name,
            "filename":  format!("{name}.cov"),
            "source":    source,
        })),
        None => error_result(&format!(
            "unknown construct '{construct}'. Valid: record, token, confidential token, ballot, \
             counter, encrypted counter, board, market, vault, registry, bridge, ceremony, \
             module, hybrid module"
        )),
    }
}

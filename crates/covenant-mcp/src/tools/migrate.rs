use rmcp::model::{CallToolResult, Tool};
use serde_json::{json, Map, Value};

use crate::migrate;
use crate::server::{error_result, schema, text_result};

pub fn definition() -> Tool {
    Tool {
        name: "migrate".into(),
        description: "Convert Solidity (.sol) source to Covenant (.cov), applying the 11 \
             Solidity→Covenant transformations: mapping→map, function→action, require→when, \
             //→--, contract→construct, uint256→amount, modifier→only, msg.sender→caller, \
             visibility removal, constructor→initialize, payable removal. \
             Returns the migrated source and a transformation report."
            .into(),
        input_schema: schema(json!({
            "type": "object",
            "required": ["source"],
            "properties": {
                "source": {
                    "type": "string",
                    "description": "Solidity source code to migrate"
                },
                "contract_name": {
                    "type": "string",
                    "description": "Optional override for the contract name in the output"
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

    let result = migrate::apply(source);

    text_result(json!({
        "covenant_source":         result.output,
        "construct_selected":      result.construct,
        "transformations_applied": result.applied,
        "todos":                   result.todos,
        "todo_count":              result.todos.len(),
    }))
}

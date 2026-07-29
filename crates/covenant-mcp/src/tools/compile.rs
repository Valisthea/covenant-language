use covenant_diag::{DiagnosticLevel, SourceId};
use covenant_driver::compile;
use covenant_evm_backend::EvmConfig;
use covenant_opt::OptimizerConfig;
use covenant_stdlib::StdlibConfig;
use rmcp::model::{CallToolResult, Tool};
use serde_json::{json, Map, Value};

use crate::server::{error_result, schema, text_result};

pub fn definition() -> Tool {
    Tool {
        name: "compile".into(),
        description: "Compile Covenant source to EVM bytecode and ABI. \
             Returns deploy_bytecode (hex), runtime_bytecode (hex), ABI JSON, \
             action count, runtime size in bytes, and all diagnostics."
            .into(),
        input_schema: schema(json!({
            "type": "object",
            "required": ["source"],
            "properties": {
                "source": {
                    "type": "string",
                    "description": "Covenant source code to compile"
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

    let (artifact, diagnostics) = compile(
        source,
        SourceId::new(0),
        EvmConfig::default(),
        StdlibConfig::default(),
        OptimizerConfig::default(),
    );

    let diag_json = format_diagnostics(&diagnostics);
    let error_count = diagnostics
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Error)
        .count();

    let output = if let Some(art) = artifact {
        json!({
            "success": true,
            "abi": art.abi,
            "deploy_bytecode": hex(&art.deploy_bytecode),
            "runtime_bytecode": hex(&art.runtime_bytecode),
            "runtime_size_bytes": art.runtime_size(),
            "action_count": art.function_selectors.len(),
            "diagnostics": diag_json,
        })
    } else {
        json!({
            "success": false,
            "error_count": error_count,
            "diagnostics": diag_json,
        })
    };

    text_result(output)
}

pub fn format_diagnostics(diagnostics: &[covenant_diag::Diagnostic]) -> Vec<Value> {
    diagnostics
        .iter()
        .map(|d| {
            json!({
                "level": match d.level {
                    DiagnosticLevel::Error   => "error",
                    DiagnosticLevel::Warning => "warning",
                    DiagnosticLevel::Note    => "note",
                },
                "code":    d.code.to_string(),
                "message": d.message,
                "span":    { "start": d.span.start, "end": d.span.end },
                "help":    d.help,
            })
        })
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

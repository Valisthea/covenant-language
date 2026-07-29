use std::future::Future;
use std::sync::Arc;

use rmcp::{
    model::{
        CallToolRequestParam, CallToolResult, Content, Implementation, ListToolsResult,
        PaginatedRequestParam, ProtocolVersion, ServerCapabilities, ServerInfo, ToolsCapability,
    },
    service::RequestContext,
    Error as McpError, RoleServer, ServerHandler,
};
use serde_json::{Map, Value};

use crate::tools;

#[derive(Clone)]
pub struct CovenantMcpServer;

impl ServerHandler for CovenantMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::default(),
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability { list_changed: None }),
                ..Default::default()
            },
            server_info: Implementation {
                name: "covenant-mcp".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            // The version is interpolated rather than written out. This
            // string said "V0.8" for three minor releases because nothing
            // tied it to the compiler it was describing.
            instructions: Some(format!(
                "Covenant v{} compiler tools. Covenant is testnet-only and its \
                     cryptographic primitives are mocked; the compiler refuses to \
                     target a mainnet. \
                     compile: source → EVM bytecode + ABI. \
                 check_syntax: fast frontend-only validation. \
                 lint: security analysis with Finding codes. \
                 scaffold: generate a new .cov file for any of 14 constructs. \
                 migrate: convert Solidity source to Covenant. \
                     explain: describe a construct, guard, type, or ERC standard. \
                     list_constructs: enumerate all 14 top-level keywords.",
                env!("CARGO_PKG_VERSION")
            )),
        }
    }

    fn list_tools(
        &self,
        _request: PaginatedRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        let tools = tools::all_definitions();
        std::future::ready(Ok(ListToolsResult {
            tools,
            next_cursor: None,
        }))
    }

    fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CallToolResult, McpError>> + Send + '_ {
        let params = request.arguments.unwrap_or_default();
        tracing::debug!(tool = %request.name, "dispatching tool call");

        let result = match request.name.as_ref() {
            "compile" => tools::compile::run(&params),
            "check_syntax" => tools::check_syntax::run(&params),
            "lint" => tools::lint::run(&params),
            "scaffold" => tools::scaffold::run(&params),
            "migrate" => tools::migrate::run(&params),
            "explain" => tools::explain::run(&params),
            "list_constructs" => tools::list_constructs::run(),
            name => {
                return std::future::ready(Err(McpError::invalid_params(
                    format!("unknown tool: {name}"),
                    None,
                )))
            }
        };

        std::future::ready(Ok(result))
    }
}

/// Build a JSON text `CallToolResult`.
pub fn text_result(json: serde_json::Value) -> CallToolResult {
    CallToolResult {
        content: vec![Content::text(
            serde_json::to_string_pretty(&json).unwrap_or_default(),
        )],
        is_error: Some(false),
    }
}

/// Build an error `CallToolResult`.
pub fn error_result(message: &str) -> CallToolResult {
    CallToolResult {
        content: vec![Content::text(format!(
            "{{\"error\": {}}}",
            serde_json::Value::String(message.to_string())
        ))],
        is_error: Some(true),
    }
}

/// Convert a `serde_json::json!({...})` literal into the `Arc<JsonObject>` that
/// `Tool.input_schema` requires.
pub fn schema(v: serde_json::Value) -> Arc<Map<String, Value>> {
    Arc::new(v.as_object().cloned().unwrap_or_default())
}

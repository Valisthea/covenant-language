use anyhow::Result;
use covenant_mcp::server::CovenantMcpServer;
use rmcp::{transport::stdio, ServiceExt};
use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    // MCP protocol: stdout is the wire protocol; all human-readable output goes to stderr.
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    tracing::info!("covenant-mcp {} starting", env!("CARGO_PKG_VERSION"));

    let service = CovenantMcpServer.serve(stdio()).await?;
    service.waiting().await?;

    Ok(())
}

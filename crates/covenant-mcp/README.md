# covenant-mcp

MCP (Model Context Protocol) server that exposes the Covenant V0.8 compiler to any MCP-compatible AI agent — Claude Code, Cursor, Claude Desktop.

## Tools

| Tool | Description |
|------|-------------|
| `compile` | Compile Covenant source → EVM bytecode + ABI |
| `check_syntax` | Fast frontend-only validation (no codegen) |
| `lint` | Security analysis with Finding codes |
| `scaffold` | Generate a new `.cov` file for any of 14 constructs |
| `migrate` | Convert Solidity source to Covenant |
| `explain` | Explain a construct, guard, type alias, or ERC standard |
| `list_constructs` | Enumerate all 14 top-level keywords |

## Usage

### Claude Code

Add to `claude_desktop_config.json` or `~/.claude.json`:

```json
{
  "mcpServers": {
    "covenant": {
      "command": "covenant-mcp"
    }
  }
}
```

### Cursor

Add to `.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "covenant": {
      "command": "covenant-mcp",
      "args": []
    }
  }
}
```

## Build

```bash
cargo build -p covenant-mcp --release
# Binary at: target/release/covenant-mcp
```

## Protocol

- **stdout** — MCP JSON-RPC wire protocol
- **stderr** — structured logs (tracing)
- **Transport** — stdio (stdin/stdout)

Set `RUST_LOG=debug` for verbose logging.

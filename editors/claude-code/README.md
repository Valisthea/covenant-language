# Covenant: Claude Code Plugin

Agent-side Covenant v0.9.5 language expertise for [Claude Code](https://claude.ai/code).
Teaches the Claude agent how to write, migrate, and defensively review `.cov`
smart contracts, FHE, ZK, post-quantum, and cryptographic amnesia included.

---

## What this plugin is

This is a **Claude Code AI plugin**, not an editor extension. It does not provide
syntax highlighting, bracket matching, or LSP diagnostics, those are handled by
the companion [VS Code extension](../vscode/), which also works in Cursor's
built-in editor.

What this plugin does: it teaches the **Claude Code agent** the Covenant v0.9.5
language so that AI-assisted workflows (scaffolding, migration, code review)
produce correct `.cov` code rather than Solidity-flavored pseudocode that the
compiler rejects. It is the Claude Code equivalent of [editors/cursor/](../cursor/)
for Cursor users, identical agent capabilities, same plugin format.

---

## What it ships

| Type | Count | Items |
|------|-------|-------|
| Skill | 1 | `covenant-expert`, full v0.9.5 language knowledge |
| Guidance | 1 | `CLAUDE.md`, persistent syntax + ERC-822x rules (auto-loaded) |
| Commands | 3 | `/covenant-new`, `/covenant-migrate`, `/covenant-review` |
| Subagent | 1 | `reviewer`, structured defensive audit report |

---

## Install

**From the marketplace** (once published):

```
/plugin marketplace add Valisthea/covenant-language
/plugin install covenant@Valisthea/covenant-language
```

**Local development / before marketplace approval:**

```bash
git clone https://github.com/Valisthea/covenant-language
/plugin marketplace add /absolute/path/to/covenant/editors/claude-code
```

The `CLAUDE.md` at the plugin root is loaded automatically whenever the plugin
is active. The three slash commands become available in all Claude Code sessions.

---

## Commands

| Command | What it does |
|---------|--------------|
| `/covenant-new <construct>` | Scaffold a minimally-compiling `.cov` file for the named construct |
| `/covenant-migrate [file.sol]` | Migrate a Solidity file to Covenant, applying the 11 anti-pattern fixes |
| `/covenant-review [file.cov]` | Defensive review, findings by severity with suggested patches |

Construct options for `/covenant-new`:
`token` · `confidential token` · `vault` · `record` · `ballot` · `counter` ·
`encrypted counter` · `board` · `market` · `registry` · `bridge` · `ceremony` ·
`module` · `hybrid module`

---

## Relationship to sibling plugins

All three plugins share the same agent capabilities. Choose based on your editor.

| Concern | `editors/vscode/` | `editors/cursor/` | `editors/claude-code/` |
|---------|:-----------------:|:-----------------:|:----------------------:|
| Syntax highlighting (TextMate grammar) | ✓ | ✓ via vscode |, |
| LSP diagnostics (`covenant-lsp`) | ✓ | ✓ via vscode |, |
| Hover documentation | ✓ | ✓ via vscode |, |
| Agent: scaffold new contract |, | ✓ `/covenant-new` | ✓ `/covenant-new` |
| Agent: migrate from Solidity |, | ✓ `/covenant-migrate` | ✓ `/covenant-migrate` |
| Agent: defensive review |, | ✓ `/covenant-review` | ✓ `/covenant-review` |
| Persistent syntax guidance |, | ✓ `rules/*.mdc` | ✓ `CLAUDE.md` |
| ERC-822x conformance guidance |, | ✓ `rules/erc-822x.mdc` | ✓ `CLAUDE.md` |
| Structured audit subagent |, | ✓ `reviewer` | ✓ `reviewer` |

Install the VS Code extension alongside whichever agent plugin fits your editor.

---

## License & Security Audit

Licensed under [Apache-2.0](LICENSE).

Covenant is **self-audited** by Kairos Lab's internal OMEGA adversarial review
(V4/V5/V6), **not** third-party audited; an external firm audit is the gate for V1.0.
The internal self-audit findings below are all resolved. See
[STATUS.md](../../STATUS.md) for the honest security posture.

| Severity | Count | Status |
|----------|------:|--------|
| Critical | 5 | Resolved |
| High | 8 | Resolved |
| Medium | 8 | Resolved |
| Low | 6 | Resolved |
| Info | 9 | Resolved |
| **Total** | **36** | **All resolved** |

Full self-audit reports: covenant-security-reviews.

---

## Roadmap

- **covenant-mcp server**: MCP server exposing compile, lint, scaffold, migrate, and explain
  as Claude Code tools (direct in-process covenant-cli integration)
- **Formatter integration**: `/covenant-format` command wrapping `covenant fmt`
- **Testnet deployment tools**: deploy directly to Robinhood Chain / Sepolia from
  Claude Code
- **Expanded migrate patterns**: ERC-721, ERC-1155, Governor, Timelock → Covenant equivalents

# Covenant: Cursor Plugin

Agent-side Covenant v0.9.7 language expertise for [Cursor](https://cursor.sh).
Teaches the Cursor agent how to write, migrate, and defensively review `.cov`
smart contracts, FHE, ZK, post-quantum, and cryptographic amnesia included.

---

## What this plugin is

This is a **Cursor AI plugin**, not an editor extension. It does not provide
syntax highlighting, bracket matching, or LSP diagnostics, those are handled
by the companion [VS Code extension](../vscode/), which also works in Cursor's
built-in editor.

What this plugin does: it teaches the **Cursor agent** the Covenant v0.9.7 language
so that AI-assisted workflows (scaffolding, migration, code review) produce correct
`.cov` code rather than Solidity-flavored pseudocode that the compiler rejects.

---

## What it ships

| Type | Count | Items |
|------|-------|-------|
| Skill | 1 | `covenant-expert`, full V0.9 language knowledge |
| Rules | 2 | `covenant-syntax`, `erc-822x` |
| Commands | 3 | `/covenant-new`, `/covenant-migrate`, `/covenant-review` |
| Subagent | 1 | `reviewer`, structured defensive audit report |

---

## Install

**Cursor Marketplace** (once approved):

Search for "Covenant" in the Cursor plugin marketplace and click Install. *(Coming soon, not yet published; use local development below in the meantime.)*

**Local development / before marketplace approval:**

```bash
mkdir -p ~/.cursor/plugins/covenant
cp -r editors/cursor/. ~/.cursor/plugins/covenant/
```

Restart Cursor, then open any `.cov` file, the `covenant-expert` skill activates
automatically. The three slash commands become available in all chats.

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

## Relationship to `editors/vscode/`

The VS Code extension and this Cursor plugin are **complementary, not overlapping**.
Install both for the full experience.

| Concern | VS Code extension | Cursor plugin |
|---------|:-----------------:|:-------------:|
| Syntax highlighting (TextMate grammar) | ✓ |, |
| LSP diagnostics (`covenant-lsp`) | ✓ |, |
| Hover documentation | ✓ |, |
| Agent: scaffold new contract |, | ✓ `/covenant-new` |
| Agent: migrate from Solidity |, | ✓ `/covenant-migrate` |
| Agent: defensive review |, | ✓ `/covenant-review` |
| Persistent syntax rules (`.mdc`) |, | ✓ `covenant-syntax` |
| ERC-822x conformance rules (`.mdc`) |, | ✓ `erc-822x` |
| Structured audit subagent |, | ✓ `reviewer` |

The extension handles the editor surface; the plugin handles AI assistance.

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

- **Testnet MCP server**: direct deploy-to-Robinhood-Chain / Sepolia workflow
- **Formatter integration**: `/covenant-format` command wrapping `covenant fmt`
- **Expanded migrate patterns**: ERC-721, ERC-1155, Governor, Timelock → Covenant equivalents
- **Sealed ballot**: `/covenant-new sealed ballot` once V0.9 ships the construct

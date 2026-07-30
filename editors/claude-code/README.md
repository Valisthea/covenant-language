# Covenant: Claude Code Plugin

Agent-side Covenant v0.9.7 language expertise for [Claude Code](https://claude.ai/code).
Teaches the Claude agent how to write, migrate, and defensively review `.cov`
smart contracts, FHE, ZK, post-quantum, and cryptographic amnesia included.

---

## What this plugin is

This is a **Claude Code AI plugin**, not an editor extension. It does not provide
syntax highlighting, bracket matching, or LSP diagnostics, those are handled by
the companion [VS Code extension](../vscode/), which also works in Cursor's
built-in editor.

What this plugin does: it teaches the **Claude Code agent** the Covenant v0.9.7
language so that AI-assisted workflows (scaffolding, migration, code review)
produce correct `.cov` code rather than Solidity-flavored pseudocode that the
compiler rejects. It is the Claude Code equivalent of [editors/cursor/](../cursor/)
for Cursor users, identical agent capabilities, same plugin format.

---

## What it ships

| Type | Count | Items |
|------|-------|-------|
| Skill | 1 | `covenant-expert`, full v0.9.7 language knowledge |
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
| `/covenant-new <construct>` | Scaffold a `.cov` file for the named construct. Thirteen of the fourteen constructs have a template and each one builds at v0.9.7; `registry` has none, see below |
| `/covenant-migrate [file.sol]` | Migrate a Solidity file to Covenant, applying the 11 anti-pattern fixes |
| `/covenant-review [file.cov]` | Defensive review, findings by severity with suggested patches |

Construct options for `/covenant-new`:
`token` · `confidential token` · `vault` · `record` · `ballot` · `counter` ·
`encrypted counter` · `board` · `market` · `registry` · `bridge` · `ceremony` ·
`module` · `hybrid module`

### `registry` has no buildable template

Thirteen of the fourteen constructs have a template in `commands/new.md`, and
every one of them passes `covenant build` exactly as scaffolded. `registry` is
the exception: it does not compile in any form at this release, so
`/covenant-new registry` writes no file and offers a `module` fallback instead.

`registry Demo { }`, with a completely empty body, already fails with two
`E505`. The ERC-8231 synthesis injects `register` and `key_of` over `pq_key`,
which lowers to a dynamic ABI `bytes` that this release's codegen refuses; the
gap is tracked in `DEBT.md`. Declaring your own `key_of` instead yields a single
`E601`, `user-declared function key_of conflicts with ERC-8231 synthesis`, and
the build stops there. `covenant check` exits 0 in both cases, so the failure
shows up only under `covenant build`.

---

## Relationship to sibling plugins

All three plugins share the same agent capabilities. Choose based on your editor.

| Concern | `editors/vscode/` | `editors/cursor/` | `editors/claude-code/` |
|---------|:-----------------:|:-----------------:|:----------------------:|
| Syntax highlighting (TextMate grammar) | ✓ | ✓ via vscode | ✗ |
| LSP diagnostics (`covenant-lsp`) | ✓ | ✓ via vscode | ✗ |
| Hover documentation | ✓ | ✓ via vscode | ✗ |
| Agent: scaffold new contract | ✗ | ✓ `/covenant-new` | ✓ `/covenant-new` |
| Agent: migrate from Solidity | ✗ | ✓ `/covenant-migrate` | ✓ `/covenant-migrate` |
| Agent: defensive review | ✗ | ✓ `/covenant-review` | ✓ `/covenant-review` |
| Persistent syntax guidance | ✗ | ✓ `rules/*.mdc` | ✓ `CLAUDE.md` |
| ERC-822x conformance guidance | ✗ | ✓ `rules/erc-822x.mdc` | ✓ `CLAUDE.md` |
| Structured audit subagent | ✗ | ✓ `reviewer` | ✓ `reviewer` |

Install the VS Code extension alongside whichever agent plugin fits your editor.

---

## License & Security Audit

Licensed under [Apache-2.0](LICENSE).

Covenant is **self-reviewed** by Kairos Lab's internal OMEGA adversarial review,
**not** third-party audited; an external firm audit is the gate for V1.0.
See [STATUS.md](../../STATUS.md) and
[docs/security-and-audit-roadmap.md](../../docs/security-and-audit-roadmap.md)
for the honest security posture, and `DEBT.md` for the open items.

| Cycle | Target | Outcome |
|---|---|---|
| OMEGA V3.6.1, 2026-07-24 | v0.9.6 | 43 findings, 14 Critical. Published unremediated, then closed in v0.9.7. Full report in [covenant-security-reviews](https://github.com/Valisthea/covenant-security-reviews/tree/main/audits/2026-07-24-omega-v3.6-covenant-v0.9.6) |
| Adversarial bounty, 2026-07-23 | v0.9.4 | 1 Critical, 2 High, 4 Medium, 2 Low. Fixed in v0.9.5 |
| OMEGA V6, 2026-07-05 | v0.9.2 | 6 Critical, 6 High, 5 Medium, plus Low and Info. Fixed in v0.9.3 |
| OMEGA V5, April 2026 | v0.9.0 | Gate review before the tag. Go, with conditions recorded |
| OMEGA V4, April 2026 | v0.8 | Incomplete. Stopped after phase 2, issued no verdict |
| OMEGA V4, April 2026 | v0.6 | 40 graded findings plus 1 withdrawn. All resolved |

Not everything found is closed. Two Criticals from the v0.9.2 cycle are carried
rather than fixed: CRT-005 is partially fixed, with the `CeremonyHelper.sol`
layer still open, and CRT-007 is mitigated by refusal (`E505`), which disables
the `registry` construct while the root cause stays in `DEBT.md`.

The v0.9.2 cycle found six Critical defects that four earlier cycles had
missed, and the v0.8 cycle was never finished, so it is evidence of nothing.
Absence of a finding here has repeatedly not meant absence of a defect.

Full self-review archive:
[covenant-security-reviews](https://github.com/Valisthea/covenant-security-reviews).

---

## Companion: the covenant-mcp server

Shipped, at v0.9.7, in a separate crate. It is an MCP server that exposes the
compiler to Claude Code as tools, linking `covenant-driver`, `covenant-lint`,
`covenant-diag`, `covenant-evm-backend`, `covenant-opt` and `covenant-stdlib`
directly rather than shelling out to the CLI. Seven tools register: `compile`,
`check_syntax`, `lint`, `scaffold`, `migrate`, `explain`, `list_constructs`.

Source: `crates/covenant-mcp/`. Packaged bundle:
`crates/covenant-mcp/dist/covenant-mcp-0.9.7.mcpb`.

It is a separate install from this plugin. Install both if you want the agent to
compile as well as to write.

---

## Roadmap

- **Formatter integration**: `/covenant-format` command wrapping `covenant fmt`
- **Testnet deployment tools**: deploy directly to Sepolia from Claude Code
- **Expanded migrate patterns**: ERC-721, ERC-1155, Governor, Timelock → Covenant equivalents

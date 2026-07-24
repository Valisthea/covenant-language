# Changelog

All notable changes to the Covenant Claude Code plugin are documented here.

## [0.2.0] — 2026-06-09

### Fixed
- **Corrected the ERC-8228 attribution.** The `ceremony` (amnesia) guidance no longer cites ERC-8228 — that number was officially assigned to the Styx *Encrypted Token* Standard (`Valisthea/styx-erc-encrypted-token`), a different spec. The amnesia ceremony has no assigned ERC; the reviewer no longer flags a "missing ERC-8228 comment" for `ceremony`, and the scaffold template emits a plain construct comment instead. Updated `CLAUDE.md`, `agents/reviewer.md`, `commands/new.md`, `commands/review.md`. **⚠️ REVERTED (2026-07-24): this was wrong. Per ethereum/ERCs PR #1681 (editor-renumbered 1681→8228, titled "Cryptographic Amnesia"), ERC-8228 IS the amnesia standard — a `ceremony` DOES cite ERC-8228.**

### Changed
- Description tracked to Covenant **V0.9** (was V0.8).

## [0.1.0] — 2026-04-26

Initial release.

### Added

- **Skill** `covenant-expert` — agent-side Covenant V0.8 language expertise: all 14 top-level
  constructs, FHE/ZK/post-quantum/cryptographic-amnesia stack, ERC-8227/8228/8229/8231,
  the 11 Solidity migration anti-patterns, privacy qualifiers, and access guards.
- **`CLAUDE.md`** — persistent agent guidance file (auto-loaded by Claude Code); merges the
  `covenant-syntax` and `erc-822x` rule sets from `editors/cursor/rules/` into a single
  document covering V0.8 syntax rules, keyword aliases, and ERC-822x citation requirements.
- **Command** `/covenant-new` — scaffold a minimally-compiling `.cov` file for any of the
  14 top-level constructs using verified V0.8 syntax.
- **Command** `/covenant-migrate` — migrate a Solidity `.sol` file to Covenant, applying all
  11 anti-pattern transformations and selecting the most specialized construct.
- **Command** `/covenant-review` — defensive review of a `.cov` file; outputs structured
  findings by severity citing `docs/diagnostic-codes.md` and the syntax/ERC rules.
- **Subagent** `reviewer` — reads `.cov` files end-to-end and produces a defensive audit
  report grouped by severity (info / low / medium / high), with suggested patches.

### Notes

This plugin is the Claude Code counterpart to [`editors/cursor/`](../cursor/) (plugin v0.1.0)
and complements [`editors/vscode/`](../vscode/) (extension v0.7.1).
The VS Code extension provides syntax highlighting and LSP diagnostics; this plugin
teaches the Claude Code agent the Covenant language for AI-assisted workflows.

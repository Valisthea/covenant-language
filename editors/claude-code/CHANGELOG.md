# Changelog

All notable changes to the Covenant Claude Code plugin are documented here.

## [0.9.5] — 2026-07-24

### Changed
- **Version realigned to the compiler / VS Code line (0.9.5).** The plugin previously
  carried its own `0.2.0` scheme; it now tracks the shipping Covenant release version
  (`0.9.5`), matching the compiler and the VS Code extension. Current-release
  descriptions read "Covenant v0.9.5".

### Added
- **Knowledge updated for the OMEGA v0.9.5 fail-loud diagnostics.** Added a concise
  "Compiler diagnostics (fail-loud)" section to `CLAUDE.md` and
  `skills/covenant-expert/SKILL.md` listing the constructs the compiler now
  **refuses** to compile (E424 stdlib math builtins, E425 map introspection,
  E426 `in` operator, E427 map `.argmax`/`.argmin`, E512 >3 indexed event params,
  E519 divide/modulo by literal zero, E520 missing precompile helper, E521 >32-byte
  text constant, E522 nested maps) plus W508 (`only caller` allow-all no-op) and the
  fail-closed guard-principal errors (E516/E517/E518). The compiler is fail-loud: it
  errors rather than silently miscompiling, so the agent must not generate these
  constructs and should explain the error when a user hits one.

### Notes
- The ERC-8228 = **Cryptographic Amnesia** correction is already in place — a
  `ceremony` correctly cites `-- ERC-8228`. Left intact.

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

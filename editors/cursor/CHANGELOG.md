# Changelog

All notable changes to the Covenant Cursor plugin are documented here.

## [0.9.5]: 2026-07-24

### Changed
- **Version realigned to the compiler / VS Code line (0.9.5).** The plugin previously
  carried its own `0.2.0` scheme; it now tracks the Covenant compiler and VS Code
  extension release train. `.cursor-plugin/plugin.json`, README, commands, and the
  reviewer agent now describe the shipping release as **v0.9.5**.

### Added
- **Knowledge updated for the OMEGA v0.9.5 fail-loud diagnostics.** Added a
  "Compiler diagnostics (fail-loud)" section to `rules/covenant-syntax.mdc` and
  `skills/covenant-expert/SKILL.md` listing the constructs the compiler now refuses
  (E424 math builtins, E425 map introspection, E426 `in` operator, E427 map
  `.argmax`/`.argmin`, E512 >3 indexed event params, E519 literal-zero division,
  E520 missing precompile helper, E521 >32-byte text constant, E522 nested maps,
  W508 `only caller` no-op). The AI no longer generates these plausible-but-wrong
  constructs and explains the error when a user hits one.

### Notes
- The **ERC-8228 = Cryptographic Amnesia** correction is already in place (a `ceremony`
  cites ERC-8228); this release leaves that mapping intact.

## [0.2.0]: 2026-06-09

### Fixed
- **Corrected the ERC-8228 attribution.** The `ceremony` (amnesia) guidance no longer cites ERC-8228, that number was officially assigned to the Styx *Encrypted Token* Standard (`Valisthea/styx-erc-encrypted-token`), a different spec. The amnesia ceremony has no assigned ERC; the `erc-822x` rule no longer requires an ERC-8228 comment for `ceremony`, and the scaffold template emits a plain construct comment instead. Updated `rules/erc-822x.mdc`, `agents/reviewer.md`, `commands/new.md`, `commands/review.md`. **⚠️ REVERTED (2026-07-24): this was wrong. Per ethereum/ERCs PR #1681 (editor-renumbered 1681→8228, titled "Cryptographic Amnesia"), ERC-8228 IS the amnesia standard, a `ceremony` DOES cite ERC-8228.**
  > **Follow-up correction (reverted):** this change was based on a wrong mapping and has since been reverted. Per the canonical Styx Protocol mapping (draft standards authored by Kairos Lab), **ERC-8228 = Cryptographic Amnesia** (`Valisthea/styx-erc-cryptographic-amnesia`) and the *Encrypted Token* Standard is **ERC-8227**. The amnesia `ceremony` therefore *does* map to ERC-8228 and should cite it, exactly as `confidential token` cites ERC-8227.

### Changed
- Description tracked to Covenant **V0.9** (was V0.8).

## [0.1.0]: 2026-04-26

Initial release.

### Added

- **Skill** `covenant-expert`, agent-side Covenant V0.8 language expertise: all 14 top-level
  constructs, FHE/ZK/post-quantum/cryptographic-amnesia stack, ERC-8227/8228/8229/8231,
  the 11 Solidity migration anti-patterns, privacy qualifiers, and access guards.
- **Rule** `covenant-syntax`, persistent `.mdc` rule applied when editing `.cov` files;
  enforces comment syntax, type aliases, construct selection, and `vault` reentrancy default.
- **Rule** `erc-822x`, `.mdc` rule that requires ERC draft citation comments when generating
  `confidential token` (ERC-8227), `ceremony` (ERC-8228), `verified_by` (ERC-8229), or
  `pq_signed` (ERC-8231) constructs.
- **Command** `/covenant-new`, scaffold a minimally-compiling `.cov` file for any of the
  14 top-level constructs using verified V0.8 syntax.
- **Command** `/covenant-migrate`, migrate a Solidity `.sol` file to Covenant, applying all
  11 anti-pattern transformations and selecting the most specialized construct.
- **Command** `/covenant-review`, defensive review of a `.cov` file; outputs structured
  findings by severity citing `docs/diagnostic-codes.md` and the syntax/ERC rules.
- **Subagent** `reviewer`, reads `.cov` files end-to-end and produces a defensive audit
  report grouped by severity (info / low / medium / high), with suggested patches.

### Notes

This plugin is distinct from [`editors/vscode/`](../vscode/) (extension v0.7.1).
The VS Code extension provides syntax highlighting and LSP diagnostics; this plugin
teaches the Cursor agent the Covenant language for AI-assisted workflows.

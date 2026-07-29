# Changelog

## [0.9.4]: 2026-07-23

### Changed

- The language server now runs the **full compile pipeline** (`check_deep`) instead of
  frontend-only checking, so the compiler's fail-loud diagnostics surface as live squiggles
  while you type, `E424` (unimplemented stdlib math), `E425` (map introspection),
  `E519` (division by a literal zero), `E520` (missing precompile helper method),
  `E521` (text constant longer than 32 bytes). Previously these only appeared at `build` time.
- Version aligned with the `0.9.4` compiler release (fail-loud pass, silent miscompiles now
  error or work correctly). See the root [CHANGELOG](../../CHANGELOG.md).

## [0.9.3]: 2026-07-05

### Changed
- **Version bump 0.9.2 → 0.9.3** to track the compiler's V0.9.3 release, an OMEGA V6 self-audit cycle that found and fixed 6 Critical, 6 High, and 5 Medium severity defects (the largest single-cycle finding count since the V0.6 launch audit), including a Critical bug where `for each` never actually iterated and a High-severity uncatchable stack-overflow crash reachable from any `.cov` file the LSP opens (a few hundred bytes of nested parens or a long chained arithmetic expression). See the covenant-src `CHANGELOG.md` for the full list.
- Every diagnostic surfaced through this extension (hover, publishDiagnostics) reflects the fixed frontend as soon as the bundled `covenant-lsp` binary is refreshed, see the Note below.

### Note
- The bundled `covenant-lsp` binary should be refreshed from the v0.9.3 release assets (`covenant-lsp-win32-x64.exe`) when the VSIX is next repackaged, so `serverInfo.version` reports `0.9.3`. Until a v0.9.3 GitHub release is tagged and its binaries published, `scripts/package-vsix.js` has nothing to download, this bump is a source-level version sync, not yet a repackaged VSIX.

## [0.9.2]: 2026-06-09

### Changed
- **Version bump 0.8.2 → 0.9.2** to track the compiler's V0.9.2 security & correctness release. That release came out of a full compiler audit and fixed a Critical ERC-721 caller-authorization gap (+ zero-address receiver), a map storage-slot collision, wrapping checked-arithmetic, and dropped external-call return values. See the covenant-src `CHANGELOG.md` for detail.

### Note
- The bundled `covenant-lsp` binary should be refreshed from the v0.9.2 release assets (`covenant-lsp-win32-x64.exe`) when the VSIX is next repackaged, so `serverInfo.version` reports `0.9.2`. Diagnostics already reflect the current compiler frontend; LSP capabilities remain hover + symbols (completion/definition/rename are V0.9.x backlog).

## [0.8.2]: 2026-04-25

### Fixed
- **Workspace version sync**: all 17 crates had intra-workspace dependency versions hardcoded at `0.7.0` / `0.8.0`; updated to `0.8.2` so `cargo build` resolves cleanly without version-mismatch errors
- **LSP unit test**: `analyze_hello_produces_no_errors` now correctly filters `covenant-lint` findings (C100 on unguarded actions in tutorial fixtures is expected lint behavior, not a compiler error)
- **`covenant-lsp` binary rebuilt at 0.8.2**: bundled win32-x64 binary now reports `serverInfo.version: 0.8.2` (was `0.6.1` in PATH / `0.7.0` in stale release binary)

### Verified (E2E JSON-RPC test: 3/3 PASS)
- `textDocument/didOpen` on clean source → `publishDiagnostics` with 0 compiler errors
- `textDocument/didOpen` on `ghosttype` → `publishDiagnostics` with E102 compiler error
- Lint pipeline: C100 fires correctly on actions without access guards (source: `covenant-lint`)

## [0.8.1]: 2026-04-25

### Fixed
- **LSP binary not bundled**: `bin/covenant-lsp.exe` included in win32-x64 VSIX
- **LSP reported wrong version**: server now reports correct version in `serverInfo`
- **`bridge` snippet did not compile**: rewritten as `module`-based lock/unlock pattern
- **`ceremony` snippet did not compile**: corrected to `ceremony { guardians: N; threshold: M; on_destroy { ... } }` syntax
- **`covenant.runGasBenchmark` command had no handler**: registered in `activate()`, opens terminal + runs `covenant bench --print`
- **Problem matcher `$covenant` not contributed**: two-line pattern for `[E102] Error` + `╭─[file:line:col]`

## [0.8.0]: 2026-04-24

### Added
- **Snippets**: 20 code templates covering all major V0.8 constructs:
  - `bridge`: cross-chain asset escrow module (lock/unlock pattern)
  - `ceremony`: amnesia ceremony (`guardians`/`threshold`/`on_destroy` real syntax, compiles)
  - `etoken`: encrypted token (ERC-8227) with FHE balances and value hashes
  - `vault`: encrypted vault with FHE balance map
  - `ballot`: encrypted ballot with FHE voting
  - `@batch`: `@batch_up_to(N)` annotated batched action template
  - `@precompute`: compile-time `@precompute(keccak(...))` hash field
  - `@nonreentrant`: explicit `@non_reentrant` action template
  - `pqsigned`: `pq_signed(key)` post-quantum guarded action
  - `verified`: `verified_by(predicate)` ZK guard action
  - `vdflocked`: `vdf_locked(delay)` time-locked action
  - `sum`, `count`, `argmax`, aggregate view function templates
  - `shares`: Shamir Secret Sharing typed field
  - `match`, `tryaction`, `event`, `error`, `fhemap`, `pqueue`, core language templates
- **Grammar, new annotations** with dedicated scope `entity.name.function.annotation.covenant`:
  - `@batch_up_to`: loop unrolling bound
  - `@precompute`: compile-time evaluation
  - `@prove_offchain`: off-chain proof generation hint
  - `@verify_at_compile_time`: static verification annotation
  - `@audit`, `@invariant`, `@deprecated`, documentation annotations
- **Grammar, aggregate methods** with scope `support.function.aggregate.covenant`:
  - `.sum`, `.count`, `.max`, `.min`, `.avg`
  - `.argmax_by_address`, `.argmin_by_address`
  - `.any`, `.all`, `.first`, `.last`, `.length`
- **Grammar, intrinsic functions** with scope `support.function.builtin.covenant`:
  - `decrypt`, `encrypt`, `hash`, `keccak`, `sha256`, `fhe_random_bytes`
  - `destroy`, `freeze`, `verify_proof`, `sign`, `commit`, `reveal_value`, `noise_budget`
- **Grammar, new types**: `amnesia_phase`, `validator_set`, `fhe`, `u8`, `u256`, `i8`, `i256`
- **Grammar, guard keywords** with dedicated scope `keyword.other.guard.covenant`:
  - `verified_by`, `pq_signed`, `anchored_on`, `upgradeable_by`, `vdf_locked`
- **Grammar, new keywords**: `phase`, `helper`, `constant`, `idle`, `gathering`, `finalized`, `destroyed`, `deployer`, `caller`, `validators`
- **New command**: `Covenant: Run Gas Benchmark` in the command palette
- **Categories**: Added `Snippets` to extension marketplace categories

### Changed
- `covenant-lsp` bundled binary rebuilt at version **0.8.0** (workspace bump from 0.7.0)
- `bridge` snippet corrected to `module`-based lock/unlock pattern (`anchored_on [...]` did not compile)
- `ceremony` snippet corrected to `{ guardians: N; threshold: M; on_destroy { ... } }` real syntax
- `taskDefinitions` target: `["evm"]` only, WASM is a preview stub in V0.8, removed from enum
- Extension description: removed WASM/bridge/amnesia marketing claims (WASM backend is a stub)
- Grammar annotations split: named → `entity.name.function.annotation.covenant`; fallback → `entity.other.attribute-name.covenant`

### Fixed
- `covenant.runGasBenchmark` command: **registered handler added** (opens terminal, runs `covenant bench --print`)
- Problem matcher `$covenant` contributed, `covenant build` errors surface in VS Code Problems panel
- Highlighting for `verified_by(...)` guard expressions
- Highlighting for `destroy(field)` and `freeze(field)` intrinsics

### Known issues (V0.8)
- LSP capabilities: hover + symbols only. Completion / definition / rename respond `-32601 Method not found` (V0.9)
- LSP false positives E020 on `public amount`, `private amount`, `sealed amount`, `[T; N]`, compiler accepts these; LSP grammar lags. Workaround: `"covenant.lsp.enabled": false`
- Non-win32 VSIX (darwin/linux): `covenant-lsp` not bundled, uses PATH fallback

### Compatibility
- Compatible with Covenant compiler V0.8.0+
- Requires VS Code 1.85.0+
- win32-x64: bundled `covenant-lsp` 0.8.0 · darwin/linux: PATH fallback
- OMEGA V4 audited (41 findings resolved, 0 open)

## [0.7.1]: 2026-04-22

### Added
- Platform-specific bundled `covenant-lsp` binary, install from Marketplace and open any `.cov` file immediately, no Rust or PATH setup needed (linux-x64, linux-arm64, darwin-x64, darwin-arm64, win32-x64)
- Binary resolution order: bundled → `covenant.lsp.path` config → system PATH

### Changed
- `covenant.lsp.path` setting description updated to reflect bundled binary as the recommended default

## [0.7.0]: 2026-04-22

### Added
- Lint diagnostics wired into LSP, `covenant-lsp` now runs the security linter (`covenant-lint`) after a clean frontend pass, emitting E4xx/W0xx findings (e.g. E421, W003) as squiggles in the Problems panel alongside parse and type errors
- Platform-specific bundled binaries, the extension now ships `covenant-lsp` for each platform (linux-x64, linux-arm64, darwin-x64, darwin-arm64, win32-x64). Zero Rust/PATH setup required: install from Marketplace and start editing `.cov` files immediately
- Binary resolution order: bundled → explicit `covenant.lsp.path` config → system PATH, developer overrides still work

## [0.6.5]: 2026-04-22

### Fixed
- LSP client now detects when `covenant-lsp` is not in PATH and shows a warning with a link to the installation guide instead of silently failing
- Improved error reporting if the language server crashes at startup

### Added
- Binary resolution via `where`/`which` before starting the LSP, prevents cryptic ENOENT errors in the Output panel

## [0.6.3]: 2026-04-22

### Changed
- Updated icon to bracket + purple square design matching Covenant visual identity

## [0.6.2]: 2026-04-22

### Added
- Full marketplace README with language samples, settings table, audit summary, and roadmap
- 128×128 icon

## [0.6.1]: 2026-04-22

### Added
- Initial release
- Syntax highlighting, full TextMate grammar for all Covenant keywords, types, privacy qualifiers, annotations, operators, and `--` line comments
- LSP client, connects to `covenant-lsp` via stdio; real-time diagnostics, hover documentation, document symbols
- Language configuration, bracket auto-close, comment toggle (`Ctrl+/`), smart indentation, code folding
- Settings, `covenant.lsp.path` and `covenant.lsp.enabled`

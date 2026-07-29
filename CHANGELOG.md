# Covenant — Changelog

Release history for the Covenant compiler and specifications.

Format : [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versioning : [SemVer](https://semver.org) when feature-complete ; pre-release tags for research milestones.

> **⚠️ Naming note (updated 2026-07-24)** : ERC-8228 IS the **Cryptographic
> Amnesia** standard — ethereum/ERCs PR #1681 (editor-renumbered 1681→8228,
> titled "Cryptographic Amnesia"). The Encrypted Token Standard is a
> separate proposal, **ERC-8227**
> ([Valisthea/styx-erc-encrypted-token](https://github.com/Valisthea/styx-erc-encrypted-token)).
> Pre-V0.9.0 entries referencing "ERC-8228" for the amnesia ceremony were
> correct. Internal Rust module rename (`covenant-stdlib::erc8228`
> → `covenant-stdlib::amnesia_ceremony`) is tracked in `DEBT.md` for
> V0.9.1. Historical entries below are preserved verbatim — they
> reflect the project's state at that time.

For the historical record of project firsts (first deploy, first ceremony,
etc.), see [`MILESTONES.md`](./MILESTONES.md).

---

## [Unreleased]

### Fixed, fail-loud

- **`transfer <amount> from <src> to <dst>` is refused (`E523`).** The parser accepted
  the three-operand form and the IR builder lowered all three operands, but
  `emit_transfer` destructured the operand list as `(operands[0], operands[2])`, so the
  `from` operand was read, lowered, and then silently discarded. The statement compiled
  clean, raised no diagnostic, and emitted a plain `CALL` paying `<dst>` out of the
  **contract's own balance** while ignoring the source named in the source text. A
  silent miscompile on a value path. There is no EVM primitive that spends the native
  balance of an account the executing contract does not control, so the form has no
  faithful lowering and is now rejected at compile time rather than mis-lowered.
  `covenant-evm-backend::codegen::emit_transfer` + `tests/transfer_from_hardfail.rs`
  (negative-control verified: neutralise the guard and the rejection test fails while
  the two-operand control keeps passing).

---

## [0.9.5] — 2026-07-24 (OMEGA adversarial bounty pass — reveal access-control + fail-loud sweep)

> An internal OMEGA adversarial bounty run against v0.9.4 (two-block aggressive-generator /
> hostile-critic, 210 probes, every PoC confirmed on anvil, source citations checked) surfaced
> **1 Critical + 2 High + 4 Medium + 2 Low**. All fixed here, each with a negative-controlled
> regression test. **1102 tests, clippy-clean, fmt-clean.**

### Fixed — Critical

- **Reveal access-control gate (F07).** `reveal <field> to <target>` compiled with **zero** caller
  check — the owner-only disclosure restriction was silently unenforced (the target was dropped at
  IR lowering, so the reveal reached the backend guardless). The reveal now emits the
  `msg.sender == owner` gate, reusing the same `only <principal>` codegen: `to owner` resolves to the
  `owner` field or the deployer, `to caller` is public, and collection / unresolved targets fail
  closed. **Anvil-verified: a non-owner reveal reverts, the owner's succeeds.**
  `covenant-ir::lower_reveal` + `reveal_access_control.rs`.

### Fixed — High

- **`in` membership operator (F01, `E426`).** `given x in list` lowered to a single scalar `EQ`
  (an `In => Opcode::Eq` placeholder) — a membership guard that passed only for the first element.
  Now **fail-loud** (`E426`) until a real `ListContains` compare-loop lands.
- **Nested-map writes (F09, `E522`).** `inner[a][b] = v` emitted zero `SSTORE`, returned success,
  and read back `0` — a silent dropped write on the allowance/`owner→spender→amount` pattern. Now
  **fail-loud** (`E522 nested map not yet supported`) instead of a success-returning no-op.

### Fixed — Medium

- **Map `.argmax`/`.argmin` (F02, `E427`).** Fell through to `StructGet(0)` (no iteration, always
  returned `0`). Now fail-loud for **maps**; list `.argmax`/`.argmin` still work.
- **Over-indexed events (F04, `E512`).** A non-anonymous event with >3 `indexed` params compiled
  clean but the `emit` lowered to an unconditional `REVERT` and shipped an invalid ABI. Now rejected
  at compile time (`E512`).
- **`parse_type` stack overflow (F06).** A deeply-nested `map<…>` **type** overflowed the Rust stack
  — an uncatchable crash on every subcommand and the LSP. Added a type-nesting depth guard mirroring
  the expression-depth `E031`.
- **Reveal ABI (F08).** The reveal function's ABI declared `outputs:[]` / `nonpayable` while the
  runtime is read-only and returns 32 bytes of plaintext. Now emits `stateMutability:"view"` + the
  real output type.

### Fixed — Low

- **Ceremony threshold validation (F10).** `threshold: 0` (or `threshold > guardians`) compiled clean
  and degenerated the finalize gate to `count >= 0` (always true → finalize with **zero** guardian
  shares, re-opening the CRT-005 fail-open via a degenerate config). Now validated
  `1 <= threshold <= guardians` at compile time.
- **`only caller` no-op (F05, `W508`).** `only caller` emitted an allow-all no-op while every other
  degenerate principal fails closed with `W421`. Now flagged, consistent with that path.

### Also

- **ERC mapping corrected** to the canonical Styx numbering — ERC-8227 Encrypted Token,
  **ERC-8228 Cryptographic Amnesia**, ERC-8229 FHE Verification, ERC-8231 PQ Key Registry — per
  ethereum/ERCs PR #1681 (editor-renumbered 1681 → 8228). Reverses an earlier mis-statement that
  the amnesia ceremony had no assigned ERC.

## [0.9.4] — 2026-07-23 (fail-loud pass — no more silent miscompiles)

> The pre-open-source hardening release. Seven classes of **silent miscompile**
> — ordinary-looking source that compiled clean and produced wrong on-chain
> behaviour with no diagnostic — are now either implemented correctly or
> refused at compile time. One was found by the fuzzer; none was caught by a
> passing test suite, which is the point. The language server now surfaces
> these diagnostics live, and CI is green.

### Fixed — fail-loud (source that silently mis-compiled now errors, or works)

- **`min`/`max`/`abs`/`pow`/`sqrt` no longer lower to `a + b`** (E424). They
  mapped to `AddChecked` as a placeholder, so `max(cap, bid)` returned
  `cap + bid` — type-checked, compiled clean. Refused until a real
  compare-and-branch lowering exists.
- **Division / remainder by zero reverts** (E519) instead of yielding 0. EVM
  `DIV`/`MOD` are total (`x / 0 == 0`), so `pot / participants` silently paid 0
  to everyone on an empty set. A literal `x / 0` is now a compile error.
- **Crypto opcodes with no helper method are rejected** (E520) instead of
  emitting a selector that matches nothing on the deployed helper. The
  consequential case was `FheCmpGe`: every `confidential token` compiled for
  Sepolia would have shipped a contract that reverts on the first
  `transferEncrypted`.
- **`map.length` / `.keys` / `.values` are refused** (E425) instead of
  compiling to a constant 0 — `for each k in m.keys` used to run zero
  iterations on clean-compiling source.
- **Over-long text constants report E521** instead of crashing the compiler.
  A token whose `name:` exceeded 32 bytes hit a bare `assert!` and aborted with
  an internal compiler error. **Found by the cargo-fuzz `compile_pipeline`
  target**; the crashing input is checked into the corpus so every run replays it.
- **`test_*` actions are stripped from release builds.** They compiled into the
  deployed contract as public functions — a test that mutates was a public,
  unguarded state mutator. Release builds omit them; the `covenant test`
  harness keeps them. Proven: a contract and the same contract + tests now
  produce byte-identical runtime bytecode.
- **Constant field defaults are written at deploy** (integers, bools,
  address/hash). `field a: amount = 42` was silently dropped, so the field read
  back as 0 on chain; now SSTOREd in the constructor.

### Changed

- The language server (`covenant-lsp`) now runs the whole pipeline, not just the
  frontend, so the diagnostics above appear as editor squiggles instead of only
  failing at build time (`covenant_driver::check_deep`).

### Infrastructure

- CI is green again: `cargo fmt` drift cleared, a `clippy` lint fixed, and all
  four jobs pinned to `rust-toolchain.toml`'s `stable` (they had hardcoded 1.81,
  which diverged from the local toolchain and masked lints).

1,080+ tests passing · clippy-clean · all four CI jobs green.

---

## [0.9.3] — 2026-07-05 (V0.9.3 patch — 6 Critical + 6 High + 5 Medium from OMEGA V6 self-audit)

> Patch release from a full OMEGA V6 self-audit (breadth-first adversarial
> sweep across compiler, stdlib, and produced-contract surfaces; every
> finding empirically verified against source and reproduced with a new
> end-to-end regression test before being reported as a finding). Fixes
> 6 **Critical**, 6 **High**, and 5 **Medium** severity defects — the
> largest single-cycle finding count since the V0.6 launch audit. Full
> per-finding write-ups and remediation detail:
> `covenant-audits/audits/2026-07-05-omega-v6-covenant-v0.9.2/`.
> As with V0.9.2 : bytecode/storage layout for contracts already deployed
> is unchanged ; only NEW compiles get the fixes. Full dynamic-`bytes`
> storage+ABI remains out of scope (tracked in `DEBT.md`) — see the
> MED-003 entry below for the honest interim signal instead.

### Fixed — security (Critical)

- **`if` without `else` orphans the success path** (CRT-002). The dead-code
  eliminator's `Terminator::Unreachable` guard treated a then-only `if`'s
  implicit merge block as unreachable, deleting every statement after it —
  including the shipped `07_revert_paths.cov` fixture's own success path.
  (`covenant-ir/src/builder.rs`.)
- **`for each` executed its body exactly once, with no real iteration**
  (CRT-003). Rewritten as a real block-parameter-based loop (header/body/
  merge blocks, comparison, per-iteration element read, checked increment,
  back-edge jump) — and since this touched the same surface, `append` and
  `list[idx].field` read/write are now real dynamic-array storage
  (Solidity keccak-derived-slot convention) instead of no-ops.
  (`covenant-ir/src/builder.rs`, `covenant-evm-backend/src/codegen.rs`.)
- **`only <builtin_predicate>` guards compiled to unconditional `true`**
  (CRT-004). Every `BuiltinPredicate` variant beyond owner/admin/deployer/
  address (~16 variants, including `first_time_caller` and
  `registered_key`) silently authorized every caller. Now a hard compile
  error (E518) instead of a defeated guard. (`covenant-evm-backend/src/codegen.rs`.)
- **Amnesia ceremony had zero threshold enforcement** (CRT-005). `finalize()`
  trusted the (mocked) precompile's boolean with no on-chain corroboration
  that any guardian shares were actually submitted — a ceremony could
  finalize with zero shares submitted. `finalize()` now asserts a real
  count of distinct submitters against the threshold first; `submit_share`
  dedupes by caller. (`covenant-stdlib/src/amnesia_ceremony.rs`.)
- **Builtin identifiers silently shadowed same-named user fields**
  (CRT-006). A user field/param named `caller`/`now`/`block`/`msg`/
  `current_block` silently won or lost against the language-provided
  binding depending on seeding order, with no diagnostic either way. The
  resolver now checks for an existing binding in the parent scope before
  seeding a builtin. (`covenant-resolver/src/resolver.rs`.)
- **ERC-8231 PQ-key registry ABI/codegen mismatch** (CRT-007). `pq_key`
  (used by `register(bytes)`/`key_of(...) returns bytes`) declares ABI
  type `bytes` (dynamic) but codegen could only read/return it as a single
  32-byte word — a spec-compliant caller's ABI encoding would silently
  corrupt the stored key. The `registry` construct now hard-fails
  compilation (E505) until real dynamic-`bytes` support lands.
  (`covenant-evm-backend/src/codegen.rs`.)

### Fixed — security (High)

- **Type-position identifiers resolving to non-types were silently
  `Ty::Unknown`** (HGH-026). `field x: some_action` (naming an action,
  event, error, etc. instead of a struct/credential) compiled with an
  unchecked `Unknown`-typed field instead of failing at the mistake. Now
  raises E231 for any identifier that resolves to something real but
  non-type-shaped. (`covenant-types/src/checker.rs`.)
- **External-contract calls were never type-checked against the declared
  interface** (HGH-027). `IFoo.at(addr).method(args)` chains hit a
  permissive `Ty::Unknown` fallback unconditionally — wrong arity, wrong
  argument types, and typo'd method names all passed type-check, with the
  mistake surfacing (if at all) as an unexplained runtime revert. Now
  checked against the matching `function` in the `external contract`
  block (E205/E201/E203). (`covenant-types/src/checker.rs`.)
- **Non-view external calls always reverted** (HGH-028). `emit_external_call`
  pushed `addr` before `value` on the non-view CALL path, but CALL's real
  pop order needs `value` first — every non-view `IFoo.at(addr).action(...)`
  call attempted to "send" a fabricated huge `value` and failed with
  insufficient balance. (`covenant-evm-backend/src/codegen.rs`.)
- **Unbounded AST recursion — uncatchable stack-overflow crash** (HGH-029).
  Every recursive-descent AST walker (the Pratt expression/block parser,
  the resolver's `resolve_expr`, the typechecker's `synth_expr`) recursed
  one native stack frame per nesting level with no depth counter. A few
  hundred bytes of nested parens, or a long chained `+` expression,
  overflowed the process stack — an uncatchable `STATUS_STACK_OVERFLOW`,
  not a normal Rust panic — across every subcommand and the LSP. Each
  stage now bounds its own recursion depth independently and raises a
  normal diagnostic (E031/E113/E232) instead.
  (`covenant-parser`, `covenant-resolver`, `covenant-types`.)
- **Ceremony `guardians:`/`threshold:` source values silently discarded**
  (HGH-030, fixed alongside CRT-005). The ceremony synthesizer hardcoded
  `(3, 2)` regardless of the `.cov` source's declared values; now reads
  the real threshold from `module.metadata`.
  (`covenant-stdlib/src/amnesia_ceremony.rs`.)
- **No warning when FHE/PQ/ZK constructs route to `Mocked*` contracts on a
  real testnet** (HGH-031). The `Mocked*.sol` helpers' "V0.9 PLACEHOLDER —
  NOT FOR PRODUCTION SECRETS" labeling lived only in Solidity comments a
  `covenant` CLI user never sees. `covenant build --target-chain=sepolia`
  now prints `warning[mocked-crypto]` (naming the primitive and helper
  contract) and always populates a `mockedCryptoPrimitives` field in
  `.metadata.json`, mirroring the existing `build_aster_target` warning
  precedent. (`covenant-cli/src/commands/build.rs`,
  `covenant-evm-backend/src/mocked_crypto.rs`.)

### Fixed — correctness (Medium)

- **Privacy analyzer false-positive on `ciphertext<T>` struct fields in
  lists** (MED-001). `Analyzer::lvalue_domain` approximated the domain for
  `FieldAccess`/`Index` lvalue targets instead of resolving the precise
  field type, so legitimate `list<Struct>` writes with a `ciphertext<T>`
  field were rejected as an E301 privacy violation.
  (`covenant-privacy/src/analyzer.rs`.)
- **Non-constant dynamic-type returns silently encoded as a single raw
  word** (MED-003). `view read returns text { some_field }` (any
  non-compile-time-constant text/bytes/list return) ABI-encoded as a
  single word instead of offset+length+data, with zero diagnostic. Now
  raises `W507_DYNAMIC_RETURN_NOT_ENCODED` — a warning, not a hard error:
  this pattern is the language's own Hello World example, too common to
  block compilation on before real dynamic-ABI encoding lands.
  (`covenant-evm-backend/src/codegen.rs`.)
- **`MockChain`'s `ZK_VDF_VERIFY` had no fail-mode at all** (MED-005),
  unlike every other verify-shaped mock precompile. Local tests could
  never exercise "VDF proof rejected" even though the real Sepolia-
  deployed helper can fail. Now gates on `zk_force_fail`, symmetric with
  `ZK_VERIFY`. (`covenant-evm-runtime/src/precompiles.rs`.)
- `docs/v0.9/known-acceptable-risks.md`'s "Compiler fuzz coverage" entry
  corrected (MED-002) : it claimed no fuzz harness existed and that the
  parser had bounded recursion — a harness exists but has never been run,
  and recursion is *now* genuinely bounded thanks to HGH-029.
- `ListLength` under-reporting (MED-004) required no separate fix — it was
  already resolved by the CRT-003 real-storage rewrite; verified via
  `covenant-testing/tests/list_of_struct_lifecycle.rs`.

### Added

- New regression tests, one or more per finding above: `covenant-ir/tests/unit.rs`
  (CRT-002), `covenant-testing/tests/list_of_struct_lifecycle.rs` (CRT-003 /
  MED-001 / MED-004), `covenant-evm-backend/tests/pq_registry_hardfail.rs`
  (CRT-007), `covenant-types/tests/unit.rs` (HGH-026, HGH-027, HGH-029),
  `covenant-testing/tests/external_call_stack_order.rs` (HGH-028),
  `covenant-parser/tests/errors.rs` + `covenant-resolver/tests/errors.rs`
  (HGH-029), `covenant-evm-backend/tests/mocked_crypto_metadata.rs`
  (HGH-031), `covenant-evm-backend/tests/dynamic_return_warning.rs`
  (MED-003), `covenant-evm-runtime/src/precompiles.rs` (MED-005).

---

## [0.9.2] — 2026-06-09 (V0.9.2 patch — security & correctness from full compiler audit)

> Patch release from a full adversarial audit of the compiler (37-agent
> sweep across 7 dimensions, every finding verified against source, then
> reproduced with new end-to-end tests). Fixes one **Critical** and three
> **High** severity defects plus supporting Mediums. All fixes are
> internal codegen / synthesis changes — **bytecode & storage layout for
> contracts already deployed under V0.9.0/V0.9.1 are unchanged; only NEW
> compiles get the fixes.** Larger items (full dynamic-`bytes` storage/ABI,
> external-call argument type-checking) are scoped to V0.9.3 and tracked in
> `DEBT.md`.

### Fixed — security

- **CRITICAL — ERC-721 `transferFrom` performed no caller authorization**
  (anyone could steal any NFT). The auto-synthesized `transferFrom` only
  checked `ownerOf(token_id) == from`, where `from` is an attacker-chosen
  parameter — it never loaded `msg.sender`. Any account could call
  `transferFrom(victim, attacker, id)` for any token the victim owned and
  seize it; the audit fixture even *documented* the (absent) gate. Now
  enforces OpenZeppelin `_isAuthorized` semantics: caller must be the token
  owner, the per-token approved address, or an approved operator, else
  `NotApprovedOrOwner`. Shared `emit_is_authorized` helper also gates
  `burn`. (`covenant-stdlib/src/erc721.rs`.)

- **HIGH — storage-slot collision: all maps hashed `keccak(key ‖ 0)`**
  (KSR-CVN-029). Keyed map slots used the map's metadata-slot *value*
  (always `0` for a fresh map) instead of the slot *number*, so any two
  maps accessed with the same key value aliased the same storage slot.
  For ERC-721 this meant `approve(spender, id)` (writing
  `token_approvals[id]`) silently overwrote `owners[id]`; for ERC-8231,
  `keys[caller]` and `registered[caller]` clobbered each other. `MapGet`/
  `MapSet`/`MapDelete` now trace the base operand back to the field's slot
  number and hash `keccak(key ‖ slot_number)`; the constructor genesis-
  balance writer is updated in lockstep. (`covenant-evm-backend/src/codegen.rs`.)

- **HIGH — `@slot(N)` could silently alias reserved compiler slots**
  (KSR-CVN-042). A user `@slot` landing on `DEPLOYER_SLOT` (backs
  `only deployer`) or `REENTRANT_LOCK_SLOT` (backs `@non_reentrant`) is now
  reported through the E423 slot-conflict path instead of silently
  hijacking auth / disabling the reentrancy guard. (`covenant-evm-backend/src/storage.rs`.)

### Fixed — correctness

- **HIGH — checked arithmetic now reverts on overflow/underflow**
  (KSR-CVN-031). `AddChecked` / `SubChecked` / `MulChecked` (the lowering
  of Covenant `+` / `-` / `*`) lowered to bare wrapping EVM `ADD`/`SUB`/
  `MUL`, so e.g. `balance = balance - amount` would wrap to
  ~`type(uint256).max` on underflow rather than reverting. They now emit
  the appropriate overflow/underflow check and revert. (`covenant-evm-backend/src/codegen.rs`.)

- **HIGH — external-contract calls returned 0/default** (KSR-CVN-030,
  completes the M3 milestone for statically-typed returns). After the
  `STATICCALL`/`CALL`, codegen zeroed `mem[0x00]` *before* reading it —
  wiping the return word the call had just written to `retOffset=0`. Every
  `IFoo.at(addr).method()` view returned 0/default. The post-call wipe is
  removed; the return word is read, guarded by `RETURNDATASIZE` so an empty
  return still yields 0. (Dynamic-typed returns like `string`/`bytes` still
  need offset+length decoding — V0.9.3, see `DEBT.md`.)
  (`covenant-evm-backend/src/codegen.rs`.)

### Added

- **`burn(token_id)` auto-synthesized for `nft` constructs** — gated by the
  same authorization predicate as `transferFrom`, clears the approval,
  zeroes ownership, decrements the owner balance, and emits
  `Transfer(owner, address(0), id)` (ERC-721 burn convention). New
  `InvalidReceiver` error; `transferFrom` now also reverts on
  `to == address(0)`. (`covenant-stdlib/src/erc721.rs`.)

- **New regression tests**: `covenant-testing/tests/erc721_authorization.rs`
  (9 end-to-end tests: unauthorized-steal blocked, owner/approved/operator
  transfers, zero-address & wrong-owner reverts, burn auth + lifecycle,
  zero-owner free-mint blocked), `covenant-testing/tests/checked_arithmetic.rs`
  (underflow reverts), plus reserved-slot and external-call-returndata guards
  in the `covenant-evm-backend` test suite.

### Notes

- Deferred to **V0.9.3** (tracked in `DEBT.md`): full dynamic-`bytes`/`string`
  storage + ABI return/param encoding (root cause of the ERC-8231 `key_of`
  return-shape issue and dynamic external-call returns), external-contract
  call argument/return type-checking, `Ty::Unknown` propagation hardening,
  stdlib `min`/`max`/`abs`/`pow`/`sqrt` placeholder lowering, and `div`/`mod`
  by-zero revert semantics.

---

## [0.9.1] — 2026-04-27 (V0.9.1 patch — empirical-loop fixes from M2/M3/M5)

> Patch release fixing 4 V0.9.0 items surfaced empirically by the
> M2/M3/M5 deploy loops (2026-04-26 / 04-27). All 4 fixes are
> internal — bytecode/ABI semantics for V0.9.0-deployed contracts
> are unchanged ; only NEW compiles benefit from V0.9.1.

### Added

- **`covenant doctor --strict` flag** : exits non-zero if any probe
  is in `Failed` state (warnings still don't trigger non-zero exit).
  Designed for CI gates : `covenant doctor --strict || exit 1`
  blocks the pipeline if the dev env can't reproduce the V0.9.x
  baseline. (DEBT.md V0.9.x candidate G — closed.)

- **`Binding::ExternalContract` + `DeclKind::ExternalContract`** in
  `covenant-resolver` : the resolver now registers each `external
  contract IFoo { ... }` declaration's name in the construct scope
  before pass1, so body expressions like `IFoo.at(addr).method(args)`
  resolve `IFoo` to a known binding. The IR builder + codegen already
  handled external contracts ; this closes the previously-orphan
  resolver gap. Unblocks **M3 milestone compilation** (first cross-
  contract Covenant call on Sepolia). Caught empirically during the
  M3 fixture build attempt 2026-04-27. (DEBT.md `external contract`
  resolver entry — RESOLVED.)

  **Note** : the codegen STATICCALL chain still returns defaults at
  runtime — M3 deploys + state writes work, cross-contract reads
  return zero. Tracked separately as a V0.9.2 follow-up (DEBT.md
  `external contract codegen` entry).

### Changed

- **Internal Rust module rename** : `covenant-stdlib::erc8228` →
  `covenant-stdlib::amnesia_ceremony`. Public exports renamed :
  `ERC8228_CANONICAL_SELECTORS` → `AMNESIA_CEREMONY_CANONICAL_SELECTORS`.
  Reflects the disambiguation done at the doc layer in V0.9.0
  (commit c0c1f92) — EIP-8228 was officially assigned to Styx
  Encrypted Token Standard (`Valisthea/styx-erc-encrypted-token`),
  so the Covenant amnesia ceremony pattern carries no EIP number.
  No bytecode change. (DEBT.md V0.9.0→V0.9.1 candidate — closed.)

  > **Follow-up correction** : the above claim was mistaken and has
  > since been reverted. Per the canonical Styx Protocol mapping (draft
  > standards authored by Kairos Lab), **ERC-8228 = Cryptographic
  > Amnesia** (`Valisthea/styx-erc-cryptographic-amnesia`), and the
  > Encrypted Token Standard is **ERC-8227**. The amnesia ceremony
  > therefore *does* map to ERC-8228 and should cite it, exactly as
  > confidential-token cites ERC-8227. The Rust module name
  > `amnesia_ceremony` remains fine, but its ERC-8228 attribution is
  > correct, not a misnomer.

- **Workspace bump** : `0.9.0` → `0.9.1` across the 22-crate
  workspace + per-crate Cargo.toml inter-crate version pins.

### Type checker — permissive `IFoo.at(addr).method(args)` chains

The type checker's call-on-FieldAccess path now returns `Ty::Unknown`
silently when the base_ty is `Unknown` (typically from an external
contract reference). This unblocks compilation of cross-contract call
chains. Trade-off : typos like `IFoo.at(addr).balanccOf(...)` now
type-check too (the resulting bytecode call will revert at runtime
if the wrong selector is dispatched). Tighter method-level signature
checking is V0.9.x backlog.

### Did NOT ship in V0.9.1 (deferred to V0.9.2)

  - ERC-721 `transferFrom`-to-zero strict revert + explicit `burn`
    action (DEBT.md M2 finding) — non-blocking for V0.9.0 deployed
    NFTs ; testnet-only by design.
  - ERC-8231 `key_of(address) returns bytes` return-shape mismatch
    (DEBT.md M5 finding) — auto-synth returns uint256 marker instead
    of bytes payload ; same V0.9.0 NFTs work for compute-only flows
    that don't read back.
  - External contract codegen STATICCALL chain returning defaults
    (DEBT.md M3 finding, see Note above).
  - LSP debounce, IR-instrumented coverage, fuzz harness, SBOM,
    reproducible-build CI gate (V0.9.x backlog tracked in ROADMAP.md
    H2).
  - VS Code extension bump 0.8.2 → 0.9.x (separate repo, separate
    release cycle).

### Verification

  - cargo test --workspace --lib : 148 passed
  - cargo test -p covenant-cli  : 36+56+2 = 94 passed
  - cargo clippy --workspace --all-targets -- -D warnings : PASS
  - cargo run -- --version : `covenant 0.9.1` ✓
  - covenant build examples/audit/10_m3_cross_contract.cov
    --target-chain=sepolia : ok 433 bytes (would have failed E102
    in V0.9.0)

### Tag

`v0.9.1` on commit (TBD-this-commit) of branch `release/v0.9.1`,
fast-forward merged to `main`.

---

---

## [0.9.0] — 2026-04-26 (V0.9 GA — Helper Bridge + Audit Prep)

> **OMEGA V5 self-audit gate : ✅ PASS** (see
> [`OMEGA_V5_AUDIT_REPORT.md`](./OMEGA_V5_AUDIT_REPORT.md)).
>
> **Scope** : compiler V0.9 master plan Sprints 27-46 (16 weeks).
> Helper-contract bridge architecture (Sepolia + Aster-ready),
> stdlib synthesis (ERC-721 + ERC-8231), CLI ergonomics
> (test isolation, coverage, doctor, init), LSP go-to-definition,
> source-scan lint detector, audit-prep package
> (SECURITY.md + threat model + audit fixture pack +
> CeremonyHelper deep-dive).
>
> **Live milestones** :
>   - **M0** — first Hello-on-Sepolia at `0xab083fc4...`
>   - **M1** — first end-to-end Covenant ceremony on Sepolia at
>     `0x2FB87d54...` (5 lifecycle txs + 4 Sprint 31.b bugs caught
>     empirically and fixed)
>
> **Not in V0.9.0** : real cryptography (mocked by design ;
> `Mocked*` helpers + `onlyTestnet` modifier), mainnet helpers
> (compile-time + runtime gated), Aster end-to-end deploy
> (codegen ready, deploy deferred V0.9.x), external audit
> (V1.0 gate).

### V0.9 Phase A.1 (Precompile Bridge) — COMPLETE — Sprints 29-32

This phase makes Covenant constructs that depend on cryptographic
primitives (`ceremony`, `encrypted counter`, `pq_signed`, `verified_by`)
work on real Ethereum testnets, not just the in-tab MockChain.

**M1 milestone reached on 2026-04-26** — see
[`MILESTONES.md`](./MILESTONES.md#m1) for the full record. KSR-CVN-PRELIM-005
(V0.8 audit) status flipped to FIX VERIFIED.

#### Added

- **`Target` enum + per-target `PrecompileAddresses` resolution**
  (`covenant-evm-backend/src/target.rs`). Three targets shipped:
  MockChain (V0.8 default), Sepolia, AsterTestnet. Mainnet rejected at
  parse time.
- **`EvmAddress = [u8; 20]`** — `PrecompileAddresses` field type widened
  from `u16` to full 20-byte EVM addresses. Codegen emits `PUSH20`
  instead of `PUSH2`. MockChain backward-compatible via `lift_v08_addr`.
- **CLI `--target-chain={mockchain,sepolia,aster_testnet}`** in
  `covenant-cli/src/commands/build.rs`. Existing `aster` value
  (Aster native backend) preserved.
- **`compile_to_evm_for_target(source, target)`** WASM-bindgen export
  in `covenant-wasm-bindings/src/lib.rs`. Legacy `compile_to_evm(source)`
  preserved as MockChain shorthand.
- **`config/helper-addresses-v0.9.0.json`** — registry of deployed
  helper-contract addresses. Sepolia entry now populated with real
  deploy block + deployer + Etherscan-verified status.
- **`docs/v0.9/`** — three design docs (`precompile-bridge-architecture.md`,
  `helper-interfaces.md`, `address-resolution.md`) and the
  `helper-source-audit-checklist.md` documenting the V0.9 → V1.0 swap-in
  plan.
- **`helpers/`** — Foundry sub-project shipping the four helper contracts
  with 34/34 unit tests passing. Deployed to Sepolia at the predicted
  CREATE2 addresses on 2026-04-26.

#### Resolved

- **KSR-CVN-PRELIM-005 (V0.8 OMEGA V4 audit)** — `OP_CALL` to non-precompile
  addresses silently no-op'ing on Sepolia. Resolution: helper contracts
  are deployed at the addresses the compiler emits, and they DO execute
  their bytecode (verified end-to-end on real Sepolia, tx hash in
  MILESTONES.md M1).

### V0.9 Phase A.2 (External Calls API + Security) — VERIFIED EXISTING

Sprint 33 (external `interface` declarations + `call_interface(addr)`
syntax) and Sprint 34 (default `@non_reentrant` + `try_action`/`catch`)
were found to be substantially shipped already in V0.7-V0.8 work
(Phase 17 Solidity interop + V0.8 audit fixes). No new work landed in
V0.9 sprints; the existing surface was confirmed via the
`external_call_codegen.rs` test suite.

### V0.9 Phase A.3 (NFT + Registry) — Sprint 35

#### Added

- **`nft` keyword** as the 13th top-level construct (Sprint 35.a).
  Recognized end-to-end (lex → parse → IR → codegen).
- **ERC-721 auto-synthesis** for `nft` constructs (Sprint 35.b).
  515-line synthesizer in `crates/covenant-stdlib/src/erc721.rs`
  emitting 11 functions, 3 events, 4 errors. A bare 4-line
  `nft Foo { name symbol base_uri }` produces a 1233-byte deploy
  contract.
- **ERC-8231 auto-synthesis** for `registry` constructs (Sprint 35.b).
  340-line synthesizer in `crates/covenant-stdlib/src/erc8231.rs`
  emitting the post-quantum key registry surface (5 functions,
  3 events, 2 errors). `algorithm_id()` returns 1 (Dilithium-5
  per FIPS 204).
- **`StdlibConfig::synthesize_erc721` and `synthesize_erc8231`** flags
  (default `true`).

#### Deferred

- `safeTransferFrom` (NFT receiver-hook callback) → Sprint 35.c
- `update_key(new_pk, sig)` PQ-signed key rotation (Registry) → Sprint 35.c

### Branch / repo cleanup

13 historical V0.7 / V0.8 / release / audit branches across covenant-src
and covenant-playground deleted (local + remote) after consolidating
their commits to `main` and verifying no lost work. Repos now have
exactly `main` + at most one in-flight feature branch.

### V0.9 Phase B (Diagnostics + Lint + LSP + Test) — Sprints 36-40

#### Added

- **Sprint 38** (Diagnostics) — `covenant-diag/src/explanations.rs` :
  long-form prose registry for diagnostic codes. `covenant explain
  <code>` CLI subcommand prints body + summary. 8 codes seeded
  (E003, E020, E421, E503, E510, E601, W606, E7000). Append-only
  registry — adding new codes is safe ; LSP can attach codeDescription.href
  pointing at future docs URLs.
- **Sprint 38** (LSP go-to-definition) — `covenant-lsp/src/analysis.rs` :
  `find_definition_at()`, `decl_definition_for_name()`,
  `DefinitionTarget`. Backend exposes `definition_provider:
  Some(OneOf::Left(true))`. Editors can now jump from a reference to
  its declaration site.
- **Sprint 39** (Lint) — `covenant-lint/src/source_scan.rs` (~280
  lines) source-text anti-pattern scanner. 6 rules : L001 `//` C-style
  comment, L002 `mapping(`, L003 `function`, L004 `require(`, L005
  `uint256`, L006 `string`. Word-boundary aware, skips string literals.
  Runs even when the file fails to parse (catches syntax issues
  early).
- **Sprint 39** (Lint config) — `.covenantlint.json` config schema +
  loader. `LintConfig { rules, exclude }` with per-rule `RuleSeverity`.
  `find_and_load()` walks ancestor dirs.
- **Sprint 40** (Test isolation) — `covenant test` now creates a
  fresh `CovenantTestHarness` per `test_*` action. Per-test fresh
  world. Cost ≈ 50 ms per test ; worth the determinism guarantee.
  Empirically validated by `examples/test_isolation_demo.cov` (3-test
  fixture proving state from one test does NOT leak into the next).
- **Sprint 40** (CLI fmt) — `covenant fmt --check` confirmed and
  documented. Reference doc `docs/v0.9/cli-test-and-fmt.md`.

### V0.9 Phase C (Playground sync + Aster + CLI ergonomics) — Sprints 36b-42

#### Added (covenant-playground repo)

- **Sprint 36b** (Cross-tab sync) — `src/lib/cross-tab.ts` (~190 lines)
  CrossTabSync class wrapping BroadcastChannel. tabId UUID +
  heartbeat + bye on `beforeunload`. Header chip indicator showing
  N other tabs.
- **Sprint 37** (Persistence) — `src/lib/persistence.ts` (~190 lines)
  raw IndexedDB wrapper. Schema v1, 3 stores
  (contracts/transactions/preferences). Per-target keying. bigint →
  string roundtrip.
- **Sprint 37** (Event decoding) — `src/lib/event-decoder.ts` :
  `decodeLogs(iface, logs)` decodes raw Sepolia logs to MockEvent
  format. Defensive fallback on parseLog throw.
- Wired CrossTabSync + persistence + event-decoder into the Zustand
  store. State-mutating Sepolia call path now decodes logs end-to-end.

#### Added (compiler repo)

- **Sprint 41** (CLI doctor) — `crates/covenant-cli/src/commands/doctor.rs`
  ~280 lines. 9 probes : covenant version, rustc, cargo, forge, cast,
  SEPOLIA_RPC_URL, ETHERSCAN_API_KEY, ASTER_RPC_URL,
  config/helper-addresses-v0.9.0.json, helpers/foundry.toml. Human ✓/⚠/✗
  output or `--json` for tooling. `probe_env_var(var, label, hint)`
  per-probe message customization. Doc :
  `docs/v0.9/cli-doctor-and-coverage.md`.
- **Sprint 41** (CLI test --coverage) — name-heuristic action coverage.
  For every non-test action declared in the contract, check whether
  at least one `test_*` action's name contains it. Reports
  covered/total + lists uncovered actions. Heuristic is documented
  honest ; IR-instrumented ground-truth deferred to V0.9.x.
- **Sprint 42** (Aster Chain placeholder) — codegen ready
  (`Target::AsterTestnet`, chain_id 1996, helpers CREATE2-deterministic).
  Doctor probes ASTER_RPC_URL. Status doc
  `docs/v0.9/aster-chain-integration-status.md` enumerating 4
  blockers (Arachnid factory verify on Aster, ASTER gas, Foundry
  script targeting, explorer verification) + the deploy procedure
  when unblocked. Sprint 43 (M2 milestone) gated on operational
  Aster access ; deferred to V0.9.x.

### V0.9 Phase D (Audit prep) — Sprints 44-46

#### Added (audit deliverables)

- **Sprint 44** (Audit prep package) :
  - `SECURITY.md` (top-level) — vulnerability disclosure policy,
    `security@kairos-lab.org`, response timeline (48h ack / 30d
    High-Critical fix), severity scale.
  - `docs/v0.9/audit-scope-v0.9.0.md` — in-scope vs out-of-scope
    crate inventory, P0/P1/P2 audit priorities, audit reproduction
    baseline (build + test + clippy + audit + forge + fixture pack +
    doctor), Sprint 31.b empirical bug log.
  - `docs/v0.9/threat-model-v0.9.0.md` — STRIDE-aligned catalog
    (16+ entries S/T/R/I/D/E), adversary model, cross-cutting
    controls, open items.
  - `docs/v0.9/known-acceptable-risks.md` — formal ledger :
    RUSTSEC-2024-0421 (idna transitive), Mocked* on mainnet
    (4-layer defense), fmt comment-loss, LSP no-debounce, no fuzz,
    no SBOM. Each entry has finding + why-acceptable + remediation
    + verification command.
- **Sprint 45** (Audit fixture pack) :
  - `examples/audit/` — 9 curated fixtures (token, ceremony, ballot,
    nft, registry + auth/reverts/views/E601-doc).
  - `examples/audit/README.md` — pack guide.
  - `docs/v0.9/helper-deep-dive-ceremony.md` — CeremonyHelper
    audit reference (state machine diagram, per-method checklists,
    cross-cutting properties, Sprint 31.b empirical bug log).
- **Sprint 46** (OMEGA V5 audit gate) :
  - `OMEGA_V5_AUDIT_REPORT.md` — self-audit synthesis. 10
    findings/verifications, repository metrics, risk register,
    sign-off conditions, recommendations for V0.9.x and V1.0.
  - `docs/v0.9/audit-gate-decision.md` — formal go/no-go decision
    doc. **Verdict : GO for V0.9.0 tag.**

#### Fixed

- **Sprint 44** — 7 clippy warnings across 3 crates (covenant-diag
  needless reference, covenant-cli build doc indentation +
  explain.rs print literals, covenant-wasm-bindings strip prefix).
  Workspace now passes `cargo clippy --workspace --all-targets -- -D warnings`.
- **Sprint 45** — `covenant lint` ICE'd on every file. Root cause :
  `LintArgs --color: String` clap-arg collision with global `Cli
  --color: ColorMode`. Fix : remove local `--color`, thread
  `use_color` from main.rs dispatch. Caught empirically while
  smoke-testing the audit fixture pack (same pattern as the 4
  Sprint 31.b bugs caught during M1 deploy). Improved panic hook
  in `main.rs` : with `RUST_BACKTRACE` set, surfaces actual panic
  payload + location (was previously showing only the hook closure
  site).

#### Validated (gate criteria, all met)

  - 1172 / 1172 Rust workspace tests passing
  - 34 / 34 Foundry helper tests passing
  - `cargo clippy -D warnings` clean
  - `cargo audit` baseline matches accepted residuals (1 known)
  - 9 / 9 audit fixtures compile end-to-end
  - M0 + M1 Sepolia milestones publicly verifiable

---

## [0.7.0] — 2026-04-22

### Added

- **Aster Chain backend** (`covenant-aster-backend`): New crate providing V0.7 foundation-mode compilation for [Aster Chain](https://asterdex.com) (chain ID 1996, 50 ms PoSA L1).
  - `compile_module()` pipeline: lowering → type validation → precompile resolution → bytecode emission
  - Precompile address registry: FHE (`0x0300`–`0x0308`), PQ (`0x0400`–`0x0401`), ZK (`0x0500`–`0x0501`)
  - Placeholder bytecode with `COV7\x01` magic + function count; `aster-sdk-pending` warning emitted
  - Full SDK lowering deferred to V0.8 pending Aster SDK GA

- **CLI `--target-chain aster`**: `covenant build path/to/contract.cov --target-chain aster` compiles to Aster and writes `.aster` + `.aster.json` artifacts.

- **LSP lint diagnostics**: `covenant-lsp` now runs `covenant-lint` after a clean frontend pass and surfaces security findings (C100, C200, etc.) as LSP diagnostics with `source: "covenant-lint"`.

- **VS Code platform extensions**: Multi-platform VSIXes for win32-x64, linux-x64, linux-arm64, darwin-arm64, darwin-x64. Each bundles the native `covenant-lsp` binary for zero-config setup.

- **`docs/aster-integration.md`**: Integration guide covering usage, artifact format, precompile addresses, and V0.8 roadmap.

### Fixed

- Clippy `-D warnings` fixed across all 19 crates: `is_some_and`, `is_none_or`, `is_multiple_of(2)`, `checked_div`, `checked_rem`, `.ok()` pattern.

### Documentation

- **LICENSE_CLARIFICATION.md** — new document establishing Covenant's scheme-agnostic architecture and IP position. Key points:
  - Covenant does not depend on, include, or derive from any Zama code (verified: zero `tfhe`/`zama` deps across 19 crates)
  - References the academic TFHE scheme (ASIACRYPT 2016), not Zama's commercial variants
  - Standing policy: never adopt Zama-licensed code
  - Users deploying commercially must verify their target chain's FHE implementation licensing
  - Verification commands included for full transparency

- **README.md** — new "License & FHE Technology Note" section with quick-access summary and link to the full clarification document.

### Breaking changes from 0.6.x

- None. The `--target-chain aster` flag is additive. EVM default behaviour is unchanged.

---

## [0.6.1] — 2026-04-22

### Fixed

- **covenant-cli**: Remove unnecessary `unsafe` block around safe `libc_isatty` wrapper in `color.rs`. Clippy `-D warnings` now passes cleanly across the full workspace. Commit `039e02d`.

### Added

- **VS Code extension** (`editors/vscode/`): Syntax highlighting (TextMate grammar covering all Covenant keywords, types, privacy qualifiers, annotations, operators, and `--` comments), LSP client via `vscode-languageclient` v9, and language configuration (bracket matching, folding, comment toggle). Package with `cd editors/vscode && npx vsce package`. Commit `68a7cf0`.

### CI

- All 4 CI jobs (check, fmt, clippy, test) now green on Rust 1.81.

---

## [0.6.0] — 2026-04-24 (General Availability)

First general availability release of Covenant. Post-quantum + FHE + ZK + Cryptographic Amnesia primitives for privacy-preserving smart contracts.

### Audit

Covenant V0.6 underwent a full OMEGA V4 audit by Kairos Lab Security Research. **41 findings identified across 5 phases; all resolved.** Audit reports and reproducible PoCs will be published at `github.com/Valisthea/covenant-audits` (coming soon).

### Critical fixes (Session 1)

- **KSR-CVN-011** — `IrGuard::Only` never lowered: `only`/`when`/`given` clauses were fictional — access control was a no-op. Fix: `emit_only_assert` in `builder.rs` now emits `LoadCaller → LoadDeployer → Eq → Assert`; EVM lowers `Assert` to `ISZERO + JUMPI → __revert__`. Commit `bf89e8c`.
- **KSR-CVN-014** — Stale-memory forges verification: absent precompile → `MLOAD 0x00` read as "verified". Fix: `RETURNDATASIZE == 32` check added to every precompile call. Commit `d5cbcd3`.
- **KSR-CVN-013** — STATICCALL success flag discarded: every precompile call popped success silently. Fix: `ISZERO + JUMPI` after every STATICCALL. Commit `d5cbcd3`.
- **KSR-CVN-012** — Proxy initializer hijack: `action initialize(...)` had no re-init guard. Fix: `emit_initializer_guard` wraps every `initialize` with `keccak256("covenant.proxy.initializer.<Module>")` SLOAD/SSTORE. Commit `f285368`.
- **KSR-CVN-001** — ERC-8228 ceremony phase transitions unchecked. Fix: `setup`/`finalize`/`destroy` synthesised guards. Commit `a2207b4`.

### High fixes (Session 2)

- **KSR-CVN-015** — `FheCmpNe`/`FheCmpLe` aliased `cmp_eq`/`cmp_lt` precompile addresses. Commit `d378297`.
- **KSR-CVN-016 + 017** — CSE collapsed `FheEncryptFresh` and `PqRand` (randomness collapsed to one call). Commit `f3a5277`.
- **KSR-CVN-018** — Reentrancy lints didn't cover `ExternalCall` opcode. Commit `d88910c`.
- **KSR-CVN-019 + 020** — No `EXTCODESIZE` canary + no 4-byte selector prefix on precompile calldata. Commit `5ee93a8`.
- **KSR-CVN-021** — `@slot(N)` annotation silently ignored. Commit `0eb1ad8`.
- **KSR-CVN-022 + 023** — Shamir and VDF opcodes compiled to broken stubs; now hard-fail with `E516`/`E517`. Commit `fc4e7f6`.
- **KSR-CVN-024 + 025** — P4 nonce / P5 monotonicity invariants had no detectors. Commit `3fc6c47`.
- **KSR-CVN-026** — DCE eliminated `FheBootstrap`. Fixed in `e17e1ad`.
- **KSR-CVN-027** — External `CALL`/`STATICCALL` success flag discarded. Commit `5eb1c38`.

### Medium / Low / Info fixes (Sessions 3 + 4)

- Layout-diff CLI (`covenant diff-layout`), precompile ABI version marker in bytecode, annotation validation, Cargo.lock pinning, CSE FHE arithmetic exclusion, `fhe_fold` type-aware key, ceremony seed entropy, version binding, `@slot` collision detection wired (`E423`), `PRECOMPILE_ABI_VERSION` consolidated. Full list in `covenant-audits`.

### Language features

- Post-quantum signatures (Dilithium-5 via precompile)
- FHE operations (TFHE: encrypt, add, mul, cmp, branch)
- ZK proof verification (Nova IVC, Halo2 via precompile)
- Cryptographic Amnesia (Shamir SSS ceremony + Wesolowski VDF + destruction proofs — hard-fail until V0.7 chain support)
- ERC-8227 Encrypted Token Standard
- ERC-8228 Cryptographic Amnesia Interface
- Proxy patterns (`@proxy_compatible`, UUPS / Transparent / Beacon) with initializer guards
- External contract calls with type safety and reentrancy detection
- Storage layout protection (`@slot(N)`, `covenant diff-layout`, `E423` collision detection)
- Privacy analysis pass (P1–P5 invariants)

### Tooling

- `covenant compile` / `inspect ir` / `inspect bytecode`
- `covenant lint` with 38+ detectors across 13 categories
- `covenant diff-layout` for upgrade safety
- LSP for VS Code integration

### Breaking changes from 0.5.x

- Precompile calldata now carries a 4-byte ABI selector prefix. Precompile implementations must be upgraded to match.
- `@slot(N)` annotation is now actually honored (was silently ignored). Existing contracts using it will observe different storage layout.
- Unknown annotation names now produce `W430` warnings. Misspelled security annotations (e.g. `@nonreentrant` vs `@non_reentrant`) are now surfaced.
- `@slot(N)` conflicts now produce `E423` errors.

### Known limitations

- Shamir / VDF opcodes hard-fail compilation (`E516`/`E517`). Contracts using `shamir_split`, `vdf_eval`, or `@vdf_locked` cannot be compiled until V0.7 chain support is available. This is intentional.
- Aster Chain adapter: pending SDK GA.

### Audit artifacts

- Full audit reports: `github.com/Valisthea/covenant-audits` (coming soon)
- Executive summary: covenant-audits/audits/2026-04-22-omega-v4-covenant-v0.6/00-executive-summary.md
- 7 reproducible PoCs: covenant-audits/.../03-adversarial-testing/
- Phase 5 re-audit: covenant-audits/.../phase5-report.md

---

## [0.6.1-rc4] — 2026-04-22 (Session 4: Pre-GA cleanup)

OMEGA V4 Session 4 — closes KSR-CVN-040 and KSR-CVN-041 (two Phase 5 Info findings) and resolves three documentation drifts found in Phase 5 Pass D.

### Fixed

- **KSR-CVN-040** (`crates/covenant-evm-backend/src/artifact.rs`): `PRECOMPILE_ABI_VERSION` was independently defined in both `abi.rs` and `artifact.rs`. `artifact.rs` now re-exports from `abi.rs` — single source of truth; a maintainer bumping one can no longer silently diverge the other.
- **KSR-CVN-041** (`crates/covenant-evm-backend/src/lib.rs`): `detect_slot_collisions` was fully implemented in `storage.rs` but never called. `codegen_evm` now invokes it before returning; conflicting `@slot(N)` pairs produce `E423_SLOT_ANNOTATION_CONFLICT` diagnostics that callers can treat as compile errors.

### Added

- `crates/covenant-evm-backend/tests/abi_version_coherence.rs` — 2 tests guarding KSR-CVN-040.
- `crates/covenant-evm-backend/tests/storage_layout.rs` — 3 tests guarding KSR-CVN-041 (duplicate explicit slots, distinct explicit slots, sequential-slot alias).

### Changed

- `Cargo.toml` workspace version `0.6.0-rc3` → `0.6.1-rc4` (aligns with git tag).
- `README.md` audit banner updated to reflect OMEGA V4 completion and audit report location.

### Total tests: 952 (was 947 in rc3)

---

## [0.6.1-rc3] — 2026-04-22 (Session 3: Medium / Low / Info remediation)

OMEGA V4 Session 3 — 22 Medium / Low / Info findings closed (16 remediated, 5 N/A with rationale, 1 restructured). Tag `v0.6.1-rc3` at commit `9a39a70`.

### Fixed

- **KSR-CVN-028** — Missing `@slot` annotations on synthesised ERC-20 fields (beacon-slot mis-target).
- **KSR-CVN-029** — No precompile ABI version marker in deploy bytecode; constructor now PUSHes `PRECOMPILE_ABI_VERSION`.
- **KSR-CVN-030** — Unknown annotation silently ignored; compiler now emits `W430_UNKNOWN_ANNOTATION` and maintains an allow-list.
- **KSR-CVN-031** — `Cargo.lock` not committed; added to repository and `.gitignore` allowlisted.
- **KSR-CVN-032** — CSE was eligible to merge FHE arithmetic ops with identical operands; ops added to CSE blacklist.
- **KSR-CVN-033** — `fhe_fold` key derived without regard to element type; key now encodes the element type tag.
- **KSR-CVN-034** — Amnesia ceremony seed was constant zero; seed is now `keccak256(module_name ‖ deploy_block)`.
- **KSR-CVN-035** — No version binding between compiled contract and the chain's precompile set; `PRECOMPILE_ABI_VERSION` marker added.
- **KSR-CVN-036** — DCE terminator match non-exhaustive (`_ => {}` wildcard); match is now exhaustive over `Return | Revert | Unreachable`.
- **KSR-CVN-038** — No layout-diff CLI to detect storage layout regressions across upgrades; `covenant diff-layout` subcommand added.

### N/A (code evolved before remediation)

- KSR-CVN-037 (no timelock on upgrade), KSR-CVN-039 (I804 fires on synth_finalize), MED-028, MED-031, LOW-035 — proxy/upgrade layer removed; INF-039 lint removed.

### Total tests: 947 (was ~946 in rc2)

---

## [0.6.1-rc2] — 2026-04-22 (Session 2: High findings remediation)

OMEGA V4 Session 2 — 12 High findings closed. Tag `v0.6.1-rc2` at commit `3fc6c47`.

### Fixed

- **KSR-CVN-015** — `FheCmpNe` and `FheCmpLe` precompile addresses aliased `cmp_eq`/`cmp_lt`; corrected to `0x0113` / `0x0114`.
- **KSR-CVN-016** — CSE collapsed multiple `FheEncryptFresh` calls into one (randomness collapse); `has_randomness_or_state()` flag added; CSE gate respects it.
- **KSR-CVN-017** — CSE collapsed `PqRand`; same fix as CVN-016.
- **KSR-CVN-018** — Reentrancy detector scope too narrow (missed `ExternalCall` opcode); `is_side_effecting` updated.
- **KSR-CVN-019** — No `EXTCODESIZE` canary at deploy time; `emit_precompile_extcodesize_canary` added.
- **KSR-CVN-020** — Precompile calldata lacked a 4-byte ABI selector prefix; `precompile_selector()` added and used in every `emit_precompile_call`.
- **KSR-CVN-021** — `@slot(N)` annotation silently ignored; `slot_for_global` now honours `explicit_slot`.
- **KSR-CVN-022** — `ShamirSplit` opcode compiled to REVERT stub instead of being rejected at compile time; now emits `E516` error.
- **KSR-CVN-023** — `@vdf_locked` qualifier not lowered; now emits `E517` error.
- **KSR-CVN-024** — P4 invariant (no FHE in constructor) had no detector; lint added.
- **KSR-CVN-025** — P5 invariant (no `AmnesiaBegin` outside a ceremony context) had no detector; lint added.
- **KSR-CVN-026** — DCE eliminated `FheBootstrap`; added to `is_preserve_op` list.
- **KSR-CVN-027** — External CALL / STATICCALL success flag discarded; `emit_external_call` now has ISZERO+JUMPI after every CALL.

### Total tests: ~946 (was 898 in rc1)

---

## [0.6.1-rc1] — 2026-04-22 (Session 1: Critical findings remediation)

OMEGA V4 Session 1 — 5 Critical findings closed. Tag `v0.6.1-rc1` at commit `a2207b4`.

### Fixed

- **KSR-CVN-001** — ERC-8228 ceremony phase transitions unchecked; `setup` / `finalize` / `destroy` synthesised guards added (`emit_assert_caller_is_deployer`, `emit_assert_phase_eq`). Commit `a2207b4`.
- **KSR-CVN-011** — `IrGuard::Only` never lowered to EVM; `emit_only_assert` in `builder.rs` now emits `LoadCaller → LoadDeployer → Eq → Assert`; EVM backend lowers `Assert` to `ISZERO + JUMPI → __revert__`. Commit `bf89e8c`.
- **KSR-CVN-012** — Proxy `action initialize(...)` had no re-init guard; `emit_initializer_guard` now wraps every `initialize` action with `keccak256("covenant.proxy.initializer.<Module>")` SLOAD/SSTORE pattern. Commit `f285368`.
- **KSR-CVN-013** — Every precompile STATICCALL popped the success flag; `emit_precompile_call` now has `ISZERO + JUMPI` after every STATICCALL. Commit `d5cbcd3`.
- **KSR-CVN-014** — Stale memory read as "verified" when precompile absent; `emit_precompile_call` now checks `RETURNDATASIZE == 32` before reading result. Commit `d5cbcd3`.

### Total tests: 898 (was 870 in v0.5.0-cli-s3)

---

## [v0.5.0-cli-s3] — 2026-04-21 (Phase 17: Solidity Interop)

Phase 17: Solidity Interop — `external contract` declarations, `IContract.at(addr).method(args)` invocation, `@non_reentrant` auto-injection, stdlib ABI helpers, and 61 new tests (870 total).

### Added

- **Lexer** (`covenant-lexer/src/token.rs`): 3 new keyword tokens — `KwExternal`, `KwContract`, `KwFunction`.
- **AST** (`covenant-parser/src/ast.rs`): `ExternalContractDecl`, `ExternalFunctionDecl`, `ExternalParam` structs; `external_contracts: Vec<ExternalContractDecl>` field on `File`.
- **Parser** (`covenant-parser/src/parse_file.rs`): parses `external contract Name { function ... }` blocks before the main construct. `parse_external_contract()` and `parse_external_function()` methods. Keywords allowed as external function names and parameter names via `expect_ident_or_any_keyword()`.
- **Parser fix** (`parse_expr.rs`): field access after `.` now accepts any keyword token (e.g. `.transfer`, `.approve`) via `expect_ident_or_any_keyword`.
- **Parser util** (`parser.rs`): `expect_ident_or_any_keyword()` method; `any_keyword_as_ident_text()` exhaustive keyword→text mapping.
- **Printer** (`covenant-parser/src/printer.rs`): `print_external_contract()` and `print_external_function()` for round-trip output.
- **IR opcode** (`covenant-ir/src/opcode.rs`): `ExternalCall { abi_sig: Box<str>, is_view: bool, arg_count: u32 }` variant with `operand_count() = 1 + arg_count`.
- **IR module** (`covenant-ir/src/module.rs`): `IrExternalContract`, `IrExternalFunc` structs; `external_contracts` field on `IrModule`.
- **IR builder** (`covenant-ir/src/builder.rs`): `lower_external_contracts()` builds ABI sig lookup table; `lower_call()` detects `IName.at(addr).method(args)` pattern and emits `ExternalCall`. `ast_type_to_abi_str()` maps Covenant types to Solidity ABI strings.
- **EVM backend** (`covenant-evm-backend/src/`):
  - `storage::REENTRANT_LOCK_SLOT = 0xFFFF_FFFFu32`.
  - `codegen::emit_external_call()`: emits CALL (non-view) or STATICCALL (view) with correct calldata and stack discipline.
  - `codegen::emit_reentrant_lock_acquire/release()`: SLOAD check + SSTORE acquire/release using lock slot.
  - `abi::selector()`: 4-byte keccak selector from ABI signature string.
- **DCE optimizer** (`covenant-opt/src/dce.rs`): `ExternalCall` added to `is_side_effecting()` — prevents optimizer from eliminating void external calls.
- **25 parser tests** (`covenant-parser/tests/external_contract.rs`).
- **16 IR tests** (`covenant-ir/tests/external_contract_ir.rs`).
- **19 EVM codegen tests** (`covenant-evm-backend/tests/external_call_codegen.rs`).

### Changed

- `IrModule` struct literals in stdlib crates updated with `external_contracts: vec![]`.
- `empty_module()` in `covenant-ir/tests/errors.rs` updated with `external_contracts: vec![]`.

### Total tests: 870 (was 809 in Phase 16)

---

## [v0.5.0-cli-s2] — 2026-04-21 (Sessions 2+3)

Phase 14: CLI UX — V0.5 Sessions 2+3. Implements `covenant test`, `covenant inspect`, `covenant fmt`, and `covenant completions` subcommands as production-ready implementations. Fixes AST pretty-printer idempotency for ballot constructs.

### Added

- **`covenant test`** (`commands/test.rs`): compiles .cov source, discovers test actions (names starting with `test_` or annotated `@test`, zero args), deploys via `CovenantTestHarness`, calls each test action. Flags: `--filter`, `--no-fail-fast`, `--list`, `--gas-report`. Uses `covenant-testing` crate.
- **`covenant inspect`** (`commands/inspect.rs`): dumps AST, IR, bytecode, ABI, storage layout, or diagnostics for a .cov file. Flags: `--item` for construct filtering, verbosity via `-v/-vv`. IR pretty-printer walks `IrModule`, `IrFunction`, `IrBlock`, terminators. Uses `covenant-ir` crate.
- **`covenant fmt`** (`commands/fmt.rs`): canonical source formatter via parse→AST→pretty-print. Flags: `--check`, `--diff`, `--stdin`. Idempotent (re-formatting already-formatted output is a no-op). Lenient: formats syntactically parseable files regardless of semantic diagnostics.
- **`covenant completions`** (`commands/completions.rs`): generates shell completion scripts for bash, zsh, fish, powershell via `clap_complete 4.4`.
- **AST pretty-printer** (`covenant-parser/src/printer.rs`, NEW): `pub fn pretty_print(file: &File) -> String`. Canonical output for all Covenant construct types. Free functions: `expr_str`, `type_str`. Idempotency verified against 6 fixtures including ballot.
- **Printer idempotency fixes**:
  - `Type::Choice(_, vec![])` now prints as `choice` (not `choice<>`)
  - `Type::List(Type::Choice(...), _)` prints as `[choice]` (bracket syntax)
  - Multiple guards now comma-separated: `when ... , only ... , given ...`
  - `Type::List(inner)` uses `list<T>` for non-choice inner types (idiomatic)
- **10 new printer tests**: type_str_choice, type_str_list_of_choice, idempotent_ballot_construct, idempotent_multiple_guards, idempotent_action_three_guards, guard_str_when, guard_str_only_caller, idempotent_reveal_with_guard_and_body.
- **New dependencies**: `covenant-testing`, `covenant-ir`, `clap_complete` in `covenant-cli/Cargo.toml`.

### Changed

- `covenant test`/`fmt`/`inspect` stubs replaced by production implementations; integration tests updated accordingly.
- Integration tests: 3 stub tests removed, replaced with 14 real behavior tests for test/fmt/inspect/completions subcommands.
- `format_source` is now lenient — formats any syntactically parseable file regardless of semantic diagnostic errors.

### Total tests: 810 (was 743 in V0.5 Session 1)

---

## [v0.6.0-lsp] — 2026-04-21 (Session 1)

Phase 15: LSP Implementation — V0.6. Implements `covenant-lsp`: a tower-lsp based language server providing diagnostics, hover, and document symbols for Covenant source files.

### Added

- **`covenant-lsp` crate** (`crates/covenant-lsp/`): language server binary (`covenant-lsp`) + library (`covenant_lsp`). Depends on `tower-lsp 0.20`, `tokio 1`.
- **LSP server** (`src/main.rs`): reads from stdin / writes to stdout using tower-lsp's `Server::new(stdin, stdout, socket).serve(service).await` pattern. Starts with `covenant-lsp`.
- **`Backend`** (`src/backend.rs`): implements `tower_lsp::LanguageServer`:
  - `initialize` → reports `textDocumentSync: FULL`, `hoverProvider`, `documentSymbolProvider` capabilities.
  - `initialized` → logs startup message to client.
  - `did_open` / `did_change` / `did_save` → stores full-text snapshot, runs frontend analysis, publishes `textDocument/publishDiagnostics`.
  - `did_close` → evicts snapshot, clears diagnostics.
  - `hover` → parses document, finds innermost entity at cursor, returns markdown description.
  - `document_symbol` → parses document, emits nested `DocumentSymbol` tree (construct → fields/actions/views/events/errors).
- **`analysis` module** (`src/analysis.rs`): pure (sync, no runtime) helpers:
  - `byte_offset_to_position` / `position_to_byte_offset` — byte ↔ LSP `Position` conversion.
  - `diag_to_lsp` — maps compiler `Diagnostic` (byte spans) to `lsp_types::Diagnostic` (line/col).
  - `analyze(source)` — runs `covenant_driver::check()` and converts all diagnostics.
  - `parse_source(source)` — runs lex + parse, returns `Option<ast::File>`.
  - `type_to_string(ty)` — formats `ast::Type` for hover / symbol detail.
  - `collect_symbols(file, source)` — extracts nested `DocumentSymbol` list.
  - `find_hover_at(file, source, offset)` — walks AST to find entity at byte offset.
- **Dependency pinning**: `url` pinned to `=2.4.1` in `Cargo.lock` via `cargo update url --precise 2.4.1` to prevent `idna 1.x` → ICU crates that require Rust ≥ 1.82.
- **14 tests**: 5 unit tests (`src/analysis.rs` `#[cfg(test)]`) + 9 integration tests (`tests/lsp_analysis_tests.rs`): position conversion, diagnostics on clean/erroneous source, document symbol structure for `Hello` and `Coin`, hover over field and action names.

### Total tests: 668 (was 654 in V0.5-cli)

### Known limitations / Session 2+

- Go-to-definition: not implemented (requires resolver-level symbol table export)
- Completion: not implemented (requires keyword list + scope-aware symbol completion)
- Rename / find-references: not implemented
- Incremental analysis: each `didChange` re-runs the full frontend pipeline; no caching
- VS Code extension: not implemented this session (binary exists; extension scaffold deferred)
- Hover inside statement bodies (non-top-level positions) returns `None`

---

## [v0.5.0-cli] — 2026-04-21 (Session 1)

Phase 14: CLI UX — V0.5. Implements Doc 9 (CLI UX Specification) Session 1: subcommand architecture, manifest support, `init`, `check`, `clean`, ariadne diagnostics, exit codes, JSON output.

### Added

- **Subcommand architecture** (`covenant-cli/src/`): clap-based multi-command dispatch replacing the single `build` command. Module structure: `commands/`, `error.rs`, `color.rs`, `diagnostics.rs`, `output.rs`.
- **`covenant init`** with 5 templates: `basics`, `token`, `ballot`, `ceremony`, `module`. Supports `--name`, `--template`, `--force`, `--no-git` flags. Templates embedded as string constants (no external files needed).
- **`covenant check`** (frontend-only): runs lex → parse → resolve → typecheck → privacy without codegen. New `check()` function added to `covenant-driver`. Exits 0 if no errors, 1 if errors found.
- **`covenant clean`**: removes project build output directory. Reads `Covenant.toml` for output path. Supports `--dry-run`.
- **`covenant build` refactored**: now supports project mode (reads `Covenant.toml`) and single-file mode (`covenant build <file.cov> --out <dir>`). Both modes preserved.
- **Ariadne diagnostic rendering** (`diagnostics.rs`): `covenant check` now renders errors with source context (file path, line/column, underlined span, help text) via `ariadne 0.4.0`.
- **Exit codes per Doc 9 §3.4**: 0 (ok), 1 (compile error), 2 (usage), 3 (I/O / not found), 4 (ICE). All subcommands consistent.
- **JSON output format** (`--format=json`): `build` and `check` emit one JSON line per construct/file. Diagnostic JSON with spans, level, code, message.
- **Color control** (`--color auto|always|never`, `NO_COLOR` env var): propagated to ariadne renderer.
- **Manifest parsing** (`covenant-manifest/src/lib.rs`): full `Covenant.toml` schema (project, source, build, test, compiler sections). Upward search via `Manifest::find_upward()`. Compiler version constraint via `semver`. Uses `toml 0.5.11` (compatible with Rust 1.81).
- **5 CLI tests files**: 19 unit tests (build/check/clean/init/diagnostics helpers) + 27 integration tests + 2 preserved legacy tests = **48 CLI tests**.
- **Stubs** for Session 2/3 subcommands: `test`, `fmt`, `inspect` all compile and exit 2 with "not yet implemented" message.
- **Global flags**: `--verbose` / `-v`, `--quiet` / `-q`, `--format`, `--color`, `--manifest`.

### Fixed

- **`clap = "=4.4"` maintained**: clap 4.5+ transitively requires `edition2024` features via `clap_lex 1.x` and `indexmap 2.14`, incompatible with Rust 1.81. Bumping is deferred until toolchain upgrade.

### Total tests: 654 (was 604 in V0.4-edge)

### Known limitations / Session 2+

- `covenant test`: not implemented (Phase 14 Session 2 — requires Phase 13 test construct support)
- `covenant fmt`: not implemented (Phase 14 Session 3 — requires AST-preserving formatter)
- `covenant inspect`: not implemented (Phase 14 Session 2 — requires artifact dump infrastructure)
- Shell completions: Phase 14 Session 3
- Verbosity levels `-v`/`-vv`/`-vvv` parsed but not yet used to vary log output

---

## [v0.4.0-edge] — 2026-04-20 (Session 1)

Phase 13: Edge Cases Codex. Adds `hybrid module` construct with per-field privacy qualifiers and typed error declarations with ABI-encoded `revert_with`.

### Added

- **HybridState (Example 10)** (`example_10_hybrid_state.cov`): `hybrid module` construct with per-field privacy qualifiers. All three fields are plain (no per-field qualifier) under `hybrid`, defaulting to public. 8 integration tests, all passing.
- **SafeTransfer (Example 11)** (`example_11_safe_transfer.cov`): typed error declarations (`error InsufficientBalance(required: amount, actual: amount)`, `error Unauthorized(caller: address)`, `error ZeroAmount()`) and `revert_with ErrorName(args)` statements. 8 integration tests, all passing.
- **ABI-encoded typed reverts** (`covenant-evm-backend/src/codegen.rs`): `Terminator::Revert { error, args }` now emits a proper EVM revert payload: 4-byte selector (`keccak256(sig)[0..4]`) followed by ABI-encoded args (32 bytes each). Selector placed via `SHL(224, sel_4bytes); MSTORE(0)`; args placed at offsets `4 + i*32`.

### Fixed

- **Resolver LangIdent shadowing** (`covenant-resolver/src/resolver.rs`): `register_field` now allows user-declared fields to shadow pre-seeded `LangIdent` bindings (e.g. `field owner: address` no longer raises E101 "duplicate declaration `owner`"). Previously, `seed_construct_lang_idents` ran before Pass 1, seeding `owner` → `LangIdent::Owner`; any explicit `field owner` would collide. Fix: if the existing binding is `Binding::LangIdent(_)`, overwrite it rather than emitting E101.

### Total tests: 604 (was 588 in V0.3-advanced)

### Known limitations carried forward

- `hybrid module` per-field `encrypted` qualifier is parsed but not yet enforced in type-checking (the `field encrypted name: type` syntax lowers to `Ty::Ciphertext` but privacy-domain analysis may not fully propagate for hybrid constructs)
- `try_action`/`catch` multi-statement blocks are not yet implemented (deferred to Phase 13 Session 2)

---

## [v0.3.0-advanced] — 2026-04-20 (Session 1)

Phase 12: Advanced Codex. Adds ERC-8228 ceremony synthesis, EncryptedBridge cross-chain escrow, and fixes map compound-assignment read-modify-write.

### Added

- **Amnesia precompile lowering** (lifts E502 for 4 opcodes): `AmnesiaBegin` (0x120), `AmnesiaSubmitShare` (0x121), `AmnesiaFinalize` (0x122), `DestructionProof` (0x123) now lower to `emit_precompile_call` in EVM backend instead of emitting E502.
- **ERC-8228 synthesizer** (`covenant-stdlib/src/erc8228.rs`): synthesizes 8 functions for `ceremony` constructs: `setup`, `submit_share`, `finalize`, `destroy`, `phase`, `session_id`, `is_destroyed`, `owner`. Emits `AmnesiaCeremonyDestroyed(uint256 indexed sessionId)` event on destruction.
- **Example 8: AmnesiaCeremony** (`example_08_amnesia_ceremony.cov`) — ERC-8228 reference ceremony contract with `on_destroy { destroy(0) }` block. 12 integration tests, all passing.
- **Example 9: EncryptedBridge** (`example_09_encrypted_bridge.cov`) — cross-chain asset escrow. `lock`, `unlock` (with `when` guard), `locked_by` view, `total_locked` view. 9 integration tests, all passing.
- **`synthesize_ceremony_queries` default `true`** in `StdlibConfig`: ceremony synthesis is now enabled by default.

### Fixed

- **Map compound-assignment read-modify-write** (`covenant-ir/src/builder.rs`): `deposits[caller] -= value` was incorrectly compiled as `deposits[caller] = value` (storing rhs directly), skipping the read of the existing map entry. Fix: for `LValue::Index` with compound operators, emit `MapGet(cur, key)` first, apply the binop, then `MapSet(cur, key, result)`. This bug was masked in `+=` tests (initial value 0 meant `0 + x = x`), but manifested in `-=` when the initial value was non-zero.

### Total tests: 588 (was 560 in V0.2-intermediate)

### Known limitations carried forward

- `ceremony` guardian/threshold parameters are parsed but not enforced at EVM level (precompile mock always accepts any share)
- `on_destroy` body is validated by privacy analyzer (E309) but body instructions after `destroy()` are not reachable (ceremony lifecycle enforces ordering at precompile level)

---

## [v0.2.0-intermediate] — 2026-04-20 (in progress)

Phase 11: Intermediate Codex. Lifts the remaining E510/E511 stubs, adds ERC-8227 synthesis, and ships SecretCoin + PrivateDAO examples.

### Added

- **Rust toolchain** : bumped 1.75 → 1.81 (clears DEBT.md #1: "Rust toolchain 1.75 pin")
- **`AssertEncrypted` lowering** (lifts E511): threshold-decrypts a `ciphertext<bool>` via precompile 0x110 and reverts if false. Enables balance-sufficient checks in ERC-8227 `transferEncrypted`.
- **`FheBranch` terminator** (lifts E510): sequential execution — jumps to then-branch, else and merge blocks follow by fallthrough. Enables `encrypted_when` constructs to compile to EVM.
- **ERC-8227 synthesizer** (`covenant-stdlib/src/erc8227.rs`): synthesizes 9 functions, 2 events, and 2 errors for `confidential token` constructs. Functions: `transferEncrypted`, `balanceOfEncrypted`, `transferFromEncrypted`, `approveEncrypted`, `allowanceEncrypted`, `totalSupply`, `decimals`, `symbol`, `name`.
- **Encrypted genesis mint** : `confidential token` with `supply: N to deployer` FHE-encrypts the genesis amount via precompile 0x101 at constructor time. Handle stored in `balances[deployer]`.
- **New precompile addresses**: 0x110 (`threshold_decrypt_bool`) and 0x111 (`threshold_decrypt_uint256`) added to config and mock.
- **Example 6: SecretCoin** — `confidential token` ERC-8227 reference contract, 16 integration tests.
- **Example 7: PrivateDAO** — `encrypted counter` governance contract with FHE vote tallies, 9 integration tests.

### Total tests: 560 (was 508 in V0.1-basics)

### Known limitations carried forward (see DEBT.md)

- `FheBranch` gas optimization deferred (current: sequential then-branch only)
- `FheCmpGe` lowered to `CmpGt` precompile — equal values fail the check; workaround: approve strictly more than transfer amount
- `revm` migration deferred (custom mini-EVM interpreter still in use)
- Real threshold decryption (validator quorum) deferred — mock returns immediately

---

## [v0.1.0-basics] — 2026-04-20

First public release. V0.1 Basics GA.

### Added

- Complete Rust compiler for Covenant V0 language subset
- 18 Rust crates : lexer, parser, resolver, types, privacy, ir, opt, stdlib, evm-backend, driver, cli, testing, diag, manifest, aster (stub), wasm (stub), circuits (stub), lsp (stub)
- 5 reference contracts compile end-to-end : Hello, Coin, OpenBallot, ShieldedCounter, QuantumBoard
- ERC-20 synthesis for `token` constructs (Phase 9) : 9 canonical functions, 2 events, 2 errors
- EVM bytecode generation (Phase 8) with MemoryMapped SSA allocator
- Privacy flow soundness checker (Phase 5) enforcing P1 property
- `covenant build <source.cov> --out <dir>` CLI producing `.bin`, `.runtime.bin`, `.abi.json`, `.storage.json`, `.metadata.json`
- Custom mini-EVM interpreter in `covenant-testing` for end-to-end test harness (509 lines, revm substitute due to MSRV pin at 1.75)
- 508+ passing tests across the workspace
- Deployment Validation V0.1 procedure documented in `deployment/REPORT.md`

### First public deployment

- **Coin (ERC-20)** on Sepolia : `0x6C7986a3d79E1AFECfE242f92f2A0DFeC3397133`
- Deploy bytecode : 1045 bytes
- Runtime bytecode : 994 bytes
- Gas used : 337,077
- Recognized by Etherscan Token Tracker
- ABI roundtripped via ethers.js
- Functional in Metamask (import via token address)

### Known placeholders (see DEBT.md)

- Dynamic-type function parameters : E516
- Strings > 64 bytes in metadata : E516
- `FheBranch` with plaintext side effects or revert : E510
- `AssertEncrypted` : E511
- Dynamic indexed event parameters : E512
- `selective_disclosure` circuit compilation : E513
- Amnesia ceremony opcodes : stubbed (panics if exercised)
- Stack allocation with live-range analysis : MemoryMapped only (gas-expensive)

### Bugs fixed during development (see LESSONS.md)

- `binop()` reverse operand order — commit `82bfdbf`
- `Ne` / `Le` / `Ge` double-MSTORE with empty stack — commit `82bfdbf`
- Calldata not copied to SSA parameter slots — commit `e361017` (Fix 1)
- Genesis mint not applied for token supply — commit `82bfdbf` (Fix 2)
- `covenant build` CLI was a stub — commit `680a88a` (Fix 3)
- String metadata not persisted to storage (symbol/name returned empty) — commit `5075a79` (Fix 4)
- Event topic using 4-byte selector instead of 32-byte keccak256 — commit `5075a79` (Fix 5)
- LOG1 used for all events regardless of indexed parameter count — commit `5075a79` (Fix 5)

### Disclaimers

- **Not audited.** Internal review planned post-V0.1 announcement. External audit planned for V0.3.
- **Experimental.** Do not deploy with production value.
- **Research release.** Intended for demonstration of language semantics, not production use.

### Contributors

- [Valisthea](https://github.com/Valisthea) — lead implementation, spec authoring, Kairos Lab

---

## [v0.0.0-scaffold] — 2026 (internal)

Phase 0 : 18-crate Cargo workspace bootstrapped. CI configured. No compilation logic.

---

## Template for future releases

```markdown
## [vX.Y.Z-name] — YYYY-MM-DD

Short descriptor of the milestone.

### Added
- ...

### Changed
- ...

### Fixed
- ...

### Removed / Deprecated
- ...

### Disclaimers (if any)
- ...

### Contributors
- ...
```

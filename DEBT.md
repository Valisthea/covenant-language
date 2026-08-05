# Covenant: Technical Debt

Explicit tracking of deferred work. Each item : what, why deferred, when it re-obligates.

Updated at every phase that knowingly creates or resolves debt.

---

## V1.0 public-launch blockers: fail-loud pass (V1.0 readiness analysis, 2026-07-05)

Surfaced by the multi-agent V1.0 readiness analysis. These are **silent
miscompiles / silent mis-dispatches**, ordinary-looking source compiles clean
and produces wrong on-chain behaviour with NO diagnostic. Same class as the
V0.9.2 E518 auth-bypass. **Must be made fail-loud (or implemented) before the
open-source public launch**, a public compiler that silently corrupts economic
or auth logic is a credibility killer. All verified against source.

- **`Math.min/max/abs/pow/sqrt` lower to `AddChecked` (return `a+b`)**,
  `crates/covenant-ir/src/builder.rs:2336-2340` (`StdlibFn::Min|Max|Abs|Pow|Sqrt
  => Opcode::AddChecked // placeholder`). `max(cap, bid)` returns `cap+bid`,
  type-checks, compiles clean. **Fix:** implement `min`/`max`/`abs` correctly
  (cheap); emit an E-code for `pow`/`sqrt` until real (loop) lowering exists.
- ~~**`div` / `mod` by zero returns `0`, not revert**~~, **RESOLVED 2026-07-23.**
  (The former citation `codegen.rs:556-557` was stale, those lines are inside
  `emit_param_prelude`; the real arms were `574-575`.) `Opcode::Div`/`Opcode::Mod`
  now lower through `binop_div_guarded`: a runtime `ISZERO` → `__revert__` guard
  when the divisor is not statically known, **E519 at compile time** when the
  divisor is a literal zero, and unchanged bytecode when it is a literal non-zero
  (so `value * bps / 10000` pays nothing for the fix). Regression tests in
  `crates/covenant-testing/tests/division_by_zero.rs`, verified to fail without
  the guard. Panic(0x12)-style revert *data* is deliberately not emitted: the
  checked-arith opcodes revert with empty data too, and parity should be one
  separate item covering all of them at once.
- ~~**Test actions ship in the deployed contract as public functions**~~,
  **RESOLVED 2026-07-23.** Found on the M6 deployment; **confirmed on-chain**. The V0.9 test pattern
  is `action test_X() when <assert> {}`, a plain `action`, so it gets a
  selector, an ABI entry and runtime code like any other. All five `test_*`
  selectors of `kairos_coin.cov` are present in the runtime bytecode at
  `0x40254d0b…65025`, and `cast call test_genesis_supply_minted()` executes
  against live state.
  **Security dimension (the real problem):** a test that *mutates* becomes a
  public, unguarded state mutator on the deployed contract. The project's own
  documented example does exactly this,
  `examples/test_isolation_demo.cov` contains `action test_mutate_n_to_5() { n = 5 }`.
  Ship that and anyone can call it and rewrite your state. We are teaching a
  pattern that is exploitable the moment it reaches a chain.
  **Fix:** a release mode that excludes test actions from codegen (a `test { }`
  block or `@test` attribute the backend drops, `#[cfg(test)]`-style), plus a
  lint that refuses to emit a release artifact containing any `test_*` action.
  Until then, tests must live in a separate file from anything deployed.
  **Resolution:** a `test_*`-named action (or one carrying `@test`) is now
  classified as `IrFunctionKind::Test` in the IR builder, and the EVM backend
  gained an `include_test_actions` flag (default **false**) threaded through the
  ABI, the selector table and the runtime dispatcher. Only the `covenant test`
  harness sets it true. Proven: `kairos_coin.test.cov` (contract + 5 tests) and
  `kairos_coin.cov` (no tests) now produce a **byte-identical** runtime
  (sha256 d37278f4…2afb), while `covenant test` still runs all five. The
  single-source-of-truth predicate is `covenant_ir::builder::is_test_action`.
  Tests: `crates/covenant-driver/tests/test_action_stripped.rs`. This makes the
  `.cov`/`.test.cov` split for the milestone token unnecessary, kept for now,
  can be re-merged. **Playground follow-up:** `covenant-wasm-bindings/adapt.rs`
  inherits the false default, so the in-browser test-isolation demo loses its
  test buttons until adapt.rs opts back in (handled in the playground pass).
- ~~**Non-zero field defaults are silently dropped**~~, **RESOLVED 2026-07-23.** Found while
  building the Robinhood milestone token; **verified on anvil AND on Robinhood
  Chain testnet (live)**. `field a: amount = 42` emits `PUSH1 0x2a` into the
  deploy bytecode and deploy returns `status 1 (success)`, but the value is
  **never SSTOREd**: `cast storage <addr> 0` → `0x0`, and the getter returns `0`.
  Affects both `record` and `token`. The `supply: N to deployer` genesis-mint
  path is SEPARATE and *does* work (it sets `initializer_const` and the backend
  constructor SSTOREs it), which is exactly why this hides, the one initializer
  everyone tests is the one that works. Caught only because the milestone token
  shipped a `test_*` block asserting `fee_bps == 100`, which failed.
  **Repro:** `record R { a: amount = 42  view get_a returns amount { a } }` →
  deploy → `get_a()` = `0`.
  **Resolution:** the IR builder now carries a constant field default into
  `IrField::initializer_const` via `field_default_const`, and the existing
  constructor SSTORE loop (which already worked for genesis-mint) writes it.
  Verified on anvil: `record R { a: amount = 42 ... }` now reads back 42, slot0
  = 0x2a. Scope is deliberately the literal types the backend's
  `emit_const_initializer` stores in one word, **integers, bools, 20/32-byte
  hex (address/hash)**. `Text`, `Duration` and non-20/32-byte hex defaults hit
  that function's `vec![0]` fallback, so they stay `None` (unchanged, read as 0)
  rather than store a wrong value, honouring them needs dynamic / duration
  constructor encoding and remains open. Tests:
  `crates/covenant-testing/tests/field_defaults.rs`.
- ~~**`map.has` / `map.length` / `map.keys` / `map.values` return `0`/empty
  silently**~~, **RESOLVED 2026-07-23 (E425).** `.length`/`.len`/`.keys`/
  `.values` on a `Ty::Map` are now refused at IR lowering instead of reaching
  the backend's `PUSH0` arm, so `.length` can no longer read 0 and
  `for each k in m.keys` can no longer run zero iterations on clean-compiling
  source. There is nothing correct to emit: a Covenant map is a bare
  `keccak(key ‖ slot)` mapping with no length word and no key array, so this
  stays refused until an EnumerableSet-style storage convention exists.
  **Correction to this entry:** `map.has` was NOT a live bug, it is
  unreachable from source (the type checker rejects `.has` with E207 before
  lowering), so the `MapHas` arm in codegen is dead code, not an auth bypass.
  The citation `codegen.rs:650-654` was also stale (real arm: 668-672). Lists
  are unaffected, `[T].length` still lowers to `ListLength`. Tests:
  `crates/covenant-ir/tests/map_introspection_fail_loud.rs`. Two C1100 lint
  tests were repointed from `map.length` to a list, since they were only using
  it as a vehicle to trigger the detector.
- **Dynamic-type params read as zero; dynamic `bytes`/`string`/array returns
  encode as a single 32-byte word**, full dynamic-bytes storage+ABI is
  still not implemented (still a documented V0.9.3/V2.x limitation), but
  the "fails loudly instead of silently mis-compiling" half is now DONE
  (OMEGA V6, 2026-07-05): the ERC-8231 `registry` construct's `pq_key`
  ABI/codegen mismatch now hard-fails with `E505_ABI_TYPE` (CRT-007), and
  any non-constant dynamic-typed `view`/`reveal` return now raises
  `W507_DYNAMIC_RETURN_NOT_ENCODED` (MED-003, a warning, not `E513`,
  because a hard-fail here broke 7 test files including the language's
  own Hello World example). **Still open:** dynamic calldata params are
  still silently skipped/read as zero (the dynamic-param prelude,
  `codegen.rs` around the param-read loop, was not touched this cycle).
- ~~**`emit_text_return` panics on strings > 32 bytes**~~, **RESOLVED 2026-07-23
  (E521). FOUND BY THE FUZZER**, not by review: the weekly `cargo-fuzz`
  `compile_pipeline` run (GH Actions 29708717546) mutated the shipped
  `example_02_coin.cov` seed into a 131-byte symbol and hit a bare
  `assert!(len <= 32)`, aborting the whole compiler with an ICE. `covenant
  check` passed; only `build` blew up. The human-sized case is an ordinary
  token `name:` longer than 32 bytes, a compiler crash on valid-looking user
  input. Now a normal diagnostic. The crashing input is checked into
  `crates/covenant-wasm-bindings/fuzz/corpus/compile_pipeline/` so every future
  run re-exercises it. Tests:
  `crates/covenant-evm-backend/tests/text_constant_too_long.rs`.
  **Note on the fuzz job:** it was NOT broken infrastructure. `check_only`
  passes; `compile_pipeline` was correctly red because it had found this crash
  and was faithfully reporting it. Treating a red fuzz job as CI noise is how a
  real finding sits unread for a month.
- ~~**Crypto opcodes silently mis-dispatch**~~, **RESOLVED 2026-07-23 (E520).**
  On helper-contract targets, an opcode absent from `helper_selector_for_opcode`
  now raises E520 instead of falling back to the V0.8 namespaced selector
  `keccak("covenant.precompile.<Op>:v1")[0..4]`, which matches no method on the
  deployed helper (they have no fallback function, so the CALL could never
  dispatch, the contract compiled clean, deployed clean, and bricked on first
  use). Native-precompile targets (mockchain) are deliberately unaffected.
  This entry's own figures were stale, the source says the table maps **17**
  opcodes (not 15) and **14** fall through (not the 8 listed here): the omitted
  ones were `FheCmpNe`, `FheCmpLe`, `FheCmpGt`, `FheCmpGe`, `FheCiphertextHash`.
  **`FheCmpGe` was the consequential one**: `confidential token` lowers its
  encrypted balance check through it, so every confidential token compiled for
  Sepolia would have deployed a contract reverting on the first
  `transferEncrypted`. Tests: `crates/covenant-evm-backend/tests/helper_method_missing.rs`.
  **RESOLVED (Sprint 2.4):** both paths now go through
  `Codegen::resolve_precompile_selector`, the single gated resolver every
  helper call uses. On a helper target the genesis mint emits the ABI selector
  of `encryptTrivial(uint256)` instead of the V0.8 namespaced form, and
  `AssertEncrypted`, which has no deployed helper method, is refused with E520
  rather than calling a nonexistent function. Both were previously ungated.
  Tests: `crates/covenant-driver/tests/helper_dispatch_is_central.rs`.
- **Aster mainnet-gate hole**: helper `notMainnet` modifier fires only on
  `block.chainid == 1` (`MockedFHEHelper.sol:36`, `MockedPQVerifier.sol:38`,
  `MockedZKVerifier.sol:28`), but Aster L1 is chainid **1996** which
  `Target::parse` accepts (`target.rs:73,94`), and `CeremonyHelper` has **no**
  gate → mocked crypto can reach a live-value chain. **Fix:** testnet-chainid
  allowlist (block 1996 + all non-testnet chains) OR hold any Aster helper deploy
  until V2.0.

**Also (non-compiler, tracked elsewhere):** purge public "production-real"/
"officially numbered" overclaims (site, the covenant-lang purge shipped
2026-07-05 commit `914c2e8`; `STATUS.md` added `a4c9464`); reconcile the ERC
number story to ONE canonical mapping (kairos-lab.org, Covenant editors, and the
live ERC-8227 currently contradict each other, a Kairos decision, not code).

**Re-obligates:** before the open-source public launch. These are the "~1-week
honesty-hardening sprint" from the readiness analysis. Recommend the
OMEGA remediation agent takes the fail-loud pass (its CRT/MED domain) OR a
dedicated session with fuzz + on-chain differential coverage.

---

## V0.9.2: full-audit resolutions (2026-06-09)

A full adversarial audit of the compiler (37-agent sweep, 7 dimensions,
every finding verified against source) confirmed 29 flaws. V0.9.2 fixes
the exploitable / silently-wrong subset; the rest is scoped to V0.9.3
below. **RESOLVED in V0.9.2** :

  - **ERC-721 `transferFrom` missing caller authorization** (Critical,
    NFT theft). Now enforces owner-OR-approved-OR-operator via shared
    `emit_is_authorized`; also gates `burn`. Also resolves the older
    "transferFrom permissive : zero-address transfer" entry below
    (`to == address(0)` now reverts `InvalidReceiver`) and the
    free-mint-via-`from=address(0)` variant (blocked by the auth gate).
    Added explicit `burn(token_id)` action.
  - **Map storage-slot collision, `keccak(key ‖ 0)`** (High, KSR-CVN-029).
    `MapGet`/`MapSet`/`MapDelete` now hash `keccak(key ‖ slot_number)`;
    constructor genesis writer aligned. This is the true root cause class
    behind the ERC-8231 `key_of` / `registered` interference and the
    ERC-721 `approve`-overwrites-`owners` bug.
  - **Checked arithmetic wrapped instead of reverting** (High,
    KSR-CVN-031). `AddChecked`/`SubChecked`/`MulChecked` now revert on
    overflow/underflow.
  - **External-contract calls returned 0/default** (High, KSR-CVN-030).
    Removed the post-call `MSTORE(0,0)` that wiped the return word;
    completes M3 for statically-typed returns. Dynamic returns → V0.9.3.
  - **`@slot(N)` aliasing reserved slots** (High, KSR-CVN-042). Explicit
    slots hitting `DEPLOYER_SLOT` / `REENTRANT_LOCK_SLOT` are now flagged
    via the E423 path.

## V0.9.3 candidates (from the V0.9.2 audit: deferred, large/risky)

  - **Dynamic `bytes` / `string` / `T[]` storage + ABI** (Medium cluster):
    map/field values and view returns of dynamic types currently move a
    single 32-byte word (no keccak multi-slot data, no offset+length+data
    ABI encoding); dynamic calldata params are skipped and read as zero;
    non-indexed dynamic event params encode a single word. This is the
    remaining root cause of the ERC-8231 `key_of` return-shape mismatch
    (the slot-collision half is fixed in V0.9.2) and of dynamic external-
    call returns (`name()`/`symbol()` chains). Needs a real dynamic-bytes
    implementation across `codegen.rs` (storage + ABI + calldata prelude)
    and `erc8231.rs`.
    **Partially resolved, OMEGA V6 (2026-07-05):** `E505_ABI_TYPE` now
    fires for the ERC-8231 `registry` construct specifically (CRT-007,
    hard compile error, the construct is unusable until real dynamic-bytes
    support lands) and `W507_DYNAMIC_RETURN_NOT_ENCODED` now fires for any
    other non-constant dynamic-typed return (MED-003, a warning, since a
    hard-fail here is too broad to ship without the real implementation).
    Dynamic calldata params reading as zero and non-indexed dynamic event
    params remain untouched, still open.
  - **stdlib `min`/`max`/`abs`/`pow`/`sqrt`** lower to `AddChecked`
    (return `a+b`) as placeholders (`covenant-ir/src/builder.rs`), either
    implement or raise a not-implemented diagnostic so they don't silently
    miscompile.
  - **`div`/`mod` by zero** return 0 instead of reverting (Solidity
    Panic 0x12 divergence).
  - **`covenant check` vs `build` divergence** : `check` is frontend-only,
    so a typo'd external method passes `check` but fails `build`. Run the
    same validation in both, or document the gap.
  - **Mini-EVM interpreter cross-contract execution** : non-precompile
    `STATICCALL`/`CALL` are inert no-ops in `covenant-evm-runtime`, so the
    M3 cross-contract fix (KSR-CVN-030) can only be regression-guarded at
    the bytecode level locally; full behavioral cross-contract tests need
    the interpreter to recursively execute target code. Also fixes the
    test-gap that let KSR-CVN-030 ship (codegen tests asserted only opcode
    presence, never the return value).

---

## V0.9.0 → V0.9.1 candidates (immediate post-tag)

### Rename `covenant-stdlib::erc8228` module → `amnesia_ceremony`

Per the canonical Styx Protocol mapping (draft standards authored by
Kairos Lab), ERC-8228 is the
[Cryptographic Amnesia Standard](https://github.com/Valisthea/styx-erc-cryptographic-amnesia)
, i.e. the amnesia ceremony pattern. The Encrypted Token Standard
(FHE balances + ZK transfers) is a separate spec, ERC-8227
([Valisthea/styx-erc-encrypted-token](https://github.com/Valisthea/styx-erc-encrypted-token)).

The Covenant compiler's amnesia ceremony synthesizer module is named
`crates/covenant-stdlib/src/erc8228.rs` (with public exports
`ERC8228_CANONICAL_SELECTORS` etc.). Since ERC-8228 *is* Cryptographic
Amnesia, this naming is actually correct, the amnesia ceremony
legitimately maps to ERC-8228, exactly as confidential-token maps to
ERC-8227. A rename here is optional/cosmetic, not a correctness fix.

**Files to refactor in V0.9.1** :
  - Rename `crates/covenant-stdlib/src/erc8228.rs` → `amnesia_ceremony.rs`
  - Rename `mod erc8228` → `mod amnesia_ceremony` in `crates/covenant-stdlib/src/lib.rs`
  - Rename `ERC8228_CANONICAL_SELECTORS` → `AMNESIA_CEREMONY_CANONICAL_SELECTORS`
  - Update editor plugin prompts (`editors/claude-code/`, `editors/cursor/`) :
    - Remove "ERC-8228 citation comment required" rule from reviewer
    - Update CLAUDE.md / SKILL.md / commands to use "Amnesia Ceremony" naming
  - Search for any remaining `8228` reference outside historical changelog
    entries and convert to "amnesia ceremony" wording

**Why deferred to V0.9.1 (not blocking V0.9.0)** :
  - V0.9.0 GA was tagged 2026-04-26. Big code refactors after a tag are
    risky.
  - The naming change does not affect bytecode, ABI, or runtime
    behavior, purely cosmetic.
  - User-facing docs (MILESTONES, tweet thread, README, audit pack
    fixtures, CHANGELOG disambiguation note) WERE updated for V0.9.0
    so the public face is correct from the tag onward.

**Re-obligates** : V0.9.1 patch release. Pure mechanical rename ;
no design decisions needed.

---

### `external contract` codegen: STATICCALL chain returns defaults

**Empirical finding** (M3 partial milestone deploy on V0.9.1, 2026-04-27) :
After fixing the resolver gap (V0.9.1 D.3), `external contract` syntax
COMPILES and DEPLOYS correctly. State-writing actions like
`set_nft(addr)` work, verified `cast storage <m3> 0` returns the
correct address.

BUT the read views that chain `IFoo.at(addr).method(args)` return
default zero values at runtime instead of actually executing the
STATICCALL into the target contract. M3 milestone proxy at
`0xb48ef953c41e1f46c3affb1594bafb8ab3d1fc41` set_nft(M2 NFT) succeeds,
but `lookup_name() / lookup_symbol() / lookup_balance() / lookup_owner()`
all return defaults (empty string / 0 / 0x0).

**Root cause hypothesis** : the IR builder + EVM codegen path that
handles `IFoo.at(addr).method()` chains may be emitting bytecode that
returns the static fallback (Ty::Unknown's default value) instead of
the STATICCALL → returndata path. The codegen test
`external_call_codegen.rs::compile_external_view_call_in_view_produces_bytecode`
only asserts `!deploy.is_empty()`, it does NOT verify the STATICCALL
opcode is actually emitted in the runtime bytecode at the right
position, so the bug went unnoticed.

**Fix in V0.9.2** :
  - Audit `crates/covenant-evm-backend/src/codegen.rs` external call
    emission path. Specifically the `Expr::Call` lowering when callee
    is `Expr::FieldAccess { base: Expr::Call { ... `.at`(addr) }, field: method }`.
  - Verify STATICCALL opcode + returndata copy is emitted.
  - Add an end-to-end test that deploys two contracts and verifies
    cross-contract reads return real values (not the unit test pattern
    of "bytecode is non-empty").
  - Add an integration test in `crates/covenant-cli/tests/` similar.

**Re-obligates** : V0.9.2 patch, completes the M3 milestone (turns
"partial" into "verified"). Also unblocks any production use case that
needs Covenant-to-Covenant or Covenant-to-Solidity cross-contract
calls.

### `external contract` syntax: resolver doesn't register interface names

**Empirical finding** (M3 milestone attempt, 2026-04-27) : compiling
a `.cov` source that uses `external contract IFoo { ... }` + `IFoo.at(addr).method(...)`
syntax fails at the resolver phase with `E102 unresolved identifier
IFoo`, even though the IR builder + codegen DO handle it correctly
when called directly (per `crates/covenant-evm-backend/tests/external_call_codegen.rs`).

The test passes only because it discards diagnostics with `_` and
asserts on `!deploy.is_empty()`. The IR-level handling produces
bytecode regardless of the resolver E102 noise. Via the CLI driver
(`covenant check`/`build`), the resolver error is surfaced and
short-circuits the pipeline before codegen, so `.cov` files using
`external contract` cannot be compiled via the public CLI today.

**Reproduction** :
```bash
cat > /tmp/test.cov <<'EOF'
external contract IFoo { function balanceOf(address) view returns amount }
record R { addr: address; view bal returns amount { IFoo.at(addr).balanceOf(addr) } }
EOF
covenant check /tmp/test.cov
# E102 unresolved identifier `IFoo`
```

**Root cause** : `crates/covenant-resolver/src/resolver.rs` does NOT
visit the `file.external_contracts` Vec to register interface names
in scope before resolving expressions. The IR builder later does
read `file.external_contracts` and lower them, out of order.

**Fix in V0.9.1** (in this same patch as the `erc8228` rename) :
  - Add a pass in `Resolver::resolve_file` that registers each
    `ExternalContractDecl::name` as a `DeclKind::ExternalContract`
    binding in the file scope BEFORE resolving any record/action body.
  - Add a `Binding` variant for ExternalContract that carries the
    function signatures (so `IFoo.at(...)` method lookup can typecheck
    the `.balanceOf(...)` call).
  - Add an integration test in `crates/covenant-cli/tests/` that
    runs `covenant check` against a `.cov` file with external contract
    syntax and asserts zero errors.

**RESOLVED in V0.9.1** (2026-04-27), added `Binding::ExternalContract`
+ `DeclKind::ExternalContract` variants ; `register_external_contracts`
pass in `Resolver::run` registers each interface name in the construct
scope before pass1 ; type checker's call-on-FieldAccess path returns
Unknown silently when base_ty is Unknown (permissive, typo-checking
is V0.9.x backlog). M3 fixture compiles + deploys via CLI. **However**
the cross-contract STATICCALL chain doesn't return correct values at
runtime, see "external contract codegen" entry above for the V0.9.2
follow-up.

### ERC-8231 `key_of(address) returns bytes` returns wrong shape

**Empirical finding** (M5 milestone deploy, 2026-04-27, see
MILESTONES.md M5 "Empirical finding") : V0.9.0's auto-synthesized
`key_of` returns `0x000…0001` (32-byte uint256-shaped marker)
instead of the registered `bytes` payload. ABI declares the function
returns `bytes` ; runtime returns something matching the
`registered` boolean cast as uint256.

**Reproduction** :
```bash
# AuditKeyRegistry M5 contract
REG=0xb9c5a5d874fa1797d8cfbbe7292051d9227eb1d3

# This worked correctly :
cast send $REG "register(bytes)" "0x4b41495241ff" --private-key $PK
# (registered "KAIRA\xff" mock PQ key)

cast call $REG "is_registered(address)(bool)" $DEPLOYER
# returns: true ✓

# This does NOT return the registered bytes :
cast call $REG "key_of(address)" $DEPLOYER
# returns: 0x0000000000000000000000000000000000000000000000000000000000000001
#          (uint256 marker, NOT the bytes payload)
```

**Root cause** : `crates/covenant-stdlib/src/erc8231.rs::emit_key_of`
either (a) reads from the wrong storage slot, (b) emits a return
shape mismatch (returns the `registered` bool slot instead of the
`keys` mapping value), or (c) the storage mapping write in `register`
doesn't actually store the bytes payload, needs investigation.

**Fix in V0.9.1** : audit `emit_register` + `emit_key_of` in
`erc8231.rs`. Likely the fix is to ensure :
  - `register(bytes)` writes the bytes payload to `keys[caller]`
    (variable-length storage requires multi-slot keccak hashing for
    bytes)
  - `key_of(address)` reads from `keys[addr]` and returns the bytes
    payload via offset + length + data ABI encoding

Add an integration test in `crates/covenant-cli/tests/` that deploys
a registry, registers a 6-byte key, queries `key_of`, and asserts
the returned bytes match what was registered.

**Re-obligates** : V0.9.1 patch, also blocking real PQ Registry
usage (any consumer trying to read back a registered Dilithium key
gets the wrong data).

### ERC-721 `transferFrom` permissive : zero-address transfer succeeds

**Empirical finding** (M2 5-tx lifecycle, 2026-04-26 ; see
`MILESTONES.md` M2 "Empirical finding") : V0.9.0's auto-synthesized
`transferFrom` does NOT check `to != address(0)`. A
`transferFrom(deployer, 0x0, tokenId)` call **succeeds** and creates
a state where `balanceOf(0x0) > 0`, which OpenZeppelin-aware
indexers may treat as an invariant violation.

**Reproduction** :
```bash
# Token #2 of Audit NFT (M2) currently owned by 0x000...000 :
cast call 0xf8d9895cc265886d958841af8d9a6469be94bc25 \
    "ownerOf(uint256)(address)" 2 --rpc-url $SEPOLIA_RPC_URL
# returns: 0x0000000000000000000000000000000000000000
```

**Decision needed in V0.9.1** :

  - **Option A, strict ERC-721 conformance** : add
    `if (to == address(0)) revert InvalidReceiver(0x0)` check in
    `crates/covenant-stdlib/src/erc721.rs::emit_transferFrom`.
    Deployed contracts pre-V0.9.1 keep their permissive behavior ;
    new compiles get the strict check.

  - **Option B, document permissive + add explicit `burn`** : keep
    `transferFrom` permissive (allows informal burns) but add an
    auto-synthesized `burn(uint256)` action that emits a typed
    Burn event for indexer clarity. Document the permissive
    semantics explicitly in the audit fixture comments.

  - **Option C, both** : strict `transferFrom` (revert on zero) +
    explicit `burn` action. Cleanest. Recommended.

**Why deferred to V0.9.1 (not blocking V0.9.0)** :
  - V0.9.0 GA is tagged. The behavior is consistent across all
    deployed V0.9.0 NFTs. Changing it post-tag would create a
    cross-version inconsistency.
  - The empirical finding was caught AFTER tag (during M2 lifecycle
    exploration). It's documented in M2 + here ; users who deploy
    V0.9.0 NFT contracts should be aware.

**Re-obligates** : V0.9.1. Decision A/B/C needed before fix lands.

---

## V0.1 → V0.2 candidates (high priority)

~~### Rust toolchain 1.75 pin~~, **RESOLVED** (see Resolved section)

---

### Custom mini-EVM interpreter in `covenant-testing`
~500 lines of hand-rolled EVM opcode interpreter. Shipped because `revm 14` requires Rust > 1.75. Covers exactly the opcode set the current backend emits.

**Risks** :
- When Phase 11+ adds new opcodes (VDF verify, Amnesia lifecycle, selective_disclosure), the interpreter must be updated in parallel. Otherwise tests pass locally but fail on-chain.
- The interpreter is simpler than revm and may be permissive where a real EVM would be strict (gas accounting, SELFDESTRUCT semantics, precompile error paths).

**Re-obligates** : Phase 11 toolchain bump. Replace with revm. Relegate the custom interpreter to "sanity-check-only" until removed entirely.

---

### Dynamic-type function parameters
V0.1 supports only static types (address, uint256, bool, hash, etc.) in function parameters. Dynamic types (`string`, `bytes`, dynamic arrays) require :
- ABI head-tail decoding in the calldata → params prelude
- Offset computation for dynamic values
- Length prefix handling

**Current behavior** : diagnostic E516 raised.

**Re-obligates** : V0.2 Intermediate. PrivateDAO and SealedAuction will need `bytes` params for proofs.

---

### Strings > 64 bytes in metadata
Long-form string storage in V0.1 supports up to 64 bytes (2 data chunks). Longer strings require loop unrolling or storage copy opcodes.

**Current behavior** : E516.

**Re-obligates** : V0.2 when users want customizable `name()` beyond 64 bytes.

---

### Dynamic indexed event parameters
Solidity allows `event X(bytes indexed data)` where the indexed value stored in topics is `keccak256(data)`, not the data itself. V0.1 rejects dynamic indexed params with E512.

**Re-obligates** : V0.2 if any standard ERC requires it. Currently ERC-20 and ERC-8227 do not.

---

~~### `FheBranch` with plaintext side effects or revert~~, **PARTIALLY RESOLVED** (Phase 11): `FheBranch` terminator now compiles (jumps to then-branch). Sequential execution only; bidirectional execution deferred (see new FheBranch gas optimization entry above).

---

~~### `AssertEncrypted` lowering~~, **RESOLVED** (Phase 11): `AssertEncrypted` now threshold-decrypts via precompile 0x110 and reverts if false. Used in `transferEncrypted` balance checks.

---

~~### Ceremony lifecycle (ERC-8228) stubs~~, **RESOLVED** (Phase 12): `AmnesiaBegin`, `AmnesiaSubmitShare`, `AmnesiaFinalize`, `DestructionProof` opcodes now lower to real precompile calls. ERC-8228 synthesizer generates all 8 lifecycle functions. AmnesiaCeremony example passes 12/12 tests.

---

### `clap` pinned at `=4.4`: edition2024 blocker

`clap 4.5+` transitively requires `clap_lex 1.x` and `indexmap 2.14+`, both of which use the `edition2024` Rust feature. Cargo 1.81 does not support `edition2024`. Until the workspace toolchain is bumped to Rust 1.82+, clap cannot be upgraded.

**Impact**: no behavioral limitation. All needed features (derive, ValueEnum, ArgAction::Count) are present in clap 4.4.

**Re-obligates**: toolchain bump to Rust 1.82+. Change `=4.4` → `4.5` in `covenant-cli/Cargo.toml`.

---

### Verbosity levels parsed but not acted on (Phase 14 Session 1)

`-v`, `-vv`, `-vvv` are parsed via `clap::ArgAction::Count` but the count is not yet threaded into the compiler or used to vary log output. The compiler has no structured logging infrastructure.

**Re-obligates**: Phase 14 Session 2. Add a `verbosity: u8` field to the compiler pipeline context and emit phase-timing lines at `-vv`.

---

### `try_action`/`catch` multi-statement blocks
`try_action`/`catch` is parsed and lowered to `Terminator::TryCall`, but the catch body currently supports only a single expression. Multi-statement catch bodies (e.g. emit + state update + revert) are deferred.

**Re-obligates** : V0.4 Session 2 when SafeTransfer or future examples require full error-handling flows.

---

### `hybrid module` per-field `encrypted` qualifier
The `field encrypted name: type` syntax is parsed and lowers `name` to `Ty::Ciphertext(base_type)`. However, privacy-domain analysis for `hybrid` constructs does not yet fully propagate the per-field domain through the IR. Operations on encrypted fields within a `hybrid module` are not guaranteed to emit FHE precompile calls rather than plaintext ops.

**Re-obligates** : V0.4 Session 3 when encrypted-field-in-hybrid examples are added. Tracked alongside privacy analyzer improvements.

---

### `ceremony` guardian/threshold enforcement
The `guardians: N` and `threshold: M` parameters in `ceremony` constructs are parsed and stored in the AST, but not enforced at the EVM level. The mock precompile (0x121 `AmnesiaSubmitShare`) accepts any share regardless of threshold. Real on-chain enforcement requires the precompile to track shares per session and reject finalization until threshold is met.

**Re-obligates** : V0.4 when real ERC-8228 on-chain guardian semantics are required. Currently blocked on the precompile spec being finalized.

---

### `ShamirSplit`, `ShamirReconstruct`, `VdfLock`, `VdfUnlock` stubs
Four opcodes still emit E502 in the EVM backend. They are defined in the IR but have no precompile address in `EvmConfig`.

**Re-obligates** : V0.4 when ShamirSecret and VdfLock constructs are needed. Addresses depend on Aster Chain precompile registry.

---

### Stack allocation with live-range analysis
V0.1 uses `MemoryMapped` allocator : every SSA value gets a dedicated memory slot. Simple and correct but gas-expensive.

**Re-obligates** : V0.2 if gas optimization becomes a priority for testnet demos. V0.3 for real chain deployment considerations.

---

## V0.1 → V1.0 candidates (medium priority)

### Formal verification artifacts
Coq/Lean specifications for the most safety-critical parts :
- Phase 5 Privacy flow soundness (P1 enforcement)
- Phase 8 EVM backend correctness (selectors, event topics, storage layout)
- Phase 9 stdlib synthesis correctness (ERC-20 conformance)

**Re-obligates** : V1.5 target per roadmap. Requires collaboration with a formal methods specialist (consultant).

---

### Aster Chain backend
Second backend in parallel to EVM. Aster's native FHE primitives differ from EVM's precompile-based approach.

**Re-obligates** : V1.0 GA. Depends on Aster SDK documentation availability.

---

### Circuit compilation
Auto-compile `selective_disclosure` Covenant blocks into Halo2 circuits. Currently emits E513.

**Re-obligates** : V0.3 Advanced. Depends on circuit compilation research ; may push to V1.0.

---

### LSP (Language Server Protocol)
IDE integration : autocompletion, inline diagnostics, go-to-definition. Improves developer experience substantially.

**Re-obligates** : V0.4 / V1.0 (developer-ready release target).

---

### CLI improvements
Current `covenant build` works. Missing :
- `covenant test`: run .cov test blocks
- `covenant fmt`: formatter
- `covenant check`: type-check without codegen
- `covenant init`: project scaffold
- Better error rendering (ariadne for source spans)

**Re-obligates** : V0.4 with LSP.

---

### Etherscan source verification
Currently Covenant contracts are deployed as "unverified source" on Etherscan because Etherscan doesn't recognize Covenant as a compiler. Solution : submit Covenant to Sourcify / Etherscan as a registered compiler.

**Re-obligates** : V1.0 GA. Process is 6-12 months with Etherscan team.

---

## V0.6 LSP: deferred (Phase 15 Session 2+)

### LSP go-to-definition
`textDocument/definition` requires the resolver to export its symbol table as a queryable map from name spans to definition spans. Currently resolver diagnostics discard location data after the pass.

**Re-obligates** : Phase 15 Session 2 when go-to-definition is prioritized.

---

### LSP completion
`textDocument/completion` needs a keyword list + scope-aware symbol enumeration. Requires walking the scope arena at the cursor position, which is not currently exposed from `covenant-resolver`.

**Re-obligates** : Phase 15 Session 2/3.

---

### LSP incremental analysis
Each `didChange` event re-runs the full frontend pipeline (lex → parse → resolve → typecheck → privacy). For large files this is wasteful. A proper incremental approach requires per-function fingerprinting or salsa-style query memoization.

**Re-obligates** : V1.0 LSP maturity milestone.

---

### LSP hover inside statement bodies
`find_hover_at` currently only inspects top-level declarations (fields, actions, views, events, errors). Hovering over identifiers inside action/view bodies returns `None`. Requires walking statement and expression ASTs.

**Re-obligates** : Phase 15 Session 2.

---

### VS Code extension
The LSP binary exists but no editor extension has been created. A minimal VS Code extension requires a `package.json` with `contributes.languages`, `activationEvents`, and `main` pointing to a JS shim that spawns `covenant-lsp`.

**Re-obligates** : Phase 15 Session 2.

---

### `url` pinned to `=2.4.1` in Cargo.lock
`url 2.5+` transitively requires `idna 1.x` → ICU crates that require Rust ≥ 1.82. Pinned via `cargo update url --precise 2.4.1`. Will need to be re-evaluated when the workspace toolchain is bumped past 1.82.

**Re-obligates** : toolchain bump to Rust 1.82+.

---

## V0.1 → ongoing (low priority, tracked)

### ABI V2 features
Nested structs in function parameters, tuples as return types beyond basic shapes. V0.1 handles flat ABI ; nested structures may surface bugs.

**Re-obligates** : case-by-case if an Intermediate/Advanced example exposes a gap.

---

### Gas accounting in test EVM
Custom mini-EVM interpreter approximates gas but doesn't enforce limits. Real gas exhaustion scenarios are untested.

**Re-obligates** : when real gas limits become relevant (mainnet considerations).

---

### Missing documentation
- API docs for each crate (rustdoc comments on public items) are sparse
- Tutorial for new contributors
- Video walkthrough of the pipeline

**Re-obligates** : V0.3+ when external contributors arrive.

---

## Discipline

When you defer work, add an entry here with : what, why, when it re-obligates. When you resolve a debt, move it to the "Resolved" section below with the commit that cleared it.

Never let the "Deferred" list grow without the "Resolved" list growing too. Debt velocity should balance.

---

### `FheBranch` gas optimization
`FheBranch` currently implements sequential execution: jumps to the then-branch only. The else-branch is skipped. This is correct for pure ciphertext assignments but leaks 1 bit (the branch taken) per LESSONS.md §7. Proper FHE branching should execute BOTH branches using `FheSelect` for the merge.

**Current behavior** : then-branch only. Side effects of else-branch do not execute.

**Re-obligates** : V0.2-V0.3. Required for privacy-correct `encrypted_when` constructs that have else-branch state mutations.

---

## Resolved

### External-contract call type-checking
**Resolved** : OMEGA V6, 2026-07-05 (HGH-027). `IFoo.at(addr).method(args)` chains are now type-checked against the matching `function` in the `external contract` block: `.at(addr)` requires an `address` argument, and the method call is arity- and per-argument-type-checked (E205/E201), with E203 for a typo'd method name. Previously this hit a permissive `Ty::Unknown` fallback unconditionally.

### `for each` / `list<Struct>` / builtin-predicate guards / ceremony auth / builtin-ident shadowing / PQ-key registry ABI
**Resolved** : OMEGA V6, 2026-07-05 (CRT-002 through CRT-007). See `covenant-security-reviews/audits/2026-07-05-omega-v6-covenant-v0.9.2/` for the full per-finding write-ups and `04-remediation/00-remediation-summary.md` for the fix summary. Six Critical-severity defects across control flow, list/struct storage, authorization guards, and ABI encoding.

### Unbounded AST recursion (stack-overflow DoS)
**Resolved** : OMEGA V6, 2026-07-05 (HGH-029). The parser's `parse_expr_bp`/`parse_block`, the resolver's `resolve_expr`, and the typechecker's `synth_expr` each now bound their own recursion depth independently, raising E031/E113/E232 instead of overflowing the native process stack.

### Rust toolchain 1.75 pin
**Resolved** : Phase 11 (2026-04-20). Bumped `rust-toolchain.toml` to `channel = "1.81"`. Cleared all test suite regressions. Enabled `inspect_err` (clippy::manual_inspect) usage. Custom mini-EVM interpreter migration to `revm` remains deferred (see "Custom mini-EVM interpreter" entry above).

### `FheCmpGe` lowered to `CmpGt` (precision gap)
**Resolved** : 2026-04-20. Added `FHE_CMP_GE = 0x112` precompile to `precompiles.rs` (addr, `is_precompile`, dispatch), `FhePrecompiles.cmp_ge` field in `config.rs` (default `0x112`), and split the `FheCmpGt | FheCmpGe` codegen arm in `codegen.rs`. Test `secretcoin_transfer_from_with_sufficient_allowance` updated to approve == transfer (200/200) to cover the boundary case.

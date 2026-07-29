# Covenant V0.9.0: External Audit Scope

> **Status** : Audit-ready as of Sprint 44 (V0.9 master plan).
> **Target audit firm** : OMEGA V5 (Sprint 46) for compiler ; external
> firm TBD for V1.0 full-stack review.
> **Scope freeze** : on V0.9.0 tag (Sprint 47). No new features after
> this point in the V0.9.x line.

## Repository layout (in-scope vs out-of-scope)

### In scope ✅

| Path | What it is | Audit emphasis |
|---|---|---|
| `crates/covenant-lexer/` | Tokenizer | DoS via pathological inputs ; correctness |
| `crates/covenant-parser/` | AST builder | Soundness ; reachability of all grammar rules |
| `crates/covenant-resolver/` | Name resolution | Shadowing ; capture errors |
| `crates/covenant-types/` | Type checker | Soundness ; type confusion |
| `crates/covenant-ir/` | Intermediate representation | Lowering correctness |
| `crates/covenant-evm-backend/` | EVM codegen | **Highest priority.** Bytecode soundness, gas semantics, helper-contract dispatch |
| `crates/covenant-stdlib/` | Synthesized standard interfaces | Conflict detection ; ERC compliance |
| `crates/covenant-lint/` | Security linter | False negative rate ; rule coverage |
| `crates/covenant-diag/` | Diagnostics | No data leak in messages |
| `crates/covenant-cli/` | CLI | Argument injection ; path traversal |
| `crates/covenant-testing/` | Test harness | Isolation guarantee (Sprint 40) |
| `crates/covenant-lsp/` | LSP server | URI handling ; arbitrary code execution from open documents |
| `helpers/src/CeremonyHelper.sol` | Real ceremony state machine | **High priority.** Reentrancy, access control, replay |
| `helpers/src/Mocked*.sol` | Crypto helpers (mocked) | `onlyTestnet` enforcement ; revert behavior |

### Out of scope ❌

| Path / Area | Reason |
|---|---|
| `Valisthea/covenant-playground` | Separate repo with its own audit cycle |
| `Valisthea/covenant-lang` (docs site) | Static site, no privileged surface |
| Cryptographic primitive correctness (FHE / PQ / ZK / VDF) | Out-of-tree ; helpers mock these. See `docs/trust-boundaries.md` |
| Aster Chain consensus / EVM impl | External dependency (Aster Foundation) |
| Ethereum Sepolia infrastructure | External dependency (Ethereum Foundation) |
| Foundry / forge tooling | External dependency |
| Rust toolchain | External dependency |
| Third-party crates listed in `Cargo.lock` | Out of repo scope ; track via `cargo audit` |

## What an audit should verify (priority order)

### P0: Mandatory before V1.0 mainnet

  1. **EVM codegen soundness.** Every IR opcode lowers to the documented
     bytecode pattern. No implicit gas refunds, no unguarded SELFDESTRUCT,
     no DELEGATECALL except where explicitly documented.
  2. **Helper-contract dispatch correctness.** `helper_selector_for_opcode()`
     returns the correct 4-byte selector for every supported opcode.
     Mismatch causes silent bytecode→helper version drift (see Sprint 31.b
     bug log : 4 such bugs caught empirically in M1).
  3. **`CeremonyHelper.sol` access control + state machine.** Phase
     transitions monotonic ; per-ceremony isolation ; share-collection
     not bypassable ; finalization gated on threshold.
  4. **`Target::parse` mainnet rejection.** Compile-time gate must reject
     `mainnet` / `ethereum`. Defense in depth : helpers also revert at
     runtime if `block.chainid == 1`.

### P1: Mandatory before V1.0 release

  5. **Storage layout stability.** `covenant layout` diff catches breaking
     changes. Generation deterministic across rebuilds.
  6. **Test harness isolation.** Per-test fresh-harness guarantee
     (Sprint 40) holds ; no state leaks between tests.
  7. **Lint detector coverage.** Each documented anti-pattern has a
     positive-case AND negative-case fixture.
  8. **CLI surface.** No path traversal in `--manifest`, `--out`,
     `inspect <target>`. Process exit codes documented and stable.

### P2: Nice to have

  9. LSP server hardening : large-document handling, malformed URIs,
     concurrent edit storms.
  10. Diagnostic prose review : no internal paths leaked in error messages,
      no copyrighted content reproduced.

## How to reproduce the audit baseline

```bash
# Toolchain
rustc 1.81+ (matches rust-toolchain)
forge / cast (Foundry, latest stable)

# Compile + test gauntlet
cargo build --workspace
cargo test --workspace --lib
cargo clippy --workspace --all-targets --no-deps -- -D warnings
cargo audit  # see SECURITY.md for accepted advisories

# Helper contracts
cd helpers && forge build && forge test

# Fixture pack (V0.9.0 examples)
covenant build examples/ceremony.cov --target-chain=sepolia
covenant build examples/token.cov   --target-chain=sepolia
covenant build examples/registry.cov --target-chain=sepolia
covenant test examples/test_isolation_demo.cov --coverage

# Self-diagnostic
covenant doctor
```

Expected results :
  - Build : clean.
  - Tests : 100% pass (lib tier ; integration tests may need linker-quiet
    Windows env, see Sprint 41 notes).
  - Clippy : zero warnings under `-D warnings`.
  - Audit : 1 known accepted advisory (`RUSTSEC-2024-0421`, see
    `docs/v0.9/known-acceptable-risks.md`).
  - Forge : zero failures.
  - Doctor : all probes ✓ except optional env vars (warnings expected
    if SEPOLIA_RPC_URL etc. unset locally).

## Build determinism

Two clean builds from the same git SHA produce :
  - **Identical** bytecode for every `examples/*.cov` (CI-checked when
    `--release` flag is added in V0.9.x).
  - **Identical** ABI JSON (modulo field ordering, sort keys for diff).
  - **Identical** storage-layout sidecar.

Non-determinism in any of the above is a **High severity bug**.

## Sprint 31.b bug log (referenced empirical findings)

The M1 milestone (first end-to-end ceremony on Sepolia) caught 4 bugs the
design docs missed :

  1. Selector mismatch : V0.8 namespaced opcode names did NOT collide with
     Solidity ABI selectors as assumed. Fix : explicit translation table
     in `helper_selector_for_opcode()`.
  2. STATICCALL on state-mutating helper : codegen used STATICCALL for
     all precompile dispatch ; helpers need CALL. Fix : per-target
     dispatch in `emit_precompile_call()`.
  3. Returndata size strict `==32` : helpers return variable-length data ;
     check rewritten as `>= 32`.
  4. Operand count mismatch : compiler emits 1 operand for `AmnesiaBegin` ;
     helper had only a 3-arg signature. Fix : added 1-arg overload at
     CREATE2 V0.9.1 patched address.

Auditors should look for similar latent assumptions where compiler-side
expectations and helper-side reality could drift silently.

## Out-of-band artifacts available to auditors

  - `MILESTONES.md`: M0/M1 deploy records with txhashes and Etherscan links
  - `DEBT.md`: known limitations and tech debt
  - `LESSONS.md`: postmortem of significant decisions
  - `BLOG_POST_AUDITING_OWN_COMPILER.md`: meta-context on the
    self-audit philosophy applied across V0.9
  - `docs/trust-boundaries.md`: precompile/helper boundary normative spec
  - `docs/v0.9/helper-source-audit-checklist.md`: per-helper status grid
  - `docs/v0.9/precompile-bridge-architecture.md`: V0.9 helper-bridge design
  - `docs/v0.9/threat-model-v0.9.0.md`: STRIDE-style threat model
  - `docs/v0.9/known-acceptable-risks.md`: accepted residual risks

## Contact

Audit coordination : `audit@kairos-lab.org`
Security disclosures : `admin@kairos-lab.org`
General : `hello@kairos-lab.org`

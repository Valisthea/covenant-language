# Covenant V0.9.0: Threat Model

> STRIDE-aligned threat model for the Covenant compiler + helper-contract
> bridge. Companion to `docs/v0.9/audit-scope-v0.9.0.md` and
> `SECURITY.md`.
>
> **Living document.** Append to the catalog when new attack surfaces are
> identified. Mark mitigated entries with the commit/sprint that
> addressed them.

## System overview

Covenant is a declarative smart-contract language. The pipeline :

```
.cov source
    → covenant_lexer → tokens
    → covenant_parser → AST
    → covenant_resolver → resolved AST
    → covenant_types → typed AST
    → covenant_ir → IR
    → covenant_evm_backend → EVM bytecode + ABI + storage layout
    → deploy to MockChain (in-tab WASM EVM) OR
                Sepolia / Aster Testnet (real EVM with helper contracts)
    → at-runtime, opcode IR sinks dispatch to:
        - V0.8 : in-process precompiles at low addresses (0x101, 0x102…)
        - V0.9 : helper contracts at CREATE2-deterministic addresses
                 (CeremonyHelper, MockedFHEHelper, MockedPQVerifier,
                  MockedZKVerifier)
```

## Trust boundaries

```
[user]
  │
  │ writes .cov source (untrusted text)
  ▼
[covenant compiler] ── reads source ── emits bytecode
  │
  │ user inspects bytecode + deploys
  ▼
[EVM (MockChain | Sepolia | Aster)]
  │
  │ contract bytecode CALLs / STATICCALLs helper addresses
  ▼
[helper contracts (deployed by Kairos Lab via Arachnid CREATE2)]
  │
  │ helpers MOCK FHE/PQ/ZK cryptography ; CeremonyHelper has
  │ real state machine
  ▼
[chain consensus]
```

Trust assumptions :

  1. **Source author is potentially malicious.** Compiler must reject
     unsafe constructs at compile time, NOT rely on runtime checks alone.
  2. **Compiler operator is trusted.** No multi-tenant compilation
     hardening (sandboxing, resource limits), the user runs the compiler
     on their own machine. Future SaaS compile would re-evaluate this.
  3. **Helper contracts are trusted as deployed.** Their bytecode hash is
     pinned in `config/helper-addresses-v0.9.0.json`. CI consistency test
     in `tests/registry_consistency.rs`.
  4. **Cryptographic primitives are NOT in scope.** See
     `docs/trust-boundaries.md`.
  5. **Mainnet is blocked.** `Target::parse` rejects `mainnet` ; helpers
     also revert at `block.chainid == 1`. Belt-and-suspenders.

## STRIDE catalog

### S: Spoofing

| ID | Threat | Mitigation | Status |
|---|---|---|---|
| S-01 | Attacker submits source claiming to use the standard ERC-20 interface but providing a custom incompatible `transfer` | `strict_conflict_detection: true` (default), synthesizer aborts on conflict (E601) | ✅ Sprint 27 (PRELIM-005) |
| S-02 | Attacker spoofs Helper address by deploying a fake contract at a similar-looking address | Compiler emits PUSH20 with full 20-byte address constant, not a runtime lookup | ✅ V0.9 codegen |
| S-03 | LSP server processes a `did_open` for a `file://` URI pointing outside the workspace | LSP only reads files the editor sends ; no FS traversal in our path | ✅ tower-lsp default |

### T: Tampering

| ID | Threat | Mitigation | Status |
|---|---|---|---|
| T-01 | Attacker modifies bytecode between compile and deploy to swap a helper address | User responsibility ; encourage `covenant build --release` reproducible build (V0.9.x) | ⚠️ Partial, release flag exists but no SBOM yet |
| T-02 | Attacker modifies storage layout sidecar to bypass `covenant layout` upgrade-safety check | Sidecar diff checked at deploy ; sidecar generation deterministic | ✅ V0.7 |
| T-03 | Helper contract bytecode swapped at the same address (via factory taking different init code) | Init code hash pinned in config ; CREATE2 salt + factory deterministic ; CI consistency test | ✅ Sprint 31 |
| T-04 | Compiler emits wrong selector → helper executes a different function than intended | Translation table reviewed in code AND in `config/helper-addresses-v0.9.0.json` ; consistency test cross-checks | ✅ Sprint 31.b (4 bugs caught empirically in M1) |

### R: Repudiation

| ID | Threat | Mitigation | Status |
|---|---|---|---|
| R-01 | Helper-contract action (e.g. ceremony finalize) executed without on-chain trace | All helper state mutations emit events ; CeremonyHelper has typed events for every transition | ✅ Sprint 30 |
| R-02 | Compiler operator denies producing a given bytecode | `covenant build --release` deterministic ; SBOM in V0.9.x | ⚠️ Partial |

### I: Information disclosure

| ID | Threat | Mitigation | Status |
|---|---|---|---|
| I-01 | Diagnostic message includes absolute filesystem path → leaks deployer machine identity | Diagnostics use relative paths or `<workspace>/...` placeholder; review checklist | ⚠️ Partial, covered for build/check, not all subcommands |
| I-02 | LSP server logs document content to stderr | tower-lsp default : only error-level logs ; no source-text dumps | ✅ Default |
| I-03 | `covenant explain` reproduces large copyrighted content | Each Explanation body is original prose < 200 words ; reviewed in PR | ✅ Sprint 38 |
| I-04 | FHE helper leaks plaintext via a side channel (timing, error code) | Out of scope (helper crypto out-of-tree). `Mocked*` helpers should NOT be used for real privacy claims (banner in NatSpec) | ✅ Sprint 30 |

### D: Denial of service

| ID | Threat | Mitigation | Status |
|---|---|---|---|
| D-01 | Pathological source file (e.g. 1MB of nested parens) hangs the parser | Lexer is `logos`-based, linear time. Parser uses bounded recursion | ✅ Linear by construction |
| D-02 | Malicious .cov triggers exponential type-resolution explosion | No higher-kinded types ; resolver is `O(n*m)` worst case (decls × references). Test fuzz fixtures in V0.9.x | ⚠️ No fuzz suite yet |
| D-03 | LSP server pinned 100% CPU on large document | `did_change` debounced ?, currently NOT, full re-parse on every keystroke. **Open issue.** | ❌ Tracked in DEBT.md |
| D-04 | Deployed contract callable in a way that exhausts gas | User responsibility per language semantics ; `covenant lint` flags unbounded loops | ✅ Linter rule L007 (V0.9.x backlog) |

### E: Elevation of privilege

| ID | Threat | Mitigation | Status |
|---|---|---|---|
| E-01 | Source compiles to bytecode that calls a helper from an unintended caller (msg.sender mismatch) | Helpers check msg.sender for ownership-sensitive functions (CeremonyHelper) | ✅ Sprint 30 |
| E-02 | Compiler injects a backdoor opcode (compromised supply chain) | Reproducible build + open source ; user diffs `covenant build --release` output against expected | ⚠️ Reproducibility documented but not CI-enforced yet |
| E-03 | `covenant init` template includes an action with `only any` (no auth) by mistake | Templates reviewed in PR ; CI runs `covenant lint` on every template after generation (V0.9.x) | ⚠️ Manual review only |
| E-04 | Mocked helper deployed on mainnet by mistake | `onlyTestnet` modifier reverts on mainnet chain ID ; compile-time `Target::parse` mainnet rejection | ✅ Sprint 30 (defense in depth) |

## Adversary model

We assume the attacker has :
  - Full source-level access to the compiler (open source).
  - Ability to write arbitrary `.cov` source.
  - Network access to call deployed helper contracts on testnet.
  - **Cannot** modify deployed helper bytecode (pinned via CREATE2).
  - **Cannot** modify mainnet chain consensus (out of scope ; mainnet
    blocked anyway).
  - **Cannot** run code on the compiler operator's machine outside of the
    compiler binary itself (compiler runs as the user ; standard OS
    sandboxing applies, no Covenant-specific guarantee beyond Rust's
    memory safety).

## Cross-cutting controls

  - **Rust memory safety.** All compiler crates compile with default
    deny on `unsafe_code` (`#![forbid(unsafe_code)]` in 21 of 22 crates ;
    one exception in WASM bindings, audited).
  - **No `panic!` on attacker-controllable input.** Lexer/parser produce
    diagnostics, never panic. ICE handler catches the rest and asks user
    to file a bug.
  - **No filesystem writes outside the explicit output path.** All
    write operations go through `--out` or default `build/` ; no
    arbitrary path emission.
  - **No network access from the compiler core.** The `build`/`check`/
    `lint`/`fmt`/`inspect` commands are 100% offline. Only `doctor`
    optionally checks RPC reachability (V0.9.x feature, not yet shipped).

## Open items (will address in V0.9.x or V1.0)

  - D-03 LSP debouncing
  - I-01 path leak audit across all subcommands
  - E-02 reproducible-build CI gate
  - E-03 lint-on-templates in CI
  - Compiler fuzz suite (cargo-fuzz)
  - SBOM emission on `--release`

## Methodology notes

This threat model was constructed bottom-up : we listed every attack
surface (file format, network protocol, IPC channel, chain interaction)
and then mapped STRIDE categories onto each. We did NOT start from
boilerplate and try to fit Covenant into it. As a result, some STRIDE
buckets are sparse (e.g. R, since most Covenant operations are on-chain
and inherently logged), that's accurate, not an oversight.

External auditors are encouraged to challenge the trust assumptions
above : if any of {1..5} is wrong, the threat model needs revision.

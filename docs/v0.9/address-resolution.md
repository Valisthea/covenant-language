# Compiler Routing & Address Resolution (V0.9)

> **Sprint** : 29 (Phases 29.3 + 29.4)
> **Status** : Design, Sprint 31 implements against it
> **Architecture** : See [precompile-bridge-architecture.md](./precompile-bridge-architecture.md), Option A (compile-time injection)
> **Interfaces** : See [helper-interfaces.md](./helper-interfaces.md)
> **Author** : Kairos Lab

This document specifies (a) how the compiler picks the right helper address
when emitting a `CALL` for a cryptographic primitive, (b) how the user
expresses target-chain choice, and (c) the format of the
`helper-addresses-v0.9.x.json` registry that drives the lookup.

---

## 1. The data flow

```
  ┌────────────────────────────┐
  │  source.cov                │  ← user writes Covenant
  └─────────────┬──────────────┘
                │
                ▼
  ┌────────────────────────────┐
  │  covenant-cli / WASM       │  reads:
  │                            │   • --target=<chain> flag
  │                            │   • OR covenant.toml → [deploy].default_target
  │                            │   • OR sane default = mockchain
  └─────────────┬──────────────┘
                │
                ▼
  ┌────────────────────────────┐
  │  PrecompileMap::for_target │  loads:
  │                            │   • config/helper-addresses-v0.9.0.json
  │                            │   • picks `targets.<chain>.helpers`
  │                            │   • returns one resolved Address per
  │                            │     primitive
  └─────────────┬──────────────┘
                │
                ▼
  ┌────────────────────────────┐
  │  covenant-codegen          │  emits:
  │                            │   PUSH20 <helper_address>
  │                            │   ... CALLDATA setup ...
  │                            │   CALL
  │                            │  for each cryptographic primitive
  └─────────────┬──────────────┘
                │
                ▼
  ┌────────────────────────────┐
  │  contract.bin              │  ← bytecode is target-specific
  └────────────────────────────┘
```

Critical property : **the compiler is the only place that resolves addresses.**
There is no on-chain registry, no constructor-arg injection, no runtime lookup.
What you compiled with is what the bytecode references for the rest of its life.

---

## 2. The `Target` type

```rust
// covenant-codegen/src/target.rs (NEW)

use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    /// In-tab EVM used by the playground. Helpers are NOT used; the
    /// compiler still emits the V0.8 precompile addresses (0x101+, etc.)
    /// because MockChain implements them in MockPrecompileState.
    MockChain,

    /// Sepolia testnet (chain id 11155111). Uses helper contracts deployed
    /// by Sprint 30 at the addresses captured in helper-addresses-v0.9.x.json.
    Sepolia,

    /// Aster Chain testnet (chain id 1996). Helper deployment coordinated
    /// by Sprint 42-43; addresses TBD until then.
    Aster,
}

impl Target {
    pub fn as_str(&self) -> &'static str {
        match self {
            Target::MockChain => "mockchain",
            Target::Sepolia => "sepolia",
            Target::Aster => "aster",
        }
    }

    pub fn chain_id(&self) -> u64 {
        match self {
            Target::MockChain => 31337,
            Target::Sepolia => 11155111,
            Target::Aster => 1996,
        }
    }

    /// The helpers JSON registry uses MockChain for builtin precompiles, but
    /// `mockchain` doesn't appear in `helpers` because the compiler routes
    /// MockChain via the V0.8 path. Returns true for targets that need an
    /// external helper lookup.
    pub fn needs_helper_lookup(&self) -> bool {
        !matches!(self, Target::MockChain)
    }
}

impl FromStr for Target {
    type Err = TargetParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "mockchain" | "mock" => Ok(Target::MockChain),
            "sepolia" => Ok(Target::Sepolia),
            "aster" | "aster_testnet" => Ok(Target::Aster),
            "mainnet" | "ethereum" => Err(TargetParseError::MainnetForbidden),
            other => Err(TargetParseError::Unknown(other.to_string())),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TargetParseError {
    #[error("V0.9 ships testnet-only. Mainnet helpers land in V1.0 \
             after external audit.")]
    MainnetForbidden,
    #[error("unknown target '{0}' (valid: mockchain, sepolia, aster)")]
    Unknown(String),
}
```

The `MainnetForbidden` error is the **compiler's** mainnet block, complementing
the helper contracts' runtime `notMainnet` modifier (see
[helper-interfaces.md §7](./helper-interfaces.md#7-mainnet-hard-revert)). Two
independent gates, defense in depth.

---

## 3. The `PrecompileMap` type

```rust
// covenant-codegen/src/precompile_map.rs (NEW)

use ethereum_types::Address;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

/// Resolved precompile addresses for a single compilation. Built once at
/// compile-start time, then threaded through codegen.
#[derive(Debug, Clone)]
pub struct PrecompileMap {
    pub target: Target,

    // Ceremony helpers
    pub ceremony_setup:        Address,
    pub ceremony_submit_share: Address,
    pub ceremony_finalize:     Address,
    pub ceremony_destroy:      Address,

    // FHE helpers (mocked in V0.9)
    pub fhe_encrypt_trivial: Address,
    pub fhe_encrypt_fresh:   Address,
    pub fhe_add:             Address,
    pub fhe_sub:             Address,
    pub fhe_mul:             Address,
    pub fhe_eq:              Address,
    pub fhe_lt:              Address,
    pub fhe_cmux:            Address,
    pub fhe_decrypt:         Address,

    // ZK helpers (mocked in V0.9)
    pub zk_verify:    Address,
    pub zk_nullifier: Address,
    pub zk_aggregate: Address,

    // PQ helpers (mocked in V0.9)
    pub pq_verify: Address,
    pub pq_keygen: Address,
    pub pq_random: Address,
}

impl PrecompileMap {
    /// Build a map for the given target. Reads helper-addresses-<version>.json
    /// for non-MockChain targets; for MockChain returns the built-in V0.8
    /// addresses (0x101+, 0x120+, 0x130+, 0x150+).
    pub fn for_target(target: Target, version: &str) -> Result<Self, PrecompileMapError> {
        if !target.needs_helper_lookup() {
            return Ok(Self::mock_chain());
        }
        let registry = HelperRegistry::load(version)?;
        let target_helpers = registry.targets.get(target.as_str())
            .ok_or(PrecompileMapError::TargetNotInRegistry {
                target, version: version.to_string(),
            })?;
        if target_helpers.helpers.is_none() {
            return Err(PrecompileMapError::TargetHelpersNotDeployed {
                target,
                note: target_helpers.note.clone().unwrap_or_default(),
            });
        }
        Self::from_registry_entry(target, target_helpers.helpers.as_ref().unwrap())
    }

    fn mock_chain() -> Self {
        // Built-in V0.8 precompile layout. Preserved for backward compat.
        Self {
            target: Target::MockChain,
            ceremony_setup:        Address::from_low_u64_be(0x120),
            ceremony_submit_share: Address::from_low_u64_be(0x121),
            ceremony_finalize:     Address::from_low_u64_be(0x122),
            ceremony_destroy:      Address::from_low_u64_be(0x123),
            fhe_encrypt_trivial:   Address::from_low_u64_be(0x101),
            fhe_encrypt_fresh:     Address::from_low_u64_be(0x102),
            fhe_add:               Address::from_low_u64_be(0x103),
            fhe_sub:               Address::from_low_u64_be(0x104),
            fhe_mul:               Address::from_low_u64_be(0x105),
            fhe_eq:                Address::from_low_u64_be(0x106),
            fhe_lt:                Address::from_low_u64_be(0x107),
            fhe_cmux:              Address::from_low_u64_be(0x108),
            fhe_decrypt:           Address::from_low_u64_be(0x10F),
            zk_verify:             Address::from_low_u64_be(0x130),
            zk_nullifier:          Address::from_low_u64_be(0x131),
            zk_aggregate:          Address::from_low_u64_be(0x132),
            pq_verify:             Address::from_low_u64_be(0x150),
            pq_keygen:             Address::from_low_u64_be(0x151),
            pq_random:             Address::from_low_u64_be(0x152),
        }
    }

    fn from_registry_entry(target: Target, h: &Helpers)
        -> Result<Self, PrecompileMapError>
    {
        // The registry stores only the four helper *contract* addresses.
        // Each contract dispatches multiple precompile selectors internally,
        // but from the compiler's point of view, every method on a given
        // helper resolves to the same contract address.
        let ceremony = h.ceremony_helper;
        let fhe      = h.fhe_helper;       // MockedFHEHelper in V0.9
        let zk       = h.zk_helper;        // MockedZKVerifier in V0.9
        let pq       = h.pq_helper;        // MockedPQVerifier in V0.9
        Ok(Self {
            target,
            ceremony_setup:        ceremony,
            ceremony_submit_share: ceremony,
            ceremony_finalize:     ceremony,
            ceremony_destroy:      ceremony,
            fhe_encrypt_trivial:   fhe,
            fhe_encrypt_fresh:     fhe,
            fhe_add:               fhe,
            fhe_sub:               fhe,
            fhe_mul:               fhe,
            fhe_eq:                fhe,
            fhe_lt:                fhe,
            fhe_cmux:              fhe,
            fhe_decrypt:           fhe,
            zk_verify:             zk,
            zk_nullifier:          zk,
            zk_aggregate:          zk,
            pq_verify:             pq,
            pq_keygen:             pq,
            pq_random:             pq,
        })
    }
}
```

### Note on selectors

V0.8 emits exactly one `CALL <addr>` per cryptographic operation, where the
address itself encodes which operation. V0.9 still emits exactly one `CALL`
per operation, but now the **selector** (first 4 bytes of calldata) carries the
"which method on the helper" information, while the **address** identifies the
helper contract.

This means `covenant-codegen` for V0.9 has slightly more work : each emit site
must (a) push the right helper address, (b) construct calldata starting with
the right selector, (c) pack arguments per Solidity ABI. Concretely, a
`CALL ceremony_destroy(sid)` call site emits :

```text
PUSH4  <selector_for_amnesiaDestroy>     ; first 4 bytes
PUSH32 <sessionId>                        ; arg
... store calldata in memory ...
PUSH20 <ceremony_helper_address>
CALL
```

The selector table in [helper-interfaces.md §5](./helper-interfaces.md#5-selector-table)
is the source of truth. Sprint 30 fills in the actual 4-byte values once the
final Solidity sources are committed; Sprint 31 reads them from a constant
map in `covenant-codegen/src/selectors.rs`.

---

## 4. The `helper-addresses-v0.9.x.json` schema

Lives at `config/helper-addresses-v0.9.0.json` in this repo. Each compiler
release ships its own version of this file. The file is **typed** by JSON Schema
(`config/helper-addresses.schema.json`) so editors and CI can validate it.

### 4.1 Top-level structure

```json
{
  "$schema": "./helper-addresses.schema.json",
  "version": "0.9.0",
  "release_date": "2026-XX-XX",
  "compiler_version_required": "^0.9.0",
  "targets": {
    "sepolia":   { ... },
    "aster":     { ... },
    "mainnet":   { ... }
  }
}
```

- `version`: the registry's own version. Compiler refuses a registry with a
  major.minor mismatch (`0.9.x` registry only loadable by `0.9.x` compiler).
- `compiler_version_required`: semver range. Belt + suspenders for the
  version field.
- `release_date`: ISO 8601 date the helpers were deployed. Informational.

### 4.2 Per-target structure

```json
"sepolia": {
  "chain_id": 11155111,
  "deployed_at_block": 12345678,
  "deployer_address": "0x1234...",
  "helpers": {
    "ceremony_helper": "0xABCD...",
    "fhe_helper":      "0xDEF0...",   ← MockedFHEHelper at this address
    "pq_helper":       "0x0123...",   ← MockedPQVerifier
    "zk_helper":       "0x4567..."    ← MockedZKVerifier
  },
  "selectors": {
    "amnesiaSetup":         "0x4d6f4a8b",
    "amnesiaSubmitShare":   "0x...",
    "amnesiaFinalize":      "0x...",
    "amnesiaDestroy":       "0x...",
    "encryptTrivial":       "0x...",
    "encryptFresh":         "0x...",
    "add":                  "0x...",
    "sub":                  "0x...",
    "mul":                  "0x...",
    "eq":                   "0x...",
    "lt":                   "0x...",
    "cmux":                 "0x...",
    "decrypt":              "0x...",
    "verify":               "0x...",
    "nullifier":            "0x...",
    "proofAggregate":       "0x...",
    "pqVerify":             "0x...",
    "pqKeygenFromSeed":     "0x...",
    "pqRandom":             "0x..."
  },
  "verification": {
    "etherscan_verified": true,
    "code_hashes": {
      "ceremony_helper": "0x...",
      "fhe_helper":      "0x...",
      "pq_helper":       "0x...",
      "zk_helper":       "0x..."
    }
  }
}
```

The `selectors` block is shared across all targets (the Solidity ABI doesn't
depend on chain), but it lives under each target so :

- A future migration that changes a helper interface on one chain only doesn't
  silently break others.
- The compiler reads `targets.<x>.selectors.<method>` and gets back a
  guaranteed-correct selector for that target's deployed helper.

The `code_hashes` block lets the compiler (or a verifier tool) confirm at
compile-start that the helpers on chain are the *exact* code expected. If the
helper at the registry address has a different code hash, the compiler refuses
to emit the bytecode (deferred to Sprint 32 to wire up; for V0.9.0 first ship,
the check is opt-in).

### 4.3 Mainnet block

```json
"mainnet": {
  "chain_id": 1,
  "helpers": null,
  "note": "V0.9 ships testnet-only. Mainnet helpers land in V1.0 \
           post-external-audit. Compiler refuses --target=mainnet."
}
```

The `helpers: null` value is the runtime signal that this target is not
deployable. Combined with the `Target::from_str("mainnet")` early-rejection,
mainnet is unreachable from any path.

### 4.4 Aster placeholder

```json
"aster": {
  "chain_id": 1996,
  "helpers": null,
  "note": "Helper deployment coordinated by Sprint 42-43. Track progress \
           in covenant/docs/v0.9/aster-integration-status.md."
}
```

Sprint 42 fills this in once Aster Testnet helpers are deployed.

### 4.5 Loader code

```rust
// covenant-codegen/src/registry.rs (NEW)

#[derive(Debug, Deserialize)]
pub struct HelperRegistry {
    pub version: String,
    pub release_date: String,
    pub compiler_version_required: String,
    pub targets: HashMap<String, TargetEntry>,
}

#[derive(Debug, Deserialize)]
pub struct TargetEntry {
    pub chain_id: u64,
    pub deployed_at_block: Option<u64>,
    pub deployer_address: Option<String>,
    pub helpers: Option<Helpers>,
    pub selectors: Option<HashMap<String, String>>,
    pub verification: Option<Verification>,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Copy)]
pub struct Helpers {
    pub ceremony_helper: Address,
    pub fhe_helper: Address,
    pub pq_helper: Address,
    pub zk_helper: Address,
}

#[derive(Debug, Deserialize)]
pub struct Verification {
    pub etherscan_verified: bool,
    pub code_hashes: HashMap<String, String>,
}

impl HelperRegistry {
    pub fn load(version: &str) -> Result<Self, RegistryError> {
        // Resolve the path: bundled file from compiler binary's known location
        let path = Self::registry_path(version);
        let bytes = std::fs::read(&path)
            .map_err(|e| RegistryError::Read { path: path.clone(), source: e })?;
        let reg: HelperRegistry = serde_json::from_slice(&bytes)
            .map_err(|e| RegistryError::Parse { path, source: e })?;

        // Sanity-check that the registry's version matches what we asked for
        if !reg.version.starts_with(version) {
            return Err(RegistryError::VersionMismatch {
                expected: version.to_string(),
                got: reg.version,
            });
        }
        Ok(reg)
    }

    fn registry_path(version: &str) -> PathBuf {
        // Search order:
        //   1. CARGO_MANIFEST_DIR/../../config/helper-addresses-<version>.json
        //      (workspace dev mode)
        //   2. <exe_dir>/config/helper-addresses-<version>.json
        //      (installed binary)
        //   3. $COVENANT_HELPER_REGISTRY env var (CI / tests / overrides)
        // ... implementation in Sprint 31
        unimplemented!()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("could not read registry at {path:?}: {source}")]
    Read { path: PathBuf, source: std::io::Error },
    #[error("could not parse registry at {path:?}: {source}")]
    Parse { path: PathBuf, source: serde_json::Error },
    #[error("registry version mismatch: expected {expected}, got {got}")]
    VersionMismatch { expected: String, got: String },
}
```

---

## 5. The `covenant.toml` user surface

End users don't touch `helper-addresses-*.json` directly. They configure target
in `covenant.toml` :

```toml
# covenant.toml: project manifest

[package]
name = "my-contract"
version = "0.1.0"
covenant_version = "0.9.0"

[deploy]
default_target = "sepolia"
# Other valid: "mockchain", "aster"
# Forbidden: "mainnet" (compile-time rejected)

[targets.sepolia]
# Optional override. By default the compiler reads
# config/helper-addresses-v0.9.0.json bundled with the compiler binary.
# Power users can point at their own registry (e.g. forked helpers for
# integration testing).
helper_registry = "default"  # or "/path/to/custom-helpers.json"

[targets.aster]
helper_registry = "default"
```

The CLI accepts `--target` to override `default_target` :

```bash
# Use the project default (sepolia)
covenant build src/main.cov

# Override per-call
covenant build src/main.cov --target=mockchain

# Refused at parse time
covenant build src/main.cov --target=mainnet
# error: V0.9 ships testnet-only. Mainnet helpers land in V1.0
# after external audit.
```

In the playground, the Chain Target dropdown writes `--target=<choice>` into
the WASM `compile_to_evm(source, target)` invocation; no `covenant.toml` lives
in-browser.

---

## 6. Backward compatibility with V0.8

V0.8 contracts compiled before this sprint exists used hardcoded `0x101+` etc.
Those contracts are :

- Deployed on **MockChain** : keep working unchanged. V0.9 MockChain target
  emits the same addresses.
- Deployed on **Sepolia** : already broken (KSR-CVN-005, calls to empty
  addresses). V0.9 fixes this for *new* compiles only. Old V0.8-compiled
  Sepolia contracts stay broken; users have to rebuild and redeploy.

Migration note for the V0.9 release notes :

> "If you deployed Covenant V0.8 contracts to Sepolia, the cryptographic
> operations (ceremony, FHE counters, PQ signatures, ZK proofs) silently
> failed. V0.9 fixes this by routing those operations to deployed helper
> contracts. To get the fix on an existing contract, recompile against V0.9
> and redeploy."

---

## 7. Testing strategy for Sprint 31

Three test layers :

1. **Unit**: `PrecompileMap::for_target(MockChain, "0.9.0")` returns the V0.8
   layout exactly. Existing fixture tests pass unchanged.
2. **Integration**: `PrecompileMap::for_target(Sepolia, "0.9.0")` loads the
   committed JSON, every field is populated with the address-shaped value
   from the registry. No address is `0x0`.
3. **End-to-end** (Sprint 32), compile a fixture for `--target=sepolia`,
   inspect the bytecode, confirm the embedded address matches the JSON.

The unit + integration tests live in `covenant-codegen/src/` next to the
implementation. The E2E test belongs in Sprint 32's verification harness.

---

## 8. Open questions for Sprint 31 to resolve

1. **Bundling the registry.** Should `helper-addresses-v0.9.0.json` be
   `include_str!`'d into the compiler binary or read from disk? Disk is
   easier to update per-release; `include_str!` removes a runtime dep. Sprint
   31 should pick, recommendation : `include_str!` for the default registry,
   disk read for the `helper_registry = "/path/..."` override.
2. **Per-version path search.** Sprint 31 must implement the 3-step path
   resolution in `HelperRegistry::registry_path` (see §4.5). Decide the
   precedence (env var first? bundled first?).
3. **Code-hash check enforcement.** Should the compiler abort if
   `verification.code_hashes` mismatch the on-chain code? Sprint 32 wires this
   up; Sprint 31 should at least add a `compile_options.verify_helper_code:
   bool` flag, default false for V0.9.0.

---

## 9. What this document does NOT decide

- The actual addresses (Sprint 30 deploys, captures)
- The actual selectors (Sprint 30 captures from final Solidity ABIs)
- The CREATE2 salts used for deterministic deploy (Sprint 30 picks)
- Aster integration timing (Sprint 42-43)
- Bytecode reproducible-build verification UX (V1.0)

---

## 10. Sign-off

This is the architectural commitment for Sprint 31's compiler refactor and
Sprint 30's JSON registry shape.

| Role | Reviewer | Status |
|---|---|---|
| Architect | Kairos Lab | ✅ Decided |
| Sprint 30 lead | Kairos Lab | reads §4 schema before deploy |
| Sprint 31 lead | Kairos Lab | implements `Target` + `PrecompileMap` per §2-3 |
| Sprint 32 lead | Kairos Lab | wires verification per §7 layer 3 |

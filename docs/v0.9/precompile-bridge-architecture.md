# Precompile Bridge Architecture (V0.9)

> **Sprint** : 29 (Phase 29.1)
> **Status** : Design, no code yet, this document drives Sprints 30 to 32
> **Decision** : **Option A, compile-time injection** (rationale below)
> **Author** : Kairos Lab
> **Date** : V0.8 GA + 1 day

---

## 1. The problem this document solves

V0.8 ships a working compiler that emits EVM bytecode for every Covenant
construct, including the cryptographic ones (`ceremony`, `encrypted counter`,
`pq_signed`, `verified_by`). On MockChain (the in-tab EVM the playground uses)
these constructs work end-to-end because MockChain implements the precompile
addresses `0x101`, `0x10F` (FHE), `0x120`, `0x123` (amnesia), `0x130`, `0x133` (ZK),
and `0x150`, `0x154` (PQ) inside `MockPrecompileState`.

On Sepolia, those addresses are empty. A `CALL 0x123` (ceremony destroy)
**succeeds** with zero return data, there's no code at the address, and the
calling contract proceeds as if the precompile returned a valid result. The
behavior is silently incorrect : the ceremony believes it was destroyed; it
wasn't. Audit finding **KSR-CVN-005** (V0.8 OMEGA V4) tracks this.

V0.9 must close the gap. The decision in front of us is *how* the compiler
addresses the precompile. Three options were considered.

---

## 2. The three options

### Option A: Compile-time injection

The compiler accepts a `--target=<chain>` flag (or reads `[deploy]
default_target` from `covenant.toml`). At codegen time, the compiler embeds a
chain-specific helper-contract address directly into the bytecode :

```rust
// pseudo-IR
let dest = match config.target {
    Target::MockChain => Address(0x123),           // existing precompile
    Target::Sepolia   => SEPOLIA_CEREMONY_HELPER,  // 0xABCD…
    Target::Aster     => ASTER_CEREMONY_HELPER,    // 0xEF01…
};
emit_call(dest);
```

Different bytecode per target. Rebuilding for a new target is one CLI command.

**Pros**
- Smallest gas overhead, exactly the same as V0.8 (one `CALL`)
- Smallest audit surface, no runtime dispatch logic, no registry lookup
- Easiest to reason about in the OMEGA V5 audit (Sprint 46)
- Compiler stays the source of truth; no on-chain helper lookup table

**Cons**
- Different bytecode per target → if a contract is deployed to two targets, the
  source-bytecode mapping is one-to-many
- Code-coverage / testing artifacts don't carry across targets cleanly
- Per-version helper redeploy means per-version compiler rebuild for downstream
  users (mitigated by versioned `helper-addresses-v0.9.0.json`)

### Option B: Per-deploy registry

A `HelperRegistry` contract is deployed once per chain. At deploy-time, every
Covenant contract reads its precompile addresses from the registry into local
storage (or accepts them as constructor args). Subsequent calls go through an
indirect lookup :

```solidity
mapping(bytes4 => address) helpers;
function _call_ceremony_destroy(uint256 sid) internal {
    helpers[CEREMONY_DESTROY].call(...);
}
```

Same bytecode on every chain.

**Pros**
- One bytecode per source, clean reproducible-build property
- Bug-fix redeploys don't require recompilation : just upgrade the registry
- Multi-chain deployment is conceptually trivial

**Cons**
- ~250 gas per call overhead (`SLOAD` + indirect `CALL`)
- Registry contract is critical infrastructure : if it has a bug, every contract
  on chain is affected. Audit cost goes up sharply
- Registry upgrade authority becomes a centralization vector
- Storage growth in every Covenant contract for the helpers mapping

### Option C, Hybrid : compile-time hint + runtime fallback

Bytecode embeds a primary helper address (compile-time). If the call returns an
unexpected sentinel (e.g. zero), the runtime falls back to a registry lookup.

**Pros**
- Optimal performance in the common path
- Fallback mechanism for emergency redeploy

**Cons**
- Highest implementation complexity
- Hardest to audit, two paths, both must be correct
- The fallback is essentially Option B with extra steps
- Doesn't fit the V0.9 timeline

---

## 3. Decision matrix

Scored 1 to 5 (5 = best fit), summed.

| Criterion (weight) | A (compile-time) | B (registry) | C (hybrid) |
|---|---|---|---|
| Implementation complexity (×3) | **5** = 15 | 3 = 9 | 1 = 3 |
| Gas overhead per call (×2) | **5** = 10 | 3 = 6 | 4 = 8 |
| Audit complexity for OMEGA V5 (×3) | **5** = 15 | 3 = 9 | 1 = 3 |
| Multi-chain ergonomics (×1) | 2 = 2 | **5** = 5 | 4 = 4 |
| Fits V0.9 16-week timeline (×3) | **5** = 15 | 4 = 12 | 1 = 3 |
| Centralization risk (×2) | **5** = 10 | 2 = 4 | 3 = 6 |
| Bug-fix flexibility (×1) | 2 = 2 | **5** = 5 | 4 = 4 |
| **Total** | **69** | 50 | 31 |

**Decision** : **Option A** for V0.9.0.

Option A wins on every weighted criterion that matters for V0.9 : ship-on-time,
audit-friendly, and gas-optimal. The "one bytecode per target" is a real cost
but it's manageable :

- The `helper-addresses-v0.9.x.json` registry is committed to the compiler repo
  and versioned alongside compiler releases. Downstream users never edit it by
  hand.
- The playground UI handles the per-target rebuild transparently, switching the
  Chain Target dropdown re-runs `compile_to_evm(source, target)` in WASM.
- For projects that need true cross-chain bytecode reproducibility, a future
  V1.0 can add Option B as an opt-in. Nothing in Option A precludes layering B
  on top later.

---

## 4. Concrete consequences of choosing A

The downstream sprints assume Option A :

- **Sprint 30** writes four standalone Solidity helper contracts that match the
  precompile interface exactly. Each helper is an independent contract, there
  is no umbrella registry contract.
- **Sprint 30** deploys each helper via CREATE2 with a deterministic salt. The
  resulting addresses are captured in `helper-addresses-v0.9.0.json`.
- **Sprint 31** refactors `covenant-codegen` to thread a `PrecompileMap` value
  through every emit site. The map is constructed from the JSON registry at
  compile-start time.
- **Sprint 32** verifies end-to-end on Sepolia : every fixture deploys and
  exercises every cryptographic primitive against the deployed helpers.

Option A also implies :

- **Mainnet ships post-audit only**. V0.9.0 helper addresses are Sepolia +
  Aster Testnet. Mainnet helpers are deferred to V1.0 with external audit
  approval. The compiler refuses `--target=mainnet` until those addresses
  exist.
- **Per-version helper rebuild** : if a helper bug is found in V0.9.0,
  V0.9.1 redeploys the affected helper at a *new* CREATE2 salt
  (`covenant-v0.9.1-ceremony` instead of `covenant-v0.9.0-ceremony`),
  publishes new addresses in `helper-addresses-v0.9.1.json`, and the
  compiler bumps to read v0.9.1 by default. Old V0.9.0 contracts continue
  to call old helpers, no forced upgrade.

---

## 5. What this document does NOT decide

Out of scope for Sprint 29 :

- The exact CREATE2 salts (Sprint 30 picks them, captures in JSON)
- The wire-format of helper interfaces (Sprint 29 Phase 29.2,
  see [helper-interfaces.md](./helper-interfaces.md))
- The compiler routing implementation (Sprint 29 Phase 29.3,
  see [address-resolution.md](./address-resolution.md))
- Aster Chain helper deployment (Sprint 42 to 43)
- Mainnet deployment policy (V1.0, pending external audit)

---

## 6. Things to revisit if Option A breaks down

Triggers that would force a re-evaluation toward Option B :

- A partner needs the *same* contract address on multiple chains (e.g. for a
  cross-chain identity). Option A makes this impossible because bytecode
  differs per target.
- The OMEGA V5 audit (Sprint 46) finds that per-target bytecode complicates
  reproducible-build verification beyond what we tolerate.
- A user deploys to Sepolia, deploys to Aster, and discovers their two
  contracts behave differently because of helper discrepancies, a Class C
  user-experience bug.

If any of these surface mid-V0.9, this document gets a v1.1 with Option B
re-considered. Probability low, but documented.

---

## 7. Sign-off

This document is the architectural commitment for V0.9 Phase A.1.
Sprints 30 to 32 implement against it. If you change your mind on Option A,
do it *here*, not silently in a sprint downstream.

| Role | Reviewer | Status |
|---|---|---|
| Architect | Kairos Lab | ✅ Decided |
| Auditor (V0.8 OMEGA V4) | self | Reviewed during Sprint 29 design |
| External audit pre-check | (deferred to Sprint 44) |, |

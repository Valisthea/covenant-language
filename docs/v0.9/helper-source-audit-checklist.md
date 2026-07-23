# Helper Source Audit Checklist & V0.9 → V1.0 Path

> **Sprint** : 30 (Phase 30.7 deliverable)
> **Status** : Living checklist — tracks each Mocked* helper's V1.0 swap-in plan
> **Owner** : Kairos Lab
> **Audit reference** : OMEGA V5 (Sprint 46) and external audit (V1.0)

This document is the contract for what each helper **promises** vs what it
**delivers** in V0.9, and the path each takes to become production-grade
in V1.0. Every Mocked* helper has an entry here. If a helper's behavior
changes between V0.9 minor releases, append a row.

---

## Helper-by-helper status

### 1. CeremonyHelper

| Aspect | V0.9 | V1.0 path |
|---|---|---|
| State machine (Setup→Active→Finalized→Destroyed) | **Real** ✅ | unchanged |
| Phase transitions monotonic | **Real** ✅ | unchanged |
| Per-ceremony isolation (msg.sender check) | **Real** ✅ | unchanged |
| Share collection + count | **Real** ✅ | unchanged |
| Threshold check | **Real** ✅ | unchanged |
| Wesolowski VDF | ❌ Stand-in (keccak commitment) | Add real VDF circuit |
| Shamir secret reconstruction | ❌ Not implemented | Add reconstruction proof |
| Destruction proof verifiability off-chain | Partial (commitment only) | Full VDF + Shamir verifier |
| Storage wipe of shares | **Real** ✅ | unchanged |

**Audit-clean checklist** (Sprint 30 + OMEGA V5):
- [x] No `selfdestruct`
- [x] No `delegatecall`
- [x] No assembly except where explicitly needed
- [x] All errors are typed custom (no plain string reverts)
- [x] All state mutations emit events
- [x] Reentrancy: not exploitable (no external calls during state changes)
- [ ] Slither clean (run in Sprint 30 Phase 30.5)
- [ ] Mythril clean (run in Sprint 30 Phase 30.5)

**V1.0 swap-in plan**:
1. Add `WesolowskiVDF` library that accepts a (sessionId, shares, time) tuple and produces a verifiable VDF output
2. Modify `amnesiaDestroy` to compute the VDF, embed in destructionProof
3. Publish off-chain Python verifier (`tools/verify_destruction_proof_v1.py`) that decodes the new proof shape
4. Update destructionProof ABI to be `abi.encode(uint256 sessionId, bytes32 commitment, bytes32 vdfOutput, uint256 vdfIterations)` — backward-compatible decoders read first 64 bytes only
5. Bump `helper-addresses-v1.0.0.json` and re-deploy via CREATE2

---

### 2. MockedFHEHelper → V1.0 FHEHelper

| Aspect | V0.9 (Mocked) | V1.0 (Real) |
|---|---|---|
| Encrypt | Plaintext stored under `bytes32` handle | Zama TFHE ciphertext |
| Add/Sub/Mul | Plaintext arithmetic, re-handle | Homomorphic operations |
| Eq/Lt | Plaintext compare, encrypted result | Encrypted comparison |
| Cmux | Plaintext branch, re-handle | Encrypted multiplexer |
| Decrypt | Returns plaintext (open access) | Threshold decrypt with access policy |
| Mainnet block | **Real** (`notMainnet` modifier) | Removed in V1.0 (real FHE on mainnet) |
| Gas cost per op | <30k | 1M-5M (real TFHE) |

**Why mocked in V0.9**: Real Zama TFHE on Sepolia today costs >2M gas per `add`,
which makes the playground UX unusable. The interface is correct so V1.0 swap is
contract-replacement, not interface-redesign.

**V1.0 swap-in plan**:
1. Wait for Zama fhEVM mainnet-grade pricing (target: <500k gas per `add`)
2. Implement `FHEHelper` (no `Mocked` prefix) that delegates to fhEVM precompiles
3. Same external selectors as `MockedFHEHelper` so existing Covenant bytecode
   continues to work — only the address changes
4. Deploy V1.0 helpers at NEW CREATE2 salts (`covenant-v1.0.0-fhe`)
5. Old V0.9 contracts continue to call `MockedFHEHelper`; new V1.0 compiles call
   `FHEHelper`. No forced migration.

**Risk**: V1.0 cannot ship until Zama fhEVM has acceptable gas cost. If that
doesn't happen, V1.0 keeps `MockedFHEHelper` and FHE remains demo-only.
Documented as R3 in V0.9 master plan §0.4.

---

### 3. MockedPQVerifier → V1.0 PQVerifier

| Aspect | V0.9 (Mocked) | V1.0 (Real) |
|---|---|---|
| `pqVerify` | Length checks + parity (NOT cryptographic) | Real Dilithium-5 verifier |
| `pqKeygenFromSeed` | Deterministic stub | Real Dilithium keygen (off-chain typically) |
| `pqRandom` | block.prevrandao + nonce hash | VRF or PQ-PRG |
| Mainnet block | **Real** | Removed once verifier is real |
| Gas cost | <30k (mocked) | 150-300k (real Dilithium) |

**Library candidates for V1.0 Dilithium verifier**:
- Solady PQ (in development as of V0.8 release)
- PQClean Solidity port (research-grade)
- Custom verifier following FIPS 204 (highest engineering cost, fewest deps)

**V1.0 swap-in plan**:
1. Pick library by V1.0 RC1
2. Implement `PQVerifier` (no `Mocked` prefix) that wraps the chosen library
3. Same external selectors as `MockedPQVerifier`
4. Deploy at new CREATE2 salts
5. Update `helper-addresses-v1.0.0.json`

**Test strategy for V1.0**: cross-verify against known-good Dilithium-5 test
vectors (FIPS 204 Appendix A). If V1.0 verifier passes the FIPS test vectors,
it's accepted. If not, V1.0 ships without real PQ verify and the gap is
documented.

---

### 4. MockedZKVerifier → V1.0 ZKVerifier

| Aspect | V0.9 (Mocked) | V1.0 (Real) |
|---|---|---|
| `verify` (Halo2) | Length floor + parity (NOT cryptographic) | Real Halo2 verifier |
| `nullifier` | **Real** ✅ (keccak of "nullifier"+secret) | unchanged |
| `proofAggregate` (Nova IVC) | Stub blob | Real Nova fold |
| Mainnet block | **Real** for `verify`; not needed for `nullifier` | Same |
| Gas cost | <50k (mocked verify) | 250-400k (real Halo2) |

**Note**: `nullifier` is real even in V0.9. Suitable for double-spend
prevention TODAY because the nullifier doesn't depend on proof verification —
it depends only on the secret being unique. So a Covenant contract that uses
`nullifier(secret)` to prevent double-spend works in V0.9 as long as the secret
is generated correctly off-chain.

**V1.0 swap-in plan**:
1. Adopt an open-source Halo2 verifier in Solidity (PSE / Scroll have variants)
2. Implement `ZKVerifier` that wraps it
3. Same external selectors
4. Deploy at new CREATE2 salts

**Aggregation (`proofAggregate`)**: lower priority than `verify`. May ship as
stub even in V1.0 if Nova IVC tooling on EVM isn't mature. Documented as
deferred if so.

---

## Cross-cutting V0.9 → V1.0 changes

These apply to all Mocked* helpers when they swap in V1.0:

1. **Remove `notMainnet` modifier** — once verified by external audit, the
   helpers ARE production-grade and mainnet-deployable. The modifier was
   defense-in-depth for the V0.9 placeholder period.
2. **Drop `Mocked*` prefix from contract names** — `FHEHelper`, `PQVerifier`,
   `ZKVerifier` (no prefix). Same external interface = drop-in swap for the
   compiler routing layer.
3. **Drop `_status: "PREDICTED"`** field from `helper-addresses-v1.0.0.json`
   once V1.0 deployment is live.
4. **Add `external_audit` block** to each target entry pointing to the audit
   firm + report URL.
5. **Drop `MockedHelperUsed` event** — production helpers don't emit it. Old
   V0.9 contracts still emit it because they call old helpers; that's expected.

---

## Sprint 30 sign-off

When Sprint 30 is complete (Phase 30.6 verify):

- [x] All 4 helpers compile clean (`forge build` warnings only on test files)
- [x] All 34 unit tests pass (`forge test`)
- [x] CREATE2 salts + init code hashes captured in `helper-addresses-v0.9.0.json._notes`
- [x] Predicted addresses calculated via Arachnid CREATE2 factory
- [ ] Slither runs clean on each helper (Sprint 30 phase 30.5 follow-up)
- [ ] Mythril runs clean on each helper (Sprint 30 phase 30.5 follow-up)
- [ ] Sepolia deployment succeeds at predicted addresses (Sprint 30 phase 30.5 — needs operator)
- [ ] Etherscan verification succeeds for all 4 (Sprint 30 phase 30.6 — needs operator)
- [ ] One end-to-end manual test against deployed CeremonyHelper (Sprint 30 §8 acceptance)

The unchecked items require credentials (Sepolia RPC, deployer key, Etherscan
API key) and a Slither install. Operator follow-up.

---

## When V1.0 starts

This document gets a v2 in Sprint 44 (External Audit Preparation Phase 1).
The v2 lists every change between V0.9 and V1.0 in detail, makes the V1.0
audit firm's job mechanical: read this doc, verify each row, sign off.

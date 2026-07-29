# Helper Contract Interfaces (V0.9)

> **Sprint** : 29 (Phase 29.2)
> **Status** : Design, interfaces frozen for Sprint 30 implementation
> **Architecture** : See [precompile-bridge-architecture.md](./precompile-bridge-architecture.md), Option A
> **Author** : Kairos Lab

This document specifies the **external interface** of the four helper contracts
that V0.9 introduces. The interfaces are frozen, Sprint 30 implements
them as written here. Any Sprint 30 deviation must come back and amend this
document, not silently diverge.

---

## 0. Naming convention: read this first

The Sprint 30 spec proposed names like `FHEHelper`, `PQHelper`, `ZKHelper`.
**Sprint 29 changes these to make the trust boundary explicit at the contract
name itself** :

| Sprint 30 spec | Sprint 29 frozen name | Why |
|---|---|---|
| `CeremonyHelper` | `CeremonyHelper` | Real implementation; name unchanged |
| `FHEHelper` | **`MockedFHEHelper`** | V0.9 stores plaintext; the name must telegraph it |
| `PQHelper` | **`MockedPQVerifier`** | V0.9 verify is a parity check, not Dilithium; name must telegraph it |
| `ZKHelper` | **`MockedZKVerifier`** | V0.9 verify is a parity check, not Halo2; name must telegraph it |

This naming is **load-bearing** for risk mitigation. The OMEGA V4 audit found
PRELIM-009 (broken examples shipped as if they worked); the same class of bug
on the mainnet helpers would be much worse. A user reading
`MockedFHEHelper.add(a, b)` on Etherscan cannot honestly mistake it for a
production-grade FHE primitive. A user reading `FHEHelper.add(a, b)` *can*.

The non-mocked `CeremonyHelper` keeps its plain name because its logic is
real (state machine, share collection, destruction proof emission), the
underlying VDF in V0.9 is a keccak-bound stand-in for Wesolowski, which is
honest about being "destruction commitment" without being "Wesolowski VDF" in
the name.

In addition to the rename, every mocked helper :

1. Has a NatSpec `@notice` block at the top of the contract that screams
   "MOCKED, NOT FOR PRODUCTION SECRETS" (verbatim wording in §6 below)
2. Reverts hard if `block.chainid == 1` (Ethereum mainnet), see §7
3. Emits an `MockedHelperUsed(bytes4 selector, address caller)` event on every
   call so off-chain observers can flag mainnet-equivalent usage in dashboards

---

## 1. CeremonyHelper (real)

**Replaces** : precompiles `0x120` (setup), `0x121` (submit_share),
`0x122` (finalize), `0x123` (destroy).

**Storage model** : one `Session` struct per session_id. Sessions are indexed
both by id and by the calling Covenant ceremony contract (forward and reverse
maps), so a ceremony contract can enumerate its own sessions for migration or
recovery scenarios.

```solidity
// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.20;

interface ICeremonyHelper {
    enum Phase { Setup, Active, Finalized, Destroyed }

    // ─── State-changing (called from Covenant ceremony contracts) ──────

    /// @notice Allocate a new ceremony session for the calling contract.
    /// @param nonce  Caller-supplied entropy; combined with internal counter
    ///               + msg.sender to produce a unique session_id.
    /// @param guardiansCount  Total number of guardians who will hold shares.
    /// @param threshold       Minimum shares required to reconstruct.
    /// @return sessionId      The new session's identifier.
    /// @dev Initial phase is Active immediately after Setup completes. Setup
    ///      is one logical transition.
    function amnesiaSetup(
        uint256 nonce,
        uint256 guardiansCount,
        uint256 threshold
    ) external returns (uint256 sessionId);

    /// @notice A guardian (via the ceremony) submits one share.
    /// @param sessionId  Active session id.
    /// @param share      The guardian's Shamir share (32 bytes).
    /// @return ok        True on successful append.
    /// @dev tx.origin is recorded as the guardian; msg.sender is the
    ///      ceremony contract. This binds the guardian identity to the
    ///      original transaction signer.
    function amnesiaSubmitShare(
        uint256 sessionId,
        bytes32 share
    ) external returns (bool ok);

    /// @notice Close share collection. Returns whether enough shares to
    ///         reconstruct were submitted.
    /// @dev Phase moves Active → Finalized. Reversion if not Active.
    function amnesiaFinalize(uint256 sessionId)
        external returns (bool thresholdMet);

    /// @notice Irrevocably destroy the secret. Wipes shares from storage,
    ///         emits the destruction proof event.
    /// @dev Phase moves Finalized → Destroyed. The returned bytes are the
    ///      destruction proof: ABI-encoded (sessionId, keccak commitment).
    /// @dev V0.9 destruction proof = keccak256(abi.encodePacked(shares,
    ///      sessionId)). V1.0 will add a Wesolowski VDF stage; the proof
    ///      schema is forward-compatible (event topic + abi.encode means
    ///      future proofs append fields without breaking decoders).
    function amnesiaDestroy(uint256 sessionId)
        external returns (bytes memory destructionProof);

    // ─── Read-only (for Covenant `view` and `reveal` constructs) ───────

    function phase(uint256 sessionId) external view returns (uint8);
    function isDestroyed(uint256 sessionId) external view returns (bool);
    function getDestructionProof(uint256 sessionId)
        external view returns (bytes memory);
    function sessionsByCeremony(address ceremony, uint256 idx)
        external view returns (uint256 sessionId);
    function sessionCount(address ceremony) external view returns (uint256);

    // ─── Events ─────────────────────────────────────────────────────────

    event AmnesiaSetup(
        uint256 indexed sessionId,
        address indexed ceremony,
        uint256 guardiansCount,
        uint256 threshold
    );
    event AmnesiaShareSubmitted(
        uint256 indexed sessionId,
        address indexed guardian
    );
    event AmnesiaFinalized(
        uint256 indexed sessionId,
        bool thresholdMet
    );
    event AmnesiaDestroyed(
        uint256 indexed sessionId,
        bytes destructionProof
    );

    // ─── Errors ─────────────────────────────────────────────────────────

    error InvalidSession();
    error InvalidPhase(uint8 currentPhase, uint8 requiredPhase);
    error UnauthorizedCaller(address expected, address actual);
}
```

**Security invariants** (Sprint 46 audit must verify each) :

1. Phase transitions are strictly monotonic : Setup → Active → Finalized → Destroyed.
2. Once `phase == Destroyed`, no state-changing method can succeed against that
   session.
3. `share` storage is wiped via `delete` on `amnesiaDestroy` (gas-refund + privacy).
4. Only the original calling ceremony contract (via `msg.sender == ceremony`)
   can transition its own session.
5. The `destructionProof` returned/event-emitted is deterministic given the same
   inputs (no oracle, no randomness in the proof itself).
6. `amnesiaSetup` is reentrancy-safe : it sets phase to Active *after* recording
   the session, no external calls during state setup.

**Gas budget** (target, Sprint 30 must verify with Foundry gas reports) :

| Op | Gas target | Acceptable ceiling |
|---|---|---|
| amnesiaSetup | 100k | 200k |
| amnesiaSubmitShare | 50k | 80k |
| amnesiaFinalize | 30k | 50k |
| amnesiaDestroy | 80k | 150k |
| phase / isDestroyed (view) | < 5k | 10k |

If Sprint 30 measurement exceeds the ceiling, the implementation gets reviewed
before deployment. Per R1 in the V0.9 risk register : helper gas blowup is the
Phase A.1 risk.

---

## 2. MockedFHEHelper (mocked)

**Replaces** : precompiles `0x101`, `0x10F`.

**V0.9 reality** : not real FHE. Each "ciphertext handle" is a `bytes32` keyed
into a `mapping(bytes32 => uint256)` that stores the actual plaintext.
"Homomorphic" operations are applied to the plaintexts and re-stored under a
new handle. Anyone reading chain state can recover plaintexts.

**This is documented at every layer** :

- Contract name : `MockedFHEHelper` (not `FHEHelper`)
- NatSpec banner (§6)
- Mainnet revert (§7)
- `MockedHelperUsed` event on every call
- Playground UI banner on every example that uses encrypted constructs
- Documentation site banner on every related doc page

```solidity
// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.20;

interface IMockedFHEHelper {
    /// @notice Encrypt a plaintext value. V0.9 = trivial (deterministic) encryption.
    /// @return handle A bytes32 ciphertext identifier. The plaintext is stored
    ///                under this key in the helper's storage.
    function encryptTrivial(uint256 plaintext) external returns (bytes32 handle);

    /// @notice Encrypt with caller-supplied randomness. V0.9 = same as trivial
    ///         but the handle includes the random nonce.
    function encryptFresh(uint256 plaintext, uint256 randomNonce)
        external returns (bytes32 handle);

    /// @notice Add two encrypted values, return a new encrypted result.
    function add(bytes32 a, bytes32 b) external returns (bytes32 result);
    function sub(bytes32 a, bytes32 b) external returns (bytes32 result);
    function mul(bytes32 a, bytes32 b) external returns (bytes32 result);

    /// @notice Encrypted comparisons. Result is a new ciphertext with
    ///         plaintext 1 (true) or 0 (false).
    function eq(bytes32 a, bytes32 b) external returns (bytes32 result);
    function lt(bytes32 a, bytes32 b) external returns (bytes32 result);

    /// @notice Encrypted multiplexer: returns ifTrue when cond != 0, else ifFalse.
    function cmux(bytes32 cond, bytes32 ifTrue, bytes32 ifFalse)
        external returns (bytes32 result);

    /// @notice Decrypt a handle. V0.9 = open access (no policy enforcement).
    /// @dev Access policies (`reveal X to Y`) are enforced at compile time
    ///      by the Covenant compiler, NOT here. The helper trusts the caller.
    function decrypt(bytes32 handle, address requester)
        external view returns (uint256);

    event HandleMinted(bytes32 indexed handle, address indexed creator, bytes4 op);
    event MockedHelperUsed(bytes4 indexed selector, address indexed caller);

    error InvalidHandle();
    error MainnetForbidden();
}
```

**Critical design choices** :

- All mutating ops emit `HandleMinted` so a user inspecting chain history can
  see every plaintext-bearing handle they created (audit trail).
- `decrypt` is a `view` function (no state mutation, no event) but takes
  `requester` as an argument, included so future versions with real access
  control have the parameter in the ABI from day one.
- No batch operations. Sprint 30 may add `addBatch` etc. as a gas optimization
  but only if Sprint 32 measures show it's needed.

**V1.0 path** (documented in `helper-source-audit-checklist.md` per Sprint 30) :
swap implementation to call Zama's real TFHE precompiles (or fhEVM-compatible
equivalent). The interface above is preserved; only the contract behind it
changes. Existing Covenant contracts continue to compile against the same
interface.

---

## 3. MockedPQVerifier (mocked)

**Replaces** : precompiles `0x150`, `0x154`.

**V0.9 reality** : signature verification is a length-check + modular parity.
Cryptographically meaningless. The interface matches what real Dilithium-5
verify will need so V1.0's swap-in is drop-in.

```solidity
// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.20;

interface IMockedPQVerifier {
    /// @notice Verify a Dilithium-5 signature.
    /// @param messageHash  keccak256 of the signed message.
    /// @param signature    Dilithium-5 signature (4595 bytes).
    /// @param pubKey       Dilithium-5 public key (2592 bytes).
    /// @return ok          True if the signature verifies.
    /// @dev V0.9 implementation = pseudo-verify (length checks + parity).
    ///      DOES NOT detect forgeries. V1.0 swaps in a real verifier
    ///      (Solady's pq lib or equivalent).
    function pqVerify(
        bytes32 messageHash,
        bytes calldata signature,
        bytes calldata pubKey
    ) external view returns (bool ok);

    /// @notice Derive a Dilithium-5 public key from a seed (test only).
    /// @dev V0.9 = deterministic stub. NOT a real keygen.
    function pqKeygenFromSeed(uint256 seed)
        external pure returns (bytes memory pubKey);

    /// @notice PRG-style randomness using prevrandao + caller nonce.
    /// @dev V0.9 = block.prevrandao + nonce hash. NOT cryptographically
    ///      secure for production use. V1.0 = VRF or PQ-PRG.
    function pqRandom(uint256 nonce) external view returns (bytes32);

    event MockedHelperUsed(bytes4 indexed selector, address indexed caller);

    error InvalidSignatureLength(uint256 actual, uint256 expected);
    error InvalidPublicKeyLength(uint256 actual, uint256 expected);
    error MainnetForbidden();
}
```

**Sprint 30 gas budget for `pqVerify`** : ≤ 30k (it's a parity check).
V1.0 with real Dilithium will be 150k to 300k.

---

## 4. MockedZKVerifier (mocked)

**Replaces** : precompiles `0x130`, `0x133`.

**V0.9 reality** : same shape as PQ, interface-correct, semantically meaningless.

```solidity
// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.20;

interface IMockedZKVerifier {
    /// @notice Verify a Halo2 SNARK proof against a verification key.
    /// @param vk            Verification key (commitment).
    /// @param publicInputs  ABI-encoded public inputs.
    /// @param proof         Serialized proof (>= 256 bytes).
    /// @return ok           True if the proof verifies.
    /// @dev V0.9 = parity check. V1.0 = real Halo2 verifier
    ///      (~250 to 400k gas).
    function verify(
        bytes32 vk,
        bytes calldata publicInputs,
        bytes calldata proof
    ) external view returns (bool ok);

    /// @notice Derive a nullifier from a secret (one-way hash).
    /// @dev Real implementation; not mocked. keccak256("nullifier" || secret).
    function nullifier(bytes32 secret) external pure returns (bytes32);

    /// @notice Aggregate multiple proofs (Nova IVC fold).
    /// @dev V0.9 = stub returning ABI-encoded ("AGG_V0_9_STUB", keccak(proofs)).
    ///      V1.0 = real Nova IVC accumulator.
    function proofAggregate(bytes calldata proofs)
        external pure returns (bytes memory);

    event MockedHelperUsed(bytes4 indexed selector, address indexed caller);

    error InvalidProofFormat(uint256 length);
    error MainnetForbidden();
}
```

**Note** : `nullifier` is *not* mocked, it's a deterministic hash used to
prevent ZK proof double-spend. Even in V0.9 it must be cryptographically sound
(keccak256 is fine). Marking it correctly here so Sprint 30 doesn't accidentally
weaken it under "mocked" framing.

---

## 5. Selector table

For Sprint 31 codegen and Sprint 32 cross-checks. Each row is an ABI selector
the compiler can emit a `CALL` for, plus the *original* V0.8 precompile
address being replaced.

| V0.8 precompile | V0.9 helper.method | Selector | Gas budget |
|---|---|---|---|
| `0x120` | `CeremonyHelper.amnesiaSetup` | `0x4d6f4a8b` (TBD verify) | 100k |
| `0x121` | `CeremonyHelper.amnesiaSubmitShare` | `0x...` | 50k |
| `0x122` | `CeremonyHelper.amnesiaFinalize` | `0x...` | 30k |
| `0x123` | `CeremonyHelper.amnesiaDestroy` | `0x...` | 80k |
| `0x101` | `MockedFHEHelper.encryptTrivial` | `0x...` | 30k |
| `0x102` | `MockedFHEHelper.encryptFresh` | `0x...` | 30k |
| `0x103` | `MockedFHEHelper.add` | `0x...` | 25k |
| `0x104` | `MockedFHEHelper.sub` | `0x...` | 25k |
| `0x105` | `MockedFHEHelper.mul` | `0x...` | 25k |
| `0x106` | `MockedFHEHelper.eq` | `0x...` | 25k |
| `0x107` | `MockedFHEHelper.lt` | `0x...` | 25k |
| `0x108` | `MockedFHEHelper.cmux` | `0x...` | 35k |
| `0x10F` | `MockedFHEHelper.decrypt` | `0x...` | 5k (view) |
| `0x130` | `MockedZKVerifier.verify` | `0x...` | 50k mocked / 350k real |
| `0x131` | `MockedZKVerifier.nullifier` | `0x...` | 5k (pure) |
| `0x132` | `MockedZKVerifier.proofAggregate` | `0x...` | 10k stub |
| `0x150` | `MockedPQVerifier.pqVerify` | `0x...` | 30k mocked / 250k real |
| `0x151` | `MockedPQVerifier.pqKeygenFromSeed` | `0x...` | 20k |
| `0x152` | `MockedPQVerifier.pqRandom` | `0x...` | 5k (view) |

Selectors are computed by Sprint 30 from final Solidity sources and committed
to `helper-addresses-v0.9.0.json` alongside the addresses (see
[address-resolution.md](./address-resolution.md) §3).

---

## 6. NatSpec banner (mandatory verbatim text)

Every mocked helper contract must open with this exact banner, it's part of
the contract's source. Etherscan displays NatSpec, which means anyone reading
the contract sees this immediately.

```solidity
/**
 * ⚠ V0.9 PLACEHOLDER, NOT FOR PRODUCTION SECRETS ⚠
 *
 * This contract implements the Covenant V0.9 [FHE / PQ / ZK] helper interface
 * with MOCKED logic. It is suitable ONLY for:
 *   - Playground demos and developer onboarding
 *   - Sepolia / testnet integration testing
 *   - Audit dry-runs of the Covenant compiler routing layer
 *
 * It is NOT suitable for:
 *   - Storing real secrets (plaintexts are recoverable from chain state)
 *   - Verifying real signatures (verify always passes for any well-formed input)
 *   - Verifying real ZK proofs (verify is a parity check, not Halo2)
 *
 * Production-grade implementations land in V1.0 after external audit.
 * Until then, this contract reverts on Ethereum mainnet (chainid 1).
 *
 * For the V0.9 → V1.0 swap-in plan, see:
 *   covenant/docs/v0.9/helper-source-audit-checklist.md
 */
```

The square-bracket placeholder `[FHE / PQ / ZK]` is filled in per contract.

---

## 7. Mainnet hard-revert

Every mocked helper includes this guard at the top of every state-changing
function (and in `view`/`pure` functions if they could be misread as production
oracles):

```solidity
modifier notMainnet() {
    if (block.chainid == 1) revert MainnetForbidden();
    _;
}
```

The check is on `block.chainid == 1` specifically, not "any non-testnet",
because L2s and sidechains may reasonably want to deploy testnet-equivalent
helpers. Sepolia (11155111), Aster (1996), Goerli (5), Holesky (17000) all
allowed.

If a future Covenant V1.0 wants to put real helpers on mainnet, V1.0 ships
*new* contracts under different names (`FHEHelper` without `Mocked` prefix),
deployed at different addresses. The V0.9 mocked contracts are never
unblocked.

---

## 8. Open questions for Sprint 30 to resolve

These are decisions the implementation sprint inherits, they're not blocked
by Sprint 29 design but should be revisited before Sprint 30 deploy :

1. **CeremonyHelper sessionId collision risk.** The current scheme is
   `keccak256(nonce || internal_counter || msg.sender)`. Bound is 2^256 so
   collisions are impractical, but if two different ceremonies somehow
   collide, the second `amnesiaSetup` call overwrites the first. Sprint 30
   should add an explicit `if (sessions[sessionId].ceremony != address(0))
   revert SessionCollision();` check.
2. **MockedFHEHelper handle privacy.** Even though the helper is mocked, the
   `HandleMinted` event leaks the handle and the creator. For some demo flows
   this is fine (educational). For others it's confusing. Sprint 30 should
   evaluate whether to add an opt-in private mode that suppresses the event.
3. **Gas-optimization vs audit clarity.** Sprint 30 should default to
   audit-clear code over gas-tight code. If Sprint 32 measurements are within
   the ceilings in §1, §4, leave the code unoptimized for OMEGA V5 ease.
4. **Library reuse.** The PQ verifier interface assumes Solady (or equivalent)
   provides a real Dilithium verifier in V1.0. Sprint 30 should add a TODO
   comment in `MockedPQVerifier.pqVerify` pointing to the chosen library
   reference (URL + commit hash).

---

## 9. Sign-off

These four interfaces are frozen for Sprint 30 to implement.

If Sprint 30 needs to change an interface during implementation, the change
comes back to this document with a rationale section, *and* affects Sprint 31
(compiler routing), so don't deviate without thinking through both.

| Role | Reviewer | Status |
|---|---|---|
| Architect | Kairos Lab | ✅ Frozen |
| Sprint 30 lead | Kairos Lab | reads this before writing Solidity |
| Sprint 31 lead | Kairos Lab | references §5 selector table for codegen |
| OMEGA V5 auditor | (future) | reviews against this doc + helpers source |

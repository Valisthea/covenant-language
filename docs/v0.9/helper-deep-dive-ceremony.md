# CeremonyHelper: Audit Deep Dive

> **Scope** : `helpers/src/CeremonyHelper.sol` (V0.9.1, deployed at
> `0x627f1Ff6Dc93AEba050c242FD9E26961E8F6c6F0` on Sepolia).
> **Status** : Real state machine ; cryptographic primitives (Wesolowski VDF,
> Shamir reconstruction) are placeholders pending V1.0.
> **Audit emphasis** : THIS is the audit-priority helper. The other three
> (`MockedFHEHelper`, `MockedPQVerifier`, `MockedZKVerifier`) are mocked.

## Why this helper matters

CeremonyHelper is the only V0.9 helper with a **real state machine**,
the others return deterministic stub values. It is exercised by every
contract that uses the `ceremony` keyword (including the M1 milestone
contract on Sepolia, see `MILESTONES.md`).

If the state machine is wrong, e.g. allows skipping phases, allows
double-finalize, allows post-destruction submits, every Covenant
ceremony contract is broken. This is the highest-leverage helper to
audit carefully.

## State machine

```
   ┌─────────┐  amnesiaSetup(seed)      ┌─────────┐
   │  ∅      │ ────────────────────────▶│  Setup  │
   │  (init) │                          │ (phase=0)│
   └─────────┘                          └────┬────┘
                                             │ (organizer activates)
                                             ▼
                                        ┌─────────┐
                                        │ Active  │
                                        │ (phase=1)│
                                        └────┬────┘
              amnesiaSubmitShare           │
              (guardians × N)              │ amnesiaFinalize(session)
                                             │ requires : shares_collected ≥ threshold
                                             ▼
                                        ┌──────────┐
                                        │Finalized │
                                        │(phase=2) │
                                        └────┬─────┘
                                             │ amnesiaDestroy(session)
                                             │ caller MUST be organizer
                                             ▼
                                        ┌──────────┐
                                        │Destroyed │
                                        │(phase=3) │
                                        └──────────┘
```

Transitions are **monotonic** (Setup → Active → Finalized → Destroyed)
and **gated by msg.sender** for organizer-only operations.

## Method-by-method audit notes

### `amnesiaSetup(uint256 seed)`

**Selector** : `0x09dc3eb0`
**Caller** : anyone (msg.sender becomes the ceremony organizer).
**Effect** : creates a new ceremony session keyed by `(msg.sender, seed)`
or `keccak(msg.sender, seed)` (verify in source). Sets phase = Setup,
records organizer, initializes guardian list (empty).

**Audit checklist** :
  - [ ] No collision possible if two callers use the same seed,
        sessions must be disambiguated by msg.sender as well.
  - [ ] Returns the new session_id for use in subsequent calls.
  - [ ] Emits `SetupComplete(session_id, organizer, timestamp)`.
  - [ ] Pays no value back (no ETH manipulation).

**Why we ship the 1-arg overload** : the V0.9.0 helper had only a 3-arg
signature `amnesiaSetup(seed, threshold, guardian_count)`, but the
Covenant compiler emits 1 operand for `Opcode::AmnesiaBegin`. This
mismatch was Sprint 31.b bug #4 (M1 deploy reverted 3 times before the
helper was patched to V0.9.1 with a 1-arg overload defaulting to
`(seed, 3, 2)`). Auditors should verify both overloads exist and the
3-arg one is still callable.

### `amnesiaSubmitShare(uint256 session, bytes32 share)`

**Selector** : `0x75ee5722`
**Caller** : any guardian for this session.
**Effect** : appends the share to the session's share list. Increments
the per-guardian submission count.

**Audit checklist** :
  - [ ] Reverts if session phase != Active.
  - [ ] Reverts if session does not exist.
  - [ ] Reverts if `msg.sender` already submitted for this session
        (one-share-per-guardian rule).
  - [ ] Emits `ShareSubmitted(session, guardian, share_index)`.
  - [ ] Does NOT reveal the share value in the event (commitment-only
        if PRELIM-018 hardening was applied, verify in source).
  - [ ] Storage write is bounded (per-session list cannot grow without
        bound, DoS surface).

### `amnesiaFinalize(uint256 session)`

**Selector** : `0x4ef88c73`
**Caller** : organizer ONLY.
**Effect** : transitions phase from Active to Finalized. Locks share
collection (no further `submitShare` accepted).

**Audit checklist** :
  - [ ] Reverts if `msg.sender != session.organizer`.
  - [ ] Reverts if phase != Active.
  - [ ] Reverts if `shares_collected < threshold` (declared at setup).
  - [ ] Emits `Finalized(session, organizer, timestamp,
        shares_count)`.
  - [ ] Atomic : either both phase and event are set, or neither.

### `amnesiaDestroy(uint256 session)`

**Selector** : `0x7688304b`
**Caller** : organizer ONLY.
**Effect** : transitions phase to Destroyed. Wipes share storage.

**Audit checklist** :
  - [ ] Reverts if `msg.sender != session.organizer`.
  - [ ] Reverts if phase != Finalized (cannot destroy directly from
        Active).
  - [ ] Wipes share storage (verify with `cast storage` post-call,
        slots returning to zero).
  - [ ] Emits `Destroyed(session, organizer, timestamp,
        destruction_proof)`. The `destruction_proof` is the
        commitment placeholder in V0.9 ; V1.0 will be a real
        Wesolowski VDF output.
  - [ ] Phase becomes Destroyed and is **irreversible** (no
        re-activation path exists).

## Cross-cutting properties

### Reentrancy

CeremonyHelper does **not** make external calls during state mutations.
The `amnesia*` family writes storage and emits events ; no `call`,
`delegatecall`, or `transfer` to user-controlled addresses. Reentrancy
not exploitable.

### Access control

**Correction (OMEGA V6, 2026-07-05, CRT-005) :** this section previously
claimed `submit_share` was protected by "the per-guardian submission
counter" ensuring one submission per address. No such counter existed
anywhere in the shipped source (`CeremonyHelper.sol` nor the synthesized
Covenant contract), a single address could call `submit_share` repeatedly
with garbage data and single-handedly satisfy any threshold, and
`finalize` had no on-chain check that any real shares had been submitted
at all. As of this fix, the **synthesized Covenant contract** (not this
Solidity helper) tracks distinct submitters via a `ceremony_submitted`
map and a `ceremony_submitter_count` field, and `finalize` now asserts
`submitter_count >= threshold` before trusting the precompile. This is a
distinct-CALLER count, not a pre-registered guardian-ADDRESS allowlist,
Covenant's `ceremony` construct has no language-level way to declare a
specific set of guardian addresses today (only a guardian *count* via
`guardians: N`), so it cannot yet verify a caller is one of the *intended*
guardians, only that `threshold` distinct addresses participated. See
`crates/covenant-stdlib/src/amnesia_ceremony.rs` and `DEBT.md`.

Two-tier model (current, corrected) :
  1. **Organizer-only** : finalize, destroy. Enforced via `require(msg.sender == organizer)`
     (Solidity helper) / `Assert(caller == deployer)` (synthesized contract).
  2. **Guardian-only** : submit_share. The Solidity helper (`CeremonyHelper.sol`)
     still has NO per-caller dedup or guardian-identity check of its own,
     the fix above lives entirely in the synthesized Covenant contract's own
     storage, one layer above this helper. Fixing `CeremonyHelper.sol`
     itself (and threading a real guardian-address list through, not just a
     count) is tracked as a follow-up in `DEBT.md`.

There is **no** admin role, **no** pausing mechanism, **no** upgrade
hook. The contract is functionally immutable post-deploy.

### Defense in depth: `onlyTestnet` modifier

CeremonyHelper does NOT carry the `onlyTestnet` modifier (unlike the
Mocked* helpers) because its state machine is real and could in
principle be useful even on mainnet. However :

  - The Covenant compiler refuses `--target-chain=mainnet` at compile
    time (`Target::parse` returns `Err(MainnetForbidden)`).
  - V1.0 will re-evaluate whether CeremonyHelper deploys to mainnet
    "as is" or with additional hardening (audited VDF, formal
    verification of the state machine).

### Storage layout

Verify storage layout stability across V0.9.x releases. Adding fields
must append-only ; reordering or removing fields breaks deployed
contracts that hold a CeremonyHelper reference.

```bash
forge inspect helpers/src/CeremonyHelper.sol:CeremonyHelper storageLayout
# Compare across V0.9.x patch versions.
```

### Events

All state transitions emit events. This is non-negotiable : off-chain
provability of the ceremony lifecycle requires every state change to
be loggable. Auditors should verify :

  - Every `amnesia*` method emits at least one event on success.
  - No method emits an event on failure (revert MUST roll back the
    log too, this is EVM-guaranteed but worth re-verifying).
  - Event signatures match what `ethers.js` and the playground
    `event-decoder.ts` expect.

## Known limitations (V0.9.x backlog)

| Limitation | Impact | Remediation |
|---|---|---|
| Wesolowski VDF is a keccak commitment placeholder | Destruction proof not externally verifiable as VDF | V1.0 : real VDF circuit |
| Shamir secret reconstruction not implemented | Threshold check on share count, not on reconstructible secret | V1.0 : add reconstruction proof |
| No per-ceremony gas budget | Long-running ceremonies could OOG | V0.9.x : add gas-checkpoint events |
| No batch-finalize | Each ceremony requires its own tx | Out of scope (rare use case) |

## Sprint 31.b empirical bug log

Four bugs were caught during the M0/M1 empirical deploy loop on Sepolia
that the design docs missed. Auditors should look for similar patterns
elsewhere :

  1. **Selector mismatch.** V0.8 namespaced opcode names did NOT
     collide with Solidity ABI selectors as initially assumed.
     Fix : explicit translation table in
     `crates/covenant-evm-backend/src/target.rs::helper_selector_for_opcode()`.
     Audit pattern : check that EVERY opcode the compiler emits has a
     test that confirms it dispatches to the correct selector.

  2. **STATICCALL on state-mutating helper.** V0.8 codegen used
     STATICCALL for all precompile dispatch because pre-V0.9 precompiles
     were view-only. CeremonyHelper is state-mutating ; needs CALL.
     Fix : per-target dispatch in
     `crates/covenant-evm-backend/src/codegen.rs::emit_precompile_call()`.
     Audit pattern : check that the codegen path for every helper
     correctly emits CALL vs STATICCALL based on the helper's
     state-mutability.

  3. **Returndata size strict `==32`.** Helper returns variable-length
     data ; the canary check was `EQ` ; needed to be `GTE`. Fix :
     conditional check based on `uses_helper_contracts()`.
     Audit pattern : returndata size assertions should match the
     actual return shape, not assume 32 bytes.

  4. **Operand count mismatch.** Compiler emitted 1 operand
     (`AmnesiaBegin` = seed only) ; helper had only a 3-arg signature.
     Fix : added 1-arg overload `amnesiaSetup(uint256)` at CREATE2
     V0.9.1 patched address.
     Audit pattern : every opcode's operand count MUST match the
     helper signature ; CI consistency test in
     `tests/registry_consistency.rs` cross-checks.

These bugs were caught EMPIRICALLY (M1 deploy reverted 3 times before
all 4 were fixed). The lesson : design docs are not enough ; the only
way to verify a compiler-helper bridge is to actually deploy and call
end-to-end. Auditors should re-run the M1 deploy sequence and observe
each tx succeed.

# Covenant Trust Boundaries

*Status: normative for the `covenant-v0.6-rc3` release. Keep in sync with
`crates/covenant-evm-backend/src/config.rs` (`PrecompileAddresses` defaults)
and `crates/covenant-evm-backend/src/artifact.rs` (`PRECOMPILE_ABI_VERSION`).*

## Scope

Covenant's advertised cryptographic primitives — Dilithium-5, Kyber-1024, TFHE,
Nova/Halo2, Shamir SSS, Wesolowski VDF — are **not implemented in this
repository**. They are exposed as IR opcodes that the EVM backend lowers to
STATICCALL / CALL against fixed precompile addresses (see table below). The
correctness and security of every Covenant contract therefore depends on a set
of chain-side precompiles that are:

- maintained out-of-tree;
- not audited by this repository's audit program;
- not selected by version at deploy time (only by address).

This document enumerates the boundary explicitly so downstream reviewers
understand what is in-scope for a compiler audit and what is not.

## Precompile inventory

Addresses below mirror `PrecompileAddresses::default()`. A chain that supports
Covenant-compiled contracts MUST expose compatible precompiles at these
addresses or an equivalent address set configured through `EvmConfig`.

### FHE (TFHE-rs semantics assumed)

| Opcode sink | Address | Assumed semantics |
|---|---|---|
| `FheEncryptTrivial` | `0x101` | Wrap plaintext in a ciphertext with zero noise (public encrypt). |
| `FheEncryptFresh` | `0x102` | Fresh encrypt under a published key. |
| `FheAdd` | `0x103` | Homomorphic add — consumes per-op noise budget. |
| `FheSub` | `0x104` | Homomorphic subtract. |
| `FheMul` | `0x105` | Homomorphic multiply — dominant noise consumer. |
| `FheCmpEq` | `0x106` | Homomorphic equality → ciphertext<bool>. |
| `FheCmpNe` | `0x113` | Homomorphic inequality. |
| `FheCmpLt` | `0x107` | Homomorphic less-than. |
| `FheCmpLe` | `0x114` | Homomorphic less-than-or-equal. |
| `FheCmpGt` | `0x108` | Homomorphic greater-than. |
| `FheCmpGe` | `0x112` | Homomorphic greater-than-or-equal. |
| `FheAnd` | `0x109` | Boolean AND over ciphertexts. |
| `FheOr`  | `0x10A` | Boolean OR. |
| `FheNot` | `0x10B` | Boolean NOT. |
| `FheSelect` | `0x10C` | Oblivious select: `c ? a : b`. |
| `FheCiphertextHash` | `0x10D` | Collision-resistant digest of a ciphertext. |
| `RevealDecrypt` | `0x10E` | Public reveal of a ciphertext (authorized principal only). |
| `FheBootstrap` | `0x10F` | Noise refresh. |
| `AssertEncrypted (bool)` | `0x110` | Threshold-decrypt `ciphertext<bool>` to 1 byte. |
| `threshold_decrypt_uint256` | `0x111` | Threshold-decrypt `ciphertext<uint256>` to 32 bytes. |

### ZK / VDF

| Opcode sink | Address | Assumed semantics |
|---|---|---|
| `ZkVerify` | `0x130` | Succinct proof verification (Nova/Halo2-class). |
| `ZkNullifier` | `0x131` | Nullifier emission for spent-note tracking. |
| `VdfEval`   | `0x132` | Wesolowski VDF evaluation. |
| `VdfVerify` | `0x133` | Wesolowski VDF verification. |

### Post-quantum

| Opcode sink | Address | Assumed semantics |
|---|---|---|
| `PqVerifyDilithium` | `0x150` | Dilithium-5 signature verification. |
| `PqHybridVerify` | `0x151` | Hybrid (Dilithium + classical) verification. |
| `PqRand` | `0x152` | PQ-safe randomness source. |
| `KyberEncrypt` | `0x153` | Kyber-1024 KEM encrypt. |
| `KyberDecrypt` | `0x154` | Kyber-1024 KEM decrypt. |

### Amnesia ceremony (Covenant)

| Opcode sink | Address | Assumed semantics |
|---|---|---|
| `AmnesiaBegin` | `0x120` | Open a new destruction ceremony session. |
| `AmnesiaSubmitShare` | `0x121` | Accept a Shamir share for the active session. |
| `AmnesiaFinalize` | `0x122` | Reconstruct and burn; returns success boolean. |
| `DestructionProof` | `0x123` | Emit a public proof that destruction occurred. |

## Defenses already in place

1. **EXTCODESIZE canary (KSR-CVN-019).** The constructor asserts
   `EXTCODESIZE(addr) == 0` for every precompile address the module references.
   A deployment reverts if an attacker or misconfigured chain has placed actual
   contract code at a precompile address.
2. **Selector prefix guard (KSR-CVN-020).** Each precompile call includes a
   selector prefix so a mis-routed call fails loudly rather than executing
   the wrong primitive.
3. **Success-flag propagation (KSR-CVN-013/014/027).** Every precompile
   `STATICCALL` / `CALL` is followed by `ISZERO ; PUSH __revert__ ; JUMPI`,
   so a failing precompile aborts the containing transaction rather than
   returning garbage to the caller.
4. **Precompile-ABI version marker (KSR-CVN-029).** The compiler embeds its
   target ABI version (`PRECOMPILE_ABI_VERSION`, currently `1`) at the top
   of every constructor as a dead `PUSH4 ; POP` sequence and publishes it in
   `metadata.json`. Chain governance and off-chain deploy tooling can
   byte-scan this marker to reject a deployment whose precompile ABI does
   not match the target chain's.

## Open boundary items (out-of-repo)

- **Precompile version reporter.** There is no reserved precompile that
  reports its own ABI version on-chain. The marker described above is
  compiler-side only; chains that want hard on-chain enforcement must add
  a version-reporting precompile and gate deployments through it.
- **Precompile upgrade path.** Key rotation and upgrade semantics are
  defined by the chain, not by Covenant.
- **Stdlib correctness.** Any flaw in the precompile implementation (noise
  analysis, signature verification, VDF soundness) immediately compromises
  every contract using that primitive.

## Verification checklist for a chain integrator

Before accepting Covenant-compiled bytecode on a new chain:

1. Confirm every address in `PrecompileAddresses::default()` is populated
   with the assumed primitive, or override `EvmConfig::precompile_addresses`
   and document the delta.
2. Read the constructor bytecode and locate the `PUSH4` of
   `PRECOMPILE_ABI_VERSION`. Reject deployments whose version ≠ your chain's
   supported version.
3. Wire `cargo deny check` and `cargo audit` into CI for the Covenant tree
   (`Cargo.lock` is committed per KSR-CVN-007 / KSR-CVN-009).
4. Keep `docs/trust-boundaries.md` in your audit bundle; any change to the
   address table or version field is a breaking change for downstream
   contracts and must be governance-gated.

## Audit trail

- 2026-04-22 — KSR-CVN-010 introduced this document requirement.
- 2026-04-22 — KSR-CVN-029 added `PRECOMPILE_ABI_VERSION = 1` and its
  constructor marker.

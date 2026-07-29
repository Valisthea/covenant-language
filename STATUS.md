# Covenant: Project Status & Honest Disclaimer

**Read this before evaluating, deploying, or writing about Covenant.**

Covenant is a declarative smart-contract language (Rust compiler → EVM bytecode) that makes
cryptographic patterns, FHE, post-quantum signatures, ZK proofs, cryptographic amnesia,
**first-class language primitives**. The genuinely novel contribution is a **compile-time
privacy / key-identity type system**, and that part is real.

## ⚠️ The one thing you must know

> **The cryptographic primitives are MOCKED.** FHE / PQ / ZK / VDF / Shamir are implemented as
> **deterministic stubs**, gated to testnets. They provide **ZERO confidentiality and ZERO
> cryptographic security**: FHE "ciphertexts" are plaintext entries recoverable from chain
> state, and the PQ / ZK verifiers are effectively coin-flips that accept forged proofs.
> **Do not place real value at risk. Do not deploy to mainnet.** Real implementations plus an
> external audit land in a later cryptography release (V2.0).

## What is REAL (production-quality, tested)

- **The compiler**: 21-crate Rust workspace: lexer → parser → resolver → type-checker →
  privacy analyzer → IR → EVM bytecode. 1000+ tests passing, clippy-clean.
- **The compile-time privacy / key-identity type system**: the novel core; true without any crypto.
- **Auto-synthesis** of ERC-20, ERC-721 and PQ-registry surfaces from ~5-line declarations,
  deployed and exercised on Ethereum **Sepolia** (verifiable tx hashes in [`MILESTONES.md`](MILESTONES.md)).
- **The ceremony state machine** (`CeremonyHelper`), a real 4-phase lifecycle, distinct from the
  mocked crypto it orchestrates.
- **Tooling**: Foundry-class CLI (init/build/check/test/fmt/lint/inspect/doctor/explain),
  Ariadne-style diagnostics, VS Code / Cursor / Claude Code editor integrations.

## What is MOCKED (deterministic stubs: no security whatsoever)

| Primitive | Reality today |
|---|---|
| FHE (`MockedFHEHelper`) | plaintext key-value store; "encrypted" values are readable on-chain |
| Post-quantum signatures (`MockedPQVerifier`) | parity check, accepts ~50% of forgeries |
| ZK proofs (`MockedZKVerifier`) | same coin-flip; verifies nothing |
| VDF time-lock | keccak commitment; no sequential work / no delay |
| Shamir secret sharing | share **count** only; no real split or reconstruction |

Every mock file carries a `PLACEHOLDER` banner and is gated by an `onlyTestnet` modifier.

## Audit status

- **Internal only.** OMEGA V4 / V5 / V6 are Kairos-Lab **self-audits**, not third-party audits.
- A V0.9.2 self-audit found and fixed a **Critical** (ERC-721 authorization bypass) plus
  several High-severity codegen defects. The V0.9.3 (OMEGA V6, 2026-07-05) self-audit found
  and fixed **6 Critical + 6 High + 5 Medium** defects, the largest single-cycle finding
  count since the V0.6 launch audit, including three complete-authorization-bypass bugs and
  an uncatchable stack-overflow DoS reachable via every subcommand and the LSP. See
  `covenant-security-reviews/audits/2026-07-05-omega-v6-covenant-v0.9.2/` for full detail. Every cycle
  finding more defects, not fewer, is itself evidence the codegen surface is still
  pre-external-audit.
- **No external firm audit yet.** The unqualified word "audited" should not be used publicly
  until an external audit passes.

## The three-axis roadmap

| Milestone | Meaning | Status |
|---|---|---|
| **Public launch** | Open-source, honest, **testnet-only** Language & Compiler | current target |
| **V1.0** | + external audit of the compiler + `CeremonyHelper` | +3 to 5 months |
| **V2.0, Cryptography** | real FHE / PQ / ZK / VDF / Shamir + crypto audit | 12 to 24 months |

## Standards

The Styx Protocol ERC specs are **Draft standards authored by Kairos Lab**: **ERC-8227**
(Encrypted Token), **ERC-8228** (Cryptographic Amnesia), **ERC-8229** (FHE Computation
Verification), and **ERC-8231** (Post-Quantum Key Registry). The amnesia ceremony maps to
**ERC-8228** (Cryptographic Amnesia), exactly as the confidential token maps to ERC-8227.

---

*Testnet-only · crypto mocked · internally self-audited · not for production or real value.*

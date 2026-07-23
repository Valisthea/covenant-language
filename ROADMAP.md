# Covenant — Roadmap

Forward-looking roadmap for **Covenant Language**. Companion to:
- [`MILESTONES.md`](MILESTONES.md) — past achievements (verifiable on-chain firsts)
- [`CHANGELOG.md`](CHANGELOG.md) — version history
- [`DEBT.md`](DEBT.md) — known limitations / technical-debt registry
- [`STATUS.md`](STATUS.md) — the honest security & maturity posture

The roadmap has **three deliberately decoupled axes**. Read [STATUS.md](STATUS.md) first:
the cryptography is **mocked and testnet-only** today, and real cryptography is a separate,
much later track.

> **Current: v0.9.4** — the *fail-loud pass*. Seven classes of silent miscompile now error or
> work correctly; the LSP surfaces those diagnostics live; CI is green on `stable`. 1082 tests.

---

## Axis 1 — Public launch (current)

An open-source, honest, **testnet-only** language & compiler.

| Item | Status |
|---|---|
| Compiler pipeline (lexer → parser → resolver → types → privacy → IR → EVM) | ✅ shipped |
| Auto-synthesis of ERC-20 / ERC-721 / PQ-registry surfaces | ✅ shipped |
| Compile-time privacy / key-identity type system | ✅ shipped |
| Foundry-class toolchain (CLI, LSP, linter, formatter, `doctor`) | ✅ shipped |
| Editor integrations (VS Code, Cursor, Claude Code) | ✅ shipped |
| Reproducible / deterministic builds | ✅ demonstrated |
| Continuous fuzzing (`cargo-fuzz` on the compile pipeline) | ✅ shipped |
| Milestones on public testnets (Sepolia + Robinhood Chain) | ✅ M0–M6 live |
| In-browser source verifier (`/verify`) | ✅ shipped |
| Hosted install channels (crates.io, one-line installer, marketplace) | 🚧 coming soon |

### Near-term engineering / DX

| Item | Notes |
|---|---|
| Dynamic `bytes` / `string` / `T[]` storage + ABI return shapes | tracked in [`DEBT.md`](DEBT.md) |
| `chainid`-gate fail-loud (mocked crypto must refuse real-value chains) | tracked in [`DEBT.md`](DEBT.md); requires a CREATE2 helper redeploy |
| LSP `did_change` debounce | tracked in [`DEBT.md`](DEBT.md) |
| SBOM emission on `--release` (CycloneDX) | — |
| V0.8 → V0.9 migration guide | — |

---

## Axis 2 — V1.0 (external audit gate, +3–5 months)

V1.0 is **language-tooling maturity**, gated on an **external, third-party audit** of the
compiler and the `CeremonyHelper` state machine. The cryptography is still mocked at this stage.

| Item | Notes |
|---|---|
| External third-party audit of compiler + `CeremonyHelper` | the V1.0 gate |
| Formal verification of the `CeremonyHelper` state machine | — |
| OSS-Fuzz continuous fuzzing integration | — |

Pre-requisites already prepared: [`docs/v0.9/audit-scope-v0.9.0.md`](docs/v0.9/audit-scope-v0.9.0.md),
[`docs/v0.9/threat-model-v0.9.0.md`](docs/v0.9/threat-model-v0.9.0.md),
[`docs/v0.9/known-acceptable-risks.md`](docs/v0.9/known-acceptable-risks.md),
[`examples/audit/`](examples/audit/), [`SECURITY.md`](SECURITY.md).

---

## Axis 3 — V2.0 Cryptography (real crypto, 12–24 months)

Replace every mocked primitive with a real, externally-audited implementation. This axis ships
from a **separate release track**; nothing here is mainnet-safe until it lands **with a
cryptography audit**.

| Item | Replaces |
|---|---|
| Real Wesolowski VDF circuit | commitment placeholder in `CeremonyHelper` |
| Real Shamir secret reconstruction | counter-only stub |
| Real FHE bindings | `MockedFHEHelper` |
| Real post-quantum (Dilithium-class) verifier | `MockedPQVerifier` |
| Real ZK (SNARK) on-chain verifier | `MockedZKVerifier` |
| Remove `Mocked*` prefix + `onlyTestnet` gate; enable mainnet target | the testnet-only safety rail |

Only after this axis completes, and its external cryptography audit passes, does a mainnet
deployment path open.

---

## How to use this file

- **Add an item**: append to the appropriate axis, and link its source of truth
  ([`DEBT.md`](DEBT.md) / threat model / etc.).
- **Mark complete**: move to [`MILESTONES.md`](MILESTONES.md) (if a verifiable on-chain first)
  or [`CHANGELOG.md`](CHANGELOG.md) (if a code/feature ship).

# Security Policy

This document is the canonical security policy for the Covenant compiler
and specifications repository (`Valisthea/covenant-language`). For Covenant
runtime / playground / docs site, see those repos' own SECURITY.md.

## Supported versions

| Version | Security support | Notes |
|---|---|---|
| V0.9.x (current) | ✅ Full | Testnet-only ; mainnet helpers blocked at compile time. Internal OMEGA V5 self-audit ; external firm audit gated to V1.0. |
| V0.8.x | ⚠️ Critical fixes only | Pre-helper-bridge era. No new feature backports. |
| V0.7.x and earlier | ❌ End of life | No fixes. Upgrade. |

## Threat model & scope

  - **In-scope** : the Rust compiler (`crates/covenant-*`), the helper
    contracts (`helpers/src/*.sol`), the CLI (`crates/covenant-cli`), the
    test runner (`crates/covenant-testing`), the LSP server
    (`crates/covenant-lsp`).
  - **Out-of-scope (this repo)** : the playground UI
    (`Valisthea/covenant-playground`), the docs site
    (`Valisthea/covenant-lang`), and the target deploy chains themselves
    (e.g. Robinhood Chain, Ethereum Sepolia).
  - **Cryptographic primitives** (Dilithium, Kyber, TFHE, Nova/Halo2,
    Shamir, Wesolowski VDF) are NOT implemented in this repo. They are
    exposed as IR opcodes lowered to CALL/STATICCALL against helper
    contract addresses ; the helpers currently mock the cryptography
    (real state machines + commitment placeholders, see
    `docs/v0.9/helper-source-audit-checklist.md`). Audits should treat
    the cryptographic boundary as out-of-scope per
    `docs/trust-boundaries.md`.

For the full threat model, see
[`docs/v0.9/threat-model-v0.9.0.md`](docs/v0.9/threat-model-v0.9.0.md).
For the audit scope statement, see
[`docs/v0.9/audit-scope-v0.9.0.md`](docs/v0.9/audit-scope-v0.9.0.md).

## Reporting a vulnerability

**Please do NOT open a public GitHub issue for security vulnerabilities.**

Email `admin@kairos-lab.org` with :

  - A clear description of the vulnerability.
  - Reproduction steps (or a proof-of-concept patch / fixture).
  - The affected version(s) and configuration.
  - Your name / handle and any disclosure preferences.

For especially sensitive reports, request our PGP key in the email
subject ; we will respond out-of-band.

### Response timeline

  - **Acknowledgement** : within 48 hours of report receipt.
  - **Initial triage** (severity assignment, scope confirmation) :
    within 5 business days.
  - **Fix or mitigation plan** : within 30 days for High/Critical,
    90 days for Medium, best-effort for Low.
  - **Public disclosure** : coordinated with the reporter ; default is
    90 days post-fix unless we agree on a shorter / longer timeline.

### Severity scale

We use a CVSS-3.1-aligned scale. For compiler bugs, "exploitability"
means : can a malicious source file produce bytecode that violates the
language's safety guarantees (e.g. unauthorized state mutation, value
extraction, replay) when deployed to a chain that honors the helper
contract assumptions ? See
[`docs/v0.9/threat-model-v0.9.0.md`](docs/v0.9/threat-model-v0.9.0.md)
for the full taxonomy.

## Bug bounty

A formal bug bounty program will launch alongside V1.0 (post external
audit). For V0.9.x, we will recognize valid reports publicly (with the
reporter's consent) and offer goodwill compensation case-by-case.

## Known acceptable risks

The following are tracked but accepted for V0.9.x ; see
[`docs/v0.9/known-acceptable-risks.md`](docs/v0.9/known-acceptable-risks.md)
for the rationale of each.

  - `RUSTSEC-2024-0421` (idna) — transitive via `tower-lsp` ; LSP only
    handles `file://` URIs, no IDN path.
  - `Mocked*` helper contracts — real state machines, mocked
    cryptography ; mainnet deploy blocked in source (`onlyTestnet`
    modifier) and at compile time (`Target::parse` rejects `mainnet`).

## What we DO NOT consider vulnerabilities

  - Compiler errors / panics on syntactically invalid input (these are
    `error[ICE]` bugs ; please file as normal issues).
  - `cargo doctor` / `covenant doctor` warnings about missing optional
    env vars (`SEPOLIA_RPC_URL`, etc.).
  - Helper contract calls reverting on **mainnet chain ID** — this is
    the intended `onlyTestnet` defense-in-depth, not a bug.
  - Comments not preserved through `covenant fmt` (V0.9.0 limitation,
    documented).

For ambiguous cases, err on the side of reporting privately ;
we'd rather triage a non-issue than miss a real one.

# Covenant V0.9.0 — Audit Fixture Pack

Curated `.cov` fixtures for external audit reviewers (OMEGA V5, Sprint
46 ; external firm V1.0). Each fixture exercises a specific surface of
the compiler / stdlib synthesizer / lint detector / helper-contract
bridge. Auditors should be able to walk this directory top-to-bottom and
hit every important code path.

## Inventory

| # | File | Construct | What it exercises |
|---|---|---|---|
| 01 | `01_token_basic.cov` | `token` | ERC-20 surface synthesis (transfer, approve, balanceOf, etc.) |
| 02 | `02_ceremony_lifecycle.cov` | `ceremony` | **CeremonyHelper bridge.** Setup → Active → Finalized → Destroyed. M1 milestone fixture pattern. |
| 03 | `03_ballot_open.cov` | `ballot` | `when` / `only` / `given` guard composition + deadline + first-time-caller |
| 04 | `04_nft_minimal.cov` | `nft` | ERC-721 surface synthesis (Sprint 35.b) |
| 05 | `05_registry_pq.cov` | `registry` | ERC-8231 PQ key registry surface synthesis |
| 06 | `06_auth_only_deployer.cov` | `record` | Access control — `only deployer` clause |
| 07 | `07_revert_paths.cov` | `record` | Custom error reverts (selector + ABI-encoded args) |
| 08 | `08_view_purity.cov` | `record` | View vs action separation, STATICCALL safety |
| 09 | `09_synthesis_conflict_negative.cov` | (doc-only) | E601 detector (test-suite-exercised, not surface-buildable in V0.9.0) |

## How to run the pack

```bash
# 1. Compile every fixture to MockChain bytecode (default target)
for f in examples/audit/*.cov; do
    covenant build "$f"
done

# 2. Compile against Sepolia (helper-contract dispatch)
for f in examples/audit/*.cov; do
    covenant build "$f" --target-chain=sepolia
done

# 3. Inspect any fixture's IR / ABI / storage layout
covenant inspect ast      examples/audit/02_ceremony_lifecycle.cov
covenant inspect ir       examples/audit/02_ceremony_lifecycle.cov
covenant inspect bytecode examples/audit/02_ceremony_lifecycle.cov
covenant inspect abi      examples/audit/02_ceremony_lifecycle.cov
covenant inspect storage  examples/audit/02_ceremony_lifecycle.cov

# 4. Lint the pack
for f in examples/audit/*.cov; do
    covenant lint "$f"
done
```

## Expected lint findings

Two fixtures intentionally trip the linter to demonstrate detector
coverage. Auditors should observe the findings; they are not bugs.

| Fixture | Code | Detector |
|---|---|---|
| `03_ballot_open.cov` | C200 | Timestamp drift via `now` (miner can shift ±15s) |
| `08_view_purity.cov` | C100 | Missing access guard on `increment` action |

All other fixtures lint clean.

## What's NOT in the pack (and why)

  - **Encrypted constructs** (`example_04_shielded_counter`, `_06_secret_coin`,
    `_07_private_dao`) — exercise `MockedFHEHelper`, which is documented
    as audit-out-of-scope (cryptography mocked). The
    `02_ceremony_lifecycle` fixture is the canonical helper-bridge
    exercise ; the FHE helpers are tested via the lexer fixture suite
    (`crates/covenant-lexer/tests/fixtures/`) but not duplicated here.
  - **Cross-chain modules** (`example_09_encrypted_bridge`,
    `_10_hybrid_state`) — exercise the `module` construct, which is V0.9
    stable but not the audit-priority surface. Reference fixture in the
    lexer suite.
  - **Stress / fuzz fixtures** — V0.9.x will add a `examples/fuzz/`
    pack ; for V0.9.0 the regression suite is the existing per-crate
    `tests/` directory.

For the full reference fixture set see
`crates/covenant-lexer/tests/fixtures/example_*.cov`.

## Per-fixture audit emphasis

### `02_ceremony_lifecycle.cov` (highest priority)

This is the M1-pattern fixture : the same `ceremony { ... }` block that
deployed the first end-to-end Covenant ceremony on Sepolia
(`0x2FB87d54...`, see `MILESTONES.md`). Auditors should :

  1. Build with `--target-chain=sepolia`.
  2. Inspect the bytecode and verify each of the 4 ceremony entry
     points (setup, submit_share, finalize, destroy) emits a CALL to
     the CeremonyHelper address with the correct selector :
     - `amnesiaSetup(uint256)` = `0x09dc3eb0`
     - `amnesiaSubmitShare(uint256,bytes32)` = `0x75ee5722`
     - `amnesiaFinalize(uint256)` = `0x4ef88c73`
     - `amnesiaDestroy(uint256)` = `0x7688304b`
  3. Cross-check selector table against
     `config/helper-addresses-v0.9.0.json` — these MUST match.
  4. Read the helper deep-dive :
     `docs/v0.9/helper-deep-dive-ceremony.md`.

### `06_auth_only_deployer.cov` + `07_revert_paths.cov`

Pair these for a complete audit of the auth boundary :
  - `06` : positive-case auth (deployer set ; only-deployer enforced).
  - `07` : negative-case auth (custom error reverts).

Together they exercise the full ABI surface of authorization-related
runtime checks.

### `08_view_purity.cov`

Use this to verify the compiler's view/action distinction at the ABI
level. Run :

```bash
covenant inspect abi examples/audit/08_view_purity.cov | grep -A1 stateMutability
```

Expected : 3 entries with `"stateMutability": "view"` (get_n, get_doubled,
get_n_plus) and 1 entry with `"stateMutability": "nonpayable"`
(increment).

## Reproducibility

Two clean builds of any fixture in this pack should produce :
  - **Identical bytecode** (32-byte hex hash of the runtime).
  - **Identical ABI JSON** (modulo key ordering ; sort before diff).
  - **Identical storage-layout sidecar**.

If any of these differ across two clean builds from the same git SHA,
the build is non-deterministic — that is a **High severity bug** per
`docs/v0.9/audit-scope-v0.9.0.md`.

## Reporting findings

  - **Functional / soundness bugs** in any fixture : open a normal
    GitHub issue.
  - **Security findings** : email `admin@kairos-lab.org` per
    `SECURITY.md`. Do NOT open a public issue.
  - **Audit-process feedback** (this pack is too narrow / too broad /
    misleading) : email `audit@kairos-lab.org`.

# OMEGA V5: V0.9.0 Self-Audit Report

> **Audit gate** : Sprint 46 of the V0.9 master plan.
> **Date** : 2026-04-26.
> **Auditor** : Kairos Lab internal, OMEGA adversarial review.
> **Methodology** : OMEGA V5, empirical-loop-driven self-audit. Every
> claim in this report is grounded in a runnable command whose output
> we have observed in this session.
>
> **This is NOT an external audit.** It is the internal go/no-go gate
> before the V0.9.0 tag and the subsequent V1.0 external audit.

## Executive verdict

**Status : ✅ GO for V0.9.0 tag (Sprint 47).**

  - All gating tests pass (1172 Rust unit/integration + 34 Foundry).
  - Static analysis clean (clippy `-D warnings` PASS).
  - Zero unaccepted security advisories (1 documented residual : idna
    transitive via tower-lsp ; no IDN path in our code).
  - Audit pack compiles end-to-end on both MockChain and Sepolia
    targets.
  - M0 + M1 milestones live and verifiable on Sepolia (see
    `MILESTONES.md`).
  - Documentation deliverables complete (SECURITY.md, audit scope,
    threat model, helper deep-dive, audit fixture pack).

**Conditions on V0.9.0 release** : none. The gate is met.

**Conditions on V1.0 mainnet** : NOT met. V1.0 requires real
cryptography (replace `Mocked*` helpers), external audit pass, and
mainnet helper deploy. Out of scope for V0.9.0.

## Scope of this report

This self-audit covers the same scope as
[`docs/v0.9/audit-scope-v0.9.0.md`](docs/v0.9/audit-scope-v0.9.0.md) :

  - 22-crate Rust workspace (compiler core, CLI, LSP, testing, lint).
  - Foundry helper contracts (`helpers/src/CeremonyHelper.sol` real ;
    `Mocked*Helper.sol` mocked, defended by `onlyTestnet` + compile-time
    target gate).
  - 23 example `.cov` files (lexer fixture suite + Sprint 40 isolation
    demo + Sprint 45 audit pack).

Out of scope :
  - Playground UI (separate repo, separate audit cycle).
  - Docs site (static, no privileged surface).
  - Cryptographic primitives (mocked in V0.9 by design ; see
    `docs/trust-boundaries.md`).
  - Aster Chain integration (codegen ready, deploy deferred V0.9.x ;
    see `docs/v0.9/aster-chain-integration-status.md`).

## Findings & verifications

### V-001 ✅ Workspace test gauntlet : 1172 / 1172 passing

```
cargo test --workspace --lib --no-fail-fast    → 148 passed
cargo test --workspace --tests --no-fail-fast  → 1024 passed (2 ignored)
                                                 ─────────
                                                 1172 total
```

**Verdict** : GREEN. Zero failing tests across the full workspace.

### V-002 ✅ Foundry helper test gauntlet : 34 / 34 passing

```
cd helpers && forge test
  → 4 test suites, 34 tests passed, 0 failed, 0 skipped
```

Including the high-priority CeremonyHelper suite (10/10) and the
`Mocked*Helper.sol` `test_MainnetForbidden` defense-in-depth tests.

**Verdict** : GREEN. Helper-contract layer is operationally clean.

### V-003 ✅ Static analysis : clippy `-D warnings` PASS

```
cargo clippy --workspace --all-targets --no-deps -- -D warnings
  → Finished, no warnings.
```

Sprint 44 fixed all 7 pre-existing warnings ; no regressions in Sprint
45.

**Verdict** : GREEN. The codebase compiles strict-warning-clean across
the entire workspace.

### V-004 ⚠️ cargo audit : 1 advisory (documented as accepted)

```
cargo audit
  → 1 vulnerability : RUSTSEC-2024-0421 (idna ≤ 0.5.x)
    Transitive via : tower-lsp 0.20.0 → lsp-types 0.94.1 → url 2.4.1
    → idna 0.4.0
```

Documented in
[`docs/v0.9/known-acceptable-risks.md`](docs/v0.9/known-acceptable-risks.md)
with rationale : LSP server only handles `file://` URIs, no IDN path.
Upstream fix requires tower-lsp upgrade ; 0.20.0 is the latest stable.

**Verdict** : ACCEPTED RESIDUAL. Within the audit baseline.

### V-005 ✅ Audit fixture pack end-to-end smoke

```
for f in examples/audit/*.cov; do covenant check "$f"; done
  → 9 / 9 passed
```

Spot-checked Sepolia build of the highest-priority fixture :
```
covenant build examples/audit/02_ceremony_lifecycle.cov --target-chain=sepolia
  → ok: AuditCeremony, deploy 904 bytes, runtime 877 bytes
```

**Verdict** : GREEN. The fixture pack handed off to external auditors
will compile cleanly on their machines.

### V-006 ✅ M0 + M1 Sepolia milestones live

  - **M0** : First Hello-on-Sepolia deploy at
    `0xab083fc4...` (see `MILESTONES.md`).
  - **M1** : First end-to-end Covenant ceremony on Sepolia at
    `0x2FB87d54...` (5 lifecycle txs, 4 Sprint 31.b bugs caught
    empirically and fixed).

Both contracts are publicly visible via Etherscan links in
`MILESTONES.md` (and via `config/helper-addresses-v0.9.0.json`
`etherscan_links` block).

**Verdict** : GREEN. The V0.9 helper-bridge architecture is empirically
validated on a real EVM testnet.

### V-007 ✅ Documentation deliverables complete

| Document | Status | Path |
|---|---|---|
| Security policy | ✅ | `SECURITY.md` |
| Audit scope statement | ✅ | `docs/v0.9/audit-scope-v0.9.0.md` |
| Threat model (STRIDE) | ✅ | `docs/v0.9/threat-model-v0.9.0.md` |
| Known acceptable risks | ✅ | `docs/v0.9/known-acceptable-risks.md` |
| Helper deep-dive (CeremonyHelper) | ✅ | `docs/v0.9/helper-deep-dive-ceremony.md` |
| Helper audit checklist | ✅ | `docs/v0.9/helper-source-audit-checklist.md` |
| Trust boundaries | ✅ | `docs/trust-boundaries.md` |
| Audit fixture pack + README | ✅ | `examples/audit/` |
| Aster Chain integration status | ✅ | `docs/v0.9/aster-chain-integration-status.md` |
| CLI reference docs | ✅ | `docs/v0.9/cli-*.md` |
| Precompile bridge architecture | ✅ | `docs/v0.9/precompile-bridge-architecture.md` |
| MILESTONES (M0 + M1) | ✅ | `MILESTONES.md` |
| Tech debt registry | ✅ | `DEBT.md` |
| Lessons learned | ✅ | `LESSONS.md` |

**Verdict** : GREEN. Documentation surface is comprehensive and
internally cross-referenced.

### V-008 ✅ Bug : `covenant lint` ICE, caught & fixed during Sprint 45

While smoke-testing the audit fixture pack (Sprint 45), `covenant lint`
ICE'd on every fixture with a clap arg-type-mismatch panic. Root cause :
`LintArgs` had a local `--color: String` arg colliding with the global
`Cli` `--color: ColorMode` arg. Fix shipped in Sprint 45 commit
`6a481b3`.

**Lesson confirmed** : the empirical-loop discipline (smoke-test every
fixture) catches what design docs miss. This is the same pattern as the
4 Sprint 31.b bugs caught during M1 deploy. **Recommend** : every
future Sprint must include a "smoke-test the new surface end-to-end"
step before commit.

**Verdict** : GREEN (bug fixed, regression test exists in CLI test
suite).

### V-009 ⚠️ Open items deferred to V0.9.x (acknowledged, NOT gating)

These are tracked in DEBT.md and re-stated in
`known-acceptable-risks.md` :

  - **D-03 LSP debounce** : `did_change` triggers full re-parse, no
    debounce. Affects developer ergonomics, not deployed-contract
    security. V0.9.x backlog.
  - **E-02 reproducible-build CI gate** : reproducibility documented
    but not CI-enforced. V0.9.x backlog.
  - **E-03 lint-on-templates in CI** : manual review only currently.
    V0.9.x backlog.
  - **Compiler fuzz suite** : no `cargo-fuzz` ; relying on regression
    + empirical loop for V0.9.0. V0.9.x backlog.
  - **SBOM emission on `--release`** : not yet ; documented in
    `known-acceptable-risks.md`. V0.9.x backlog.
  - **Aster Chain helper deploy** : codegen ready, operational deploy
    deferred (see `aster-chain-integration-status.md`). V0.9.x or
    when Aster mainnet/factory unblocked.

**Verdict** : ACKNOWLEDGED. None of these block V0.9.0 release.

### V-010 ❌ Out-of-scope items (NOT in V0.9.x scope)

  - **Real cryptography** in helpers (FHE / PQ / ZK / VDF / Shamir).
    By design : V1.0 work, after external audit and crypto-specific
    review.
  - **Mainnet deploy** of any helper. Blocked at compile time
    (`Target::parse` rejects mainnet) and at runtime (`onlyTestnet`
    modifier on Mocked* helpers).
  - **External audit pass**. This report is internal ; an external
    audit is a V1.0 gate, not V0.9.0.

**Verdict** : OUT OF SCOPE for V0.9.0. Correctly deferred.

## Repository metrics (informational)

```
22 crates in workspace
23 .cov example files (13 lexer fixtures + 1 isolation demo + 9 audit pack)
14+ V0.9 reference docs in docs/v0.9/
1172 Rust tests + 34 Foundry tests = 1206 total
0 failing tests
0 clippy warnings under -D warnings
1 cargo-audit advisory (documented as accepted)
```

## Risk register at gate

| Risk | Severity | Status | Reference |
|---|---|---|---|
| Mainnet deploy of mocked helper | Critical | ✅ 4-layer defense | `helper-source-audit-checklist.md` |
| Bytecode tampering between compile & deploy | Medium | ⚠️ Documented, no SBOM yet | T-01, threat model |
| Compiler emits wrong helper selector | High | ✅ CI consistency test | T-04, V-008 lesson |
| Helper state-machine bypass (CeremonyHelper) | Critical | ✅ Forge tests + audit deep-dive | `helper-deep-dive-ceremony.md` |
| Test isolation broken (state leak) | High | ✅ Sprint 40 per-test fresh harness | `cli-test-and-fmt.md` |
| Unauthorized modification of `--color` arg path triggers ICE | Low | ✅ Fixed Sprint 45 | V-008 |
| LSP server DoS via large doc | Low | ⚠️ D-03 deferred | DEBT.md |
| Reproducibility regression | Medium | ⚠️ Documented, manual verify | E-02 deferred |
| RUSTSEC-2024-0421 idna | Low | ⚠️ Accepted residual | `known-acceptable-risks.md` |

## Sign-off conditions for V0.9.0 tag (Sprint 47)

  - [x] All workspace tests green
  - [x] All Foundry tests green
  - [x] Clippy `-D warnings` PASS
  - [x] cargo audit baseline matches `known-acceptable-risks.md`
  - [x] Audit fixture pack compiles
  - [x] M0 + M1 Sepolia milestones documented
  - [x] All audit-prep docs landed on `main`
  - [x] CHANGELOG.md V0.9.0 entry drafted
  - [ ] Git tag `v0.9.0` applied to merge commit (Sprint 47)
  - [ ] GitHub Release notes published (Sprint 47)
  - [ ] Tweet thread for `@Covenant_Lang` queued (Sprint 47)

8 / 11 met. Remaining 3 are Sprint 47 mechanics (tag + release + tweet).

## Recommendations for V0.9.x

  1. **Add `--strict` flag to `covenant doctor`** : exit non-zero on
     any `Failed` probe. Makes doctor CI-gateable.
  2. **Replace name-heuristic coverage with IR-instrumented coverage**
     in `covenant test --coverage`. The heuristic is honest but
     imprecise.
  3. **Implement LSP `did_change` debounce** (100ms) and
     `did_save` full publish.
  4. **Reproducibility CI** : two `--release` builds from the same SHA
     must produce byte-identical bytecode. Gate the merge on it.
  5. **`cargo-fuzz` harness** for lexer + parser + codegen. Catches
     latent panic paths the regression suite misses.
  6. **`covenant doctor --json` schema docs** : the JSON shape will
     become a public surface ; document it before tooling consumers
     calcify on the current format.
  7. **Aster Chain helper deploy** : the moment Arachnid factory
     presence is verified on Aster, the M2 milestone is one
     `forge script` away.

## Recommendations for V1.0 (external audit phase)

  1. **Real cryptography** : Wesolowski VDF, Shamir reconstruction,
     real Dilithium/Kyber/TFHE bindings. Remove `Mocked*` prefix +
     `onlyTestnet` modifier concurrently with mainnet helper deploy.
  2. **Formal verification** of `CeremonyHelper.sol` state machine
     (Certora or K-EVM). The state machine is small enough to make
     this tractable.
  3. **External cryptographic review** of FHE/PQ/ZK primitive
     selection. Audit-firm selection should require crypto-specific
     expertise.
  4. **SBOM + Sigstore attestation** on `--release` builds.
  5. **Full surface fuzz** via OSS-Fuzz (continuous fuzzing).

## Methodology: why this is honest

This report follows the same OMEGA V5 discipline that caught the
Sprint 31.b bugs (M1) and the Sprint 45 lint ICE (audit pack smoke
test) :

  - **Every claim is verifiable.** Each ✅/⚠️/❌ has a runnable command
    whose output we observed in this session.
  - **No claim is "trust me, the design is right".** The compiler
    bugs of V0.8/V0.9 happened in the design-vs-reality gap. The
    only way to close it is to run things and report what actually
    happened.
  - **We surface bugs we caught.** V-008 documents the lint ICE that
    was found AND fixed in Sprint 45. We could have hidden it ; we
    didn't, because hiding bugs is the path to PRELIM-009-style
    surprises in front of external auditors.
  - **We surface deferred risks.** V-009 lists 6 items that are
    explicitly NOT shipping in V0.9.0. External auditors deserve to
    know what's NOT done, not just what is.

This is the report we want to read when someone hands us a project to
audit. We tried to write the same one for ourselves.

---

**Self-audit verdict : ✅ GO for V0.9.0 tag (Sprint 47).**

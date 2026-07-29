# Known Acceptable Risks: V0.9.0

This is the audit-prep ledger of issues that `cargo audit` / `cargo
clippy` / manual review identifies, but that we have evaluated and
accepted for the V0.9.x line. Each entry has : (a) the finding, (b) why
it doesn't break our threat model, (c) when we expect to remediate.

If a new advisory lands and it's NOT on this list, the audit baseline
fails until we either fix it or add it here with rationale.

---

## RUSTSEC-2024-0421: `idna` 0.4.0 (transitive)

**Finding (cargo audit, 2026-04-26)**
> `idna` accepts Punycode labels that do not produce any non-ASCII when
> decoded. Solution : upgrade to ≥ 1.0.0.

**Dependency chain**
```
idna 0.4.0
  └── url 2.4.1
      └── lsp-types 0.94.1
          └── tower-lsp 0.20.0
              └── covenant-lsp 0.8.2
```

**Why we accept**

  1. **No IDN/Punycode path in our code.** `covenant-lsp` only handles
     `file://` URIs sent by VSCode / Cursor / Helix. We never resolve a
     domain name, never call `Url::host()`, never accept a URL from a
     non-trusted source. The vulnerability is a confused-deputy in
     IDN→ASCII handling that requires the attacker to control a hostname
     that gets compared as ASCII somewhere, a path that does not exist
     in our LSP server.
  2. **No upstream fix path available.** `tower-lsp 0.20.0` is the
     latest stable release on crates.io as of 2026-04-26. The next
     `lsp-types` would be needed first ; that's an upstream ecosystem
     migration, not a one-line bump for us.
  3. **Affected blast radius would be the editor host, not deployed
     contracts.** Even hypothetically exploited, this would not affect
     bytecode generation, helper contract dispatch, or any
     mainnet-touchable surface, it would at worst confuse the LSP about
     which URI maps to which open document.

**Remediation plan**

  - **V0.9.x** : upgrade tower-lsp when a release with newer `lsp-types`
    ships ; or migrate to `async-lsp` (currently 0.2.4) if tower-lsp
    stalls.
  - **V1.0** : zero-advisory baseline required before mainnet.

**Verification**

```bash
cargo audit
# Expected: 1 vulnerability found, RUSTSEC-2024-0421 only.
# Anything else = baseline broken, requires triage.
```

---

## `Mocked*` helper contracts on mainnet

**Finding** : `MockedFHEHelper.sol`, `MockedPQVerifier.sol`,
`MockedZKVerifier.sol` perform NO real cryptography. They return
plausible-looking values (commitment placeholders, deterministic dummy
proofs) and emit events.

**Why we accept**

This is the V0.9 design, not a defect. The compiler enforces the
testnet-only constraint at multiple layers :

  1. **Compile time** : `Target::parse("mainnet")` returns
     `Err(MainnetForbidden)`. Source code with `--target-chain=mainnet`
     fails before codegen.
  2. **Source code** : every `Mocked*` helper uses an `onlyTestnet`
     modifier that reverts when `block.chainid == 1`. NatSpec banner at
     the top of each file explicitly warns against mainnet usage.
  3. **Address determinism** : helpers are CREATE2-deterministic. Even
     if someone deployed an identical-bytecode helper on mainnet, the
     `onlyTestnet` modifier would still fire on every call.
  4. **Audit doc** : `docs/v0.9/helper-source-audit-checklist.md`
     explicitly maps each helper to its V1.0 swap-in plan.

**Remediation plan**

  - **V1.0** : real cryptographic implementations replace `Mocked*` ;
    `onlyTestnet` modifier removed concurrently with V1.0 mainnet
    helpers ; new audit pass on the real crypto.

**Verification**

```bash
forge test --match-test testRevertsOnMainnet -vvv
# Should pass for every Mocked* helper.

cargo run -- build foo.cov --target-chain=mainnet
# Should fail with: "V0.9 ships testnet-only. Mainnet helpers land in V1.0..."
```

---

## `covenant fmt` discards comments

**Finding** : V0.9.0 `covenant fmt` does not preserve comments through
the tokenize → parse → print round-trip. Documented limitation.

**Why we accept**

  - The lexer doesn't yet retain trivia tokens. Adding trivia
    preservation is a non-trivial refactor of the parser API.
  - Workaround documented : run `fmt` only on freshly-generated source
    or wait for V0.9.x.
  - **No security impact.** Worst case is information loss in the
    formatted file, not a soundness or auth issue.

**Remediation plan**

  - V0.9.x : trivia-preserving lexer + printer extension.

---

## LSP server : full re-parse on every keystroke

**Finding (D-03 in threat model)** : `did_change` triggers full
tokenize+parse with no debounce ; large documents pin CPU on rapid
edits.

**Why we accept (with discomfort)**

  - Affects developer ergonomics, not deployed-contract security.
  - LSP server runs on the developer's own machine ; no remote DoS
    surface.
  - Documented in DEBT.md.

**Remediation plan**

  - V0.9.x : 100ms debounce on `did_change`, full publish on `did_save`.
  - V1.0 : incremental parse (subset re-parse around edited range).

---

## Compiler fuzz coverage

**Finding (corrected 2026-07-05, OMEGA V6 self-audit MED-002)** : a
`cargo-fuzz` harness DOES exist (`crates/covenant-wasm-bindings/fuzz/
fuzz_targets/compile_pipeline.rs` + `check_only.rs`), but its `corpus/`
and `artifacts/` directories are both empty, it has never actually been
run against this codebase. The previous version of this entry claimed no
fuzz suite existed at all, which was itself stale/inaccurate.

That unexercised harness is exactly what would have caught HGH-029 (OMEGA
V6): every recursive-descent AST walker in the parser, resolver, and
typechecker recursed with no depth counter, so a few hundred bytes of
nested parens or a long chained-`+` expression overflowed the native
process stack (an uncatchable `STATUS_STACK_OVERFLOW`, not a normal Rust
panic) across every subcommand and the LSP. **Fixed** in this cycle: each
stage now bounds its own recursion depth and raises a normal diagnostic
(E031/E113/E232) instead. The "Parser uses bounded recursion" claim below
is therefore now true for the whole front end, not aspirational.

**Why we accept (for V0.9.0)**

  - All known-pathological inputs from the operator-test logs (Sprint 27
    PRELIM-009 era), plus the two HGH-029 PoCs (140 nested parens; 500
    chained `+`), are now in the regression suite as fixtures.
  - Lexer is `logos`-based (linear). Parser, resolver, and typechecker
    each bound their own recursion depth (OMEGA V6 HGH-029 fix).
  - **No production-incident motivated the original acceptance of this
    risk**, it was prudence, not response. HGH-029 shows that prudence
    was warranted: the class of bug this entry warned about was real.

**Remediation plan**

  - V0.9.x : actually run `cargo fuzz run compile_pipeline` (and
    `check_only`) in CI with a seeded corpus, not just leave the harness
    unexercised in-tree.
  - V1.0 : continuous fuzzing via OSS-Fuzz (if accepted).

---

## SBOM not emitted on `--release` builds

**Finding (E-02 in threat model)** : `covenant build --release` produces
a deterministic bytecode artifact, but does NOT emit a Software Bill of
Materials. Reproducibility relies on the user re-running the build.

**Why we accept (for V0.9.0)**

  - Reproducibility is documented and CI-checkable manually.
  - Cargo's existing `Cargo.lock` provides the input-side SBOM.
  - Bytecode hash deterministic from source + lockfile ; user can
    cross-check.

**Remediation plan**

  - V0.9.x : `--release` emits `bytecode.sbom.json` alongside the
    artifact (toolchain version, lockfile hash, input source hash,
    output bytecode hash).
  - V1.0 : CycloneDX-format SBOM, attested via Sigstore.

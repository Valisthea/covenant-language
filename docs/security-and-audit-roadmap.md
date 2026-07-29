# Security and audit roadmap

What review Covenant has actually had, what it has not, and in what order that
changes. Written so a risk or governance function can place the project without
having to infer anything from marketing language.

---

## Where this sits today

**No independent firm has audited Covenant.** An external audit by an independent
security team is the gate for V1.0, and it has not happened.

**There is no formal verification.** No proof framework, no model checking, no
mechanised proof of any property of the compiler. Formal verification appears on
the long-horizon roadmap as a goal. It has never been a present claim, and where
it was previously stated as one, that was an error and has been retracted.

**The cryptographic primitives are placeholders.** The FHE, zero-knowledge and
post-quantum machinery is stubbed. The privacy type system is real and enforced
at compile time, but the primitives behind it have no security property today.

**Testnet only.** Mainnet targets are refused at compile time. Deployed tokens
carry zero value and exist as compiler evidence, not as products.

---

## What review has happened

Four internal adversarial review cycles, all run by Kairos Lab against its own
compiler using its own tooling. **These are self-reviews, not third-party audits,
and they carry none of the independence of one.** The full archive is published
so the methodology can be judged from outside:
[covenant-security-reviews](https://github.com/Valisthea/covenant-security-reviews).

| Cycle | Target | Outcome |
|---|---|---|
| Adversarial bounty, 2026-07-23 | v0.9.4 | 1 Critical, 2 High, 4 Medium, 2 Low. All fixed in v0.9.5 |
| OMEGA V6, 2026-07-05 | v0.9.2 | 6 Critical, 6 High, 5 Medium, plus Low and Info. Fixed in v0.9.3 |
| OMEGA V5, April 2026 | v0.9.0 | Gate review before the tag. Go, with conditions recorded |
| OMEGA V4, April 2026 | v0.8 | **Incomplete.** Stopped after phase 2, issued no verdict |
| OMEGA V4, April 2026 | v0.6 | 40 graded findings plus 1 withdrawn. All resolved |

Two things in that table are worth reading twice. The v0.8 cycle was never
finished, so it is evidence of nothing. And the v0.9.2 cycle found six Critical
defects that **four earlier cycles had missed**, including a loop construct that
ran its body exactly once, demonstrated live in this project's own shipped
example fixture. Absence of a finding has repeatedly not meant absence of a
defect here.

---

## What actually backs the fail-loud claim

The claim is narrow: when the compiler reaches a construct it cannot lower
faithfully, it raises a compile error instead of emitting bytecode that merely
looks plausible. It is a refusal mechanism, not a correctness proof. What
supports it:

- **1,105 tests** across the workspace, run in CI on every change.
- **Negative-control verification.** Every regression test guarding a fail-loud
  diagnostic is checked for non-vacuity: neutralise the guard and the test must
  fail. This prevents the common failure where a suite is green because it never
  exercises what it claims to cover.
- **Empirical reproduction is mandatory in review.** Reading source is not
  accepted as evidence for a compiler. A finding is recorded only once reproduced
  against the real compiled binary and, where it concerns emitted code, by
  executing that bytecode on a chain.
- **Adversarial re-verification.** Every candidate finding is handed to a separate
  pass instructed to refute it. In the v0.9.2 cycle that killed 3 candidates. In
  the v0.9.4 bounty it killed 14 of 25.
- **A fuzz harness** over the compile pipeline, in CI. It is not decorative: it
  found the over-long text constant crash that shipped as diagnostic `E521`, and
  the crashing input is kept in the corpus so every run replays it.
- **`cargo clippy` with warnings denied**, and `cargo fmt`, both gating.

---

## Known open items

The authoritative list is `DEBT.md`. The items most likely to matter to an
evaluator:

- **Dynamic `bytes` and `string`** storage and ABI encoding are incomplete. This
  is what hard-blocks the `registry` construct via diagnostic `E505`.
- **No existence check on helper contracts.** The compiler bakes helper addresses
  into bytecode as immediates with no compile-time or runtime check that code is
  actually deployed there. A call into an empty address fails at runtime rather
  than at build time. This is a defect against this project's own fail-loud
  principle, not a design choice.
- **Two hardcoded precompile selectors** bypass the `E520` gate that is supposed
  to catch exactly that class of mismatch.
- **The helper contracts' mainnet gate only rejects chain id 1**, so it does not
  protect a non-mainnet chain that nonetheless carries value.
- **The compiler is not gas aware.** Every emitted call pushes a literal large
  gas value.

---

## Sequence

Deliberately ordered by trust rather than by features.

**V1.0 is an external audit.** Not a feature release. The gate is handing the
compiler to an independent security team and publishing what they find. This is
currently **unfunded**, so there is no timeline, and none will be published until
there is one. Auditing a compiler is not the same scope as auditing a single
contract: the surface is the whole toolchain that produces the bytecode, which
means a firm with compiler and EVM codegen expertise spending real weeks on it.
That is a budget an unfunded solo project does not have. If it becomes possible,
the scope, the auditors and the full report will be public, including whatever
they find.

**V1.0 also requires** the dynamic `bytes` and `string` debt closed, and helper
contracts deployed on the target chain.

**V2.0 is real cryptography.** Replacing the mocked FHE, zero-knowledge and
post-quantum primitives with production implementations, on its own track and
held to its own bar. It is explicitly **not** a V1.0 condition. Likely ordering
is post-quantum signature verification first as the most tractable,
zero-knowledge verification next, FHE last. No dates.

One caveat worth stating in advance: real primitives have gas and code size
characteristics the placeholders do not, so the swap will affect feasibility and
cost, not just addresses. Anyone adopting early should treat the cryptographic
constructs as experimental and the plain EVM subset as the stable surface.

---

## Reporting something

See `SECURITY.md`. Reports go to `admin@kairos-lab.org`. Given the project is
testnet only with nothing at stake financially, there is no embargo theatre: if
you find something, we would rather fix it and write it up than manage it.

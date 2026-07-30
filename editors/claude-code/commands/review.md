---
name: covenant-review
description: >-
  Defensive review of a Covenant v0.9.7 .cov file or selection. Checks against
  covenant-syntax rules, docs/diagnostic-codes.md, and erc-822x rules. Outputs
  structured findings by severity with suggested fixes. Usage: /covenant-review [file.cov]
---

# /covenant-review

Run a defensive review of Covenant v0.9.7 code.

## Usage

```
/covenant-review [path/to/contract.cov]
```

If no argument is given, operate on the currently selected code in the editor.

## Behavior

1. **Read** the `.cov` source in full.
2. **Check the Syntax section of `CLAUDE.md`**: flag all violations. This plugin ships its
   rules in `CLAUDE.md`, which Claude Code loads automatically; the `rules/*.mdc` files are
   the Cursor edition's copy of the same guidance and are not part of this package.
3. **Check `docs/diagnostic-codes.md`**: match patterns against the full lint catalog;
   cite the diagnostic code (e.g., `E110`) in each finding where applicable. Codes carry a
   minimum of three digits and no padding beyond that: the compiler prints `[E003]` and
   `[E110]`, never `E0110`. Cite the prefix the compiler actually emitted, not the one the
   catalog constant is named after: an unresolved `only` principal prints `[W421]`.
4. **Check the ERC-822x Compliance section of `CLAUDE.md`**: verify ERC-8227/8228/8229/8231 citation conformance (note: a `ceremony` maps to ERC-8228, Cryptographic Amnesia, Styx Protocol)
   wherever the corresponding constructs or guards appear.
5. **Output structured findings**: one block per issue, then a summary table.

## Output format

```
File:       <path or "(selection)">
Construct:  <top-level keyword and name>

FINDINGS
────────────────────────────────────────────────────────────────────
[HIGH-001]  <Title>
  Lines:     <start>, <end>
  Rule:      <rule-id or diagnostic code>
  Violation: <one sentence>
  Fix:       <concrete suggested replacement>

[MED-001]   <Title>
  ...

[LOW-001]   <Title>
  ...

[INFO-001]  <Title>
  ...

SUMMARY
────────────────────────────────────────────────────────────────────
  High:   N
  Medium: N
  Low:    N
  Info:   N
  Total:  N

PASSED (no issues found in)
  - <list of constructs / sections that passed all checks>
```

## Severity definitions

| Severity | Meaning |
|----------|---------|
| `high` | Potential fund loss, unauthorized state mutation, or critical guard missing |
| `medium` | Logic gap, access-control weakness, or missing guard for a reachable condition |
| `low` | Minor correctness issue, type mismatch, or deprecated pattern |
| `info` | Style / best practice, missing ERC citation comment, or naming inconsistency |

Anything the compiler rejects outright is at least `high`, whatever its surface looks like: a
file that does not build cannot be reviewed further, and `covenant lint` skips it entirely
(`covenant-lint: 1 compile error(s) in <file>: skipping`). Never file a hard diagnostic as
`info`.

## Standard checks

| Check | Rule / Code |
|-------|-------------|
| `//` or `/* */` used as comment | `covenant-syntax` |
| `mapping(...)` syntax | `covenant-syntax` |
| `function` keyword | `covenant-syntax` |
| `require(...)` in action body | `covenant-syntax` |
| `uint256` type | `covenant-syntax` |
| `msg.sender` | `covenant-syntax` |
| `string` instead of `text` | `covenant-syntax` |
| `constructor` instead of `initialize` | `covenant-syntax` |
| `@non_reentrant`, or any annotation outside the valid set | `covenant-syntax` (high), E110 |
| `now` used as `amount` (type error) | `covenant-syntax` |
| `now + N` without duration literal | `covenant-syntax` |
| `confidential token` missing ERC-8227 `--` comment | `erc-822x` |
| `ceremony` missing its ERC-8228 (Cryptographic Amnesia) citation | `erc-822x` |
| `verified_by` missing ERC-8229 `--` comment | `erc-822x` |
| `pq_signed` missing ERC-8231 `--` comment | `erc-822x` |
| State-mutating action emits no event | `covenant-syntax` (low) |
| Access guard missing for privileged action | `covenant-syntax` (medium) |
| Bare revert (no typed `error` + `revert_with`) | `covenant-syntax` (low) |
| Diagnostic codes from `docs/diagnostic-codes.md` | Cite code per finding |

### Note on `@non_reentrant`

`@non_reentrant` is not a Covenant annotation at v0.9.7. The resolver rejects it with a hard
error in every construct, `vault` included, so a source file carrying it does not compile at
all and `covenant lint` refuses to analyze the file:

```
[E110] Error: unknown annotation `@non_reentrant`
   Help: valid annotations: `@precompute`, `@batch_up_to`, `@prove_offchain`, `@gas_budget`
error: compilation failed (1 error(s))
```

Those four are the complete valid set. Report the annotation as `high`, not `info`, and remove
it: it is a build-stopper, not a style nit.

The annotation is not "redundant" either, because `vault` synthesizes no reentrancy guard at
v0.9.7. A `vault` compiles with `[W606] Warning: vault standard-interface synthesis not yet
implemented` and the construct is passed through unchanged, byte for byte the same code the
equivalent `module` would produce. If a withdraw path needs reentrancy safety, it has to come
from the ordering of the body, debiting state before `transfer(value) to caller`, not from an
annotation and not from the choice of construct.

## Scope

This command performs **defensive review only**:

- Checks syntax correctness, anti-patterns, type misuse, missing guards, and ERC conformance.
- Does **not** construct exploits, attack payloads, or offensive tooling of any kind.
- References the public lint catalog (`docs/diagnostic-codes.md`) exclusively.
- Does not name or reference internal audit methodologies.

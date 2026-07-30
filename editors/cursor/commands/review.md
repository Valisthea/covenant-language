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
2. **Check `rules/covenant-syntax.mdc`**: flag all violations.
3. **Check `docs/diagnostic-codes.md`**: match patterns against the full lint catalog;
   cite the diagnostic code (e.g., `E0421`) in each finding where applicable.
4. **Check `rules/erc-822x.mdc`**: verify ERC-8227/8228/8229/8231 citation conformance (note: a `ceremony` maps to ERC-8228, Cryptographic Amnesia, Styx Protocol)
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
| `info` | Style / best practice, missing ERC citation comment, or redundant annotation |

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
| Redundant `@non_reentrant` on `vault` | `covenant-syntax` (info) |
| `if/else` in action body (V0.9) | `covenant-syntax` |
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

## Scope

This command performs **defensive review only**:

- Checks syntax correctness, anti-patterns, type misuse, missing guards, and ERC conformance.
- Does **not** construct exploits, attack payloads, or offensive tooling of any kind.
- References the public lint catalog (`docs/diagnostic-codes.md`) exclusively.
- Does not name or reference internal audit methodologies.

# Covenant Diagnostic Codes

*Source of truth for every diagnostic code the compiler and linter emit.*
*When adding, renaming, or removing a code, update this file in the same
commit. The registry self-test in `crates/covenant-lint/src/framework/registry.rs`
and the scope doc referenced below both cross-check against this inventory.*

Prefix conventions:

- `E`: **E**rror (hard fail, blocks compilation)
- `W`: **W**arning (non-blocking advisory)
- `I`: **I**nfo (hint; lowest severity)
- `C`: **C**ritical lint (lint-only class; critical severity)

## Frontend (lexer, parser, resolver, typecheck, privacy)

| Code | Crate | Meaning |
|---|---|---|
| E001 to E099 | covenant-lexer | Lexer failures (unterminated string, bad escape, …). |
| E100 to E199 | covenant-parser | Parser failures (unexpected token, missing delimiter, …). |
| E200 to E299 | covenant-resolver | Name resolution (unknown identifier, double binding, …). |
| E300 to E399 | covenant-types | Type checker (arity mismatch, cast to non-compatible, …). |
| E380 to E389 | covenant-privacy | Privacy taint / domain violations. |

Frontend codes pre-date this inventory and are considered stable; changes
require a Covenant-SPEC-level update.

## IR builder (E401 to E420 + extensions)

| Code | Meaning |
|---|---|
| E401_SSA_DOMINANCE | SSA value used before defined. |
| E402_BLOCK_NO_TERMINATOR | Block has no terminator. |
| E403_USER_CALL | User-defined call at IR level (not supported in V0). |
| E404_LAMBDA_UNSUPPORTED | Lambda expression in lowerable position. |
| E405_BLOCK_ARG_MISMATCH | Block-argument arity mismatch. |
| E406_UNKNOWN_EVENT | Event name not declared in scope. |
| E407_UNKNOWN_ERROR | Error type not declared. |
| E408_FOREACH_NOT_LIST | `for` over a non-list value. |
| E409_UNEXPECTED_STMT | Statement unsupported in this position. |
| E410_OPCODE_ARITY | Wrong number of operands for opcode. |
| E411_CIPHERTEXT_TO_PLAINTEXT | Implicit conversion from ciphertext to plaintext. |
| E412_UNLOWERABLE_EXPR | Expression shape cannot be lowered. |
| E413_UNKNOWN_STRUCT_FIELD | Struct literal references missing field. |
| E414_MAP_ON_NON_MAP | Map-op used on non-Map type. |
| E415_MISSING_STDLIB_CALL | Stdlib call target not found. |
| E416_SELECTIVE_DISCLOSURE_DEFERRED | Selective disclosure not yet lowered. |
| E417_ONDESTROY_UNKNOWN_FIELD | `ondestroy` references unknown field. |
| E418_MIGRATE_UNKNOWN_FIELD | `migrate` references unknown field. |
| E419_FHEBRANCH_PHI_MISSING | FHE branch block without phi argument. |
| E420_FHEBRANCH_NO_MERGE | FHE branch has no merge block. |
| E421_GUARD_UNRESOLVED_PRINCIPAL | `only(principal)` cannot be lowered; fail-closed. |
| E422_SLOT_ANNOTATION_INVALID | `@slot(...)` malformed. |
| E423_SLOT_ANNOTATION_CONFLICT | Two fields assigned to the same slot. |
| W850_UNKNOWN_ANNOTATION | Unknown annotation name (KSR-CVN-030). |

## EVM backend (E501 to E515, W501 to W506, …)

See `crates/covenant-evm-backend/src/diag.rs`. Summary:

| Range | Meaning |
|---|---|
| E501 to E510 | Stack depth, unknown opcodes, precompile unset, storage overflow. |
| E511 to E515 | AssertEncrypted shape, event topic limits, unresolved jump label. |
| W501 to W506 | Large memory / storage / runtime, selector near-collision. |

## Linter: detector codes (category prefixes)

Each lint detector has a code composed of a prefix (`C` / `W` / `I`) and a
numeric class. See `crates/covenant-lint/src/detectors/`. **Adding a new
detector requires a new row here AND registration in `registry.rs`; the
registry's self-test will fail otherwise.**

### REE: Reentrancy

| Code | Severity | Name |
|---|---|---|
| C001 | Critical | State mutation after external transfer. |
| C002 | Critical | Transfer inside a loop. |
| W003 | Warning  | Action with external call but no `@non_reentrant`. |
| I004  | Info     | Transfer inside initializer/constructor. |

### ACC: Access Control

| Code | Severity | Name |
|---|---|---|
| C100 | Critical | Privileged action without access guard. |
| C101 | Critical | `block.*` condition used as authorization. |
| W102 | Warning  | Admin field with no timelock. |
| W103 | Warning  | Owner-zero deployment risk. |
| I104  | Info     | Single-step ownership transfer. |

### EXT: External Calls

| Code | Severity | Name |
|---|---|---|
| C300 | Critical | Transfer to zero address. |
| C301 | Critical | Unchecked transfer parameter (tainted destination). |
| W302 | Warning  | Transfer inside a loop. |
| W303 | Warning  | No `ensure` before transfer. |
| I304  | Info     | Transfer without event emission. |

### GAS: Gas / DOS

| Code | Severity | Name |
|---|---|---|
| C1100 | Critical | Unbounded loop. |
| W1101 | Warning  | Expensive view. |
| W1102 | Warning  | Storage write inside a loop. |
| I1103 | Info     | Very high instruction count. |

### TIM: Timestamp

| Code | Severity | Name |
|---|---|---|
| W1200 | Warning | Timestamp used as randomness. |
| W1201 | Warning | Block number in a branch condition. |
| I1202 | Info    | Timestamp dependency. |

### PQ: Post-Quantum (Session 2, KSR-CVN-024)

| Code | Severity | Name |
|---|---|---|
| C700 | Critical | `PqVerifyDilithium` without a chain-bound nonce. |

### AMN: Amnesia Ceremony (Session 2, KSR-CVN-025)

| Code | Severity | Name |
|---|---|---|
| C801 | Critical | Ceremony phase backward transition (phase ≥ 1 → phase 0). |

## Intentionally-reserved ranges

The scope document in `audits/2026-04-22-omega-v4-covenant-v0.6/` references
several code ranges for planned-but-not-yet-implemented detectors. These are
**reserved**; implementations must reuse the indicated code rather than
inventing a new one.

| Range | Category | Status |
|---|---|---|
| C600 | Privacy domain escape | Reserved, not yet implemented. |
| E820 to W826 | Privacy diagnostics (P7) | Reserved, phase-7 follow-up. |
| E830 to W836 | Proxy / upgradeability | Reserved, blocked on proxy infra. |
| E831 | Proxy slot collision | Reserved, blocked on proxy infra. |

When any of these codes lands, move its row from this "reserved" table into
the relevant active-detector table above and add it to the registry
self-test's `required` list.

## Audit trail

- 2026-04-22, KSR-CVN-004 introduced this inventory.
- 2026-04-22, KSR-CVN-005 bound it to the registry via a self-test.
- 2026-04-22, KSR-CVN-030 added `W850_UNKNOWN_ANNOTATION`.

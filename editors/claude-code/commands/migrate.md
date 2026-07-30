---
name: covenant-migrate
description: >-
  Migrate a Solidity .sol file (or current selection) to Covenant v0.9.7 .cov,
  applying the 11 anti-pattern transformations and selecting the most specialized
  top-level construct. Usage: /covenant-migrate [path/to/Contract.sol]
---

# /covenant-migrate

Migrate Solidity code to Covenant v0.9.7.

## Usage

```
/covenant-migrate [path/to/Contract.sol]
```

If no argument is given, operate on the currently selected code in the editor.

## Behavior

### Step 1: Read

Read the `.sol` source in full (file or selection). Do not truncate.

### Step 2, Classify: pick the most specialized Covenant construct

| Solidity pattern | Covenant construct |
|------------------|--------------------|
| ERC-20 (`transfer`, `approve`, `balanceOf`, `transferFrom`) | `token` |
| ERC-20 with FHE-encrypted balances | `confidential token` |
| `nonReentrant` + ETH custody + `withdraw` | `vault` |
| Voting / poll contract | `ballot` |
| Simple counter (`count++`) | `counter` |
| Append-only event log / message list | `board` (schema only: it cannot append or read posts at v0.9.7, see Notes) |
| Order book / matching engine | `market` (read side only: its queues have no insert, see Notes) |
| Address / public key registry | `module` (never `registry`: it does not compile at v0.9.7, see Notes) |
| Cross-chain bridge | `bridge anchored_on [...]` |
| Secret sharing / key ceremony | `ceremony` |
| Generic, none of the above | `module` (last resort) |

Do **not** default to `module` if a more specific keyword fits.

### Step 3: Apply the 11 anti-pattern transformations

Apply each transformation throughout the migrated output:

| # | From (Solidity) | To (Covenant) |
|---|-----------------|----------------|
| 1 | `mapping(K => V) public xs;` | `field xs: map<K, V>` |
| 2 | `function name(...) public { }` | `action name(...) { }` |
| 3 | `function name(...) public view returns (T) { }` | `view name(...) returns T { }` |
| 4 | `require(cond, "msg");` | `when cond` guard on the action signature |
| 5 | `// comment` | `-- comment` |
| 6 | `contract Foo { }` | appropriate top-level keyword |
| 7 | `uint256` | `amount` |
| 8 | `modifier onlyOwner() { _; }` | `only owner` inline guard |
| 9 | `msg.sender` | `caller` |
| 10 | `public` / `external` / `internal` / `private` on functions | (remove) |
| 11 | `constructor(...) { }` | `action initialize(...)` (or `supply: N to deployer` for tokens) |

Additionally:
- `string` → `text`
- `payable` → (remove everywhere; the keyword does not parse. `payable action bump(...)`
  gives E020, "expected `:` or `{` after identifier, got identifier `payable`".)
  Actions are implicitly **non**-payable: across every ABI this release emits for the
  building templates, 63 entries scanned, `stateMutability: "payable"` appears 0 times
  and no construct emits a `receive` or `fallback`. A `vault`'s own `deposit` comes out
  `"stateMutability":"nonpayable"`. So a Solidity `payable deposit()` migrated by
  deleting the keyword reverts on any value-bearing call; flag it with
  `-- TODO(migrate): was payable, v0.9.7 emits nonpayable only, this action cannot receive value`
- `import "...";` → (remove, stdlib is auto-available in v0.9.7)
- `nonReentrant` modifier → (remove from **every** construct, `vault` included, and always leave a TODO: v0.9.7 has no reentrancy guard at all, see Notes)
- `only owner` (row 8) needs a `field owner: address` declared in the same construct. Without it the build still succeeds, but the compiler warns W421, "`only owner` (no `field owner` declared) cannot be enforced at runtime; the action will revert on every call", so migrate Solidity's `owner` state variable as well and assign it in `initialize`.
  The enforced principals at v0.9.7 are `owner`, `admin`, `deployer` and an explicit
  address literal. `only admin` behaves exactly like `only owner`: clean build with
  `field admin: address` declared, W421 without it. A literal such as
  `only 0x00000000000000000000000000000000000000A1` builds with no diagnostic. `only first_time_caller` and `only registered_key` are refused with E518,
  whose own text names those four as the alternatives that ARE enforced.
- `emit Transfer(...)` syntax is identical; ensure the `event Transfer(...)` declaration uses Covenant arg syntax, and note that `from` and `to` are reserved words, so an arg cannot be named after them
- `revert "msg"` / `require(false, "msg")` → `revert_with ErrorName(args)` (define typed `error`)

### Step 4: Preserve logic fidelity

Every state mutation, event emission, and error condition in the original must
appear in the output. If a pattern has no direct equivalent (e.g., `int256`),
flag it with a `-- TODO(migrate):` comment and the closest v0.9.7 idiom:

- `int256` → `amount` with a `-- TODO(migrate): signed arithmetic unavailable at v0.9.7, int256 is E102` note
- `nonReentrant` → drop the modifier and add `-- TODO(migrate): reentrancy guard dropped, no Covenant equivalent at v0.9.7` on the action

In-body `if`/`else` is **not** one of those patterns. It compiles directly at
v0.9.7, through the full `build` pipeline, with no diagnostic, so migrate it
one-for-one and emit no TODO. Parentheses around the condition are optional:
`if value > 100 { … }` and `if (value > 100) { … }` both build, and `else if`
chains build. Two things to avoid:

- Do **not** rewrite it as a `when` guard. A `when` guard reverts when its
  condition is false, it has no second branch, so the else-path state mutation
  would be silently lost. Written out, it does not even parse:
  `action bump(value: amount) when value > 100 { … } else { … }` gives
  `[E020] Error: unexpected token: KwElse` (and `otherwise` in that position
  gives `[E020] Error: unexpected token: KwOtherwise`).
- Use `encrypted_when … otherwise` only when the condition is over FHE-encrypted
  values. Over a plaintext condition it still builds but the compiler emits
  W306, "`encrypted_when` condition is already plaintext; use `if` instead",
  which is the exact opposite of restructuring `if` into `encrypted_when`.

### Step 5: Output

- Write the result to `<original-stem>.cov` alongside the source file.
  Example: `contracts/Vault.sol` → `contracts/Vault.cov`
- **Do not delete or modify the original** `.sol` file.
- Print the path written and a one-line summary:
  ```
  Wrote contracts/Vault.cov
  Construct: vault (selected over module, ETH custody + nonReentrant pattern)
  Anti-patterns applied: 11/11
  TODOs: 1 (reentrancy guard dropped, no v0.9.7 equivalent)
  ```

  A migration that dropped a `nonReentrant` modifier always reports at least one
  TODO. Reporting `TODOs: 0` on such a contract is a bug in the migration.

## Notes

- `vault` applies **no** reentrancy guard at v0.9.7, and you cannot add one.
  Three facts, each produced by running the compiler:
  1. A `vault` body and the same body written as `module` compile to
     byte-identical runtime bytecode. `vault` currently warns W606,
     "vault standard-interface synthesis not yet implemented", and adds nothing
     to codegen.
2. `covenant lint` raises W003, "action `withdraw` makes an external call
   with no reentrancy protection, and this release has none to offer", on the
   vault itself. Its help says to write every state change before the
   transfer.
  3. That annotation cannot be written. `@non_reentrant` is rejected with
     `[E110] Error: unknown annotation`, on every construct, at `check` as well
     as at `build`. The annotations accepted at v0.9.7 are exactly
     `@precompute`, `@batch_up_to`, `@prove_offchain` and `@gas_budget`.

  So a Solidity `nonReentrant` modifier has no equivalent in this release.
  Remove it, never claim `vault` replaces it, and leave
  `-- TODO(migrate): reentrancy guard dropped, no Covenant equivalent at v0.9.7`
  on every action that performs an external call. Expect this to be visible
  downstream: W003 alone is enough for `covenant lint` to exit non-zero with
  "linter found critical security findings, release build blocked", and since
`build` still succeeds. The finding can be silenced with a comment,
`-- @allow(W003, reason: "...")`, which the linter reads from the source text
and the parser ignores. Do not do that on a migration: hand it to a human.
- `registry` does not compile at v0.9.7, in any form. The ERC-8231 synthesizer
  injects `register` and `key_of` over `pq_key` (dynamic ABI `bytes`), which this
  release's codegen refuses, with E505: "`register` uses `pq_key` (ABI type
  `bytes`, dynamic) in a position that this release's codegen can only
  read/return as a single 32-byte word". Even `registry R { }` with no fields
  fails, two E505 errors and no artifact, and there is no escape hatch:
  declaring your own gives E601, "user-declared function `key_of` conflicts with
  ERC-8231 synthesis". `check` passes on a registry and only `build` surfaces
  this, so never accept a registry on `check` alone. Migrate address and
  public-key registries to `module` instead.
- `board` carries the post schema and nothing else at v0.9.7. `append post { .. }`
  fails with E430, "`post` has no storage field, so there is nowhere to write the
  element", and any read of the collection fails with E431, "`posts` has no storage
  field and cannot be read". Both pass `check` and only surface at `build`, so never
  accept a board on `check` alone. The post-schema-only board does build, to an empty
  contract: `ok: Demoboard: deploy 37 bytes, runtime 12 bytes`. If the Solidity
  contract actually stores and reads its log, `board` cannot carry it.
- `market` has a read side only. The `priority_queue<K, V, min|max>` fields expose
  `length`/`len`, `top_key`/`top` and `top_value`; anything else is E207, "value of
  type `priority_queue<amount, address, Min>` has no field `push`", and the same for
  `enqueue`, `insert` and `add`. So the queues cannot take an order. The view-only
  scaffold builds: `ok: Demomarket: deploy 197 bytes, runtime 172 bytes`.
- `payable` does not exist in Covenant; remove all occurrences.
- `import` is unsupported in v0.9.7; remove all import statements.
- `time` and `amount` are distinct types; `block.timestamp` → `now` (type `time`).
- `block.number` → `current_block` (type `amount`).
- `address(0)` → `zero_address`.

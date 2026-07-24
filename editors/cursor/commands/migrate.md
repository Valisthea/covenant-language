---
name: covenant-migrate
description: >-
  Migrate a Solidity .sol file (or current selection) to Covenant v0.9.5 .cov,
  applying the 11 anti-pattern transformations and selecting the most specialized
  top-level construct. Usage: /covenant-migrate [path/to/Contract.sol]
---

# /covenant-migrate

Migrate Solidity code to Covenant v0.9.5.

## Usage

```
/covenant-migrate [path/to/Contract.sol]
```

If no argument is given, operate on the currently selected code in the editor.

## Behavior

### Step 1 — Read

Read the `.sol` source in full (file or selection). Do not truncate.

### Step 2 — Classify: pick the most specialized Covenant construct

| Solidity pattern | Covenant construct |
|------------------|--------------------|
| ERC-20 (`transfer`, `approve`, `balanceOf`, `transferFrom`) | `token` |
| ERC-20 with FHE-encrypted balances | `confidential token` |
| `nonReentrant` + ETH custody + `withdraw` | `vault` |
| Voting / poll contract | `ballot` |
| Simple counter (`count++`) | `counter` |
| Append-only event log / message list | `board` |
| Order book / matching engine | `market` |
| Address / public key registry | `registry` |
| Cross-chain bridge | `bridge anchored_on [...]` |
| Secret sharing / key ceremony | `ceremony` |
| Generic, none of the above | `module` (last resort) |

Do **not** default to `module` if a more specific keyword fits.

### Step 3 — Apply the 11 anti-pattern transformations

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
- `payable` → (remove everywhere — all actions are implicitly payable)
- `import "...";` → (remove — stdlib is auto-available in V0.9)
- `nonReentrant` modifier → (remove from `vault` — it is the default; remove from others and note)
- `emit Transfer(...)` syntax is identical; ensure the `event Transfer(...)` declaration uses Covenant arg syntax
- `revert "msg"` / `require(false, "msg")` → `revert_with ErrorName(args)` (define typed `error`)

### Step 4 — Preserve logic fidelity

Every state mutation, event emission, and error condition in the original must
appear in the output. If a pattern has no direct equivalent (e.g., `int256`,
`if/else` in body), flag it with a `-- TODO(migrate):` comment and the closest
V0.9 idiom:

- `int256` → `amount` with a `-- TODO(migrate): signed arithmetic is V1.0` note
- `if/else` in body → restructure as `when` guards, or `encrypted_when … otherwise`
  for FHE branches; add `-- TODO(migrate): in-body if/else is V0.9` if restructure
  is not straightforward

### Step 5 — Output

- Write the result to `<original-stem>.cov` alongside the source file.
  Example: `contracts/Vault.sol` → `contracts/Vault.cov`
- **Do not delete or modify the original** `.sol` file.
- Print the path written and a one-line summary:
  ```
  Wrote contracts/Vault.cov
  Construct: vault (selected over module — ETH custody + nonReentrant pattern)
  Anti-patterns applied: 11/11
  TODOs: 0
  ```

## Notes

- `vault` is `@non_reentrant` by default — never add it explicitly.
- `payable` does not exist in Covenant; remove all occurrences.
- `import` is unsupported in V0.9; remove all import statements.
- `time` and `amount` are distinct types; `block.timestamp` → `now` (type `time`).
- `block.number` → `current_block` (type `amount`).
- `address(0)` → `zero_address`.

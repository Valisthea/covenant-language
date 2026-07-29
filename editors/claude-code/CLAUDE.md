# Covenant: Persistent Agent Guidance

This file is loaded automatically by Claude Code whenever the plugin is active.
It merges both rule sets from the Cursor plugin into a single guidance document.

---

## Syntax (applies whenever editing .cov files)

Apply these rules whenever editing or generating `.cov` source files.

### Comments

- Use `--` for single-line comments. **Never** `//`.
- Use `(* ... *)` for block comments (nestable). **Never** `/* ... */`.
- Both `//` and `/* */` are rejected by the compiler with a dedicated diagnostic.

### Top-level construct selection

Prefer the most specialized construct over generic `module`:

| Use case | Preferred construct |
|----------|---------------------|
| Fungible token (public balances) | `token` |
| Fungible token (FHE-encrypted balances) | `confidential token` |
| Value custody / escrow | `vault` |
| On-chain voting | `ballot` |
| Cryptographic amnesia / secret-sharing | `ceremony` |
| Identity or key directory | `registry` |
| Append-only message log | `board` |
| Order book / marketplace | `market` |
| Cross-chain escrow | `bridge` |
| Single-value counter (plaintext) | `counter` |
| Single-value counter (FHE-encrypted) | `encrypted counter` |
| Key-value storage with auto-getters | `record` |
| Mixed plaintext + encrypted fields | `hybrid module` |
| Generic logic (last resort) | `module` |

### Type and keyword aliases

Always write:

| Write | Not |
|-------|-----|
| `amount` | `uint256` |
| `action name() { }` | `function name() public { }` |
| `view name() returns T { }` | `function name() public view returns (T) { }` |
| `when condition` (guard) | `require(condition, "msg")` |
| `only X` (guard) | `modifier onlyX() { _; }` |
| `map<K, V>` | `mapping(K => V)` |
| `record` / `token` / appropriate keyword | `contract` |
| `caller` | `msg.sender` |
| `text` | `string` |
| `action initialize(…)` | `constructor(…)` |

### Field declarations

- Inside `module` and `hybrid module`: use `field name: type`
- Inside `record`: bare `name: type` (no `field` keyword)
- Remove all Solidity visibility modifiers (`public`, `private`, `internal`, `external`)

### vault: reentrancy default

`vault` is `@non_reentrant` by default. Do **not** add `@non_reentrant` manually;
the compiler will warn. Write:

```covenant
-- correct
vault MyVault {
    field balances: map<address, amount>

    action withdraw(value: amount)
            when balances[caller] >= value {
        balances[caller] -= value
        transfer(value) to caller
    }
}
```

### Post-quantum (`pq_signed`)

- `pq_signed(content, sig, key)` at action level enables Dilithium-5 verification (Fortress layer).
- `pq_key` is the correct type for Dilithium-5 public keys.

### FHE / encrypted fields

- Fields qualified `encrypted` require TFHE context (Veil layer).
- `reveal X to Y` is the only way to disclose an encrypted field; it declares an access policy.
- Use `encrypted_when ... otherwise` for conditional logic over FHE values; this compiles to a
  homomorphic `cmux`. In-body `if/else` over plaintext values is a V0.9 feature.

### Time and duration

- `now` is typed `time`, not `amount`. You cannot add a bare number to `now`.
- Write `now + 7 days` (produces `time`). Available duration literals:
  `seconds`, `minutes`, `hours`, `days`, `weeks`.
- Do not cast `now` to `amount`, use a separate `time` field if comparison is needed.

### Events and errors

```covenant
event Transfer(from: address, to: address, value: amount)
emit Transfer(caller, to, value)

error InsufficientBalance(required: amount, actual: amount)
revert_with InsufficientBalance(needed, balances[caller])
```

- Use `revert_with ErrorName(args)` not bare `revert`.
- Prefer typed errors over bare string reverts for ABI-decodable failures.

---

## Compiler diagnostics (fail-loud)

The Covenant v0.9.7 compiler is **fail-loud**: rather than silently emitting
plausible-but-wrong bytecode, it **refuses and errors**. Do **not** generate the
constructs below, they will not compile. If a user hits one of these, explain the
error and pick a supported construct instead. Trust the error.

| Code | Refused construct | Guidance |
|------|-------------------|----------|
| **E424** | stdlib math builtins `min` / `max` / `abs` / `pow` / `sqrt` | Not implemented → compile error. Do not use them; write the arithmetic explicitly. |
| **E425** | map introspection `.length` / `.keys` / `.values` | Unsupported → error. Track size/keys in a separate field. |
| **E426** | the `in` membership operator (`given x in list`) | Not implemented → error. Use an explicit lookup / `map` membership. |
| **E427** | map `.argmax` / `.argmin` | Unsupported → error. (List `.argmax` / `.argmin` **do** work.) |
| **E512** | a non-anonymous `event` with **>3** `indexed` params | Error. Max 3 indexed params (drop `indexed` or make the event `anonymous`). |
| **E519** | division / modulo by a **literal** zero | Error. (A non-literal divisor instead gets a runtime guard.) |
| **E520** | a missing precompile helper method | Error. The referenced precompile helper does not exist. |
| **E521** | a `text` / string constant longer than **32 bytes** | Error. Keep constant strings ≤ 32 bytes. |
| **E522** | nested maps (`map<_, map<_, _>>`) | Not yet supported → error. Use a struct-valued map or flatten the key. |
| **E523** | `transfer <amt> from <src> to <dst>` | No faithful lowering → error. A native transfer compiles to a `CALL`, which spends the *contract's own* balance, so `from` was silently dropped. Use `transfer <amt> to <dst>` and debit the source in storage first. |
| **W508** | `only caller` | Warning, it is an allow-all no-op that guards nothing. Use a real principal (`only owner`, `only deployer`, …). |
| **E040** | an operator, field, index or call chain longer than the parser will build | Error. Split the expression. The old behaviour was a native stack overflow, an uncatchable crash the language server inherited. |
| **E041** | a single body with more statements than the compiler will lower | Error. Split the action. Code generation was superlinear in body size, so a large body hung the compiler. |
| **E060** | a duration literal whose value in seconds does not fit in u64 | Error. Use a smaller literal. |
| **E240** | an `append` literal naming a field the element struct does not have | Error. It used to type-check and privacy-check nothing at all. |
| **E430** | `append <collection> { .. }` where the collection has no storage field | Error. Nothing allocated a slot, so the append reported success and stored nothing. Declare the collection as a real field. |
| **E431** | reading a construct-implicit collection that has no storage field | Error. It lowered to the constant 0, so the backend read storage slot 0, disclosing the first declared field for every index. |
| **E432** | `match` in expression position | Error. It evaluated to the constant 0. The statement form now lowers correctly and is supported. |
| **E433** | `try_action { .. } catch _ { .. }` | Error. The catch body was discarded and no error was trapped. |
| **E434** | a non-empty list literal (`xs = [10, 20, 30]`) | Error. It compiled to nothing and left the list empty. |
| **E435** | `delete <target>` on a shape with no zeroing lowering | Error. `delete` compiled to nothing, so a revocation action revoked nothing. |
| **E436** | an `only <principal>` clause whose principal is not an address | Error. It emitted an unsatisfiable comparison with no diagnostic. |
| **E437** | `match` on an encrypted scrutinee | Error. The statement form lowers to a plaintext comparison, which would leak the value. |
| **E530** | a `hex` literal wider than 32 bytes | Error. A single PUSH carries at most 32 bytes, so the excess was emitted as executable bytecode. |
| **E531** | a bare struct-typed field (`field cfg: Cfg`) | Error. Writes were dropped and reads returned the NEXT declared field. Use a list of structs. |
| **E532** | an `indexed` event parameter of a dynamic type | Error. The topic was a zero placeholder, so two logs with different values had identical topics. |
| **E640** | `supply: N to <principal>` where the principal is not `deployer` | Error. It minted nothing at all. Use `supply: N to deployer` plus a deployer-guarded action to move the balance. |
| **E641** | a `total_supply` field default that contradicts the genesis mint | Error. The default silently won over the mint amount. |
| **E642** | `decimals` outside the EIP-20 uint8 range | Error. |
| **E643** | a user event or error shadowing a synthesized one with a different shape | Error. It produced a broken ABI. |
| **W440** | `given <cond>` | Warning. It compiles as a PRECONDITION asserted before the body runs, which the shipped guide described differently. |
| **W530** | a non-indexed event parameter of a dynamic type | Warning. The log data word is a zero placeholder, so a decoder reading offset plus length gets nothing. |

Guard principals that cannot be resolved **fail closed** (E516 / E517 / E518 from
earlier releases), a guard whose principal is unknown errors rather than silently
allowing the action.

---

## ERC-822x Compliance (Styx Protocol)

When generating or reviewing Covenant code that uses any of the following
primitives, verify ERC conformance and cite the ERC number in a `--` comment
adjacent to the construct.

### ERC-8227: Confidential Token Interface

**Trigger:** `confidential token` construct.

The `confidential token` keyword auto-synthesizes the full ERC-8227 surface:

- `transferEncrypted(to: address, amount: encrypted amount)`
- `balanceOfEncrypted(who: address) returns encrypted amount`
- `approveEncrypted(spender: address, amount: encrypted amount)`

**Required citation comment:**

```covenant
-- ERC-8227: Confidential Token Interface (Styx Protocol)
confidential token PrivateCoin {
    symbol:   "PRIV"
    name:     "Private Coin"
    decimals: 18
    supply:   1_000_000 to deployer
}
```

If `confidential token` is present without the ERC-8227 citation comment, flag it
as a compliance gap in any review output.

### ERC-8228: Cryptographic Amnesia (Amnesia Ceremony)

**Trigger:** `ceremony` construct, or any use of `destroy()` / `on_destroy { }`.

> **Numbering note:** the amnesia ceremony maps to **ERC-8228 (Cryptographic
> Amnesia)**, a Draft standard authored by Kairos Lab as the Styx Protocol
> (`Valisthea/styx-erc-cryptographic-amnesia`). ERC-8227 is the separate
> Encrypted Token Standard (`Valisthea/styx-erc-encrypted-token`). A `ceremony`
> **should** carry an `-- ERC-8228` citation, exactly as a `confidential token`
> carries `-- ERC-8227`.

The `ceremony` keyword auto-synthesizes the full amnesia lifecycle:

- Phases: `Setup(0)` → `Active(1)` → `Finalized(2)` → `Destroyed(3)`
- `setup() → uint256` (returns `session_id`)
- `submit_share(bytes32) → bool`
- `finalize() → bool`
- `destroy() → bool`
- `phase() → uint256`
- `is_destroyed() → bool`
- `session_id() → uint256`
- `owner() → address`

The `on_destroy` block runs on the `Destroyed` phase transition.
`destroy(0)` triggers the Wesolowski VDF proof + Shamir reconstruction emission.

**Required citation comment:**

```covenant
-- ERC-8228: Cryptographic Amnesia (Styx Protocol)
ceremony AuditTrail {
    guardians: 3
    threshold: 2

    on_destroy {
        destroy(0)
    }
}
```

If `ceremony` is present without the ERC-8228 citation comment, flag it as a
compliance gap in any review output. A `ceremony` that cites `ERC-8228`
(Cryptographic Amnesia) is correct, do not flag it.

### ERC-8229: FHE Computation Verification

**Trigger:** `verified_by(zk_proof)` guard qualifier on an action.

The `verified_by` guard invokes the Prism layer (Halo2 SNARK + Nova IVC folding)
to verify a recursive proof before the action body executes.

**Required citation comment:**

```covenant
-- ERC-8229: FHE Computation Verification (Styx Protocol)
action settle(proof: proof_payload, result: encrypted amount)
        verified_by(proof) {
    treasury += result
}
```

If `verified_by` is present without the ERC-8229 citation comment, flag it as a
compliance gap in any review output.

### ERC-8231: Post-Quantum Signature Verification

**Trigger:** `pq_signed(content, sig, key)` guard qualifier on an action.

The `pq_signed` guard invokes the Fortress layer (Dilithium-5) to verify a
post-quantum signature before the action body executes.

**Required citation comment:**

```covenant
-- ERC-8231: Post-Quantum Signature Verification (Styx Protocol)
action post_signed(content: hash, sig: bytes)
        pq_signed(content, sig, keys[caller]) {
    messages[caller] = content
}
```

If `pq_signed` is present without the ERC-8231 citation comment, flag it as a
compliance gap in any review output.

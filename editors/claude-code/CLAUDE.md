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
| Identity or key directory | `record` (`registry` does not compile, see below) |
| Append-only message log | `record` with an explicit list field (see below) |
| Order book / marketplace | `market` |
| Cross-chain escrow | `bridge` |
| Single-value counter (plaintext) | `counter` |
| Single-value counter (FHE-encrypted) | `encrypted counter` |
| Key-value storage (no getters are synthesized, declare `view` accessors) | `record` |
| Mixed plaintext + encrypted fields | `hybrid module` |
| Generic logic (last resort) | `module` |

Only three of these synthesize an ABI at v0.9.7. Measured with
`covenant inspect abi`:

- `token` synthesizes nine functions, `confidential token` nine, `ceremony`
  eight.
- `counter`, `encrypted counter`, `vault`, `ballot`, `market`, `bridge`,
  `record`, `hybrid module` and `module` synthesize nothing. A body of only
  fields compiles to an empty ABI and a 12-byte runtime that reverts on every
  selector, so every entry point has to be declared by hand. `vault`, `ballot`
  and `bridge` say so with `W606`; `counter`, `encrypted counter` and `market`
  are silent about it.
- `registry` does not compile in any form. Even `registry R { }` fails with two
  `E505` errors, because its synthesizer injects `register` and `key_of` over
  `pq_key`. Use a `record` holding a `map` plus hand-written accessors.
- `board` compiles, but its implicit `post` collection has no storage field:
  appending to it is `E430` and reading it is `E431`. Declare a `struct` and a
  real list field on a `record` instead:

```covenant
record MessageLog {
    struct Entry {
        author: address
        body: hash
    }

    posts: [Entry] = []

    action publish(body: hash) {
        append posts { author: caller, body: body }
    }

    view count() returns amount { posts.length }
}
```

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
- `from` and `to` are reserved keywords. They cannot name a field, an action
  parameter or an event argument: the parser rejects them with `E020`
  (`expected field name, got KwTo`). Use `src` / `dst`.

### record has no synthesized getters

At v0.9.7 `record` synthesizes nothing. A record whose body is only fields
compiles to an empty ABI and a 12-byte runtime that reverts on every selector, so
its state is written to storage but unreachable from outside. Declare each
accessor explicitly:

```covenant
record Settings {
    owner: address
    count: amount
    enabled: bool

    view get_owner() returns address { owner }
    view get_count() returns amount { count }
    view get_enabled() returns bool { enabled }
}
```

A `view` body is a bare expression. Writing `{ return owner }` is `E020`
(`expected an expression, got KwReturn`).

### vault: no automatic reentrancy guard

`vault` adds **no** reentrancy protection at v0.9.7. Its standard-interface
synthesizer is stubbed: the construct is passed through unchanged with `W606`, and
the same body written as a `module` compiles to byte-identical runtime bytecode
and the same storage layout. `inspect storage` on a vault shows only the declared
fields, with no lock slot.

Do **not** write `@non_reentrant`. It is not a recognised annotation at v0.9.7:
the compiler rejects it with `E110` (`Help: valid annotations: @precompute,
@batch_up_to, @prove_offchain, @gas_budget`) and the build fails. It does not
warn, it errors. Note that `covenant lint` still emits `W003` advising you to add
warn, it errors. `covenant lint` reports `W003` on the shape and its help now
says to order your state writes, not to reach for an annotation.
The protection you get is the one you write. Order every state write before any
`transfer`:

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
  homomorphic `cmux`. In-body `if/else` over plaintext values compiles at v0.9.7,
  with no parentheses around the condition.

### Time and duration

- `now` is typed `time`, not `amount`. You cannot add a bare number to `now`.
- Write `now + 7 days` (produces `time`). Available duration literals:
  `seconds`, `minutes`, `hours`, `days`, `weeks`.
- Do not cast `now` to `amount`, use a separate `time` field if comparison is needed.

### Events and errors

```covenant
module Ledger {
    field balances: map<address, amount>

    event Transfer(src: address, dst: address, value: amount)
    error InsufficientBalance(required: amount, actual: amount)

    action send(dst: address, value: amount) {
        if balances[caller] < value {
            revert_with InsufficientBalance(value, balances[caller])
        }
        balances[caller] -= value
        balances[dst] += value
        emit Transfer(caller, dst, value)
    }
}
```

- Use `revert_with ErrorName(args)` not bare `revert`.
- Prefer typed errors over bare string reverts for ABI-decodable failures.
- Never name an event argument `from` or `to`. Both are keywords and the parser
  refuses them with `E020` before any type checking happens.

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
| **E521** | a `text` constant longer than **32 bytes** *that reaches the V0 ABI return encoder*: a `view ... returns text` literal, or a `token` `name:` / `symbol:` | Error. Keep those strings ≤ 32 bytes. The same 63-byte literal assigned to an ordinary `text` field compiles clean, so the length limit is not a blanket rule on string constants. |
| **E505** | `pq_key` in an ABI-visible position: a `view` return, an action parameter, or a synthesized interface | Error. `pq_key` is ABI type `bytes` (dynamic) and V0 codegen can only move a single 32-byte word. A `pq_key` that only ever sits in a field compiles clean. This is what makes every `registry` uncompilable. |
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
| **E215** | `match` on an encrypted scrutinee | Error, reported as `match pattern type mismatch: expected ciphertext<amount>, found amount`. `check`, `build` and `inspect diagnostics` all give this code, not one of the E43x codes. |
| **E530** | a `hex` literal wider than 32 bytes | Error. A single PUSH carries at most 32 bytes, so the excess was emitted as executable bytecode. |
| **E531** | a bare struct-typed field (`field cfg: Cfg`) | Error. Writes were dropped and reads returned the NEXT declared field. Use a list of structs. |
| **E532** | an `indexed` event parameter of a dynamic type | Error. The topic was a zero placeholder, so two logs with different values had identical topics. |
| **E640** | `supply: N to <principal>` where the principal is not `deployer` | Error. It minted nothing at all. Use `supply: N to deployer` plus a deployer-guarded action to move the balance. |
| **E641** | a `total_supply` field default that contradicts the genesis mint | Error. The default silently won over the mint amount. |
| **E642** | `decimals` outside the EIP-20 uint8 range | Error. |
| **E643** | a user event or error shadowing a synthesized one with a different shape | Error. It produced a broken ABI. |
| **W440** | `given <cond>` | Warning. It compiles as a PRECONDITION asserted before the body runs, which the shipped guide described differently. |
| **W530** | a non-indexed event parameter of a dynamic type | Warning. The log data word is a zero placeholder, so a decoder reading offset plus length gets nothing. |

Guard principals that cannot be resolved **fail closed**: a guard whose principal
is unknown errors rather than silently allowing the action. The three codes are:

| Code | Guard principal case |
|------|----------------------|
| **E106** | unknown principal predicate (`only nonexistent_principal`) |
| **E436** | the principal resolves but is not an address (`only owner` where `owner: amount`) |
| **E518** | a builtin predicate with no real EVM check (`only first_time_caller`, `only registered_key`), which would pass for every caller |

The principals enforced at v0.9.7 are `deployer`, which needs no declaration,
`owner` and `admin` when backed by a same-named `address` field, and an
explicit address literal such as `only 0x00..01`. Four more names resolve
without being enforced: unbacked `owner` or `admin` builds with `W421` and
reverts on every call, `caller` builds with `W508` because it lowers to
`msg.sender == msg.sender`, and `guardians` / `parties` / `holders` build with
`W421` as collection-typed principals with no codegen. Treat `W421` and `W508`
as errors. Any other name is `E106`, a same-named field of the wrong type is
`E436`, and `first_time_caller` / `registered_key` are `E518`.

`E516` and `E517` are **not** access-control diagnostics, do not read them as
such. `E516` is an unlowered amnesia opcode in a body (`ShamirSplit`, `VdfLock`)
and `E517` is the `vdf_locked for <duration>` action qualifier, which has no EVM
lowering in this release.

---

## ERC-822x Compliance (Styx Protocol)

When generating or reviewing Covenant code that uses any of the following
primitives, verify ERC conformance and cite the ERC number in a `--` comment
adjacent to the construct.

### ERC-8227: Confidential Token Interface

**Trigger:** `confidential token` construct.

The `confidential token` keyword auto-synthesizes nine ABI functions. Read back
with `covenant inspect abi`:

- `transferEncrypted(address,bytes32) -> bool`
- `balanceOfEncrypted(address) -> bytes32`
- `transferFromEncrypted(address,address,bytes32) -> bool`
- `approveEncrypted(address,bytes32) -> bool`
- `allowanceEncrypted(address,address) -> bytes32`
- `totalSupply() -> uint256`
- `decimals() -> uint256`
- `symbol() -> string`
- `name() -> string`

plus the events `TransferEncrypted` and `ApprovalEncrypted` and the errors
`InsufficientEncryptedBalance` and `InsufficientEncryptedAllowance`.

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

`destroy(0)` lowers to a single IR instruction, `DestructionProof`, which is one
call to the `amnesiaDestroy(uint256)` precompile helper. Confirm with
`covenant inspect ir <file>`. No VDF proof and no Shamir reconstruction is
emitted: `ShamirSplit` / `ShamirReconstruct` / `VdfLock` / `VdfUnlock` are
separate opcodes that this path never produces, and the backend refuses them with
`E516` if they ever reach it. At v0.9.7 the helper is a deterministic stub, in the
compiler's own words "the VDF, the Shamir split and the destruction proof are
deterministic stubs, and the 'destroyed' secret remains readable from chain
state". Do not present a `ceremony` to a user as cryptographically destroying
anything at this version.

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

**Trigger:** `verified_by(<proof param>)` guard qualifier on an action.

The `verified_by` guard invokes the Prism layer (Halo2 SNARK + Nova IVC folding)
to verify a recursive proof before the action body executes.

The proof parameter is typed `bytes`. Neither `proof_payload` nor `zk_proof` is a
type at v0.9.7: `proof_payload` is a language-provided identifier bound as a
*value* inside a `verified_by` action, so using it in type position is `E231`, and
`zk_proof` does not exist at all (`E102`).

**Required citation comment:**

```covenant
hybrid module Settlement {
    field treasury: encrypted amount

    -- ERC-8229: FHE Computation Verification (Styx Protocol)
    action settle(proof: bytes, result: encrypted amount)
            verified_by(proof) {
        treasury += result
    }
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
module SignedBoard {
    field keys: map<address, pq_key>
    field messages: map<address, hash>

    -- ERC-8231: Post-Quantum Signature Verification (Styx Protocol)
    action post_signed(content: hash, sig: bytes)
            pq_signed(content, sig, keys[caller]) {
        messages[caller] = content
    }
}
```

If `pq_signed` is present without the ERC-8231 citation comment, flag it as a
compliance gap in any review output.

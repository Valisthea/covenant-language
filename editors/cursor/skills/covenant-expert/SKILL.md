---
name: covenant-expert
description: >-
  Expert knowledge of the Covenant v0.9.7 smart-contract language (Kairos Lab).
  Activate for any of: Covenant code.cov files; the top-level constructs
  (record, token, nft, confidential token, ballot, counter, encrypted counter,
  board, market, vault, registry, bridge, ceremony, module, hybrid module); FHE; ZK;
  post-quantum; cryptographic amnesia; ERC-8227; ERC-8228; ERC-8229; ERC-8231;
  Solidity migration to Covenant; Covenant code review; compile errors in .cov
  files.
---

# Covenant v0.9.7: Language Reference

Every fenced sample in this file was built with the v0.9.7 compiler
(`covenant build <file> --out <dir>`), and section h tracks the v0.9.7 diagnostic
registry. Where a construct or a guard does not compile at v0.9.7, this file names
the diagnostic instead of hiding it.

Top-level keyword = architecture decision. Solidity has one `contract`; Covenant
has a specialized keyword per shape. Only a few of them auto-synthesize an ABI
surface; the rest give you the right shape, defaults and privacy qualifiers, and
you write the actions yourself. The table below says which is which.

---

## a) The top-level constructs

| Construct | Auto-synthesizes | Pick when… |
|-----------|-----------------|------------|
| `record C { }` | **Nothing.** No auto-getters at any layer. A field-only record compiles to an empty ABI and a runtime that only reverts; write the `view` accessors yourself | Key-value storage, simple state bag |
| `token C { }` | Full ERC-20 surface (`transfer`, `approve`, `balanceOf`, `Transfer`, `Approval`) | Standard fungible token |
| `nft C { }` | Full ERC-721 surface (`mint`, `burn`, `ownerOf`, `balanceOf`, `approve`, `getApproved`, `setApprovalForAll`, `isApprovedForAll`, `transferFrom`, `tokenURI`, `name`, `symbol`, events `Transfer`, `Approval`, `ApprovalForAll`) | Non-fungible token |
| `confidential token C { }` | ERC-8227 surface (`transferEncrypted`, `balanceOfEncrypted`, `approveEncrypted`) | FHE-encrypted token balances |
| `ballot C { }` | **Nothing yet** (emits `W606`, synthesis not implemented). Write the actions yourself | On-chain voting / polls |
| `counter C { }` | **Nothing.** Write the actions yourself | Single-value counter |
| `encrypted counter C { }` | **Nothing.** The `encrypted` qualifier is real; there is no generated surface | Privacy-preserving counter |
| `board C { post { } }` | **Nothing.** Write the actions yourself | Message board / append-only log |
| `market C { }` | **Nothing.** `priority_queue` fields are read-only in this release | Marketplace, DEX order book |
| `vault C { }` | **Nothing.** No generated surface (emits `W606`) and no reentrancy guard either, see section g | Funds vault, escrow |
| `registry C { }` | ERC-8231 surface, but the construct **does not compile** in this release (`E505`) | Identity registry, key directory |
| `bridge C anchored_on ["a","b"] { }` | **Nothing yet** (emits `W606`, synthesis not implemented). Write the actions yourself | Multi-chain bridge |
| `ceremony C { on_destroy { … } }` | Full amnesia-ceremony lifecycle (`setup`, `submit_share`, `finalize`, `destroy`, `phase`, `is_destroyed`) | Cryptographic amnesia / secret-sharing ceremonies |
| `module C { }` | Nothing, generic escape hatch | Generic logic when no specialized keyword fits |
| `hybrid module C { }` | Nothing, but allows per-field privacy qualifiers | Mixed plaintext + encrypted state |


Only `token`, `nft`, `confidential token` and `ceremony` have a real stdlib
synthesizer today. The other constructs give you the right shape, defaults and
privacy qualifiers, but you write the actions. Where a construct is listed as
emitting `W606`, the compiler says so out loud rather than pretending.

---

## b) Comment syntax

```covenant
-- single-line comment, ends at line break

(* multi-line comment
   can nest (* like this *)
   ends with closing *)
record Notes {
    field n: amount
}
```

A file of comments alone is not a compilation unit: the compiler needs at least one
top-level construct, otherwise you get `[E028] expected a top-level construct
keyword (record, token, ...), got end of file`.

`//` and `/* */` are **rejected** by the compiler with a dedicated diagnostic.
This is intentional, the syntax break signals a different mental model.

---

## c) The 11 Solidity → Covenant anti-patterns

| ❌ Solidity (wrong) | ✅ Covenant (right) | One-line fix |
|---------------------|---------------------|--------------|
| `mapping(K => V) public xs;` | `field xs: map<K, V>` | Change `mapping(K => V)` to `map<K, V>`, prefix with `field` |
| `function name() public { }` | `action name() { }` | Replace `function` with `action` (or `view` for reads) |
| `require(cond, "msg");` | `when cond` guard | Lift out of body into action signature as `when cond` |
| `// comment` | `-- comment` | Replace `//` with `--` |
| `contract Foo { }` | `record` / `token` / `vault` / … | Pick the most specialized top-level keyword |
| `uint256` | `amount` | Replace `uint256` with `amount` throughout |
| `modifier onlyOwner() { _; }` | `only owner` | Inline as `only owner` guard on each action |
| `msg.sender` | `caller` | Replace `msg.sender` with `caller` |
| `public` / `external` visibility | (remove) | Delete visibility modifiers, actions are accessible by default |
| `constructor(...) { }` | `action initialize(...)` or `supply: N to deployer` | Rename to `initialize` or use token metadata block |
| `emit Transfer(a, b, v);` | `emit Transfer(a, b, v)`, identical syntax, but ensure `event Transfer(…)` is declared | Declare the event with Covenant-style arg syntax |

---

## d) Privacy qualifiers

Privacy qualifiers **prefix** the construct keyword (or, inside `hybrid module`,
they sit between `field` and the field name).

| Qualifier | Applies to | Meaning |
|-----------|-----------|---------|
| `public` | `module`, `record`, `token`, `ballot`, `counter`, `board` | Default visibility; state readable on-chain |
| `private` | `ballot`, `counter`, `board` | State hidden from direct read; access via `view` only |
| `encrypted` | `module`, `record`, `counter` | All fields are TFHE ciphertext (Veil layer) |
| `sealed` | `ballot`, `board` | Sealed-bid / sealed-vote variant |
| `confidential` | `token` | Alias that selects the ERC-8227 surface |
| `hybrid` | `module`, `token` | Mixed plaintext + encrypted fields; qualify per-field |

Inside `hybrid module`, the per-field qualifiers are exactly two, `public` and
`encrypted`. They sit between `field` and the field name. `public` is the default
and may be written explicitly.

```covenant
hybrid module Wallet {
    field headcount:          amount   -- plaintext, the default
    field encrypted treasury: amount   -- TFHE ciphertext
}
```

`pq_signed` is **not** a field qualifier. It is an action guard (section e), and
writing `field pq_signed history: [hash]` is a parse error:
`[E020] expected field name, got KwPqSigned`.

---

## e) Access guards

Guards are declared **after the arg list, before the body**, comma-separated.
All guards must hold or the action reverts before the body runs.

```covenant
module Ledger {
    field owner:    address
    field balances: map<address, amount>

    event Sent(sender: address, recipient: address, value: amount)

    action send(recipient: address, value: amount)
            when balances[caller] >= value,
            only owner {
        balances[caller]    -= value
        balances[recipient] += value
        emit Sent(caller, recipient, value)
    }
}
```

Two naming traps in that sample. `transfer` and `to` are **reserved keywords**, so
an action cannot be called `transfer` (`[E020] expected action name, got
KwTransfer`) and a parameter cannot be called `to` (`[E020] expected argument name,
got KwTo`). The statement form `transfer(value) to caller` is what those two
keywords are for.

| Guard | Status at v0.9.7 |
|-------|------------------|
| `when expr` | Works. Arbitrary boolean expression must be true |
| `given expr` | Works. Boolean precondition asserted before the body runs |
| `only owner` | Works. `caller` must equal the `owner` address field. Without a declared `field owner: address` you get `W421` and the action reverts on every call |
| `only deployer` | Works. `caller` must equal the deployment address |
| `pq_signed(content, sig, key)` | Works. Dilithium-5 signature verification (ERC-8231, Fortress layer). `key` must be a **stored field** typed `pq_key`. A field typed `bytes` is `E201`; a `pq_key` **action parameter** is `E505`, the same dynamic-`bytes` ABI blocker that stops `registry` |
| `verified_by(zk_proof)` | Works. Recursive IVC proof verification (ERC-8229, Prism layer). The proof argument is typed `bytes` |
| `only first_time_caller` | **Refused** (`E518`). No real EVM authorization check exists, so the guard would pass for every caller and the compiler will not lower it |
| `only registered_key` | **Refused** (`E518`). Resolves in the frontend, same missing authorization check, same refusal at build |
| `only registered_account` | **Does not exist.** `E106`, unknown principal predicate, in every construct including `token` |
| `given x in collection` | **Refused** (`E426`). The `in` membership operator has no lowering. Write the comparison out: `given x == a \|\| x == b` |
| `vdf_locked for <delay>` | **Refused** (`E517`). Parses, but has no EVM lowering. Note the form is `for <delay>`: the parenthesised `vdf_locked(handle, time)` is a parse error, E020, expected `for`, got an open paren |

Only the `E106` row is caught by `covenant check`. The `E426`, `E517` and `E518`
rows pass the frontend clean and are refused by `covenant build`, so a green
`check` proves nothing about a guard. Build it.

`only deployer` and `only <address literal>` build clean and lower to a real
`caller` comparison. `only owner` and `only admin` do the same, but only when a
same-named `address` field is declared; without it they build with `W421`, the
guard is not enforced and the action reverts on every call. `guardians`,
`parties` and `holders` also resolve and also emit `W421`, as collection-typed
principals with no codegen. `only caller` emits `W508` and restricts nothing. A
name outside those seven is `E106`, not a warning. Multiple guards are
comma-separated, **not** joined with `&&`.

---

## f) ERC-822x construct mapping (Styx Protocol)

| ERC | Covenant trigger | Auto-synthesized surface |
|-----|-----------------|--------------------------|
| **ERC-8227**, Confidential Token Interface | `confidential token C { }` | `transferEncrypted`, `balanceOfEncrypted`, `approveEncrypted` |
| **Amnesia Ceremony** (ERC-8228, Cryptographic Amnesia, Styx Protocol) | `ceremony C { on_destroy { … } }` | `setup`, `submit_share`, `finalize`, `destroy`, `phase`, `is_destroyed`, `session_id`, `owner`, lifecycle: Setup → Active → Finalized → Destroyed |
| **ERC-8229**, FHE Computation Verification | `verified_by(zk_proof)` guard on an action | Halo2 SNARK + Nova IVC proof verification at action entry |
| **ERC-8231**, Post-Quantum Signature Verification | `pq_signed(content, sig, key)` guard on an action | Dilithium-5 signature check at action entry |

Cite the ERC number in a `--` comment near the construct. Example:

```covenant
-- ERC-8227: Confidential Token Interface (Styx Protocol)
confidential token PrivateCoin {
    symbol:   "PRIV"
    name:     "Private Coin"
    decimals: 18
    supply:   1_000_000 to deployer
}
```

---

## g) `vault` and `@non_reentrant`

`vault` does **not** insert reentrancy protection at v0.9.7. The construct emits
`W606`, "vault standard-interface synthesis not yet implemented, passing construct
through unchanged", and the emitted bytecode contains no read or write of the
reentrancy lock slot. `covenant lint` confirms it from the other side: it reports
`W003`, "action `withdraw` makes an external call with no reentrancy
protection, and this release has none to offer", on the sample below.

Writing the annotation yourself does not help either. `@non_reentrant` is not in the
resolver's annotation allowlist, so it is a hard error, not a warning:

```
[E110] Error: unknown annotation `@non_reentrant`
   Help: valid annotations: `@precompute`, `@batch_up_to`, `@prove_offchain`, `@gas_budget`
```

So no Covenant source at v0.9.7 can turn the lock on. Order your state writes
**before** the `transfer`, as the sample does, and treat the vault body as
unprotected.

```covenant
vault Treasury {
    field balances: map<address, amount>

    action deposit() {
        balances[caller] += 1
    }

    action withdraw(value: amount)
            when balances[caller] >= value {
        balances[caller] -= value
        transfer(value) to caller
    }

    view balance_of(who: address) returns amount {
        balances[who]
    }
}
```

---

## Quick-reference: built-in symbols

| Covenant | Solidity equivalent | Type |
|----------|---------------------|------|
| `caller` | `msg.sender` | `address` |
| `now` | `block.timestamp` | `time` (NOT `amount`) |
| `current_block` | `block.number` | `amount` |
| `this` | `address(this)` | `address` |
| `deployer` | (none built-in) | `address` |
| `zero_address` | `address(0)` | `address` |

`now` is typed `time`. Write `now + 7 days`, not `now + 604800`.
Available durations: `seconds`, `minutes`, `hours`, `days`, `weeks`.

---

## h) Compiler diagnostics (fail-loud)

The Covenant v0.9.7 compiler is **fail-loud**: rather than silently emitting
plausible-but-wrong bytecode, it refuses and errors. Do **not** generate the
constructs below; if a user hits one of these, explain the error and pick a
supported construct.

| Code | Meaning |
|------|---------|
| **E424** | stdlib math builtins `min`/`max`/`abs`/`pow`/`sqrt` are not implemented → compile error (don't use them). |
| **E425** | map introspection `.length`/`.keys`/`.values` is unsupported → error. |
| **E426** | the `in` membership operator (`given x in list`) is not implemented → error. |
| **E427** | map `.argmax`/`.argmin` is unsupported → error (list `.argmax`/`.argmin` DO work). |
| **E512** | a non-anonymous event with >3 `indexed` params → error (max 3 indexed). |
| **E519** | division/modulo by a literal zero → error (a non-literal divisor gets a runtime guard). |
| **E520** | a missing precompile helper method → error. |
| **E521** | a text constant longer than 32 bytes in a **return** position → error; the V0 return encoder emits at most 32 bytes. The same constant is fine as a field default, a field assignment or an `emit` argument. |
| **E522** | nested maps (`map<_, map<_,_>>`) are not yet supported → error (use a struct-valued map or flatten). |
| **E523** | `transfer <amt> from <src> to <dst>` has no faithful lowering → error. A native transfer compiles to a `CALL`, which spends the contract's own balance, so `from` was silently dropped. Use `transfer <amt> to <dst>` and debit the source in storage first. |
| **W508** | `only caller` is an allow-all no-op → warning (it guards nothing; use a real principal). |
| **E040** | an operator, field, index or call chain longer than the parser will build: Error. Split the expression. The old behaviour was a native stack overflow, an uncatchable crash the language server inherited. |
| **E041** | a single body with more statements than the compiler will lower: Error. Split the action. Code generation was superlinear in body size, so a large body hung the compiler. |
| **E060** | a duration literal whose value in seconds does not fit in u64: Error. Use a smaller literal. |
| **E240** | an `append` literal naming a field the element struct does not have: Error. It used to type-check and privacy-check nothing at all. |
| **E430** | `append <collection> { .. }` where the collection has no storage field: Error. Nothing allocated a slot, so the append reported success and stored nothing. Declare the collection as a real field. |
| **E431** | reading a construct-implicit collection that has no storage field: Error. It lowered to the constant 0, so the backend read storage slot 0, disclosing the first declared field for every index. |
| **E432** | `match` in expression position: Error. It evaluated to the constant 0. The statement form now lowers correctly and is supported. |
| **E433** | `try_action { .. } catch _ { .. }`: Error. The catch body was discarded and no error was trapped. |
| **E434** | a non-empty list literal (`xs = [10, 20, 30]`): Error. It compiled to nothing and left the list empty. |
| **E435** | `delete <target>` on a shape with no zeroing lowering: Error. `delete` compiled to nothing, so a revocation action revoked nothing. |
| **E436** | an `only <principal>` clause whose principal is not an address: Error. It emitted an unsatisfiable comparison with no diagnostic. |
| **E437** | `match` on an encrypted scrutinee: Error. The statement form lowers to a plaintext comparison, which would leak the value. |
| **E530** | a `hex` literal wider than 32 bytes: Error. A single PUSH carries at most 32 bytes, so the excess was emitted as executable bytecode. |
| **E531** | a bare struct-typed field (`field cfg: Cfg`): Error. Writes were dropped and reads returned the NEXT declared field. Use a list of structs. |
| **E532** | an `indexed` event parameter of a dynamic type: Error. The topic was a zero placeholder, so two logs with different values had identical topics. |
| **E640** | `supply: N to <principal>` where the principal is not `deployer`: Error. It minted nothing at all. Use `supply: N to deployer` plus a deployer-guarded action to move the balance. |
| **E641** | a `total_supply` field default that contradicts the genesis mint: Error. The default silently won over the mint amount. |
| **E642** | `decimals` outside the EIP-20 uint8 range: Error. |
| **E643** | a user event or error shadowing a synthesized one with a different shape: Error. It produced a broken ABI. |
| **W440** | `given <cond>`: Warning. It compiles as a PRECONDITION asserted before the body runs, which the shipped guide described differently. |
| **W530** | a non-indexed event parameter of a dynamic type: Warning. The log data word is a zero placeholder, so a decoder reading offset plus length gets nothing. |
| **E020** | a reserved keyword used as an identifier, for instance an action named `transfer`, a parameter named `to`, or `pq_signed` in field position: Error. Rename the identifier. |
| **E110** | an annotation outside the allowlist, `@non_reentrant` included: Error. Valid annotations are `@precompute`, `@batch_up_to`, `@prove_offchain`, `@gas_budget`. |
| **E106** | an `only <principal>` naming a predicate the resolver does not know, such as `registered_account`: Error. |
| **E516** | an amnesia IR opcode with no EVM lowering: Error at build time. It used to emit a REVERT stub. Not a guard diagnostic, despite the neighbouring code numbers. |
| **E517** | `vdf_locked for <delay>`: Error at build time. The qualifier parses but has no EVM lowering, so a time-locked action would be instant at runtime. |
| **E518** | `only first_time_caller` / `only registered_key`: Error at build time. The predicate has no real EVM authorization check, so the guard would pass for every caller. |
| **W421** | `only owner` (or `only admin`) with no matching `address` field declared: Warning. The guard cannot be enforced, so the action reverts on every call, fail-closed. |

E517 and E518 fire at v0.9.7, not in some earlier release, and they fire only at
`build`: both pass `covenant check`. Guard principals the compiler cannot enforce
fail **closed**, either by refusing to compile (E517/E518) or by reverting on every
call (W421).

**Trust the error and pick a supported construct**: the compiler refuses rather
than silently miscompiling, so a diagnostic here is protecting you from wrong
bytecode, not blocking working code.

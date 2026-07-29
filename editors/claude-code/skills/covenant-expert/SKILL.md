---
name: covenant-expert
description: >-
  Expert knowledge of the Covenant v0.9.7 smart-contract language (Kairos Lab).
  Activate for any of: Covenant code.cov files; the 14 top-level constructs
  (record, token, confidential token, ballot, counter, encrypted counter, board,
  market, vault, registry, bridge, ceremony, module, hybrid module); FHE; ZK;
  post-quantum; cryptographic amnesia; ERC-8227; ERC-8228; ERC-8229; ERC-8231;
  Solidity migration to Covenant; Covenant code review; compile errors in .cov
  files.
---

# Covenant v0.9.7: Language Reference

Every syntax claim here is verified against the Covenant v0.9.7 compiler fixtures.
Top-level keyword = architecture decision. Solidity has one `contract`; Covenant
has 14 specialized constructs, pick the right one and the compiler auto-synthesizes
the correct ABI surface.

---

## a) The 14 top-level constructs

| Construct | Auto-synthesizes | Pick when… |
|-----------|-----------------|------------|
| `record C { }` | Per-field auto-getters (from the ABI layer, not the stdlib synthesizer) | Key-value storage, simple state bag |
| `token C { }` | Full ERC-20 surface (`transfer`, `approve`, `balanceOf`, `Transfer`, `Approval`) | Standard fungible token |
| `confidential token C { }` | ERC-8227 surface (`transferEncrypted`, `balanceOfEncrypted`, `approveEncrypted`) | FHE-encrypted token balances |
| `ballot C { }` | **Nothing yet** (emits `W606`, synthesis not implemented). Write the actions yourself | On-chain voting / polls |
| `counter C { }` | **Nothing.** Write the actions yourself | Single-value counter |
| `encrypted counter C { }` | **Nothing.** The `encrypted` qualifier is real; there is no generated surface | Privacy-preserving counter |
| `board C { post { } }` | **Nothing.** Write the actions yourself | Message board / append-only log |
| `market C { }` | **Nothing.** `priority_queue` fields are read-only in this release | Marketplace, DEX order book |
| `vault C { }` | `@non_reentrant` by default, which is real. **No generated surface** (emits `W606`) | Funds vault, escrow |
| `registry C { }` | ERC-8231 surface, but the construct **does not compile** in this release (`E505`) | Identity registry, key directory |
| `bridge C anchored_on ["a","b"] { }` | **Nothing yet** (emits `W606`, synthesis not implemented). Write the actions yourself | Multi-chain bridge |
| `ceremony C { guardians: N threshold: M }` | Full amnesia-ceremony lifecycle (`setup`, `submit_share`, `finalize`, `destroy`, `phase`, `is_destroyed`) | Cryptographic amnesia / secret-sharing ceremonies |
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
```

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

Privacy qualifiers **prefix** the construct keyword (or a field type inside `hybrid module`).

| Qualifier | Applies to | Meaning |
|-----------|-----------|---------|
| `public` | `module`, `record`, `token`, `ballot`, `counter`, `board` | Default visibility; state readable on-chain |
| `private` | `ballot`, `counter`, `board` | State hidden from direct read; access via `view` only |
| `encrypted` | `module`, `record`, `counter` | All fields are TFHE ciphertext (Veil layer) |
| `sealed` | `ballot`, `board` | Sealed-bid / sealed-vote variant |
| `confidential` | `token` | Alias that selects the ERC-8227 surface |
| `hybrid` | `module`, `token` | Mixed plaintext + encrypted fields; qualify per-field |

Inside `hybrid module`, per-field qualifiers:
```covenant
hybrid module Wallet {
    field headcount:          amount   -- plaintext
    field encrypted treasury: amount   -- TFHE ciphertext
    field pq_signed history:  [hash]   -- post-quantum signed list
}
```

---

## e) Access guards

Guards are declared **after the arg list, before the body**, comma-separated.
All guards must hold or the action reverts before the body runs.

```covenant
action transfer(to: address, value: amount)
        when balances[caller] >= value,
        only registered_account {
    balances[caller] -= value
    balances[to]     += value
    emit Transfer(caller, to, value)
}
```

| Guard | Meaning |
|-------|---------|
| `when expr` | Arbitrary boolean expression must be true |
| `only owner` | `caller` must equal the deployer-set `owner` address |
| `only deployer` | `caller` must equal the deployment address |
| `only first_time_caller` | This address has never called this action before |
| `only registered_account` | (token context) Caller must have a registered account |
| `only registered_key` | (registry/board context) Caller must have registered a key |
| `given x in collection` | Value-in-set / value-in-array membership check |
| `pq_signed(content, sig, key)` | Dilithium-5 signature verification (ERC-8231, Fortress layer) |
| `verified_by(zk_proof)` | Recursive IVC proof verification (ERC-8229, Prism layer) |
| `vdf_locked(handle, time)` | Wesolowski VDF time-lock check (Oblivion layer) |

Multiple guards are comma-separated, **not** joined with `&&`.

---

## f) ERC-822x construct mapping (Styx Protocol)

| ERC | Covenant trigger | Auto-synthesized surface |
|-----|-----------------|--------------------------|
| **ERC-8227**, Confidential Token Interface | `confidential token C { }` | `transferEncrypted`, `balanceOfEncrypted`, `approveEncrypted` |
| **Amnesia Ceremony** (ERC-8228, Cryptographic Amnesia, Styx Protocol) | `ceremony C { }` with `on_destroy` / `destroy()` | `setup`, `submit_share`, `finalize`, `destroy`, `phase`, `is_destroyed`, `session_id`, `owner`, lifecycle: Setup → Active → Finalized → Destroyed |
| **ERC-8229**, FHE Computation Verification | `verified_by(zk_proof)` guard on an action | Halo2 SNARK + Nova IVC proof verification at action entry |
| **ERC-8231**, Post-Quantum Signature Verification | `pq_signed(content, sig, key)` guard on an action | Dilithium-5 signature check at action entry |

Cite the ERC number in a `--` comment near the construct, the `erc-822x` rule
enforces this. Example:

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

`vault` includes reentrancy protection **by default**, the compiler inserts the
equivalent of `@non_reentrant` automatically. Do **not** add it manually; doing so
produces a compiler warning.

```covenant
-- correct: vault is already @non_reentrant
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

## h) Compiler diagnostics (fail-loud)

The compiler is **fail-loud**: it refuses rather than silently miscompiling. When a
construct below appears, the compiler **errors** instead of emitting
plausible-but-wrong bytecode. Do **not** generate these; if a user hits the error,
explain it and switch to a supported construct. Trust the error.

| Code | Refused construct | Guidance |
|------|-------------------|----------|
| **E424** | stdlib math builtins `min` / `max` / `abs` / `pow` / `sqrt` | Not implemented → error. Write the arithmetic explicitly instead. |
| **E425** | map introspection `.length` / `.keys` / `.values` | Unsupported → error. Track size/keys in a separate field. |
| **E426** | the `in` membership operator (`given x in list`) | Not implemented → error. Use an explicit lookup / `map` membership. |
| **E427** | map `.argmax` / `.argmin` | Unsupported → error. (List `.argmax` / `.argmin` **do** work.) |
| **E512** | a non-anonymous `event` with **>3** `indexed` params | Error, max 3 indexed params. Drop `indexed` or make the event `anonymous`. |
| **E519** | division / modulo by a **literal** zero | Error. (A non-literal divisor instead gets a runtime guard.) |
| **E520** | a missing precompile helper method | Error, the referenced precompile helper does not exist. |
| **E521** | a `text` / string constant longer than **32 bytes** | Error. Keep constant strings ≤ 32 bytes. |
| **E522** | nested maps (`map<_, map<_, _>>`) | Not yet supported → error. Use a struct-valued map or flatten the key. |
| **E523** | `transfer <amt> from <src> to <dst>` | No faithful lowering → error. A native transfer compiles to a `CALL`, which spends the *contract's own* balance, so `from` was silently dropped. Use `transfer <amt> to <dst>` and debit the source in storage first. |
| **W508** | `only caller` | Warning, allow-all no-op that guards nothing. Use a real principal (`only owner`, `only deployer`, …). |
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
earlier releases): a guard whose principal is unknown errors rather than silently
allowing the action.

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

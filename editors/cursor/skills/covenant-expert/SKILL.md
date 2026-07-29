---
name: covenant-expert
description: >-
  Expert knowledge of the Covenant v0.9.5 smart-contract language (Kairos Lab).
  Activate for any of: Covenant code, .cov files; the 14 top-level constructs
  (record, token, confidential token, ballot, counter, encrypted counter, board,
  market, vault, registry, bridge, ceremony, module, hybrid module); FHE; ZK;
  post-quantum; cryptographic amnesia; ERC-8227; ERC-8228; ERC-8229; ERC-8231;
  Solidity migration to Covenant; Covenant code review; compile errors in .cov
  files.
---

# Covenant v0.9.5 — Language Reference

Every syntax claim here is verified against the Covenant v0.9.5 compiler fixtures.
Top-level keyword = architecture decision. Solidity has one `contract`; Covenant
has 14 specialized constructs — pick the right one and the compiler auto-synthesizes
the correct ABI surface.

---

## a) The 14 top-level constructs

| Construct | Auto-synthesizes | Pick when… |
|-----------|-----------------|------------|
| `record C { }` | Per-field auto-getters | Key-value storage, simple state bag |
| `token C { }` | Full ERC-20 surface (`transfer`, `approve`, `balanceOf`, `Transfer`, `Approval`) | Standard fungible token |
| `confidential token C { }` | ERC-8227 surface (`transferEncrypted`, `balanceOfEncrypted`, `approveEncrypted`) | FHE-encrypted token balances |
| `ballot C { }` | Tally management, voting actions | On-chain voting / polls |
| `counter C { }` | `increment` / `decrement` actions | Single-value counter |
| `encrypted counter C { }` | TFHE counter operations (homomorphic `+=` / `-=`) | Privacy-preserving counter |
| `board C { post { } }` | Append-only post storage (`posts` array, `append`) | Message board / append-only log |
| `market C { }` | Order-book / matching primitives | Marketplace, DEX order book |
| `vault C { }` | Reentrancy-safe value custody (`@non_reentrant` by default) | Funds vault, escrow |
| `registry C { }` | Identity / key registration with PQ randomness | Identity registry, key directory |
| `bridge C anchored_on ["a","b"] { }` | Cross-chain escrow primitives | Multi-chain bridge |
| `ceremony C { guardians: N threshold: M }` | Full amnesia-ceremony lifecycle (`setup`, `submit_share`, `finalize`, `destroy`, `phase`, `is_destroyed`) | Cryptographic amnesia / secret-sharing ceremonies |
| `module C { }` | Nothing — generic escape hatch | Generic logic when no specialized keyword fits |
| `hybrid module C { }` | Nothing, but allows per-field privacy qualifiers | Mixed plaintext + encrypted state |

---

## b) Comment syntax

```covenant
-- single-line comment, ends at line break

(* multi-line comment
   can nest (* like this *)
   ends with closing *)
```

`//` and `/* */` are **rejected** by the compiler with a dedicated diagnostic.
This is intentional — the syntax break signals a different mental model.

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
| `public` / `external` visibility | (remove) | Delete visibility modifiers — actions are accessible by default |
| `constructor(...) { }` | `action initialize(...)` or `supply: N to deployer` | Rename to `initialize` or use token metadata block |
| `emit Transfer(a, b, v);` | `emit Transfer(a, b, v)` — identical syntax, but ensure `event Transfer(…)` is declared | Declare the event with Covenant-style arg syntax |

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
| **ERC-8227** — Confidential Token Interface | `confidential token C { }` | `transferEncrypted`, `balanceOfEncrypted`, `approveEncrypted` |
| **Amnesia Ceremony** (ERC-8228 — Cryptographic Amnesia, Styx Protocol) | `ceremony C { }` with `on_destroy` / `destroy()` | `setup`, `submit_share`, `finalize`, `destroy`, `phase`, `is_destroyed`, `session_id`, `owner` — lifecycle: Setup → Active → Finalized → Destroyed |
| **ERC-8229** — FHE Computation Verification | `verified_by(zk_proof)` guard on an action | Halo2 SNARK + Nova IVC proof verification at action entry |
| **ERC-8231** — Post-Quantum Signature Verification | `pq_signed(content, sig, key)` guard on an action | Dilithium-5 signature check at action entry |

Cite the ERC number in a `--` comment near the construct — the `erc-822x` rule
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

`vault` includes reentrancy protection **by default** — the compiler inserts the
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
        transfer(value, to: caller)
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

The Covenant v0.9.5 compiler is **fail-loud**: rather than silently emitting
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
| **E521** | a text/string constant longer than 32 bytes → error. |
| **E522** | nested maps (`map<_, map<_,_>>`) are not yet supported → error (use a struct-valued map or flatten). |
| **E523** | `transfer <amt> from <src> to <dst>` has no faithful lowering → error. A native transfer compiles to a `CALL`, which spends the contract's own balance, so `from` was silently dropped. Use `transfer <amt> to <dst>` and debit the source in storage first. |
| **W508** | `only caller` is an allow-all no-op → warning (it guards nothing; use a real principal). |

Guard principals that can't be resolved fail **closed** (E516/E517/E518 from earlier releases).

**Trust the error and pick a supported construct** — the compiler refuses rather
than silently miscompiling, so a diagnostic here is protecting you from wrong
bytecode, not blocking working code.

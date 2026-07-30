---
name: covenant-expert
description: >-
  Expert knowledge of the Covenant v0.9.7 smart-contract language (Kairos Lab).
  Activate for any of: Covenant code.cov files; the top-level constructs
  (record, token, nft, confidential token, ballot, counter, encrypted counter,
  board, market, vault, registry, bridge, ceremony, module, hybrid module);
  FHE; ZK; post-quantum; cryptographic amnesia; ERC-8227; ERC-8228; ERC-8229; ERC-8231;
  Solidity migration to Covenant; Covenant code review; compile errors in .cov
  files.
---

# Covenant v0.9.7: Language Reference

Every fenced sample in this file was run through `covenant build` on v0.9.7, and
every guard, construct and diagnostic named here was checked against that same
compiler. Where something does not compile, this file says so and quotes the
diagnostic instead of leaving a gap.

Top-level keyword = architecture decision. Solidity has one `contract`; Covenant
has the specialized constructs listed below. Picking the right one gives you the
right shape and defaults, but only a few of them synthesize an ABI surface for
you, and most emit nothing at all. Check the table below before assuming a
surface exists.

---

## a) The top-level constructs

| Construct | Auto-synthesizes | Pick when… |
|-----------|-----------------|------------|
| `record C { }` | **Nothing.** No auto-getters are emitted: the ABI is `[]` and the runtime is the same 12-byte stub `module` produces. Write `view` accessors yourself | Key-value storage, simple state bag |
| `token C { }` | Full ERC-20 surface (`transfer`, `approve`, `balanceOf`, `Transfer`, `Approval`) | Standard fungible token |
| `nft C { }` | Full ERC-721 surface: 21 ABI entries, including `mint`, `burn`, `ownerOf`, `balanceOf`, `approve`, `setApprovalForAll`, `transferFrom`, `tokenURI`, `name`, `symbol`, plus `Transfer` / `Approval` / `ApprovalForAll` | Non-fungible token collection |
| `confidential token C { }` | ERC-8227 surface (`transferEncrypted`, `balanceOfEncrypted`, `approveEncrypted`) | FHE-encrypted token balances |
| `ballot C { }` | **Nothing yet** (emits `W606`, synthesis not implemented). Write the actions yourself | On-chain voting / polls |
| `counter C { }` | **Nothing.** Write the actions yourself | Single-value counter |
| `encrypted counter C { }` | **Nothing.** The `encrypted` qualifier is real; there is no generated surface | Privacy-preserving counter |
| `board C { post { } }` | **Nothing.** Write the actions yourself | Message board / append-only log |
| `market C { }` | **Nothing.** `priority_queue` fields are read-only in this release | Marketplace, DEX order book |
| `vault C { }` | **Nothing** (emits `W606`). No reentrancy guard is inserted and no lock slot is allocated; `lint` still reports `W003` on any value-transferring action. See section g) | Funds vault, escrow |
| `registry C { }` | ERC-8231 surface, but the construct **does not compile** in this release (`E505`) | Identity registry, key directory |
| `bridge C anchored_on ["a","b"] { }` | **Nothing yet** (emits `W606`, synthesis not implemented). Write the actions yourself | Multi-chain bridge |
| `ceremony C { guardians: N threshold: M on_destroy { … } }` | Full amnesia-ceremony lifecycle (`setup`, `submit_share`, `finalize`, `destroy`, `phase`, `session_id`, `is_destroyed`, `owner`). The `on_destroy { }` block is mandatory: without it the build fails with `E309` | Cryptographic amnesia / secret-sharing ceremonies |
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
record Holder {
    field balance: amount
}
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

Inside `hybrid module`, two per-field qualifiers compile: `public` and
`encrypted`. Only `encrypted` changes the field's representation, `storage.json`
types it `ciphertext<_>` where `public` (like an unqualified field) is `uint256`.
The other four are rejected by the parser, `E020 expected field name`, got
`KwPrivate` / `KwSealed` / `KwConfidential` / `KwHybrid`.

```covenant
hybrid module Wallet {
    field headcount:          amount   -- plaintext
    field encrypted treasury: amount   -- TFHE ciphertext
    field history:            [hash]   -- plaintext list of digests
}
```

There is **no `pq_signed` field qualifier**. `field pq_signed history: [hash]` is
rejected by the parser with `[E020] expected field name, got KwPqSigned`, and no
other placement in a field declaration parses either (`field history: pq_signed
[hash]` gives `E020` plus `E024`, `pq_signed field history: [hash]` gives `E020`).
`pq_signed` exists only as an action guard, see section e).

---

## e) Access guards

Guards are declared **after the arg list, before the body**, comma-separated.
All guards must hold or the action reverts before the body runs.

```covenant
module Ledger {
    field owner:    address
    field paused:   bool
    field balances: map<address, amount>

    event Transfer(sender: address indexed, recipient: address indexed, value: amount)

    action send(recipient: address, value: amount)
            when balances[caller] >= value,
            when !paused {
        balances[caller]    -= value
        balances[recipient] += value
        emit Transfer(caller, recipient, value)
    }

    action set_paused(flag: bool) only owner {
        paused = flag
    }
}
```

Note the two renames in that sample. `transfer` is the reserved keyword
`KwTransfer` and cannot be an action name (`E020 expected action name, got
KwTransfer`), and `to` is `KwTo` and cannot be a parameter name (`E020 expected
argument name, got KwTo`). Both are rejected in every construct, so pick other
identifiers, `send` and `recipient` here.

| Guard | Meaning |
|-------|---------|
| `when expr` | Arbitrary boolean expression must be true |
| `given expr` | Same thing. `given` and `when` lower to byte-identical output |
| `only owner` | `caller` must equal the `owner` address. Requires `field owner: address` |
| `only admin` | `caller` must equal the `admin` address. Requires `field admin: address` |
| `only deployer` | `caller` must equal the deployment address |
| `only <address literal>` | `caller` must equal that hard-coded address |
| `pq_signed(content, sig, key)` | Dilithium-5 signature verification (ERC-8231, Fortress layer). `key` must be a stored `pq_key` **field**, a `pq_key` parameter is refused with `E505` because dynamic-`bytes` ABI decoding would corrupt it |
| `verified_by(proof)` | Recursive IVC proof verification (ERC-8229, Prism layer) |

Multiple guards are comma-separated, **not** joined with `&&`. Inside a single
`when` expression, disjunction is `||`; there is no `or` keyword.

`only` resolves seven names plus a literal address. Only some of them are
enforced, and the compiler tells you which:

- enforced: `deployer`, an explicit address such as `only 0x00..01`, and
  `owner` or `admin` when a same-named `address` field is declared
- `only owner` or `only admin` with no such field declared compiles but emits
  `W421 only with unresolved principal cannot be enforced at runtime; the
  action will revert on every call`, which bricks the action on an immutable
  contract
- `guardians`, `parties` and `holders` resolve but emit `W421` too, as
  collection-typed principals with no codegen
- `only caller` compiles with `W508`: it lowers to `msg.sender == msg.sender`
  and restricts nothing
- an identifier outside those seven is `E106 unknown principal predicate`
- a principal whose declared type is not `address` is `E436`

Treat `W421` and `W508` as errors: both mean the guard you wrote does not
guard anything.

### Guards the compiler refuses

These are named in older material but do not compile at v0.9.7. Do not generate
them.

| Guard | Diagnostic at v0.9.7 | Write instead |
|-------|----------------------|---------------|
| `only first_time_caller` | `E518`, no real EVM authorization check, the guard "would silently pass for every caller" | `field called: map<address, bool>` plus `when !called[caller]`, setting `called[caller] = true` in the body |
| `only registered_key` | `E518`, same reason | `field keys: map<address, amount>` plus `when keys[caller] != 0` |
| `only registered_account` | `E106 unknown principal predicate`. No such predicate exists, in a token context or anywhere else | `field accounts: map<address, bool>` plus `when accounts[caller]` |
| `given x in collection` | `E426`, the `in` membership operator has no lowering. It used to compile to equality against the FIRST element only | `given x == a \|\| x == b` |
| `vdf_locked(handle, time)` | Does not even parse. The parser demands `for` after the keyword and rejects the open paren with `E020`. The real spelling takes one operand and no parentheses, `vdf_locked for <duration>`, and that spelling is then refused with `E517`, no EVM lowering (KSR-CVN-023) | `field unlock_at: time` plus `when now >= unlock_at` |

---

## f) ERC-822x construct mapping (Styx Protocol)

| ERC | Covenant trigger | Auto-synthesized surface |
|-----|-----------------|--------------------------|
| **ERC-8227**, Confidential Token Interface | `confidential token C { }` | `transferEncrypted`, `balanceOfEncrypted`, `approveEncrypted` |
| **Amnesia Ceremony** (ERC-8228, Cryptographic Amnesia, Styx Protocol) | `ceremony C { }` with `on_destroy` / `destroy()` | `setup`, `submit_share`, `finalize`, `destroy`, `phase`, `is_destroyed`, `session_id`, `owner`, lifecycle: Setup → Active → Finalized → Destroyed |
| **ERC-8229**, FHE Computation Verification | `verified_by(zk_proof)` guard on an action | Halo2 SNARK + Nova IVC proof verification at action entry |
| **ERC-8231**, Post-Quantum Signature Verification | `pq_signed(content, sig, key)` guard on an action | Dilithium-5 signature check at action entry |

Citing the ERC number in a `--` comment near the construct is a convention only.
Nothing checks it: `lint --deep --severity info` reports the same (nothing) with
the comment present and with it removed. Example:

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

`vault` does **not** insert reentrancy protection in this release. The construct
emits `W606` and passes through unchanged: the same body with the keyword changed
to `module` produces byte-identical `runtime.bin`, `bin`, `abi.json`,
`metadata.json` and `storage.json`. No lock slot is allocated. Running `lint` on
the sample below reports `W003 action withdraw makes an external call with no
reentrancy protection, and this release has none to offer`, exactly as it does
for the `module` version.

`@non_reentrant` is not writable either. The resolver's annotation allowlist is
`@precompute`, `@batch_up_to`, `@prove_offchain`, `@gas_budget`, so writing
`@non_reentrant` on an action is a hard error, `E110 unknown annotation
@non_reentrant`, and the build fails. Placing it above the construct is worse
still, `E028` plus `E020`.

So guard the withdraw path yourself, checks-effects-interactions: debit storage
**before** the `transfer`, as the sample below does. One caveat if you are
tempted to swap `vault` for `module` wholesale: the two are not interchangeable in
every case, because the resolver seeds `opens_at` and `closes_at` for `vault` and
`market` but not for `module`, so a body referencing those gives `E102` once
converted.

```covenant
-- vault adds no reentrancy guard: debit storage BEFORE the transfer
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
| **E106** | an `only <name>` clause whose name is neither a declared field nor a known predicate | Error, unknown principal predicate. `only registered_account` lands here: it does not exist. |
| **E110** | an unknown annotation, including `@non_reentrant` | Error, not a warning. The whole allowlist is `@precompute`, `@batch_up_to`, `@prove_offchain`, `@gas_budget`. |
| **E424** | stdlib math builtins `min` / `max` / `abs` / `pow` / `sqrt` | Not implemented → error. Write the arithmetic explicitly instead. |
| **E425** | map introspection `.length` / `.keys` / `.values` | Unsupported → error. Track size/keys in a separate field. |
| **E426** | the `in` membership operator (`given x in list`) | Not implemented → error. Use an explicit lookup / `map` membership. |
| **E427** | map `.argmax` / `.argmin` | Unsupported → error. (List `.argmax` / `.argmin` **do** work.) |
| **E512** | an `event` with **>3** `indexed` params | Error, max 3 indexed params. Mark at most 3 parameters `indexed`. Covenant has no anonymous-event syntax, so that escape hatch is not writable: `anonymous event Ev(…)` is `E020`, `event Ev(…) anonymous` is `E030`, `@anonymous` is `E021`. |
| **E519** | division / modulo by a **literal** zero | Error. (A non-literal divisor instead gets a runtime guard.) |
| **E520** | a missing precompile helper method | Error, the referenced precompile helper does not exist. |
| **E521** | a `text` constant longer than **32 bytes** in a **return position** | Error, the V0 return encoder emits at most 32 bytes. Keep returned string constants ≤ 32 bytes. This is not a limit on `text` constants generally: a 33-byte constant in `field s: text = "…"` or assigned in a body builds clean. |
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
| **E505** | a `pq_key` (or other dynamic-`bytes`) value in an ABI parameter or return position | Error. The codegen can only move it as one 32-byte word, so a spec-compliant caller's encoding would corrupt it. Hold the key in a field instead. This is also what makes `registry` uncompilable. |
| **E517** | the `vdf_locked for <duration>` qualifier | Error, no EVM lowering (KSR-CVN-023). A time-locked action would be instant at runtime. Use `when now >= unlock_at`. |
| **E518** | `only first_time_caller`, `only registered_key` | Error. The predicate has no real EVM check and would pass for every caller. Track the state in a `map` and guard with `when`. |
| **E530** | a `hex` literal wider than 32 bytes | Error. A single PUSH carries at most 32 bytes, so the excess was emitted as executable bytecode. |
| **E531** | **reading** a bare struct-typed field (`field cfg: Cfg`, then `cfg.w` in an expression) | Error, reads returned the NEXT declared field. Use a list of structs (`field cfgs: [Cfg]`), whose element access IS lowered. The error fires on the read only: declaring `field cfg: Cfg` builds clean, and so does a write `cfg.w = v`, which is still silently dropped (byte-identical to an empty body). |
| **E532** | an `indexed` event parameter of a dynamic type | Error. The topic was a zero placeholder, so two logs with different values had identical topics. |
| **E640** | `supply: N to <principal>` where the principal is not `deployer` | Error. It minted nothing at all. Use `supply: N to deployer` plus a deployer-guarded action to move the balance. |
| **E641** | a `total_supply` field default that contradicts the genesis mint | Error. The default silently won over the mint amount. |
| **E642** | `decimals` outside the EIP-20 uint8 range | Error. |
| **E643** | a user event or error shadowing a synthesized one with a different shape | Error. It produced a broken ABI. |
| **W440** | a `given <cond>` guard that reads a field the body writes | Warning. `given` compiles as a PRECONDITION asserted before the body runs, so such a guard sees the OLD value. Narrow trigger: `given n <= 10 { n = n + v }` warns, `given n <= 10 { x = x + v }` and `given v >= a` are silent. |
| **W530** | a non-indexed event parameter of a dynamic type | Warning. The log data word is a zero placeholder, so a decoder reading offset plus length gets nothing. |

Guard principals **fail closed**, but not under the codes older material named for
them. At v0.9.7:

- an unknown principal name is `E106 unknown principal predicate`
- a principal whose declared type is not `address` is `E436`
- a built-in predicate with no real EVM check (`first_time_caller`,
  `registered_key`) is `E518`
- a declared field that is not `owner`, `admin` or `deployer` compiles but emits
  `W421` and reverts on every call

None of these silently allows the action. `E516` is the unlowered-amnesia-opcode
code and `E517` the unlowered-`vdf_locked` code; neither has anything to do with
principals, and neither is historical: `E517` fires on v0.9.7 today for
`vdf_locked for <duration>`.

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

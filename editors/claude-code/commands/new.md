---
name: covenant-new
description: >-
  Scaffold a Covenant v0.9.7 .cov file for the given top-level construct.
  Every template below builds at v0.9.7 except `registry`, which does not
  compile in any form at this release. Usage: /covenant-new <construct> [Name]
---

# /covenant-new

Scaffold a new Covenant v0.9.7 source file for the specified top-level construct.

## Usage

```
/covenant-new <construct> [Name]
```

`<construct>` is one of (case-insensitive):
`token` · `confidential token` · `vault` · `record` · `ballot` · `counter` ·
`encrypted counter` · `board` · `market` · `registry` · `bridge` · `ceremony` ·
`module` · `hybrid module`

`registry` is accepted but has no buildable template at v0.9.7. See the
`registry` section below.

`[Name]` is optional. If omitted, prompt the user for it before writing.

## Behavior

1. Parse the construct keyword (case-insensitive, handle spaces in two-word keywords).
2. If no `Name` is given, ask: "What should the contract be named?"
3. Scaffold using the v0.9.7 template below for that construct.
   `registry` is the one exception: it has no buildable template at this
   release. Do not write a `registry` file. Report the E505 diagnostic quoted
   in its section and offer the `module` fallback shown there instead.
4. Write to `covenant/<Name>.cov` relative to the project root.
   Create the `covenant/` directory if it does not exist.
5. Print the path written.

Every `covenant` code block below was run through `covenant build` at v0.9.7
and produced an artifact. The templates follow these invariants:
- Comments use `--` (never `//`)
- Types use `amount`, `text`, `bool`, `address`, `hash`, `time`, `map<K,V>`.
  `caller` and `now` are builtin expressions, not types: `field who: caller`
  is rejected (E102 in field position, E231 in parameter and return position)
- Actions use `action` / `view` (not `function`)
- Guards use `when` / `only` / `given` (not `require` / `modifier`)
- Of the `only` principals, `deployer` and an explicit `only <address>` are
  enforced outright, and `owner` / `admin` are enforced when a matching
  `field owner` / `field admin` is declared. Without that field they still
  build but warn W421: the action reverts on every call, fail-closed.
  `only first_time_caller` and `only registered_key` are refused outright
  (E518, no real EVM check), and `registered_account` does not exist (E106)
- `vault` adds **no** reentrancy guard. Its codegen is identical to `module`
  (`build` emits W606, vault synthesis is not implemented), `lint` reports
  W003 on any action that transfers, and `@non_reentrant` is not an accepted
  annotation at v0.9.7 (E110, valid set is `@precompute`, `@batch_up_to`,
  `@prove_offchain`, `@gas_budget`)
- No Solidity visibility modifiers (`public`, `external`, etc.)

## Templates

### token

```covenant
-- <Name>: ERC-20 fungible token
token <Name> {
    symbol:   "<SYM>"
    name:     "<Full Name>"
    decimals: 18
    supply:   1_000_000 to deployer
}
```

### confidential token

```covenant
-- ERC-8227: Confidential Token Interface (Styx Protocol)
confidential token <Name> {
    symbol:   "<SYM>"
    name:     "<Full Name>"
    decimals: 18
    supply:   1_000_000 to deployer
}
```

### vault

```covenant
-- <Name>: value custody vault.
-- No reentrancy guard at v0.9.7: this compiles to the same bytecode as the
-- identical body written as a `module`, and `lint` reports W003 on withdraw.
-- `@non_reentrant` cannot be added, it is not an accepted annotation (E110).
-- Follow checks-effects-interactions by hand, as withdraw does below.
vault <Name> {
    field balances: map<address, amount>

    event Deposit(who: address, value: amount)
    event Withdrawal(who: address, value: amount)

    error InsufficientBalance(required: amount, actual: amount)

    action deposit()
            when 1 > 0 {
        balances[caller] += 1
        emit Deposit(caller, 1)
    }

    action withdraw(value: amount)
            when balances[caller] >= value {
        balances[caller] -= value
        transfer(value) to caller
        emit Withdrawal(caller, value)
    }

    view balance_of(who: address) returns amount {
        balances[who]
    }
}
```

### record

```covenant
-- <Name>: key-value storage. No getters are synthesized; write the views yourself.
record <Name> {
    owner:   address
    count:   amount
    enabled: bool
}
```

### ballot

```covenant
-- <Name>: on-chain vote
-- `only first_time_caller` does NOT build at v0.9.7 (E518): the predicate has
-- no real EVM authorization check, so the compiler refuses it rather than ship
-- a guard that passes for every caller. `only deployer`, an explicit
-- `only <address>`, and `only owner` / `only admin` backed by a declared
-- `field owner` / `field admin` are the qualifiers that are actually enforced
-- at v0.9.7.
-- One-vote-per-address therefore has to be written by hand, for example with
-- a `map<address, bool>` and a `when` guard.
-- This template is not warning-free: `build` emits W606 (ballot synthesis is
-- not implemented, the construct passes through unchanged) and W530 on the
-- `Voted` event, because `choice: text` is published in the ABI as a dynamic
-- type while this release writes a single placeholder word in the log, so a
-- caller decoding that log per the published ABI misreads it.
ballot <Name> {
    field votes: map<address, text>
    field tally: map<text, amount>

    event Voted(who: address, choice: text)

    action vote(choice: text) {
        votes[caller] = choice
        tally[choice] += 1
        emit Voted(caller, choice)
    }

    view result(choice: text) returns amount {
        tally[choice]
    }
}
```

### counter

```covenant
-- <Name>: simple incrementing counter
counter <Name> {
    field total: amount

    event Incremented(by: address, new_total: amount)

    action increment() {
        total += 1
        emit Incremented(caller, total)
    }

    view value returns amount {
        total
    }
}
```

### encrypted counter

```covenant
-- <Name>: TFHE-encrypted counter (Veil layer)
encrypted counter <Name> {
    total: amount

    action bump(by: amount) {
        total += by
    }

    reveal total to owner
}
```

### board

There is no appending `board` that builds at v0.9.7. An inline `post { .. }`
schema allocates no storage slot, so `append post { .. }` is E430 and every
read of `posts` is E431:

```text
[E430] `append post { .. }` cannot be compiled: `post` has no storage field,
       so there is nowhere to write the element.
[E431] `posts` has no storage field and cannot be read.
```

The E430 help text suggests declaring `posts: [Entry] = []` alongside a
top-level `struct Entry { .. }`, but `struct` is not a top-level construct
keyword at v0.9.7 (E028), `append posts <expr>` does not parse (E020), and
`posts.push(..)` is E207. The `board` construct itself is sound, so the
template keeps a counter and puts the payload in the event log:

```covenant
-- <Name>: message board, counter plus event log.
-- Storing the posts on chain is not available at v0.9.7: `append post { .. }`
-- is E430 and any read of `posts` is E431. Indexers read the event instead.
board <Name> {
    field post_count: amount

    event Submitted(who: address, content: hash, at: time)

    action submit(content: hash) {
        post_count += 1
        emit Submitted(caller, content, now)
    }

    view count returns amount {
        post_count
    }
}
```

### market

```covenant
-- <Name>: order-book scaffold, read-only.
-- `priority_queue<K, V, max|min>` is read-only at v0.9.7: `.top_key`,
-- `.top_value` and `.length` exist, `.push` does not (E207). The ordering
-- parameter is the closed set `max|min`; `asc`/`desc` do not parse (E020).
-- This construct cannot accept orders yet, only report the book's top.
market <Name> {
    field asks: priority_queue<amount, address, min>
    field bids: priority_queue<amount, address, max>

    view best_ask   returns amount  { asks.top_key }
    view best_bid   returns amount  { bids.top_key }
    view ask_maker  returns address { asks.top_value }
    view bid_maker  returns address { bids.top_value }
    view ask_depth  returns amount  { asks.length }
    view bid_depth  returns amount  { bids.length }
}
```

### registry

**`registry` does not build at v0.9.7. Do not scaffold one.**

The ERC-8231 synthesizer injects `register` and `key_of` over `pq_key` into
every `registry`, and `pq_key` lowers to a dynamic ABI `bytes`, which this
release's codegen refuses. Even the emptiest possible body fails:

```text
$ covenant build Demo.cov --out out
[E505] `key_of` uses `pq_key` (ABI type `bytes`, dynamic) in a position that
       this release's codegen can only read/return as a single 32-byte word
       -- a spec-compliant caller's ABI encoding would silently corrupt the
       value with no error. Refusing to compile rather than shipping a
       compiler-caused key-corruption bug. Real dynamic-`bytes` ABI support
       is tracked in DEBT.md.
   [Demo.cov:1:10]
[E505] `register` uses `pq_key` ... (same)
   [Demo.cov:1:10]
error: compilation failed (2 error(s))
```

That is for the one-line source `registry Demo { field x: amount }`. Declaring
your own `key_of` replaces that pair with a single E601, because it collides
with the synthesized ERC-8231 surface: the run reports one error and no E505.
Renaming your actions does not help either: the synthesized pair fails on its
own. Note that `covenant check` exits 0 here, so the failure
only shows up under `build`. `--target-chain aster` also exits 0, but it emits
a 9-byte, zero-function placeholder that is explicitly not deployable.

Until dynamic-`bytes` ABI support lands, scaffold a `module` with a fixed-size
`hash` key instead:

```covenant
-- <Name>: key registry, `module` fallback.
-- `registry` does not build at v0.9.7 in any form (E505), so this stores a
-- fixed-size `hash` per address instead of a `pq_key`.
module <Name> {
    field keys: map<address, hash>

    event KeyRegistered(who: address)

    action enroll(key: hash) {
        keys[caller] = key
        emit KeyRegistered(caller)
    }

    view key_of(who: address) returns hash {
        keys[who]
    }
}
```

### bridge

```covenant
-- <Name>: cross-chain escrow bridge
bridge <Name> anchored_on ["ethereum", "aster"] {
    field locked: map<address, amount>

    event Locked(who: address, value: amount)
    event Unlocked(who: address, value: amount)

    action lock(value: amount)
            when value > 0 {
        locked[caller] += value
        emit Locked(caller, value)
    }

    action unlock(value: amount)
            when locked[caller] >= value {
        locked[caller] -= value
        transfer(value) to caller
        emit Unlocked(caller, value)
    }
}
```

### ceremony

```covenant
-- ERC-8228: Cryptographic Amnesia (Styx Protocol)
ceremony <Name> {
    guardians: 3
    threshold: 2

    on_destroy {
        destroy(0)
    }
}
```

### module

```covenant
-- <Name>: generic module (prefer a specialized construct when possible)
module <Name> {
    field owner:  address
    field active: bool

    action initialize()
            only deployer {
        owner  = caller
        active = true
    }

    view is_active returns bool {
        active
    }
}
```

### hybrid module

```covenant
-- <Name>: mixed plaintext + FHE-encrypted state (Veil layer)
hybrid module <Name> {
    field headcount:          amount   -- plaintext
    field encrypted treasury: amount   -- TFHE ciphertext

    action deposit(value: amount)
            when value > 0 {
        headcount  += 1
        treasury   += value
    }

    view participants returns amount {
        headcount
    }

    reveal treasury to owner
}
```

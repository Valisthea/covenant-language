---
name: covenant-new
description: >-
  Scaffold a minimally-compiling Covenant V0.9 .cov file for the given
  top-level construct. Usage: /covenant-new <construct> [Name]
---

# /covenant-new

Scaffold a new Covenant V0.9 source file for the specified top-level construct.

## Usage

```
/covenant-new <construct> [Name]
```

`<construct>` is one of (case-insensitive):
`token` · `confidential token` · `vault` · `record` · `ballot` · `counter` ·
`encrypted counter` · `board` · `market` · `registry` · `bridge` · `ceremony` ·
`module` · `hybrid module`

`[Name]` is optional. If omitted, prompt the user for it before writing.

## Behavior

1. Parse the construct keyword (case-insensitive, handle spaces in two-word keywords).
2. If no `Name` is given, ask: "What should the contract be named?"
3. Scaffold using the verified V0.9 template below for that construct.
4. Write to `covenant/<Name>.cov` relative to the project root.
   Create the `covenant/` directory if it does not exist.
5. Print the path written.

All templates follow these invariants verified against compiler fixtures:
- Comments use `--` (never `//`)
- Types use `amount`, `text`, `map<K,V>`, `caller`
- Actions use `action` / `view` (not `function`)
- Guards use `when` / `only` / `given` (not `require` / `modifier`)
- `vault` does **not** add `@non_reentrant` (already the default)
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
-- <Name>: value custody vault (@non_reentrant by default)
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
        transfer(value, to: caller)
        emit Withdrawal(caller, value)
    }

    view balance_of(who: address) returns amount {
        balances[who]
    }
}
```

### record

```covenant
-- <Name>: key-value storage with auto-getters
record <Name> {
    owner:   address
    count:   amount
    enabled: bool
}
```

### ballot

```covenant
-- <Name>: on-chain vote
ballot <Name> {
    field votes: map<address, text>
    field tally: map<text, amount>

    event Voted(who: address, choice: text)

    action vote(choice: text)
            only first_time_caller {
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

```covenant
-- <Name>: append-only message board
board <Name> {
    post {
        author:  address
        content: hash
        at:      time
    }

    action submit(content: hash) {
        append post {
            author:  caller
            content: content
            at:      now
        }
    }

    view count returns amount { posts.length }
    view get(i: amount) returns post when i < posts.length { posts[i] }
}
```

### market

```covenant
-- <Name>: order-book marketplace
market <Name> {
    field asks: priority_queue<amount, address, asc>
    field bids: priority_queue<amount, address, desc>

    event AskPlaced(who: address, price: amount)
    event BidPlaced(who: address, price: amount)

    action ask(price: amount)
            when price > 0 {
        asks.push(price, caller)
        emit AskPlaced(caller, price)
    }

    action bid(price: amount)
            when price > 0 {
        bids.push(price, caller)
        emit BidPlaced(caller, price)
    }
}
```

### registry

```covenant
-- <Name>: post-quantum key registry (Fortress layer)
registry <Name> {
    field keys: map<address, pq_key>

    event KeyRegistered(who: address)

    action register(key: pq_key)
            only first_time_caller {
        keys[caller] = key
        emit KeyRegistered(caller)
    }

    view key_of(who: address) returns pq_key {
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
        transfer(value, to: caller)
        emit Unlocked(caller, value)
    }
}
```

### ceremony

```covenant
-- Amnesia Ceremony — Covenant construct (no assigned ERC; ERC-8228 is the unrelated Styx Encrypted Token Standard)
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

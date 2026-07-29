#!/usr/bin/env python3
"""
Generate examples.covenant-lang.org: Covenant By Example
Produces: docs/examples/index.html + docs/examples/NN-slug.html (×15)
"""

import os, re, textwrap

OUT = os.path.join(os.path.dirname(__file__), "..", "docs", "examples")
os.makedirs(OUT, exist_ok=True)

# ─────────────────────────────────────────────
# CHAPTER DATA
# ─────────────────────────────────────────────

SECTIONS = [
    {"id": "fundamentals", "label": "Fundamentals", "color": "var(--accent)"},
    {"id": "standards",    "label": "Standards",    "color": "var(--cyan)"},
    {"id": "advanced",     "label": "Advanced",     "color": "var(--green)"},
]

CHAPTERS = [
    # ── Fundamentals ──────────────────────────────────────────────────────────
    {
        "num": 1, "slug": "hello-contract",
        "title": "Hello Contract",
        "subtitle": "The simplest Covenant contract: record, action, view.",
        "section": "fundamentals",
        "intro": """\
Every Covenant contract is a <em>record</em>: a typed bundle of persistent storage
fields and the actions that modify them. This chapter builds the minimal contract, 
analogous to "Hello World" but running on an EVM chain.""",
        "blocks": [
            {
                "label": "hello.cov",
                "lang": "covenant",
                "code": """\
record Hello {
    greeting: text;

    action update(new_greeting: text) {
        self.greeting = new_greeting;
        emit Updated(new_greeting);
    }

    view read() -> text {
        return self.greeting;
    }
}"""
            },
        ],
        "notes": [
            ("<code>record</code>", "declares a contract with named storage fields."),
            ("<code>text</code>", "is Covenant's built-in string type, equivalent to <code>string</code> in Solidity."),
            ("<code>action</code>", "is a state-mutating function, it produces a transaction."),
            ("<code>view</code>", "reads state without modifying it; no gas cost when called off-chain."),
            ("<code>emit Updated(...)</code>", "emits a log event. The event type is inferred from the arguments."),
            ("<code>self.greeting</code>", "refers to the contract's own storage field."),
        ],
        "takeaways": [
            "Records are the primary contract abstraction in Covenant.",
            "Actions and views are declared at the same level, no <code>function</code> keyword ambiguity.",
            "The compiler infers ABI, storage layout, and event signatures automatically.",
        ],
    },
    {
        "num": 2, "slug": "fields-and-storage",
        "title": "Fields and Storage",
        "subtitle": "Every primitive type, plus arrays and maps.",
        "section": "fundamentals",
        "intro": """\
Covenant's type system maps cleanly to EVM storage slots while adding
semantic types that prevent common mistakes, for example, using <code>amount</code>
instead of <code>u256</code> for ether values makes units explicit.""",
        "blocks": [
            {
                "label": "storage_types.cov",
                "lang": "covenant",
                "code": """\
record Catalog {
    // Semantic primitives
    owner:      address;
    balance:    amount;       // wei: prevents unit confusion
    created:    time;         // unix timestamp
    lock_for:   duration;     // seconds
    checksum:   hash;         // bytes32

    // Text
    name:       text;

    // Integer widths
    small:      u8;
    counter:    u64;
    big:        u256;
    signed_val: i256;

    // Boolean
    active:     bool;

    // Fixed-size array (max 32 entries)
    tags:       [text; 32];

    // Dynamic map
    deposits:   map(address => amount);

    // Nested map
    allowances: map(address => map(address => amount));
}"""
            },
            {
                "label": "Reading and writing fields",
                "lang": "covenant",
                "code": """\
record Counter {
    count: u64;
    owner: address;

    action increment() only(self.owner) {
        self.count += 1;
    }

    action reset() only(self.owner) {
        self.count = 0;
    }

    view get() -> u64 {
        return self.count;
    }
}"""
            },
        ],
        "notes": [
            ("<code>amount</code>", "compiles to <code>uint256</code> but the compiler rejects arithmetic that mixes amounts with bare integers."),
            ("<code>time</code> / <code>duration</code>", "prevent accidental subtraction of timestamps and durations, the compiler enforces dimensional compatibility."),
            ("<code>map(K => V)</code>", "compiles to a Solidity mapping; nested maps compile to nested mappings."),
            ("<code>[T; N]</code>", "is a fixed-size array stored packed into adjacent slots when possible."),
        ],
        "takeaways": [
            "Semantic types (<code>amount</code>, <code>time</code>, <code>duration</code>) prevent an entire class of unit bugs at compile time.",
            "Maps and arrays have familiar syntax but compile to efficient EVM layouts.",
            "Field initialization to zero is implicit: no constructor boilerplate needed.",
        ],
    },
    {
        "num": 3, "slug": "actions-events-errors",
        "title": "Actions, Events & Errors",
        "subtitle": "emit, transfer, revert_with: the full action vocabulary.",
        "section": "fundamentals",
        "intro": """\
Actions in Covenant carry four responsibilities: validate inputs, update state,
emit events, and transfer value. Covenant's <code>revert_with</code> attaches
typed error payloads: readable by any ABI-aware client without extra decoding.""",
        "blocks": [
            {
                "label": "bank.cov",
                "lang": "covenant",
                "code": """\
record Bank {
    balances: map(address => amount);

    // Named events
    event Deposited(who: address, value: amount);
    event Withdrawn(who: address, to: address, value: amount);

    // Typed errors
    error InsufficientFunds(requested: amount, available: amount);
    error ZeroTransfer();

    action deposit() {
        let val = msg.value;
        if val == 0 {
            revert_with ZeroTransfer();
        }
        self.balances[msg.sender] += val;
        emit Deposited(msg.sender, val);
    }

    action withdraw(to: address, value: amount) {
        let avail = self.balances[msg.sender];
        if value == 0 {
            revert_with ZeroTransfer();
        }
        if value > avail {
            revert_with InsufficientFunds(value, avail);
        }
        // Checks-Effects-Interactions order
        self.balances[msg.sender] -= value;
        emit Withdrawn(msg.sender, to, value);
        transfer(to, value);
    }

    view balance_of(who: address) -> amount {
        return self.balances[who];
    }
}"""
            },
        ],
        "notes": [
            ("<code>event</code>", "declares a log event with named, typed fields. The ABI selector is derived automatically."),
            ("<code>error</code>", "declares a typed revert payload, EIP-838 compatible. Arguments are ABI-encoded into revert data."),
            ("<code>revert_with E(...)</code>", "reverts the transaction and encodes the error payload. Equivalent to Solidity's <code>revert E(...)</code>."),
            ("<code>msg.value</code>", "the ether attached to the call, as an <code>amount</code>."),
            ("<code>transfer(to, value)</code>", "sends ether using <code>call{value: v}('')</code> with a forward-gas cap. Reverts on failure."),
        ],
        "takeaways": [
            "Events and errors are declared at the record level, they are part of the contract's public ABI.",
            "<code>revert_with</code> always carries a typed payload; bare <code>revert</code> is not idiomatic Covenant.",
            "Follow Checks-Effects-Interactions order: update state before calling <code>transfer()</code>.",
        ],
    },
    {
        "num": 4, "slug": "guards",
        "title": "Guards",
        "subtitle": "only, when, given: declarative access control and invariants.",
        "section": "fundamentals",
        "intro": """\
Covenant's guard system replaces ad-hoc <code>require()</code> chains with
<em>declared</em> pre- and post-conditions. Guards appear in the function
signature: they are part of the interface, not buried in the body.
<br><br>
<strong>Security note:</strong> OMEGA V4 finding <code>KSR-CVN-011</code> (Critical)
discovered that a prior compiler version silently dropped <code>only()</code>
clauses during IR lowering: every deployed contract was wide open. V0.7 fixes
this and adds an integration test that verifies guard bytecode is emitted.""",
        "blocks": [
            {
                "label": "guards.cov",
                "lang": "covenant",
                "code": """\
record Multisig {
    owner:    address;
    guardian: address;
    locked:   bool;
    nonce:    u64;

    error Unauthorized();
    error AlreadyLocked();

    // only(expr): caller must satisfy expr, else revert
    action change_owner(new_owner: address) only(msg.sender == self.owner) {
        self.owner = new_owner;
    }

    // when(cond): precondition checked before body runs
    action lock() only(msg.sender == self.owner) when(!self.locked) {
        self.locked = true;
    }

    // Multiple guards compose with AND semantics
    action unlock(sig: bytes)
        only(msg.sender == self.guardian)
        when(self.locked)
    {
        self.locked = false;
        self.nonce += 1;
    }

    // given(cond): postcondition checked after body runs
    // Reverts the entire action if the condition is false after execution
    action set_guardian(g: address)
        only(msg.sender == self.owner)
        given(g != address(0))
        given(g != self.owner)
    {
        self.guardian = g;
    }

    // Shorthand: only(self.owner) === only(msg.sender == self.owner)
    action emergency_reset() only(self.owner) {
        self.locked = false;
        self.nonce += 1;
    }
}"""
            },
        ],
        "notes": [
            ("<code>only(expr)</code>", "emits a <code>JUMPI</code> at function entry that reverts if <code>expr</code> is false. Shorthand <code>only(self.owner)</code> expands to <code>only(msg.sender == self.owner)</code>."),
            ("<code>when(cond)</code>", "a precondition: checked at the start of the action body, before any state changes."),
            ("<code>given(cond)</code>", "a postcondition: checked after the body executes. Reverts (and undoes state changes) if false."),
            ("Multiple guards", "compose with AND semantics, all conditions must hold."),
            ("KSR-CVN-011", "was critical because guards look present in source but produced zero bytecode. Always run <code>covenant audit</code> before deployment."),
        ],
        "takeaways": [
            "Guards are part of the function signature: they appear in the generated ABI docs and are impossible to overlook.",
            "<code>given()</code> postconditions let you express invariants that must hold after the action, useful for conservation laws.",
            "The compiler verifies that every <code>only()</code> clause produces bytecode (enforced since V0.7).",
        ],
    },
    {
        "num": 5, "slug": "external-calls",
        "title": "External Contract Calls",
        "subtitle": "interface, @non_reentrant, and the CEI pattern.",
        "section": "fundamentals",
        "intro": """\
Calling other contracts is unavoidable in DeFi. Covenant's <code>interface</code>
keyword declares the external ABI, and <code>@non_reentrant</code> inserts a
mutex at the bytecode level. The Checks-Effects-Interactions pattern is
enforced by convention and auditable via OMEGA lint rule <code>CEI-001</code>.""",
        "blocks": [
            {
                "label": "interfaces.cov",
                "lang": "covenant",
                "code": """\
// Declare the external contract's ABI
interface IERC20 {
    action transfer(to: address, amount: u256) -> bool;
    action transferFrom(from: address, to: address, amount: u256) -> bool;
    view balanceOf(who: address) -> u256;
    view allowance(owner: address, spender: address) -> u256;
}

interface IUniswapV2Pair {
    action swap(
        amount0Out: u256,
        amount1Out: u256,
        to: address,
        data: bytes
    );
    view getReserves() -> (u112, u112, u32);
}"""
            },
            {
                "label": "vault.cov: non-reentrant external calls",
                "lang": "covenant",
                "code": """\
record Vault {
    token:   address;
    shares:  map(address => u256);
    total:   u256;

    error NothingToWithdraw();

    // @non_reentrant inserts an on-chain mutex (SSTORE lock slot)
    @non_reentrant
    action deposit(amount: u256) {
        let erc20 = IERC20(self.token);

        // Checks
        // (amount > 0 validated by the caller passing a nonzero value)

        // Effects: update state BEFORE external call
        let new_shares = amount;   // simplified 1:1 for illustration
        self.shares[msg.sender] += new_shares;
        self.total += amount;
        emit Deposited(msg.sender, amount, new_shares);

        // Interactions: external call LAST
        erc20.transferFrom(msg.sender, self.addr(), amount);
    }

    @non_reentrant
    action withdraw() {
        let user_shares = self.shares[msg.sender];
        if user_shares == 0 {
            revert_with NothingToWithdraw();
        }

        // Effects before Interactions
        self.shares[msg.sender] = 0;
        self.total -= user_shares;
        emit Withdrawn(msg.sender, user_shares);

        // Interaction last
        IERC20(self.token).transfer(msg.sender, user_shares);
    }

    view share_of(who: address) -> u256 {
        return self.shares[who];
    }

    event Deposited(who: address, amount: u256, shares: u256);
    event Withdrawn(who: address, shares: u256);
}"""
            },
        ],
        "notes": [
            ("<code>interface I { ... }</code>", "declares an external contract type. Calling <code>I(addr).method()</code> compiles to a <code>CALL</code> with the correct 4-byte selector."),
            ("<code>@non_reentrant</code>", "inserts a transient storage mutex (EIP-1153 where available, SSTORE otherwise). Reverts if the function is re-entered."),
            ("<code>self.addr()</code>", "returns the contract's own address, equivalent to Solidity's <code>address(this)</code>."),
            ("CEI pattern", "is not enforced at compile time in V0.7, but OMEGA lint rule <code>CEI-001</code> flags violations. Future versions will enforce it statically."),
        ],
        "takeaways": [
            "Declare interfaces explicitly: Covenant will generate the correct selectors and ABI types.",
            "Always use <code>@non_reentrant</code> on any action that makes external calls.",
            "State updates must precede external calls (CEI). OMEGA will flag violations.",
        ],
    },
    # ── Standards ─────────────────────────────────────────────────────────────
    {
        "num": 6, "slug": "erc20-token",
        "title": "ERC-20 Token",
        "subtitle": "Six lines versus one hundred: the token keyword.",
        "section": "standards",
        "intro": """\
The <code>token</code> keyword is syntactic sugar that expands to a fully
compliant ERC-20 implementation. Covenant generates <code>transfer</code>,
<code>transferFrom</code>, <code>approve</code>, <code>allowance</code>,
<code>balanceOf</code>, <code>totalSupply</code>, and the required events, 
all generated by the compiler from the single <code>token</code> declaration.""",
        "blocks": [
            {
                "label": "my_token.cov: using the token keyword",
                "lang": "covenant",
                "code": """\
token MyToken {
    name:     "My Token";
    symbol:   "MTK";
    decimals: 18;
    supply:   1_000_000_000;   // minted to deployer
    owner:    msg.sender;
}"""
            },
            {
                "label": "Equivalent generated interface (for reference only)",
                "lang": "covenant",
                "code": """\
// This is what the compiler produces: you never write this manually.
// Shown here so you understand what the token keyword expands to.

record MyToken {
    name_:       text    = "My Token";
    symbol_:     text    = "MTK";
    decimals_:   u8      = 18;
    total_:      u256    = 1_000_000_000 * (10 ** 18);
    owner:       address;
    balances:    map(address => u256);
    allowances:  map(address => map(address => u256));

    event Transfer(from: address, to: address, value: u256);
    event Approval(owner: address, spender: address, value: u256);

    action transfer(to: address, amount: u256) -> bool { ... }
    action transferFrom(from: address, to: address, amount: u256) -> bool { ... }
    action approve(spender: address, amount: u256) -> bool { ... }
    view balanceOf(who: address) -> u256 { ... }
    view allowance(owner: address, spender: address) -> u256 { ... }
    view totalSupply() -> u256 { ... }
    view name() -> text { ... }
    view symbol() -> text { ... }
    view decimals() -> u8 { ... }
}"""
            },
            {
                "label": "Extending the token with custom logic",
                "lang": "covenant",
                "code": """\
token GovernanceToken {
    name:     "Governance Token";
    symbol:   "GOV";
    decimals: 18;
    supply:   100_000_000;
    owner:    msg.sender;
}

// Extend with a minting cap
record GovernanceTokenMinter {
    token:    address;
    cap:      u256 = 200_000_000 * (10 ** 18);
    minted:   u256;

    error CapExceeded(requested: u256, remaining: u256);

    action mint(to: address, amount: u256) only(self.owner) {
        let remaining = self.cap - self.minted;
        if amount > remaining {
            revert_with CapExceeded(amount, remaining);
        }
        self.minted += amount;
        // call into the token contract
        GovernanceToken(self.token).mint(to, amount);
    }
}"""
            },
        ],
        "notes": [
            ("<code>supply</code>", "is minted to the deployer (<code>msg.sender</code>) at construction. To distribute differently, extend with a custom distribution action."),
            ("<code>owner</code>", "is stored in the contract and used by <code>mint</code>/<code>burn</code> if those extensions are added."),
            ("ERC-20 compliance", "is verified at compile time, the Covenant compiler emits a lint error if any mandatory selector is missing from the generated ABI."),
        ],
        "takeaways": [
            "Use <code>token</code> for any standard fungible token, six lines replaces ~100 lines of Solidity.",
            "The generated contract is identical to a hand-written ERC-20, no abstraction overhead.",
            "Extensions (minting, burning, permit) are added by composing a separate record.",
        ],
    },
    {
        "num": 7, "slug": "fhe-basics",
        "title": "FHE Basics",
        "subtitle": "encrypted&lt;T&gt;, fhe_add, fhe_mul, computing on encrypted state.",
        "section": "standards",
        "intro": """\
Covenant's FHE type system lets you write contracts where state is never
decrypted on-chain. The <code>encrypted&lt;T&gt;</code> type annotates a field
or variable as ciphertext. Operations (<code>fhe_add</code>, <code>fhe_mul</code>)
compile to <code>STATICCALL</code> instructions targeting chain precompile addresses
: the actual FHE implementation lives in the chain's precompile layer, not in
Covenant. Covenant is fully scheme-agnostic (TFHE, BGV, CKKS, …).""",
        "blocks": [
            {
                "label": "private_counter.cov",
                "lang": "covenant",
                "code": """\
record PrivateCounter {
    count: encrypted<u64>;
    owner: address;

    // Increment without revealing the current count
    action increment() only(self.owner) {
        // fhe_add compiles to STATICCALL → precompile 0x0300
        self.count = fhe_add(self.count, encrypted(1u64));
    }

    action add(n: encrypted<u64>) only(self.owner) {
        self.count = fhe_add(self.count, n);
    }

    // Reveal the decrypted value: only owner can call
    view read_count() -> u64 only(self.owner) {
        return decrypt(self.count);
    }
}"""
            },
            {
                "label": "encrypted_voting.cov: private ballot tally",
                "lang": "covenant",
                "code": """\
record PrivateBallot {
    yes_votes: encrypted<u64>;
    no_votes:  encrypted<u64>;
    end_time:  time;
    owner:     address;
    voted:     map(address => bool);

    error AlreadyVoted();
    error VotingClosed();

    // Vote without revealing direction
    action vote(choice: encrypted<u64>)
        when(block.timestamp < self.end_time)
    {
        if self.voted[msg.sender] {
            revert_with AlreadyVoted();
        }
        self.voted[msg.sender] = true;

        // Homomorphic addition: tally stays encrypted
        self.yes_votes = fhe_add(self.yes_votes, choice);
        self.no_votes  = fhe_add(
            self.no_votes,
            fhe_sub(encrypted(1u64), choice)
        );
    }

    // Owner tallies after vote ends: decrypts locally
    view tally() -> (u64, u64)
        only(self.owner)
        when(block.timestamp >= self.end_time)
    {
        return (decrypt(self.yes_votes), decrypt(self.no_votes));
    }
}"""
            },
        ],
        "notes": [
            ("<code>encrypted&lt;T&gt;</code>", "is a first-class type. You cannot accidentally pass it to a function expecting <code>T</code>, the compiler rejects the type mismatch."),
            ("<code>fhe_add</code> / <code>fhe_mul</code>", "compile to <code>STATICCALL</code> targeting addresses <code>0x0300</code>-<code>0x0308</code>. The chain's precompile determines the underlying FHE scheme."),
            ("<code>encrypted(v)</code>", "wraps a plaintext value as a client-side ciphertext before sending to the contract."),
            ("<code>decrypt(c)</code>", "calls the decryption precompile. Only the keyholder can obtain the plaintext, on chains with threshold decryption, this requires a quorum."),
            ("Scheme agnosticism", "Covenant emits the same bytecode regardless of whether the chain uses TFHE, BGV, or CKKS. See <code>LICENSE_CLARIFICATION.md</code>."),
        ],
        "takeaways": [
            "<code>encrypted&lt;T&gt;</code> fields are stored as ciphertext on-chain, observers see only the encryption, not the value.",
            "FHE arithmetic is exact for integers (TFHE) and approximate for fixed-point (CKKS): choose based on chain support.",
            "Covenant never imports any FHE library: the chain's precompile layer handles all crypto.",
        ],
    },
    {
        "num": 8, "slug": "encrypted-token",
        "title": "Encrypted Token (ERC-8227)",
        "subtitle": "Private balances with selective disclosure.",
        "section": "standards",
        "intro": """\
ERC-8227 is Covenant's encrypted fungible token standard. Balances are stored
as ciphertexts; transfers are homomorphic additions that never expose amounts
on-chain. <code>selective_disclosure</code> lets the owner grant viewing access
to specific parties (auditors, regulators) without exposing data to everyone.""",
        "blocks": [
            {
                "label": "private_token.cov",
                "lang": "covenant",
                "code": """\
encrypted token PrivateToken {
    name:     "Private Token";
    symbol:   "PT";
    decimals: 18;
    supply:   1_000_000;
    owner:    msg.sender;
}"""
            },
            {
                "label": "private_token_auditable.cov: with selective disclosure",
                "lang": "covenant",
                "code": """\
encrypted token AuditableToken {
    name:     "Auditable Token";
    symbol:   "AT";
    decimals: 18;
    supply:   10_000_000;
    owner:    msg.sender;

    // Parties that can view any balance
    auditors: [address; 8];

    selective_disclosure {
        // Balance visible to the account holder
        reveal balance to self;
        // Balance visible to listed auditors
        reveal balance to self.auditors;
        // Total supply always public
        reveal totalSupply to public;
    }
}"""
            },
            {
                "label": "Reading a private balance",
                "lang": "covenant",
                "code": """\
// Off-chain: using covenant-sdk (TypeScript)

import { CovenantClient } from '@covenant-lang/sdk';

const client = new CovenantClient({ rpc: 'https://rpc.example.com' });
const token  = client.contract('0xABC...', 'AuditableToken');

// Decrypt caller's own balance (requires caller's decryption key)
const myBalance = await token.balanceOf(myAddress, { decrypt: true });

// Auditor reading another account (requires auditor key)
const balance = await token.balanceOf(
    targetAddress,
    { decrypt: true, as: auditorAddress }
);"""
            },
        ],
        "notes": [
            ("<code>encrypted token</code>", "is the ERC-8227 form of <code>token</code>. It generates a homomorphic transfer function in addition to standard ERC-20 selectors."),
            ("<code>selective_disclosure</code>", "is a block that configures which parties receive a decryption key fragment for which fields."),
            ("<code>reveal X to Y</code>", "grants <code>Y</code> the ability to call the chain's disclosure precompile and obtain the plaintext of <code>X</code>."),
            ("Transfers", "are handled by <code>fhe_add</code> / <code>fhe_sub</code> on ciphertexts, the chain validates balance constraints via ZK range proofs without decrypting."),
        ],
        "takeaways": [
            "<code>encrypted token</code> gives you a fully private ERC-20 in one declaration.",
            "Selective disclosure enables compliance (auditability) without full transparency.",
            "Balance integrity is enforced by the chain's ZK proof layer, no plaintext ever leaves the ciphertext domain during transfer.",
        ],
    },
    {
        "num": 9, "slug": "post-quantum-signatures",
        "title": "Post-Quantum Signatures",
        "subtitle": "pq_key, @pq_signed: Dilithium-5 authentication.",
        "section": "standards",
        "intro": """\
Classical ECDSA signatures are vulnerable to sufficiently large quantum computers.
Covenant adds <code>pq_key&lt;Scheme&gt;</code> as a first-class storage type and
<code>@pq_signed</code> as a decorator that verifies post-quantum signatures
at function entry. The verification compiles to a STATICCALL into the chain's
PQ precompile (<code>0x0400</code>-<code>0x0401</code>).""",
        "blocks": [
            {
                "label": "pq_vault.cov",
                "lang": "covenant",
                "code": """\
record PQVault {
    // Store a Dilithium-5 public key instead of an address
    pq_owner:  pq_key<Dilithium5>;
    balance:   amount;
    nonce:     u64;

    error BadSignature();

    // @pq_signed verifies the caller's Dilithium-5 signature over
    // (contract_address ++ nonce ++ calldata) before the body runs
    @pq_signed(self.pq_owner)
    action withdraw(to: address, value: amount) {
        if value > self.balance {
            revert_with InsufficientFunds(value, self.balance);
        }
        self.balance -= value;
        self.nonce   += 1;
        transfer(to, value);
    }

    // Standard ECDSA deposit: anyone can fund
    action deposit() {
        self.balance += msg.value;
    }

    error InsufficientFunds(requested: amount, available: amount);
}"""
            },
            {
                "label": "hybrid_auth.cov: ECDSA + PQ dual key",
                "lang": "covenant",
                "code": """\
record HybridWallet {
    // Both keys must sign for high-value ops
    classical_owner: address;
    pq_owner:        pq_key<Dilithium5>;
    threshold:       amount;

    // Small withdrawals: classical ECDSA only
    action withdraw_small(to: address, value: amount)
        only(self.classical_owner)
        when(value <= self.threshold)
    {
        transfer(to, value);
    }

    // Large withdrawals: require PQ signature too
    @pq_signed(self.pq_owner)
    action withdraw_large(to: address, value: amount)
        only(self.classical_owner)
        when(value > self.threshold)
    {
        transfer(to, value);
    }
}"""
            },
        ],
        "notes": [
            ("<code>pq_key&lt;Dilithium5&gt;</code>", "stores a Dilithium-5 public key (NIST FIPS 204 Level 5). The key is stored as <code>bytes</code> in EVM storage."),
            ("<code>@pq_signed(key)</code>", "prepends a STATICCALL to precompile <code>0x0400</code> that verifies the signature over the canonical message <code>keccak256(address ++ nonce ++ selector ++ calldata)</code>."),
            ("Supported schemes", "<code>Dilithium3</code> (NIST Level 3), <code>Dilithium5</code> (NIST Level 5), <code>Falcon512</code>, <code>Falcon1024</code>. Chain support varies."),
            ("Nonce", "is automatically incremented by the PQ precompile to prevent replay attacks, you don't manage it manually."),
        ],
        "takeaways": [
            "PQ signatures are a zero-cost abstraction: the compiler emits a precompile call, not a Rust/Solidity verifier.",
            "Hybrid wallets (classical + PQ) are the recommended migration path, quantum resistance without breaking existing tooling.",
            "Covenant's PQ support is chain-dependent; check your target chain's precompile registry.",
        ],
    },
    {
        "num": 10, "slug": "zero-knowledge-proofs",
        "title": "Zero-Knowledge Proofs",
        "subtitle": "selective_disclosure, verified_by, proving properties without revealing data.",
        "section": "standards",
        "intro": """\
ZK proofs let a contract verify a claim: "the user is over 18", "the balance
exceeds 1000 USDC", "the merkle path is valid", without the contract ever
seeing the underlying data. Covenant's <code>zk_proof</code> type and
<code>verified_by</code> clause compile to STATICCALL into the chain's ZK
precompile (<code>0x0500</code>-<code>0x0501</code>).""",
        "blocks": [
            {
                "label": "age_gate.cov: prove age without revealing it",
                "lang": "covenant",
                "code": """\
interface IZKVerifier {
    // Returns true if proof is valid for the given public inputs
    view verify(proof: zk_proof, inputs: [field; 4]) -> bool;
}

record AgeGate {
    verifier:  address;    // chain-deployed ZK verifier precompile
    min_age:   u32  = 18;
    members:   map(address => bool);

    error ProofInvalid();

    // User proves age >= 18 without revealing birth date
    // Public inputs: [min_age, current_year, 0, 0]
    action join(proof: zk_proof) {
        let ok = IZKVerifier(self.verifier).verify(
            proof,
            [self.min_age as field, block.timestamp / 31_536_000 as field, 0, 0]
        );
        if !ok {
            revert_with ProofInvalid();
        }
        self.members[msg.sender] = true;
    }

    view is_member(who: address) -> bool {
        return self.members[who];
    }
}"""
            },
            {
                "label": "merkle_drop.cov: airdrop via Merkle proof",
                "lang": "covenant",
                "code": """\
record MerkleDrop {
    root:     hash;
    token:    address;
    claimed:  map(address => bool);

    error AlreadyClaimed();
    error InvalidProof();

    // Claim tokens by proving inclusion in the Merkle tree
    action claim(amount: u256, proof: [hash; 32]) {
        if self.claimed[msg.sender] {
            revert_with AlreadyClaimed();
        }

        // Reconstruct leaf = keccak256(address ++ amount)
        let leaf = keccak256(msg.sender, amount);

        // Verify Merkle path
        if !merkle_verify(leaf, proof, self.root) {
            revert_with InvalidProof();
        }

        self.claimed[msg.sender] = true;
        IERC20(self.token).transfer(msg.sender, amount);
    }
}"""
            },
        ],
        "notes": [
            ("<code>zk_proof</code>", "is an opaque byte-array type. Its validity is verified by the ZK precompile, not Covenant itself."),
            ("<code>field</code>", "is a finite-field element (BN254 scalar by default). ZK circuits express inputs as field elements."),
            ("<code>merkle_verify</code>", "is a Covenant built-in that compiles to the ZK precompile's Merkle path verification entry-point."),
            ("Nova IVC vs Halo2", "Nova is preferred for recursive proofs (incrementally verifiable computation); Halo2 for single-shot range/inclusion proofs. Chain precompile support varies."),
        ],
        "takeaways": [
            "ZK proofs let you verify properties of private data without exposing the data itself.",
            "Covenant's <code>zk_proof</code> type is chain-agnostic, the proof system is selected by the precompile, not the language.",
            "Merkle airdrops, age gates, and range proofs are the most common ZK patterns in production.",
        ],
    },
    # ── Advanced ──────────────────────────────────────────────────────────────
    {
        "num": 11, "slug": "cryptographic-amnesia",
        "title": "Cryptographic Amnesia (ERC-8228)",
        "subtitle": "amnesia&lt;T&gt;, destroy(), vdf_proof, verifiable data deletion.",
        "section": "advanced",
        "intro": """\
ERC-8228 introduces <em>cryptographic amnesia</em>: a contract can provably
commit to destroying secret data after a delay enforced by a Verifiable Delay
Function (VDF). Once the destruction ceremony completes, no party, including
the chain operator: can recover the data. This enables use cases like
time-limited secret auctions, ephemeral keys, and GDPR-compliant on-chain storage.""",
        "blocks": [
            {
                "label": "amnesia_ceremony.cov: 4-phase destruction protocol",
                "lang": "covenant",
                "code": """\
record SecretAuction {
    // amnesia<T> wraps T in a destruction-trackable envelope
    sealed_bid:  amnesia<u256>;
    bidder:      address;
    phase:       u8;       // 0=init 1=committed 2=locked 3=destroyed
    reveal_at:   time;
    destroy_at:  time;

    error WrongPhase(expected: u8, got: u8);
    error TooEarly(ready_at: time, now: time);

    // Phase 0 → 1: commit the bid (sealed)
    action commit(bid: u256)
        when(self.phase == 0)
        only(self.bidder)
    {
        self.sealed_bid = amnesia(bid);
        self.phase = 1;
    }

    // Phase 1 → 2: lock: no more reads after this
    action lock()
        when(self.phase == 1)
        when(block.timestamp >= self.reveal_at)
        only(self.bidder)
    {
        self.phase = 2;
    }

    // Phase 2 → 3: destroy with VDF proof (prevents last-minute reads)
    // vdf_proof(t) requires t sequential squarings, ~1 million ≈ 30 seconds
    action destroy()
        when(self.phase == 2)
        when(block.timestamp >= self.destroy_at)
        only(self.bidder)
    {
        vdf_proof(t = 1_000_000);   // enforced delay before destruction
        destroy(self.sealed_bid);   // zeroes and tombstones the slot
        self.phase = 3;
    }

    // Readable only in phase 1 (after commit, before lock)
    view read_bid() -> u256
        only(self.bidder)
        when(self.phase == 1)
    {
        return self.sealed_bid;
    }
}"""
            },
        ],
        "notes": [
            ("<code>amnesia&lt;T&gt;</code>", "wraps <code>T</code> in a destruction-trackable envelope. The compiler tracks all read paths and enforces phase guards."),
            ("<code>destroy(field)</code>", "zeroes the storage slot and writes a tombstone hash that proves destruction occurred at a specific block."),
            ("<code>vdf_proof(t = N)</code>", "compiles to a STATICCALL to the VDF precompile (<code>0x0502</code>). The sequentiality of the VDF prevents parallel pre-computation of the proof, you cannot rush destruction."),
            ("ERC-8228 compliance", "requires: (a) amnesia wrapping, (b) locked phase before destruction, (c) VDF proof in the destruction action, (d) a tombstone event emitted on-chain."),
        ],
        "takeaways": [
            "Cryptographic amnesia provides a <em>verifiable</em> guarantee of deletion, not just a promise.",
            "The VDF delay prevents an adversary from watching mempool and reading the data right before the destroy transaction mines.",
            "After <code>destroy()</code>, the amnesia slot is tombstoned, any subsequent read attempt reverts.",
        ],
    },
    {
        "num": 12, "slug": "uups-upgradeable",
        "title": "UUPS Upgradeable",
        "subtitle": "upgradeable_by, version, migrate, safe contract upgrades.",
        "section": "advanced",
        "intro": """\
The Universal Upgradeable Proxy Standard (EIP-1822) separates logic from storage.
Covenant's <code>upgradeable_by</code> and <code>version</code> keywords generate
a compliant UUPS implementation. The <code>migrate from N to M</code> block
handles storage layout migrations with a re-initialization guard so migration
logic can only run once.""",
        "blocks": [
            {
                "label": "counter_v1.cov",
                "lang": "covenant",
                "code": """\
record CounterV1 {
    value:   u256;
    owner:   address;
    version: 1;    // compile-time version constant

    upgradeable_by: self.owner;  // who can authorize an upgrade

    action set(v: u256) only(self.owner) {
        self.value = v;
    }

    action increment() only(self.owner) {
        self.value += 1;
    }

    // Migration from version 0 (unversioned) to version 1
    // Runs once at upgrade time, never again
    migrate from 0 to 1 {
        // Initialize new fields added in V1
        // (owner was always present; nothing new here)
    }
}"""
            },
            {
                "label": "counter_v2.cov: adding a step field",
                "lang": "covenant",
                "code": """\
record CounterV2 {
    value:   u256;
    owner:   address;
    step:    u256;    // new in V2
    version: 2;

    upgradeable_by: self.owner;

    action set(v: u256) only(self.owner) {
        self.value = v;
    }

    action set_step(s: u256) only(self.owner) {
        self.step = s;
    }

    action increment() only(self.owner) {
        self.value += self.step;
    }

    // Storage migration: initialize step when upgrading from V1
    migrate from 1 to 2 {
        self.step = 1;   // sensible default
    }

    // Guard against re-initialization: compiler enforces this runs once
}"""
            },
            {
                "label": "Deploying and upgrading",
                "lang": "bash",
                "code": """\
# Deploy V1 through a UUPS proxy
covenant deploy counter_v1.cov --proxy uups --network sepolia

# After logic change, deploy V2 and upgrade the proxy
covenant deploy counter_v2.cov --network sepolia
covenant upgrade <proxy-address> <v2-address> --network sepolia"""
            },
        ],
        "notes": [
            ("<code>version: N</code>", "is a compile-time constant embedded in the bytecode. The upgrade mechanism verifies version monotonicity."),
            ("<code>upgradeable_by: expr</code>", "generates an <code>_authorizeUpgrade</code> override that requires <code>expr</code> to be true."),
            ("<code>migrate from N to M { ... }</code>", "generates an initializer guarded by a storage flag, it cannot be called twice (re-initialization guard)."),
            ("Storage layout", "The compiler validates that V2 does not change the slot positions of V1 fields, only appending new fields at the end is allowed."),
        ],
        "takeaways": [
            "<code>upgradeable_by</code> and <code>version</code> give you UUPS in two declarations, no boilerplate proxy code to maintain.",
            "The <code>migrate</code> block is the safest place for post-upgrade initialization, the compiler ensures it runs exactly once.",
            "Always audit storage layout compatibility between versions. The compiler warns on slot conflicts.",
        ],
    },
    {
        "num": 13, "slug": "beacon-proxy",
        "title": "Beacon Proxy Pattern",
        "subtitle": "@proxy_compatible, deploy_proxy, upgrading many instances atomically.",
        "section": "advanced",
        "intro": """\
The beacon proxy pattern (EIP-1967) lets one upgrade transaction update the
logic for hundreds of proxy instances simultaneously. Covenant's
<code>@proxy_compatible</code> decorator ensures the implementation contract
has no constructor-side effects (safe for delegate calls), and
<code>deploy_proxy</code> deploys a new proxy pointing at the beacon.""",
        "blocks": [
            {
                "label": "implementation.cov: proxy-safe logic",
                "lang": "covenant",
                "code": """\
// @proxy_compatible disables the constructor and adds
// an initializer pattern instead
@proxy_compatible
record Vault {
    owner:   address;
    balance: amount;
    initialized: bool;

    error AlreadyInitialized();

    // Called once by the factory after proxy deployment
    action initialize(owner: address) when(!self.initialized) {
        self.owner       = owner;
        self.initialized = true;
    }

    action deposit() {
        self.balance += msg.value;
    }

    action withdraw(to: address, value: amount) only(self.owner) {
        self.balance -= value;
        transfer(to, value);
    }
}"""
            },
            {
                "label": "beacon.cov: single upgrade point",
                "lang": "covenant",
                "code": """\
record VaultBeacon {
    implementation: address;
    owner:          address;

    event Upgraded(new_impl: address);

    action upgrade(new_impl: address) only(self.owner) {
        self.implementation = new_impl;
        emit Upgraded(new_impl);
    }

    view get_implementation() -> address {
        return self.implementation;
    }
}"""
            },
            {
                "label": "factory.cov: deploy many proxies",
                "lang": "covenant",
                "code": """\
record VaultFactory {
    beacon:  address;
    vaults:  map(address => address);   // user => vault proxy
    count:   u64;

    event VaultCreated(user: address, vault: address);

    action create_vault() {
        // deploy_proxy creates a minimal proxy pointing at beacon
        let vault_addr = deploy_proxy(self.beacon);

        // Initialize the vault for the caller
        Vault(vault_addr).initialize(msg.sender);

        self.vaults[msg.sender] = vault_addr;
        self.count += 1;
        emit VaultCreated(msg.sender, vault_addr);
    }

    view vault_of(user: address) -> address {
        return self.vaults[user];
    }
}"""
            },
        ],
        "notes": [
            ("<code>@proxy_compatible</code>", "adds a compile-time check that the implementation has no immutable constructor arguments and uses an initializer pattern."),
            ("<code>deploy_proxy(beacon)</code>", "deploys a minimal EIP-1967 proxy whose implementation slot points at the beacon's <code>get_implementation()</code> result."),
            ("Atomic upgrade", "Calling <code>VaultBeacon.upgrade(newImpl)</code> immediately affects all proxies, they delegate to the new implementation on the next call."),
            ("Storage slots", "All proxies share the same storage layout as the implementation. Never change field order in an implementation upgrade."),
        ],
        "takeaways": [
            "Beacon proxy is ideal for protocols that deploy many identical contract instances (lending pools, vaults, subgraphs).",
            "One upgrade transaction affects all instances, a huge operational advantage over per-proxy upgrades.",
            "Use <code>@proxy_compatible</code> to catch constructor-safety issues at compile time.",
        ],
    },
    {
        "num": 14, "slug": "oracle-integration",
        "title": "Oracle Integration",
        "subtitle": "Chainlink price feeds: staleness checks and typed errors.",
        "section": "advanced",
        "intro": """\
Price oracles are a primary attack surface in DeFi, stale prices, flash-loan
manipulation, and round-skipping have caused hundreds of millions in losses.
This chapter shows how to consume Chainlink's <code>latestRoundData</code>
correctly: checking freshness, validating sequencer uptime (for L2s), and
handling the full tuple return.""",
        "blocks": [
            {
                "label": "price_feed.cov",
                "lang": "covenant",
                "code": """\
interface IChainlinkAggregator {
    view latestRoundData() -> (
        roundId:         u80,
        answer:          i256,
        startedAt:       u256,
        updatedAt:       u256,
        answeredInRound: u80
    );
    view decimals() -> u8;
}

record PriceConsumer {
    feed:        address;
    max_staleness: duration = 3600s;   // 1 hour

    error StalePrice(age: duration, max_staleness: duration);
    error InvalidPrice(answer: i256);
    error RoundIncomplete(round_id: u80, answered_in: u80);

    view eth_usd_price() -> i256 {
        let agg = IChainlinkAggregator(self.feed);
        let (round_id, answer, _, updated_at, answered_in) =
            agg.latestRoundData();

        // Staleness check
        let age = block.timestamp - updated_at;
        if age > self.max_staleness {
            revert_with StalePrice(age, self.max_staleness);
        }

        // Sanity checks
        if answer <= 0 {
            revert_with InvalidPrice(answer);
        }
        if answered_in < round_id {
            revert_with RoundIncomplete(round_id, answered_in);
        }

        return answer;
    }
}"""
            },
            {
                "label": "lending.cov: oracle-gated borrow",
                "lang": "covenant",
                "code": """\
record SimpleLend {
    price_consumer: address;
    collateral:     map(address => amount);
    borrowed:       map(address => u256);
    ltv:            u8 = 75;   // 75% loan-to-value ratio

    error Undercollateralized(collateral_usd: u256, borrow_usd: u256);

    action borrow(usdc_amount: u256) {
        // Get current ETH price (8 decimals from Chainlink)
        let eth_price = PriceConsumer(self.price_consumer).eth_usd_price();

        let collat_eth = self.collateral[msg.sender];
        let collat_usd = collat_eth * eth_price as u256 / 1e8;
        let max_borrow = collat_usd * self.ltv / 100;

        let already_borrowed = self.borrowed[msg.sender];
        if already_borrowed + usdc_amount > max_borrow {
            revert_with Undercollateralized(collat_usd, already_borrowed + usdc_amount);
        }

        self.borrowed[msg.sender] += usdc_amount;
        // ... transfer USDC to borrower
    }
}"""
            },
        ],
        "notes": [
            ("Staleness check", "Always validate <code>block.timestamp - updatedAt &lt;= maxStaleness</code>. A stale price is an open attack vector."),
            ("Round completeness", "<code>answeredInRound &gt;= roundId</code> confirms the round closed normally. An incomplete round may signal a paused feed."),
            ("Negative price", "Chainlink <code>answer</code> is <code>int256</code>, a zero or negative value indicates a feed error. Always reject <code>answer &lt;= 0</code>."),
            ("L2 sequencer", "On L2s (Arbitrum, Optimism), also check the sequencer uptime feed before trusting prices."),
        ],
        "takeaways": [
            "Three checks are mandatory: staleness, positive price, round completeness.",
            "Use typed errors (<code>StalePrice</code>, <code>InvalidPrice</code>) so callers can programmatically handle each failure mode.",
            "On L2s, add a sequencer uptime check: a down sequencer can expose stale L1 prices.",
        ],
    },
    {
        "num": 15, "slug": "deploy-to-sepolia",
        "title": "Deploy to Sepolia",
        "subtitle": "Install, compile, deploy, verify, end-to-end in four commands.",
        "section": "advanced",
        "intro": """\
This chapter walks through a complete deployment of a Covenant contract to the
Sepolia Ethereum testnet using <code>covenant-cli</code>. You will need: a funded
Sepolia wallet, a Sepolia RPC endpoint (Alchemy or Infura), and an Etherscan
API key for verification.""",
        "blocks": [
            {
                "label": "Step 1: install covenant-cli",
                "lang": "bash",
                "code": """\
# Requires Rust 1.78+ (rustup.rs)
cargo install covenant-cli

# Verify
covenant --version
# covenant-cli 0.7.0"""
            },
            {
                "label": "counter.cov: contract to deploy",
                "lang": "covenant",
                "code": """\
record Counter {
    count: u64;
    owner: address;

    action increment() only(self.owner) {
        self.count += 1;
    }

    action reset() only(self.owner) {
        self.count = 0;
    }

    view get() -> u64 {
        return self.count;
    }
}"""
            },
            {
                "label": "Step 2: compile to EVM bytecode",
                "lang": "bash",
                "code": """\
covenant build counter.cov --target-chain evm --out build/

# Output:
#   build/Counter.bin: deployment bytecode
#   build/Counter.abi.json: ABI
#   build/Counter.metadata.json"""
            },
            {
                "label": "Step 3: set environment variables",
                "lang": "bash",
                "code": """\
export SEPOLIA_RPC="https://eth-sepolia.g.alchemy.com/v2/<YOUR_KEY>"
export PRIVATE_KEY="0x..."          # funded Sepolia wallet
export ETHERSCAN_KEY="<YOUR_KEY>"   # from etherscan.io/myapikey"""
            },
            {
                "label": "Step 4: deploy",
                "lang": "bash",
                "code": """\
covenant deploy build/Counter.bin \\
    --abi build/Counter.abi.json \\
    --network sepolia \\
    --rpc $SEPOLIA_RPC \\
    --key $PRIVATE_KEY

# Output:
#   Deploying Counter to sepolia...
#   Transaction: 0xabc...
#   Confirmed in block 5834201
#   Contract address: 0x1234...56789"""
            },
            {
                "label": "Step 5: verify on Etherscan",
                "lang": "bash",
                "code": """\
covenant verify 0x1234...56789 \\
    --source counter.cov \\
    --network sepolia \\
    --etherscan-key $ETHERSCAN_KEY

# Output:
#   Submitting source to Etherscan...
#   Verification successful.
#   https://sepolia.etherscan.io/address/0x1234...56789#code"""
            },
            {
                "label": "Step 6: interact via CLI",
                "lang": "bash",
                "code": """\
# Call a view
covenant call 0x1234...56789 get \\
    --network sepolia --rpc $SEPOLIA_RPC
# Returns: 0

# Send a transaction
covenant send 0x1234...56789 increment \\
    --network sepolia --rpc $SEPOLIA_RPC --key $PRIVATE_KEY
# Transaction mined: 0xdef...

covenant call 0x1234...56789 get \\
    --network sepolia --rpc $SEPOLIA_RPC
# Returns: 1"""
            },
        ],
        "notes": [
            ("<code>covenant build</code>", "compiles <code>.cov</code> source to EVM deployment bytecode and generates the ABI JSON."),
            ("<code>covenant deploy</code>", "sends the deployment transaction and waits for confirmation. Outputs the contract address."),
            ("<code>covenant verify</code>", "submits the source and compiler metadata to Etherscan's verification API."),
            ("Gas", "Covenant bytecode is typically 10-30% smaller than equivalent Solidity due to IR-level optimizations. This translates to lower deployment gas."),
        ],
        "takeaways": [
            "Four commands: <code>cargo install covenant-cli</code>, <code>covenant build</code>, <code>covenant deploy</code>, <code>covenant verify</code>.",
            "The CLI handles RPC, signing, and block confirmation automatically.",
            "Covenant metadata is embedded in the artifact, Etherscan verification requires no extra steps.",
        ],
    },
]

# ─────────────────────────────────────────────
# HTML GENERATION HELPERS
# ─────────────────────────────────────────────

FONTS = '<link rel="preconnect" href="https://fonts.googleapis.com"><link rel="preconnect" href="https://fonts.gstatic.com" crossorigin><link href="https://fonts.googleapis.com/css2?family=EB+Garamond:ital,wght@0,400;0,500;0,600;1,400;1,500&family=Inter:wght@300;400;500;600&family=JetBrains+Mono:wght@400;500;600&display=swap" rel="stylesheet">'

CSS_TOKENS = """
:root {
  --ink:          #1a1a1a;
  --ink-soft:     #3d3d3d;
  --ink-mute:     #6b7280;
  --ink-faint:    #9ca3af;
  --rule:         #d1d5db;
  --rule-faint:   #e5e7eb;
  --paper:        #fcfbf8;
  --paper-pure:   #ffffff;
  --accent:       #7C3AED;
  --accent-faint: #faf7ff;
  --accent-soft:  #a78bfa;
  --cyan:         #0891b2;
  --blue:         #2563eb;
  --green:        #059669;
  --red:          #dc2626;
  --orange:       #d97706;
  --header-h:     56px;
}
"""

CSS_RESET = """
*, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }
html { scroll-behavior: smooth; font-size: 16px; }
body {
  background: var(--paper);
  color: var(--ink);
  font-family: 'EB Garamond', Georgia, serif;
  -webkit-font-smoothing: antialiased;
  overflow-x: hidden;
}
"""

CSS_HEADER = """
#site-header {
  position: fixed; top: 0; left: 0; right: 0;
  height: var(--header-h);
  background: rgba(252,251,248,0.92);
  backdrop-filter: blur(8px);
  -webkit-backdrop-filter: blur(8px);
  border-bottom: 1px solid var(--rule-faint);
  display: flex; align-items: center;
  padding: 0 48px;
  z-index: 1000;
  transition: border-color 0.3s ease;
}
#site-header.scrolled { border-bottom-color: var(--rule); }
.header-brand {
  display: flex; align-items: center; gap: 10px;
  text-decoration: none; color: var(--ink); flex-shrink: 0;
}
.header-brand-name {
  font-family: 'JetBrains Mono', monospace;
  font-size: 10pt; font-weight: 600;
  letter-spacing: 0.12em; text-transform: uppercase;
}
.header-badge {
  font-family: 'JetBrains Mono', monospace;
  font-size: 7pt; font-weight: 500;
  letter-spacing: 0.1em; text-transform: uppercase;
  color: var(--accent); border: 1px solid var(--accent);
  padding: 1px 6px; margin-left: 4px;
}
.header-spacer { flex: 1; }
nav.header-nav { display: flex; align-items: center; gap: 32px; }
.nav-link {
  font-family: 'Inter', sans-serif;
  font-size: 9.5pt; font-weight: 400;
  color: var(--ink-mute); text-decoration: none;
  letter-spacing: 0.01em; transition: color 0.15s ease;
}
.nav-link:hover, .nav-link.active { color: var(--ink); }
.nav-link-cta {
  font-family: 'JetBrains Mono', monospace;
  font-size: 8.5pt; font-weight: 500;
  letter-spacing: 0.08em; text-transform: uppercase;
  color: var(--accent); text-decoration: none;
  border: 1px solid var(--accent);
  padding: 5px 12px;
  transition: background 0.15s ease, color 0.15s ease;
}
.nav-link-cta:hover { background: var(--accent); color: white; }
"""

CSS_LAYOUT = """
.page-wrap {
  display: flex;
  padding-top: var(--header-h);
  min-height: 100vh;
}
.sidebar {
  width: 240px;
  flex-shrink: 0;
  position: sticky;
  top: var(--header-h);
  height: calc(100vh - var(--header-h));
  overflow-y: auto;
  border-right: 1px solid var(--rule-faint);
  padding: 32px 0 48px;
  background: var(--paper);
}
.sidebar-section-label {
  font-family: 'JetBrains Mono', monospace;
  font-size: 6.5pt; font-weight: 600;
  letter-spacing: 0.18em; text-transform: uppercase;
  color: var(--ink-faint);
  padding: 0 20px;
  margin: 20px 0 6px;
}
.sidebar-section-label:first-child { margin-top: 0; }
.sidebar-link {
  display: flex; align-items: center; gap: 10px;
  padding: 6px 20px;
  text-decoration: none;
  font-family: 'Inter', sans-serif;
  font-size: 9pt;
  color: var(--ink-mute);
  transition: color 0.12s, background 0.12s;
  border-left: 2px solid transparent;
}
.sidebar-link:hover { color: var(--ink); background: var(--accent-faint); }
.sidebar-link.active {
  color: var(--accent);
  border-left-color: var(--accent);
  background: var(--accent-faint);
  font-weight: 500;
}
.sidebar-num {
  font-family: 'JetBrains Mono', monospace;
  font-size: 7.5pt; color: var(--ink-faint);
  flex-shrink: 0; width: 18px; text-align: right;
}
.sidebar-link.active .sidebar-num { color: var(--accent-soft); }
.main {
  flex: 1;
  min-width: 0;
  max-width: 820px;
  padding: 56px 64px 96px;
}
@media (max-width: 900px) {
  .sidebar { display: none; }
  .main { padding: 40px 24px 80px; max-width: 100%; }
}
"""

CSS_CONTENT = """
.chapter-eyebrow {
  font-family: 'JetBrains Mono', monospace;
  font-size: 7.5pt; font-weight: 600;
  letter-spacing: 0.16em; text-transform: uppercase;
  color: var(--ink-faint);
  margin-bottom: 12px;
}
.chapter-title {
  font-family: 'EB Garamond', serif;
  font-size: clamp(24pt, 4vw, 38pt);
  font-weight: 400; line-height: 1.08;
  letter-spacing: -0.02em;
  margin-bottom: 12px;
}
.chapter-subtitle {
  font-family: 'EB Garamond', serif;
  font-size: 14pt; font-style: italic;
  color: var(--ink-mute); line-height: 1.5;
  margin-bottom: 36px;
}
.chapter-divider {
  border: none; border-top: 1px solid var(--ink);
  margin-bottom: 40px;
}
.prose {
  font-family: 'EB Garamond', serif;
  font-size: 13pt; color: var(--ink-soft);
  line-height: 1.7; max-width: 680px;
  margin-bottom: 32px;
}
.prose code {
  font-family: 'JetBrains Mono', monospace;
  font-size: 10pt; background: var(--rule-faint);
  padding: 1px 5px; border-radius: 2px; color: var(--ink);
}
.prose + .prose { margin-top: -16px; }
/* Code block */
.code-wrap {
  margin: 28px 0;
  border: 1px solid var(--rule);
  background: #1a1a1a;
  border-radius: 0;
}
.code-label {
  font-family: 'JetBrains Mono', monospace;
  font-size: 7.5pt; font-weight: 500;
  color: #9ca3af;
  padding: 8px 16px;
  border-bottom: 1px solid #2d2d2d;
  background: #111;
  letter-spacing: 0.06em;
}
.code-block {
  margin: 0; padding: 20px 24px;
  font-family: 'JetBrains Mono', monospace;
  font-size: 10pt; line-height: 1.65;
  color: #e5e7eb; overflow-x: auto;
  tab-size: 4;
}
.code-block .kw   { color: #a78bfa; }  /* keywords, purple */
.code-block .tp   { color: #67e8f9; }  /* types, cyan */
.code-block .str  { color: #86efac; }  /* strings, green */
.code-block .num  { color: #fca5a5; }  /* numbers, red */
.code-block .cmt  { color: #6b7280; font-style: italic; } /* comments */
.code-block .dec  { color: #fbbf24; }  /* decorators, amber */
.code-block .fn   { color: #93c5fd; }  /* builtins, blue */
/* Notes table */
.notes-table {
  width: 100%; border-collapse: collapse;
  margin: 32px 0;
}
.notes-table tr { border-top: 1px solid var(--rule-faint); }
.notes-table tr:first-child { border-top: 1px solid var(--rule); }
.notes-table td {
  padding: 10px 12px;
  font-family: 'EB Garamond', serif;
  font-size: 12pt; vertical-align: top;
  color: var(--ink-soft); line-height: 1.55;
}
.notes-table td:first-child {
  width: 200px; white-space: nowrap;
  font-family: 'JetBrains Mono', monospace;
  font-size: 9.5pt; color: var(--ink);
  padding-right: 20px;
}
.notes-table td code {
  font-family: 'JetBrains Mono', monospace;
  font-size: 9pt; background: var(--rule-faint);
  padding: 1px 4px; border-radius: 2px; color: var(--ink);
}
/* Takeaways */
.takeaways-heading {
  font-family: 'JetBrains Mono', monospace;
  font-size: 8pt; font-weight: 600;
  letter-spacing: 0.12em; text-transform: uppercase;
  color: var(--ink-faint); margin: 40px 0 12px;
}
.takeaways {
  list-style: none;
  padding: 0; margin: 0;
}
.takeaways li {
  font-family: 'EB Garamond', serif;
  font-size: 13pt; color: var(--ink-soft);
  line-height: 1.55; padding: 6px 0;
  padding-left: 20px; position: relative;
}
.takeaways li::before {
  content: '→';
  position: absolute; left: 0;
  color: var(--accent); font-style: normal;
}
.takeaways li code {
  font-family: 'JetBrains Mono', monospace;
  font-size: 10pt; background: var(--rule-faint);
  padding: 1px 5px; border-radius: 2px; color: var(--ink);
}
/* Section label */
.section-badge {
  display: inline-flex; align-items: center;
  font-family: 'JetBrains Mono', monospace;
  font-size: 7pt; font-weight: 600;
  letter-spacing: 0.12em; text-transform: uppercase;
  padding: 2px 8px; margin-bottom: 12px;
}
.section-badge.fundamentals { color: var(--accent); border: 1px solid var(--accent); }
.section-badge.standards    { color: var(--cyan);   border: 1px solid var(--cyan); }
.section-badge.advanced     { color: var(--green);  border: 1px solid var(--green); }
/* Navigation footer */
.chapter-nav {
  display: flex; gap: 16px; margin-top: 64px;
  padding-top: 32px; border-top: 1px solid var(--rule-faint);
}
.chapter-nav-btn {
  flex: 1; display: flex; flex-direction: column; gap: 4px;
  padding: 16px 20px; border: 1px solid var(--rule);
  text-decoration: none; color: var(--ink);
  transition: border-color 0.15s, background 0.15s;
}
.chapter-nav-btn:hover { border-color: var(--accent); background: var(--accent-faint); }
.chapter-nav-btn.next { text-align: right; align-items: flex-end; }
.chapter-nav-direction {
  font-family: 'JetBrains Mono', monospace;
  font-size: 7pt; font-weight: 600;
  letter-spacing: 0.14em; text-transform: uppercase;
  color: var(--ink-faint);
}
.chapter-nav-title {
  font-family: 'EB Garamond', serif;
  font-size: 13pt; color: var(--ink);
}
"""

CSS_INDEX = """
.index-hero {
  padding-top: calc(var(--header-h) + 72px);
  padding-bottom: 64px;
  border-bottom: 1px solid var(--ink);
  background-image: radial-gradient(circle, rgba(26,26,26,0.05) 1px, transparent 1px);
  background-size: 28px 28px;
  background-color: var(--paper);
}
.index-hero-inner {
  max-width: 960px; margin: 0 auto; padding: 0 48px;
}
.index-hero-eyebrow {
  font-family: 'JetBrains Mono', monospace;
  font-size: 8pt; font-weight: 600;
  letter-spacing: 0.16em; text-transform: uppercase;
  color: var(--accent); margin-bottom: 16px;
}
.index-hero-title {
  font-family: 'EB Garamond', serif;
  font-size: clamp(32pt, 6vw, 54pt);
  font-weight: 400; line-height: 1.04;
  letter-spacing: -0.02em; margin-bottom: 16px;
}
.index-hero-sub {
  font-family: 'EB Garamond', serif;
  font-size: 15pt; font-style: italic;
  color: var(--ink-mute); line-height: 1.5;
  max-width: 560px; margin-bottom: 36px;
}
.index-hero-meta {
  display: flex; gap: 32px; flex-wrap: wrap; align-items: center;
}
.index-hero-stat {
  font-family: 'Inter', sans-serif;
  font-size: 9pt; color: var(--ink-mute);
}
.index-hero-stat strong {
  font-family: 'JetBrains Mono', monospace;
  font-size: 11pt; color: var(--ink); display: block;
  font-weight: 600;
}
.index-body {
  max-width: 960px; margin: 0 auto; padding: 0 48px 96px;
}
.section-group { margin-top: 64px; }
.section-group-header {
  display: flex; align-items: baseline; gap: 16px;
  border-bottom: 1px solid var(--ink);
  padding-bottom: 12px; margin-bottom: 28px;
}
.section-group-title {
  font-family: 'EB Garamond', serif;
  font-size: 22pt; font-weight: 400;
}
.section-group-desc {
  font-family: 'Inter', sans-serif;
  font-size: 9pt; color: var(--ink-mute);
}
.chapter-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
  gap: 1px;
  background: var(--rule);
  border: 1px solid var(--rule);
}
.chapter-card {
  background: var(--paper);
  padding: 24px 20px;
  text-decoration: none; color: var(--ink);
  display: flex; flex-direction: column; gap: 6px;
  transition: background 0.12s;
}
.chapter-card:hover { background: var(--accent-faint); }
.chapter-card-num {
  font-family: 'JetBrains Mono', monospace;
  font-size: 7.5pt; color: var(--ink-faint);
  letter-spacing: 0.06em;
}
.chapter-card-title {
  font-family: 'EB Garamond', serif;
  font-size: 15pt; font-weight: 500; line-height: 1.2;
}
.chapter-card-sub {
  font-family: 'Inter', sans-serif;
  font-size: 8.5pt; color: var(--ink-mute); line-height: 1.4;
  margin-top: 2px;
}
"""

JS_HIGHLIGHTER = r"""
function hl(code, lang) {
  // Escape HTML
  code = code.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');

  if (lang === 'bash' || lang === 'shell') {
    code = code.replace(/(#[^\n]*)/g, '<span class="cmt">$1</span>');
    code = code.replace(/\b(cargo|covenant|export|git|npm)\b/g, '<span class="kw">$1</span>');
    code = code.replace(/"([^"]*?)"/g, '<span class="str">"$1"</span>');
    return code;
  }

  // Covenant
  const KEYWORDS = [
    'record','token','encrypted token','action','view','constant','interface',
    'only','when','given','emit','transfer','revert_with','return','let',
    'if','else','fhe_add','fhe_mul','fhe_sub','decrypt','reveal','encrypt',
    'encrypted','pq_key','selective_disclosure','verified_by','amnesia',
    'destroy','vdf_proof','upgradeable_by','migrate','from','to',
    'deploy_proxy','event','error','map','import','as','pub'
  ];
  const TYPES = [
    'address','amount','time','duration','hash','text','bool','field',
    'u8','u16','u32','u64','u128','u256','i256','u80','u112',
    'zk_proof','bytes','Dilithium3','Dilithium5','Falcon512','Falcon1024'
  ];
  const BUILTINS = [
    'msg\\.sender','msg\\.value','block\\.timestamp','self\\.addr\\(\\)',
    'keccak256','merkle_verify'
  ];

  // Comments first
  code = code.replace(/(\/\/[^\n]*)/g, '\x00cmt\x01$1\x00/cmt\x01');
  // Strings
  code = code.replace(/"([^"]*?)"/g, '\x00str\x01"$1"\x00/str\x01');
  // Decorators
  code = code.replace(/(@\w+)/g, '\x00dec\x01$1\x00/dec\x01');
  // Numbers (before keywords to avoid clashing with identifiers)
  code = code.replace(/\b(\d[\d_]*(?:\.\d+)?(?:e\d+)?(?:u8|u16|u32|u64|u128|u256)?)\b/g,
    '\x00num\x01$1\x00/num\x01');
  // Builtins
  for (const b of BUILTINS) {
    code = code.replace(new RegExp(b, 'g'), '\x00fn\x01$&\x00/fn\x01');
  }
  // Keywords (longest first to avoid partial matches)
  const kwSorted = [...KEYWORDS].sort((a,b) => b.length - a.length);
  for (const kw of kwSorted) {
    const safe = kw.replace(/[.*+?^${}()|[\]\\]/g,'\\$&');
    code = code.replace(new RegExp('(?<![\\w\\x00-\\x01])' + safe + '(?![\\w\\x00-\\x01])','g'),
      '\x00kw\x01' + kw + '\x00/kw\x01');
  }
  // Types
  for (const tp of TYPES) {
    code = code.replace(new RegExp('(?<![\\w\\x00-\\x01])' + tp + '(?![\\w\\x00-\\x01])','g'),
      '\x00tp\x01' + tp + '\x00/tp\x01');
  }
  // Convert placeholders to spans
  code = code
    .replace(/\x00kw\x01/g,  '<span class="kw">')
    .replace(/\x00\/kw\x01/g,'</span>')
    .replace(/\x00tp\x01/g,  '<span class="tp">')
    .replace(/\x00\/tp\x01/g,'</span>')
    .replace(/\x00str\x01/g, '<span class="str">')
    .replace(/\x00\/str\x01/g,'</span>')
    .replace(/\x00num\x01/g, '<span class="num">')
    .replace(/\x00\/num\x01/g,'</span>')
    .replace(/\x00cmt\x01/g, '<span class="cmt">')
    .replace(/\x00\/cmt\x01/g,'</span>')
    .replace(/\x00dec\x01/g, '<span class="dec">')
    .replace(/\x00\/dec\x01/g,'</span>')
    .replace(/\x00fn\x01/g,  '<span class="fn">')
    .replace(/\x00\/fn\x01/g,'</span>');
  return code;
}

document.addEventListener('DOMContentLoaded', () => {
  document.querySelectorAll('pre[data-lang]').forEach(pre => {
    const lang = pre.getAttribute('data-lang') || 'covenant';
    pre.innerHTML = hl(pre.textContent, lang);
  });

  // Header scroll effect
  const header = document.getElementById('site-header');
  window.addEventListener('scroll', () => {
    header.classList.toggle('scrolled', window.scrollY > 8);
  }, {passive: true});
});
"""

def sidebar_html(current_num):
    sections = {
        "fundamentals": "Fundamentals",
        "standards":    "Standards",
        "advanced":     "Advanced",
    }
    out = []
    last_section = None
    for ch in CHAPTERS:
        if ch["section"] != last_section:
            out.append(f'<div class="sidebar-section-label">{sections[ch["section"]]}</div>')
            last_section = ch["section"]
        active = ' active' if ch["num"] == current_num else ''
        out.append(
            f'<a class="sidebar-link{active}" href="{ch["num"]:02d}-{ch["slug"]}">'
            f'<span class="sidebar-num">{ch["num"]}</span>'
            f'{ch["title"]}</a>'
        )
    return "\n".join(out)

def code_block_html(block):
    label = block["label"]
    lang  = block.get("lang", "covenant")
    code  = block["code"].strip()
    # Escape for pre: JS highlighter will handle the rest
    escaped = code.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
    # We'll let JS do highlighting; store raw text, JS reads textContent
    raw = code  # raw: JS highlighter escapes itself
    return (
        f'<div class="code-wrap">'
        f'<div class="code-label">{label}</div>'
        f'<pre class="code-block" data-lang="{lang}">{code}</pre>'
        f'</div>'
    )

def notes_html(notes):
    rows = ""
    for term, desc in notes:
        rows += f"<tr><td>{term}</td><td>{desc}</td></tr>\n"
    return f'<table class="notes-table">\n{rows}</table>'

def takeaways_html(items):
    lis = "\n".join(f"<li>{item}</li>" for item in items)
    return (
        f'<p class="takeaways-heading">Key takeaways</p>'
        f'<ul class="takeaways">{lis}</ul>'
    )

def nav_html(ch):
    prev_ch = next((c for c in CHAPTERS if c["num"] == ch["num"] - 1), None)
    next_ch = next((c for c in CHAPTERS if c["num"] == ch["num"] + 1), None)

    prev_btn = ""
    if prev_ch:
        prev_btn = (
            f'<a class="chapter-nav-btn prev" href="{prev_ch["num"]:02d}-{prev_ch["slug"]}">'
            f'<span class="chapter-nav-direction">← Previous</span>'
            f'<span class="chapter-nav-title">{prev_ch["title"]}</span>'
            f'</a>'
        )
    else:
        prev_btn = (
            '<a class="chapter-nav-btn prev" href="index">'
            '<span class="chapter-nav-direction">← Back</span>'
            '<span class="chapter-nav-title">All chapters</span>'
            '</a>'
        )

    next_btn = ""
    if next_ch:
        next_btn = (
            f'<a class="chapter-nav-btn next" href="{next_ch["num"]:02d}-{next_ch["slug"]}">'
            f'<span class="chapter-nav-direction">Next →</span>'
            f'<span class="chapter-nav-title">{next_ch["title"]}</span>'
            f'</a>'
        )
    else:
        next_btn = (
            '<a class="chapter-nav-btn next" href="index">'
            '<span class="chapter-nav-direction">Done ✓</span>'
            '<span class="chapter-nav-title">All chapters</span>'
            '</a>'
        )

    return f'<nav class="chapter-nav">{prev_btn}{next_btn}</nav>'

def header_html(active_nav="examples"):
    return f"""
<header id="site-header">
  <a class="header-brand" href="https://covenant-lang.org">
    <svg width="22" height="22" viewBox="0 0 24 24" fill="none">
      <polygon points="12,2 22,20 2,20" stroke="#7C3AED" stroke-width="2" fill="none"/>
      <circle cx="12" cy="20" r="1.5" fill="#7C3AED"/>
    </svg>
    <span class="header-brand-name">Covenant</span>
    <span class="header-badge">V0.7 GA</span>
  </a>
  <div class="header-spacer"></div>
  <nav class="header-nav">
    <a class="nav-link" href="https://covenant-lang.org">Home</a>
    <a class="nav-link" href="https://covenant-lang.org/audit">Audit</a>
    <a class="nav-link{'  active' if active_nav == 'examples' else ''}" href="index">Examples</a>
    <a class="nav-link" href="https://github.com/Valisthea/covenant-language" target="_blank" rel="noopener">GitHub</a>
    <a class="nav-link-cta" href="https://crates.io/crates/covenant-cli" target="_blank" rel="noopener">cargo install</a>
  </nav>
</header>
"""

def base_head(title, description, canonical):
    return f"""<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>{title} · Covenant By Example</title>
<meta name="description" content="{description}">
<meta name="author" content="Kairos Lab">
<link rel="canonical" href="{canonical}">
<link rel="icon" type="image/svg+xml" href="/favicon.svg">
<link rel="alternate icon" type="image/png" sizes="32x32" href="/favicon-32.png">
<link rel="alternate icon" type="image/x-icon" href="/favicon.ico">
<link rel="apple-touch-icon" sizes="180x180" href="/apple-touch-icon.png">
<meta name="theme-color" content="#7C3AED">
<meta property="og:type" content="article">
<meta property="og:url" content="{canonical}">
<meta property="og:site_name" content="Covenant By Example">
<meta property="og:title" content="{title} · Covenant By Example">
<meta property="og:description" content="{description}">
<meta property="og:image" content="https://examples.covenant-lang.org/og-image.jpg">
<meta name="twitter:card" content="summary_large_image">
<meta name="twitter:creator" content="@Valisthea">
<meta name="twitter:title" content="{title} · Covenant By Example">
<meta name="twitter:description" content="{description}">
{FONTS}
<style>
{CSS_TOKENS}
{CSS_RESET}
{CSS_HEADER}
{CSS_LAYOUT}
{CSS_CONTENT}
</style>
</head>"""

# ─────────────────────────────────────────────
# CHAPTER PAGE
# ─────────────────────────────────────────────

def render_chapter(ch):
    slug_full = f"{ch['num']:02d}-{ch['slug']}"
    canonical = f"https://examples.covenant-lang.org/{slug_full}"
    description = ch["subtitle"]

    head = base_head(ch["title"], description, canonical)
    header = header_html()

    # Section badge
    section_label = {"fundamentals":"Fundamentals","standards":"Standards","advanced":"Advanced"}[ch["section"]]
    badge = f'<span class="section-badge {ch["section"]}">{section_label}</span>'

    # Code blocks
    blocks_html = "\n".join(code_block_html(b) for b in ch["blocks"])

    # Notes section header
    notes_section = ""
    if ch.get("notes"):
        notes_section = f"""
<p class="takeaways-heading" style="margin-top:44px">Annotations</p>
{notes_html(ch["notes"])}
"""

    # Takeaways
    ta = takeaways_html(ch["takeaways"]) if ch.get("takeaways") else ""

    # Mocked-crypto honesty banner (testnet-only) for the cryptographic chapters
    mock_banner = ""
    if ch["num"] in (7, 8, 9, 10, 11):
        mock_banner = (
            '<div style="border:1px solid #EF4444;background:rgba(239,68,68,0.08);'
            'border-radius:8px;padding:14px 16px;margin:20px 0;font-size:14px;line-height:1.55">'
            '<strong>⚠️ Testnet-only: the cryptography is MOCKED.</strong> '
            'The FHE / post-quantum / ZK / amnesia primitives shown here are deterministic '
            'stubs with <strong>zero confidentiality and zero security</strong>: "encrypted" '
            'values are readable from chain state and the verifiers accept forged proofs. '
            'This tutorial teaches the <em>language surface</em>; real cryptography is a '
            'separate, later release. See '
            '<a href="https://github.com/Valisthea/covenant-language/blob/main/STATUS.md">STATUS.md</a>.'
            '</div>'
        )

    body = f"""<!DOCTYPE html>
<html lang="en">
{head}
<body>
{header}
<div class="page-wrap">
  <aside class="sidebar">
    {sidebar_html(ch["num"])}
  </aside>
  <main class="main">
    {badge}
    <p class="chapter-eyebrow">Chapter {ch["num"]} / 15</p>
    <h1 class="chapter-title">{ch["title"]}</h1>
    <p class="chapter-subtitle">{ch["subtitle"]}</p>
    <hr class="chapter-divider">
    {mock_banner}
    <p class="prose">{ch["intro"]}</p>
    {blocks_html}
    {notes_section}
    {ta}
    {nav_html(ch)}
  </main>
</div>
<script>
{JS_HIGHLIGHTER}
</script>
</body>
</html>"""
    return body

# ─────────────────────────────────────────────
# INDEX PAGE
# ─────────────────────────────────────────────

def render_index():
    canonical = "https://examples.covenant-lang.org/"
    description = "Covenant By Example: 15 annotated chapters covering fundamentals, standards, and advanced patterns."

    head = f"""<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Covenant By Example</title>
<meta name="description" content="{description}">
<meta name="author" content="Kairos Lab">
<link rel="canonical" href="{canonical}">
<base href="/examples/">
<link rel="icon" type="image/svg+xml" href="/favicon.svg">
<link rel="alternate icon" type="image/png" sizes="32x32" href="/favicon-32.png">
<link rel="alternate icon" type="image/x-icon" href="/favicon.ico">
<link rel="apple-touch-icon" sizes="180x180" href="/apple-touch-icon.png">
<meta name="theme-color" content="#7C3AED">
<meta property="og:type" content="website">
<meta property="og:url" content="{canonical}">
<meta property="og:site_name" content="Covenant">
<meta property="og:title" content="Covenant By Example">
<meta property="og:description" content="{description}">
<meta property="og:image" content="https://examples.covenant-lang.org/og-image.jpg">
<meta name="twitter:card" content="summary_large_image">
<meta name="twitter:creator" content="@Valisthea">
<meta name="twitter:title" content="Covenant By Example">
<meta name="twitter:description" content="{description}">
{FONTS}
<style>
{CSS_TOKENS}
{CSS_RESET}
{CSS_HEADER}
{CSS_INDEX}
</style>
</head>"""

    header = header_html()

    # Build section grids
    section_defs = [
        ("fundamentals", "Fundamentals", "Chapters 1-5", "Records, fields, actions, guards, and external calls, the language core."),
        ("standards",    "Standards",    "Chapters 6-10", "ERC-20, FHE, encrypted tokens, post-quantum signatures, and ZK proofs."),
        ("advanced",     "Advanced",     "Chapters 11-15", "Cryptographic amnesia, UUPS upgrades, beacon proxies, oracles, and deployment."),
    ]

    sections_html = ""
    for sec_id, sec_title, sec_chapters, sec_desc in section_defs:
        cards = ""
        for ch in CHAPTERS:
            if ch["section"] != sec_id:
                continue
            cards += (
                f'<a class="chapter-card" href="{ch["num"]:02d}-{ch["slug"]}">'
                f'<span class="chapter-card-num">Chapter {ch["num"]}</span>'
                f'<span class="chapter-card-title">{ch["title"]}</span>'
                f'<span class="chapter-card-sub">{ch["subtitle"]}</span>'
                f'</a>'
            )
        sections_html += f"""
<section class="section-group">
  <div class="section-group-header">
    <h2 class="section-group-title">{sec_title}</h2>
    <span class="section-group-desc">{sec_chapters} &mdash; {sec_desc}</span>
  </div>
  <div class="chapter-grid">
    {cards}
  </div>
</section>"""

    body = f"""<!DOCTYPE html>
<html lang="en">
{head}
<body>
{header}

<div class="index-hero">
  <div class="index-hero-inner">
    <p class="index-hero-eyebrow">Covenant By Example</p>
    <h1 class="index-hero-title">Learn Covenant<br>through annotated code.</h1>
    <p class="index-hero-sub">
      15 chapters. From hello contract to encrypted tokens, post-quantum
      signatures, and full deployment: with real code at every step.
    </p>
    <div class="index-hero-meta">
      <div class="index-hero-stat"><strong>15</strong>chapters</div>
      <div class="index-hero-stat"><strong>3</strong>sections</div>
      <div class="index-hero-stat"><strong>Apache-2.0</strong>open source</div>
      <div class="index-hero-stat"><strong>V0.7 GA</strong>compiler</div>
    </div>
  </div>
</div>

<div class="index-body">
  {sections_html}
</div>

<script>
const header = document.getElementById('site-header');
window.addEventListener('scroll', () => {{
  header.classList.toggle('scrolled', window.scrollY > 8);
}}, {{passive: true}});
</script>
</body>
</html>"""
    return body

# ─────────────────────────────────────────────
# WRITE ALL FILES
# ─────────────────────────────────────────────

if __name__ == "__main__":
    # Index
    idx_path = os.path.join(OUT, "index.html")
    with open(idx_path, "w", encoding="utf-8") as f:
        f.write(render_index())
    print(f"Written: {idx_path}")

    # Chapters
    for ch in CHAPTERS:
        filename = f"{ch['num']:02d}-{ch['slug']}.html"
        path = os.path.join(OUT, filename)
        with open(path, "w", encoding="utf-8") as f:
            f.write(render_chapter(ch))
        print(f"Written: {path}")

    print(f"\nDone: {1 + len(CHAPTERS)} HTML files in {OUT}")

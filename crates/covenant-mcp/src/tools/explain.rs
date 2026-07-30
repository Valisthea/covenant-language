use rmcp::model::{CallToolResult, Tool};
use serde_json::{json, Map, Value};

use crate::server::{error_result, schema, text_result};

pub fn definition() -> Tool {
    Tool {
        name: "explain".into(),
        description: "Return a detailed explanation of a Covenant construct, guard, privacy \
             qualifier, type alias, or ERC standard. Topic examples: 'vault', 'ceremony', 'when', \
             'only owner', 'encrypted', 'amount', 'ERC-8227', 'pq_signed', 'confidential token'."
            .into(),
        input_schema: schema(json!({
            "type": "object",
            "required": ["topic"],
            "properties": {
                "topic": {
                    "type": "string",
                    "description": "Construct name, guard keyword, type alias, or ERC number to explain"
                }
            }
        })),
    }
}

pub fn run(params: &Map<String, Value>) -> CallToolResult {
    let topic = match params.get("topic").and_then(Value::as_str) {
        Some(s) => s.trim().to_ascii_lowercase(),
        None => return error_result("missing required parameter: topic"),
    };

    match lookup(&topic) {
        Some(explanation) => text_result(json!({
            "topic":       topic,
            "explanation": explanation,
        })),
        None => text_result(json!({
            "topic":       topic,
            "explanation": format!(
                "No built-in explanation for '{topic}'. \
                 Try: record, token, confidential token, ballot, counter, encrypted counter, \
                 board, market, vault, registry, bridge, ceremony, module, hybrid module, \
                 when, only, given, pq_signed, verified_by, vdf_locked, \
                 amount, text, hash, time, caller, now, \
                 ERC-8227, ERC-8228, ERC-8229, ERC-8231."
            ),
        })),
    }
}

fn lookup(topic: &str) -> Option<&'static str> {
    Some(match topic {
        // ── Constructs ───────────────────────────────────────────────────────
        "record" => {
            "record C { }: key-value storage construct. Auto-synthesizes per-field getters. \
             Fields are declared bare (no `field` keyword). Best for simple state bags where \
             every piece of state needs a public getter. Example:\n\
             \n\
             record Config {\n    owner:   address\n    enabled: bool\n}"
        }
        "token" => {
            "token C { }: standard fungible token. Auto-synthesizes the full ERC-20 surface: \
             transfer, approve, transfer_from, balance_of, allowance, Transfer event, Approval event. \
             A metadata block is required:\n\
             \n\
             token Coin {\n    symbol:   \"COIN\"\n    name:     \"Covenant Coin\"\n    decimals: 18\n    supply:   1_000_000 to deployer\n}"
        }
        "confidential token" | "confidentialtoken" => {
            "confidential token C { }: FHE-encrypted token implementing ERC-8227 (Styx Protocol). \
             Balances are TFHE ciphertexts; auto-synthesizes transferEncrypted, balanceOfEncrypted, \
             approveEncrypted. Must include an ERC-8227 citation comment:\n\
             \n\
             -- ERC-8227: Confidential Token Interface (Styx Protocol)\n\
             confidential token SecretCoin {\n    symbol: \"SCOIN\"\n    name:   \"Secret Coin\"\n    decimals: 18\n    supply: 1_000_000 to deployer\n}"
        }
        "ballot" => {
            "ballot C { }: on-chain voting construct. Auto-synthesizes tally management and voting \
             actions. Supports public, private, and sealed variants via privacy qualifiers. Fields use \
             the `field` keyword. To prevent double voting, use a `when` guard over a seen-map, \
             for example `action vote(c: amount) when voted[caller] == 0`. Do NOT use \
             `only first_time_caller`: it parses and resolves, but the backend refuses it with \
             E518 because it has no real EVM authorization check and would pass for every caller."
        }
        "counter" => {
            "counter C { }: single-value counter construct. Auto-synthesizes increment and decrement \
             actions. Use for simple counting that needs no additional logic. For encrypted counting, \
             use `encrypted counter`."
        }
        "encrypted counter" | "encryptedcounter" => {
            "encrypted counter C { }: TFHE-encrypted counter (Veil layer). All arithmetic is \
             homomorphic; plaintext values are never revealed on-chain. Use `reveal X to owner` \
             to declare a decryption access policy. Example:\n\
             \n\
             encrypted counter ShieldedCounter {\n    total: amount\n    action bump(by: amount) { total += by }\n    reveal total to owner\n}"
        }
        "board" => {
            "board C { post { } }: append-only message board. The nested `post` block defines the \
             record schema for each post. Posts accumulate in a `posts` array; use `append post { }` \
             to add entries. Auto-synthesizes `posts.length` and indexed access."
        }
        "market" => {
            "market C { } declares an order-book construct with \
             `priority_queue<K, V, max|min>` fields for bid and ask queues. The queue is \
             read-only: it exposes `.top_key`, `.top_value` and `.length`, and there is no \
             enqueue or dequeue operation, so a market cannot yet accept orders and no \
             release is promised for them."
        }
        "vault" => {
            "vault C { }: value custody construct. It adds NO reentrancy protection at this \
             release: the same body written as `module` compiles to byte-identical bytecode, so \
             order your state writes before the transfer yourself. `@non_reentrant` is not an \
             annotation the compiler knows, and writing it is E110 rather than a warning. Send \
             ETH with `transfer(value) to addr`, the `to` outside the parens:\n\
             \n\
             vault Treasury {\n    field balances: map<address, amount>\n    action deposit() { balances[caller] += 1 }\n    action withdraw(value: amount) when balances[caller] >= value {\n        balances[caller] -= value\n        transfer(value) to caller\n    }\n}"
        }
        "registry" => {
            "registry C { }: identity and key registration construct (Fortress layer). It does \
             NOT build at this release. The ERC-8231 synthesizer injects `register` and `key_of` \
             over `pq_key`, whose ABI type is dynamic `bytes`, and the backend refuses that with \
             E505. Even `registry R { field x: amount }` fails the same way, so no registry body \
             compiles yet. Scaffolding one gives the intended shape, not a working contract."
        }
        "bridge" => {
            "bridge C anchored_on [\"chain_a\", \"chain_b\"] { }: cross-chain escrow construct. \
             The `anchored_on` clause lists the chain names this bridge connects. Auto-synthesizes \
             cross-chain escrow primitives."
        }
        "ceremony" => {
            "ceremony C { guardians: N threshold: M }: cryptographic amnesia construct implementing \
             ERC-8228 (Styx Protocol / Oblivion layer). Auto-synthesizes the full amnesia lifecycle: \
             setup(), submit_share(), finalize(), destroy(), phase(), is_destroyed(), session_id(), \
             owner(). The on_destroy block runs when the ceremony enters Destroyed phase. \
             Must include an ERC-8228 citation comment."
        }
        "module" => {
            "module C { }: generic escape hatch. Auto-synthesizes nothing. Use only when no \
             specialized keyword fits. Fields require the `field` keyword."
        }
        "hybrid module" | "hybridmodule" => {
            "hybrid module C { }: mixed plaintext + FHE-encrypted state (Veil layer). Each field \
             can be qualified: bare = plaintext, `encrypted` = TFHE ciphertext, `pq_signed` = \
             post-quantum signed. Fields require the `field` keyword:\n\
             \n\
             hybrid module Wallet {\n    field headcount:          amount   -- plaintext\n    field encrypted treasury: amount   -- TFHE ciphertext\n    field pq_signed history:  [hash]   -- PQ-signed list\n}"
        }

        // ── Guards ───────────────────────────────────────────────────────────
        "when" => {
            "`when expr` guard: the action reverts before the body runs if `expr` is false. \
             Replaces `require(cond, msg)` from Solidity. Multiple guards are comma-separated:\n\
             \n\
             action withdraw(value: amount)\n        when balances[caller] >= value,\n        when value > 0 { ... }"
        }
        "only" | "only owner" | "only deployer" => {
            "`only <principal>` guard: restricts the caller to a specific principal. \
             Two principals compile at this release: `owner` (deployer-set address) and \
             `deployer` (deployment address). `first_time_caller` and `registered_key` parse and \
             resolve, but the backend refuses them with E518 because neither has a real EVM \
             authorization check and both would pass for every caller. `registered_account` is \
             not a predicate the compiler knows at all: it is E106. \
             Replaces Solidity `modifier onlyOwner() { _; }`."
        }
        "given" => {
            "`given x in collection` guard: checks that `x` is a member of `collection` \
             (array or set). Example: `given pick in valid_options`."
        }
        "pq_signed" => {
            "`pq_signed(content, sig, key)` guard: verifies a Dilithium-5 post-quantum signature \
             (ERC-8231, Fortress layer). The action reverts if the signature is invalid. \
             Must include an ERC-8231 citation comment near the action."
        }
        "verified_by" => {
            "`verified_by(zk_proof)` guard: verifies a Halo2 SNARK / Nova IVC recursive proof \
             (ERC-8229, Prism layer). The action reverts if the proof is invalid. \
             Must include an ERC-8229 citation comment near the action."
        }
        "vdf_locked" => {
            "`vdf_locked(handle, time)` guard: Wesolowski VDF time-lock check (Oblivion layer). \
             The action reverts until the VDF for `handle` has been verified at `time`."
        }

        // ── Type aliases ─────────────────────────────────────────────────────
        "amount" => {
            "`amount`: the primary numeric type in Covenant. Equivalent to Solidity `uint256`. \
             All arithmetic operators (+, -, *, /) work as expected. \
             Do NOT use `uint256`: the compiler rejects it."
        }
        "text" => {
            "`text`: UTF-8 string type. Equivalent to Solidity `string`. \
             Do NOT use `string`: the compiler rejects it."
        }
        "hash" => {
            "`hash`: a bytes32 semantic alias used for content hashes, Merkle roots, etc. \
             Distinct from `bytes` (variable-length)."
        }
        "time" => {
            "`time`: Unix timestamp in seconds, typed distinctly from `amount`. \
             `now` has type `time`. Arithmetic with durations: `now + 7 days` (type: time). \
             Do NOT write `now + 604800`: the compiler rejects the type mismatch. \
             Duration literals: `seconds`, `minutes`, `hours`, `days`, `weeks`."
        }
        "caller" => {
            "`caller`: the address that invoked the current action. \
             Equivalent to Solidity `msg.sender`. Do NOT use `msg.sender`."
        }
        "now" => {
            "`now`: current block timestamp, typed as `time` (not `amount`). \
             Use `now + 7 days` for time arithmetic. \
             Equivalent to Solidity `block.timestamp`."
        }
        "deployer" => {
            "`deployer`: the address that deployed the contract. Available in any action/view. \
             No Solidity built-in equivalent (usually stored manually as `owner`)."
        }
        "zero_address" => {
            "`zero_address`: the zero address (all-zeros). \
             Equivalent to Solidity `address(0)`."
        }

        // ── ERC standards ─────────────────────────────────────────────────────
        "erc-8227" | "erc8227" => {
            "ERC-8227: Confidential Token Interface (Styx Protocol / Veil layer). \
             Triggered by `confidential token C { }`. Auto-synthesizes: \
             transferEncrypted, balanceOfEncrypted, approveEncrypted. \
             All balance storage is TFHE ciphertext. \
             Requires a `-- ERC-8227:` citation comment on the construct."
        }
        "erc-8228" | "erc8228" => {
            "ERC-8228: Cryptographic Amnesia Ceremony (Styx Protocol / Oblivion layer). \
             Triggered by `ceremony C { guardians: N threshold: M }`. Auto-synthesizes: \
             setup(), submit_share(), finalize(), destroy(), phase() [0=Setup 1=Active 2=Finalized 3=Destroyed], \
             is_destroyed(), session_id(), owner(). \
             The `on_destroy` block executes when the ceremony is destroyed. \
             Requires a `-- ERC-8228:` citation comment on the construct."
        }
        "erc-8229" | "erc8229" => {
            "ERC-8229: FHE Computation Verification (Styx Protocol / Prism layer). \
             Triggered by a `verified_by(zk_proof)` guard on an action. \
             Inserts a Halo2 SNARK + Nova IVC proof verification at action entry. \
             Requires a `-- ERC-8229:` citation comment near the guarded action."
        }
        "erc-8231" | "erc8231" => {
            "ERC-8231: Post-Quantum Signature Verification (Styx Protocol / Fortress layer). \
             Triggered by a `pq_signed(content, sig, key)` guard on an action. \
             Inserts a Dilithium-5 signature check at action entry. \
             Requires a `-- ERC-8231:` citation comment near the guarded action."
        }

        // ── Privacy qualifiers ────────────────────────────────────────────────
        "encrypted" => {
            "`encrypted` privacy qualifier: when applied to a field in `hybrid module`, marks it \
             as a TFHE ciphertext. All arithmetic on encrypted fields is homomorphic. \
             Use `encrypted_when ... otherwise` for conditional logic on encrypted values."
        }
        "sealed" => {
            "`sealed` privacy qualifier: applied to `ballot` or `board` constructs to create \
             sealed-bid / sealed-vote variants. Votes/posts are encrypted until revealed."
        }

        _ => return None,
    })
}

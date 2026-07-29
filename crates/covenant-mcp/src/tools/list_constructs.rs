use rmcp::model::{CallToolResult, Tool};
use serde_json::json;

use crate::server::{schema, text_result};

pub fn definition() -> Tool {
    Tool {
        name: "list_constructs".into(),
        description: "List all 14 Covenant top-level constructs with their auto-synthesized \
             surfaces and recommended use-cases. No parameters required."
            .into(),
        input_schema: schema(json!({
            "type": "object",
            "properties": {}
        })),
    }
}

pub fn run() -> CallToolResult {
    text_result(json!({
        "constructs": [
            {
                "keyword":       "record",
                "auto_synth":    "per-field auto-getters",
                "use_when":      "Key-value storage, simple state bag"
            },
            {
                "keyword":       "token",
                "auto_synth":    "Full ERC-20 surface (transfer, approve, balanceOf, Transfer, Approval)",
                "use_when":      "Standard fungible token"
            },
            {
                "keyword":       "confidential token",
                "erc":           "ERC-8227",
                "auto_synth":    "transferEncrypted, balanceOfEncrypted, approveEncrypted",
                "use_when":      "FHE-encrypted token balances (Veil layer)"
            },
            {
                "keyword":       "ballot",
                "auto_synth":    "Tally management, voting actions",
                "use_when":      "On-chain voting / polls"
            },
            {
                "keyword":       "counter",
                "auto_synth":    "increment / decrement actions",
                "use_when":      "Single-value counter"
            },
            {
                "keyword":       "encrypted counter",
                "auto_synth":    "TFHE counter operations (homomorphic += / -=)",
                "use_when":      "Privacy-preserving counter (Veil layer)"
            },
            {
                "keyword":       "board",
                "auto_synth":    "Append-only post storage (posts array, append)",
                "use_when":      "Message board / append-only log"
            },
            {
                "keyword":       "market",
                "auto_synth":    "Order-book / matching primitives",
                "use_when":      "Marketplace, DEX order book"
            },
            {
                "keyword":       "vault",
                "auto_synth":    "Reentrancy-safe value custody (@non_reentrant by default)",
                "use_when":      "Funds vault, escrow"
            },
            {
                "keyword":       "registry",
                "auto_synth":    "Identity / key registration with PQ randomness",
                "use_when":      "Identity registry, key directory (Fortress layer)"
            },
            {
                "keyword":       "bridge",
                "auto_synth":    "Cross-chain escrow primitives",
                "use_when":      "Multi-chain bridge (anchored_on [\"chain_a\", \"chain_b\"])"
            },
            {
                "keyword":       "ceremony",
                "erc":           "ERC-8228",
                "auto_synth":    "setup, submit_share, finalize, destroy, phase, is_destroyed",
                "use_when":      "Cryptographic amnesia / secret-sharing ceremonies (Oblivion layer)"
            },
            {
                "keyword":       "module",
                "auto_synth":    "Nothing — generic escape hatch",
                "use_when":      "Generic logic when no specialized keyword fits"
            },
            {
                "keyword":       "hybrid module",
                "auto_synth":    "Nothing, but allows per-field privacy qualifiers",
                "use_when":      "Mixed plaintext + encrypted state (Veil layer)"
            }
        ],
        "privacy_qualifiers": ["public", "private", "encrypted", "sealed", "confidential", "hybrid"],
        "guards": ["when", "only", "given", "pq_signed", "verified_by", "vdf_locked"],
        "type_aliases": {
            "amount":       "uint256",
            "text":         "string",
            "hash":         "bytes32 (semantic alias)",
            "time":         "block.timestamp (typed)",
            "caller":       "msg.sender",
            "now":          "block.timestamp",
            "current_block": "block.number",
            "zero_address": "address(0)",
            "deployer":     "(no Solidity equivalent)"
        }
    }))
}

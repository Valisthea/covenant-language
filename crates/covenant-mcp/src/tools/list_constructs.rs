use rmcp::model::{CallToolResult, Tool};
use serde_json::{json, Value};

use crate::server::{schema, text_result};

/// The verified capability registry, embedded at build time.
///
/// This file is the single source of truth for what each construct emits, and
/// `covenant-driver/tests/capability_registry.rs` asserts it against the
/// compiler's actual behaviour. Deriving the tool output from it means an
/// agent calling `list_constructs` cannot be told something the compiler
/// disagrees with.
///
/// It replaces a hand-written table that had drifted badly: it promised
/// `record` "per-field auto-getters" and `counter` "increment / decrement
/// actions" when neither synthesizes anything, credited `ballot` with voting
/// actions, `board` with append, `market` with an order book and `vault` with
/// value custody when all four synthesize nothing, and omitted `nft`, which is
/// one of the few constructs that does emit a full standard surface.
const REGISTRY: &str = include_str!("../../../../config/capabilities.json");

/// Editorial guidance, which is the one thing the registry does not carry.
/// A construct missing from this table still appears, without a use-case.
fn use_when(keyword: &str) -> Option<&'static str> {
    Some(match keyword {
        "record" => "Key-value storage, simple state bag",
        "token" => "Standard fungible token",
        "nft" => "Standard non-fungible token",
        "counter" => "Single-value counter",
        "module" => "Generic logic when no specialized keyword fits",
        "board" => "Message board / append-only log",
        "market" => "Marketplace, DEX order book",
        "vault" => "Funds vault, escrow",
        "ballot" => "On-chain voting / polls",
        "bridge" => "Multi-chain bridge (anchored_on [\"chain_a\", \"chain_b\"])",
        "confidential token" => "FHE-encrypted token balances (Veil layer)",
        "encrypted counter" => "Privacy-preserving counter (Veil layer)",
        "hybrid module" => "Mixed plaintext + encrypted state (Veil layer)",
        "ceremony" => "Cryptographic amnesia / secret-sharing ceremonies (Oblivion layer)",
        "registry" => "Identity registry, key directory (Fortress layer)",
        _ => return None,
    })
}

fn erc(keyword: &str) -> Option<&'static str> {
    Some(match keyword {
        "confidential token" => "ERC-8227",
        "ceremony" => "ERC-8228",
        "registry" => "ERC-8231",
        "token" => "ERC-20",
        "nft" => "ERC-721",
        _ => return None,
    })
}

pub fn definition() -> Tool {
    Tool {
        name: "list_constructs".into(),
        description: "List every Covenant top-level construct with what it actually emits, \
             its capability state (PORTABLE, INCOMPLETE, MOCKED_TESTNET_ONLY, NO_SYNTHESIS, \
             REFUSED) and a recommended use-case. Derived from the verified capability \
             registry. No parameters required."
            .into(),
        input_schema: schema(json!({
            "type": "object",
            "properties": {}
        })),
    }
}

pub fn run() -> CallToolResult {
    let registry: Value = match serde_json::from_str(REGISTRY) {
        Ok(v) => v,
        // Unreachable in a built binary: the file is embedded and a test parses
        // it. Refuse rather than fall back to a hand-written table that could
        // disagree with the compiler.
        Err(e) => {
            return text_result(json!({
                "error": format!("capability registry is not valid JSON: {e}")
            }))
        }
    };

    let mut constructs = Vec::new();
    if let Some(map) = registry.get("constructs").and_then(Value::as_object) {
        for (keyword, entry) in map {
            let mut item = json!({
                "keyword":  keyword,
                "state":    entry.get("state").and_then(Value::as_str).unwrap_or("UNKNOWN"),
                "emits":    entry.get("note").and_then(Value::as_str).unwrap_or(""),
            });
            if let Some(u) = use_when(keyword) {
                item["use_when"] = json!(u);
            }
            if let Some(e) = erc(keyword) {
                item["erc"] = json!(e);
            }
            constructs.push(item);
        }
    }

    text_result(json!({
        "constructs": constructs,
        "states": registry.get("states").cloned().unwrap_or(Value::Null),
        "compiler_version": registry.get("compiler_version").cloned().unwrap_or(Value::Null),
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

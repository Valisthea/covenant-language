//! Registry consistency test (Sprint 31, V0.9 Phase A.1).
//!
//! Asserts that the hardcoded V0.9.0 helper addresses in
//! `crates/covenant-evm-backend/src/target.rs` match the addresses
//! published in `config/helper-addresses-v0.9.0.json`.
//!
//! The Rust constants are the source of truth at compile time; the JSON
//! is the source of truth for *external* consumers (playground, third-party
//! tooling, audit firms). This test catches silent drift between the two.
//!
//! If this test fails, ONE of these is true:
//!   1. You updated `config/helper-addresses-v0.9.0.json` without updating
//!      `target.rs` (or vice-versa) → fix both in lockstep.
//!   2. You bumped V0.9 to a new minor (e.g. V0.9.1) but didn't update the
//!      version field → bump consistently.
//!   3. The JSON file format changed → update this test's parser.

use covenant_evm_backend::{CEREMONY_HELPER_V090, FHE_HELPER_V090, PQ_HELPER_V090, ZK_HELPER_V090};

const REGISTRY_JSON: &str = include_str!("../../../config/helper-addresses-v0.9.0.json");

fn addr_to_lower_hex(a: &[u8; 20]) -> String {
    a.iter().map(|b| format!("{b:02x}")).collect::<String>()
}

/// Look for a hex-encoded address as a value associated with the given key
/// in the registry. The check is permissive about whitespace between the
/// key and the value (the JSON uses aligned-column formatting with multiple
/// spaces). Plain string scan — no JSON parser dep.
fn json_contains_address(key: &str, addr: &[u8; 20]) -> bool {
    let registry_lc = REGISTRY_JSON.to_lowercase();
    let key_marker = format!(r#""{key}""#);
    let addr_hex = addr_to_lower_hex(addr);
    // For each occurrence of the key, check the next ~80 chars contain the addr hex.
    let mut search_from = 0;
    while let Some(pos) = registry_lc[search_from..].find(&key_marker) {
        let abs = search_from + pos;
        let window_end = (abs + key_marker.len() + 80).min(registry_lc.len());
        if registry_lc[abs..window_end].contains(&addr_hex) {
            return true;
        }
        search_from = abs + key_marker.len();
    }
    false
}

#[test]
fn ceremony_helper_address_matches_json() {
    assert!(
        json_contains_address("ceremony_helper", &CEREMONY_HELPER_V090),
        "Rust constant CEREMONY_HELPER_V090 ({}) does not match \
         config/helper-addresses-v0.9.0.json — fix one or both in lockstep",
        addr_to_lower_hex(&CEREMONY_HELPER_V090)
    );
}

#[test]
fn fhe_helper_address_matches_json() {
    assert!(
        json_contains_address("fhe_helper", &FHE_HELPER_V090),
        "Rust constant FHE_HELPER_V090 ({}) does not match \
         config/helper-addresses-v0.9.0.json — fix one or both in lockstep",
        addr_to_lower_hex(&FHE_HELPER_V090)
    );
}

#[test]
fn pq_helper_address_matches_json() {
    assert!(
        json_contains_address("pq_helper", &PQ_HELPER_V090),
        "Rust constant PQ_HELPER_V090 ({}) does not match \
         config/helper-addresses-v0.9.0.json — fix one or both in lockstep",
        addr_to_lower_hex(&PQ_HELPER_V090)
    );
}

#[test]
fn zk_helper_address_matches_json() {
    assert!(
        json_contains_address("zk_helper", &ZK_HELPER_V090),
        "Rust constant ZK_HELPER_V090 ({}) does not match \
         config/helper-addresses-v0.9.0.json — fix one or both in lockstep",
        addr_to_lower_hex(&ZK_HELPER_V090)
    );
}

#[test]
fn registry_version_matches_v090() {
    // The registry must claim to be V0.9.0 — if someone bumps the JSON to
    // 0.9.1 without updating Rust constants, this test fires.
    assert!(
        REGISTRY_JSON.contains(r#""version": "0.9.0""#),
        "config/helper-addresses-v0.9.0.json version field is not \"0.9.0\" — \
         either bump the Rust constants and rename this test, or revert the JSON"
    );
}

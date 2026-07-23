//! Wasm-bindgen exports for the playground's MockChain.
//!
//! ## Concurrency model
//!
//! `wasm-bindgen` runs JS-callable Rust on a single thread (main thread
//! or a worker — the playground stays on main for now). We use a
//! `thread_local` `RefCell<Chain>` to back the global chain instance.
//! Because every JS call into WASM is fully synchronous from the
//! browser's perspective, the `RefCell` borrow can never overlap with
//! itself, so there's no contention to manage.
//!
//! ## Argument shape
//!
//! Every `chain_*` function that takes "complex args" (deploy, call)
//! accepts a single JSON string. This pattern beats individual
//! positional `&str` args for two reasons:
//!
//! 1. The playground builds the args via `JSON.stringify`, which is
//!    safer and faster than wasm-bindgen marshalling 4-5 separate
//!    string params.
//! 2. Future fields (eg. `gas_limit`, `access_list`) can be added
//!    without changing the bindings ABI — just extend the JSON shape.
//!
//! Returns are JS objects produced via `serde_wasm_bindgen::to_value`
//! over the strongly-typed `chain::TxReceipt` / `Account` / etc. — no
//! second JSON.parse round-trip on the JS side.
//!
//! All JS-callable code is gated to `cfg(target_arch = "wasm32")` (in
//! `lib.rs` via `mod wasm`), so the helpers below cost nothing on the
//! native test build.

use std::cell::RefCell;

use covenant_evm_runtime::{Account, Address, Chain, Contract, TxReceipt, U256};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

thread_local! {
    /// Process-wide chain singleton. Initialized lazily on first access.
    /// `with_prefunded_accounts` so the playground's first `Deploy`
    /// click "just works" — no `chain_init` ceremony needed.
    static CHAIN: RefCell<Chain> = RefCell::new(Chain::with_prefunded_accounts());
}

// ─── JSON arg shapes ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct DeployArgs {
    deployer: String,
    bytecode_hex: String,
    #[serde(default)]
    constructor_args_hex: Option<String>,
}

#[derive(Deserialize)]
struct CallArgs {
    from: String,
    to: String,
    calldata_hex: String,
    #[serde(default)]
    value_wei_hex: Option<String>,
}

/// Slim view of the chain emitted by `chain_get_state` — the playground's
/// status bar consumes this on every refresh.
#[derive(Serialize)]
struct ChainStateView {
    block_number: u64,
    timestamp: u64,
    contracts_count: usize,
    accounts_count: usize,
    tx_count: usize,
    chain_id: u64,
}

/// JS-friendly contract view: omits the heavy `runtime_bytecode`/`storage`
/// blobs that the playground rarely needs in a list view.
#[derive(Serialize)]
struct ContractView {
    address: String,
    deployer: String,
    deployed_at_block: u64,
    deployed_at_timestamp: u64,
    label: Option<String>,
    code_hash: String,
    runtime_bytecode_size: usize,
}

impl From<&Contract> for ContractView {
    fn from(c: &Contract) -> Self {
        ContractView {
            address: format!("0x{}", c.address.to_hex()),
            deployer: format!("0x{}", c.deployer.to_hex()),
            deployed_at_block: c.deployed_at_block,
            deployed_at_timestamp: c.deployed_at_timestamp,
            label: c.label.clone(),
            code_hash: format!("0x{}", hex::encode(c.code_hash)),
            runtime_bytecode_size: c.runtime_bytecode.len(),
        }
    }
}

// ─── Public exports ───────────────────────────────────────────────────

/// Reset the chain to genesis: 5 prefunded accounts, no deployments,
/// block 1, clock at `DEFAULT_GENESIS`.
#[wasm_bindgen]
pub fn chain_init() {
    CHAIN.with(|c| {
        *c.borrow_mut() = Chain::with_prefunded_accounts();
    });
}

/// Alias for `chain_init`. The playground's UI uses "Reset", the
/// underlying op is identical — having both names lets the JS side
/// pick whichever reads better at the call site.
#[wasm_bindgen]
pub fn chain_reset() {
    chain_init();
}

/// Deploy a contract. Returns a `TxReceipt` JSON object.
#[wasm_bindgen]
pub fn chain_deploy(args_json: &str) -> JsValue {
    let args: DeployArgs = match serde_json::from_str(args_json) {
        Ok(a) => a,
        Err(e) => return error_value(format!("invalid deploy args: {e}")),
    };
    if let Err(msg) = validate_deploy_args(&args) {
        return error_value(msg);
    }

    let deployer = match Address::from_hex(&args.deployer) {
        Ok(a) => a,
        Err(e) => return error_value(e),
    };
    let bytecode = match parse_hex(&args.bytecode_hex) {
        Ok(b) => b,
        Err(e) => return error_value(e),
    };
    let constructor_args = match args
        .constructor_args_hex
        .as_deref()
        .map(parse_hex)
        .transpose()
    {
        Ok(Some(b)) => b,
        Ok(None) => Vec::new(),
        Err(e) => return error_value(e),
    };

    let receipt = CHAIN.with(|c| {
        c.borrow_mut()
            .deploy(deployer, &bytecode, &constructor_args)
    });
    to_js(&receipt)
}

/// State-mutating call.
#[wasm_bindgen]
pub fn chain_call(args_json: &str) -> JsValue {
    let args: CallArgs = match serde_json::from_str(args_json) {
        Ok(a) => a,
        Err(e) => return error_value(format!("invalid call args: {e}")),
    };
    if let Err(msg) = validate_call_args(&args) {
        return error_value(msg);
    }

    let from = match Address::from_hex(&args.from) {
        Ok(a) => a,
        Err(e) => return error_value(e),
    };
    let to = match Address::from_hex(&args.to) {
        Ok(a) => a,
        Err(e) => return error_value(e),
    };
    let calldata = match parse_hex(&args.calldata_hex) {
        Ok(b) => b,
        Err(e) => return error_value(e),
    };
    let value = match args
        .value_wei_hex
        .as_deref()
        .map(parse_u256_hex)
        .transpose()
    {
        Ok(Some(v)) => v,
        Ok(None) => U256::ZERO,
        Err(e) => return error_value(e),
    };

    let receipt: TxReceipt = CHAIN.with(|c| c.borrow_mut().call(from, to, &calldata, value));
    to_js(&receipt)
}

/// Read-only call. Storage changes are dropped, no receipt is appended
/// to the chain's tx_log. The returned `TxReceipt` carries the raw
/// return data hex for the playground's view-action UI to decode.
#[wasm_bindgen]
pub fn chain_static_call(args_json: &str) -> JsValue {
    let args: CallArgs = match serde_json::from_str(args_json) {
        Ok(a) => a,
        Err(e) => return error_value(format!("invalid static_call args: {e}")),
    };
    if let Err(msg) = validate_call_args(&args) {
        return error_value(msg);
    }
    let from = match Address::from_hex(&args.from) {
        Ok(a) => a,
        Err(e) => return error_value(e),
    };
    let to = match Address::from_hex(&args.to) {
        Ok(a) => a,
        Err(e) => return error_value(e),
    };
    let calldata = match parse_hex(&args.calldata_hex) {
        Ok(b) => b,
        Err(e) => return error_value(e),
    };

    let receipt: TxReceipt = CHAIN.with(|c| c.borrow().static_call(from, to, &calldata));
    to_js(&receipt)
}

/// Move the chain clock forward by `seconds`.
#[wasm_bindgen]
pub fn chain_advance_time(seconds: u64) {
    CHAIN.with(|c| c.borrow_mut().advance_time(seconds));
}

/// Mine `count` blocks. Each block also bumps the clock by 12s.
#[wasm_bindgen]
pub fn chain_mine_blocks(count: u64) {
    CHAIN.with(|c| c.borrow_mut().mine_blocks(count));
}

/// Block number, timestamp, contract count, account count, tx count.
/// Sized for the playground's status bar — refresh on every UI tick.
#[wasm_bindgen]
pub fn chain_get_state() -> JsValue {
    CHAIN.with(|c| {
        let chain = c.borrow();
        let view = ChainStateView {
            block_number: chain.block_number,
            timestamp: chain.clock.timestamp(),
            contracts_count: chain.contracts.len(),
            accounts_count: chain.accounts.len(),
            tx_count: chain.tx_log.len(),
            chain_id: covenant_evm_runtime::CHAIN_ID,
        };
        to_js(&view)
    })
}

#[wasm_bindgen]
pub fn chain_get_accounts() -> JsValue {
    CHAIN.with(|c| {
        let accounts: Vec<Account> = c.borrow().accounts.values().cloned().collect();
        to_js(&accounts)
    })
}

#[wasm_bindgen]
pub fn chain_get_contracts() -> JsValue {
    CHAIN.with(|c| {
        let views: Vec<ContractView> = c
            .borrow()
            .contracts
            .values()
            .map(ContractView::from)
            .collect();
        to_js(&views)
    })
}

#[wasm_bindgen]
pub fn chain_get_tx_log() -> JsValue {
    CHAIN.with(|c| to_js(&c.borrow().tx_log))
}

/// Read a storage slot directly. Useful for the Inspector's "Storage
/// inspector" sub-pane.
#[wasm_bindgen]
pub fn chain_get_storage(address_hex: &str, slot_hex: &str) -> JsValue {
    let address = match Address::from_hex(address_hex) {
        Ok(a) => a,
        Err(e) => return error_value(e),
    };
    let slot = match parse_u256_hex(slot_hex) {
        Ok(s) => s,
        Err(e) => return error_value(e),
    };

    let value = CHAIN.with(|c| c.borrow().get_storage(address, slot));
    to_js(&value)
}

// ─── Helpers ──────────────────────────────────────────────────────────

fn parse_hex(s: &str) -> Result<Vec<u8>, String> {
    let trimmed = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    // KSR-CVN-PRELIM-003 (Sprint 26 audit): reject odd-length input
    // explicitly. The pre-fix behavior left-padded with `0` silently,
    // turning e.g. `"0xa9059cb"` (one char short of an ERC-20 transfer
    // selector) into bytes `[0x0a, 0x90, 0x59, 0xcb]` — an entirely
    // different selector. EIP-1474 § JSON-RPC requires even-length
    // hex; we now match that.
    if trimmed.len() % 2 == 1 {
        return Err(format!(
            "hex must have even length (got {} chars); pad with a leading 0 explicitly if needed",
            trimmed.len()
        ));
    }
    hex::decode(trimmed).map_err(|e| format!("invalid hex: {e}"))
}

fn parse_u256_hex(s: &str) -> Result<U256, String> {
    let bytes = parse_hex(s)?;
    if bytes.len() > 32 {
        return Err(format!("u256 too long: {} bytes", bytes.len()));
    }
    let mut padded = [0u8; 32];
    padded[32 - bytes.len()..].copy_from_slice(&bytes);
    Ok(U256::from_be_bytes(padded))
}

fn to_js<T: Serialize>(v: &T) -> JsValue {
    serde_wasm_bindgen::to_value(v).unwrap_or(JsValue::NULL)
}

fn error_value(message: String) -> JsValue {
    let err = serde_json::json!({ "error": message });
    serde_wasm_bindgen::to_value(&err).unwrap_or(JsValue::NULL)
}

// ─── Input validators (KSR-CVN-PRELIM-006 — Sprint 26 audit) ─────────
//
// Defense-in-depth: every chain_* entry point validates its parsed
// arg shape *before* doing any chain work. Pre-fix, we relied on
// downstream parsers (Address::from_hex, parse_hex) to reject
// malformed input — which they did, but with errors surfaced from
// deep in the call stack with less actionable messages. The helpers
// below are pure functions so they're safe to unit-test natively.

/// Conservative cap on bytecode hex length: 24 KB EVM contract size
/// (EIP-170) × 2 hex chars per byte + `0x` prefix + slack for
/// constructor args concat.
const MAX_BYTECODE_HEX_CHARS: usize = 2 + 30 * 1024 * 2;

/// Conservative cap on calldata hex length: 1 MB. EVM has no hard
/// upper bound but anything above this is far past anything the
/// playground produces and likely indicates a runaway encoder.
const MAX_CALLDATA_HEX_CHARS: usize = 2 + 1024 * 1024 * 2;

pub(crate) fn validate_address_str(field: &str, value: &str) -> Result<(), String> {
    if value.len() != 42 {
        return Err(format!(
            "{field}: address must be 42 chars (0x + 40 hex), got {}",
            value.len()
        ));
    }
    if !(value.starts_with("0x") || value.starts_with("0X")) {
        return Err(format!("{field}: address must start with 0x"));
    }
    Ok(())
}

pub(crate) fn validate_hex_str(field: &str, value: &str, max_chars: usize) -> Result<(), String> {
    if !(value.starts_with("0x") || value.starts_with("0X")) {
        return Err(format!("{field}: must start with 0x"));
    }
    if value.len() > max_chars {
        return Err(format!(
            "{field}: too long ({} chars, max {max_chars})",
            value.len()
        ));
    }
    Ok(())
}

pub(crate) fn validate_deploy_args(args: &DeployArgs) -> Result<(), String> {
    validate_address_str("deployer", &args.deployer)?;
    validate_hex_str("bytecode_hex", &args.bytecode_hex, MAX_BYTECODE_HEX_CHARS)?;
    if let Some(ca) = &args.constructor_args_hex {
        validate_hex_str("constructor_args_hex", ca, MAX_CALLDATA_HEX_CHARS)?;
    }
    Ok(())
}

pub(crate) fn validate_call_args(args: &CallArgs) -> Result<(), String> {
    validate_address_str("from", &args.from)?;
    validate_address_str("to", &args.to)?;
    validate_hex_str("calldata_hex", &args.calldata_hex, MAX_CALLDATA_HEX_CHARS)?;
    if let Some(v) = &args.value_wei_hex {
        // u256 is 32 bytes max → 64 hex chars + 0x.
        validate_hex_str("value_wei_hex", v, 2 + 64)?;
    }
    Ok(())
}

// ─── Native unit tests (Sprint 26 regression coverage) ───────────────
//
// These run as `cargo test -p covenant-wasm-bindings`. They cover the
// helpers added/modified by the audit fixes; the WASM-only entry
// points (chain_*) are exercised separately by the Phase 3 Puppeteer
// harness in `covenant-playground/scripts/`.

#[cfg(test)]
mod tests {
    use super::*;

    // ─── PRELIM-003 ──────────────────────────────────────────────

    #[test]
    fn parse_hex_accepts_even_length() {
        assert_eq!(
            parse_hex("0xa9059cbb").unwrap(),
            vec![0xa9, 0x05, 0x9c, 0xbb]
        );
        assert_eq!(parse_hex("0x").unwrap(), Vec::<u8>::new());
        assert_eq!(parse_hex("").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn parse_hex_rejects_odd_length_no_silent_pad() {
        // The pre-Sprint-26 behavior silently produced [0x0a, 0x90, 0x59, 0xcb]
        // — an entirely different selector from the user's `a9059cb` typo.
        let r = parse_hex("0xa9059cb");
        assert!(r.is_err(), "odd-length hex must be rejected, got {r:?}");
        let msg = r.unwrap_err();
        assert!(msg.contains("even"), "error must explain: {msg}");
    }

    #[test]
    fn parse_hex_rejects_non_hex_chars() {
        assert!(parse_hex("0xZZ").is_err());
        assert!(parse_hex("0x ab").is_err());
    }

    // ─── PRELIM-006 ──────────────────────────────────────────────

    #[test]
    fn validate_deploy_args_rejects_short_address() {
        let args = DeployArgs {
            deployer: "0xshort".to_string(),
            bytecode_hex: "0x60".to_string(),
            constructor_args_hex: None,
        };
        let err = validate_deploy_args(&args).unwrap_err();
        assert!(err.contains("deployer"), "error context lost: {err}");
        assert!(err.contains("42"), "expected len mention: {err}");
    }

    #[test]
    fn validate_deploy_args_rejects_missing_0x() {
        let args = DeployArgs {
            deployer: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            bytecode_hex: "0x60".to_string(),
            constructor_args_hex: None,
        };
        let err = validate_deploy_args(&args).unwrap_err();
        assert!(err.contains("0x"), "must mention 0x prefix: {err}");
    }

    #[test]
    fn validate_deploy_args_rejects_oversize_bytecode() {
        let args = DeployArgs {
            deployer: format!("0x{}", "a".repeat(40)),
            bytecode_hex: format!("0x{}", "ab".repeat(40 * 1024)),
            constructor_args_hex: None,
        };
        let err = validate_deploy_args(&args).unwrap_err();
        assert!(err.contains("bytecode_hex"), "error context lost: {err}");
        assert!(err.contains("max"), "expected size cap mention: {err}");
    }

    #[test]
    fn validate_call_args_rejects_oversize_value_wei() {
        let args = CallArgs {
            from: format!("0x{}", "a".repeat(40)),
            to: format!("0x{}", "b".repeat(40)),
            calldata_hex: "0xa9059cbb".to_string(),
            // 65 hex chars > 64 max for u256
            value_wei_hex: Some(format!("0x{}", "f".repeat(65))),
        };
        let err = validate_call_args(&args).unwrap_err();
        assert!(err.contains("value_wei_hex"), "error context lost: {err}");
    }

    #[test]
    fn validate_call_args_accepts_well_formed() {
        let args = CallArgs {
            from: format!("0x{}", "a".repeat(40)),
            to: format!("0x{}", "b".repeat(40)),
            calldata_hex: "0xa9059cbb".to_string(),
            value_wei_hex: Some("0x0".to_string()),
        };
        assert!(validate_call_args(&args).is_ok());
    }
}

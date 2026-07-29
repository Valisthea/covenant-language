//! Multi-contract chain state on top of the single-call `evm::execute`.
//!
//! A `Chain` owns deployed contracts, accounts, the clock, and the
//! ordered transaction log. Deploys and calls flow through this struct;
//! the underlying `execute` function does the bytecode interpretation.
//!
//! ## Shape vs `harness::CovenantTestHarness`
//!
//! The harness facade in `covenant-testing` is the single-contract
//! version of this: it owns one `HostState` + one `MockPrecompileState`
//! and tracks one deployed code blob. `Chain` generalizes to N contracts:
//!
//! - Each call resolves the `to` address against `self.contracts`
//!   to find the runtime bytecode to execute.
//! - The shared `HostState.storage` BTreeMap is keyed by `Address`,
//!   so multi-contract storage isolation comes for free from the
//!   underlying interpreter.
//! - One `MockPrecompileState` is shared across all contracts,
//!   matches real precompile semantics (stateless from the contract's
//!   point of view; precompile-internal handle counters bump globally).
//!
//! ## What we deliberately don't do
//!
//! - **No gas accounting.** The compiler doesn't emit gas-aware code.
//!   `gas_used` is a fixed constant in receipts so the playground UI
//!   has a number to display.
//! - **No mempool / consensus / P2P.** Single-tab state machine.
//! - **No `snapshot`/`restore` in this MVP.** `Chain::new()` /
//!   `Chain::with_prefunded_accounts()` is the reset path. Snapshot
//!   support can land in a later phase if a use case appears.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};

use crate::address::Address;
use crate::clock::{Clock, DEFAULT_BLOCK_TIME_SECS};
use crate::evm::{execute, CallEnv, CallResult, HostState, LogEvent};
use crate::precompiles::MockPrecompileState;
use crate::u256::U256;

/// Initial balance assigned to every prefunded playground account: 1000 ETH
/// (`1e21` wei). Mirrors the convention of every Ethereum local-dev tool.
pub const PREFUNDED_BALANCE_WEI: u128 = 1_000_000_000_000_000_000_000u128;

/// The 5 prefunded playground accounts. Lowercase hex with 0x prefix.
/// Distinctive byte pattern (`0xaaaa…0001`-style) so they're recognisable
/// at a glance in the UI's Tx History.
pub const PREFUNDED_ADDRESSES: [&str; 5] = [
    "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa0001",
    "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa0002",
    "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa0003",
    "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa0004",
    "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa0005",
];

/// Chain id used inside `CallEnv`. The real EVM consults this for
/// `CHAINID` opcode; the playground picks a value clearly outside
/// reserved ranges so contracts can detect "I'm running in MockChain".
pub const CHAIN_ID: u64 = 31_337;

/// Fixed gas estimate stamped onto every receipt. The compiler doesn't
/// emit gas-aware code yet; faking a real number would mislead users.
/// The UI shows it as a constant so the column doesn't go blank.
pub const FIXED_GAS_ESTIMATE: u64 = 10_000;

// ─── Chain state ──────────────────────────────────────────────────────

/// A deployed contract.
///
/// Storage and runtime bytecode are owned here. The `HostState` used
/// during a call is built by copying `self.storage` into it on entry
/// and copying it back on successful return: see [`Chain::call`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    pub address: Address,
    pub deployer: Address,
    pub deploy_bytecode: Vec<u8>,
    pub runtime_bytecode: Vec<u8>,
    pub storage: BTreeMap<U256, U256>,
    pub code_hash: [u8; 32],
    pub deployed_at_block: u64,
    pub deployed_at_timestamp: u64,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub address: Address,
    pub balance: U256,
    pub nonce: u64,
    pub label: String,
}

/// JS-friendly version of [`evm::LogEvent`]: topics + data come out
/// as `0x`-prefixed hex strings instead of raw byte arrays.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainLogEvent {
    pub address: Address,
    /// Each topic is 32 bytes, encoded as `"0x"` + 64 hex chars.
    pub topics: Vec<String>,
    /// Variable-length event data, `"0x"` + 2N hex chars.
    pub data: String,
}

impl From<&LogEvent> for ChainLogEvent {
    fn from(l: &LogEvent) -> Self {
        ChainLogEvent {
            address: l.address,
            topics: l
                .topics
                .iter()
                .map(|t| format!("0x{}", hex::encode(t)))
                .collect(),
            data: format!("0x{}", hex::encode(&l.data)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TxKind {
    Deploy,
    Call {
        /// 4-byte function selector as `"0x"` + 8 hex chars.
        selector: String,
        /// Full calldata as `"0x"` + 2N hex chars (selector + ABI-encoded args).
        calldata: String,
    },
    StaticCall {
        selector: String,
        calldata: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TxStatus {
    Success,
    Reverted {
        reason: Option<String>,
    },
    /// `INVALID` opcode, unknown opcode, or interpreter abort. Distinct
    /// from `Reverted` because there's no return data to decode, the
    /// inner string is the abort reason from the interpreter.
    Aborted {
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxReceipt {
    /// `"0x"` + 64 hex chars. Deterministic per `(block_number, tx_index, from)`.
    pub hash: String,
    pub block_number: u64,
    pub timestamp: u64,
    pub from: Address,
    pub to: Option<Address>,
    pub kind: TxKind,
    pub gas_used: u64,
    pub status: TxStatus,
    /// `"0x"` + 2N hex chars. Empty (`"0x"`) when the call returned nothing.
    pub return_data: String,
    pub logs: Vec<ChainLogEvent>,
}

/// Top-level chain state. One instance per playground tab.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chain {
    pub contracts: BTreeMap<Address, Contract>,
    pub accounts: BTreeMap<Address, Account>,
    pub clock: Clock,
    pub block_number: u64,
    pub tx_log: Vec<TxReceipt>,
    /// Monotonic deploy nonce per deployer; used for deterministic
    /// CREATE-style address derivation.
    pub deploy_nonces: BTreeMap<Address, u64>,
    /// Shared precompile scratchpad: FHE handles, PQ counters, etc.
    /// Skipped from serialization because it'd cascade `HashMap` →
    /// non-Serialize and we don't yet need snapshot/restore parity for
    /// this auxiliary state.
    #[serde(skip)]
    pub precompiles: MockPrecompileState,
}

impl Chain {
    /// Empty chain. Useful for unit tests that want a clean slate
    /// without the prefunded accounts.
    pub fn new() -> Self {
        Chain {
            contracts: BTreeMap::new(),
            accounts: BTreeMap::new(),
            clock: Clock::new(),
            block_number: 1,
            tx_log: Vec::new(),
            deploy_nonces: BTreeMap::new(),
            precompiles: MockPrecompileState::default(),
        }
    }

    /// Production-flavoured chain: 5 prefunded accounts, each with
    /// 1000 ETH. Matches what every wallet tutorial expects to see.
    pub fn with_prefunded_accounts() -> Self {
        let mut chain = Chain::new();
        for (idx, hex_str) in PREFUNDED_ADDRESSES.iter().enumerate() {
            let address = Address::from_hex(hex_str).expect("hard-coded prefunded address parses");
            chain.accounts.insert(
                address,
                Account {
                    address,
                    balance: U256::from_u128(PREFUNDED_BALANCE_WEI),
                    nonce: 0,
                    label: format!("Account #{}", idx + 1),
                },
            );
        }
        chain
    }

    // ─── Core operations ──────────────────────────────────────────────

    /// Deploy `deploy_bytecode` from `deployer`. The bytecode runs as
    /// the constructor; its `RETURN` payload becomes the runtime code
    /// stored under the new contract address.
    ///
    /// The new contract's address is derived deterministically from
    /// `(deployer, nonce)`. Caller can predict it before calling by
    /// reading `chain.deploy_nonces[deployer]`.
    pub fn deploy(
        &mut self,
        deployer: Address,
        deploy_bytecode: &[u8],
        constructor_args: &[u8],
    ) -> TxReceipt {
        let nonce = *self.deploy_nonces.get(&deployer).unwrap_or(&0);
        let new_address = derive_create_address(deployer, nonce);
        self.deploy_nonces.insert(deployer, nonce + 1);

        let env = CallEnv {
            caller: deployer,
            address: new_address,
            value: U256::ZERO,
            input: constructor_args.to_vec(),
            timestamp: self.clock.timestamp(),
            block_number: self.block_number,
            chain_id: CHAIN_ID,
            is_static: false,
        };

        // Standard EVM convention: deploy bytecode is concatenated with
        // constructor args, and the bytecode reads its args via CODECOPY
        // from offset = code length.
        let mut full_bytecode = deploy_bytecode.to_vec();
        full_bytecode.extend_from_slice(constructor_args);

        let mut host = HostState::default();
        let result = execute(&full_bytecode, &env, &mut host, &mut self.precompiles);

        match result {
            CallResult::Ok(runtime_bytecode) => {
                let code_hash = keccak256(&runtime_bytecode);
                let storage = host.storage.get(&new_address).cloned().unwrap_or_default();

                self.contracts.insert(
                    new_address,
                    Contract {
                        address: new_address,
                        deployer,
                        deploy_bytecode: deploy_bytecode.to_vec(),
                        runtime_bytecode,
                        storage,
                        code_hash,
                        deployed_at_block: self.block_number,
                        deployed_at_timestamp: self.clock.timestamp(),
                        label: None,
                    },
                );

                let logs = host.logs.iter().map(ChainLogEvent::from).collect();
                self.finalize_receipt(
                    deployer,
                    Some(new_address),
                    TxKind::Deploy,
                    TxStatus::Success,
                    Vec::new(),
                    logs,
                )
            }
            CallResult::Revert(data) => self.finalize_receipt(
                deployer,
                None,
                TxKind::Deploy,
                TxStatus::Reverted {
                    reason: decode_revert_reason(&data),
                },
                data,
                Vec::new(),
            ),
            CallResult::Abort(reason) => self.finalize_receipt(
                deployer,
                None,
                TxKind::Deploy,
                TxStatus::Aborted { reason },
                Vec::new(),
                Vec::new(),
            ),
        }
    }

    /// State-mutating call. Storage changes on success commit back to
    /// the contract; on revert/abort, they're discarded.
    pub fn call(&mut self, from: Address, to: Address, calldata: &[u8], value: U256) -> TxReceipt {
        let kind = TxKind::Call {
            selector: hex_selector(calldata),
            calldata: format!("0x{}", hex::encode(calldata)),
        };

        let runtime_bytecode = match self.contracts.get(&to) {
            Some(c) => c.runtime_bytecode.clone(),
            None => {
                return self.finalize_receipt(
                    from,
                    Some(to),
                    kind,
                    TxStatus::Reverted {
                        reason: Some(format!("no contract deployed at {to}")),
                    },
                    Vec::new(),
                    Vec::new(),
                );
            }
        };

        let env = CallEnv {
            caller: from,
            address: to,
            value,
            input: calldata.to_vec(),
            timestamp: self.clock.timestamp(),
            block_number: self.block_number,
            chain_id: CHAIN_ID,
            is_static: false,
        };

        let mut host = HostState::default();
        // Seed host with the contract's current storage.
        if let Some(c) = self.contracts.get(&to) {
            host.storage.insert(to, c.storage.clone());
        }

        let result = execute(&runtime_bytecode, &env, &mut host, &mut self.precompiles);

        match result {
            CallResult::Ok(return_data) => {
                if let Some(c) = self.contracts.get_mut(&to) {
                    if let Some(updated_storage) = host.storage.remove(&to) {
                        c.storage = updated_storage;
                    }
                }
                let logs = host.logs.iter().map(ChainLogEvent::from).collect();
                self.finalize_receipt(from, Some(to), kind, TxStatus::Success, return_data, logs)
            }
            CallResult::Revert(data) => self.finalize_receipt(
                from,
                Some(to),
                kind,
                TxStatus::Reverted {
                    reason: decode_revert_reason(&data),
                },
                data,
                Vec::new(),
            ),
            CallResult::Abort(reason) => self.finalize_receipt(
                from,
                Some(to),
                kind,
                TxStatus::Aborted { reason },
                Vec::new(),
                Vec::new(),
            ),
        }
    }

    /// Read-only call: storage changes are dropped, no log entry is
    /// appended. Used by `view` actions in the playground's
    /// InteractionPanel.
    ///
    /// `&self` because we don't mutate the chain at all, including
    /// `self.precompiles`. We clone the precompile state into a local
    /// scratch copy so the interpreter can mint FHE handles internally
    /// without leaving them in the global state.
    pub fn static_call(&self, from: Address, to: Address, calldata: &[u8]) -> TxReceipt {
        let kind = TxKind::StaticCall {
            selector: hex_selector(calldata),
            calldata: format!("0x{}", hex::encode(calldata)),
        };

        let runtime_bytecode = match self.contracts.get(&to) {
            Some(c) => c.runtime_bytecode.clone(),
            None => {
                return TxReceipt {
                    hash: "0x".to_string() + &"0".repeat(64),
                    block_number: self.block_number,
                    timestamp: self.clock.timestamp(),
                    from,
                    to: Some(to),
                    kind,
                    gas_used: 0,
                    status: TxStatus::Reverted {
                        reason: Some(format!("no contract deployed at {to}")),
                    },
                    return_data: "0x".to_string(),
                    logs: Vec::new(),
                };
            }
        };

        let env = CallEnv {
            caller: from,
            address: to,
            value: U256::ZERO,
            input: calldata.to_vec(),
            timestamp: self.clock.timestamp(),
            block_number: self.block_number,
            chain_id: CHAIN_ID,
            is_static: true,
        };

        let mut host = HostState::default();
        if let Some(c) = self.contracts.get(&to) {
            host.storage.insert(to, c.storage.clone());
        }
        let mut precompiles_scratch = self.precompiles.clone();
        let result = execute(&runtime_bytecode, &env, &mut host, &mut precompiles_scratch);

        let (status, return_data) = match result {
            CallResult::Ok(data) => (TxStatus::Success, data),
            CallResult::Revert(data) => (
                TxStatus::Reverted {
                    reason: decode_revert_reason(&data),
                },
                data,
            ),
            CallResult::Abort(reason) => (TxStatus::Aborted { reason }, Vec::new()),
        };

        TxReceipt {
            hash: "0x".to_string() + &"0".repeat(64),
            block_number: self.block_number,
            timestamp: self.clock.timestamp(),
            from,
            to: Some(to),
            kind,
            gas_used: 0,
            status,
            return_data: format!("0x{}", hex::encode(&return_data)),
            logs: Vec::new(),
        }
    }

    // ─── Clock + block controls ───────────────────────────────────────

    pub fn advance_time(&mut self, seconds: u64) {
        self.clock.advance(seconds);
    }

    /// Bump the block number by `count`. Each mined block also bumps
    /// the clock by `DEFAULT_BLOCK_TIME_SECS` (12s, post-Merge cadence).
    pub fn mine_blocks(&mut self, count: u64) {
        for _ in 0..count {
            self.block_number = self.block_number.saturating_add(1);
            self.clock.advance(DEFAULT_BLOCK_TIME_SECS);
        }
    }

    // ─── Read-only inspection ─────────────────────────────────────────

    pub fn get_balance(&self, address: Address) -> U256 {
        self.accounts
            .get(&address)
            .map(|a| a.balance)
            .unwrap_or(U256::ZERO)
    }

    pub fn get_storage(&self, address: Address, slot: U256) -> U256 {
        self.contracts
            .get(&address)
            .and_then(|c| c.storage.get(&slot).copied())
            .unwrap_or(U256::ZERO)
    }

    /// Replace the chain entirely. `Chain::default()` is the canonical
    /// reset target: no need for snapshot/restore for the playground's
    /// "Reset" button.
    pub fn reset(&mut self) {
        *self = Chain::with_prefunded_accounts();
    }

    // ─── Internals ────────────────────────────────────────────────────

    fn finalize_receipt(
        &mut self,
        from: Address,
        to: Option<Address>,
        kind: TxKind,
        status: TxStatus,
        return_data: Vec<u8>,
        logs: Vec<ChainLogEvent>,
    ) -> TxReceipt {
        let mut hasher = Keccak256::new();
        hasher.update(self.block_number.to_be_bytes());
        hasher.update((self.tx_log.len() as u64).to_be_bytes());
        hasher.update(from.as_bytes());
        if let Some(t) = to {
            hasher.update(t.as_bytes());
        }
        let hash_bytes: [u8; 32] = hasher.finalize().into();
        let hash = format!("0x{}", hex::encode(hash_bytes));

        let receipt = TxReceipt {
            hash,
            block_number: self.block_number,
            timestamp: self.clock.timestamp(),
            from,
            to,
            kind,
            gas_used: FIXED_GAS_ESTIMATE,
            status,
            return_data: format!("0x{}", hex::encode(&return_data)),
            logs,
        };

        self.tx_log.push(receipt.clone());
        receipt
    }
}

impl Default for Chain {
    fn default() -> Self {
        Self::with_prefunded_accounts()
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────

/// Deterministic CREATE-style address derivation.
///
/// Real Ethereum CREATE uses RLP(deployer || nonce). We don't depend
/// on RLP here: `keccak256(deployer || nonce_be_bytes)[12..32]` is
/// deterministic and unambiguous, sufficient for the playground.
/// Documented in the playground's "Why does my address differ from
/// real Ethereum?" FAQ.
pub fn derive_create_address(deployer: Address, nonce: u64) -> Address {
    let mut hasher = Keccak256::new();
    hasher.update(deployer.as_bytes());
    hasher.update(nonce.to_be_bytes());
    let hash: [u8; 32] = hasher.finalize().into();
    let mut out = [0u8; 20];
    out.copy_from_slice(&hash[12..32]);
    Address(out)
}

/// `"0x" + 8 hex chars` of the leading 4 calldata bytes. Returns
/// `"0x00000000"` if calldata is shorter than 4 bytes (matches the
/// Solidity convention for fallback selectors).
fn hex_selector(calldata: &[u8]) -> String {
    let mut buf = [0u8; 4];
    let n = calldata.len().min(4);
    buf[..n].copy_from_slice(&calldata[..n]);
    format!("0x{}", hex::encode(buf))
}

fn keccak256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Keccak256::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// Decode a Solidity `Error(string)` revert payload into the message.
/// Returns `None` for non-Solidity reverts (custom errors, raw revert,
/// empty data).
fn decode_revert_reason(data: &[u8]) -> Option<String> {
    // Selector for Error(string): keccak256("Error(string)")[..4] = 0x08c379a0
    if data.len() < 68 || data[..4] != [0x08, 0xc3, 0x79, 0xa0] {
        return None;
    }
    // ABI layout: selector | offset (32 bytes, always 0x20) | length (32) | payload
    let len_offset = 4 + 32;
    if data.len() < len_offset + 32 {
        return None;
    }
    let mut len_bytes = [0u8; 32];
    len_bytes.copy_from_slice(&data[len_offset..len_offset + 32]);
    let len = U256::from_be_bytes(len_bytes).low_u64() as usize;

    let str_start = len_offset + 32;
    if str_start + len > data.len() {
        return None;
    }
    String::from_utf8(data[str_start..str_start + len].to_vec()).ok()
}

// MockPrecompileState doesn't derive Clone: it has a HashMap inside.
// Add a manual Clone impl so static_call can scratch-clone it.
impl Clone for MockPrecompileState {
    fn clone(&self) -> Self {
        // Field-by-field clone via Default + manual copy. Used only in
        // static_call which discards the result: performance is not
        // critical (a static_call already costs an EVM execution).
        let mut copy = MockPrecompileState::default();
        copy.fhe_handles = self.fhe_handles.clone();
        copy.pq_force_fail = self.pq_force_fail;
        copy.pq_nonce = self.pq_nonce;
        copy.zk_force_fail = self.zk_force_fail;
        copy.amnesia_nonce = self.amnesia_nonce;
        copy
    }
}

// ─── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn alice() -> Address {
        Address::from_hex(PREFUNDED_ADDRESSES[0]).unwrap()
    }

    fn bob() -> Address {
        Address::from_hex(PREFUNDED_ADDRESSES[1]).unwrap()
    }

    #[test]
    fn prefunded_accounts_have_balance() {
        let chain = Chain::with_prefunded_accounts();
        assert_eq!(chain.accounts.len(), 5);
        assert_eq!(
            chain.get_balance(alice()),
            U256::from_u128(PREFUNDED_BALANCE_WEI)
        );
        // Balance for an unknown address is zero, never panics.
        let stranger = Address::from_low_u64(0xdeadbeef);
        assert_eq!(chain.get_balance(stranger), U256::ZERO);
    }

    #[test]
    fn deploy_address_is_deterministic() {
        let mut chain = Chain::with_prefunded_accounts();
        // Empty bytecode: constructor returns empty runtime, deploy
        // succeeds with a deterministic address.
        let stop_only = vec![0x00]; // STOP
        let r1 = chain.deploy(alice(), &stop_only, &[]);
        let r2 = chain.deploy(alice(), &stop_only, &[]);
        // Two deploys from the same address must produce two distinct
        // contract addresses.
        let a1 = r1.to.expect("deploy r1 returns address");
        let a2 = r2.to.expect("deploy r2 returns address");
        assert_ne!(a1, a2);
        // And the same nonce-from-clean-state always yields the same
        // address: predictable for tests.
        let mut chain2 = Chain::with_prefunded_accounts();
        let r3 = chain2.deploy(alice(), &stop_only, &[]);
        assert_eq!(a1, r3.to.unwrap());
    }

    #[test]
    fn deploy_increments_nonce_and_block_unchanged() {
        let mut chain = Chain::with_prefunded_accounts();
        let initial_block = chain.block_number;
        chain.deploy(alice(), &[0x00], &[]);
        chain.deploy(alice(), &[0x00], &[]);
        assert_eq!(chain.deploy_nonces[&alice()], 2);
        // Each call doesn't auto-mine a block: that's an explicit op.
        assert_eq!(chain.block_number, initial_block);
        assert_eq!(chain.tx_log.len(), 2);
    }

    #[test]
    fn call_to_unknown_contract_reverts_cleanly() {
        let mut chain = Chain::with_prefunded_accounts();
        let target = Address::from_low_u64(0x1234); // not deployed
        let receipt = chain.call(alice(), target, &[0xa9, 0x05, 0x9c, 0xbb], U256::ZERO);
        assert!(matches!(receipt.status, TxStatus::Reverted { .. }));
        assert_eq!(chain.tx_log.len(), 1);
    }

    #[test]
    fn static_call_does_not_mutate_log_or_state() {
        let chain = Chain::with_prefunded_accounts();
        let target = Address::from_low_u64(0x99); // not deployed → revert
        let receipt = chain.static_call(alice(), target, &[]);
        assert!(matches!(receipt.status, TxStatus::Reverted { .. }));
        // tx_log is on `&self` so we can't even append, the borrow checker
        // enforces it. Nothing to assert beyond the type signature.
        assert_eq!(chain.tx_log.len(), 0);
    }

    #[test]
    fn mine_blocks_advances_time_too() {
        let mut chain = Chain::with_prefunded_accounts();
        let t0 = chain.clock.timestamp();
        let b0 = chain.block_number;
        chain.mine_blocks(5);
        assert_eq!(chain.block_number, b0 + 5);
        assert_eq!(chain.clock.timestamp(), t0 + 5 * DEFAULT_BLOCK_TIME_SECS);
    }

    #[test]
    fn reset_restores_genesis() {
        let mut chain = Chain::with_prefunded_accounts();
        chain.deploy(alice(), &[0x00], &[]);
        chain.mine_blocks(10);
        assert!(!chain.contracts.is_empty());
        assert!(!chain.tx_log.is_empty());
        assert_ne!(chain.block_number, 1);

        chain.reset();
        assert!(chain.contracts.is_empty());
        assert!(chain.tx_log.is_empty());
        assert_eq!(chain.block_number, 1);
        assert_eq!(chain.accounts.len(), 5);
    }

    #[test]
    fn tx_receipt_serialises_to_expected_json_shape() {
        let mut chain = Chain::with_prefunded_accounts();
        let r = chain.deploy(alice(), &[0x00], &[]);
        let json = serde_json::to_string(&r).unwrap();
        // Sanity: structural fields the playground depends on.
        assert!(json.contains("\"hash\":\"0x"));
        assert!(json.contains("\"block_number\":1"));
        assert!(json.contains("\"from\":\"0xaaaa"));
        assert!(json.contains("\"kind\":{\"type\":\"deploy\"}"));
        assert!(json.contains("\"status\":{\"status\":\"success\"}"));
        // round-trip
        let back: TxReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(back.from, alice());
    }

    #[test]
    fn revert_decoder_recovers_solidity_error_string() {
        // Build a synthetic Error(string) payload: selector | 0x20 | len | "boom"
        let mut data = Vec::new();
        data.extend_from_slice(&[0x08, 0xc3, 0x79, 0xa0]);
        let mut offset = [0u8; 32];
        offset[31] = 0x20;
        data.extend_from_slice(&offset);
        let mut len = [0u8; 32];
        len[31] = 4;
        data.extend_from_slice(&len);
        data.extend_from_slice(b"boom");
        // Pad to multiple of 32 with zeros (ABI layout).
        data.extend_from_slice(&[0u8; 28]);

        assert_eq!(decode_revert_reason(&data).as_deref(), Some("boom"));
    }

    #[test]
    fn unrelated_addresses_get_distinct_state() {
        let mut chain = Chain::with_prefunded_accounts();
        let r1 = chain.deploy(alice(), &[0x00], &[]);
        let r2 = chain.deploy(bob(), &[0x00], &[]);
        assert_ne!(r1.to.unwrap(), r2.to.unwrap());
        assert_eq!(chain.contracts.len(), 2);
    }
}

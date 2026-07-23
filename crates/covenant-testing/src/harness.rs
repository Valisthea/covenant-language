//! Top-level test harness: compile-deploy-call facade for Covenant contracts.
//!
//! Typical usage in a scenario test:
//!
//! ```ignore
//! let mut h = CovenantTestHarness::new();
//! let coin = h.deploy(SOURCE, h.addrs.deployer).expect("deploy");
//! let ok = h.call_ok(coin, h.addrs.alice,
//!     "transfer(address,uint256)",
//!     &[h.addrs.bob.to_u256().into(), U256::from_u64(100)]);
//! ```

use covenant_diag::{Diagnostic, DiagnosticLevel, SourceId};
use covenant_driver::compile;
use covenant_evm_backend::{EvmArtifact, EvmConfig};
use covenant_opt::OptimizerConfig;
use covenant_stdlib::StdlibConfig;

use crate::abi::{decode_string, decode_u256, encode_call, event_topic};
use crate::address::Address;
use crate::evm::{execute, CallEnv, CallResult, HostState, LogEvent};
use crate::precompiles::MockPrecompileState;
use crate::u256::U256;

/// Pre-funded test addresses.
#[derive(Debug, Clone, Copy)]
pub struct TestAddrs {
    pub deployer: Address,
    pub alice: Address,
    pub bob: Address,
    pub carol: Address,
    pub dave: Address,
}

impl Default for TestAddrs {
    fn default() -> Self {
        Self {
            deployer: Address::from_low_u64(0x1000),
            alice: Address::from_low_u64(0x2001),
            bob: Address::from_low_u64(0x2002),
            carol: Address::from_low_u64(0x2003),
            dave: Address::from_low_u64(0x2004),
        }
    }
}

/// A deployed contract handle.
#[derive(Debug, Clone, Copy)]
pub struct Contract {
    pub address: Address,
    artifact_idx: usize,
}

/// The testing harness: holds host state, precompile state, and per-contract artifacts.
pub struct CovenantTestHarness {
    pub host: HostState,
    pub precompiles: MockPrecompileState,
    pub addrs: TestAddrs,
    pub timestamp: u64,
    pub block_number: u64,
    pub chain_id: u64,
    artifacts: Vec<EvmArtifact>,
    next_address: u64,
}

impl Default for CovenantTestHarness {
    fn default() -> Self {
        Self::new()
    }
}

impl CovenantTestHarness {
    pub fn new() -> Self {
        let mut host = HostState::default();
        let addrs = TestAddrs::default();
        // Pre-fund test accounts.
        let big = U256::from_u64(1_000_000_000_000_000_000u64);
        for a in [
            addrs.deployer,
            addrs.alice,
            addrs.bob,
            addrs.carol,
            addrs.dave,
        ] {
            host.balances.insert(a, big);
        }
        Self {
            host,
            precompiles: MockPrecompileState::default(),
            addrs,
            timestamp: 1_700_000_000,
            block_number: 1,
            chain_id: 1,
            artifacts: Vec::new(),
            next_address: 0xC0DE_0000,
        }
    }

    /// Advance block timestamp.
    pub fn warp(&mut self, by_secs: u64) {
        self.timestamp += by_secs;
        self.block_number += 1;
    }

    /// Compile `source` and deploy the contract from `caller`.
    ///
    /// Returns the deployed contract handle. Deploy bytecode is run as the
    /// constructor; the resulting runtime is stored in host code at the new
    /// contract address.
    pub fn deploy(&mut self, source: &str, caller: Address) -> Result<Contract, Vec<Diagnostic>> {
        // The harness exists to run `test_*` actions, so it must keep them in
        // the emitted contract. A release build (EvmConfig::default) strips
        // them — see `include_test_actions`.
        let config = EvmConfig {
            include_test_actions: true,
            ..EvmConfig::default()
        };
        let (artifact_opt, diags) = compile(
            source,
            SourceId::new(0),
            config,
            StdlibConfig::default(),
            OptimizerConfig::default(),
        );
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.level == DiagnosticLevel::Error)
            .cloned()
            .collect();
        if !errors.is_empty() {
            return Err(errors);
        }
        let artifact = artifact_opt.ok_or_else(|| diags.clone())?;
        let contract_addr = self.alloc_address();

        // Run constructor.
        let env = CallEnv {
            caller,
            address: contract_addr,
            value: U256::ZERO,
            input: Vec::new(),
            timestamp: self.timestamp,
            block_number: self.block_number,
            chain_id: self.chain_id,
            is_static: false,
        };
        let result = execute(
            &artifact.deploy_bytecode,
            &env,
            &mut self.host,
            &mut self.precompiles,
        );
        match result {
            CallResult::Ok(runtime_bytes) => {
                // Convention: constructor returns runtime bytecode.
                self.host.code.insert(contract_addr, runtime_bytes);
            }
            CallResult::Revert(data) => {
                return Err(vec![Diagnostic::error(
                    covenant_diag::DiagCode(1001),
                    format!("deployment reverted with {} bytes of data", data.len()),
                    covenant_diag::Span::new(SourceId::new(0), 0, 0),
                )]);
            }
            CallResult::Abort(reason) => {
                return Err(vec![Diagnostic::error(
                    covenant_diag::DiagCode(1002),
                    format!("deployment aborted: {reason}"),
                    covenant_diag::Span::new(SourceId::new(0), 0, 0),
                )]);
            }
        }

        let idx = self.artifacts.len();
        self.artifacts.push(artifact);
        Ok(Contract {
            address: contract_addr,
            artifact_idx: idx,
        })
    }

    pub fn artifact(&self, c: Contract) -> &EvmArtifact {
        &self.artifacts[c.artifact_idx]
    }

    /// Invoke a function by signature, with positional U256 args.
    pub fn call(
        &mut self,
        contract: Contract,
        caller: Address,
        signature: &str,
        args: &[U256],
    ) -> CallResult {
        self.call_with_value(contract, caller, signature, args, U256::ZERO)
    }

    /// Invoke with an explicit CALLVALUE.
    pub fn call_with_value(
        &mut self,
        contract: Contract,
        caller: Address,
        signature: &str,
        args: &[U256],
        value: U256,
    ) -> CallResult {
        let input = encode_call(signature, args);
        self.raw_call(contract, caller, input, value, false)
    }

    /// Invoke with raw calldata bytes.
    pub fn raw_call(
        &mut self,
        contract: Contract,
        caller: Address,
        input: Vec<u8>,
        value: U256,
        is_static: bool,
    ) -> CallResult {
        let code = self
            .host
            .code
            .get(&contract.address)
            .cloned()
            .unwrap_or_default();
        let env = CallEnv {
            caller,
            address: contract.address,
            value,
            input,
            timestamp: self.timestamp,
            block_number: self.block_number,
            chain_id: self.chain_id,
            is_static,
        };
        execute(&code, &env, &mut self.host, &mut self.precompiles)
    }

    /// Call and assert OK, returning the return data.
    #[track_caller]
    pub fn call_ok(
        &mut self,
        contract: Contract,
        caller: Address,
        signature: &str,
        args: &[U256],
    ) -> Vec<u8> {
        match self.call(contract, caller, signature, args) {
            CallResult::Ok(d) => d,
            CallResult::Revert(d) => {
                panic!(
                    "expected OK from `{signature}`, got REVERT: 0x{}",
                    hex::encode(d)
                );
            }
            CallResult::Abort(r) => {
                panic!("expected OK from `{signature}`, got ABORT: {r}");
            }
        }
    }

    /// Call and assert REVERT, returning the revert data.
    #[track_caller]
    pub fn call_revert(
        &mut self,
        contract: Contract,
        caller: Address,
        signature: &str,
        args: &[U256],
    ) -> Vec<u8> {
        match self.call(contract, caller, signature, args) {
            CallResult::Revert(d) => d,
            CallResult::Ok(d) => {
                panic!(
                    "expected REVERT from `{signature}`, got OK: 0x{}",
                    hex::encode(d)
                );
            }
            CallResult::Abort(r) => {
                panic!("expected REVERT from `{signature}`, got ABORT: {r}");
            }
        }
    }

    /// Call a view function and decode as U256.
    #[track_caller]
    pub fn view_u256(
        &mut self,
        contract: Contract,
        caller: Address,
        signature: &str,
        args: &[U256],
    ) -> U256 {
        let data = self.call_ok(contract, caller, signature, args);
        decode_u256(&data)
    }

    /// Call a view function and decode as a UTF-8 string.
    /// Expects ABI-encoded dynamic string return: [offset][length][data].
    #[track_caller]
    pub fn view_string(
        &mut self,
        contract: Contract,
        caller: Address,
        signature: &str,
        args: &[U256],
    ) -> String {
        let data = self.call_ok(contract, caller, signature, args);
        decode_string(&data)
    }

    /// Find the most recent log whose topic0 matches the canonical 32-byte
    /// keccak256 of `event_sig` (e.g. `"Transfer(address,address,uint256)"`).
    pub fn find_event(&self, event_sig: &str) -> Option<&LogEvent> {
        let topic = event_topic(event_sig);
        self.host
            .logs
            .iter()
            .rev()
            .find(|l| l.topics.first().map(|t| t == &topic).unwrap_or(false))
    }

    /// Count logs matching the canonical topic for `event_sig`.
    pub fn count_events(&self, event_sig: &str) -> usize {
        let topic = event_topic(event_sig);
        self.host
            .logs
            .iter()
            .filter(|l| l.topics.first().map(|t| t == &topic).unwrap_or(false))
            .count()
    }

    fn alloc_address(&mut self) -> Address {
        let a = Address::from_low_u64(self.next_address);
        self.next_address += 1;
        a
    }
}

//! Minimal EVM interpreter and host abstractions for Covenant-emitted
//! bytecode.
//!
//! This crate is the runtime side of the Covenant compiler workspace.
//! Scope: cover exactly the opcode subset `covenant-evm-backend` emits,
//! plus the deterministic mock precompiles for the Styx privacy suite
//! (FHE / PQ / ZK / Amnesia). It is **not** a general-purpose EVM and
//! makes no attempt to match real chain semantics around gas, refunds,
//! warm/cold access lists, or any optional EIP.
//!
//! ## Consumers
//!
//! Three crates in this workspace depend on the runtime:
//!
//! - [`covenant-testing`](https://docs.rs/covenant-testing): re-exports
//!   the modules below so the existing integration suite keeps compiling
//!   unchanged. The `harness::CovenantTestHarness` facade still lives
//!   there because it's test-flavoured (rstest fixtures, hex literals,
//!   compile-deploy-call ergonomics) and brings dev-only deps the WASM
//!   bundle has no business shipping.
//!
//! - `covenant-wasm-bindings`: feeds the playground's `mockchain.ts`
//!   (Sprint 23). The `Chain` abstraction layered on top of `execute()`
//!   lives there for now and may move into this crate in a later sprint.
//!
//! - Future tools (`covenant simulate` CLI, fork-style fuzzers, CI
//!   regression harnesses): share one engine.
//!
//! ## Module map
//!
//! | Module          | What it owns |
//! |-----------------|--------------|
//! | [`u256`]        | 256-bit integer used by EVM stack/memory/storage |
//! | [`address`]     | 20-byte Ethereum-style address |
//! | [`abi`]         | Calldata packing + return decoding for tests |
//! | [`evm`]         | Stack-based interpreter ([`execute`], [`HostState`]) |
//! | [`precompiles`] | Deterministic mocks for the Styx precompile suite |

#![deny(rust_2018_idioms)]

pub mod abi;
pub mod address;
pub mod chain;
pub mod clock;
pub mod evm;
pub mod precompiles;
pub mod u256;

pub use address::Address;
pub use chain::{
    derive_create_address, Account, Chain, ChainLogEvent, Contract, TxKind, TxReceipt, TxStatus,
    CHAIN_ID, FIXED_GAS_ESTIMATE, PREFUNDED_ADDRESSES, PREFUNDED_BALANCE_WEI,
};
pub use clock::{Clock, DEFAULT_BLOCK_TIME_SECS, DEFAULT_GENESIS};
pub use evm::{execute, CallEnv, CallResult, HostState, LogEvent};
pub use precompiles::MockPrecompileState;
pub use u256::U256;

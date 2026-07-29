//! End-to-end test harness for the Covenant compiler.
//!
//! Sprint 23 split this crate in two:
//!
//! * **`covenant-evm-runtime`**: the interpreter, ABI helpers, U256,
//!   Address, and precompile mocks. Reused by the playground's
//!   `covenant-wasm-bindings` (browser MockChain), this crate (test
//!   harness), and any future `covenant simulate` style tool.
//! * **this crate**: the `harness` module, which provides the
//!   compile→deploy→call facade `CovenantTestHarness` consumed by the
//!   13 integration tests in `tests/`.
//!
//! The pre-Sprint-23 module paths (`covenant_testing::abi`,
//! `::address`, `::evm`, `::precompiles`, `::u256`) are preserved via
//! `pub use` so existing test code keeps compiling unchanged.
//!
//! For scope and trade-offs see the `evm` module docs.

// Re-export the runtime modules under their pre-Sprint-23 paths so
// `crate::abi::...`, `crate::evm::...`, etc. inside `harness.rs`
// continue to resolve and integration tests in `tests/` keep their
// `use covenant_testing::{abi, U256, ...}` imports.
pub use covenant_evm_runtime::{abi, address, evm, precompiles, u256};

pub use covenant_evm_runtime::{
    Address, CallEnv, CallResult, HostState, LogEvent, MockPrecompileState, U256,
};

pub mod harness;

pub use harness::{Contract, CovenantTestHarness, TestAddrs};

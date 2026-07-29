//! Shared front-end-to-bytecode plumbing for the backend regression suite,
//! plus a thin wrapper over the mini-EVM interpreter.
//!
//! Several of the defects this suite guards against are only visible in what
//! the emitted bytecode DOES (a reversed shift, a clobbered SSA slot, a guard
//! that accepts short returndata). Asserting on opcode presence alone cannot
//! see them, so these helpers deploy the artifact onto `covenant-evm-runtime`
//! and call it.

#![allow(dead_code)]

use covenant_diag::{Diagnostic, SourceId};
use covenant_evm_backend::{codegen_evm, EvmArtifact, EvmConfig};
use covenant_evm_runtime::{abi, Address, Chain, TxReceipt, TxStatus, U256};
use covenant_ir::build_ir;
use covenant_lexer::tokenize;
use covenant_opt::{optimize, OptimizerConfig};
use covenant_parser::parse;
use covenant_privacy::analyze_privacy;
use covenant_resolver::resolve;
use covenant_stdlib::{lower_stdlib, StdlibConfig};
use covenant_types::typecheck;

/// Run the whole front end plus codegen. Panics if the front end rejects the
/// source: these fixtures are meant to be well-formed programs, so a parse or
/// type error is a broken fixture, not a result.
pub fn compile(src: &str) -> (EvmArtifact, Vec<Diagnostic>) {
    let sid = SourceId::new(0);
    let (toks, _) = tokenize(src, sid);
    let (file, perrs) = parse(&toks, sid);
    // Not just `file.is_none()`: the parser recovers, so a fixture using a
    // reserved word as an identifier yields a partial file with zero
    // functions and a codegen result that looks deceptively clean.
    assert!(perrs.is_empty(), "fixture failed to parse: {perrs:?}");
    let file = file.expect("a fixture that parses cleanly yields a file");
    let (res, _) = resolve(file, sid);
    let (typed, _) = typecheck(res, sid);
    let (checked, _) = analyze_privacy(typed, sid);
    let (module, _) = build_ir(checked, sid);
    let (with_std, _) = lower_stdlib(module, StdlibConfig::default());
    let (optimized, _) = optimize(with_std, OptimizerConfig::default());
    codegen_evm(optimized, EvmConfig::default())
}

/// Codegen diagnostics only.
pub fn diags(src: &str) -> Vec<Diagnostic> {
    compile(src).1
}

/// `DiagCode` carries only the number, and this crate reuses each number
/// across the E and W namespaces (E501/W501, E530/W530), so every assertion
/// has to pin the level too.
pub fn has_error(ds: &[Diagnostic], code: covenant_diag::DiagCode) -> bool {
    ds.iter()
        .any(|d| d.code == code && d.level == covenant_diag::DiagnosticLevel::Error)
}

pub fn has_warning(ds: &[Diagnostic], code: covenant_diag::DiagCode) -> bool {
    ds.iter()
        .any(|d| d.code == code && d.level == covenant_diag::DiagnosticLevel::Warning)
}

/// A deployed fixture on a fresh chain.
pub struct Deployed {
    pub chain: Chain,
    pub addr: Address,
    pub deployer: Address,
    pub alice: Address,
}

/// Compile, assert codegen raised no error, deploy. Use for fixtures whose
/// point is the runtime behaviour rather than a diagnostic.
pub fn deploy(src: &str) -> Deployed {
    let (artifact, ds) = compile(src);
    let errs: Vec<_> = ds
        .iter()
        .filter(|d| d.level == covenant_diag::DiagnosticLevel::Error)
        .collect();
    assert!(errs.is_empty(), "fixture must compile clean: {errs:?}");
    deploy_artifact(&artifact)
}

pub fn deploy_artifact(artifact: &EvmArtifact) -> Deployed {
    let mut chain = Chain::with_prefunded_accounts();
    let accounts: Vec<Address> = chain.accounts.keys().copied().collect();
    let deployer = accounts[0];
    let alice = accounts[1];
    let receipt = chain.deploy(deployer, &artifact.deploy_bytecode, &[]);
    assert!(
        matches!(receipt.status, TxStatus::Success),
        "deploy failed: {:?}",
        receipt.status
    );
    let addr = receipt.to.expect("deploy yields an address");
    Deployed {
        chain,
        addr,
        deployer,
        alice,
    }
}

impl Deployed {
    pub fn send(&mut self, from: Address, sig: &str, args: &[U256]) -> TxReceipt {
        let data = abi::encode_call(sig, args);
        self.chain.call(from, self.addr, &data, U256::ZERO)
    }

    /// Send and require success.
    pub fn send_ok(&mut self, from: Address, sig: &str, args: &[U256]) -> TxReceipt {
        let r = self.send(from, sig, args);
        assert!(
            matches!(r.status, TxStatus::Success),
            "`{sig}` must succeed, got {:?}",
            r.status
        );
        r
    }

    /// Send and require a revert.
    pub fn send_reverts(&mut self, from: Address, sig: &str, args: &[U256]) -> TxReceipt {
        let r = self.send(from, sig, args);
        assert!(
            !matches!(r.status, TxStatus::Success),
            "`{sig}` must revert, but it succeeded"
        );
        r
    }

    pub fn view_u256(&mut self, sig: &str, args: &[U256]) -> U256 {
        let from = self.deployer;
        let r = self.send_ok(from, sig, args);
        decode_word(&r.return_data)
    }

    pub fn storage(&self, slot: U256) -> U256 {
        self.chain.get_storage(self.addr, slot)
    }
}

impl Deployed {
    /// Install hand-written runtime bytecode at `addr`, so a fixture can call
    /// out to a callee with chosen behaviour (a gate that returns a short
    /// word, an oversized word, nothing at all).
    pub fn install_code(&mut self, addr: Address, runtime: &[u8]) {
        self.chain.contracts.insert(
            addr,
            covenant_evm_runtime::Contract {
                address: addr,
                deployer: self.deployer,
                deploy_bytecode: Vec::new(),
                runtime_bytecode: runtime.to_vec(),
                storage: std::collections::BTreeMap::new(),
                code_hash: [0u8; 32],
                deployed_at_block: 1,
                deployed_at_timestamp: 0,
                label: None,
            },
        );
    }

    /// Call with raw calldata, bypassing `encode_call`. Needed to plant a
    /// non-canonical argument word that no conformant encoder emits.
    pub fn send_raw(&mut self, from: Address, calldata: &[u8]) -> TxReceipt {
        self.chain.call(from, self.addr, calldata, U256::ZERO)
    }

    /// Call with attached value.
    pub fn send_value(
        &mut self,
        from: Address,
        sig: &str,
        args: &[U256],
        value: U256,
    ) -> TxReceipt {
        let data = abi::encode_call(sig, args);
        self.chain.call(from, self.addr, &data, value)
    }
}

/// A selector followed by raw 32-byte words, so a test can encode an argument
/// the ABI forbids.
pub fn raw_call(sig: &str, words: &[[u8; 32]]) -> Vec<u8> {
    let mut out = abi::selector(sig).to_vec();
    for w in words {
        out.extend_from_slice(w);
    }
    out
}

pub fn word(v: u64) -> [u8; 32] {
    U256::from_u64(v).to_be_bytes()
}

/// `TxReceipt::return_data` is a `0x`-prefixed hex string.
pub fn decode_word(hex_str: &str) -> U256 {
    let raw = hex::decode(hex_str.trim_start_matches("0x")).expect("return data is hex");
    abi::decode_u256(&raw)
}

pub fn u(v: u64) -> U256 {
    U256::from_u64(v)
}

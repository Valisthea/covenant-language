//! Per-target precompile address resolution (Sprint 31, V0.9 Phase A.1).
//!
//! In V0.8, the compiler emitted hardcoded `CALL 0x101` (FHE add), `CALL 0x123`
//! (ceremony destroy), etc. MockChain implemented those addresses as
//! precompiles; Sepolia returned empty, silently breaking those constructs
//! (audit finding KSR-CVN-005).
//!
//! V0.9 introduces per-target address resolution. Each `Target` variant maps
//! to a `PrecompileAddresses` configuration. For MockChain the V0.8 addresses
//! are preserved (backward compat). For Sepolia, the addresses point to the
//! deployed helper contracts captured in `config/helper-addresses-v0.9.0.json`.
//!
//! Architecture: see `docs/v0.9/precompile-bridge-architecture.md`
//! Resolution:   see `docs/v0.9/address-resolution.md`
//! Helpers:      see `docs/v0.9/helper-interfaces.md`
//!
//! ## Why hardcoded constants instead of include_str!(JSON)
//!
//! The JSON registry at `config/helper-addresses-v0.9.0.json` is the source
//! of truth for *external* consumers (playground, third-party tooling, audit
//! firms). The Rust constants in this module *mirror* the JSON. A test (see
//! `tests/registry_consistency.rs`) verifies the mirror stays in sync.
//!
//! Reasons to mirror rather than parse at compile time:
//!   - Avoids pulling `serde` + `serde_json` into `covenant-evm-backend`,
//!     which would slow downstream compile times for every consumer.
//!   - No runtime I/O in the hot compile path.
//!   - Addresses are visible in `git grep` searches.
//!   - If the JSON drifts from Rust, CI fails loudly, drift is impossible
//!     silently.

/// 20-byte EVM address. Used in `PrecompileAddresses` to identify either a
/// V0.8-style precompile (low 2 bytes = address, high 18 bytes = 0) or a
/// V0.9 deployed helper contract (full 20 bytes).
pub type EvmAddress = [u8; 20];

/// Chain target for precompile address resolution.
///
/// Distinct from the *backend* selector (EVM bytecode vs. WASM vs. Aster
/// native). All `Target` variants produce EVM bytecode; they differ only in
/// which addresses the bytecode CALLs into for cryptographic primitives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Target {
    /// In-tab MockChain (the playground's deterministic in-memory EVM).
    /// Uses the V0.8 precompile addresses (`0x101`+, `0x120`+, etc.).
    /// MockChain implements these as native precompiles in `MockPrecompileState`.
    MockChain,

    /// Ethereum Sepolia testnet (chain id 11155111).
    /// Uses the V0.9 helper contract addresses captured in
    /// `config/helper-addresses-v0.9.0.json`. Helpers are deployed by
    /// Sprint 30 via CREATE2 from the Arachnid factory at
    /// `0x4e59b44847b379578588920cA78FbF26c0B4956C`.
    Sepolia,

    /// Aster Chain testnet (chain id 1996).
    /// Helper deployment coordinated by Sprint 42-43. As of V0.9.0 release,
    /// uses the same predicted CREATE2 addresses as Sepolia (Arachnid factory
    /// is portable across EVM chains). Sprint 42 verifies and may override.
    AsterTestnet,
}

impl Target {
    /// Parse a target name from a CLI flag or covenant.toml string.
    ///
    /// Accepts case-insensitive forms. Mainnet is rejected at the language
    /// layer: V0.9 ships testnet-only, mainnet helpers land in V1.0
    /// post-external-audit.
    pub fn parse(s: &str) -> Result<Self, TargetParseError> {
        match s.to_ascii_lowercase().as_str() {
            "mockchain" | "mock" => Ok(Target::MockChain),
            "sepolia" => Ok(Target::Sepolia),
            "aster_testnet" | "aster-testnet" => Ok(Target::AsterTestnet),
            // `evm` used to alias MockChain, which reads as "generic EVM" but is
            // not: MockChain emits calls to short mock precompile addresses that
            // hold no code on any real network. A cryptographic construct built
            // that way compiles clean and then fails at runtime on chain. The
            // alias is refused rather than silently meaning something else.
            "evm" | "generic" | "generic_evm" | "generic-evm" => {
                Err(TargetParseError::NoGenericEvmTarget)
            }
            "mainnet" | "ethereum" | "ethereum_mainnet" => Err(TargetParseError::MainnetForbidden),
            other => Err(TargetParseError::Unknown(other.to_string())),
        }
    }

    /// Stable string representation. Used in JSON registry keys, log lines,
    /// and round-trips through `parse`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Target::MockChain => "mockchain",
            Target::Sepolia => "sepolia",
            Target::AsterTestnet => "aster_testnet",
        }
    }

    /// Numeric chain id of the target's settlement chain.
    pub fn chain_id(&self) -> u64 {
        match self {
            Target::MockChain => 31337,
            Target::Sepolia => 11155111,
            Target::AsterTestnet => 1996,
        }
    }

    /// Returns true if the target uses deployed helper contracts (rather
    /// than native precompiles like MockChain does).
    pub fn uses_helper_contracts(&self) -> bool {
        !matches!(self, Target::MockChain)
    }

    /// True when the helper contracts this target points at are known to be
    /// deployed at those addresses.
    ///
    /// Sepolia is verified: all four helpers were deployed on 2026-04-26 and
    /// answer `eth_getCode`. AsterTestnet is not. It reuses Sepolia's
    /// predicted CREATE2 addresses on the assumption that the Arachnid factory
    /// is deployed on that chain, which was left to a sprint that never ran:
    /// the address manifest still carries `"helpers": null` for it. Compiling
    /// a helper call for an unverified target is the same defect as the
    /// removed `evm` alias, so codegen refuses it rather than shipping a
    /// contract that reverts on first use.
    pub fn helpers_verified_deployed(&self) -> bool {
        match self {
            // Native precompiles, nothing to deploy.
            Target::MockChain => true,
            Target::Sepolia => true,
            Target::AsterTestnet => false,
        }
    }
}

/// Errors from parsing a target name.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TargetParseError {
    #[error(
        "V0.9 ships testnet-only. Mainnet helpers land in V1.0 after external audit. \
         Use --target-chain={{mockchain,sepolia,aster_testnet}} instead."
    )]
    MainnetForbidden,
    #[error(
        "there is no generic EVM target. `evm` previously aliased the local mock chain, \
         which emits calls to mock precompile addresses that hold no code on any real \
         network, so a contract using the cryptographic constructs would compile clean \
         and then revert on chain. Use --target-chain=mockchain for local execution, or \
         a named chain target whose helper contracts are actually deployed. Contracts \
         that use no cryptographic construct emit no chain-specific address at all, so \
         their bytecode is already portable to any EVM chain without a target."
    )]
    NoGenericEvmTarget,
    #[error("unknown target '{0}' (valid: mockchain, sepolia, aster_testnet)")]
    Unknown(String),
}

// ─── V0.9.0 Sepolia / AsterTestnet hardcoded addresses ──────────────────────
//
// These mirror config/helper-addresses-v0.9.0.json. ANY change must update
// both files in lockstep: see tests/registry_consistency.rs.

/// V0.9.1 CeremonyHelper: adds the `amnesiaSetup(uint256)` 1-arg overload
/// the Covenant compiler needs (V0.9.0's 3-arg-only signature mismatched
/// the V0.8 `Opcode::AmnesiaBegin` operand count = 1, causing Solidity's
/// `calldatasize` dispatch check to revert when called from Covenant
/// bytecode). V0.9.0 helper at `0x6cABDD5Acf86D43C30CE560be68780E62F78A16e`
/// is still deployed and callable directly via cast for diagnostics.
///
/// V0.9.1 deploy: 2026-04-26, tx `0x55cabe6230a2cef1424f3e6ea1717541501e48ad917ddabb1c1c862bd5a85b62`.
pub const CEREMONY_HELPER_V090: EvmAddress =
    addr_from_hex("627f1Ff6Dc93AEba050c242FD9E26961E8F6c6F0");
/// CREATE2-predicted address of `MockedFHEHelper` (V0.9.0).
pub const FHE_HELPER_V090: EvmAddress = addr_from_hex("8f38e4F079570D77900fF2d6FfD0e6c96c401E44");
/// CREATE2-predicted address of `MockedPQVerifier` (V0.9.0).
pub const PQ_HELPER_V090: EvmAddress = addr_from_hex("D3FcA4d62dc2162d9b07DEC2aCFc0A4Bda2A9010");
/// CREATE2-predicted address of `MockedZKVerifier` (V0.9.0).
pub const ZK_HELPER_V090: EvmAddress = addr_from_hex("a9910bce5A3A47D0a92441C63D1e555A6CD7513c");

/// Lift a V0.8-style 16-bit precompile address (e.g. `0x101`) into a
/// 20-byte EVM address with the low 2 bytes set and high 18 bytes zero.
pub const fn lift_v08_addr(low: u16) -> EvmAddress {
    let mut out = [0u8; 20];
    out[18] = (low >> 8) as u8;
    out[19] = (low & 0xFF) as u8;
    out
}

const fn addr_from_hex(hex: &str) -> EvmAddress {
    let bytes = hex.as_bytes();
    assert!(bytes.len() == 40, "address hex must be 40 chars");
    let mut out = [0u8; 20];
    let mut i = 0;
    while i < 20 {
        let hi = hex_nibble(bytes[i * 2]);
        let lo = hex_nibble(bytes[i * 2 + 1]);
        out[i] = (hi << 4) | lo;
        i += 1;
    }
    out
}

const fn hex_nibble(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => panic!("invalid hex nibble"),
    }
}

/// Translate a V0.8 namespaced precompile opcode name (e.g. `"AmnesiaBegin"`)
/// to the Solidity ABI selector of the corresponding helper-contract
/// method when the target uses helper contracts.
///
/// Returns `None` if the opcode has no helper-contract counterpart (caller
/// should fall back to V0.8 namespaced `precompile_selector(...)`).
///
/// Mirror of `config/helper-addresses-v0.9.0.json::targets.sepolia.selectors`.
/// CI consistency test in `tests/registry_consistency.rs`.
/// What a helper method puts in returndata, and therefore how the call site
/// is allowed to read it.
///
/// The backend used to apply one rule to all of them: on a helper target,
/// accept any returndata of at least 32 bytes and read `MLOAD(0)`. That is
/// correct for a method returning a single word and wrong for anything else.
/// `amnesiaDestroy` returns Solidity dynamic `bytes`, whose first word is the
/// ABI offset, so the caller read `0x20` and used it as the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnShape {
    /// Exactly one 32-byte word, read as an opaque value. `bytes32`,
    /// `uint256`, `address`.
    Word,
    /// One word that the ABI says is a `bool`, so it must be 0 or 1. Read
    /// like a word, but a non-canonical value means the callee is not the
    /// contract we think it is.
    Bool,
    /// Solidity dynamic `bytes`: `offset || length || data`. There is no
    /// decoder for this at the call site, so an opcode declaring it cannot
    /// be lowered.
    DynamicBytes,
}

/// Whether the call may modify state, which decides `CALL` against
/// `STATICCALL`.
///
/// Every helper-target call used to be a `CALL`, including the five methods
/// the helpers declare `view` or `pure`. That handed read-only operations the
/// authority to write, which nothing in their semantics needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallMode {
    /// The method changes helper state.
    Call,
    /// The method is `view` or `pure` in the deployed helper.
    StaticCall,
}

/// Everything the call site needs to know about one helper method.
#[derive(Debug, Clone, Copy)]
pub struct HelperMethod {
    /// Canonical Solidity signature. The selector is derived from this, and
    /// `tests/selector_recomputation.rs` proves the derivation.
    pub signature: &'static str,
    pub selector: [u8; 4],
    pub returns: ReturnShape,
    pub mode: CallMode,
}

/// The single table every helper call goes through.
///
/// Adding an opcode without an entry here means it cannot be lowered on a
/// helper target, which is the intended failure: a new primitive has to
/// declare its signature, its return shape and whether it writes before it
/// can reach a chain.
pub fn helper_method_for_opcode(opcode_name: &str) -> Option<HelperMethod> {
    use CallMode::{Call, StaticCall};
    use ReturnShape::{Bool, DynamicBytes, Word};

    let (signature, returns, mode) = match opcode_name {
        // CeremonyHelper.sol V0.9.1. All four change session state.
        "AmnesiaBegin" => ("amnesiaSetup(uint256)", Word, Call),
        "AmnesiaSubmitShare" => ("amnesiaSubmitShare(uint256,bytes32)", Bool, Call),
        "AmnesiaFinalize" => ("amnesiaFinalize(uint256)", Bool, Call),
        // Returns `bytes memory`, so it cannot be read as a word. Declaring
        // the real shape is what makes the backend refuse it instead of
        // reading the ABI offset as a result.
        "DestructionProof" => ("amnesiaDestroy(uint256)", DynamicBytes, Call),

        // MockedFHEHelper.sol. The mocked implementations write to storage,
        // so these are genuinely state-changing even though real FHE would
        // not be.
        "FheEncryptTrivial" => ("encryptTrivial(uint256)", Word, Call),
        "FheEncryptFresh" => ("encryptFresh(uint256,uint256)", Word, Call),
        "FheAdd" => ("add(bytes32,bytes32)", Word, Call),
        "FheSub" => ("sub(bytes32,bytes32)", Word, Call),
        "FheMul" => ("mul(bytes32,bytes32)", Word, Call),
        "FheCmpEq" => ("eq(bytes32,bytes32)", Word, Call),
        "FheCmpLt" => ("lt(bytes32,bytes32)", Word, Call),
        "FheSelect" => ("cmux(bytes32,bytes32,bytes32)", Word, Call),
        // `view` in the helper.
        "RevealDecrypt" => ("decrypt(bytes32,address)", Word, StaticCall),

        // MockedZKVerifier.sol, both read-only.
        "ZkVerify" => ("verify(bytes32,bytes,bytes)", Bool, StaticCall),
        "ZkNullifier" => ("nullifier(bytes32)", Word, StaticCall),

        // MockedPQVerifier.sol, both read-only.
        "PqVerifyDilithium" => ("pqVerify(bytes32,bytes,bytes)", Bool, StaticCall),
        "PqRand" => ("pqRandom(uint256)", Word, StaticCall),

        _ => return None,
    };

    Some(HelperMethod {
        signature,
        selector: crate::abi::selector(signature),
        returns,
        mode,
    })
}

pub fn helper_selector_for_opcode(opcode_name: &str) -> Option<[u8; 4]> {
    helper_method_for_opcode(opcode_name).map(|m| m.selector)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_roundtrip() {
        for t in [Target::MockChain, Target::Sepolia, Target::AsterTestnet] {
            let parsed = Target::parse(t.as_str()).unwrap();
            assert_eq!(parsed, t);
        }
    }

    #[test]
    fn parse_case_insensitive() {
        assert_eq!(Target::parse("SEPOLIA").unwrap(), Target::Sepolia);
        assert_eq!(Target::parse("MockChain").unwrap(), Target::MockChain);
    }

    #[test]
    fn parse_aliases() {
        // `evm` is deliberately NOT an alias for MockChain: it reads as "generic
        // EVM" but MockChain emits mock precompile addresses that are empty on
        // every real chain. Refused with a dedicated error instead.
        assert_eq!(
            Target::parse("evm").unwrap_err(),
            TargetParseError::NoGenericEvmTarget
        );
        assert_eq!(Target::parse("mock").unwrap(), Target::MockChain);
        assert_eq!(
            Target::parse("aster-testnet").unwrap(),
            Target::AsterTestnet
        );
    }

    #[test]
    fn mainnet_rejected() {
        let err = Target::parse("mainnet").unwrap_err();
        assert!(matches!(err, TargetParseError::MainnetForbidden));
        let err2 = Target::parse("ethereum").unwrap_err();
        assert!(matches!(err2, TargetParseError::MainnetForbidden));
    }

    #[test]
    fn unknown_target_errors_with_name() {
        let err = Target::parse("polygon").unwrap_err();
        match err {
            TargetParseError::Unknown(name) => assert_eq!(name, "polygon"),
            _ => panic!("expected Unknown"),
        }
    }

    #[test]
    fn chain_ids_correct() {
        assert_eq!(Target::MockChain.chain_id(), 31337);
        assert_eq!(Target::Sepolia.chain_id(), 11155111);
        assert_eq!(Target::AsterTestnet.chain_id(), 1996);
    }

    #[test]
    fn lift_v08_preserves_low_bytes() {
        let lifted = lift_v08_addr(0x123);
        assert_eq!(lifted[18], 0x01);
        assert_eq!(lifted[19], 0x23);
        // High 18 bytes all zero
        for byte in &lifted[..18] {
            assert_eq!(*byte, 0);
        }
    }

    #[test]
    fn helper_addresses_first_byte_check() {
        assert_eq!(CEREMONY_HELPER_V090[0], 0x62); // V0.9.1 patched address
        assert_eq!(FHE_HELPER_V090[0], 0x8f);
        assert_eq!(PQ_HELPER_V090[0], 0xD3);
        assert_eq!(ZK_HELPER_V090[0], 0xa9);
    }
}

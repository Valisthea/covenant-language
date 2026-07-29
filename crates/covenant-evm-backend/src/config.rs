//! EVM backend configuration.

use crate::target::{lift_v08_addr, EvmAddress, Target};
use crate::target::{CEREMONY_HELPER_V090, FHE_HELPER_V090, PQ_HELPER_V090, ZK_HELPER_V090};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvmVersion {
    Shanghai,
    Cancun,
    Prague,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocatorStrategy {
    /// Every SSA value lives in its own memory slot (slow but straightforward).
    MemoryMapped,
}

#[derive(Debug, Clone)]
pub struct EvmConfig {
    pub evm_version: EvmVersion,
    pub emit_source_map: bool,
    /// Emit `test_*` actions into the ABI, selector table and dispatcher.
    /// FALSE for any release build: a test that mutates would otherwise be a
    /// public, unguarded state mutator on the deployed contract. The
    /// `covenant test` harness sets it true so it can call them.
    pub include_test_actions: bool,
    pub validate_output: bool,
    pub precompile_addresses: PrecompileAddresses,
    pub max_runtime_size: u32,
    pub allocator: AllocatorStrategy,
    /// V0.9 Sprint 31.b: chain target. Drives both precompile address
    /// selection AND selector encoding (MockChain uses V0.8 namespaced
    /// selectors `keccak("covenant.precompile.<op>:v1")`; Sepolia /
    /// AsterTestnet use the Solidity ABI selectors of the deployed
    /// helper contracts.)
    pub target: Target,
}

impl Default for EvmConfig {
    fn default() -> Self {
        Self {
            evm_version: EvmVersion::Shanghai,
            emit_source_map: true,
            // Release default: strip test actions. The `covenant test` harness
            // flips this on.
            include_test_actions: false,
            validate_output: cfg!(debug_assertions),
            precompile_addresses: PrecompileAddresses::default(),
            max_runtime_size: 24 * 1024,
            allocator: AllocatorStrategy::MemoryMapped,
            target: Target::MockChain,
        }
    }
}

impl EvmConfig {
    /// Build an `EvmConfig` for the given chain target. Identical to
    /// `Default::default()` other than `precompile_addresses` and
    /// `target`.
    pub fn for_target(target: Target) -> Self {
        Self {
            precompile_addresses: PrecompileAddresses::for_target(target),
            target,
            ..Self::default()
        }
    }
}

/// Resolved precompile / helper addresses, full 20-byte form.
///
/// V0.8 used `u16` because every precompile lived in `0x100`-`0x1FF`. V0.9
/// added Sepolia and Aster targets where helpers live at full 20-byte
/// CREATE2 addresses, so the field type is `EvmAddress = [u8; 20]`. V0.8
/// MockChain addresses are lifted via [`lift_v08_addr`].
#[derive(Debug, Clone, Copy)]
pub struct PrecompileAddresses {
    pub fhe: FhePrecompiles,
    pub zk: ZkPrecompiles,
    pub pq: PqPrecompiles,
    pub amnesia: AmnesiaPrecompiles,
}

#[derive(Debug, Clone, Copy)]
pub struct FhePrecompiles {
    pub encrypt_trivial: EvmAddress,
    pub encrypt_fresh: EvmAddress,
    pub add: EvmAddress,
    pub sub: EvmAddress,
    pub mul: EvmAddress,
    pub cmp_eq: EvmAddress,
    pub cmp_ne: EvmAddress,
    pub cmp_lt: EvmAddress,
    pub cmp_le: EvmAddress,
    pub cmp_gt: EvmAddress,
    pub cmp_ge: EvmAddress,
    pub and: EvmAddress,
    pub or: EvmAddress,
    pub not: EvmAddress,
    pub select: EvmAddress,
    pub ciphertext_hash: EvmAddress,
    pub reveal_decrypt: EvmAddress,
    pub bootstrap: EvmAddress,
    /// Threshold-decrypt a ciphertext<bool> → 1 byte (0 or 1). Used by AssertEncrypted.
    pub threshold_decrypt_bool: EvmAddress,
    /// Threshold-decrypt a ciphertext<uint256> → 32 bytes. Used by reveal-winner paths.
    pub threshold_decrypt_uint256: EvmAddress,
}

#[derive(Debug, Clone, Copy)]
pub struct ZkPrecompiles {
    pub verify: EvmAddress,
    pub nullifier: EvmAddress,
    pub vdf_eval: EvmAddress,
    pub vdf_verify: EvmAddress,
}

#[derive(Debug, Clone, Copy)]
pub struct PqPrecompiles {
    pub verify_dilithium: EvmAddress,
    pub hybrid_verify: EvmAddress,
    pub rand_pq: EvmAddress,
    pub kyber_encrypt: EvmAddress,
    pub kyber_decrypt: EvmAddress,
}

#[derive(Debug, Clone, Copy)]
pub struct AmnesiaPrecompiles {
    pub begin: EvmAddress,
    pub submit_share: EvmAddress,
    pub finalize: EvmAddress,
    pub destruction_proof: EvmAddress,
}

impl PrecompileAddresses {
    /// V0.8 / MockChain layout: precompiles at `0x101`+, `0x120`+, etc.
    /// Lifted to 20-byte addresses with high 18 bytes zero.
    pub fn mock_chain() -> Self {
        Self {
            fhe: FhePrecompiles {
                encrypt_trivial: lift_v08_addr(0x101),
                encrypt_fresh: lift_v08_addr(0x102),
                add: lift_v08_addr(0x103),
                sub: lift_v08_addr(0x104),
                mul: lift_v08_addr(0x105),
                cmp_eq: lift_v08_addr(0x106),
                cmp_ne: lift_v08_addr(0x113),
                cmp_lt: lift_v08_addr(0x107),
                cmp_le: lift_v08_addr(0x114),
                cmp_gt: lift_v08_addr(0x108),
                cmp_ge: lift_v08_addr(0x112),
                and: lift_v08_addr(0x109),
                or: lift_v08_addr(0x10A),
                not: lift_v08_addr(0x10B),
                select: lift_v08_addr(0x10C),
                ciphertext_hash: lift_v08_addr(0x10D),
                reveal_decrypt: lift_v08_addr(0x10E),
                bootstrap: lift_v08_addr(0x10F),
                threshold_decrypt_bool: lift_v08_addr(0x110),
                threshold_decrypt_uint256: lift_v08_addr(0x111),
            },
            zk: ZkPrecompiles {
                verify: lift_v08_addr(0x130),
                nullifier: lift_v08_addr(0x131),
                vdf_eval: lift_v08_addr(0x132),
                vdf_verify: lift_v08_addr(0x133),
            },
            pq: PqPrecompiles {
                verify_dilithium: lift_v08_addr(0x150),
                hybrid_verify: lift_v08_addr(0x151),
                rand_pq: lift_v08_addr(0x152),
                kyber_encrypt: lift_v08_addr(0x153),
                kyber_decrypt: lift_v08_addr(0x154),
            },
            amnesia: AmnesiaPrecompiles {
                begin: lift_v08_addr(0x120),
                submit_share: lift_v08_addr(0x121),
                finalize: lift_v08_addr(0x122),
                destruction_proof: lift_v08_addr(0x123),
            },
        }
    }

    /// V0.9.0 helper-contract layout for Sepolia / AsterTestnet.
    ///
    /// All FHE methods route to the single deployed `MockedFHEHelper`
    /// contract; the EVM CALL target encodes only WHICH helper, while the
    /// 4-byte selector (constructed in `emit_precompile_call`) encodes the
    /// METHOD on that helper.
    pub fn helpers_v0_9_0() -> Self {
        Self {
            fhe: FhePrecompiles {
                encrypt_trivial: FHE_HELPER_V090,
                encrypt_fresh: FHE_HELPER_V090,
                add: FHE_HELPER_V090,
                sub: FHE_HELPER_V090,
                mul: FHE_HELPER_V090,
                cmp_eq: FHE_HELPER_V090,
                cmp_ne: FHE_HELPER_V090,
                cmp_lt: FHE_HELPER_V090,
                cmp_le: FHE_HELPER_V090,
                cmp_gt: FHE_HELPER_V090,
                cmp_ge: FHE_HELPER_V090,
                and: FHE_HELPER_V090,
                or: FHE_HELPER_V090,
                not: FHE_HELPER_V090,
                select: FHE_HELPER_V090,
                ciphertext_hash: FHE_HELPER_V090,
                reveal_decrypt: FHE_HELPER_V090,
                bootstrap: FHE_HELPER_V090,
                threshold_decrypt_bool: FHE_HELPER_V090,
                threshold_decrypt_uint256: FHE_HELPER_V090,
            },
            zk: ZkPrecompiles {
                verify: ZK_HELPER_V090,
                nullifier: ZK_HELPER_V090,
                vdf_eval: ZK_HELPER_V090,
                vdf_verify: ZK_HELPER_V090,
            },
            pq: PqPrecompiles {
                verify_dilithium: PQ_HELPER_V090,
                hybrid_verify: PQ_HELPER_V090,
                rand_pq: PQ_HELPER_V090,
                kyber_encrypt: PQ_HELPER_V090,
                kyber_decrypt: PQ_HELPER_V090,
            },
            amnesia: AmnesiaPrecompiles {
                begin: CEREMONY_HELPER_V090,
                submit_share: CEREMONY_HELPER_V090,
                finalize: CEREMONY_HELPER_V090,
                destruction_proof: CEREMONY_HELPER_V090,
            },
        }
    }

    /// Resolve precompile addresses for a given target.
    pub fn for_target(target: Target) -> Self {
        match target {
            Target::MockChain => Self::mock_chain(),
            Target::Sepolia | Target::AsterTestnet => Self::helpers_v0_9_0(),
        }
    }
}

impl Default for PrecompileAddresses {
    fn default() -> Self {
        // V0.8 default = MockChain. Backward compatible with all V0.8 callers.
        Self::mock_chain()
    }
}

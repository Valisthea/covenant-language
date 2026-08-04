//! OMEGA V6 (HGH-031): detects which FHE/PQ/ZK primitive categories a
//! compiled module's bytecode routes to a `Mocked*.sol` helper contract on
//! real-chain targets (Sepolia / AsterTestnet).
//!
//! This scans the IR directly (ground truth of what codegen actually
//! lowers), rather than grepping source text, so it can't miss a primitive
//! reached indirectly (e.g. via stdlib synthesis) or false-positive on one
//! that only appears in a comment.
//!
//! The amnesia/ceremony category was originally EXCLUDED here, on the
//! reasoning that `CeremonyHelper.sol` implements a real if simplified
//! deterministic commitment rather than the coin-flip-parity-check class of
//! stub used for FHE/PQ/ZK, and that its source carries no "PLACEHOLDER, NOT
//! FOR PRODUCTION SECRETS" label, so conflating the two would misrepresent its
//! maturity.
//!
//! That exclusion was wrong for the question this field exists to answer. A
//! `ceremony` contract's V0.9 destruction path does not make anything
//! unrecoverable: the VDF, the Shamir split and the destruction proof are
//! deterministic stubs, and the "destroyed" secret remains readable from chain
//! state. `CeremonyHelper.sol` is also the one helper with no `notMainnet`
//! chain-id gate. A reader gating a pipeline on an empty
//! `mockedCryptoPrimitives` would therefore have concluded that a ceremony
//! contract depends on no mocked cryptography, which is the opposite of the
//! truth. It is now reported, under its own category, so the difference in
//! maturity between `CeremonyHelper` and the `Mocked*` stubs stays visible
//! instead of being erased in either direction.

use covenant_ir::{IrModule, Opcode};

/// One mocked-crypto category and the helper contract its precompile calls
/// route to on a real-chain target.
pub struct MockedCryptoUsage {
    pub category: &'static str,
    pub helper_contract: &'static str,
}

/// Scan every instruction in every function for opcodes that
/// `covenant-evm-backend`'s codegen lowers to a `CALL`/`STATICCALL` against
/// one of the `Mocked*.sol` helper contracts (see `PrecompileAddresses::helpers_v0_9_0`).
/// Returns one entry per distinct category actually used, in a fixed order.
pub fn detect_mocked_crypto_usage(module: &IrModule) -> Vec<MockedCryptoUsage> {
    let mut fhe = false;
    let mut zk = false;
    let mut pq = false;
    let mut amnesia = false;

    for func in &module.functions {
        for block in &func.blocks {
            for instr in &block.instructions {
                match &instr.opcode {
                    Opcode::FheEncryptTrivial
                    | Opcode::FheEncryptFresh
                    | Opcode::FheAdd
                    | Opcode::FheSub
                    | Opcode::FheMul
                    | Opcode::FheCmpEq
                    | Opcode::FheCmpNe
                    | Opcode::FheCmpLt
                    | Opcode::FheCmpLe
                    | Opcode::FheCmpGt
                    | Opcode::FheCmpGe
                    | Opcode::FheAnd
                    | Opcode::FheOr
                    | Opcode::FheNot
                    | Opcode::FheSelect
                    | Opcode::FheBootstrap
                    | Opcode::FheCiphertextHash
                    | Opcode::RevealDecrypt => fhe = true,
                    Opcode::ZkVerify
                    | Opcode::ZkNullifier
                    | Opcode::VdfEval
                    | Opcode::VdfVerify => zk = true,
                    Opcode::PqVerifyDilithium
                    | Opcode::PqHybridVerify
                    | Opcode::PqRand
                    | Opcode::KyberEncrypt
                    | Opcode::KyberDecrypt => pq = true,
                    Opcode::AmnesiaBegin
                    | Opcode::AmnesiaSubmitShare
                    | Opcode::AmnesiaFinalize
                    | Opcode::ShamirSplit
                    | Opcode::ShamirReconstruct
                    | Opcode::VdfLock
                    | Opcode::VdfUnlock
                    | Opcode::DestructionCommitment => amnesia = true,
                    _ => {}
                }
            }
        }
    }

    let mut out = Vec::new();
    if fhe {
        out.push(MockedCryptoUsage {
            category: "fhe",
            helper_contract: "MockedFHEHelper",
        });
    }
    if zk {
        out.push(MockedCryptoUsage {
            category: "zk",
            helper_contract: "MockedZKVerifier",
        });
    }
    if pq {
        out.push(MockedCryptoUsage {
            category: "pq",
            helper_contract: "MockedPQVerifier",
        });
    }
    if amnesia {
        out.push(MockedCryptoUsage {
            category: "amnesia",
            helper_contract: "CeremonyHelper",
        });
    }
    out
}

/// Map a `mocked_crypto_primitives` category string back to its helper
/// contract name, for callers (the CLI) that only have the category string
/// out of `CompilationMetadata`.
pub fn helper_contract_for_category(category: &str) -> &'static str {
    match category {
        "fhe" => "MockedFHEHelper",
        "zk" => "MockedZKVerifier",
        "pq" => "MockedPQVerifier",
        "amnesia" => "CeremonyHelper",
        _ => "an unknown mocked-crypto helper",
    }
}

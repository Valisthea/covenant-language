//! OMEGA V6 (HGH-031): detects which FHE/PQ/ZK primitive categories a
//! compiled module's bytecode routes to a `Mocked*.sol` helper contract on
//! real-chain targets (Sepolia / AsterTestnet).
//!
//! Deliberately excludes the amnesia/ceremony precompiles (`AmnesiaBegin`
//! etc., routed to `CeremonyHelper.sol`): that contract's V0.9 destruction
//! proof is a real (if simplified) deterministic commitment, not the
//! coin-flip-parity-check / open-access-plaintext-store class of mock the
//! audit's PoC demonstrated for FHE/PQ/ZK, and its source carries no
//! "PLACEHOLDER — NOT FOR PRODUCTION SECRETS" label. Conflating the two
//! would misrepresent CeremonyHelper's actual maturity.
//!
//! This scans the IR directly (ground truth of what codegen actually
//! lowers), rather than grepping source text, so it can't miss a primitive
//! reached indirectly (e.g. via stdlib synthesis) or false-positive on one
//! that only appears in a comment.

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
        _ => "an unknown mocked-crypto helper",
    }
}

//! KSR-CVN-016 / KSR-CVN-017 regression: `Opcode::has_randomness_or_state`
//! must return true for every opcode that carries randomness or hidden state,
//! and false for pure deterministic primitives. Optimizer CSE keys off this
//! predicate (covenant-opt/src/cse.rs).

use covenant_ir::Opcode;

#[test]
fn randomness_opcodes_are_marked() {
    let randomness = [Opcode::FheEncryptFresh, Opcode::PqRand, Opcode::ShamirSplit];
    for op in randomness {
        assert!(
            op.has_randomness_or_state(),
            "{op:?} carries randomness: must NOT be CSE-eligible (KSR-CVN-016/017)"
        );
    }
}

#[test]
fn stateful_opcodes_are_marked() {
    let stateful = [
        Opcode::VdfEval,
        Opcode::FheBootstrap,
        Opcode::AmnesiaBegin,
        Opcode::AmnesiaSubmitShare,
        Opcode::AmnesiaFinalize,
        Opcode::VdfLock,
        Opcode::VdfUnlock,
        Opcode::DestructionProof,
    ];
    for op in stateful {
        assert!(
            op.has_randomness_or_state(),
            "{op:?} carries hidden state: must NOT be CSE-eligible"
        );
    }
}

#[test]
fn pure_opcodes_are_not_marked() {
    let pure = [
        Opcode::Add,
        Opcode::Sub,
        Opcode::Mul,
        Opcode::Eq,
        Opcode::Lt,
        Opcode::Keccak,
        Opcode::FheEncryptTrivial, // deterministic encryption, CSE-eligible
        Opcode::FheAdd,
        Opcode::FheCmpEq,
        Opcode::FheCmpNe,
        Opcode::FheCmpLt,
        Opcode::FheCmpLe,
        Opcode::PqVerifyDilithium, // verification is deterministic
        Opcode::ZkVerify,
        Opcode::VdfVerify, // verification, not eval
    ];
    for op in pure {
        assert!(
            !op.has_randomness_or_state(),
            "{op:?} is pure/deterministic: must remain CSE-eligible"
        );
    }
}

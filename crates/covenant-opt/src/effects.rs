//! The optimizer's effect model.
//!
//! Every pass that deletes an instruction needs one shared answer to the
//! question "can this instruction go without changing what the contract does
//! on chain?". Until now that answer was an allowlist of interesting opcodes,
//! and anything absent from the list defaulted to *removable*. A default of
//! "removable" is exactly the wrong way round for a fail-loud compiler: each
//! time an opcode was forgotten the optimizer silently deleted real
//! behaviour. `ZkVerify` (KSR-CVN-026), the address computation feeding a
//! `StructSet` (OMEGA V6), and most recently the zero-divisor guard of
//! `let y = 100 / x` together with the underflow guard of
//! `let remaining = bal - amt` (OMEGA V3.6 F-19), which vanished from the
//! runtime bytecode with no diagnostic because their SSA results were never
//! read.
//!
//! So the allowlist is replaced by an exhaustive classification. Adding an
//! `Opcode` variant now fails to compile this file until somebody classifies
//! it, the same forcing function KSR-CVN-036 applied to `Terminator` in the
//! reachability walk. There is no default arm here on purpose: do not add
//! one.
//!
//! Four classes, one per reason an instruction can matter:
//!
//! * [`Effect::Pure`] the instruction's only product is its SSA result.
//!   Removable exactly when that result is dead.
//! * [`Effect::State`] writes storage or memory, emits a log, moves value, or
//!   calls out. Never removable.
//! * [`Effect::Trap`] cannot change state but CAN abort the transaction. The
//!   revert is the observable behaviour, so the instruction has to survive
//!   even when nobody reads its result. This is the class F-19 was missing.
//! * [`Effect::Critical`] neither writes state nor traps by itself, but
//!   removing it strips a security check or busts a runtime budget
//!   (KSR-CVN-026: the verification primitives and `FheBootstrap`).
//!
//! `State` and `Critical` behave identically for the eliminator (always
//! keep). They are separate variants because the *reason* differs, and the
//! reason is what a future reader has to get right when classifying a new
//! opcode.

use std::collections::HashMap;

use covenant_ir::{
    instr::{Instr, IrConstant, ValueInfo},
    IrFunction, Opcode, Value,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Result only. Removing it when the result is dead changes nothing.
    Pure,
    /// Changes chain state, emits a log, moves value, or calls out.
    State,
    /// Can abort the transaction. The revert IS the observable behaviour.
    Trap,
    /// Security- or budget-critical: no state write and no trap of its own,
    /// but its absence silently weakens the contract (KSR-CVN-026).
    Critical,
}

/// Classify an opcode. Exhaustive by design: no `_ =>` arm.
pub fn effect_of(opcode: &Opcode) -> Effect {
    use Effect::*;
    use Opcode::*;
    match opcode {
        // --- Plaintext arithmetic that wraps or is total on the EVM ---
        // `Add`/`Sub`/`Mul` are the deliberately-unchecked forms and the EVM
        // has no trap for them. Shifts, bitwise ops and comparisons are total.
        Add | Sub | Mul | SignedNeg | BitAnd | BitOr | BitXor | ShiftLeft | ShiftRight | BitNot => {
            Pure
        }
        Eq | Ne | Lt | Le | Gt | Ge => Pure,
        LogicalAnd | LogicalOr | LogicalNot => Pure,

        // --- Trapping arithmetic (OMEGA V3.6 F-19) ---
        // `Div`/`Mod` carry the zero-divisor guard the fail-loud pass added
        // (EVM DIV is total: `x / 0` yields 0 instead of trapping, so the
        // guard is the only thing that makes `100 / x` mean what the source
        // says). `AddChecked`/`SubChecked`/`MulChecked` carry the overflow
        // reverts of V0.9.2. In every case the revert is emitted by the
        // instruction itself, so deleting the instruction deletes the revert
        // and the contract keeps running past a state the source declared
        // impossible.
        Div | Mod | AddChecked | SubChecked | MulChecked => Trap,

        // Time and duration arithmetic lowers to bare ADD/SUB, no guard.
        TimeAdd | TimeSub | DurationAdd | DurationSub | DurationScale => Pure,

        // --- Storage and memory ---
        SLoad(_) | MLoad => Pure,
        SStore(_) | MStore => State,

        // --- Map ---
        // Reads hash a key and SLOAD it; nothing observable when dropped.
        MapGet | MapHas | MapLength | MapKeys | MapValues => Pure,
        MapSet | MapDelete => State,

        // --- List ---
        // `ListGet` is classified Trap, not Pure: an out-of-range element
        // read is a revert (the bounds check on the `keccak256(slot) +
        // index * stride` address computation), and a revert is the only
        // thing a dead `let x = items[i]` still owes the caller. The
        // asymmetry decides it: classifying a non-trapping read as Trap
        // leaves one dead SLOAD in bytecode, classifying a trapping read as
        // Pure deletes the revert, which is the F-19 defect itself.
        ListGet => Trap,
        ListLength => Pure,
        // Not lowered yet: they push 0 and report through DEBT.md, no revert.
        ListSlice | ListFirst | ListLast | ListArgMax | ListArgMin => Pure,
        ListAppend | ListSet => State,

        // --- Priority queue ---
        // `PqInsert`/`PqPop` are writes by intent. The read side has no EVM
        // lowering at all: the backend raises a hard error AND emits a jump
        // to `__revert__` so an artifact deployed in spite of the error traps
        // instead of silently answering 0 (KSR-CVN-022). Both halves of that
        // fail-loud design need the instruction to survive to codegen: delete
        // it here and the error never fires and the trap never ships.
        PqInsert | PqPop => State,
        PqTopKey | PqTopValue | PqLength => Trap,

        // --- Struct ---
        StructNew(_) | StructGet(_) => Pure,
        StructSet(_) => State,

        // --- Choice / concat / text / bytes / hashing / encoding ---
        ChoiceMatch(_) => Pure,
        TextConcat | BytesConcat | ListConcat => Pure,
        BytesLength | TextLength | TextEquals => Pure,
        // Hashes scribble on EVM scratch memory below SSA_MEMORY_BASE only.
        Keccak | Blake2 | Hmac => Pure,
        AbiEncode | AbiDecode | AbiPack => Pure,

        // --- FHE data plane ---
        // These lower to a precompile call that reverts when the call fails,
        // so in the strictest reading they can trap. They are still Pure
        // here, deliberately: that revert is call-failure propagation, not a
        // condition the source language promises to enforce, and pass F
        // (`fhe_fold`) exists precisely to collapse duplicate trivial
        // encryptions, which only pays off if the duplicates can then be
        // eliminated. The security-critical members of the family are pulled
        // out below as `Critical` instead, which is the boundary KSR-CVN-026
        // drew.
        FheEncryptTrivial | FheEncryptFresh => Pure,
        FheAdd | FheSub | FheMul => Pure,
        FheCmpEq | FheCmpNe | FheCmpLt | FheCmpLe | FheCmpGt | FheCmpGe => Pure,
        FheAnd | FheOr | FheNot | FheSelect => Pure,
        FheCiphertextHash => Pure,
        // Noise refresh: dropping it busts the runtime noise budget.
        FheBootstrap => Critical,

        // --- ZK ---
        // A dropped `ZkVerify` is a dropped proof check, whatever the branch
        // that consumed its boolean was folded into.
        ZkVerify | VdfVerify => Critical,
        ZkNullifier | ZkProofPayload | VdfEval => Pure,

        // --- PQ ---
        PqVerifyDilithium | PqHybridVerify => Critical,
        PqRand | KyberEncrypt | KyberDecrypt => Pure,

        // --- Amnesia ---
        AmnesiaBegin | AmnesiaSubmitShare | AmnesiaFinalize | DestructionProof => State,
        VdfLock | VdfUnlock => State,
        // Same unlowered-opcode revert stub as the PQ read side above.
        ShamirSplit | ShamirReconstruct => Trap,

        // --- Events, transfers, assertions, external calls ---
        Emit(_) | Transfer => State,
        Assert | AssertEncrypted => State,
        RevealDecrypt => State,
        ExternalCall { .. } => State,

        // --- Authorization ---
        // `IsCallerSender` is a constant `1`. The other two have no lowering:
        // like the PQ read side they raise a hard error and jump to
        // `__revert__` rather than ship a guard that passes for every caller
        // (OMEGA V6 CRT-004). Deleting them would restore exactly the
        // defeated access control that fix was written to prevent.
        IsCallerSender => Pure,
        CallerMatchesPrincipal | BuiltinPredicateCall(_) => Trap,

        // --- Language-provided values and coercions ---
        LoadCaller | LoadNow | LoadBlockNumber | LoadBlockTimestamp | LoadMsgValue
        | LoadMsgSender | LoadDeployer | LoadThis | LoadChainId | LoadZeroAddress => Pure,
        Coerce(_) => Pure,
    }
}

/// Every value the function registry proves is an integer constant.
///
/// Computed once per pass because the eliminator needs it while it already
/// holds a mutable borrow of the block list.
pub fn integer_constants(func: &IrFunction) -> HashMap<Value, u128> {
    func.values
        .iter()
        .filter_map(|(v, info)| match info {
            ValueInfo::Const(IrConstant::Integer(n)) => Some((*v, *n)),
            _ => None,
        })
        .collect()
}

/// Can this particular instance of a trapping opcode actually trap?
///
/// Answering "yes" always is sound but pointlessly expensive: it would pin a
/// `MulChecked(2, 3)` into the bytecode after pass B already folded it to 6.
/// So the two families whose guard is decidable from constant operands get
/// decided here, using the same reasoning the rest of the compiler uses:
///
///  - `Div`/`Mod`: a literal non-zero divisor makes the backend emit no
///    runtime guard at all (`binop_div_guarded`), so there is no trap to
///    preserve and `value * bps / 10000` still costs nothing when dead.
///  - `AddChecked`/`SubChecked`/`MulChecked`: constant operands that do not
///    overflow `u128` cannot overflow a 256-bit EVM word either, so the
///    guard is unreachable. This is exactly the condition under which pass B
///    was willing to fold the operation, and it keeps `1 + 2 * 3` collapsing
///    to a single constant.
///
/// A literal ZERO divisor is deliberately NOT treated as safe. The backend
/// refuses it with E519 instead of deferring to a runtime revert, and a
/// diagnostic raised during codegen only fires if the instruction survives
/// long enough to be lowered. Dropping it here is how `let y = 100 / 0`
/// managed to compile clean under the default optimized build while
/// `--no-optimize` correctly refused it.
///
/// Anything not decided here is kept. Guessing in the other direction is the
/// F-19 defect.
pub fn trap_can_fire(instr: &Instr, consts: &HashMap<Value, u128>) -> bool {
    let operand_const = |i: usize| instr.operands.get(i).and_then(|v| consts.get(v)).copied();
    match instr.opcode {
        Opcode::Div | Opcode::Mod => !matches!(operand_const(1), Some(d) if d != 0),
        Opcode::AddChecked => !matches!(
            (operand_const(0), operand_const(1)),
            (Some(a), Some(b)) if a.checked_add(b).is_some()
        ),
        Opcode::SubChecked => !matches!(
            (operand_const(0), operand_const(1)),
            (Some(a), Some(b)) if a.checked_sub(b).is_some()
        ),
        Opcode::MulChecked => !matches!(
            (operand_const(0), operand_const(1)),
            (Some(a), Some(b)) if a.checked_mul(b).is_some()
        ),
        _ => true,
    }
}

/// True when the trap fires as a pure function of the operand VALUES, with no
/// dependence on chain state, memory, or how many times the instruction ran.
///
/// Inside one basic block the operands are SSA values, so a second identical
/// occurrence of such an instruction cannot fail where the first one
/// succeeded: it is straight-line code and nothing between them can change
/// what the operands hold. That makes the later copy's trap redundant, which
/// is what lets CSE keep paying for itself on `bal - amt` computed twice.
///
/// The property does NOT hold for a precompile or unlowered-opcode revert
/// (outcome depends on chain state) nor for `ListGet` (whose guard shape is
/// still moving), so those stay excluded.
pub fn trap_depends_only_on_operands(opcode: &Opcode) -> bool {
    matches!(
        opcode,
        Opcode::Div | Opcode::Mod | Opcode::AddChecked | Opcode::SubChecked | Opcode::MulChecked
    )
}

#[cfg(test)]
mod tests {
    use covenant_ir::{
        id::{GlobalId, StructTypeId},
        Opcode,
    };

    use super::{effect_of, trap_depends_only_on_operands, Effect};

    #[test]
    fn f19_trapping_arithmetic_is_not_pure() {
        for op in [
            Opcode::Div,
            Opcode::Mod,
            Opcode::AddChecked,
            Opcode::SubChecked,
            Opcode::MulChecked,
        ] {
            assert_eq!(
                effect_of(&op),
                Effect::Trap,
                "{op:?} carries a runtime revert and must never be classified Pure"
            );
        }
    }

    #[test]
    fn f19_unchecked_arithmetic_stays_pure() {
        // The unchecked forms have no guard, so the model must not
        // over-approximate them into Trap or DCE stops working entirely.
        for op in [Opcode::Add, Opcode::Sub, Opcode::Mul, Opcode::ShiftLeft] {
            assert_eq!(effect_of(&op), Effect::Pure, "{op:?} emits no guard");
        }
    }

    #[test]
    fn f19_unlowered_opcodes_that_revert_are_traps() {
        // Every one of these lowers to a hard diagnostic plus a jump to
        // `__revert__`; both halves need the instruction to reach codegen.
        for op in [
            Opcode::PqTopKey,
            Opcode::PqTopValue,
            Opcode::PqLength,
            Opcode::ShamirSplit,
            Opcode::ShamirReconstruct,
            Opcode::CallerMatchesPrincipal,
        ] {
            assert_eq!(effect_of(&op), Effect::Trap, "{op:?} lowers to a revert");
        }
    }

    #[test]
    fn ksr_026_boundary_is_preserved() {
        for op in [
            Opcode::FheBootstrap,
            Opcode::ZkVerify,
            Opcode::VdfVerify,
            Opcode::PqVerifyDilithium,
            Opcode::PqHybridVerify,
        ] {
            assert_eq!(effect_of(&op), Effect::Critical, "{op:?} must be kept");
        }
    }

    #[test]
    fn state_writers_are_state() {
        for op in [
            Opcode::SStore(GlobalId(0)),
            Opcode::MapSet,
            Opcode::ListAppend,
            Opcode::StructSet(0),
            Opcode::Transfer,
            Opcode::Assert,
        ] {
            assert_eq!(effect_of(&op), Effect::State, "{op:?} changes the world");
        }
    }

    #[test]
    fn only_operand_determined_traps_may_be_deduplicated() {
        assert!(trap_depends_only_on_operands(&Opcode::SubChecked));
        assert!(trap_depends_only_on_operands(&Opcode::Div));
        // Chain-state dependent or still-moving guards must not be.
        assert!(!trap_depends_only_on_operands(&Opcode::ListGet));
        assert!(!trap_depends_only_on_operands(&Opcode::PqTopKey));
        assert!(!trap_depends_only_on_operands(
            &Opcode::CallerMatchesPrincipal
        ));
    }

    #[test]
    fn pure_reads_stay_removable() {
        // Guard against the opposite failure: an effect model that keeps
        // everything is not a model, it is a disabled pass.
        for op in [
            Opcode::SLoad(GlobalId(0)),
            Opcode::MapGet,
            Opcode::ListLength,
            Opcode::StructNew(StructTypeId(0)),
            Opcode::Keccak,
            Opcode::LoadCaller,
        ] {
            assert_eq!(effect_of(&op), Effect::Pure, "{op:?} is a pure read");
        }
    }
}

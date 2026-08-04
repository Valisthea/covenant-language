//! The opcode catalog.
//!
//! Naming is normative (see Phase 6 spec). Every variant's doc-comment
//! specifies the expected operand count and result semantics. Phase 7
//! (optimizer) and Phase 8 (backend) key off these opcodes, do not rename
//! without updating every consumer.

use covenant_parser::ast::Ident;
use covenant_resolver::BuiltinPredicate;
use covenant_types::Ty;

use crate::id::{GlobalId, StructTypeId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Opcode {
    // --- Plaintext arithmetic ---
    /// `(amount|duration, amount|duration) -> amount|duration` without overflow check
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    SignedNeg,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
    BitNot,

    // --- Comparison (plaintext) ---
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,

    // --- Logical (plaintext) ---
    LogicalAnd,
    LogicalOr,
    LogicalNot,

    // --- Checked-arithmetic variants: revert on overflow for `amount` types ---
    AddChecked,
    SubChecked,
    MulChecked,

    // --- Time and duration ---
    /// `(time, duration) -> time` or symmetric.
    TimeAdd,
    /// `(time, duration) -> time` or `(time, time) -> duration`.
    TimeSub,
    DurationAdd,
    DurationSub,
    /// `(duration, amount) -> duration`.
    DurationScale,

    // --- Storage ---
    SLoad(GlobalId),
    SStore(GlobalId),
    MLoad,
    MStore,

    // --- Map ---
    MapGet,
    MapSet,
    MapDelete,
    MapHas,
    MapLength,
    MapKeys,
    MapValues,

    // --- List ---
    ListAppend,
    ListGet,
    ListSet,
    ListLength,
    ListSlice,
    ListFirst,
    ListLast,
    ListArgMax,
    ListArgMin,

    // --- Priority queue ---
    PqInsert,
    PqPop,
    PqTopKey,
    PqTopValue,
    PqLength,

    // --- Struct ---
    StructNew(StructTypeId),
    StructGet(u32),
    StructSet(u32),

    // --- Choice ---
    ChoiceMatch(Box<[Box<str>]>),

    // --- Concatenation ---
    TextConcat,
    BytesConcat,
    ListConcat,

    // --- String / bytes ---
    BytesLength,
    TextLength,
    TextEquals,

    // --- Hashing ---
    Keccak,
    Blake2,
    Hmac,

    // --- Encoding ---
    AbiEncode,
    AbiDecode,
    AbiPack,

    // --- FHE (Styx-FHE / ERC-8227) ---
    FheEncryptTrivial,
    FheEncryptFresh,
    FheAdd,
    FheSub,
    FheMul,
    FheCmpEq,
    FheCmpNe,
    FheCmpLt,
    FheCmpLe,
    FheCmpGt,
    FheCmpGe,
    FheAnd,
    FheOr,
    FheNot,
    /// `(cond_ct, true_ct, false_ct) -> ct`: merge values after FheBranch.
    FheSelect,
    FheBootstrap,
    FheCiphertextHash,

    // --- ZK (Styx-ZK / ERC-8229) ---
    ZkVerify,
    ZkNullifier,
    ZkProofPayload,
    VdfEval,
    VdfVerify,

    // --- PQ (Styx-PQ / ERC-8231) ---
    PqVerifyDilithium,
    PqHybridVerify,
    PqRand,
    KyberEncrypt,
    KyberDecrypt,

    // --- Amnesia (Styx-Amnesia / ERC-8228) ---
    AmnesiaBegin,
    AmnesiaSubmitShare,
    AmnesiaFinalize,
    ShamirSplit,
    ShamirReconstruct,
    VdfLock,
    VdfUnlock,
    DestructionCommitment,

    // --- Events and transfers ---
    Emit(Box<Ident>),
    /// `(amount, from?, to) -> unit`. `from` is optional; when absent, transfer
    /// is from the contract itself.
    Transfer,

    // --- Authorization ---
    IsCallerSender,
    CallerMatchesPrincipal,
    BuiltinPredicateCall(BuiltinPredicate),

    // --- Language-provided values ---
    LoadCaller,
    LoadNow,
    LoadBlockNumber,
    LoadBlockTimestamp,
    LoadMsgValue,
    LoadMsgSender,
    LoadDeployer,
    LoadThis,
    LoadChainId,
    LoadZeroAddress,

    // --- Reveal ---
    /// `(ciphertext) -> plaintext`: reveal body's decryption point.
    RevealDecrypt,

    // --- Assertion ---
    Assert,
    AssertEncrypted,

    // --- Type coercion (IR-level no-op; annotation only) ---
    Coerce(Box<Ty>),

    // --- Solidity interop ---
    /// Call an external Solidity contract.
    ///
    /// Operands: `[address, arg0, arg1, ...]`.
    /// `abi_sig` is the canonical signature (e.g. `"transfer(address,uint256)"`)
    /// from which the EVM backend derives the 4-byte selector.
    /// Use STATICCALL when `is_view` is true.
    ExternalCall {
        /// Canonical ABI signature: selector computed by the EVM backend.
        abi_sig: Box<str>,
        is_view: bool,
        /// Number of ABI-encoded arguments (excluding the leading address operand).
        arg_count: u32,
    },
}

impl Opcode {
    /// Expected operand count. Used by the validator.
    pub fn operand_count(&self) -> Option<(usize, Option<usize>)> {
        use Opcode::*;
        let exact = |n: usize| Some((n, Some(n)));
        let range = |lo: usize, hi: Option<usize>| Some((lo, hi));
        match self {
            Add | Sub | Mul | Div | Mod | BitAnd | BitOr | BitXor | ShiftLeft | ShiftRight | Eq
            | Ne | Lt | Le | Gt | Ge | LogicalAnd | LogicalOr | AddChecked | SubChecked
            | MulChecked | TimeAdd | TimeSub | DurationAdd | DurationSub | DurationScale => {
                exact(2)
            }

            SignedNeg | BitNot | LogicalNot => exact(1),

            SLoad(_) => exact(0),
            SStore(_) => exact(1),
            MLoad => exact(1),
            MStore => exact(2),

            MapGet | MapHas => exact(2),
            MapSet => exact(3),
            MapDelete => exact(2),
            MapLength | MapKeys | MapValues => exact(1),

            ListAppend => exact(2),
            ListGet => exact(2),
            ListSet => exact(3),
            ListLength | ListFirst | ListLast | ListArgMax | ListArgMin => exact(1),
            ListSlice => exact(3),

            PqInsert => exact(3),
            PqPop => exact(1),
            PqTopKey | PqTopValue | PqLength => exact(1),

            StructNew(_) => range(0, None),
            StructGet(_) => exact(1),
            StructSet(_) => exact(2),

            ChoiceMatch(_) => range(1, None),

            TextConcat | BytesConcat | ListConcat => exact(2),
            BytesLength | TextLength => exact(1),
            TextEquals => exact(2),

            Keccak | Blake2 | AbiEncode | AbiPack => range(0, None),
            Hmac => exact(2),
            AbiDecode => exact(1),

            FheEncryptTrivial | FheEncryptFresh => exact(1),
            FheAdd | FheSub | FheMul | FheAnd | FheOr => exact(2),
            FheCmpEq | FheCmpNe | FheCmpLt | FheCmpLe | FheCmpGt | FheCmpGe => exact(2),
            FheNot | FheBootstrap | FheCiphertextHash => exact(1),
            FheSelect => exact(3),

            ZkVerify => exact(3),
            ZkNullifier | ZkProofPayload => exact(1),
            VdfEval => exact(2),
            VdfVerify => exact(4),

            PqVerifyDilithium | PqHybridVerify => exact(3),
            PqRand => exact(1),
            KyberEncrypt => exact(2),
            KyberDecrypt => exact(2),

            AmnesiaBegin => exact(1),
            AmnesiaSubmitShare => exact(2),
            AmnesiaFinalize => exact(1),
            ShamirSplit => exact(4),
            ShamirReconstruct => exact(1),
            VdfLock => exact(2),
            VdfUnlock => exact(2),
            DestructionCommitment => exact(1),

            Emit(_) => range(0, None),
            Transfer => range(2, Some(3)),

            IsCallerSender => exact(0),
            CallerMatchesPrincipal => exact(1),
            BuiltinPredicateCall(_) => range(0, None),

            LoadCaller | LoadNow | LoadBlockNumber | LoadBlockTimestamp | LoadMsgValue
            | LoadMsgSender | LoadDeployer | LoadThis | LoadChainId | LoadZeroAddress => exact(0),

            RevealDecrypt => exact(1),

            Assert | AssertEncrypted => exact(1),

            Coerce(_) => exact(1),

            ExternalCall { arg_count, .. } => exact(1 + *arg_count as usize),
        }
    }

    /// Stable string name for this opcode, used by backends to derive wire
    /// selectors (KSR-CVN-020 precompile selector prefix). Does NOT include
    /// payload data for tuple variants: the selector is per-operation, not
    /// per-call-site.
    pub fn stable_name(&self) -> &'static str {
        use Opcode::*;
        match self {
            Add => "Add",
            Sub => "Sub",
            Mul => "Mul",
            Div => "Div",
            Mod => "Mod",
            SignedNeg => "SignedNeg",
            BitAnd => "BitAnd",
            BitOr => "BitOr",
            BitXor => "BitXor",
            ShiftLeft => "ShiftLeft",
            ShiftRight => "ShiftRight",
            BitNot => "BitNot",
            Eq => "Eq",
            Ne => "Ne",
            Lt => "Lt",
            Le => "Le",
            Gt => "Gt",
            Ge => "Ge",
            LogicalAnd => "LogicalAnd",
            LogicalOr => "LogicalOr",
            LogicalNot => "LogicalNot",
            AddChecked => "AddChecked",
            SubChecked => "SubChecked",
            MulChecked => "MulChecked",
            TimeAdd => "TimeAdd",
            TimeSub => "TimeSub",
            DurationAdd => "DurationAdd",
            DurationSub => "DurationSub",
            DurationScale => "DurationScale",
            SLoad(_) => "SLoad",
            SStore(_) => "SStore",
            MLoad => "MLoad",
            MStore => "MStore",
            MapGet => "MapGet",
            MapSet => "MapSet",
            MapDelete => "MapDelete",
            MapHas => "MapHas",
            MapLength => "MapLength",
            MapKeys => "MapKeys",
            MapValues => "MapValues",
            ListAppend => "ListAppend",
            ListGet => "ListGet",
            ListSet => "ListSet",
            ListLength => "ListLength",
            ListSlice => "ListSlice",
            ListFirst => "ListFirst",
            ListLast => "ListLast",
            ListArgMax => "ListArgMax",
            ListArgMin => "ListArgMin",
            ListConcat => "ListConcat",
            PqInsert => "PqInsert",
            PqPop => "PqPop",
            PqTopKey => "PqTopKey",
            PqTopValue => "PqTopValue",
            PqLength => "PqLength",
            StructNew(_) => "StructNew",
            StructGet(_) => "StructGet",
            StructSet(_) => "StructSet",
            ChoiceMatch(_) => "ChoiceMatch",
            TextConcat => "TextConcat",
            BytesConcat => "BytesConcat",
            BytesLength => "BytesLength",
            TextLength => "TextLength",
            TextEquals => "TextEquals",
            Keccak => "Keccak",
            Blake2 => "Blake2",
            Hmac => "Hmac",
            AbiEncode => "AbiEncode",
            AbiDecode => "AbiDecode",
            AbiPack => "AbiPack",
            FheEncryptTrivial => "FheEncryptTrivial",
            FheEncryptFresh => "FheEncryptFresh",
            FheAdd => "FheAdd",
            FheSub => "FheSub",
            FheMul => "FheMul",
            FheCmpEq => "FheCmpEq",
            FheCmpNe => "FheCmpNe",
            FheCmpLt => "FheCmpLt",
            FheCmpLe => "FheCmpLe",
            FheCmpGt => "FheCmpGt",
            FheCmpGe => "FheCmpGe",
            FheAnd => "FheAnd",
            FheOr => "FheOr",
            FheNot => "FheNot",
            FheSelect => "FheSelect",
            FheBootstrap => "FheBootstrap",
            FheCiphertextHash => "FheCiphertextHash",
            ZkVerify => "ZkVerify",
            ZkNullifier => "ZkNullifier",
            ZkProofPayload => "ZkProofPayload",
            VdfEval => "VdfEval",
            VdfVerify => "VdfVerify",
            PqVerifyDilithium => "PqVerifyDilithium",
            PqHybridVerify => "PqHybridVerify",
            PqRand => "PqRand",
            KyberEncrypt => "KyberEncrypt",
            KyberDecrypt => "KyberDecrypt",
            AmnesiaBegin => "AmnesiaBegin",
            AmnesiaSubmitShare => "AmnesiaSubmitShare",
            AmnesiaFinalize => "AmnesiaFinalize",
            ShamirSplit => "ShamirSplit",
            ShamirReconstruct => "ShamirReconstruct",
            VdfLock => "VdfLock",
            VdfUnlock => "VdfUnlock",
            // The name and the wire string deliberately disagree.
            //
            // The Rust identifier was renamed because "proof" overstates what
            // the helper returns: a deterministic commitment over the
            // submitted shares, which is evidence, not a demonstration that
            // anything was erased.
            //
            // The wire string cannot follow yet. `stable_name` feeds
            // `abi::precompile_selector`, which hashes
            // `covenant.precompile.<name>:v<PRECOMPILE_ABI_VERSION>`, so
            // changing it changes the calldata prefix of every emitted
            // precompile call, and that prefix is part of already-published
            // artifacts. It moves when PRECOMPILE_ABI_VERSION moves, which is
            // scheduled with the helper redeployment, not before.
            DestructionCommitment => "DestructionProof",
            Emit(_) => "Emit",
            Transfer => "Transfer",
            IsCallerSender => "IsCallerSender",
            CallerMatchesPrincipal => "CallerMatchesPrincipal",
            BuiltinPredicateCall(_) => "BuiltinPredicateCall",
            LoadCaller => "LoadCaller",
            LoadNow => "LoadNow",
            LoadBlockNumber => "LoadBlockNumber",
            LoadBlockTimestamp => "LoadBlockTimestamp",
            LoadMsgValue => "LoadMsgValue",
            LoadMsgSender => "LoadMsgSender",
            LoadDeployer => "LoadDeployer",
            LoadThis => "LoadThis",
            LoadChainId => "LoadChainId",
            LoadZeroAddress => "LoadZeroAddress",
            RevealDecrypt => "RevealDecrypt",
            Assert => "Assert",
            AssertEncrypted => "AssertEncrypted",
            Coerce(_) => "Coerce",
            ExternalCall { .. } => "ExternalCall",
        }
    }

    /// Returns true for opcodes whose results MUST NOT be CSE-merged across
    /// repeated invocations, even when the operand list is identical.
    ///
    /// Two categories:
    ///   - Randomness draws (each call must yield independent entropy):
    ///     `FheEncryptFresh`, `PqRand`, `ShamirSplit`.
    ///   - Time-locked / stateful primitives whose semantics tie each call
    ///     to a distinct execution point (or whose result depends on
    ///     hidden global state): `VdfEval`, `FheBootstrap`,
    ///     `AmnesiaBegin`, `AmnesiaSubmitShare`, `AmnesiaFinalize`,
    ///     `VdfLock`, `VdfUnlock`, `DestructionCommitment`.
    ///
    /// Optimizer passes (CSE in particular: see KSR-CVN-016/017) MUST
    /// consult this method before merging two values produced by the same
    /// opcode with identical operands.
    pub fn has_randomness_or_state(&self) -> bool {
        use Opcode::*;
        matches!(
            self,
            FheEncryptFresh
                | PqRand
                | ShamirSplit
                | VdfEval
                | FheBootstrap
                | AmnesiaBegin
                | AmnesiaSubmitShare
                | AmnesiaFinalize
                | VdfLock
                | VdfUnlock
                | DestructionCommitment
        )
    }

    /// Returns true for opcodes that DCE must preserve even when their SSA
    /// result is unused: verification primitives whose absence in the final
    /// bytecode would silently strip security checks, and `FheBootstrap`
    /// whose absence breaks runtime noise budgeting (KSR-CVN-026).
    ///
    /// These are not "side-effecting" in the storage / event sense, but they
    /// are observability-critical: removing a `ZkVerify` or `PqVerifyDilithium`
    /// because the boolean result was constant-folded into a dead branch
    /// would silently disable the verification.
    pub fn is_verify_or_noise_critical(&self) -> bool {
        use Opcode::*;
        matches!(
            self,
            FheBootstrap | ZkVerify | VdfVerify | PqVerifyDilithium | PqHybridVerify
        )
    }
}

//! Instructions, value info, constants, and block terminators.

use covenant_diag::Span;
use covenant_lexer::DurationUnit;
use covenant_resolver::{LangIdent, StdlibFn, StdlibModule};

use crate::id::{BlockId, FunctionId, GlobalId, Value};
use crate::opcode::Opcode;

/// An instruction in an `IrBlock`.
#[derive(Debug, Clone)]
pub struct Instr {
    /// `Some(v)` when the instruction produces a value; `None` for void ops
    /// such as `SStore`, `Emit`, `Assert`, `Transfer`.
    pub result: Option<Value>,
    pub opcode: Opcode,
    pub operands: Vec<Value>,
    pub metadata: InstrMetadata,
    pub span: Span,
}

#[derive(Debug, Clone, Default)]
pub struct InstrMetadata {
    /// True when the instruction was emitted by Phase 4's auto-lift handler.
    pub from_lift: bool,
    pub stdlib_call: Option<StdlibCall>,
    pub gas_hint: Option<(Option<u64>, Option<u64>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StdlibCall {
    Free(StdlibFn),
    Module {
        module: StdlibModule,
        method: Box<str>,
    },
}

/// What an SSA `Value` is.
#[derive(Debug, Clone)]
pub enum ValueInfo {
    Const(IrConstant),
    Param { func: FunctionId, index: u32 },
    BlockParam { block: BlockId, index: u32 },
    InstrResult { instr_idx: u32, block: BlockId },
    StorageRead(GlobalId),
    LangValue(LangIdent),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrConstant {
    Integer(u128),
    Text(Box<str>),
    Hex(Box<[u8]>),
    Bool(bool),
    Duration { n: u64, unit: DurationUnit },
    Address(Box<[u8; 20]>),
    Hash(Box<[u8; 32]>),
    ZeroAddress,
}

/// Block terminator. Every `IrBlock` ends with exactly one of these.
#[derive(Debug, Clone)]
pub enum Terminator {
    /// Unconditional jump carrying values for the target's block parameters.
    Jump {
        target: BlockId,
        args: Vec<Value>,
    },
    /// Plaintext conditional branch.
    Branch {
        cond: Value,
        then_target: BlockId,
        then_args: Vec<Value>,
        else_target: BlockId,
        else_args: Vec<Value>,
    },
    /// Encrypted conditional branch — both targets "execute" and merge via
    /// `FheSelect` in `merge_target`. Phase 8 lowers to real FHE branching.
    FheBranch {
        cond: Value,
        then_target: BlockId,
        then_args: Vec<Value>,
        else_target: BlockId,
        else_args: Vec<Value>,
        merge_target: BlockId,
    },
    Return(Option<Value>),
    Revert {
        error: covenant_parser::ast::Ident,
        args: Vec<Value>,
    },
    Unreachable,
}

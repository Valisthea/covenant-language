//! Basic block with explicit parameters.

use covenant_diag::Span;

use crate::id::{BlockId, Value};
use crate::instr::{Instr, Terminator};

#[derive(Debug, Clone)]
pub struct IrBlock {
    pub id: BlockId,
    /// Block parameters: this block's inputs. Predecessor terminators must
    /// supply matching argument lists.
    pub params: Vec<Value>,
    pub instructions: Vec<Instr>,
    pub terminator: Terminator,
    pub span: Span,
}

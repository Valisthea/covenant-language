//! `FuncBuilder` helper: a small API for constructing `IrFunction` values from
//! scratch. Used by the ERC-20 / ERC-8227 synthesizers.

use std::collections::HashMap;

use covenant_diag::Span;
use covenant_ir::{
    block::IrBlock,
    function::{IrFunction, IrFunctionKind, IrParam},
    id::{BlockId, FunctionId, Value},
    instr::{Instr, InstrMetadata, IrConstant, Terminator, ValueInfo},
    Opcode,
};
use covenant_parser::ast::Ident;
use covenant_privacy::{domain_of, PrivacyDomain};
use covenant_types::Ty;

pub struct FuncBuilder {
    pub id: FunctionId,
    pub name: Ident,
    pub kind: IrFunctionKind,
    pub params: Vec<IrParam>,
    pub returns: Option<Ty>,
    pub blocks: Vec<IrBlock>,
    pub values: Vec<(Value, ValueInfo)>,
    pub value_types: HashMap<Value, Ty>,
    pub value_privacy: HashMap<Value, PrivacyDomain>,
    pub value_spans: HashMap<Value, Span>,
    pub current_block: BlockId,
    pub next_value: u32,
    pub span: Span,
}

impl FuncBuilder {
    pub fn new(
        id: FunctionId,
        name: &str,
        kind: IrFunctionKind,
        returns: Option<Ty>,
        span: Span,
    ) -> Self {
        let entry = BlockId(0);
        let entry_block = IrBlock {
            id: entry,
            params: Vec::new(),
            instructions: Vec::new(),
            terminator: Terminator::Return(None),
            span,
        };
        Self {
            id,
            name: Ident {
                name: name.into(),
                span,
            },
            kind,
            params: Vec::new(),
            returns,
            blocks: vec![entry_block],
            values: Vec::new(),
            value_types: HashMap::new(),
            value_privacy: HashMap::new(),
            value_spans: HashMap::new(),
            current_block: entry,
            next_value: 0,
            span,
        }
    }

    pub fn add_param(&mut self, name: &str, ty: Ty) -> Value {
        let idx = self.params.len() as u32;
        let v = self.fresh_value(ValueInfo::Param {
            func: self.id,
            index: idx,
        });
        self.value_types.insert(v, ty.clone());
        self.value_privacy.insert(v, domain_of(&ty));
        self.value_spans.insert(v, self.span);
        self.params.push(IrParam {
            name: Ident {
                name: name.into(),
                span: self.span,
            },
            ty,
            value: v,
        });
        v
    }

    pub fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.blocks.len() as u32);
        self.blocks.push(IrBlock {
            id,
            params: Vec::new(),
            instructions: Vec::new(),
            terminator: Terminator::Unreachable,
            span: self.span,
        });
        id
    }

    pub fn set_current(&mut self, b: BlockId) {
        self.current_block = b;
    }

    fn fresh_value(&mut self, info: ValueInfo) -> Value {
        let v = Value(self.next_value);
        self.next_value += 1;
        self.values.push((v, info));
        v
    }

    pub fn emit_const(&mut self, c: IrConstant, ty: Ty) -> Value {
        let v = self.fresh_value(ValueInfo::Const(c));
        self.value_types.insert(v, ty.clone());
        self.value_privacy.insert(v, domain_of(&ty));
        self.value_spans.insert(v, self.span);
        v
    }

    pub fn emit_instr(
        &mut self,
        opcode: Opcode,
        operands: Vec<Value>,
        result_ty: Option<Ty>,
    ) -> Value {
        let has_result = result_ty.is_some();
        let result_v = if has_result {
            let instr_idx = self.current_block().instructions.len() as u32;
            let current = self.current_block;
            let v = self.fresh_value(ValueInfo::InstrResult {
                instr_idx,
                block: current,
            });
            if let Some(ty) = &result_ty {
                self.value_types.insert(v, ty.clone());
                self.value_privacy.insert(v, domain_of(ty));
            }
            self.value_spans.insert(v, self.span);
            Some(v)
        } else {
            None
        };
        let instr = Instr {
            result: result_v,
            opcode,
            operands,
            metadata: InstrMetadata::default(),
            span: self.span,
        };
        self.current_block_mut().instructions.push(instr);
        result_v.unwrap_or(Value(u32::MAX))
    }

    pub fn terminate(&mut self, t: Terminator) {
        self.current_block_mut().terminator = t;
    }

    pub fn current_block(&self) -> &IrBlock {
        &self.blocks[self.current_block.0 as usize]
    }

    pub fn current_block_mut(&mut self) -> &mut IrBlock {
        &mut self.blocks[self.current_block.0 as usize]
    }

    pub fn finish(self) -> IrFunction {
        IrFunction {
            id: self.id,
            name: self.name,
            kind: self.kind,
            params: self.params,
            returns: self.returns,
            guards: Vec::new(),
            qualifiers: Vec::new(),
            annotations: Vec::new(),
            blocks: self.blocks,
            entry: BlockId(0),
            values: self.values,
            value_types: self.value_types,
            value_privacy: self.value_privacy,
            local_to_value: HashMap::new(),
            value_spans: self.value_spans,
            span: self.span,
        }
    }
}

//! Symbolic assembly → raw bytecode.
//!
//! Two-phase assembly:
//! 1. Emit a stream of `AsmOp`s with symbolic labels.
//! 2. Resolve labels to byte offsets and emit final bytes.

use std::collections::HashMap;

/// A symbolic assembly operation.
#[derive(Debug, Clone)]
pub enum AsmOp {
    /// Raw EVM opcode byte.
    Op(u8),
    /// Push literal bytes onto the stack (PUSH1..PUSH32 selected by slice length).
    PushBytes(Vec<u8>),
    /// PUSH0 (EIP-3855, Shanghai+). Pushes a 32-byte zero onto the stack in one byte.
    Push0,
    /// Define a label at the current byte offset (emits JUMPDEST, 1 byte).
    LabelDef(Box<str>),
    /// Push a label's byte offset (resolved in phase 2) — 3-byte PUSH2 by convention.
    PushLabel(Box<str>),
}

// ---- EVM opcode byte constants ----
pub const OP_STOP: u8 = 0x00;
pub const OP_ADD: u8 = 0x01;
pub const OP_MUL: u8 = 0x02;
pub const OP_SUB: u8 = 0x03;
pub const OP_DIV: u8 = 0x04;
pub const OP_MOD: u8 = 0x06;
pub const OP_LT: u8 = 0x10;
pub const OP_GT: u8 = 0x11;
pub const OP_EQ: u8 = 0x14;
pub const OP_ISZERO: u8 = 0x15;
pub const OP_AND: u8 = 0x16;
pub const OP_OR: u8 = 0x17;
pub const OP_XOR: u8 = 0x18;
pub const OP_NOT: u8 = 0x19;
pub const OP_SHL: u8 = 0x1b;
pub const OP_SHR: u8 = 0x1c;
pub const OP_KECCAK256: u8 = 0x20;
pub const OP_ADDRESS: u8 = 0x30;
pub const OP_CALLER: u8 = 0x33;
pub const OP_CALLVALUE: u8 = 0x34;
pub const OP_CALLDATALOAD: u8 = 0x35;
pub const OP_CALLDATASIZE: u8 = 0x36;
pub const OP_CODECOPY: u8 = 0x39;
pub const OP_EXTCODESIZE: u8 = 0x3b;
pub const OP_RETURNDATASIZE: u8 = 0x3d;
pub const OP_TIMESTAMP: u8 = 0x42;
pub const OP_NUMBER: u8 = 0x43;
pub const OP_CHAINID: u8 = 0x46;
pub const OP_POP: u8 = 0x50;
pub const OP_MLOAD: u8 = 0x51;
pub const OP_MSTORE: u8 = 0x52;
pub const OP_SLOAD: u8 = 0x54;
pub const OP_SSTORE: u8 = 0x55;
pub const OP_JUMP: u8 = 0x56;
pub const OP_JUMPI: u8 = 0x57;
pub const OP_JUMPDEST: u8 = 0x5b;
pub const OP_PUSH0: u8 = 0x5f;
pub const OP_DUP1: u8 = 0x80;
pub const OP_SWAP1: u8 = 0x90;
pub const OP_LOG0: u8 = 0xa0;
pub const OP_LOG1: u8 = 0xa1;
pub const OP_LOG2: u8 = 0xa2;
pub const OP_LOG3: u8 = 0xa3;
pub const OP_LOG4: u8 = 0xa4;
pub const OP_CALL: u8 = 0xf1;
pub const OP_STATICCALL: u8 = 0xfa;
pub const OP_RETURN: u8 = 0xf3;
pub const OP_REVERT: u8 = 0xfd;
pub const OP_INVALID: u8 = 0xfe;

pub fn push_n(n: u8) -> u8 {
    debug_assert!((1..=32).contains(&n));
    0x60 + (n - 1)
}

pub struct Assembler {
    ops: Vec<AsmOp>,
}

impl Default for Assembler {
    fn default() -> Self {
        Self::new()
    }
}

impl Assembler {
    pub fn new() -> Self {
        Self { ops: Vec::new() }
    }

    pub fn emit(&mut self, op: AsmOp) {
        self.ops.push(op);
    }

    pub fn op(&mut self, byte: u8) {
        self.emit(AsmOp::Op(byte));
    }

    pub fn push_u32(&mut self, v: u32) {
        let bytes = v.to_be_bytes();
        let trimmed: Vec<u8> = bytes.iter().copied().skip_while(|&b| b == 0).collect();
        let bytes = if trimmed.is_empty() { vec![0] } else { trimmed };
        self.emit(AsmOp::PushBytes(bytes));
    }

    pub fn push_slot(&mut self, slot: u32) {
        self.push_u32(slot);
    }

    pub fn push_bytes(&mut self, bytes: Vec<u8>) {
        self.emit(AsmOp::PushBytes(bytes));
    }

    pub fn label_def(&mut self, name: impl Into<Box<str>>) {
        self.emit(AsmOp::LabelDef(name.into()));
    }

    pub fn push_label(&mut self, name: impl Into<Box<str>>) {
        self.emit(AsmOp::PushLabel(name.into()));
    }

    pub fn finish(self) -> Vec<AsmOp> {
        self.ops
    }
}

/// Estimate the byte length of an `AsmOp`.
fn op_len(op: &AsmOp) -> usize {
    match op {
        AsmOp::Op(_) => 1,
        AsmOp::PushBytes(b) => 1 + b.len(),
        AsmOp::Push0 => 1,
        AsmOp::LabelDef(_) => 1,  // becomes JUMPDEST
        AsmOp::PushLabel(_) => 3, // PUSH2 + 2 bytes
    }
}

/// Two-phase assembly: compute label offsets, then emit bytes.
///
/// Returns `(bytecode, unresolved_labels)`.
pub fn assemble(ops: &[AsmOp]) -> (Vec<u8>, Vec<Box<str>>) {
    // Phase 1: compute each label's byte offset.
    let mut labels: HashMap<Box<str>, u32> = HashMap::new();
    let mut cursor = 0u32;
    for op in ops {
        if let AsmOp::LabelDef(name) = op {
            labels.insert(name.clone(), cursor);
        }
        cursor += op_len(op) as u32;
    }

    // Phase 2: emit bytes.
    let mut out = Vec::with_capacity(cursor as usize);
    let mut unresolved = Vec::new();
    for op in ops {
        match op {
            AsmOp::Op(b) => out.push(*b),
            AsmOp::PushBytes(b) => {
                let n = b.len() as u8;
                if n == 0 {
                    out.push(OP_PUSH0);
                } else {
                    out.push(push_n(n));
                    out.extend_from_slice(b);
                }
            }
            AsmOp::Push0 => out.push(OP_PUSH0),
            AsmOp::LabelDef(_) => out.push(OP_JUMPDEST),
            AsmOp::PushLabel(name) => match labels.get(name) {
                Some(&offset) => {
                    out.push(push_n(2));
                    out.push((offset >> 8) as u8);
                    out.push(offset as u8);
                }
                None => {
                    unresolved.push(name.clone());
                    // Emit a placeholder PUSH2 0x0000 so the final size stays consistent.
                    out.push(push_n(2));
                    out.push(0);
                    out.push(0);
                }
            },
        }
    }

    (out, unresolved)
}

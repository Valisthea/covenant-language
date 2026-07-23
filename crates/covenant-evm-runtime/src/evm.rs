//! Minimal EVM interpreter sufficient to execute Covenant-compiled bytecode.
//!
//! Scope: this interpreter supports *exactly* the opcode subset emitted by
//! `covenant-evm-backend` (see `codegen.rs`). It is not a general-purpose EVM
//! — notably, CALL is only valid for precompile addresses and never forwards
//! to a second contract. Gas is not metered. Refunds, access lists, warm
//! storage pricing, and EIP-2929 are all ignored. See Phase 10 for rationale.
//!
//! This file is long but strictly linear: one big `execute` loop with match
//! arms for every supported opcode.

use std::collections::BTreeMap;

use sha3::{Digest, Keccak256};

use crate::address::Address;
use crate::precompiles::{dispatch as precompile_dispatch, is_precompile, MockPrecompileState};
use crate::u256::U256;

// ---- Opcode constants (mirrors asm.rs) ----
const OP_STOP: u8 = 0x00;
const OP_ADD: u8 = 0x01;
const OP_MUL: u8 = 0x02;
const OP_SUB: u8 = 0x03;
const OP_DIV: u8 = 0x04;
const OP_MOD: u8 = 0x06;
const OP_LT: u8 = 0x10;
const OP_GT: u8 = 0x11;
const OP_EQ: u8 = 0x14;
const OP_ISZERO: u8 = 0x15;
const OP_AND: u8 = 0x16;
const OP_OR: u8 = 0x17;
const OP_XOR: u8 = 0x18;
const OP_NOT: u8 = 0x19;
const OP_SHL: u8 = 0x1b;
const OP_SHR: u8 = 0x1c;
const OP_KECCAK256: u8 = 0x20;
const OP_ADDRESS: u8 = 0x30;
const OP_CALLER: u8 = 0x33;
const OP_CALLVALUE: u8 = 0x34;
const OP_CALLDATALOAD: u8 = 0x35;
const OP_CALLDATASIZE: u8 = 0x36;
const OP_CODECOPY: u8 = 0x39;
const OP_EXTCODESIZE: u8 = 0x3b;
const OP_RETURNDATASIZE: u8 = 0x3d;
const OP_TIMESTAMP: u8 = 0x42;
const OP_NUMBER: u8 = 0x43;
const OP_CHAINID: u8 = 0x46;
const OP_POP: u8 = 0x50;
const OP_MLOAD: u8 = 0x51;
const OP_MSTORE: u8 = 0x52;
const OP_SLOAD: u8 = 0x54;
const OP_SSTORE: u8 = 0x55;
const OP_JUMP: u8 = 0x56;
const OP_JUMPI: u8 = 0x57;
const OP_JUMPDEST: u8 = 0x5b;
const OP_PUSH0: u8 = 0x5f;
const OP_PUSH1: u8 = 0x60;
const OP_PUSH32: u8 = 0x7f;
const OP_DUP1: u8 = 0x80;
const OP_DUP16: u8 = 0x8f;
const OP_SWAP1: u8 = 0x90;
const OP_SWAP16: u8 = 0x9f;
const OP_LOG0: u8 = 0xa0;
const OP_LOG4: u8 = 0xa4;
const OP_CALL: u8 = 0xf1;
const OP_RETURN: u8 = 0xf3;
const OP_STATICCALL: u8 = 0xfa;
const OP_REVERT: u8 = 0xfd;
const OP_INVALID: u8 = 0xfe;

/// Execution outcome of a single contract invocation.
#[derive(Debug, Clone)]
pub enum CallResult {
    /// Successful return with data (STOP or RETURN).
    Ok(Vec<u8>),
    /// REVERT with data.
    Revert(Vec<u8>),
    /// INVALID / out-of-gas-equivalent / unknown opcode.
    Abort(String),
}

impl CallResult {
    pub fn is_ok(&self) -> bool {
        matches!(self, CallResult::Ok(_))
    }
    pub fn is_revert(&self) -> bool {
        matches!(self, CallResult::Revert(_))
    }
    pub fn data(&self) -> &[u8] {
        match self {
            CallResult::Ok(d) | CallResult::Revert(d) => d,
            CallResult::Abort(_) => &[],
        }
    }
}

/// An event emitted by LOG{0..4}.
#[derive(Debug, Clone)]
pub struct LogEvent {
    pub address: Address,
    pub topics: Vec<[u8; 32]>,
    pub data: Vec<u8>,
}

/// Environment for a call.
#[derive(Debug, Clone)]
pub struct CallEnv {
    pub caller: Address,
    pub address: Address,
    pub value: U256,
    pub input: Vec<u8>,
    pub timestamp: u64,
    pub block_number: u64,
    pub chain_id: u64,
    pub is_static: bool,
}

/// Host storage keyed by contract address.
#[derive(Debug, Default, Clone)]
pub struct HostState {
    pub storage: BTreeMap<Address, BTreeMap<U256, U256>>,
    pub code: BTreeMap<Address, Vec<u8>>,
    pub balances: BTreeMap<Address, U256>,
    pub logs: Vec<LogEvent>,
}

impl HostState {
    pub fn sload(&self, a: Address, k: U256) -> U256 {
        self.storage
            .get(&a)
            .and_then(|m| m.get(&k).copied())
            .unwrap_or(U256::ZERO)
    }
    pub fn sstore(&mut self, a: Address, k: U256, v: U256) {
        self.storage.entry(a).or_default().insert(k, v);
    }
}

/// Execute EVM bytecode. Returns a call result plus any logs emitted.
pub fn execute(
    code: &[u8],
    env: &CallEnv,
    host: &mut HostState,
    precompiles: &mut MockPrecompileState,
) -> CallResult {
    let mut stack: Vec<U256> = Vec::with_capacity(1024);
    let mut memory: Vec<u8> = Vec::with_capacity(4096);
    // Valid JUMPDEST set.
    let jumpdests = compute_jumpdests(code);
    // Size of the last CALL/STATICCALL returndata (for RETURNDATASIZE opcode).
    let mut last_returndatasize: usize = 0;

    let mut pc = 0usize;
    let mut steps = 0usize;
    const STEP_LIMIT: usize = 5_000_000;

    macro_rules! pop {
        () => {
            match stack.pop() {
                Some(v) => v,
                None => return CallResult::Abort("stack underflow".to_string()),
            }
        };
    }
    macro_rules! push {
        ($v:expr) => {{
            if stack.len() >= 1024 {
                return CallResult::Abort("stack overflow".to_string());
            }
            stack.push($v);
        }};
    }

    while pc < code.len() {
        steps += 1;
        if steps > STEP_LIMIT {
            return CallResult::Abort("step limit exceeded".to_string());
        }

        let op = code[pc];
        match op {
            OP_STOP => return CallResult::Ok(Vec::new()),
            OP_ADD => {
                let a = pop!();
                let b = pop!();
                push!(a.wrapping_add(&b));
            }
            OP_SUB => {
                let a = pop!();
                let b = pop!();
                push!(a.wrapping_sub(&b));
            }
            OP_MUL => {
                let a = pop!();
                let b = pop!();
                push!(a.wrapping_mul(&b));
            }
            OP_DIV => {
                let a = pop!();
                let b = pop!();
                push!(a.div(&b));
            }
            OP_MOD => {
                let a = pop!();
                let b = pop!();
                push!(a.rem(&b));
            }
            OP_LT => {
                let a = pop!();
                let b = pop!();
                push!(U256::from(a < b));
            }
            OP_GT => {
                let a = pop!();
                let b = pop!();
                push!(U256::from(a > b));
            }
            OP_EQ => {
                let a = pop!();
                let b = pop!();
                push!(U256::from(a == b));
            }
            OP_ISZERO => {
                let a = pop!();
                push!(U256::from(a.is_zero()));
            }
            OP_AND => {
                let a = pop!();
                let b = pop!();
                push!(a.bitand(&b));
            }
            OP_OR => {
                let a = pop!();
                let b = pop!();
                push!(a.bitor(&b));
            }
            OP_XOR => {
                let a = pop!();
                let b = pop!();
                push!(a.bitxor(&b));
            }
            OP_NOT => {
                let a = pop!();
                push!(a.bitnot());
            }
            OP_SHL => {
                // EVM: shift amount on top, value below.
                let shift = pop!();
                let value = pop!();
                let s = if shift.fits_u64() && shift.low_u64() < 256 {
                    shift.low_u64() as u32
                } else {
                    256
                };
                push!(value.shl(s));
            }
            OP_SHR => {
                let shift = pop!();
                let value = pop!();
                let s = if shift.fits_u64() && shift.low_u64() < 256 {
                    shift.low_u64() as u32
                } else {
                    256
                };
                push!(value.shr(s));
            }
            OP_KECCAK256 => {
                let offset = pop!().as_usize_saturating();
                let length = pop!().as_usize_saturating();
                memory_expand(&mut memory, offset, length);
                let data = &memory[offset..offset + length];
                let mut hasher = Keccak256::new();
                hasher.update(data);
                let h: [u8; 32] = hasher.finalize().into();
                push!(U256::from_be_bytes(h));
            }
            OP_ADDRESS => push!(env.address.to_u256()),
            OP_CALLER => push!(env.caller.to_u256()),
            OP_CALLVALUE => push!(env.value),
            OP_CALLDATALOAD => {
                let off = pop!().as_usize_saturating();
                let mut word = [0u8; 32];
                for (i, slot) in word.iter_mut().enumerate() {
                    let idx = off.wrapping_add(i);
                    if idx < env.input.len() {
                        *slot = env.input[idx];
                    }
                }
                push!(U256::from_be_bytes(word));
            }
            OP_CALLDATASIZE => push!(U256::from_u64(env.input.len() as u64)),
            OP_CODECOPY => {
                let dest = pop!().as_usize_saturating();
                let offset = pop!().as_usize_saturating();
                let length = pop!().as_usize_saturating();
                memory_expand(&mut memory, dest, length);
                for i in 0..length {
                    let src = offset.wrapping_add(i);
                    memory[dest + i] = if src < code.len() { code[src] } else { 0 };
                }
            }
            OP_EXTCODESIZE => {
                // KSR-CVN-019: precompile-canary support. Standard precompiles
                // (and EOAs) have no code → return 0. Code lookup is via the
                // host's `code` map; unknown addresses return 0.
                let addr_word = pop!();
                let addr = Address::from_u256(addr_word);
                let size = host.code.get(&addr).map(|c| c.len() as u64).unwrap_or(0);
                push!(U256::from_u64(size));
            }
            OP_TIMESTAMP => push!(U256::from_u64(env.timestamp)),
            OP_NUMBER => push!(U256::from_u64(env.block_number)),
            OP_CHAINID => push!(U256::from_u64(env.chain_id)),
            OP_POP => {
                let _ = pop!();
            }
            OP_MLOAD => {
                let offset = pop!().as_usize_saturating();
                memory_expand(&mut memory, offset, 32);
                let mut w = [0u8; 32];
                w.copy_from_slice(&memory[offset..offset + 32]);
                push!(U256::from_be_bytes(w));
            }
            OP_MSTORE => {
                let offset = pop!().as_usize_saturating();
                let value = pop!();
                memory_expand(&mut memory, offset, 32);
                memory[offset..offset + 32].copy_from_slice(&value.to_be_bytes());
            }
            OP_SLOAD => {
                let key = pop!();
                push!(host.sload(env.address, key));
            }
            OP_SSTORE => {
                if env.is_static {
                    return CallResult::Abort("SSTORE in static context".to_string());
                }
                let key = pop!();
                let val = pop!();
                host.sstore(env.address, key, val);
            }
            OP_JUMP => {
                let dest = pop!();
                let d = dest.as_usize_saturating();
                if !jumpdests.contains(&d) {
                    return CallResult::Abort(format!("invalid JUMP to {d}"));
                }
                pc = d;
                continue;
            }
            OP_JUMPI => {
                let dest = pop!();
                let cond = pop!();
                if !cond.is_zero() {
                    let d = dest.as_usize_saturating();
                    if !jumpdests.contains(&d) {
                        return CallResult::Abort(format!("invalid JUMPI to {d}"));
                    }
                    pc = d;
                    continue;
                }
            }
            OP_JUMPDEST => {}
            OP_PUSH0 => push!(U256::ZERO),
            OP_PUSH1..=OP_PUSH32 => {
                let n = (op - OP_PUSH1 + 1) as usize;
                let start = pc + 1;
                let end = (start + n).min(code.len());
                let mut buf = [0u8; 32];
                let slice = &code[start..end];
                buf[32 - slice.len()..].copy_from_slice(slice);
                push!(U256::from_be_bytes(buf));
                pc += 1 + n;
                continue;
            }
            OP_DUP1..=OP_DUP16 => {
                let n = (op - OP_DUP1 + 1) as usize;
                if stack.len() < n {
                    return CallResult::Abort("stack underflow (DUP)".to_string());
                }
                let v = stack[stack.len() - n];
                push!(v);
            }
            OP_SWAP1..=OP_SWAP16 => {
                let n = (op - OP_SWAP1 + 1) as usize;
                if stack.len() < n + 1 {
                    return CallResult::Abort("stack underflow (SWAP)".to_string());
                }
                let top = stack.len() - 1;
                stack.swap(top, top - n);
            }
            OP_LOG0..=OP_LOG4 => {
                // KSR-CVN-PRELIM-004 (Sprint 26 audit): per EIP-214,
                // LOG{0..4} in a STATICCALL context must abort. Mirrors
                // the existing OP_SSTORE check pattern. Pre-fix, the
                // log was pushed to host.logs unconditionally; the
                // chain.rs static_call boundary discarded it, so no
                // information leaked — but the bytecode behaviour
                // diverged from real EVM, masking portability bugs
                // until deploy.
                if env.is_static {
                    return CallResult::Abort("LOG in static context".to_string());
                }
                let n_topics = (op - OP_LOG0) as usize;
                let offset = pop!().as_usize_saturating();
                let length = pop!().as_usize_saturating();
                let mut topics: Vec<[u8; 32]> = Vec::with_capacity(n_topics);
                for _ in 0..n_topics {
                    topics.push(pop!().to_be_bytes());
                }
                memory_expand(&mut memory, offset, length);
                let data = memory[offset..offset + length].to_vec();
                host.logs.push(LogEvent {
                    address: env.address,
                    topics,
                    data,
                });
            }
            OP_CALL => {
                // CALL(gas, addr, value, argsOff, argsLen, retOff, retLen)
                let _gas = pop!();
                let addr_w = pop!();
                let value = pop!();
                let args_off = pop!().as_usize_saturating();
                let args_len = pop!().as_usize_saturating();
                let ret_off = pop!().as_usize_saturating();
                let ret_len = pop!().as_usize_saturating();
                memory_expand(&mut memory, args_off, args_len);
                memory_expand(&mut memory, ret_off, ret_len);
                let input = memory[args_off..args_off + args_len].to_vec();
                let addr_low = addr_low_u16(addr_w);
                if is_precompile(addr_low) {
                    let out = precompile_dispatch(addr_low, &input, precompiles);
                    write_return(&mut memory, ret_off, ret_len, &out);
                    last_returndatasize = out.len();
                    push!(U256::ONE);
                } else {
                    last_returndatasize = 0;
                    // Plain ether transfer / external call: subtract value from
                    // caller, add to recipient. We don't execute target code.
                    let to = Address::from_u256(addr_w);
                    if !value.is_zero() {
                        let cur = host
                            .balances
                            .get(&env.address)
                            .copied()
                            .unwrap_or(U256::ZERO);
                        if cur < value {
                            push!(U256::ZERO);
                            pc += 1;
                            continue;
                        }
                        host.balances.insert(env.address, cur.wrapping_sub(&value));
                        let dst = host.balances.get(&to).copied().unwrap_or(U256::ZERO);
                        host.balances.insert(to, dst.wrapping_add(&value));
                    }
                    push!(U256::ONE);
                }
            }
            OP_STATICCALL => {
                let _gas = pop!();
                let addr_w = pop!();
                let args_off = pop!().as_usize_saturating();
                let args_len = pop!().as_usize_saturating();
                let ret_off = pop!().as_usize_saturating();
                let ret_len = pop!().as_usize_saturating();
                memory_expand(&mut memory, args_off, args_len);
                memory_expand(&mut memory, ret_off, ret_len);
                let input = memory[args_off..args_off + args_len].to_vec();
                let addr_low = addr_low_u16(addr_w);
                if is_precompile(addr_low) {
                    let out = precompile_dispatch(addr_low, &input, precompiles);
                    write_return(&mut memory, ret_off, ret_len, &out);
                    last_returndatasize = out.len();
                    push!(U256::ONE);
                } else {
                    // Non-precompile STATICCALL targets are treated as inert no-ops.
                    // Returndata size is 0 — the new precompile-boundary checks
                    // (KSR-CVN-014) will detect this and revert.
                    last_returndatasize = 0;
                    push!(U256::ONE);
                }
            }
            OP_RETURNDATASIZE => {
                push!(U256::from(last_returndatasize as u64));
            }
            OP_RETURN => {
                let offset = pop!().as_usize_saturating();
                let length = pop!().as_usize_saturating();
                memory_expand(&mut memory, offset, length);
                let data = memory[offset..offset + length].to_vec();
                return CallResult::Ok(data);
            }
            OP_REVERT => {
                let offset = pop!().as_usize_saturating();
                let length = pop!().as_usize_saturating();
                memory_expand(&mut memory, offset, length);
                let data = memory[offset..offset + length].to_vec();
                return CallResult::Revert(data);
            }
            OP_INVALID => return CallResult::Revert(Vec::new()),
            other => {
                return CallResult::Abort(format!("unsupported opcode 0x{other:02x} at pc={pc}"));
            }
        }

        pc += 1;
    }

    CallResult::Ok(Vec::new())
}

fn memory_expand(mem: &mut Vec<u8>, offset: usize, len: usize) {
    if len == 0 {
        return;
    }
    let need = offset.saturating_add(len);
    if need > mem.len() {
        mem.resize(need, 0);
    }
}

fn addr_low_u16(w: U256) -> u16 {
    let b = w.to_be_bytes();
    // Last 2 bytes.
    u16::from_be_bytes([b[30], b[31]])
}

fn write_return(mem: &mut Vec<u8>, ret_off: usize, ret_len: usize, data: &[u8]) {
    if ret_len == 0 {
        return;
    }
    memory_expand(mem, ret_off, ret_len);
    for i in 0..ret_len {
        mem[ret_off + i] = if i < data.len() { data[i] } else { 0 };
    }
}

/// Scan bytecode to identify valid JUMPDESTs (outside PUSH-data regions).
fn compute_jumpdests(code: &[u8]) -> std::collections::HashSet<usize> {
    let mut set = std::collections::HashSet::new();
    let mut i = 0;
    while i < code.len() {
        let op = code[i];
        if op == OP_JUMPDEST {
            set.insert(i);
            i += 1;
        } else if (OP_PUSH1..=OP_PUSH32).contains(&op) {
            let n = (op - OP_PUSH1 + 1) as usize;
            i += 1 + n;
        } else {
            i += 1;
        }
    }
    set
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_returns_ok() {
        let mut host = HostState::default();
        let mut pcs = MockPrecompileState::default();
        let env = CallEnv {
            caller: Address::ZERO,
            address: Address::ZERO,
            value: U256::ZERO,
            input: Vec::new(),
            timestamp: 0,
            block_number: 0,
            chain_id: 1,
            is_static: false,
        };
        let r = execute(&[OP_STOP], &env, &mut host, &mut pcs);
        assert!(r.is_ok());
    }

    #[test]
    fn push_add_return() {
        // PUSH1 2; PUSH1 3; ADD; PUSH1 0; MSTORE; PUSH1 32; PUSH1 0; RETURN
        let code = [
            OP_PUSH1, 2, OP_PUSH1, 3, OP_ADD, OP_PUSH1, 0, OP_MSTORE, OP_PUSH1, 32, OP_PUSH1, 0,
            OP_RETURN,
        ];
        let mut host = HostState::default();
        let mut pcs = MockPrecompileState::default();
        let env = CallEnv {
            caller: Address::ZERO,
            address: Address::ZERO,
            value: U256::ZERO,
            input: Vec::new(),
            timestamp: 0,
            block_number: 0,
            chain_id: 1,
            is_static: false,
        };
        let r = execute(&code, &env, &mut host, &mut pcs);
        match r {
            CallResult::Ok(d) => {
                assert_eq!(U256::from_be_bytes(d.try_into().unwrap()).low_u64(), 5);
            }
            _ => panic!("expected ok"),
        }
    }
}

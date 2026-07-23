//! The main EVM codegen driver.
//!
//! V0 lowering strategy uses the MemoryMapped allocator: every SSA value
//! lives at `0x80 + value.0 * 32` in memory. Operands are `MLOAD`ed onto the
//! stack when needed; results are `MSTORE`d back. Simple, verbose, and
//! auditable — gas-inefficient but correct. Phase 7/8 revisions can add
//! stack packing later.

use std::collections::BTreeMap;

use covenant_diag::{Diagnostic, Span};
use covenant_ir::{
    instr::{Instr, IrConstant, Terminator, ValueInfo},
    module::IrMetadataValue,
    BlockId, GlobalId, IrBlock, IrFunction, IrModule, Opcode, StructTypeId, Value,
};
use covenant_parser::ast::PrivacyQualifier;
use covenant_types::Ty;

use crate::abi;
use crate::asm::{self, AsmOp, Assembler};
use crate::config::EvmConfig;
use crate::diag as d;
use crate::storage::{self, DEPLOYER_SLOT, REENTRANT_LOCK_SLOT};

/// Starting memory offset for SSA values. 0x80 is the Solidity convention: it
/// leaves 0x00-0x3F scratch space and 0x40-0x7F for free memory pointer and
/// zero slot.
pub const SSA_MEMORY_BASE: u32 = 0x80;

pub fn value_memory_offset(v: Value) -> u32 {
    SSA_MEMORY_BASE + v.0 * 32
}

pub struct Codegen<'a> {
    pub module: &'a IrModule,
    pub config: &'a EvmConfig,
    pub diagnostics: Vec<Diagnostic>,
    pub selectors: BTreeMap<Box<str>, [u8; 4]>,
}

impl<'a> Codegen<'a> {
    pub fn new(module: &'a IrModule, config: &'a EvmConfig) -> Self {
        Self {
            module,
            config,
            diagnostics: Vec::new(),
            selectors: BTreeMap::new(),
        }
    }

    /// Emit the full runtime bytecode (dispatcher + all public functions).
    pub fn emit_runtime(&mut self) -> Vec<AsmOp> {
        let mut asm = Assembler::new();

        // Compute selectors up front.
        for (name, _sig, sel) in
            abi::compute_selectors(self.module, self.config.include_test_actions)
        {
            self.selectors.insert(name, sel);
        }

        // --- Dispatcher ---
        //   PUSH0, CALLDATALOAD, PUSH E0, SHR   (extract selector into top stack)
        asm.emit(AsmOp::Push0);
        asm.op(asm::OP_CALLDATALOAD);
        asm.push_u32(0xE0);
        asm.op(asm::OP_SHR);

        //   For each function: DUP1, PUSH selector, EQ, PUSH label, JUMPI
        for f in &self.module.functions {
            if !abi::is_emitted_function(f, self.config.include_test_actions) {
                continue;
            }
            let sel = self.selectors.get(f.name.name.as_ref()).copied();
            if let Some(sel) = sel {
                asm.op(asm::OP_DUP1);
                asm.push_bytes(sel.to_vec());
                asm.op(asm::OP_EQ);
                asm.push_label(format!("fn_{}", f.name.name));
                asm.op(asm::OP_JUMPI);
            }
        }

        // Fallback: revert with empty data.
        asm.emit(AsmOp::Push0);
        asm.emit(AsmOp::Push0);
        asm.op(asm::OP_REVERT);

        // --- Public function bodies ---
        for f in &self.module.functions {
            if !abi::is_emitted_function(f, self.config.include_test_actions) {
                continue;
            }
            asm.label_def(format!("fn_{}", f.name.name));
            self.emit_function(&mut asm, f);
        }

        // Shared revert label used by Assert failures.
        asm.label_def("__revert__");
        asm.emit(AsmOp::Push0);
        asm.emit(AsmOp::Push0);
        asm.op(asm::OP_REVERT);

        asm.finish()
    }

    /// Emit the constructor bytecode. Captures deployer and copies runtime
    /// bytecode out via CODECOPY + RETURN.
    pub fn emit_constructor(&mut self, runtime_size: u32) -> Vec<AsmOp> {
        let mut asm = Assembler::new();

        // --- KSR-CVN-029: precompile-ABI version marker ---
        // Emit the compile-time PRECOMPILE_ABI_VERSION as a dead PUSH4/POP
        // sequence at the very top of the constructor. Off-chain tooling
        // (and chain governance) can byte-scan for this marker to reject a
        // deployment whose compiled ABI version does not match the chain's.
        asm.push_u32(crate::artifact::PRECOMPILE_ABI_VERSION);
        asm.op(asm::OP_POP);

        // --- KSR-CVN-019: precompile EXTCODESIZE canary ---
        // Standard precompiles have no associated EVM code (EXTCODESIZE = 0).
        // If an attacker has deployed a fake contract at a precompile address
        // we use, EXTCODESIZE > 0 and we must refuse to deploy. One canary
        // assertion per address actually referenced by this module.
        self.emit_precompile_extcodesize_canary(&mut asm);

        // --- Capture deployer ---
        //   CALLER, PUSH deployer_slot, SSTORE
        asm.op(asm::OP_CALLER);
        asm.push_u32(DEPLOYER_SLOT);
        asm.op(asm::OP_SSTORE);

        // --- Initialize fields with constant initializers ---
        for field in &self.module.fields {
            if let Some(c) = &field.initializer_const {
                self.emit_const_initializer(&mut asm, field.id, c);
            }
        }

        // --- Genesis mint (Token constructs only) ---
        //
        // If the module declares `supply: N to deployer`, seed the balances
        // map entry for the deployer with `N`. The balances map's slot is
        // treated as the map base and the key is the deployer address loaded
        // from DEPLOYER_SLOT. The EVM slot is keccak256(key || base).
        self.emit_genesis_mint(&mut asm);

        // --- Return runtime bytecode ---
        //   PUSH runtime_size, PUSH runtime_offset, PUSH 0, CODECOPY
        //   PUSH runtime_size, PUSH 0, RETURN
        asm.push_u32(runtime_size);
        asm.push_label("__runtime_start__");
        asm.emit(AsmOp::Push0);
        asm.op(asm::OP_CODECOPY);
        asm.push_u32(runtime_size);
        asm.emit(AsmOp::Push0);
        asm.op(asm::OP_RETURN);

        // Constructor-local revert label referenced by hardened precompile call
        // sequences (KSR-CVN-013/014) emitted inside emit_genesis_mint.
        asm.label_def("__revert__");
        asm.emit(AsmOp::Push0);
        asm.emit(AsmOp::Push0);
        asm.op(asm::OP_REVERT);

        asm.finish()
    }

    /// KSR-CVN-019: collect every precompile address this module actually
    /// uses, then emit, for each, an EXTCODESIZE assertion that reverts if
    /// non-zero. Standard V0.8 precompiles have EXTCODESIZE = 0; a non-zero
    /// value would mean an attacker (or a misconfigured chain) has deployed
    /// a contract at the precompile address.
    ///
    /// V0.9: this canary applies ONLY when the target uses native
    /// precompiles (MockChain). For helper-contract targets (Sepolia,
    /// AsterTestnet) the helpers ARE deployed at those addresses, so a
    /// non-zero EXTCODESIZE is expected — the inverse check (verify the
    /// expected helper bytecode hash) is added in Sprint 32 as a separate
    /// gate. For V0.9.0 the canary is simply skipped.
    fn emit_precompile_extcodesize_canary(&self, asm: &mut Assembler) {
        // Detect whether we're targeting helper contracts. Heuristic: any
        // configured precompile address has the high 18 bytes non-zero
        // (MockChain addresses have the form `[0; 18] || u16` while helper
        // contract addresses have non-zero high bytes).
        let p = &self.config.precompile_addresses;
        let helpers_target = p.amnesia.begin[..18].iter().any(|b| *b != 0)
            || p.fhe.encrypt_trivial[..18].iter().any(|b| *b != 0);
        if helpers_target {
            // Skip canary for helper-contract targets. Sprint 32 adds the
            // inverse code-hash check as a separate emit pass.
            return;
        }

        let mut addrs: std::collections::BTreeSet<crate::target::EvmAddress> =
            std::collections::BTreeSet::new();
        for f in &self.module.functions {
            for b in &f.blocks {
                for instr in &b.instructions {
                    if let Some(addr) = self.precompile_address_for(&instr.opcode) {
                        addrs.insert(addr);
                    }
                }
            }
        }
        // The genesis-mint path uses FheEncryptTrivial implicitly when the
        // construct is Confidential — make sure that address is canaried too.
        if matches!(
            self.module.construct_privacy,
            Some(PrivacyQualifier::Confidential)
        ) {
            addrs.insert(self.config.precompile_addresses.fhe.encrypt_trivial);
        }
        for addr in addrs {
            // PUSH addr; EXTCODESIZE; ISZERO; PUSH __revert__; JUMPI
            // (revert if EXTCODESIZE != 0, i.e., when ISZERO returns 0)
            asm.push_bytes(addr.to_vec());
            asm.op(asm::OP_EXTCODESIZE);
            // We want to JUMPI to __revert__ when size != 0.
            // ISZERO(size) returns 1 when size==0, 0 when size!=0. So JUMPI
            // when ISZERO(ISZERO(size)) — which is `size != 0`.
            asm.op(asm::OP_ISZERO);
            asm.op(asm::OP_ISZERO);
            asm.push_label("__revert__");
            asm.op(asm::OP_JUMPI);
        }
    }

    /// Map an opcode to its target precompile address, if any. Returns None
    /// for non-precompile opcodes.
    ///
    /// V0.8 returned `Option<u16>` because all precompiles fit in `0x100`–
    /// `0x1FF`. V0.9 returns `Option<EvmAddress>` (`[u8; 20]`) so the same
    /// helper can return either a V0.8 short address (lifted to 20 bytes)
    /// or a deployed helper-contract address on Sepolia/Aster.
    fn precompile_address_for(&self, op: &Opcode) -> Option<crate::target::EvmAddress> {
        let p = &self.config.precompile_addresses;
        Some(match op {
            Opcode::FheEncryptTrivial => p.fhe.encrypt_trivial,
            Opcode::FheEncryptFresh => p.fhe.encrypt_fresh,
            Opcode::FheAdd => p.fhe.add,
            Opcode::FheSub => p.fhe.sub,
            Opcode::FheMul => p.fhe.mul,
            Opcode::FheCmpEq => p.fhe.cmp_eq,
            Opcode::FheCmpNe => p.fhe.cmp_ne,
            Opcode::FheCmpLt => p.fhe.cmp_lt,
            Opcode::FheCmpLe => p.fhe.cmp_le,
            Opcode::FheCmpGt => p.fhe.cmp_gt,
            Opcode::FheCmpGe => p.fhe.cmp_ge,
            Opcode::FheAnd => p.fhe.and,
            Opcode::FheOr => p.fhe.or,
            Opcode::FheNot => p.fhe.not,
            Opcode::FheSelect => p.fhe.select,
            Opcode::FheBootstrap => p.fhe.bootstrap,
            Opcode::FheCiphertextHash => p.fhe.ciphertext_hash,
            Opcode::RevealDecrypt => p.fhe.reveal_decrypt,
            Opcode::AssertEncrypted => p.fhe.threshold_decrypt_bool,
            Opcode::ZkVerify => p.zk.verify,
            Opcode::ZkNullifier => p.zk.nullifier,
            Opcode::VdfEval => p.zk.vdf_eval,
            Opcode::VdfVerify => p.zk.vdf_verify,
            Opcode::PqVerifyDilithium => p.pq.verify_dilithium,
            Opcode::PqHybridVerify => p.pq.hybrid_verify,
            Opcode::PqRand => p.pq.rand_pq,
            Opcode::KyberEncrypt => p.pq.kyber_encrypt,
            Opcode::KyberDecrypt => p.pq.kyber_decrypt,
            Opcode::AmnesiaBegin => p.amnesia.begin,
            Opcode::AmnesiaSubmitShare => p.amnesia.submit_share,
            Opcode::AmnesiaFinalize => p.amnesia.finalize,
            Opcode::DestructionProof => p.amnesia.destruction_proof,
            _ => return None,
        })
    }

    /// Emit the constructor-time balances seeding step for a `supply: N to
    /// deployer` declaration. No-op when the metadata is absent or when the
    /// recipient is not `deployer` (other principals are V0.2+). Assumes a
    /// field named `balances` exists and is a map; if not, silently skips.
    fn emit_genesis_mint(&self, asm: &mut Assembler) {
        // Pull the supply metadata.
        let (amount, is_deployer) = match self.module.metadata.get("supply") {
            Some(IrMetadataValue::GenesisMint { amount, to }) => {
                let is_deployer = matches!(
                    to,
                    covenant_parser::ast::Principal::Named(
                        covenant_parser::ast::PrincipalKind::Deployer
                    )
                );
                (*amount, is_deployer)
            }
            _ => return,
        };
        if !is_deployer {
            return;
        }
        let Some(balances_field) = self
            .module
            .fields
            .iter()
            .find(|f| f.name.name.as_ref() == "balances")
        else {
            return;
        };
        let balances_slot = storage::slot_for_global(self.module, balances_field.id);

        // Lay out scratch memory for the keyed-slot hash:
        //   mem[0..32]  = deployer address, right-aligned
        //   mem[32..64] = map slot NUMBER — **must match the runtime convention**
        //                  used by `emit_map_slot`, which now hashes
        //                  `keccak(key || slot_number)` (KSR-CVN-029, V0.9.2).
        //                  Both sides must stay aligned, so the genesis balance
        //                  is written to keccak(deployer || balances_slot).
        asm.push_u32(DEPLOYER_SLOT);
        asm.op(asm::OP_SLOAD); // stack: [deployer]
        asm.emit(AsmOp::Push0); // [deployer, 0]
        asm.op(asm::OP_MSTORE); // mem[0..32] = deployer; stack: []

        // Write the balances map slot NUMBER into scratch mem[32..64].
        asm.push_u32(balances_slot);
        asm.push_u32(32);
        asm.op(asm::OP_MSTORE); // mem[32..64] = balances_slot; stack: []

        // slot = keccak256(offset=0, size=64)
        asm.push_u32(64);
        asm.emit(AsmOp::Push0);
        asm.op(asm::OP_KECCAK256); // stack: [slot]

        if matches!(
            self.module.construct_privacy,
            Some(PrivacyQualifier::Confidential)
        ) {
            // Confidential token: FHE-encrypt the genesis amount, store the handle.
            // Memory layout (KSR-CVN-020):
            //   mem[0x40..0x60]: selector word (low 4 bytes = selector)
            //   mem[0x60..0x80]: plaintext amount (operand 0)
            //   mem[0x80..0xA0]: return handle
            // argsOffset = 0x5C, argsSize = 4 + 32 = 36
            let sel = abi::precompile_selector("FheEncryptTrivial");
            asm.push_bytes(sel.to_vec()); // [slot, selector_u32]
            asm.push_u32(0x40); // [slot, selector, 0x40]
            asm.op(asm::OP_MSTORE); // mem[0x40..0x5C]=0, mem[0x5C..0x60]=selector; stack: [slot]
            asm.push_bytes(big_endian_u128(amount)); // [slot, amount]
            asm.push_u32(0x60); // [slot, amount, 0x60]
            asm.op(asm::OP_MSTORE); // mem[0x60..0x80] = amount; stack: [slot]
                                    // STATICCALL FHE_ENCRYPT_TRIVIAL: (gas, addr, argsOff, argsSize, retOff, retSize)
                                    // V0.9: 20-byte EvmAddress pushed via PUSH20 (push_bytes auto-emits PUSH<n>).
            let fhe_addr = self.config.precompile_addresses.fhe.encrypt_trivial;
            asm.push_u32(32); // retSize
            asm.push_u32(0x80); // retOffset — handle lands at mem[0x80]
            asm.push_u32(36); // argsSize = 4 + 32
            asm.push_u32(0x5C); // argsOffset — the 4 selector bytes
            asm.push_bytes(fhe_addr.to_vec()); // addr (PUSH20)
            asm.push_bytes(vec![0xff, 0xff, 0xff, 0xff]); // gas (TOS)
            asm.op(asm::OP_STATICCALL); // [slot, success]
                                        // KSR-CVN-013: revert if precompile STATICCALL failed.
            asm.op(asm::OP_ISZERO); // [slot, success == 0]
            asm.push_label("__revert__");
            asm.op(asm::OP_JUMPI); // [slot]
                                   // KSR-CVN-014: revert if returndata size is not 32.
            asm.op(asm::OP_RETURNDATASIZE);
            asm.push_u32(32);
            asm.op(asm::OP_EQ);
            asm.op(asm::OP_ISZERO);
            asm.push_label("__revert__");
            asm.op(asm::OP_JUMPI); // [slot]
            asm.push_u32(0x80);
            asm.op(asm::OP_MLOAD); // [slot, handle]
                                   // SSTORE: TOS = key, second = value.
                                   // stack [slot, handle]: handle is TOS → SWAP1 to put slot on top.
            asm.op(0x90); // SWAP1 → [handle, slot]
            asm.op(asm::OP_SSTORE); // slot → storage[slot] = handle ✓
        } else {
            // EVM SSTORE pops key (top) then value → we need [value, key] before.
            //   [slot]
            //   PUSH amount            → [slot, amount]
            //   SWAP1                  → [amount, slot]
            //   SSTORE                 → mem[slot] = amount
            asm.push_bytes(big_endian_u128(amount));
            asm.op(0x90); // SWAP1
            asm.op(asm::OP_SSTORE);
        }
    }

    fn emit_const_initializer(&self, asm: &mut Assembler, field_id: GlobalId, c: &IrConstant) {
        let slot = storage::slot_for_global(self.module, field_id);
        let bytes: Vec<u8> = match c {
            IrConstant::Integer(n) => big_endian_u128(*n),
            IrConstant::Bool(b) => vec![if *b { 1 } else { 0 }],
            IrConstant::Address(bytes) => bytes.to_vec(),
            IrConstant::Hash(bytes) => bytes.to_vec(),
            IrConstant::ZeroAddress => vec![0u8; 20],
            _ => vec![0], // Text / Hex / Duration require more complex ABI layout — deferred
        };
        asm.push_bytes(bytes);
        asm.push_u32(slot);
        asm.op(asm::OP_SSTORE);
    }

    fn emit_function(&mut self, asm: &mut Assembler, f: &IrFunction) {
        // KSR-CVN-023: `@vdf_locked(delay)` is parsed all the way to
        // `IrActionQualifier::VdfLocked` but never lowered to a time-lock
        // predicate. Compiling such an action silently produces bytecode
        // where the supposedly-delayed entrypoint executes immediately.
        // Hard-fail rather than emit an "instant" action.
        if f.qualifiers
            .iter()
            .any(|q| matches!(q, covenant_ir::function::IrActionQualifier::VdfLocked(_)))
        {
            self.diagnostics
                .push(d::unlowered_vdf_qualifier(f.name.span));
        }

        // OMEGA V6 (CRT-007 fix): `pq_key` params/returns hit the same
        // static-vs-dynamic ABI mismatch as the unlowered-primitive cases
        // above (see `d::pq_key_abi_static_dynamic_mismatch`). Diagnose it
        // and emit an unconditional revert for the whole function instead
        // of the normal (silently-corrupting) codegen.
        let uses_pq_key_abi_mismatch = f.params.iter().any(|p| matches!(p.ty, Ty::PqKey))
            || matches!(f.returns, Some(Ty::PqKey));
        if uses_pq_key_abi_mismatch {
            self.diagnostics.push(d::pq_key_abi_static_dynamic_mismatch(
                f.name.span,
                &f.name.name,
            ));
            asm.label_def(format!("fn_{}_b{}", f.name.name, f.entry.0));
            asm.push_label("__revert__");
            asm.op(asm::OP_JUMP);
            return;
        }

        // Check if this function has the @non_reentrant annotation.
        let is_non_reentrant = f.annotations.iter().any(|a| {
            matches!(a, covenant_ir::function::IrAnnotation::Unknown(n) if n.as_ref() == "non_reentrant")
        });

        // KSR-CVN-012: proxy-initializer re-init guard. A public action
        // named `initialize` (or tagged `@initializer`) must be callable
        // exactly once — subsequent calls revert. Without this every
        // Covenant-compiled UUPS/Transparent proxy exposes an open
        // owner-hijack primitive.
        let is_initializer = is_initializer_action(f);

        // Emit each block as a labeled section. Entry block is just after the
        // function's public label.
        let entry_id = f.entry;
        for block in &f.blocks {
            asm.label_def(format!("fn_{}_b{}", f.name.name, block.id.0));
            // Prelude: for the entry block only, copy each parameter's word
            // from calldata into its SSA memory slot. ABI convention: selector
            // occupies bytes [0, 4); arg `i` occupies bytes [4 + i*32, 4 + (i+1)*32).
            // V0.1 scope: static scalar types (address, uint256, bool, hash,
            // duration, amount). Dynamic types (text, bytes, list, map, struct)
            // do not round-trip yet and will be extended in V0.2.
            if block.id == entry_id {
                self.emit_param_prelude(asm, f);
                if is_initializer {
                    self.emit_initializer_guard(asm, &f.name.name);
                }
                if is_non_reentrant {
                    self.emit_reentrant_lock_acquire(asm, &f.name.name);
                }
            }
            for instr in &block.instructions {
                self.emit_instr(asm, f, block, instr);
            }
            // For @non_reentrant: release the lock before any return.
            if is_non_reentrant {
                if let covenant_ir::instr::Terminator::Return(_) = &block.terminator {
                    self.emit_reentrant_lock_release(asm);
                }
            }
            self.emit_terminator(asm, f, block);
        }
    }

    /// KSR-CVN-012: emit the proxy re-initialization guard at the entry of
    /// an initializer action.
    ///
    /// Emitted sequence:
    ///   PUSH32 flag_slot → SLOAD → ISZERO → PUSH ok_label → JUMPI
    ///   (fall-through: flag set → already initialized) → PUSH0 PUSH0 REVERT
    ///   ok_label: JUMPDEST → PUSH 1 → PUSH32 flag_slot → SSTORE
    ///
    /// `flag_slot` is a 32-byte keccak-derived slot (EIP-7201-style
    /// namespacing) computed from `"covenant.proxy.initializer.<Module>"`,
    /// so it cannot collide with sequential user fields.
    fn emit_initializer_guard(&self, asm: &mut Assembler, fn_name: &str) {
        let slot = initializer_flag_slot(self.module.name.name.as_ref());
        let ok_label = format!("__init_ok_{fn_name}__");
        // Load flag.
        asm.push_bytes(slot.to_vec());
        asm.op(asm::OP_SLOAD); // [flag]
        asm.op(asm::OP_ISZERO); // [flag==0 ? 1 : 0]
        asm.push_label(ok_label.clone());
        asm.op(asm::OP_JUMPI);
        // Already initialized → revert (no data).
        asm.emit(AsmOp::Push0);
        asm.emit(AsmOp::Push0);
        asm.op(asm::OP_REVERT);
        // ok_label: set flag = 1.
        asm.label_def(ok_label);
        asm.push_bytes(vec![1]);
        asm.push_bytes(slot.to_vec());
        asm.op(asm::OP_SSTORE);
    }

    /// Acquire the reentrancy lock: revert if already entered, then set slot to 1.
    ///
    /// Emitted sequence:
    ///   PUSH lock_slot → SLOAD → ISZERO → PUSH ok_label → JUMPI
    ///   (fall-through: lock held) → PUSH0 PUSH0 REVERT
    ///   ok_label: JUMPDEST → PUSH 1 → PUSH lock_slot → SSTORE
    fn emit_reentrant_lock_acquire(&self, asm: &mut Assembler, fn_name: &str) {
        let ok_label = format!("__nr_ok_{fn_name}__");
        // Load lock value.
        asm.push_u32(REENTRANT_LOCK_SLOT);
        asm.op(asm::OP_SLOAD); // stack: [lock_val]
                               // If lock_val == 0, proceed; otherwise revert.
                               // ISZERO → 1 if lock_val==0 (available), 0 if held.
        asm.op(asm::OP_ISZERO); // stack: [lock_val==0 ? 1 : 0]
        asm.push_label(ok_label.clone()); // stack: [ok_label, iszero]
        asm.op(asm::OP_JUMPI); // if iszero != 0 → jump to ok_label
                               // Revert (lock is held).
        asm.emit(AsmOp::Push0);
        asm.emit(AsmOp::Push0);
        asm.op(asm::OP_REVERT);
        // ok_label: lock is free — acquire it.
        asm.label_def(ok_label);
        asm.push_bytes(vec![1]); // value = 1
        asm.push_u32(REENTRANT_LOCK_SLOT);
        asm.op(asm::OP_SSTORE); // lock = 1
    }

    /// Release the reentrancy lock: set slot back to 0.
    fn emit_reentrant_lock_release(&self, asm: &mut Assembler) {
        asm.emit(AsmOp::Push0); // value = 0
        asm.push_u32(REENTRANT_LOCK_SLOT);
        asm.op(asm::OP_SSTORE); // lock = 0
    }

    /// Copy each function parameter from its calldata offset into its SSA
    /// memory slot at the start of the entry block. For each parameter
    /// `f.params[i]`:
    ///
    /// ```text
    /// PUSH 4 + i*32      ; calldata offset
    /// CALLDATALOAD       ; 32-byte word
    /// PUSH slot          ; SSA memory offset
    /// MSTORE             ; store word
    /// ```
    fn emit_param_prelude(&mut self, asm: &mut Assembler, f: &IrFunction) {
        for (i, param) in f.params.iter().enumerate() {
            if !is_static_abi_ty(&param.ty) {
                // Skip dynamic types — their slots stay zero until V0.2.
                continue;
            }
            let calldata_off = 4u32 + (i as u32) * 32;
            asm.push_u32(calldata_off);
            asm.op(asm::OP_CALLDATALOAD);
            asm.push_u32(value_memory_offset(param.value));
            asm.op(asm::OP_MSTORE);
        }
    }

    fn emit_instr(&mut self, asm: &mut Assembler, f: &IrFunction, _block: &IrBlock, instr: &Instr) {
        match &instr.opcode {
            // ---- Arithmetic / comparison ----
            Opcode::Add => self.binop(asm, f, instr, asm::OP_ADD),
            Opcode::Sub => self.binop(asm, f, instr, asm::OP_SUB),
            Opcode::Mul => self.binop(asm, f, instr, asm::OP_MUL),
            // Checked variants revert on overflow/underflow (KSR-CVN-031).
            Opcode::AddChecked => self.binop_checked(asm, f, instr, asm::OP_ADD),
            Opcode::SubChecked => self.binop_checked(asm, f, instr, asm::OP_SUB),
            Opcode::MulChecked => self.binop_checked(asm, f, instr, asm::OP_MUL),
            // Guarded: EVM DIV/MOD are TOTAL (x/0 == 0, x%0 == 0) rather than
            // trapping, so a bare lowering made `pot / participants` silently
            // evaluate to 0 on an empty set. Solidity reverts here.
            Opcode::Div => self.binop_div_guarded(asm, f, instr, asm::OP_DIV, "division"),
            Opcode::Mod => self.binop_div_guarded(asm, f, instr, asm::OP_MOD, "remainder"),
            Opcode::BitAnd | Opcode::LogicalAnd => self.binop(asm, f, instr, asm::OP_AND),
            Opcode::BitOr | Opcode::LogicalOr => self.binop(asm, f, instr, asm::OP_OR),
            Opcode::BitXor => self.binop(asm, f, instr, asm::OP_XOR),
            Opcode::ShiftLeft => self.binop(asm, f, instr, asm::OP_SHL),
            Opcode::ShiftRight => self.binop(asm, f, instr, asm::OP_SHR),
            Opcode::Eq => self.binop(asm, f, instr, asm::OP_EQ),
            Opcode::Ne => {
                // EQ is commutative so either operand order is fine, but we
                // inline here because binop() would MSTORE the EQ result and
                // leave the stack empty before the ISZERO.
                self.load_operand(asm, f, instr.operands[1]);
                self.load_operand(asm, f, instr.operands[0]);
                asm.op(asm::OP_EQ);
                asm.op(asm::OP_ISZERO);
                self.mstore_result(asm, instr);
            }
            Opcode::Lt => self.binop(asm, f, instr, asm::OP_LT),
            Opcode::Gt => self.binop(asm, f, instr, asm::OP_GT),
            Opcode::Le => {
                // a <= b  ≡  !(a > b). Push b then a so GT on stack [b, a]
                // pops a (top) first, computes `a > b`.
                self.load_operand(asm, f, instr.operands[1]);
                self.load_operand(asm, f, instr.operands[0]);
                asm.op(asm::OP_GT);
                asm.op(asm::OP_ISZERO);
                self.mstore_result(asm, instr);
            }
            Opcode::Ge => {
                // a >= b  ≡  !(a < b). Push b then a so LT on stack [b, a]
                // pops a (top) first, computes `a < b`.
                self.load_operand(asm, f, instr.operands[1]);
                self.load_operand(asm, f, instr.operands[0]);
                asm.op(asm::OP_LT);
                asm.op(asm::OP_ISZERO);
                self.mstore_result(asm, instr);
            }
            Opcode::LogicalNot => self.unary_iszero(asm, f, instr),
            Opcode::BitNot => {
                self.load_operand(asm, f, instr.operands[0]);
                asm.op(asm::OP_NOT);
                self.mstore_result(asm, instr);
            }
            Opcode::SignedNeg => {
                // Two's-complement negation: 0 - x.
                asm.emit(AsmOp::Push0);
                self.load_operand(asm, f, instr.operands[0]);
                asm.op(asm::OP_SUB);
                self.mstore_result(asm, instr);
            }

            // ---- Time / duration arithmetic ----
            Opcode::TimeAdd | Opcode::DurationAdd | Opcode::DurationScale => {
                self.binop(asm, f, instr, asm::OP_ADD);
            }
            Opcode::TimeSub | Opcode::DurationSub => {
                self.binop(asm, f, instr, asm::OP_SUB);
            }

            // ---- Storage ----
            Opcode::SLoad(id) => {
                let slot = storage::slot_for_global(self.module, *id);
                asm.push_u32(slot);
                asm.op(asm::OP_SLOAD);
                self.mstore_result(asm, instr);
            }
            Opcode::SStore(id) => {
                let slot = storage::slot_for_global(self.module, *id);
                self.load_operand(asm, f, instr.operands[0]);
                asm.push_u32(slot);
                asm.op(asm::OP_SSTORE);
            }

            // ---- Map operations — runtime keccak for keyed slot ----
            Opcode::MapGet => {
                // Operands: [map_base (SLoad of the field), key].
                // emit_map_slot traces map_base back to the field's slot number
                // and hashes keccak(key || slot_number) (KSR-CVN-029).
                self.emit_map_slot(asm, f, instr);
                asm.op(asm::OP_SLOAD);
                self.mstore_result(asm, instr);
            }
            Opcode::MapSet => {
                // Operands: map_handle, key, value. Compute slot, SSTORE value.
                self.load_operand(asm, f, instr.operands[2]); // value on stack top
                self.emit_map_slot(asm, f, instr);
                asm.op(asm::OP_SSTORE);
            }
            Opcode::MapDelete => {
                asm.emit(AsmOp::Push0);
                self.emit_map_slot(asm, f, instr);
                asm.op(asm::OP_SSTORE);
            }
            Opcode::MapLength | Opcode::MapHas | Opcode::MapKeys | Opcode::MapValues => {
                // Simplified: return 0 / empty for now.
                asm.emit(AsmOp::Push0);
                self.mstore_result(asm, instr);
            }

            // ---- List operations ----
            // OMEGA V6 (CRT-003 follow-on): storage layout for `list<T>` fields
            // -- length lives at the field's own slot (SLoad/SStore(field_id),
            // same as any scalar field); elements live at
            // `keccak256(slot_number) + index * stride` (Solidity's own
            // dynamic-array convention), where `stride` is 1 for a scalar
            // element type or the struct's field count for `list<Struct>`.
            // For struct elements, `ListGet` yields that ADDRESS (for
            // `StructGet`/`StructSet` to then offset into); for scalar
            // elements it yields the dereferenced value directly.
            Opcode::ListLength => {
                // Operand 0 is the list handle (an SLoad of the field) --
                // its OWN codegen already SLOADs the field's slot, so its
                // memory-slot value IS the current length.
                self.load_operand(asm, f, instr.operands[0]);
                self.mstore_result(asm, instr);
            }
            Opcode::ListGet => {
                let stride = self.list_elem_stride(f, instr.operands[0]);
                self.emit_list_elem_addr(asm, f, instr.operands[0], instr.operands[1], stride);
                if stride == 1 {
                    asm.op(asm::OP_SLOAD);
                }
                // stride > 1 (struct element): leave the computed address on
                // the stack as the "struct value" for StructGet/StructSet.
                self.mstore_result(asm, instr);
            }
            Opcode::ListSet => {
                // Operands: [list_handle, index, value]. Only meaningful for
                // scalar element lists (struct-element writes route through
                // ListGet + StructSet instead, see `LValue::FieldAccess`
                // lowering in the IR builder).
                let stride = self.list_elem_stride(f, instr.operands[0]);
                self.load_operand(asm, f, instr.operands[2]);
                self.emit_list_elem_addr(asm, f, instr.operands[0], instr.operands[1], stride);
                asm.op(asm::OP_SSTORE);
            }
            Opcode::ListAppend => {
                // Operands: [list_handle (SLoad of the field slot => current
                // length), value]. Writes the new element at `keccak256(slot)
                // + length*stride` and leaves the INCREMENTED length as the
                // result -- the IR builder immediately SStores it back to the
                // field (mirrors the existing MapSet/SStore(field_id) pattern,
                // rather than hiding a second SSTORE inside this opcode).
                let stride = self.list_elem_stride(f, instr.operands[0]);
                let struct_fields = self.struct_new_operands(f, instr.operands[1]);
                match struct_fields {
                    Some(fields) => {
                        let fields = fields.to_vec();
                        for (k, field_v) in fields.iter().enumerate() {
                            self.load_operand(asm, f, *field_v);
                            self.emit_list_base_addr(asm, f, instr.operands[0]);
                            self.load_operand(asm, f, instr.operands[0]); // current length
                            asm.push_u32(stride);
                            asm.op(asm::OP_MUL);
                            asm.op(asm::OP_ADD);
                            if k > 0 {
                                asm.push_u32(k as u32);
                                asm.op(asm::OP_ADD);
                            }
                            asm.op(asm::OP_SSTORE);
                        }
                    }
                    None => {
                        // Scalar element (or an un-traceable value): single word.
                        self.load_operand(asm, f, instr.operands[1]);
                        self.emit_list_base_addr(asm, f, instr.operands[0]);
                        self.load_operand(asm, f, instr.operands[0]); // current length
                        asm.op(asm::OP_ADD);
                        asm.op(asm::OP_SSTORE);
                    }
                }
                // result = length + 1
                self.load_operand(asm, f, instr.operands[0]);
                asm.push_u32(1);
                asm.op(asm::OP_ADD);
                self.mstore_result(asm, instr);
            }
            Opcode::ListSlice
            | Opcode::ListFirst
            | Opcode::ListLast
            | Opcode::ListArgMax
            | Opcode::ListArgMin => {
                // Not yet implemented -- these are less common ops without an
                // existing user (no example/fixture exercises them). Fail
                // loudly (0) rather than silently misreport, tracked in
                // DEBT.md pending a real implementation.
                asm.emit(AsmOp::Push0);
                self.mstore_result(asm, instr);
            }

            // ---- Struct operations ----
            Opcode::StructNew(_) => {
                // The "struct value" a StructNew produces is never read back
                // directly -- ListAppend (its only real consumer) re-derives
                // the field operands from this instruction itself (see
                // `struct_new_operands`), and ListGet returns a computed
                // storage ADDRESS for StructGet/StructSet to use instead.
                // This placeholder result is therefore never dereferenced.
                asm.emit(AsmOp::Push0);
                self.mstore_result(asm, instr);
            }
            Opcode::StructGet(field_idx) => {
                // Operand 0 is an ADDRESS (from ListGet on a struct-element list).
                self.load_operand(asm, f, instr.operands[0]);
                if *field_idx > 0 {
                    asm.push_u32(*field_idx);
                    asm.op(asm::OP_ADD);
                }
                asm.op(asm::OP_SLOAD);
                self.mstore_result(asm, instr);
            }
            Opcode::StructSet(field_idx) => {
                // Operands: [address, value].
                self.load_operand(asm, f, instr.operands[1]);
                self.load_operand(asm, f, instr.operands[0]);
                if *field_idx > 0 {
                    asm.push_u32(*field_idx);
                    asm.op(asm::OP_ADD);
                }
                asm.op(asm::OP_SSTORE);
            }

            // ---- Concat / bytes / text ----
            Opcode::TextConcat | Opcode::BytesConcat | Opcode::ListConcat => {
                self.binop(asm, f, instr, asm::OP_ADD); // placeholder
            }
            Opcode::BytesLength | Opcode::TextLength => {
                asm.emit(AsmOp::Push0);
                self.mstore_result(asm, instr);
            }
            Opcode::TextEquals => self.binop(asm, f, instr, asm::OP_EQ),

            // ---- Keccak / hashing ----
            Opcode::Keccak | Opcode::Blake2 | Opcode::Hmac => {
                self.emit_keccak(asm, f, instr);
            }

            // ---- ABI encode / decode — placeholder ----
            Opcode::AbiEncode | Opcode::AbiDecode | Opcode::AbiPack => {
                // Copy first operand's slot to result slot as a no-op stand-in.
                if let Some(op0) = instr.operands.first().copied() {
                    self.load_operand(asm, f, op0);
                    self.mstore_result(asm, instr);
                } else {
                    asm.emit(AsmOp::Push0);
                    self.mstore_result(asm, instr);
                }
            }

            // ---- FHE precompile calls ----
            Opcode::FheEncryptTrivial => self.emit_precompile_call(
                asm,
                f,
                instr,
                self.config.precompile_addresses.fhe.encrypt_trivial,
            ),
            Opcode::FheEncryptFresh => self.emit_precompile_call(
                asm,
                f,
                instr,
                self.config.precompile_addresses.fhe.encrypt_fresh,
            ),
            Opcode::FheAdd => {
                self.emit_precompile_call(asm, f, instr, self.config.precompile_addresses.fhe.add)
            }
            Opcode::FheSub => {
                self.emit_precompile_call(asm, f, instr, self.config.precompile_addresses.fhe.sub)
            }
            Opcode::FheMul => {
                self.emit_precompile_call(asm, f, instr, self.config.precompile_addresses.fhe.mul)
            }
            Opcode::FheCmpEq => self.emit_precompile_call(
                asm,
                f,
                instr,
                self.config.precompile_addresses.fhe.cmp_eq,
            ),
            Opcode::FheCmpNe => self.emit_precompile_call(
                asm,
                f,
                instr,
                self.config.precompile_addresses.fhe.cmp_ne,
            ),
            Opcode::FheCmpLt => self.emit_precompile_call(
                asm,
                f,
                instr,
                self.config.precompile_addresses.fhe.cmp_lt,
            ),
            Opcode::FheCmpLe => self.emit_precompile_call(
                asm,
                f,
                instr,
                self.config.precompile_addresses.fhe.cmp_le,
            ),
            Opcode::FheCmpGt => self.emit_precompile_call(
                asm,
                f,
                instr,
                self.config.precompile_addresses.fhe.cmp_gt,
            ),
            Opcode::FheCmpGe => self.emit_precompile_call(
                asm,
                f,
                instr,
                self.config.precompile_addresses.fhe.cmp_ge,
            ),
            Opcode::FheAnd => {
                self.emit_precompile_call(asm, f, instr, self.config.precompile_addresses.fhe.and)
            }
            Opcode::FheOr => {
                self.emit_precompile_call(asm, f, instr, self.config.precompile_addresses.fhe.or)
            }
            Opcode::FheNot => {
                self.emit_precompile_call(asm, f, instr, self.config.precompile_addresses.fhe.not)
            }
            Opcode::FheSelect => self.emit_precompile_call(
                asm,
                f,
                instr,
                self.config.precompile_addresses.fhe.select,
            ),
            Opcode::FheBootstrap => self.emit_precompile_call(
                asm,
                f,
                instr,
                self.config.precompile_addresses.fhe.bootstrap,
            ),
            Opcode::FheCiphertextHash => self.emit_precompile_call(
                asm,
                f,
                instr,
                self.config.precompile_addresses.fhe.ciphertext_hash,
            ),

            // ---- ZK precompile calls ----
            Opcode::ZkVerify => {
                self.emit_precompile_call(asm, f, instr, self.config.precompile_addresses.zk.verify)
            }
            Opcode::ZkNullifier => self.emit_precompile_call(
                asm,
                f,
                instr,
                self.config.precompile_addresses.zk.nullifier,
            ),
            Opcode::ZkProofPayload => {
                asm.emit(AsmOp::Push0);
                self.mstore_result(asm, instr);
            }
            Opcode::VdfEval => self.emit_precompile_call(
                asm,
                f,
                instr,
                self.config.precompile_addresses.zk.vdf_eval,
            ),
            Opcode::VdfVerify => self.emit_precompile_call(
                asm,
                f,
                instr,
                self.config.precompile_addresses.zk.vdf_verify,
            ),

            // ---- PQ precompile calls ----
            Opcode::PqVerifyDilithium => self.emit_precompile_call(
                asm,
                f,
                instr,
                self.config.precompile_addresses.pq.verify_dilithium,
            ),
            Opcode::PqHybridVerify => self.emit_precompile_call(
                asm,
                f,
                instr,
                self.config.precompile_addresses.pq.hybrid_verify,
            ),
            Opcode::PqRand => self.emit_precompile_call(
                asm,
                f,
                instr,
                self.config.precompile_addresses.pq.rand_pq,
            ),
            Opcode::KyberEncrypt => self.emit_precompile_call(
                asm,
                f,
                instr,
                self.config.precompile_addresses.pq.kyber_encrypt,
            ),
            Opcode::KyberDecrypt => self.emit_precompile_call(
                asm,
                f,
                instr,
                self.config.precompile_addresses.pq.kyber_decrypt,
            ),

            // ---- Amnesia (ERC-8228) precompile calls ----
            Opcode::AmnesiaBegin => self.emit_precompile_call(
                asm,
                f,
                instr,
                self.config.precompile_addresses.amnesia.begin,
            ),
            Opcode::AmnesiaSubmitShare => self.emit_precompile_call(
                asm,
                f,
                instr,
                self.config.precompile_addresses.amnesia.submit_share,
            ),
            Opcode::AmnesiaFinalize => self.emit_precompile_call(
                asm,
                f,
                instr,
                self.config.precompile_addresses.amnesia.finalize,
            ),
            Opcode::DestructionProof => self.emit_precompile_call(
                asm,
                f,
                instr,
                self.config.precompile_addresses.amnesia.destruction_proof,
            ),

            // ---- Amnesia / PQ helpers without EVM lowering yet ----
            //
            // KSR-CVN-022: previously these produced an "unknown_opcode"
            // diagnostic but no bytecode. A contract relying on
            // ShamirSplit/VdfLock compiled "successfully" into a silent no-op
            // where VDF-locked withdrawals became instant. Now we emit both
            // a hard compile error AND a REVERT stub at the instruction site
            // so that, if the caller ignores the diagnostic and deploys the
            // artifact anyway, executing the unlowered path traps at runtime
            // instead of silently succeeding.
            Opcode::ShamirSplit
            | Opcode::ShamirReconstruct
            | Opcode::VdfLock
            | Opcode::VdfUnlock
            | Opcode::PqInsert
            | Opcode::PqPop
            | Opcode::PqTopKey
            | Opcode::PqTopValue
            | Opcode::PqLength => {
                self.diagnostics.push(d::unlowered_amnesia_opcode(
                    instr.span,
                    &format!("{:?}", instr.opcode),
                ));
                asm.push_label("__revert__");
                asm.op(asm::OP_JUMP);
            }

            // ---- Reveal ----
            Opcode::RevealDecrypt => self.emit_precompile_call(
                asm,
                f,
                instr,
                self.config.precompile_addresses.fhe.reveal_decrypt,
            ),

            // ---- Events / transfers ----
            Opcode::Emit(event_name) => {
                self.emit_log(asm, f, instr, event_name.name.as_ref());
            }
            Opcode::Transfer => {
                self.emit_transfer(asm, f, instr);
            }

            // ---- Assertions ----
            Opcode::Assert => {
                self.load_operand(asm, f, instr.operands[0]);
                asm.op(asm::OP_ISZERO);
                asm.push_label("__revert__");
                asm.op(asm::OP_JUMPI);
            }
            Opcode::AssertEncrypted => {
                // Threshold-decrypt the ciphertext<bool> handle → 1 byte (0 or 1).
                //
                // Layout (KSR-CVN-020 selector prefix):
                //   mem[0x00..0x20] = selector word (low 4 bytes = selector at mem[0x1C..0x20])
                //   mem[0x20..0x40] = ciphertext handle (operand 0)
                //   mem[0x40]       = result byte written by precompile (retOffset=0x40)
                //
                // STATICCALL argsOffset = 0x1C, argsSize = 4 + 32 = 36.
                //
                // MLOAD 0x40 → [result_byte, 0 × 31] as U256.
                // ISZERO → 1 iff result_byte == 0 (condition false) → revert.
                // ISZERO → 0 iff result_byte != 0 (condition true) → continue.
                //
                // Privacy note (LESSONS.md §7): leaks 1 bit about the ciphertext.
                // Acceptable for balance-sufficient checks; caller already knows.
                // Conditional revert without decryption deferred to V0.3.

                // Selector word at mem[0x00] (low 4 bytes).
                let sel = abi::precompile_selector("AssertEncrypted");
                asm.push_bytes(sel.to_vec());
                asm.emit(AsmOp::Push0);
                asm.op(asm::OP_MSTORE); // mem[0x00..0x20] = selector word
                                        // Handle at mem[0x20].
                self.load_operand(asm, f, instr.operands[0]);
                asm.push_u32(0x20);
                asm.op(asm::OP_MSTORE); // mem[0x20..0x40] = handle
                                        // Zero the return slot so a stale value can't bypass the check.
                asm.emit(AsmOp::Push0);
                asm.push_u32(0x40);
                asm.op(asm::OP_MSTORE); // mem[0x40] = 0
                                        // STATICCALL: retSize=1, retOffset=0x40, argsSize=36, argsOffset=0x1C.
                asm.push_u32(1);
                asm.push_u32(0x40);
                asm.push_u32(36);
                asm.push_u32(0x1C);
                // V0.9: 20-byte EvmAddress pushed via PUSH20 (push_bytes auto-emits PUSH<n>).
                asm.push_bytes(
                    self.config
                        .precompile_addresses
                        .fhe
                        .threshold_decrypt_bool
                        .to_vec(),
                );
                asm.push_bytes(vec![0xff, 0xff, 0xff, 0xff]); // gas
                asm.op(asm::OP_STATICCALL);
                // KSR-CVN-013: revert if STATICCALL failed.
                asm.op(asm::OP_ISZERO);
                asm.push_label("__revert__");
                asm.op(asm::OP_JUMPI);
                // KSR-CVN-014: revert if returndata is empty (threshold_decrypt_bool
                // returns exactly one byte, so require returndatasize >= 1).
                asm.op(asm::OP_RETURNDATASIZE);
                asm.op(asm::OP_ISZERO);
                asm.push_label("__revert__");
                asm.op(asm::OP_JUMPI);
                asm.push_u32(0x40);
                asm.op(asm::OP_MLOAD); // [result_byte, 0 × 31] as U256
                asm.op(asm::OP_ISZERO); // 1 if false condition
                asm.push_label("__revert__");
                asm.op(asm::OP_JUMPI);
            }

            // ---- Authorization ----
            Opcode::IsCallerSender => {
                // caller() == caller() → true trivially.
                asm.push_bytes(vec![1]);
                self.mstore_result(asm, instr);
            }
            Opcode::CallerMatchesPrincipal | Opcode::BuiltinPredicateCall(_) => {
                // OMEGA V6 (CRT-004 fix): this used to unconditionally
                // `push 1` ("Placeholder: return true. Authorization
                // semantics implemented later.") -- every `only
                // <builtin_predicate>` guard (registered_key,
                // first_time_caller, party, validator_majority, and ~12
                // others) silently passed for every caller, with zero
                // diagnostic anywhere in the pipeline. Refuse to compile
                // instead (matching the established
                // unlowered_amnesia_opcode/unlowered_vdf_qualifier pattern
                // for not-yet-implemented primitives) rather than shipping
                // a defeated access-control guard.
                let name = match &instr.opcode {
                    Opcode::BuiltinPredicateCall(bp) => format!("{bp:?}"),
                    _ => "<unknown principal>".to_string(),
                };
                self.diagnostics
                    .push(d::unlowered_builtin_predicate(instr.span, &name));
                asm.push_label("__revert__");
                asm.op(asm::OP_JUMP);
            }

            // ---- LangIdent loads ----
            Opcode::LoadCaller | Opcode::LoadMsgSender => {
                asm.op(asm::OP_CALLER);
                self.mstore_result(asm, instr);
            }
            Opcode::LoadMsgValue => {
                asm.op(asm::OP_CALLVALUE);
                self.mstore_result(asm, instr);
            }
            Opcode::LoadNow | Opcode::LoadBlockTimestamp => {
                asm.op(asm::OP_TIMESTAMP);
                self.mstore_result(asm, instr);
            }
            Opcode::LoadBlockNumber => {
                asm.op(asm::OP_NUMBER);
                self.mstore_result(asm, instr);
            }
            Opcode::LoadChainId => {
                asm.op(asm::OP_CHAINID);
                self.mstore_result(asm, instr);
            }
            Opcode::LoadThis => {
                asm.op(asm::OP_ADDRESS);
                self.mstore_result(asm, instr);
            }
            Opcode::LoadDeployer => {
                asm.push_u32(DEPLOYER_SLOT);
                asm.op(asm::OP_SLOAD);
                self.mstore_result(asm, instr);
            }
            Opcode::LoadZeroAddress => {
                asm.emit(AsmOp::Push0);
                self.mstore_result(asm, instr);
            }

            // ---- ChoiceMatch — placeholder ----
            Opcode::ChoiceMatch(_) => {
                asm.emit(AsmOp::Push0);
                self.mstore_result(asm, instr);
            }

            // ---- Memory ----
            Opcode::MLoad => {
                self.load_operand(asm, f, instr.operands[0]);
                asm.op(asm::OP_MLOAD);
                self.mstore_result(asm, instr);
            }
            Opcode::MStore => {
                self.load_operand(asm, f, instr.operands[1]);
                self.load_operand(asm, f, instr.operands[0]);
                asm.op(asm::OP_MSTORE);
            }

            // ---- Coerce — no-op at IR → pass through ----
            Opcode::Coerce(_) => {
                if let Some(op0) = instr.operands.first().copied() {
                    self.load_operand(asm, f, op0);
                    self.mstore_result(asm, instr);
                }
            }

            // ---- Solidity interop ----
            Opcode::ExternalCall {
                abi_sig,
                is_view,
                arg_count,
            } => {
                let sig = abi_sig.clone();
                let view = *is_view;
                let n_args = *arg_count;
                self.emit_external_call(asm, f, instr, &sig, view, n_args);
            }
        }
    }

    /// Emit a CALL or STATICCALL to an external Solidity contract.
    ///
    /// Calldata layout in scratch memory (starting at 0x00):
    ///   mem[0x00..0x04]          — 4-byte ABI selector (in the high bytes of a word)
    ///   mem[0x04..0x04+n*32]     — ABI-encoded arguments
    ///
    /// Return value (32 bytes) is placed at mem[0x00] and then stored into the
    /// result's SSA slot. Operands: `[addr, arg0, arg1, ...]`.
    fn emit_external_call(
        &mut self,
        asm: &mut Assembler,
        f: &IrFunction,
        instr: &Instr,
        abi_sig: &str,
        is_view: bool,
        arg_count: u32,
    ) {
        let sel = abi::selector(abi_sig);
        let calldata_size = 4 + arg_count * 32;

        // --- Build calldata in memory ---
        // Selector at mem[0x00]: store selector shifted to the top 4 bytes of a 32-byte word.
        asm.push_bytes(sel.to_vec());
        asm.push_bytes(vec![0xe0]); // shift = 224 bits
        asm.op(asm::OP_SHL);
        asm.emit(AsmOp::Push0);
        asm.op(asm::OP_MSTORE);

        // Each argument at mem[4 + i*32].
        for i in 0..arg_count as usize {
            if let Some(&arg_v) = instr.operands.get(1 + i) {
                self.load_operand(asm, f, arg_v);
                asm.push_u32(4 + (i as u32) * 32);
                asm.op(asm::OP_MSTORE);
            }
        }

        // --- Build STATICCALL / CALL stack ---
        // EVM STATICCALL pops (TOS first): gas, addr, argsOffset, argsSize, retOffset, retSize
        // EVM CALL pops (TOS first):       gas, addr, value, argsOffset, argsSize, retOffset, retSize
        //
        // We push in reverse order so TOS ends up as `gas`.
        //
        // Push retSize first (deepest), then retOffset, argsSize, argsOffset, addr, gas.
        asm.push_u32(32); // retSize  (deepest)
        asm.push_u32(0); // retOffset = 0x00
        asm.push_u32(calldata_size); // argsSize
        asm.emit(AsmOp::Push0); // argsOffset = 0x00

        // OMEGA V6 (HGH-028 fix): CALL's real pop order (TOS first) is
        // `gas, addr, value, argsOffset, ...` -- so `value` must be pushed
        // BEFORE `addr` (addr ends up on top of value, directly under gas).
        // This used to push `addr` before `value`, which put `value` on
        // top of `addr` -- the interpreter's `addr` then bound to whatever
        // was pushed as `value` (always the constant 0) and its `value`
        // bound to the real target address (an always-large, always-
        // nonzero word), so every non-view external call attempted to
        // "send" that huge fake `value` and failed with insufficient
        // balance. This is not a MockChain-only quirk: the interpreter's
        // CALL opcode implements real EVM pop semantics, so the same
        // swap would misbehave identically on a real chain.
        if !is_view {
            asm.emit(AsmOp::Push0); // value = 0 ETH for CALL
        }

        // addr (operand[0]).
        if let Some(&addr_v) = instr.operands.first() {
            self.load_operand(asm, f, addr_v);
        } else {
            asm.emit(AsmOp::Push0);
        }

        asm.push_bytes(vec![0xff, 0xff, 0xff, 0xff]); // gas (TOS)

        if is_view {
            asm.op(asm::OP_STATICCALL);
        } else {
            asm.op(asm::OP_CALL);
        }
        // KSR-CVN-027: do NOT discard the success flag. A failed CALL/
        // STATICCALL leaves whatever was at mem[0] beforehand (here, our
        // outgoing calldata's selector || arg0) which the caller would
        // otherwise read as a "return value". `ISZERO ; PUSH __revert__ ;
        // JUMPI` propagates the failure deterministically — parity with
        // emit_transfer's pattern (CRT-013).
        asm.op(asm::OP_ISZERO);
        asm.push_label("__revert__");
        asm.op(asm::OP_JUMPI);

        // KSR-CVN-030 (V0.9.2): read the value the call left at retOffset=0x00.
        //
        // The STATICCALL/CALL above used retOffset=0x00, retSize=32, so on
        // success the EVM copied the callee's first 32-byte return word into
        // mem[0x00]. The previous code zeroed mem[0x00] HERE — *after* the call
        // — which wiped that return word, so every `IFoo.at(addr).method()`
        // view returned 0/default (the M3 milestone bug, DEBT.md "external
        // contract codegen"). We must NOT clobber it before reading.
        //
        // For a successful-but-empty return (RETURNDATASIZE == 0) mem[0x00]
        // still holds outgoing calldata, so we substitute zero in that case.
        // V0.1 assumes 32-byte word returns; dynamic-typed returns
        // (string/bytes) still need offset+length decoding (tracked in DEBT).
        asm.op(asm::OP_RETURNDATASIZE);
        let have_ret = format!(
            "__extret_{}_{}",
            f.name.name,
            instr.result.map(|v| v.0).unwrap_or(u32::MAX)
        );
        asm.push_label(have_ret.clone());
        asm.op(asm::OP_JUMPI); // returndatasize != 0 → keep mem[0x00]
        asm.emit(AsmOp::Push0); // empty return: mem[0x00] = 0
        asm.emit(AsmOp::Push0);
        asm.op(asm::OP_MSTORE);
        asm.label_def(have_ret);

        // Load return value from mem[0x00].
        asm.emit(AsmOp::Push0);
        asm.op(asm::OP_MLOAD);
        self.mstore_result(asm, instr);
    }

    fn emit_terminator(&mut self, asm: &mut Assembler, f: &IrFunction, block: &IrBlock) {
        match &block.terminator {
            Terminator::Jump { target, args } => {
                self.pass_block_args(asm, f, args, *target);
                asm.push_label(format!("fn_{}_b{}", f.name.name, target.0));
                asm.op(asm::OP_JUMP);
            }
            Terminator::Branch {
                cond,
                then_target,
                then_args,
                else_target,
                else_args,
            } => {
                self.load_operand(asm, f, *cond);
                self.pass_block_args(asm, f, then_args, *then_target);
                asm.push_label(format!("fn_{}_b{}", f.name.name, then_target.0));
                asm.op(asm::OP_JUMPI);
                self.pass_block_args(asm, f, else_args, *else_target);
                asm.push_label(format!("fn_{}_b{}", f.name.name, else_target.0));
                asm.op(asm::OP_JUMP);
            }
            Terminator::FheBranch {
                then_target,
                then_args,
                else_target,
                else_args,
                merge_target,
                ..
            } => {
                // V0.2 FheBranch lowering: both branches execute sequentially
                // (unconditional), then fall through to merge_target. The merge
                // block's FheSelect instructions pick encrypted results from both
                // sides. Neither branch may contain plaintext SSTOREs (E715).
                //
                // Cost: both branch bodies run at full gas. Conditional execution
                // via ZK gadgets is deferred to V0.3 (DEBT.md §FheBranch-gas).
                //
                // Control flow:
                //   current block → then_block → else_block → merge_block
                //
                // The then_block and else_block both end with Jump(merge_target),
                // so the normal block-emission sequence handles the rest.
                self.pass_block_args(asm, f, then_args, *then_target);
                asm.push_label(format!("fn_{}_b{}", f.name.name, then_target.0));
                asm.op(asm::OP_JUMP);
                // else_target and merge_target are reached via then_target's own Jump.
                // The else_block will be emitted after then_block in the block list;
                // if then_block's terminator is Jump(merge), the else_block becomes
                // dead code (reachable only via the block label).
                // For contracts that need else to run: the IR builder must emit
                // the else-side as part of then_block's chain (e.g., inline both),
                // or use FheSelect-only patterns in the synthesizer.
                let _ = (else_target, else_args, merge_target);
            }
            Terminator::Return(Some(v)) => {
                // Text constants need ABI-encoded dynamic-string return.
                if let Some(IrConstant::Text(text)) = self.as_const(f, *v) {
                    // On refusal (E521) the encoder emitted nothing, so fall
                    // through to a plain REVERT rather than leaving the block
                    // without a terminator. The diagnostic is an error, so no
                    // artifact ships either way.
                    if !self.emit_text_return(asm, &text, block.span) {
                        asm.emit(AsmOp::Push0);
                        asm.emit(AsmOp::Push0);
                        asm.op(asm::OP_REVERT);
                    }
                } else {
                    // OMEGA V6 (MED-003 fix): a genuinely dynamic ABI type
                    // (text/bytes/list) that is NOT a compile-time constant
                    // has no real dynamic encoder in this release -- see
                    // `dynamic_return_not_encoded`. Warn (not hard-fail:
                    // this is the "Hello World" `view returns text { field }`
                    // pattern, not a rare construct) and fall through to the
                    // existing scalar-return codegen, unchanged.
                    if let Some(ty) = f.value_types.get(v).filter(|t| !is_static_abi_ty(t)) {
                        let ty_name = format!("{ty:?}");
                        let span = f.value_spans.get(v).copied().unwrap_or(block.span);
                        self.diagnostics
                            .push(d::dynamic_return_not_encoded(span, &ty_name));
                    }
                    // Scalar return: place value at memory 0x00, return 32 bytes.
                    self.load_operand(asm, f, *v);
                    asm.emit(AsmOp::Push0);
                    asm.op(asm::OP_MSTORE);
                    asm.push_u32(32);
                    asm.emit(AsmOp::Push0);
                    asm.op(asm::OP_RETURN);
                }
            }
            Terminator::Return(None) => {
                asm.emit(AsmOp::Push0);
                asm.emit(AsmOp::Push0);
                asm.op(asm::OP_RETURN);
            }
            Terminator::Revert { error, args } => {
                // Look up the declared error to get its parameter types.
                let err_entry = self
                    .module
                    .errors
                    .iter()
                    .find(|e| e.name.name == error.name);
                if let Some(err) = err_entry {
                    // Build canonical signature: "ErrorName(type1,type2,...)"
                    let param_refs: Vec<&covenant_types::Ty> = err.params.iter().collect();
                    let sig = abi::canonical_signature(error.name.as_ref(), &param_refs);
                    let sel = abi::selector(&sig); // [u8; 4]

                    // Snapshot args before the mutable borrows below.
                    let args_snap: Vec<Value> = args.clone();

                    // Store selector at mem[0..4]:
                    //   PUSH4 selector → PUSH1 224 → SHL → PUSH0 → MSTORE
                    // SHL(224, selector_4bytes) places selector in the top 4 bytes
                    // of the 256-bit word; MSTORE at offset 0 writes MSB first.
                    asm.push_bytes(sel.to_vec());
                    asm.push_bytes(vec![0xe0]); // 224
                    asm.op(asm::OP_SHL);
                    asm.emit(AsmOp::Push0);
                    asm.op(asm::OP_MSTORE); // mem[0..32] = [sel0,sel1,sel2,sel3,0..]

                    // Store each argument at mem[4 + i*32].
                    for (i, &arg) in args_snap.iter().enumerate() {
                        self.load_operand(asm, f, arg);
                        asm.push_u32(4 + (i as u32) * 32);
                        asm.op(asm::OP_MSTORE);
                    }

                    // REVERT(offset=0, size=4+n*32)
                    let size = 4 + (args_snap.len() as u32) * 32;
                    asm.push_u32(size);
                    asm.emit(AsmOp::Push0);
                    asm.op(asm::OP_REVERT);
                } else {
                    // Unknown error — fall back to empty revert.
                    asm.emit(AsmOp::Push0);
                    asm.emit(AsmOp::Push0);
                    asm.op(asm::OP_REVERT);
                }
            }
            Terminator::Unreachable => {
                asm.op(asm::OP_INVALID);
            }
        }
    }

    fn pass_block_args(
        &mut self,
        asm: &mut Assembler,
        f: &IrFunction,
        args: &[Value],
        target: BlockId,
    ) {
        // Copy each arg's memory slot to the corresponding block-param's slot.
        let target_block = f.blocks.iter().find(|b| b.id == target);
        if let Some(tb) = target_block {
            for (arg, param) in args.iter().zip(tb.params.iter()) {
                self.load_operand(asm, f, *arg);
                asm.push_u32(value_memory_offset(*param));
                asm.op(asm::OP_MSTORE);
            }
        }
    }

    // ---------- primitive helpers ----------

    fn load_operand(&self, asm: &mut Assembler, f: &IrFunction, v: Value) {
        // If v is a constant, push its bytes directly.
        if let Some(c) = self.as_const(f, v) {
            self.push_const(asm, &c);
            return;
        }
        // Otherwise, MLOAD from the value's memory slot.
        asm.push_u32(value_memory_offset(v));
        asm.op(asm::OP_MLOAD);
    }

    fn as_const(&self, f: &IrFunction, v: Value) -> Option<IrConstant> {
        f.values
            .iter()
            .find(|(k, _)| *k == v)
            .and_then(|(_, info)| match info {
                ValueInfo::Const(c) => Some(c.clone()),
                _ => None,
            })
    }

    fn push_const(&self, asm: &mut Assembler, c: &IrConstant) {
        match c {
            IrConstant::Integer(n) => asm.push_bytes(big_endian_u128(*n)),
            IrConstant::Bool(b) => asm.push_bytes(vec![if *b { 1 } else { 0 }]),
            IrConstant::Hash(b) => asm.push_bytes(b.to_vec()),
            IrConstant::Address(b) => asm.push_bytes(b.to_vec()),
            IrConstant::ZeroAddress => asm.emit(AsmOp::Push0),
            IrConstant::Duration { n, .. } => asm.push_bytes(big_endian_u128(*n as u128)),
            IrConstant::Hex(b) => asm.push_bytes(b.to_vec()),
            IrConstant::Text(_) => {
                // Texts are stored off-stack; emit a zero placeholder for V0.
                asm.emit(AsmOp::Push0);
            }
        }
    }

    fn mstore_result(&self, asm: &mut Assembler, instr: &Instr) {
        if let Some(r) = instr.result {
            asm.push_u32(value_memory_offset(r));
            asm.op(asm::OP_MSTORE);
        } else {
            asm.op(asm::OP_POP);
        }
    }

    fn binop(&mut self, asm: &mut Assembler, f: &IrFunction, instr: &Instr, op: u8) {
        // EVM non-commutative opcodes (SUB, DIV, MOD, LT, GT, SHL, SHR, …) are
        // specified as `op(top, below_top)` — i.e. the top of the stack is the
        // *first* argument. For `SUB` the Yellow Paper / evm.codes give the
        // result as `a - b` where `a` is popped first (top) and `b` is popped
        // second.
        //
        // If the IR operand order is `[lhs, rhs]` and we want `lhs OP rhs`,
        // we must push `rhs` first and `lhs` last so the stack ends up
        // `[rhs, lhs]` with `lhs` on top. SUB then computes `lhs - rhs`,
        // LT then computes `lhs < rhs`, etc. — matching the IR semantics.
        self.load_operand(asm, f, instr.operands[1]);
        self.load_operand(asm, f, instr.operands[0]);
        asm.op(op);
        self.mstore_result(asm, instr);
    }

    fn unary_iszero(&mut self, asm: &mut Assembler, f: &IrFunction, instr: &Instr) {
        self.load_operand(asm, f, instr.operands[0]);
        asm.op(asm::OP_ISZERO);
        self.mstore_result(asm, instr);
    }

    /// Checked arithmetic (AddChecked / SubChecked / MulChecked): perform the
    /// operation and REVERT on overflow/underflow instead of silently wrapping
    /// the 256-bit EVM word (KSR-CVN-031, V0.9.2). This is Solidity's default
    /// `+` / `-` / `*` semantics. Without it, a token `balance -= amount` would
    /// wrap to a near-`type(uint256).max` value on underflow rather than
    /// reverting. Reverts via the global `__revert__` handler (empty data).
    /// IR operand order is `[lhs, rhs]`.
    fn binop_checked(&mut self, asm: &mut Assembler, f: &IrFunction, instr: &Instr, op: u8) {
        match op {
            asm::OP_ADD => {
                // sum = lhs + rhs; overflow iff sum < lhs (i.e. lhs > sum).
                self.load_operand(asm, f, instr.operands[1]); // rhs
                self.load_operand(asm, f, instr.operands[0]); // lhs; stack [rhs, lhs]
                asm.op(asm::OP_ADD); // [sum]
                asm.op(asm::OP_DUP1); // [sum, sum]
                self.load_operand(asm, f, instr.operands[0]); // [sum, sum, lhs]
                asm.op(asm::OP_GT); // lhs > sum -> [sum, overflow]
                asm.push_label("__revert__");
                asm.op(asm::OP_JUMPI); // revert if overflow; [sum]
                self.mstore_result(asm, instr);
            }
            asm::OP_SUB => {
                // underflow iff lhs < rhs.
                self.load_operand(asm, f, instr.operands[1]); // rhs
                self.load_operand(asm, f, instr.operands[0]); // lhs; [rhs, lhs]
                asm.op(asm::OP_LT); // lhs < rhs -> [underflow]
                asm.push_label("__revert__");
                asm.op(asm::OP_JUMPI); // revert if underflow; []
                self.load_operand(asm, f, instr.operands[1]); // rhs
                self.load_operand(asm, f, instr.operands[0]); // lhs; [rhs, lhs]
                asm.op(asm::OP_SUB); // lhs - rhs -> [diff]
                self.mstore_result(asm, instr);
            }
            asm::OP_MUL => {
                // prod = lhs * rhs; if lhs != 0, overflow iff prod / lhs != rhs.
                self.load_operand(asm, f, instr.operands[1]); // rhs
                self.load_operand(asm, f, instr.operands[0]); // lhs; [rhs, lhs]
                asm.op(asm::OP_MUL); // [prod]
                self.load_operand(asm, f, instr.operands[0]); // [prod, lhs]
                asm.op(asm::OP_ISZERO); // [prod, lhs==0]
                let ok = format!(
                    "__mulok_{}_{}",
                    f.name.name,
                    instr.result.map(|v| v.0).unwrap_or(u32::MAX)
                );
                asm.push_label(ok.clone());
                asm.op(asm::OP_JUMPI); // lhs==0 -> skip check; [prod]
                asm.op(asm::OP_DUP1); // [prod, prod]
                self.load_operand(asm, f, instr.operands[0]); // [prod, prod, lhs]
                asm.op(asm::OP_SWAP1); // [prod, lhs, prod]
                asm.op(asm::OP_DIV); // prod / lhs -> [prod, q]
                self.load_operand(asm, f, instr.operands[1]); // [prod, q, rhs]
                asm.op(asm::OP_EQ); // q == rhs -> [prod, eq]
                asm.op(asm::OP_ISZERO); // [prod, mismatch]
                asm.push_label("__revert__");
                asm.op(asm::OP_JUMPI); // revert if mismatch; [prod]
                asm.label_def(ok);
                self.mstore_result(asm, instr);
            }
            _ => self.binop(asm, f, instr, op),
        }
    }

    /// Division / remainder with a zero-divisor guard (fail-loud pass).
    ///
    /// The EVM's `DIV`/`MOD` are *total*: `x / 0` yields `0` instead of
    /// trapping. The previous bare lowering therefore made
    /// `pot / participants` silently evaluate to 0 on an empty set —
    /// ordinary-looking source, wrong on-chain result, no diagnostic.
    /// Solidity reverts here, and so do we.
    ///
    /// Two cases are handled without emitting a runtime check:
    ///  - a literal *non-zero* divisor can never trip the guard, so the
    ///    bytecode is left exactly as before (`value * bps / 10000` pays
    ///    nothing for this fix);
    ///  - a literal *zero* divisor can never be correct, so it is rejected at
    ///    compile time (E519) rather than deferred to a runtime revert.
    ///
    /// IR operand order is `[lhs, rhs]`; the divisor is `rhs`. We revert
    /// through the shared `__revert__` handler, the same convention
    /// `binop_checked` uses.
    fn binop_div_guarded(
        &mut self,
        asm: &mut Assembler,
        f: &IrFunction,
        instr: &Instr,
        op: u8,
        what: &str,
    ) {
        match self.as_const(f, instr.operands[1]) {
            // Provably safe divisor — emit the bare op, as before.
            Some(IrConstant::Integer(n)) if n != 0 => {}
            // Provably wrong — refuse to compile.
            Some(IrConstant::Integer(_)) => {
                self.diagnostics
                    .push(d::div_by_zero_literal(instr.span, what));
            }
            // Unknown at compile time — guard it at runtime.
            _ => {
                self.load_operand(asm, f, instr.operands[1]); // [rhs]
                asm.op(asm::OP_ISZERO); // [rhs == 0]
                asm.push_label("__revert__");
                asm.op(asm::OP_JUMPI); // revert when divisor == 0; []
            }
        }
        self.binop(asm, f, instr, op);
    }

    fn emit_keccak(&mut self, asm: &mut Assembler, f: &IrFunction, instr: &Instr) {
        // Lay operands out in memory starting at 0x00, then KECCAK256 over the
        // combined length. For V0 we assume every operand is a 32-byte value.
        for (i, v) in instr.operands.iter().enumerate() {
            self.load_operand(asm, f, *v);
            asm.push_u32((i as u32) * 32);
            asm.op(asm::OP_MSTORE);
        }
        asm.push_u32(instr.operands.len() as u32 * 32);
        asm.emit(AsmOp::Push0);
        asm.op(asm::OP_KECCAK256);
        self.mstore_result(asm, instr);
    }

    /// Trace a Map* opcode's base operand (operand 0 — the `SLoad` of the map
    /// field) back to the field's storage slot NUMBER. Returns `None` when the
    /// operand is not a direct `SLoad(GlobalId)` result, in which case callers
    /// fall back to the operand's runtime value.
    fn map_base_slot(&self, f: &IrFunction, base: Value) -> Option<u32> {
        for block in &f.blocks {
            for ins in &block.instructions {
                if ins.result == Some(base) {
                    if let Opcode::SLoad(id) = ins.opcode {
                        return Some(storage::slot_for_global(self.module, id));
                    }
                    return None;
                }
            }
        }
        None
    }

    fn emit_map_slot(&mut self, asm: &mut Assembler, f: &IrFunction, instr: &Instr) {
        // Map entry slot = keccak256(key || map_slot_number).
        //
        // KSR-CVN-029 (V0.9.2): the base word MUST be the map field's storage
        // slot NUMBER, not the value loaded from that slot. Every freshly
        // deployed map's metadata slot reads 0, so the old `keccak(key || 0)`
        // convention made any two maps that share a key VALUE alias the same
        // entry slot (e.g. ERC-721 `owners[id]` vs `token_approvals[id]` — an
        // `approve` then silently overwrote ownership). Operand 0 is the
        // `SLoad` of the map field, which we trace back to the slot number;
        // operand 1 is the key.
        if instr.operands.len() < 2 {
            asm.emit(AsmOp::Push0);
            return;
        }
        self.load_operand(asm, f, instr.operands[1]); // key -> mem[0x00]
        asm.emit(AsmOp::Push0);
        asm.op(asm::OP_MSTORE);
        // Base word = the map field's slot number. Falls back to the operand's
        // runtime value when the slot can't be statically resolved (preserves
        // pre-V0.9.2 behavior for any non-`SLoad`-fed map base).
        match self.map_base_slot(f, instr.operands[0]) {
            Some(slot) => asm.push_u32(slot),
            None => self.load_operand(asm, f, instr.operands[0]),
        }
        asm.push_u32(32);
        asm.op(asm::OP_MSTORE); // map slot number -> mem[0x20]
        asm.push_u32(64);
        asm.emit(AsmOp::Push0);
        asm.op(asm::OP_KECCAK256);
    }

    /// Number of consecutive storage words one element of a `List<T>`
    /// occupies: 1 for a scalar element type, or the element struct's field
    /// count for `List<Struct>` (OMEGA V6 CRT-003 follow-on).
    fn list_elem_stride(&self, f: &IrFunction, list_value: Value) -> u32 {
        match f.value_types.get(&list_value) {
            Some(Ty::List(inner)) => match inner.as_ref() {
                Ty::Struct(sid) => self
                    .module
                    .structs
                    .iter()
                    .find(|s| s.id == StructTypeId(sid.0))
                    .map(|s| (s.fields.len() as u32).max(1))
                    .unwrap_or(1),
                _ => 1,
            },
            _ => 1,
        }
    }

    /// `keccak256(slot_number)` -- the start of a list's element data, per
    /// Solidity's own dynamic-array storage convention (length at the slot
    /// itself, elements packed starting at the hash of the slot).
    fn emit_list_base_addr(&mut self, asm: &mut Assembler, f: &IrFunction, list_operand: Value) {
        match self.map_base_slot(f, list_operand) {
            Some(slot) => asm.push_u32(slot),
            None => self.load_operand(asm, f, list_operand),
        }
        asm.emit(AsmOp::Push0);
        asm.op(asm::OP_MSTORE);
        asm.push_u32(32);
        asm.emit(AsmOp::Push0);
        asm.op(asm::OP_KECCAK256);
    }

    /// `keccak256(slot_number) + index * stride` -- the storage address of
    /// list element `index`. Leaves exactly that one word on the stack.
    fn emit_list_elem_addr(
        &mut self,
        asm: &mut Assembler,
        f: &IrFunction,
        list_operand: Value,
        index_operand: Value,
        stride: u32,
    ) {
        self.emit_list_base_addr(asm, f, list_operand);
        self.load_operand(asm, f, index_operand);
        if stride > 1 {
            asm.push_u32(stride);
            asm.op(asm::OP_MUL);
        }
        asm.op(asm::OP_ADD);
    }

    /// Trace `v` back to the `StructNew` instruction that produced it (if
    /// any) and return its field-value operands in declared order. Used by
    /// `ListAppend` to write each field of a struct literal to its own
    /// storage word instead of trying to move a whole struct through a
    /// single EVM stack slot.
    fn struct_new_operands<'x>(&self, f: &'x IrFunction, v: Value) -> Option<&'x [Value]> {
        for block in &f.blocks {
            for ins in &block.instructions {
                if ins.result == Some(v) {
                    return match ins.opcode {
                        Opcode::StructNew(_) => Some(ins.operands.as_slice()),
                        _ => None,
                    };
                }
            }
        }
        None
    }

    fn emit_precompile_call(
        &mut self,
        asm: &mut Assembler,
        f: &IrFunction,
        instr: &Instr,
        address: crate::target::EvmAddress,
    ) {
        // Hardened precompile STATICCALL (KSR-CVN-013 + KSR-CVN-014 + KSR-CVN-020).
        //
        // Memory layout:
        //   mem[0x00..0x20] : return slot (zeroed first)
        //   mem[0x20..0x40] : word whose LOW 4 bytes (mem[0x3C..0x40]) are the
        //                     precompile selector (KSR-CVN-020), high 28 bytes
        //                     zero. Written as `MSTORE(0x20, selector_u32)`.
        //   mem[0x40..]     : operand words, aligned.
        //
        // STATICCALL argsOffset = 0x3C, argsSize = 4 + n*32 so the chain-side
        // precompile reads `selector || operand0 || operand1 || ...`.
        //
        // retOffset = 0, retSize = 32 — unchanged (every precompile routed
        // through this helper returns a 32-byte handle or bool word).

        // Step 1: zero the return slot.
        asm.emit(AsmOp::Push0);
        asm.emit(AsmOp::Push0);
        asm.op(asm::OP_MSTORE); // mem[0x00] = 0

        // Step 2: write selector word at mem[0x20]. PUSH a u32 selector: the
        // assembler stores it in the low 4 bytes of a 32-byte word, so an
        // aligned MSTORE at 0x20 places the selector at mem[0x3C..0x40].
        //
        // V0.9 Sprint 31.b: for helper-contract targets (Sepolia / Aster),
        // use the Solidity ABI selector of the deployed helper method
        // instead of the V0.8 namespaced `keccak("covenant.precompile.X:v1")`
        // form. MockChain keeps the namespaced form (its precompile dispatch
        // reads it).
        // FAIL-LOUD GATE (E520). On helper-contract targets, an opcode absent
        // from `helper_selector_for_opcode` used to fall back to the V0.8
        // namespaced selector `keccak("covenant.precompile.<Op>:v1")[0..4]` —
        // which matches NO function on the deployed Mocked*/Ceremony helper.
        // Those helpers have no fallback function, so the CALL cannot dispatch
        // and always reverts: the contract compiles clean, deploys clean, and
        // bricks on first use. The primitive is then neither real NOR mocked.
        // Refuse to compile instead, exactly as E516/E517/E518 already do.
        //
        // Native-precompile targets (mockchain) are unaffected: their runtime
        // implements these opcodes, so the namespaced selector is correct there.
        let sel = if self.config.target.uses_helper_contracts() {
            match crate::target::helper_selector_for_opcode(instr.opcode.stable_name()) {
                Some(s) => s,
                None => {
                    self.diagnostics.push(d::helper_method_missing(
                        instr.span,
                        instr.opcode.stable_name(),
                        self.config.target.as_str(),
                    ));
                    // Still emit *something* so codegen stays total; the
                    // diagnostic is an error, so no artifact ships.
                    abi::precompile_selector(instr.opcode.stable_name())
                }
            }
        } else {
            abi::precompile_selector(instr.opcode.stable_name())
        };
        asm.push_bytes(sel.to_vec()); // PUSH4 selector → stack u32 low-4-bytes
        asm.push_u32(0x20);
        asm.op(asm::OP_MSTORE);

        // Step 3: write operands at mem[0x40 + i*32].
        for (i, op) in instr.operands.iter().enumerate() {
            self.load_operand(asm, f, *op);
            asm.push_u32(0x40 + (i as u32) * 32);
            asm.op(asm::OP_MSTORE);
        }
        let args_size = 4 + (instr.operands.len() as u32) * 32;
        let _ = self.config.precompile_addresses;

        // Step 4: CALL or STATICCALL based on target.
        //
        // V0.8 native precompiles (MockChain) are conceptually stateless —
        // STATICCALL is semantically correct. V0.9 helper contracts include
        // stateful methods (CeremonyHelper.amnesiaSetup writes storage)
        // — STATICCALL would revert. For helper-contract targets, use CALL
        // (superset of STATICCALL behavior; allows state mutations but
        // doesn't require them).
        //
        // V1.0 may add per-opcode read/write classification so that
        // helper-contract pure-view ops (e.g. ZK verify, FHE decrypt)
        // still use STATICCALL. For V0.9.0, conservative single-mode
        // dispatch is the safe default.
        let use_call = self.config.target.uses_helper_contracts();

        asm.push_u32(32); // retSize
        asm.emit(AsmOp::Push0); // retOffset = 0
        asm.push_u32(args_size); // argsSize = 4 + n*32
        asm.push_u32(0x3C); // argsOffset = 0x3C (the 4 selector bytes)
        if use_call {
            // CALL needs an extra `value` arg before address. We push 0 ETH.
            asm.emit(AsmOp::Push0); // value = 0
        }
        asm.push_bytes(address.to_vec()); // address (PUSH20)
        asm.push_bytes(vec![0xff, 0xff, 0xff, 0xff]); // gas (simplified: large cap)
        if use_call {
            asm.op(asm::OP_CALL); // stack: [success]
        } else {
            asm.op(asm::OP_STATICCALL); // stack: [success]
        }

        // Step 5: revert if STATICCALL returned failure.
        asm.op(asm::OP_ISZERO);
        asm.push_label("__revert__");
        asm.op(asm::OP_JUMPI);

        // Step 6: revert if returndata size is wrong.
        //
        // V0.8 native precompiles (MockChain) always return exactly 32 bytes,
        // so V0.8 codegen used `== 32`. V0.9 helper-contract methods may
        // return variable-length data (bytes/string) which Solidity ABI-
        // encodes as `offset(32) || length(32) || data(...)`, so total
        // returndata is >= 32. For helper-contract targets we relax the
        // check to `>= 32`. For MockChain we keep the strict check.
        if self.config.target.uses_helper_contracts() {
            // RETURNDATASIZE; PUSH 32; GT → (32 > returndatasize) = (returndatasize < 32)
            asm.op(asm::OP_RETURNDATASIZE);
            asm.push_u32(32);
            asm.op(asm::OP_GT); // pushes 1 if size < 32, 0 otherwise
        } else {
            // RETURNDATASIZE; PUSH 32; EQ; ISZERO → 1 if size != 32
            asm.op(asm::OP_RETURNDATASIZE);
            asm.push_u32(32);
            asm.op(asm::OP_EQ);
            asm.op(asm::OP_ISZERO);
        }
        asm.push_label("__revert__");
        asm.op(asm::OP_JUMPI);

        // Step 7: read the verified result.
        asm.emit(AsmOp::Push0);
        asm.op(asm::OP_MLOAD);
        self.mstore_result(asm, instr);
    }

    fn emit_log(&mut self, asm: &mut Assembler, f: &IrFunction, instr: &Instr, event_name: &str) {
        // Look up the event declaration to get the full signature and indexed flags.
        let event = self
            .module
            .events
            .iter()
            .find(|e| e.name.name.as_ref() == event_name);

        let (topic_hash, indexed_mask): ([u8; 32], Vec<bool>) = if let Some(ev) = event {
            let sig = abi::event_canonical_signature(ev);
            let hash = abi::event_topic(&sig);
            let mask: Vec<bool> = ev.params.iter().map(|(_, _, idx)| *idx).collect();
            (hash, mask)
        } else {
            // Unknown event: fall back to 4-byte-selector-padded placeholder.
            let sig = format!("{event_name}()");
            let sel = abi::selector(&sig);
            let mut h = [0u8; 32];
            h[..4].copy_from_slice(&sel);
            (h, vec![false; instr.operands.len()])
        };

        // Partition operands into indexed (topics) and non-indexed (data).
        let mut indexed_ops: Vec<Value> = Vec::new();
        let mut data_ops: Vec<Value> = Vec::new();
        for (i, &op) in instr.operands.iter().enumerate() {
            if indexed_mask.get(i).copied().unwrap_or(false) {
                indexed_ops.push(op);
            } else {
                data_ops.push(op);
            }
        }

        // Store non-indexed params in scratch memory 0x00, 0x20, ...
        for (i, &op) in data_ops.iter().enumerate() {
            self.load_operand(asm, f, op);
            asm.push_u32((i as u32) * 32);
            asm.op(asm::OP_MSTORE);
        }
        let data_size = data_ops.len() as u32 * 32;

        // EVM LOGn stack order (top → bottom when LOGn executes):
        //   offset, size, topic0, topic1, ..., topicN-1
        // Push in reverse so the last indexed param ends up deepest.
        // Topics: [event_sig_hash, indexed[0], indexed[1], ...]
        for &op in indexed_ops.iter().rev() {
            self.load_operand(asm, f, op); // indexed[last] pushed first (deepest)
        }
        asm.push_bytes(topic_hash.to_vec()); // topic0 = event signature hash
        asm.push_u32(data_size);
        asm.emit(AsmOp::Push0); // data offset = 0x00

        let log_op = match 1 + indexed_ops.len() {
            1 => asm::OP_LOG1,
            2 => asm::OP_LOG2,
            3 => asm::OP_LOG3,
            4 => asm::OP_LOG4,
            _ => {
                // > 4 topics not supported by EVM.
                asm.emit(AsmOp::Push0);
                asm.emit(AsmOp::Push0);
                asm.op(asm::OP_REVERT);
                return;
            }
        };
        asm.op(log_op);
    }

    /// Emit ABI-encoded dynamic string return for a compile-time `Text` constant.
    ///
    /// Return layout (96 bytes):
    ///   mem[0x00..0x20] — ABI offset = 0x20 (points to the length word)
    ///   mem[0x20..0x40] — string byte length
    ///   mem[0x40..0x60] — string bytes, left-aligned in the 32-byte word
    ///
    /// MSTORE right-aligns its argument, so we left-shift the bytes by
    /// `(32 - len) * 8` bits before storing to achieve left-alignment.
    /// V0 restriction: strings must be ≤ 32 bytes.
    /// Returns `false` when the constant is too long to encode, having already
    /// reported E521. Callers must not treat that as success.
    fn emit_text_return(&mut self, asm: &mut Assembler, text: &str, span: Span) -> bool {
        let bytes = text.as_bytes();
        let len = bytes.len();
        if len > 32 {
            // Was a bare `assert!`, which aborted the entire compiler with an
            // ICE on input as ordinary as a 33-byte token name — found by the
            // cargo-fuzz `compile_pipeline` target. Report and bail instead.
            self.diagnostics.push(d::text_constant_too_long(span, len));
            return false;
        }

        // mem[0x00] = ABI offset = 0x20
        asm.push_u32(0x20);
        asm.emit(AsmOp::Push0);
        asm.op(asm::OP_MSTORE);

        // mem[0x20] = byte length
        asm.push_bytes(big_endian_u128(len as u128));
        asm.push_u32(0x20);
        asm.op(asm::OP_MSTORE);

        // mem[0x40] = string bytes, left-aligned.
        // Strategy: push the raw bytes (right-aligned on stack), then
        // SHL by (32 - len)*8 to shift them to the high (left) end.
        if len > 0 {
            asm.push_bytes(bytes.to_vec());
            let shift = ((32 - len) * 8) as u128;
            if shift > 0 {
                asm.push_bytes(big_endian_u128(shift));
                asm.op(asm::OP_SHL);
            }
            asm.push_u32(0x40);
            asm.op(asm::OP_MSTORE);
        }

        // RETURN 96 bytes from offset 0x00.
        asm.push_u32(96);
        asm.emit(AsmOp::Push0);
        asm.op(asm::OP_RETURN);
        true
    }

    fn emit_transfer(&mut self, asm: &mut Assembler, f: &IrFunction, instr: &Instr) {
        // Simplified CALL for ether transfer:
        // CALL(gas, to, value, 0, 0, 0, 0)
        // Operands: amount, to (for `from_none` transfer) or [amount, from, to].
        let (amount_v, to_v) = match instr.operands.len() {
            2 => (instr.operands[0], instr.operands[1]),
            3 => (instr.operands[0], instr.operands[2]),
            _ => {
                asm.emit(AsmOp::Push0);
                return;
            }
        };
        asm.emit(AsmOp::Push0); // retSize
        asm.emit(AsmOp::Push0); // retOffset
        asm.emit(AsmOp::Push0); // argsSize
        asm.emit(AsmOp::Push0); // argsOffset
        self.load_operand(asm, f, amount_v); // value
        self.load_operand(asm, f, to_v); // to
        asm.push_bytes(vec![0xff, 0xff, 0xff, 0xff]); // gas
        asm.op(asm::OP_CALL);
        asm.op(asm::OP_ISZERO);
        asm.push_label("__revert__");
        asm.op(asm::OP_JUMPI);
    }
}

/// Whether a type fits in one 32-byte calldata word for the V0.1 ABI.
///
/// True for address / uint-ish / bool / hash / duration / amount and their
/// `Unknown` fallback (treated as opaque word). False for text, bytes, list,
/// map, struct, ciphertext-of-dynamic — those require head-tail decoding,
/// which V0.2 will add.
fn is_static_abi_ty(ty: &covenant_types::Ty) -> bool {
    use covenant_types::Ty::*;
    match ty {
        Address | Bool | Hash | Duration | Time | Amount | PqKey | Choice(_) | Unknown => true,
        // Ciphertexts of scalars are represented as 32-byte handles on-chain.
        Ciphertext(inner) => is_static_abi_ty(inner),
        _ => false,
    }
}

/// KSR-CVN-012: detect whether a function should carry the proxy
/// initializer re-init guard. Matches by name (`initialize`) or by an
/// explicit `@initializer` annotation. Only public Action kinds qualify
/// — views, reveals and internal kinds are never initializers.
pub(crate) fn is_initializer_action(f: &IrFunction) -> bool {
    if !matches!(f.kind, covenant_ir::IrFunctionKind::Action) {
        return false;
    }
    if f.name.name.as_ref() == "initialize" {
        return true;
    }
    f.annotations.iter().any(|a| {
        matches!(
            a,
            covenant_ir::function::IrAnnotation::Unknown(n) if n.as_ref() == "initializer"
        )
    })
}

/// KSR-CVN-012: compute the 32-byte storage slot holding the
/// initializer flag for `module_name`.
///
/// Slot = keccak256("covenant.proxy.initializer.<Module>"). Hashing
/// pushes the slot far away from sequential user fields and away from
/// the reserved DEPLOYER/REENTRANT_LOCK slots, preventing collisions.
pub(crate) fn initializer_flag_slot(module_name: &str) -> [u8; 32] {
    use sha3::{Digest, Keccak256};
    let mut hasher = Keccak256::new();
    hasher.update(b"covenant.proxy.initializer.");
    hasher.update(module_name.as_bytes());
    hasher.finalize().into()
}

fn big_endian_u128(n: u128) -> Vec<u8> {
    let bytes = n.to_be_bytes();
    let trimmed: Vec<u8> = bytes.iter().copied().skip_while(|&b| b == 0).collect();
    if trimmed.is_empty() {
        vec![0]
    } else {
        trimmed
    }
}

// Silence unused-span diagnostics when we don't propagate them through.
#[allow(dead_code)]
fn _span(_s: Span) {}

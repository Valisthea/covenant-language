//! ERC-8231 (post-quantum key registry) interface synthesis.
//!
//! Synthesizes the ERC-8231 surface into an `IrModule` whose construct
//! kind is `Registry`. The standard provides a per-address binding from
//! an account to its post-quantum (Dilithium-5 by default) public key,
//! plus revocation + key-rotation flows.
//!
//! Surface synthesized:
//!   Functions (5):
//!     view  is_registered(account: address) returns bool
//!     view  key_of(account: address) returns pq_key
//!     view  algorithm_id() returns amount     (always 1 in V0.9 = Dilithium-5)
//!     action register(pk: pq_key)
//!     action revoke()
//!
//!   Events (3): KeyRegistered, KeyUpdated, KeyRevoked
//!   Errors (2): NotRegistered, AlreadyRegistered
//!
//! V0.9.0 design choices:
//!   - `update_key(new_pk, sig)` (the PQ-signature-protected rotation
//!     flow) is documented in the spec but NOT synthesized in V0.9 because
//!     it depends on the `pq_signed` guard surface which V0.9.0 does not
//!     yet expose to stdlib synthesis. Sprint 35.c will add it. Until
//!     then, users wanting a rotation flow declare `update_key` by hand
//!     and the synthesizer respects the conflict (skips its own
//!     synthesis of any user-declared name).
//!   - `algorithm_id()` returns 1 (Dilithium-5 per FIPS 204). V1.0 may
//!     add 2 (Falcon-512), 3 (SPHINCS+), etc.
//!   - `register` is open-access — anyone can register their own key.
//!     `revoke` clears the caller's registration only.

use std::collections::HashSet;

use covenant_diag::{Diagnostic, Span};
use covenant_ir::{
    function::IrFunctionKind,
    id::{FunctionId, GlobalId, Value},
    instr::{IrConstant, Terminator},
    module::{IrEvent, IrField},
    IrError, IrModule, Opcode,
};
use covenant_parser::ast::Ident;
use covenant_privacy::PrivacyDomain;
use covenant_types::Ty;

use crate::builder::FuncBuilder;
use crate::config::StdlibConfig;
use crate::diag as d;

/// Name used in the diagnostics this synthesizer raises.
const STANDARD: &str = "ERC-8231";

/// Algorithm IDs per FIPS 204 / ERC-8231 §3.
const ALGORITHM_DILITHIUM_5: u128 = 1;

/// Entry point for ERC-8231 synthesis on a single Registry module.
pub fn synthesize(module: &mut IrModule, config: &StdlibConfig, diags: &mut Vec<Diagnostic>) {
    let span = module.name.span;

    let user_fns: HashSet<Box<str>> = module
        .functions
        .iter()
        .map(|f| f.name.name.clone())
        .collect();

    for name in STANDARD_FN_NAMES {
        if user_fns.contains(*name) {
            if config.strict_conflict_detection {
                diags.push(d::user_fn_conflict(span, STANDARD, name));
                return;
            } else {
                diags.push(d::warn_user_override(span, name));
            }
        }
    }

    let keys_id = ensure_field(
        module,
        "keys",
        Ty::Map(Box::new(Ty::Address), Box::new(Ty::PqKey)),
    );
    let registered_id = ensure_field(
        module,
        "registered",
        Ty::Map(Box::new(Ty::Address), Box::new(Ty::Bool)),
    );

    let skip = |name: &str| user_fns.contains(name);
    let mut next_id = module.functions.len() as u32;

    if !skip("is_registered") {
        module
            .functions
            .push(synth_is_registered(next_id, registered_id, span));
        next_id += 1;
    }
    if !skip("key_of") {
        module.functions.push(synth_key_of(next_id, keys_id, span));
        next_id += 1;
    }
    if !skip("algorithm_id") {
        module.functions.push(synth_algorithm_id(next_id, span));
        next_id += 1;
    }
    if !skip("register") {
        module
            .functions
            .push(synth_register(next_id, keys_id, registered_id, span));
        next_id += 1;
    }
    if !skip("revoke") {
        module
            .functions
            .push(synth_revoke(next_id, registered_id, span));
        // next_id += 1; // last write
    }

    inject_events(module, span);
    inject_errors(module, span);
}

const STANDARD_FN_NAMES: &[&str] = &[
    "is_registered",
    "key_of",
    "algorithm_id",
    "register",
    "revoke",
    // Reserved (Sprint 35.c) — flagged so user-declared overrides are detected:
    "update_key",
];

fn ensure_field(module: &mut IrModule, name: &str, ty: Ty) -> GlobalId {
    if let Some(existing) = module.fields.iter().find(|f| f.name.name.as_ref() == name) {
        return existing.id;
    }
    let id = GlobalId(module.fields.len() as u32);
    let span = module.name.span;
    module.fields.push(IrField {
        id,
        name: Ident {
            name: name.into(),
            span,
        },
        ty,
        privacy: PrivacyDomain::Plaintext,
        initializer_fn: None,
        initializer_const: None,
        span,
        explicit_slot: None,
    });
    id
}

fn inject_events(module: &mut IrModule, span: Span) {
    let existing: HashSet<Box<str>> = module.events.iter().map(|e| e.name.name.clone()).collect();
    let i = |name: &str| Ident {
        name: name.into(),
        span,
    };
    if !existing.contains("KeyRegistered") {
        module.events.push(IrEvent {
            name: i("KeyRegistered"),
            params: vec![(i("who"), Ty::Address, true), (i("pk"), Ty::PqKey, false)],
            span,
        });
    }
    if !existing.contains("KeyUpdated") {
        // Reserved for Sprint 35.c — schema fixed now so user-declared
        // overrides match this signature exactly.
        module.events.push(IrEvent {
            name: i("KeyUpdated"),
            params: vec![
                (i("who"), Ty::Address, true),
                (i("old_pk"), Ty::PqKey, false),
                (i("new_pk"), Ty::PqKey, false),
            ],
            span,
        });
    }
    if !existing.contains("KeyRevoked") {
        module.events.push(IrEvent {
            name: i("KeyRevoked"),
            params: vec![(i("who"), Ty::Address, true)],
            span,
        });
    }
}

fn inject_errors(module: &mut IrModule, span: Span) {
    let existing: HashSet<Box<str>> = module.errors.iter().map(|e| e.name.name.clone()).collect();
    for name in ["NotRegistered", "AlreadyRegistered"] {
        if !existing.contains(name) {
            module.errors.push(IrError {
                name: Ident {
                    name: name.into(),
                    span,
                },
                params: Vec::new(),
                span,
            });
        }
    }
}

// -------------------------------------------------------------------------
// Function synthesizers
// -------------------------------------------------------------------------

fn synth_is_registered(
    next_id: u32,
    registered_id: GlobalId,
    span: Span,
) -> covenant_ir::IrFunction {
    let mut fb = FuncBuilder::new(
        FunctionId(next_id),
        "is_registered",
        IrFunctionKind::View,
        Some(Ty::Bool),
        span,
    );
    let account = fb.add_param("account", Ty::Address);
    let map_base = fb.emit_instr(
        Opcode::SLoad(registered_id),
        vec![],
        Some(Ty::Map(Box::new(Ty::Address), Box::new(Ty::Bool))),
    );
    let val = fb.emit_instr(Opcode::MapGet, vec![map_base, account], Some(Ty::Bool));
    fb.terminate(Terminator::Return(Some(val)));
    fb.finish()
}

fn synth_key_of(next_id: u32, keys_id: GlobalId, span: Span) -> covenant_ir::IrFunction {
    let mut fb = FuncBuilder::new(
        FunctionId(next_id),
        "key_of",
        IrFunctionKind::View,
        Some(Ty::PqKey),
        span,
    );
    let account = fb.add_param("account", Ty::Address);
    let map_base = fb.emit_instr(
        Opcode::SLoad(keys_id),
        vec![],
        Some(Ty::Map(Box::new(Ty::Address), Box::new(Ty::PqKey))),
    );
    let pk = fb.emit_instr(Opcode::MapGet, vec![map_base, account], Some(Ty::PqKey));
    fb.terminate(Terminator::Return(Some(pk)));
    fb.finish()
}

fn synth_algorithm_id(next_id: u32, span: Span) -> covenant_ir::IrFunction {
    let mut fb = FuncBuilder::new(
        FunctionId(next_id),
        "algorithm_id",
        IrFunctionKind::View,
        Some(Ty::Amount),
        span,
    );
    let v = fb.emit_const(IrConstant::Integer(ALGORITHM_DILITHIUM_5), Ty::Amount);
    fb.terminate(Terminator::Return(Some(v)));
    fb.finish()
}

fn synth_register(
    next_id: u32,
    keys_id: GlobalId,
    registered_id: GlobalId,
    span: Span,
) -> covenant_ir::IrFunction {
    let mut fb = FuncBuilder::new(
        FunctionId(next_id),
        "register",
        IrFunctionKind::Action,
        None,
        span,
    );
    let pk = fb.add_param("pk", Ty::PqKey);
    let caller = fb.emit_instr(Opcode::LoadCaller, vec![], Some(Ty::Address));

    // Check NOT already registered: registered[caller] == false
    let reg_map = fb.emit_instr(
        Opcode::SLoad(registered_id),
        vec![],
        Some(Ty::Map(Box::new(Ty::Address), Box::new(Ty::Bool))),
    );
    let already = fb.emit_instr(Opcode::MapGet, vec![reg_map, caller], Some(Ty::Bool));
    let false_v = fb.emit_const(IrConstant::Bool(false), Ty::Bool);
    let is_new = fb.emit_instr(Opcode::Eq, vec![already, false_v], Some(Ty::Bool));

    let do_reg = fb.new_block();
    let revert_block = fb.new_block();
    fb.terminate(Terminator::Branch {
        cond: is_new,
        then_target: do_reg,
        then_args: Vec::new(),
        else_target: revert_block,
        else_args: Vec::new(),
    });
    fb.set_current(revert_block);
    fb.terminate(Terminator::Revert {
        error: Ident {
            name: "AlreadyRegistered".into(),
            span,
        },
        args: Vec::new(),
    });

    fb.set_current(do_reg);
    let keys_map = fb.emit_instr(
        Opcode::SLoad(keys_id),
        vec![],
        Some(Ty::Map(Box::new(Ty::Address), Box::new(Ty::PqKey))),
    );
    fb.emit_instr(Opcode::MapSet, vec![keys_map, caller, pk], None);
    let true_v = fb.emit_const(IrConstant::Bool(true), Ty::Bool);
    fb.emit_instr(Opcode::MapSet, vec![reg_map, caller, true_v], None);
    fb.emit_instr(
        Opcode::Emit(Box::new(Ident {
            name: "KeyRegistered".into(),
            span,
        })),
        vec![caller, pk],
        None,
    );
    fb.terminate(Terminator::Return(None));
    fb.finish()
}

fn synth_revoke(next_id: u32, registered_id: GlobalId, span: Span) -> covenant_ir::IrFunction {
    let mut fb = FuncBuilder::new(
        FunctionId(next_id),
        "revoke",
        IrFunctionKind::Action,
        None,
        span,
    );
    let caller = fb.emit_instr(Opcode::LoadCaller, vec![], Some(Ty::Address));

    // Check IS registered: registered[caller] == true
    let reg_map = fb.emit_instr(
        Opcode::SLoad(registered_id),
        vec![],
        Some(Ty::Map(Box::new(Ty::Address), Box::new(Ty::Bool))),
    );
    let cur = fb.emit_instr(Opcode::MapGet, vec![reg_map, caller], Some(Ty::Bool));

    let do_revoke = fb.new_block();
    let revert_block = fb.new_block();
    fb.terminate(Terminator::Branch {
        cond: cur,
        then_target: do_revoke,
        then_args: Vec::new(),
        else_target: revert_block,
        else_args: Vec::new(),
    });
    fb.set_current(revert_block);
    fb.terminate(Terminator::Revert {
        error: Ident {
            name: "NotRegistered".into(),
            span,
        },
        args: Vec::new(),
    });

    fb.set_current(do_revoke);
    let false_v = fb.emit_const(IrConstant::Bool(false), Ty::Bool);
    fb.emit_instr(Opcode::MapSet, vec![reg_map, caller, false_v], None);
    fb.emit_instr(
        Opcode::Emit(Box::new(Ident {
            name: "KeyRevoked".into(),
            span,
        })),
        vec![caller],
        None,
    );
    fb.terminate(Terminator::Return(None));
    fb.finish()
}

/// Canonical ERC-8231 selectors. Verifiable via `cast sig`.
pub const CANONICAL_SELECTORS: &[(&str, [u8; 4])] = &[
    // Selectors below are placeholders; Sprint 35.c will lock them in
    // with `cast sig` verification once the surface stabilizes. The
    // names + arg types are stable as of V0.9.0.
    ("is_registered", [0x00, 0x00, 0x00, 0x00]),
    ("key_of", [0x00, 0x00, 0x00, 0x00]),
    ("algorithm_id", [0x00, 0x00, 0x00, 0x00]),
    ("register", [0x00, 0x00, 0x00, 0x00]),
    ("revoke", [0x00, 0x00, 0x00, 0x00]),
];

#[allow(dead_code)]
fn _mark(_: Value) {}

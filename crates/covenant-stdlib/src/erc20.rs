//! ERC-20 interface synthesis.
//!
//! Synthesizes the 9 standard ERC-20 functions, 2 events, and 2 errors into an
//! `IrModule` whose construct kind is `Token`. Auto-injects required fields
//! (`total_supply`, `balances`, `allowances`) if not already present.

use std::collections::HashSet;

use covenant_diag::{Diagnostic, Span};
use covenant_ir::{
    function::IrFunctionKind,
    id::{FunctionId, GlobalId, Value},
    instr::{IrConstant, Terminator},
    module::{IrEvent, IrMetadataValue},
    IrError, IrModule, Opcode,
};
use covenant_parser::ast::Ident;
use covenant_types::Ty;

use crate::builder::FuncBuilder;
use crate::config::StdlibConfig;
use crate::conform;
use crate::diag as d;

/// Name used in the diagnostics this synthesizer raises.
const STANDARD: &str = "ERC-20";

/// Canonical shape of the ERC-20 `Transfer` / `Approval` events: two indexed
/// addresses then a non-indexed amount. Kept beside `inject_events` so the
/// shadow check and the injection cannot drift apart.
const EVENT_SHAPE_TRANSFER: &[(Ty, bool)] = &[
    (Ty::Address, true),
    (Ty::Address, true),
    (Ty::Amount, false),
];

/// Entry point for ERC-20 synthesis on a single Token module.
pub fn synthesize(module: &mut IrModule, config: &StdlibConfig, diags: &mut Vec<Diagnostic>) {
    let span = module.name.span;

    // --- Detect existing user-declared functions to respect conflicts ---
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

    // --- Refuse a shadowing event or error whose shape the bodies below
    //     cannot honour (F-35). Checked before anything is injected so the
    //     module is left exactly as the author wrote it. ---
    if reject_shadow_shape_conflicts(module, diags) {
        return;
    }

    // --- Resolve or inject required fields ---
    //
    // `conform::ensure_field` returns None when the author declared a field of
    // that name with a different type: reusing it would address the slot with
    // a shape the declaration contradicts (F-14), so synthesis stops.
    let (Some(total_supply_id), Some(balances_id), Some(allowances_id)) = (
        conform::ensure_field(module, "total_supply", Ty::Amount, STANDARD, diags),
        conform::ensure_field(
            module,
            "balances",
            Ty::Map(Box::new(Ty::Address), Box::new(Ty::Amount)),
            STANDARD,
            diags,
        ),
        conform::ensure_field(
            module,
            "allowances",
            // V0 uses flattened key: Hash = keccak(owner, spender) -> Amount.
            Ty::Map(Box::new(Ty::Hash), Box::new(Ty::Amount)),
            STANDARD,
            diags,
        ),
    ) else {
        return;
    };

    // --- Apply genesis mint from `supply: N to <principal>` metadata ---
    //
    // Only `to deployer` is honored in the constructor; every other principal
    // is refused rather than dropped (F-20), and a `total_supply` field default
    // that disagrees with the genesis amount is refused rather than silently
    // preferred (F-28). See `conform::apply_genesis_supply`.
    if !conform::apply_genesis_supply(module, total_supply_id, diags) {
        return;
    }

    // --- Pull metadata defaults ---
    let name_text = module
        .metadata
        .get("name")
        .and_then(|m| match m {
            IrMetadataValue::Text(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_else(|| module.name.name.clone());
    let symbol_text = module
        .metadata
        .get("symbol")
        .and_then(|m| match m {
            IrMetadataValue::Text(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_else(|| {
            diags.push(d::warn_missing_metadata(span, "symbol"));
            "".into()
        });
    if symbol_text.is_empty() {
        diags.push(d::warn_empty_symbol(span));
    }
    let decimals_value = module
        .metadata
        .get("decimals")
        .and_then(|m| match m {
            IrMetadataValue::Integer(n) => Some(*n),
            _ => None,
        })
        .unwrap_or_else(|| {
            diags.push(d::warn_missing_metadata(span, "decimals"));
            18
        });
    // EIP-20 types `decimals()` as uint8; a value that cannot round-trip
    // through it is refused rather than warned about (F-41).
    if !conform::check_decimals(span, decimals_value, diags) {
        return;
    }

    // --- Synthesize the 9 functions, skipping any user-declared ones in permissive mode ---
    let skip = |name: &str| user_fns.contains(name);

    if !skip("totalSupply") {
        module.functions.push(synth_total_supply(
            module.functions.len() as u32,
            total_supply_id,
            span,
        ));
    }
    if !skip("balanceOf") {
        module.functions.push(synth_balance_of(
            module.functions.len() as u32,
            balances_id,
            span,
        ));
    }
    if !skip("transfer") {
        module.functions.push(synth_transfer(
            module.functions.len() as u32,
            balances_id,
            span,
        ));
    }
    if !skip("transferFrom") {
        module.functions.push(synth_transfer_from(
            module.functions.len() as u32,
            balances_id,
            allowances_id,
            span,
        ));
    }
    if !skip("approve") {
        module.functions.push(synth_approve(
            module.functions.len() as u32,
            allowances_id,
            span,
        ));
    }
    if !skip("allowance") {
        module.functions.push(synth_allowance(
            module.functions.len() as u32,
            allowances_id,
            span,
        ));
    }
    if !skip("decimals") {
        module.functions.push(synth_decimals(
            module.functions.len() as u32,
            decimals_value,
            span,
        ));
    }
    if !skip("symbol") {
        module.functions.push(synth_symbol(
            module.functions.len() as u32,
            &symbol_text,
            span,
        ));
    }
    if !skip("name") {
        module
            .functions
            .push(synth_name(module.functions.len() as u32, &name_text, span));
    }

    // --- Inject standard events + errors if absent ---
    inject_events(module, span);
    inject_errors(module, span);
}

const STANDARD_FN_NAMES: &[&str] = &[
    "totalSupply",
    "balanceOf",
    "transfer",
    "transferFrom",
    "approve",
    "allowance",
    "decimals",
    "symbol",
    "name",
];

/// Refuse any user-declared event or error that shadows a synthesized one with
/// a different shape (F-35).
///
/// `inject_events` / `inject_errors` skip a name the author already used, but
/// `synth_transfer` and friends keep emitting the canonical arity, so a
/// differently-shaped redeclaration ships an ABI the runtime does not honour:
/// a two-word `InsufficientBalance` reverting with a zero-byte payload, or a
/// `Transfer` whose topic0 is no longer the canonical ERC-20 hash. Returns
/// `true` when synthesis must stop.
fn reject_shadow_shape_conflicts(module: &IrModule, diags: &mut Vec<Diagnostic>) -> bool {
    let mut conflict = false;
    for name in ["Transfer", "Approval"] {
        conflict |= conform::event_shadow_conflicts(module, name, EVENT_SHAPE_TRANSFER, diags);
    }
    // Both synthesized errors revert with `args: Vec::new()`, so their
    // canonical shape carries no parameters at all.
    for name in ["InsufficientBalance", "InsufficientAllowance"] {
        conflict |= conform::error_shadow_conflicts(module, name, &[], diags);
    }
    conflict
}

fn inject_events(module: &mut IrModule, span: Span) {
    let existing: HashSet<Box<str>> = module.events.iter().map(|e| e.name.name.clone()).collect();
    if !existing.contains("Transfer") {
        module.events.push(IrEvent {
            name: Ident {
                name: "Transfer".into(),
                span,
            },
            params: vec![
                (
                    Ident {
                        name: "from".into(),
                        span,
                    },
                    Ty::Address,
                    true,
                ),
                (
                    Ident {
                        name: "to".into(),
                        span,
                    },
                    Ty::Address,
                    true,
                ),
                (
                    Ident {
                        name: "value".into(),
                        span,
                    },
                    Ty::Amount,
                    false,
                ),
            ],
            span,
        });
    }
    if !existing.contains("Approval") {
        module.events.push(IrEvent {
            name: Ident {
                name: "Approval".into(),
                span,
            },
            params: vec![
                (
                    Ident {
                        name: "owner".into(),
                        span,
                    },
                    Ty::Address,
                    true,
                ),
                (
                    Ident {
                        name: "spender".into(),
                        span,
                    },
                    Ty::Address,
                    true,
                ),
                (
                    Ident {
                        name: "value".into(),
                        span,
                    },
                    Ty::Amount,
                    false,
                ),
            ],
            span,
        });
    }
}

fn inject_errors(module: &mut IrModule, span: Span) {
    let existing: HashSet<Box<str>> = module.errors.iter().map(|e| e.name.name.clone()).collect();
    if !existing.contains("InsufficientBalance") {
        module.errors.push(IrError {
            name: Ident {
                name: "InsufficientBalance".into(),
                span,
            },
            params: Vec::new(),
            span,
        });
    }
    if !existing.contains("InsufficientAllowance") {
        module.errors.push(IrError {
            name: Ident {
                name: "InsufficientAllowance".into(),
                span,
            },
            params: Vec::new(),
            span,
        });
    }
}

// -------------------------------------------------------------------------
// Individual function synthesizers
// -------------------------------------------------------------------------

fn synth_total_supply(
    next_id: u32,
    total_supply_id: GlobalId,
    span: Span,
) -> covenant_ir::IrFunction {
    let mut fb = FuncBuilder::new(
        FunctionId(next_id),
        "totalSupply",
        IrFunctionKind::View,
        Some(Ty::Amount),
        span,
    );
    let v = fb.emit_instr(Opcode::SLoad(total_supply_id), vec![], Some(Ty::Amount));
    fb.terminate(Terminator::Return(Some(v)));
    fb.finish()
}

fn synth_balance_of(next_id: u32, balances_id: GlobalId, span: Span) -> covenant_ir::IrFunction {
    let mut fb = FuncBuilder::new(
        FunctionId(next_id),
        "balanceOf",
        IrFunctionKind::View,
        Some(Ty::Amount),
        span,
    );
    let owner = fb.add_param("owner", Ty::Address);
    let map_base = fb.emit_instr(
        Opcode::SLoad(balances_id),
        vec![],
        Some(Ty::Map(Box::new(Ty::Address), Box::new(Ty::Amount))),
    );
    let bal = fb.emit_instr(Opcode::MapGet, vec![map_base, owner], Some(Ty::Amount));
    fb.terminate(Terminator::Return(Some(bal)));
    fb.finish()
}

fn synth_transfer(next_id: u32, balances_id: GlobalId, span: Span) -> covenant_ir::IrFunction {
    let mut fb = FuncBuilder::new(
        FunctionId(next_id),
        "transfer",
        IrFunctionKind::Action,
        Some(Ty::Bool),
        span,
    );
    let to_param = fb.add_param("to", Ty::Address);
    let value_param = fb.add_param("value", Ty::Amount);

    // Entry block: balance check.
    let caller = fb.emit_instr(Opcode::LoadCaller, vec![], Some(Ty::Address));
    let map_base = fb.emit_instr(
        Opcode::SLoad(balances_id),
        vec![],
        Some(Ty::Map(Box::new(Ty::Address), Box::new(Ty::Amount))),
    );
    let from_bal = fb.emit_instr(Opcode::MapGet, vec![map_base, caller], Some(Ty::Amount));
    let ok = fb.emit_instr(Opcode::Ge, vec![from_bal, value_param], Some(Ty::Bool));

    let transfer_block = fb.new_block();
    let revert_block = fb.new_block();
    fb.terminate(Terminator::Branch {
        cond: ok,
        then_target: transfer_block,
        then_args: Vec::new(),
        else_target: revert_block,
        else_args: Vec::new(),
    });

    // Revert block.
    fb.set_current(revert_block);
    fb.terminate(Terminator::Revert {
        error: Ident {
            name: "InsufficientBalance".into(),
            span,
        },
        args: Vec::new(),
    });

    // Transfer block: subtract from caller, add to recipient, emit, return true.
    fb.set_current(transfer_block);
    let new_from = fb.emit_instr(
        Opcode::SubChecked,
        vec![from_bal, value_param],
        Some(Ty::Amount),
    );
    fb.emit_instr(Opcode::MapSet, vec![map_base, caller, new_from], None);
    let map_base2 = fb.emit_instr(
        Opcode::SLoad(balances_id),
        vec![],
        Some(Ty::Map(Box::new(Ty::Address), Box::new(Ty::Amount))),
    );
    let to_bal = fb.emit_instr(Opcode::MapGet, vec![map_base2, to_param], Some(Ty::Amount));
    let new_to = fb.emit_instr(
        Opcode::AddChecked,
        vec![to_bal, value_param],
        Some(Ty::Amount),
    );
    fb.emit_instr(Opcode::MapSet, vec![map_base2, to_param, new_to], None);
    fb.emit_instr(
        Opcode::Emit(Box::new(Ident {
            name: "Transfer".into(),
            span,
        })),
        vec![caller, to_param, value_param],
        None,
    );
    let true_v = fb.emit_const(IrConstant::Bool(true), Ty::Bool);
    fb.terminate(Terminator::Return(Some(true_v)));

    fb.finish()
}

fn synth_transfer_from(
    next_id: u32,
    balances_id: GlobalId,
    allowances_id: GlobalId,
    span: Span,
) -> covenant_ir::IrFunction {
    let mut fb = FuncBuilder::new(
        FunctionId(next_id),
        "transferFrom",
        IrFunctionKind::Action,
        Some(Ty::Bool),
        span,
    );
    let from_param = fb.add_param("from", Ty::Address);
    let to_param = fb.add_param("to", Ty::Address);
    let value_param = fb.add_param("value", Ty::Amount);

    // Entry: compute composite allowance key, check allowance.
    let caller = fb.emit_instr(Opcode::LoadCaller, vec![], Some(Ty::Address));
    let key = fb.emit_instr(Opcode::Keccak, vec![from_param, caller], Some(Ty::Hash));
    let alw_map = fb.emit_instr(
        Opcode::SLoad(allowances_id),
        vec![],
        Some(Ty::Map(Box::new(Ty::Hash), Box::new(Ty::Amount))),
    );
    let allowance = fb.emit_instr(Opcode::MapGet, vec![alw_map, key], Some(Ty::Amount));
    let alw_ok = fb.emit_instr(Opcode::Ge, vec![allowance, value_param], Some(Ty::Bool));

    let check_balance = fb.new_block();
    let revert_alw = fb.new_block();
    fb.terminate(Terminator::Branch {
        cond: alw_ok,
        then_target: check_balance,
        then_args: Vec::new(),
        else_target: revert_alw,
        else_args: Vec::new(),
    });

    // Revert: InsufficientAllowance.
    fb.set_current(revert_alw);
    fb.terminate(Terminator::Revert {
        error: Ident {
            name: "InsufficientAllowance".into(),
            span,
        },
        args: Vec::new(),
    });

    // Check balance.
    fb.set_current(check_balance);
    let bal_map = fb.emit_instr(
        Opcode::SLoad(balances_id),
        vec![],
        Some(Ty::Map(Box::new(Ty::Address), Box::new(Ty::Amount))),
    );
    let from_bal = fb.emit_instr(Opcode::MapGet, vec![bal_map, from_param], Some(Ty::Amount));
    let bal_ok = fb.emit_instr(Opcode::Ge, vec![from_bal, value_param], Some(Ty::Bool));

    let do_transfer = fb.new_block();
    let revert_bal = fb.new_block();
    fb.terminate(Terminator::Branch {
        cond: bal_ok,
        then_target: do_transfer,
        then_args: Vec::new(),
        else_target: revert_bal,
        else_args: Vec::new(),
    });

    fb.set_current(revert_bal);
    fb.terminate(Terminator::Revert {
        error: Ident {
            name: "InsufficientBalance".into(),
            span,
        },
        args: Vec::new(),
    });

    // Transfer: decrement allowance, subtract/add balances, emit, return true.
    fb.set_current(do_transfer);
    let new_alw = fb.emit_instr(
        Opcode::SubChecked,
        vec![allowance, value_param],
        Some(Ty::Amount),
    );
    fb.emit_instr(Opcode::MapSet, vec![alw_map, key, new_alw], None);
    let new_from = fb.emit_instr(
        Opcode::SubChecked,
        vec![from_bal, value_param],
        Some(Ty::Amount),
    );
    fb.emit_instr(Opcode::MapSet, vec![bal_map, from_param, new_from], None);
    let to_bal = fb.emit_instr(Opcode::MapGet, vec![bal_map, to_param], Some(Ty::Amount));
    let new_to = fb.emit_instr(
        Opcode::AddChecked,
        vec![to_bal, value_param],
        Some(Ty::Amount),
    );
    fb.emit_instr(Opcode::MapSet, vec![bal_map, to_param, new_to], None);
    fb.emit_instr(
        Opcode::Emit(Box::new(Ident {
            name: "Transfer".into(),
            span,
        })),
        vec![from_param, to_param, value_param],
        None,
    );
    let true_v = fb.emit_const(IrConstant::Bool(true), Ty::Bool);
    fb.terminate(Terminator::Return(Some(true_v)));

    fb.finish()
}

fn synth_approve(next_id: u32, allowances_id: GlobalId, span: Span) -> covenant_ir::IrFunction {
    let mut fb = FuncBuilder::new(
        FunctionId(next_id),
        "approve",
        IrFunctionKind::Action,
        Some(Ty::Bool),
        span,
    );
    let spender = fb.add_param("spender", Ty::Address);
    let value_param = fb.add_param("value", Ty::Amount);

    let caller = fb.emit_instr(Opcode::LoadCaller, vec![], Some(Ty::Address));
    let key = fb.emit_instr(Opcode::Keccak, vec![caller, spender], Some(Ty::Hash));
    let alw_map = fb.emit_instr(
        Opcode::SLoad(allowances_id),
        vec![],
        Some(Ty::Map(Box::new(Ty::Hash), Box::new(Ty::Amount))),
    );
    fb.emit_instr(Opcode::MapSet, vec![alw_map, key, value_param], None);

    fb.emit_instr(
        Opcode::Emit(Box::new(Ident {
            name: "Approval".into(),
            span,
        })),
        vec![caller, spender, value_param],
        None,
    );
    let true_v = fb.emit_const(IrConstant::Bool(true), Ty::Bool);
    fb.terminate(Terminator::Return(Some(true_v)));
    fb.finish()
}

fn synth_allowance(next_id: u32, allowances_id: GlobalId, span: Span) -> covenant_ir::IrFunction {
    let mut fb = FuncBuilder::new(
        FunctionId(next_id),
        "allowance",
        IrFunctionKind::View,
        Some(Ty::Amount),
        span,
    );
    let owner = fb.add_param("owner", Ty::Address);
    let spender = fb.add_param("spender", Ty::Address);

    let key = fb.emit_instr(Opcode::Keccak, vec![owner, spender], Some(Ty::Hash));
    let alw_map = fb.emit_instr(
        Opcode::SLoad(allowances_id),
        vec![],
        Some(Ty::Map(Box::new(Ty::Hash), Box::new(Ty::Amount))),
    );
    let value = fb.emit_instr(Opcode::MapGet, vec![alw_map, key], Some(Ty::Amount));
    fb.terminate(Terminator::Return(Some(value)));
    fb.finish()
}

fn synth_decimals(next_id: u32, decimals: u128, span: Span) -> covenant_ir::IrFunction {
    let mut fb = FuncBuilder::new(
        FunctionId(next_id),
        "decimals",
        IrFunctionKind::View,
        Some(Ty::Amount),
        span,
    );
    let v = fb.emit_const(IrConstant::Integer(decimals), Ty::Amount);
    fb.terminate(Terminator::Return(Some(v)));
    fb.finish()
}

fn synth_symbol(next_id: u32, symbol: &str, span: Span) -> covenant_ir::IrFunction {
    let mut fb = FuncBuilder::new(
        FunctionId(next_id),
        "symbol",
        IrFunctionKind::View,
        Some(Ty::Text),
        span,
    );
    let v = fb.emit_const(IrConstant::Text(symbol.into()), Ty::Text);
    fb.terminate(Terminator::Return(Some(v)));
    fb.finish()
}

fn synth_name(next_id: u32, name: &str, span: Span) -> covenant_ir::IrFunction {
    let mut fb = FuncBuilder::new(
        FunctionId(next_id),
        "name",
        IrFunctionKind::View,
        Some(Ty::Text),
        span,
    );
    let v = fb.emit_const(IrConstant::Text(name.into()), Ty::Text);
    fb.terminate(Terminator::Return(Some(v)));
    fb.finish()
}

// Convenience so downstream callers can iterate the canonical selectors.
pub const CANONICAL_SELECTORS: &[(&str, [u8; 4])] = &[
    ("totalSupply", [0x18, 0x16, 0x0d, 0xdd]),
    ("balanceOf", [0x70, 0xa0, 0x82, 0x31]),
    ("transfer", [0xa9, 0x05, 0x9c, 0xbb]),
    ("transferFrom", [0x23, 0xb8, 0x72, 0xdd]),
    ("approve", [0x09, 0x5e, 0xa7, 0xb3]),
    ("allowance", [0xdd, 0x62, 0xed, 0x3e]),
    ("decimals", [0x31, 0x3c, 0xe5, 0x67]),
    ("symbol", [0x95, 0xd8, 0x9b, 0x41]),
    ("name", [0x06, 0xfd, 0xde, 0x03]),
];

/// Mark a `Value` used (reserved for dead-code warnings; currently unused).
#[allow(dead_code)]
fn _mark(_: Value) {}

//! ERC-8227 interface synthesis for `confidential token` constructs.
//!
//! Synthesizes encrypted-balance token functions:
//!   - `transferEncrypted(address, bytes32) → bool`
//!   - `balanceOfEncrypted(address) → bytes32`
//!   - `transferFromEncrypted(address, address, bytes32) → bool`
//!   - `approveEncrypted(address, bytes32) → bool`
//!   - `allowanceEncrypted(address, address) → bytes32`
//!   - `decimals() → uint256`, `symbol() → string`, `name() → string`
//!   - `totalSupply() → uint256` (plaintext total)
//!
//! Ciphertext handles are `bytes32` on-chain (mapped from `Ty::Ciphertext(_)`).
//! All FHE operations use precompiles; `AssertEncrypted` handles the balance
//! check via threshold-decrypt-then-revert (see LESSONS.md §7).

use std::collections::HashSet;

use covenant_diag::{Diagnostic, Span};
use covenant_ir::{
    function::IrFunctionKind,
    id::{FunctionId, GlobalId, Value},
    instr::{IrConstant, Terminator},
    module::{IrError, IrEvent, IrMetadataValue},
    IrModule, Opcode,
};
use covenant_parser::ast::Ident;
use covenant_types::Ty;

use crate::builder::FuncBuilder;
use crate::config::StdlibConfig;
use crate::conform;
use crate::diag as d;

/// Name used in the diagnostics this synthesizer raises.
const STANDARD: &str = "ERC-8227";

// -------------------------------------------------------------------------
// ERC-8227 canonical ABI selectors.
// Computed from keccak256("funcName(types...)")[0..4].
// Verified in integration tests (secretcoin_erc8227_selectors_match_canonical).
// -------------------------------------------------------------------------
pub const CANONICAL_SELECTORS: &[(&str, [u8; 4])] = &[
    // keccak256("transferEncrypted(address,bytes32)")[0..4]
    ("transferEncrypted", [0x4a, 0x93, 0x2b, 0x8d]),
    // keccak256("balanceOfEncrypted(address)")[0..4]
    ("balanceOfEncrypted", [0x51, 0xa6, 0xa2, 0x12]),
    // keccak256("transferFromEncrypted(address,address,bytes32)")[0..4]
    ("transferFromEncrypted", [0x8d, 0x72, 0xe0, 0xf5]),
    // keccak256("approveEncrypted(address,bytes32)")[0..4]
    ("approveEncrypted", [0x30, 0x4d, 0xe1, 0x6b]),
    // keccak256("allowanceEncrypted(address,address)")[0..4]
    ("allowanceEncrypted", [0x8b, 0x70, 0x3d, 0x44]),
    // Standard metadata functions (same as ERC-20):
    ("totalSupply", [0x18, 0x16, 0x0d, 0xdd]),
    ("decimals", [0x31, 0x3c, 0xe5, 0x67]),
    ("symbol", [0x95, 0xd8, 0x9b, 0x41]),
    ("name", [0x06, 0xfd, 0xde, 0x03]),
];

const STANDARD_FN_NAMES: &[&str] = &[
    "transferEncrypted",
    "balanceOfEncrypted",
    "transferFromEncrypted",
    "approveEncrypted",
    "allowanceEncrypted",
    "totalSupply",
    "decimals",
    "symbol",
    "name",
];

/// Entry point: synthesize all ERC-8227 functions into a `confidential token` module.
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

    // --- Ensure required fields ---
    //
    // A field of the right name but the wrong type is refused, not reused
    // (F-14): the synthesized bodies address each of these with one shape,
    // and here a plaintext `balances` would also silently leave the encrypted
    // domain.
    let ct_amount_ty = Ty::Ciphertext(Box::new(Ty::Amount));

    let (Some(total_supply_id), Some(balances_id), Some(allowances_id)) = (
        conform::ensure_field(module, "total_supply", Ty::Amount, STANDARD, diags),
        conform::ensure_field(
            module,
            "balances",
            Ty::Map(Box::new(Ty::Address), Box::new(ct_amount_ty.clone())),
            STANDARD,
            diags,
        ),
        conform::ensure_field(
            module,
            "allowances",
            Ty::Map(Box::new(Ty::Hash), Box::new(ct_amount_ty.clone())),
            STANDARD,
            diags,
        ),
    ) else {
        return;
    };

    // --- Initialize total_supply constant from supply metadata ---
    //
    // Same refusals as the ERC-20 path: a non-deployer genesis principal is
    // never minted (F-20) and a `total_supply` field default that disagrees
    // with the genesis amount silently wins (F-28).
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
    // The synthesized `decimals()` is the same ERC-20 accessor, so the same
    // uint8 representability bound applies (F-41).
    if !conform::check_decimals(span, decimals_value, diags) {
        return;
    }

    let skip = |name: &str| user_fns.contains(name);

    // --- Encrypted transfer functions ---
    if !skip("transferEncrypted") {
        module.functions.push(synth_transfer_encrypted(
            module.functions.len() as u32,
            balances_id,
            span,
        ));
    }
    if !skip("balanceOfEncrypted") {
        module.functions.push(synth_balance_of_encrypted(
            module.functions.len() as u32,
            balances_id,
            span,
        ));
    }
    if !skip("transferFromEncrypted") {
        module.functions.push(synth_transfer_from_encrypted(
            module.functions.len() as u32,
            balances_id,
            allowances_id,
            span,
        ));
    }
    if !skip("approveEncrypted") {
        module.functions.push(synth_approve_encrypted(
            module.functions.len() as u32,
            allowances_id,
            span,
        ));
    }
    if !skip("allowanceEncrypted") {
        module.functions.push(synth_allowance_encrypted(
            module.functions.len() as u32,
            allowances_id,
            span,
        ));
    }

    // --- Standard metadata views ---
    if !skip("totalSupply") {
        module.functions.push(synth_total_supply(
            module.functions.len() as u32,
            total_supply_id,
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

    inject_events(module, span);
    inject_errors(module, span);
}

// -------------------------------------------------------------------------
// Event / error injection
// -------------------------------------------------------------------------

fn inject_events(module: &mut IrModule, span: Span) {
    let existing: HashSet<Box<str>> = module.events.iter().map(|e| e.name.name.clone()).collect();

    if !existing.contains("TransferEncrypted") {
        // TransferEncrypted(address indexed from, address indexed to, bytes32 encryptedAmount)
        module.events.push(IrEvent {
            name: Ident {
                name: "TransferEncrypted".into(),
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
                        name: "encryptedAmount".into(),
                        span,
                    },
                    Ty::Ciphertext(Box::new(Ty::Amount)),
                    false,
                ),
            ],
            span,
        });
    }

    if !existing.contains("ApprovalEncrypted") {
        // ApprovalEncrypted(address indexed owner, address indexed spender, bytes32 encryptedAmount)
        module.events.push(IrEvent {
            name: Ident {
                name: "ApprovalEncrypted".into(),
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
                        name: "encryptedAmount".into(),
                        span,
                    },
                    Ty::Ciphertext(Box::new(Ty::Amount)),
                    false,
                ),
            ],
            span,
        });
    }
}

fn inject_errors(module: &mut IrModule, span: Span) {
    let existing: HashSet<Box<str>> = module.errors.iter().map(|e| e.name.name.clone()).collect();

    if !existing.contains("InsufficientEncryptedBalance") {
        module.errors.push(IrError {
            name: Ident {
                name: "InsufficientEncryptedBalance".into(),
                span,
            },
            params: Vec::new(),
            span,
        });
    }
    if !existing.contains("InsufficientEncryptedAllowance") {
        module.errors.push(IrError {
            name: Ident {
                name: "InsufficientEncryptedAllowance".into(),
                span,
            },
            params: Vec::new(),
            span,
        });
    }
}

// -------------------------------------------------------------------------
// Function synthesizers
// -------------------------------------------------------------------------

/// `transferEncrypted(address to, bytes32 encryptedAmount) → bool`
///
/// Checks `FheCmpGe(from_balance, amount)` via `AssertEncrypted`, then does
/// encrypted sub/add on the balances map. Emits `TransferEncrypted`.
fn synth_transfer_encrypted(
    next_id: u32,
    balances_id: GlobalId,
    span: Span,
) -> covenant_ir::IrFunction {
    let ct_amount_ty = Ty::Ciphertext(Box::new(Ty::Amount));
    let ct_bool_ty = Ty::Ciphertext(Box::new(Ty::Bool));
    let map_ty = || Ty::Map(Box::new(Ty::Address), Box::new(ct_amount_ty.clone()));

    let mut fb = FuncBuilder::new(
        FunctionId(next_id),
        "transferEncrypted",
        IrFunctionKind::Action,
        Some(Ty::Bool),
        span,
    );
    let to_param = fb.add_param("to", Ty::Address);
    let enc_amount = fb.add_param("encryptedAmount", ct_amount_ty.clone());

    let caller = fb.emit_instr(Opcode::LoadCaller, vec![], Some(Ty::Address));

    // Load from-balance ciphertext handle.
    let map_base = fb.emit_instr(Opcode::SLoad(balances_id), vec![], Some(map_ty()));
    let ct_from_bal = fb.emit_instr(
        Opcode::MapGet,
        vec![map_base, caller],
        Some(ct_amount_ty.clone()),
    );

    // AssertEncrypted(FheCmpGe(from_balance, amount)):
    // V0.2 note: leaks 1 bit (balance-sufficient), see LESSONS.md §7.
    let ct_ge = fb.emit_instr(
        Opcode::FheCmpGe,
        vec![ct_from_bal, enc_amount],
        Some(ct_bool_ty),
    );
    fb.emit_instr(Opcode::AssertEncrypted, vec![ct_ge], None);

    // Compute new encrypted balances.
    let ct_new_from = fb.emit_instr(
        Opcode::FheSub,
        vec![ct_from_bal, enc_amount],
        Some(ct_amount_ty.clone()),
    );

    // Reload map base for to-balance (separate SLoad to get fresh handle).
    let map_base2 = fb.emit_instr(Opcode::SLoad(balances_id), vec![], Some(map_ty()));
    let ct_to_bal = fb.emit_instr(
        Opcode::MapGet,
        vec![map_base2, to_param],
        Some(ct_amount_ty.clone()),
    );
    let ct_new_to = fb.emit_instr(
        Opcode::FheAdd,
        vec![ct_to_bal, enc_amount],
        Some(ct_amount_ty.clone()),
    );

    // Write new balances.
    fb.emit_instr(Opcode::MapSet, vec![map_base2, caller, ct_new_from], None);
    let map_base3 = fb.emit_instr(Opcode::SLoad(balances_id), vec![], Some(map_ty()));
    fb.emit_instr(Opcode::MapSet, vec![map_base3, to_param, ct_new_to], None);

    // Emit TransferEncrypted event.
    fb.emit_instr(
        Opcode::Emit(Box::new(Ident {
            name: "TransferEncrypted".into(),
            span,
        })),
        vec![caller, to_param, enc_amount],
        None,
    );

    let true_v = fb.emit_const(IrConstant::Bool(true), Ty::Bool);
    fb.terminate(Terminator::Return(Some(true_v)));
    fb.finish()
}

/// `balanceOfEncrypted(address holder) view → bytes32`
///
/// Returns the raw ciphertext handle for the holder's balance. The caller
/// invokes a reveal separately (owner-authorized decrypt path).
fn synth_balance_of_encrypted(
    next_id: u32,
    balances_id: GlobalId,
    span: Span,
) -> covenant_ir::IrFunction {
    let ct_amount_ty = Ty::Ciphertext(Box::new(Ty::Amount));
    let map_ty = Ty::Map(Box::new(Ty::Address), Box::new(ct_amount_ty.clone()));

    let mut fb = FuncBuilder::new(
        FunctionId(next_id),
        "balanceOfEncrypted",
        IrFunctionKind::View,
        Some(ct_amount_ty.clone()),
        span,
    );
    let holder = fb.add_param("holder", Ty::Address);
    let map_base = fb.emit_instr(Opcode::SLoad(balances_id), vec![], Some(map_ty));
    let handle = fb.emit_instr(Opcode::MapGet, vec![map_base, holder], Some(ct_amount_ty));
    fb.terminate(Terminator::Return(Some(handle)));
    fb.finish()
}

/// `transferFromEncrypted(address from, address to, bytes32 encryptedAmount) → bool`
///
/// Verifies encrypted allowance ≥ amount (AssertEncrypted), then delegates
/// to the transfer logic. Updates the encrypted allowance after transfer.
fn synth_transfer_from_encrypted(
    next_id: u32,
    balances_id: GlobalId,
    allowances_id: GlobalId,
    span: Span,
) -> covenant_ir::IrFunction {
    let ct_amount_ty = Ty::Ciphertext(Box::new(Ty::Amount));
    let ct_bool_ty = Ty::Ciphertext(Box::new(Ty::Bool));
    let bal_map_ty = || Ty::Map(Box::new(Ty::Address), Box::new(ct_amount_ty.clone()));
    let alw_map_ty = || Ty::Map(Box::new(Ty::Hash), Box::new(ct_amount_ty.clone()));

    let mut fb = FuncBuilder::new(
        FunctionId(next_id),
        "transferFromEncrypted",
        IrFunctionKind::Action,
        Some(Ty::Bool),
        span,
    );
    let from_param = fb.add_param("from", Ty::Address);
    let to_param = fb.add_param("to", Ty::Address);
    let enc_amount = fb.add_param("encryptedAmount", ct_amount_ty.clone());

    let caller = fb.emit_instr(Opcode::LoadCaller, vec![], Some(Ty::Address));

    // Check encrypted allowance.
    let key = fb.emit_instr(Opcode::Keccak, vec![from_param, caller], Some(Ty::Hash));
    let alw_map = fb.emit_instr(Opcode::SLoad(allowances_id), vec![], Some(alw_map_ty()));
    let ct_allowance = fb.emit_instr(
        Opcode::MapGet,
        vec![alw_map, key],
        Some(ct_amount_ty.clone()),
    );
    let ct_alw_ge = fb.emit_instr(
        Opcode::FheCmpGe,
        vec![ct_allowance, enc_amount],
        Some(ct_bool_ty.clone()),
    );
    fb.emit_instr(Opcode::AssertEncrypted, vec![ct_alw_ge], None);

    // Check encrypted balance.
    let bal_map = fb.emit_instr(Opcode::SLoad(balances_id), vec![], Some(bal_map_ty()));
    let ct_from_bal = fb.emit_instr(
        Opcode::MapGet,
        vec![bal_map, from_param],
        Some(ct_amount_ty.clone()),
    );
    let ct_bal_ge = fb.emit_instr(
        Opcode::FheCmpGe,
        vec![ct_from_bal, enc_amount],
        Some(ct_bool_ty),
    );
    fb.emit_instr(Opcode::AssertEncrypted, vec![ct_bal_ge], None);

    // Deduct allowance.
    let ct_new_alw = fb.emit_instr(
        Opcode::FheSub,
        vec![ct_allowance, enc_amount],
        Some(ct_amount_ty.clone()),
    );
    let alw_map2 = fb.emit_instr(Opcode::SLoad(allowances_id), vec![], Some(alw_map_ty()));
    let key2 = fb.emit_instr(Opcode::Keccak, vec![from_param, caller], Some(Ty::Hash));
    fb.emit_instr(Opcode::MapSet, vec![alw_map2, key2, ct_new_alw], None);

    // Transfer balances.
    let ct_new_from = fb.emit_instr(
        Opcode::FheSub,
        vec![ct_from_bal, enc_amount],
        Some(ct_amount_ty.clone()),
    );
    let bal_map2 = fb.emit_instr(Opcode::SLoad(balances_id), vec![], Some(bal_map_ty()));
    let ct_to_bal = fb.emit_instr(
        Opcode::MapGet,
        vec![bal_map2, to_param],
        Some(ct_amount_ty.clone()),
    );
    let ct_new_to = fb.emit_instr(
        Opcode::FheAdd,
        vec![ct_to_bal, enc_amount],
        Some(ct_amount_ty.clone()),
    );
    fb.emit_instr(
        Opcode::MapSet,
        vec![bal_map2, from_param, ct_new_from],
        None,
    );
    let bal_map3 = fb.emit_instr(Opcode::SLoad(balances_id), vec![], Some(bal_map_ty()));
    fb.emit_instr(Opcode::MapSet, vec![bal_map3, to_param, ct_new_to], None);

    fb.emit_instr(
        Opcode::Emit(Box::new(Ident {
            name: "TransferEncrypted".into(),
            span,
        })),
        vec![from_param, to_param, enc_amount],
        None,
    );

    let true_v = fb.emit_const(IrConstant::Bool(true), Ty::Bool);
    fb.terminate(Terminator::Return(Some(true_v)));
    fb.finish()
}

/// `approveEncrypted(address spender, bytes32 encryptedAmount) → bool`
///
/// Stores an encrypted allowance for (caller, spender). Emits `ApprovalEncrypted`.
fn synth_approve_encrypted(
    next_id: u32,
    allowances_id: GlobalId,
    span: Span,
) -> covenant_ir::IrFunction {
    let ct_amount_ty = Ty::Ciphertext(Box::new(Ty::Amount));
    let alw_map_ty = Ty::Map(Box::new(Ty::Hash), Box::new(ct_amount_ty.clone()));

    let mut fb = FuncBuilder::new(
        FunctionId(next_id),
        "approveEncrypted",
        IrFunctionKind::Action,
        Some(Ty::Bool),
        span,
    );
    let spender_param = fb.add_param("spender", Ty::Address);
    let enc_amount = fb.add_param("encryptedAmount", ct_amount_ty.clone());

    let caller = fb.emit_instr(Opcode::LoadCaller, vec![], Some(Ty::Address));
    let key = fb.emit_instr(Opcode::Keccak, vec![caller, spender_param], Some(Ty::Hash));
    let alw_map = fb.emit_instr(Opcode::SLoad(allowances_id), vec![], Some(alw_map_ty));
    fb.emit_instr(Opcode::MapSet, vec![alw_map, key, enc_amount], None);

    fb.emit_instr(
        Opcode::Emit(Box::new(Ident {
            name: "ApprovalEncrypted".into(),
            span,
        })),
        vec![caller, spender_param, enc_amount],
        None,
    );

    let true_v = fb.emit_const(IrConstant::Bool(true), Ty::Bool);
    fb.terminate(Terminator::Return(Some(true_v)));
    fb.finish()
}

/// `allowanceEncrypted(address owner, address spender) view → bytes32`
///
/// Returns the encrypted allowance handle for the (owner, spender) pair.
fn synth_allowance_encrypted(
    next_id: u32,
    allowances_id: GlobalId,
    span: Span,
) -> covenant_ir::IrFunction {
    let ct_amount_ty = Ty::Ciphertext(Box::new(Ty::Amount));
    let alw_map_ty = Ty::Map(Box::new(Ty::Hash), Box::new(ct_amount_ty.clone()));

    let mut fb = FuncBuilder::new(
        FunctionId(next_id),
        "allowanceEncrypted",
        IrFunctionKind::View,
        Some(ct_amount_ty.clone()),
        span,
    );
    let owner = fb.add_param("owner", Ty::Address);
    let spender = fb.add_param("spender", Ty::Address);
    let key = fb.emit_instr(Opcode::Keccak, vec![owner, spender], Some(Ty::Hash));
    let alw_map = fb.emit_instr(Opcode::SLoad(allowances_id), vec![], Some(alw_map_ty));
    let handle = fb.emit_instr(Opcode::MapGet, vec![alw_map, key], Some(ct_amount_ty));
    fb.terminate(Terminator::Return(Some(handle)));
    fb.finish()
}

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

// =========================================================================
// Placeholder: allow dead_code for fields not yet exercised in V0.2 tests.
// =========================================================================
#[allow(dead_code)]
fn _unused(_: Value) {}

#[cfg(test)]
mod tests {
    use super::*;
    use covenant_diag::SourceId;
    use covenant_ir::module::IrMetadataValue;
    use covenant_parser::ast::{ConstructKind, Ident, PrivacyQualifier};

    fn make_confidential_module() -> IrModule {
        let span = covenant_diag::Span::new(SourceId::new(0), 0, 0);
        let mut m = IrModule {
            name: Ident {
                name: "TestToken".into(),
                span,
            },
            construct_kind: ConstructKind::Token,
            construct_privacy: Some(PrivacyQualifier::Confidential),
            fields: Vec::new(),
            structs: Vec::new(),
            functions: Vec::new(),
            external_contracts: Vec::new(),
            events: Vec::new(),
            errors: Vec::new(),
            choices: Vec::new(),
            metadata: Default::default(),
            anchor: None,
            upgradeable: None,
            source_id: SourceId::new(0),
        };
        m.metadata
            .insert("name".into(), IrMetadataValue::Text("Test Token".into()));
        m.metadata
            .insert("symbol".into(), IrMetadataValue::Text("TST".into()));
        m.metadata
            .insert("decimals".into(), IrMetadataValue::Integer(18));
        m
    }

    #[test]
    fn canonical_selectors_has_nine_entries() {
        assert_eq!(CANONICAL_SELECTORS.len(), 9);
    }

    #[test]
    fn canonical_selectors_includes_transfer_encrypted() {
        assert!(
            CANONICAL_SELECTORS
                .iter()
                .any(|(name, _)| *name == "transferEncrypted"),
            "CANONICAL_SELECTORS must include transferEncrypted"
        );
    }

    #[test]
    fn synthesize_produces_nine_functions() {
        let mut m = make_confidential_module();
        let config = crate::config::StdlibConfig::default();
        let mut diags = Vec::new();
        synthesize(&mut m, &config, &mut diags);
        assert_eq!(
            m.functions.len(),
            9,
            "expected 9 synthesized functions, got {}: {:?}",
            m.functions.len(),
            m.functions
                .iter()
                .map(|f| f.name.name.as_ref())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn synthesize_injects_two_events() {
        let mut m = make_confidential_module();
        let config = crate::config::StdlibConfig::default();
        let mut diags = Vec::new();
        synthesize(&mut m, &config, &mut diags);
        let event_names: Vec<&str> = m.events.iter().map(|e| e.name.name.as_ref()).collect();
        assert!(
            event_names.contains(&"TransferEncrypted"),
            "missing TransferEncrypted event"
        );
        assert!(
            event_names.contains(&"ApprovalEncrypted"),
            "missing ApprovalEncrypted event"
        );
    }

    #[test]
    fn synthesize_injects_two_errors() {
        let mut m = make_confidential_module();
        let config = crate::config::StdlibConfig::default();
        let mut diags = Vec::new();
        synthesize(&mut m, &config, &mut diags);
        let error_names: Vec<&str> = m.errors.iter().map(|e| e.name.name.as_ref()).collect();
        assert!(
            error_names.contains(&"InsufficientEncryptedBalance"),
            "missing InsufficientEncryptedBalance error"
        );
        assert!(
            error_names.contains(&"InsufficientEncryptedAllowance"),
            "missing InsufficientEncryptedAllowance error"
        );
    }

    #[test]
    fn synthesize_creates_balances_and_allowances_fields() {
        let mut m = make_confidential_module();
        let config = crate::config::StdlibConfig::default();
        let mut diags = Vec::new();
        synthesize(&mut m, &config, &mut diags);
        let field_names: Vec<&str> = m.fields.iter().map(|f| f.name.name.as_ref()).collect();
        assert!(field_names.contains(&"balances"), "missing balances field");
        assert!(
            field_names.contains(&"allowances"),
            "missing allowances field"
        );
        assert!(
            field_names.contains(&"total_supply"),
            "missing total_supply field"
        );
    }

    #[test]
    fn synthesize_no_conflict_warning_for_fresh_module() {
        let mut m = make_confidential_module();
        let config = crate::config::StdlibConfig::default();
        let mut diags = Vec::new();
        synthesize(&mut m, &config, &mut diags);
        // No error or conflict diagnostics for a fresh module.
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.level == covenant_diag::DiagnosticLevel::Error)
            .collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    }
}

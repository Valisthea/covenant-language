//! ERC-721 (NFT) interface synthesis.
//!
//! Synthesizes the standard ERC-721 surface into an `IrModule` whose
//! construct kind is `Nft`. Auto-injects required fields (`owners`,
//! `balances`, `token_approvals`, `operator_approvals`) if not already
//! present.
//!
//! Surface synthesized (mirrors OpenZeppelin ERC721.sol method-by-method):
//!   Functions (12):
//!     view  name() returns text
//!     view  symbol() returns text
//!     view  tokenURI(token_id: amount) returns text
//!     view  balanceOf(owner: address) returns amount
//!     view  ownerOf(token_id: amount) returns address
//!     view  getApproved(token_id: amount) returns address
//!     view  isApprovedForAll(owner: address, operator: address) returns bool
//!     action approve(spender: address, token_id: amount)
//!     action setApprovalForAll(operator: address, approved: bool)
//!     action transferFrom(from: address, to: address, token_id: amount)
//!                — gated: caller is owner OR approved OR operator;
//!                  reverts when to == address(0)
//!     action mint(to: address, token_id: amount)
//!     action burn(token_id: amount)
//!                — gated: caller is owner OR approved OR operator
//!
//!   Events (3): Transfer, Approval, ApprovalForAll
//!   Errors (5): NotTokenOwner, TokenAlreadyMinted, TokenDoesNotExist,
//!               NotApprovedOrOwner, InvalidReceiver
//!
//! Design choices:
//!   - `transferFrom` and `burn` enforce caller authorization (caller is
//!     the token owner, the per-token approved address, or an approved
//!     operator) and `transferFrom` reverts when `to == address(0)`,
//!     matching OpenZeppelin's `_isAuthorized` / `ERC721InvalidReceiver`
//!     semantics. Earlier (≤ V0.9.1) synthesis left both unchecked
//!     (V0.9.2 hardening — DEBT.md "ERC-721 transferFrom permissive").
//!   - `mint` is open-access. Add an explicit `only deployer` (or other
//!     guard) in source if you need access control. This matches the
//!     ERC-20 `token` synthesizer's `transfer` philosophy (the language
//!     handles the standard surface; access control is per-contract).
//!   - `tokenURI` returns `base_uri` concatenated with the token id's
//!     decimal representation. Compositional URI patterns (e.g. trailing
//!     `.json`) require user-declared override.
//!   - V0.9 does not synthesize `safeTransferFrom` — the receiver-hook
//!     callback dance adds external-call surface that V0.9.0 prefers to
//!     keep audit-explicit. Sprint 35.c may add it.

use std::collections::HashSet;

use covenant_diag::{Diagnostic, Span};
use covenant_ir::{
    function::IrFunctionKind,
    id::{FunctionId, GlobalId, Value},
    instr::{IrConstant, Terminator},
    module::{IrEvent, IrField, IrMetadataValue},
    IrError, IrModule, Opcode,
};
use covenant_parser::ast::Ident;
use covenant_privacy::PrivacyDomain;
use covenant_types::Ty;

use crate::builder::FuncBuilder;
use crate::config::StdlibConfig;
use crate::diag as d;

/// Entry point for ERC-721 synthesis on a single Nft module.
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
                diags.push(d::user_fn_conflict(span, name));
                return;
            } else {
                diags.push(d::warn_user_override(span, name));
            }
        }
    }

    // --- Resolve or inject required fields ---
    let owners_id = ensure_field(
        module,
        "owners",
        Ty::Map(Box::new(Ty::Amount), Box::new(Ty::Address)),
    );
    let balances_id = ensure_field(
        module,
        "balances",
        Ty::Map(Box::new(Ty::Address), Box::new(Ty::Amount)),
    );
    let token_approvals_id = ensure_field(
        module,
        "token_approvals",
        Ty::Map(Box::new(Ty::Amount), Box::new(Ty::Address)),
    );
    let operator_approvals_id = ensure_field(
        module,
        "operator_approvals",
        // Flattened key: keccak256(owner, operator) → bool
        Ty::Map(Box::new(Ty::Hash), Box::new(Ty::Bool)),
    );

    // --- Pull metadata (name / symbol / base_uri) ---
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
    let base_uri_text = module
        .metadata
        .get("base_uri")
        .and_then(|m| match m {
            IrMetadataValue::Text(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "".into());

    // --- Synthesize functions, skipping any user-declared ones ---
    let skip = |name: &str| user_fns.contains(name);
    let mut next_id = module.functions.len() as u32;

    if !skip("name") {
        module
            .functions
            .push(synth_text_view(next_id, "name", &name_text, span));
        next_id += 1;
    }
    if !skip("symbol") {
        module
            .functions
            .push(synth_text_view(next_id, "symbol", &symbol_text, span));
        next_id += 1;
    }
    if !skip("tokenURI") {
        module
            .functions
            .push(synth_token_uri(next_id, &base_uri_text, span));
        next_id += 1;
    }
    if !skip("balanceOf") {
        module
            .functions
            .push(synth_balance_of(next_id, balances_id, span));
        next_id += 1;
    }
    if !skip("ownerOf") {
        module
            .functions
            .push(synth_owner_of(next_id, owners_id, span));
        next_id += 1;
    }
    if !skip("getApproved") {
        module
            .functions
            .push(synth_get_approved(next_id, token_approvals_id, span));
        next_id += 1;
    }
    if !skip("isApprovedForAll") {
        module.functions.push(synth_is_approved_for_all(
            next_id,
            operator_approvals_id,
            span,
        ));
        next_id += 1;
    }
    if !skip("approve") {
        module
            .functions
            .push(synth_approve(next_id, owners_id, token_approvals_id, span));
        next_id += 1;
    }
    if !skip("setApprovalForAll") {
        module.functions.push(synth_set_approval_for_all(
            next_id,
            operator_approvals_id,
            span,
        ));
        next_id += 1;
    }
    if !skip("transferFrom") {
        module.functions.push(synth_transfer_from(
            next_id,
            owners_id,
            balances_id,
            token_approvals_id,
            operator_approvals_id,
            span,
        ));
        next_id += 1;
    }
    if !skip("mint") {
        module
            .functions
            .push(synth_mint(next_id, owners_id, balances_id, span));
        next_id += 1;
    }
    if !skip("burn") {
        module.functions.push(synth_burn(
            next_id,
            owners_id,
            balances_id,
            token_approvals_id,
            operator_approvals_id,
            span,
        ));
        // next_id += 1; // last write — discarded; keep increment style consistent
    }

    inject_events(module, span);
    inject_errors(module, span);
}

const STANDARD_FN_NAMES: &[&str] = &[
    "name",
    "symbol",
    "tokenURI",
    "balanceOf",
    "ownerOf",
    "getApproved",
    "isApprovedForAll",
    "approve",
    "setApprovalForAll",
    "transferFrom",
    "mint",
    "burn",
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
    if !existing.contains("Transfer") {
        module.events.push(IrEvent {
            name: i("Transfer"),
            params: vec![
                (i("from"), Ty::Address, true),
                (i("to"), Ty::Address, true),
                (i("token_id"), Ty::Amount, true),
            ],
            span,
        });
    }
    if !existing.contains("Approval") {
        module.events.push(IrEvent {
            name: i("Approval"),
            params: vec![
                (i("owner"), Ty::Address, true),
                (i("approved"), Ty::Address, true),
                (i("token_id"), Ty::Amount, true),
            ],
            span,
        });
    }
    if !existing.contains("ApprovalForAll") {
        module.events.push(IrEvent {
            name: i("ApprovalForAll"),
            params: vec![
                (i("owner"), Ty::Address, true),
                (i("operator"), Ty::Address, true),
                (i("approved"), Ty::Bool, false),
            ],
            span,
        });
    }
}

fn inject_errors(module: &mut IrModule, span: Span) {
    let existing: HashSet<Box<str>> = module.errors.iter().map(|e| e.name.name.clone()).collect();
    for name in [
        "NotTokenOwner",
        "TokenAlreadyMinted",
        "TokenDoesNotExist",
        "NotApprovedOrOwner",
        "InvalidReceiver",
    ] {
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
// Individual function synthesizers
// -------------------------------------------------------------------------

fn synth_text_view(next_id: u32, name: &str, text: &str, span: Span) -> covenant_ir::IrFunction {
    let mut fb = FuncBuilder::new(
        FunctionId(next_id),
        name,
        IrFunctionKind::View,
        Some(Ty::Text),
        span,
    );
    let v = fb.emit_const(IrConstant::Text(text.into()), Ty::Text);
    fb.terminate(Terminator::Return(Some(v)));
    fb.finish()
}

fn synth_token_uri(next_id: u32, base_uri: &str, span: Span) -> covenant_ir::IrFunction {
    // V0.9 simplification: return base_uri verbatim. The token_id
    // concatenation will land in V1.0 alongside `text` interpolation
    // primitives. For now, base_uri is the URI pattern; clients append
    // the id off-chain.
    //
    // This matches the OpenZeppelin pattern where `_baseURI()` is the
    // hook clients override; full `tokenURI(id)` composition is V1.0.
    let mut fb = FuncBuilder::new(
        FunctionId(next_id),
        "tokenURI",
        IrFunctionKind::View,
        Some(Ty::Text),
        span,
    );
    let _id_param = fb.add_param("token_id", Ty::Amount);
    let v = fb.emit_const(IrConstant::Text(base_uri.into()), Ty::Text);
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

fn synth_owner_of(next_id: u32, owners_id: GlobalId, span: Span) -> covenant_ir::IrFunction {
    let mut fb = FuncBuilder::new(
        FunctionId(next_id),
        "ownerOf",
        IrFunctionKind::View,
        Some(Ty::Address),
        span,
    );
    let token_id = fb.add_param("token_id", Ty::Amount);
    let map_base = fb.emit_instr(
        Opcode::SLoad(owners_id),
        vec![],
        Some(Ty::Map(Box::new(Ty::Amount), Box::new(Ty::Address))),
    );
    let owner = fb.emit_instr(Opcode::MapGet, vec![map_base, token_id], Some(Ty::Address));
    fb.terminate(Terminator::Return(Some(owner)));
    fb.finish()
}

fn synth_get_approved(
    next_id: u32,
    token_approvals_id: GlobalId,
    span: Span,
) -> covenant_ir::IrFunction {
    let mut fb = FuncBuilder::new(
        FunctionId(next_id),
        "getApproved",
        IrFunctionKind::View,
        Some(Ty::Address),
        span,
    );
    let token_id = fb.add_param("token_id", Ty::Amount);
    let map_base = fb.emit_instr(
        Opcode::SLoad(token_approvals_id),
        vec![],
        Some(Ty::Map(Box::new(Ty::Amount), Box::new(Ty::Address))),
    );
    let approved = fb.emit_instr(Opcode::MapGet, vec![map_base, token_id], Some(Ty::Address));
    fb.terminate(Terminator::Return(Some(approved)));
    fb.finish()
}

fn synth_is_approved_for_all(
    next_id: u32,
    operator_approvals_id: GlobalId,
    span: Span,
) -> covenant_ir::IrFunction {
    let mut fb = FuncBuilder::new(
        FunctionId(next_id),
        "isApprovedForAll",
        IrFunctionKind::View,
        Some(Ty::Bool),
        span,
    );
    let owner = fb.add_param("owner", Ty::Address);
    let operator = fb.add_param("operator", Ty::Address);
    let key = fb.emit_instr(Opcode::Keccak, vec![owner, operator], Some(Ty::Hash));
    let map_base = fb.emit_instr(
        Opcode::SLoad(operator_approvals_id),
        vec![],
        Some(Ty::Map(Box::new(Ty::Hash), Box::new(Ty::Bool))),
    );
    let approved = fb.emit_instr(Opcode::MapGet, vec![map_base, key], Some(Ty::Bool));
    fb.terminate(Terminator::Return(Some(approved)));
    fb.finish()
}

fn synth_approve(
    next_id: u32,
    owners_id: GlobalId,
    token_approvals_id: GlobalId,
    span: Span,
) -> covenant_ir::IrFunction {
    let mut fb = FuncBuilder::new(
        FunctionId(next_id),
        "approve",
        IrFunctionKind::Action,
        None,
        span,
    );
    let spender = fb.add_param("spender", Ty::Address);
    let token_id = fb.add_param("token_id", Ty::Amount);

    // Check caller == ownerOf(token_id)
    let caller = fb.emit_instr(Opcode::LoadCaller, vec![], Some(Ty::Address));
    let owners_map = fb.emit_instr(
        Opcode::SLoad(owners_id),
        vec![],
        Some(Ty::Map(Box::new(Ty::Amount), Box::new(Ty::Address))),
    );
    let owner = fb.emit_instr(
        Opcode::MapGet,
        vec![owners_map, token_id],
        Some(Ty::Address),
    );
    let is_owner = fb.emit_instr(Opcode::Eq, vec![caller, owner], Some(Ty::Bool));

    let do_approve = fb.new_block();
    let revert_block = fb.new_block();
    fb.terminate(Terminator::Branch {
        cond: is_owner,
        then_target: do_approve,
        then_args: Vec::new(),
        else_target: revert_block,
        else_args: Vec::new(),
    });

    fb.set_current(revert_block);
    fb.terminate(Terminator::Revert {
        error: Ident {
            name: "NotTokenOwner".into(),
            span,
        },
        args: Vec::new(),
    });

    fb.set_current(do_approve);
    let approvals_map = fb.emit_instr(
        Opcode::SLoad(token_approvals_id),
        vec![],
        Some(Ty::Map(Box::new(Ty::Amount), Box::new(Ty::Address))),
    );
    fb.emit_instr(Opcode::MapSet, vec![approvals_map, token_id, spender], None);
    fb.emit_instr(
        Opcode::Emit(Box::new(Ident {
            name: "Approval".into(),
            span,
        })),
        vec![owner, spender, token_id],
        None,
    );
    fb.terminate(Terminator::Return(None));
    fb.finish()
}

fn synth_set_approval_for_all(
    next_id: u32,
    operator_approvals_id: GlobalId,
    span: Span,
) -> covenant_ir::IrFunction {
    let mut fb = FuncBuilder::new(
        FunctionId(next_id),
        "setApprovalForAll",
        IrFunctionKind::Action,
        None,
        span,
    );
    let operator = fb.add_param("operator", Ty::Address);
    let approved = fb.add_param("approved", Ty::Bool);
    let caller = fb.emit_instr(Opcode::LoadCaller, vec![], Some(Ty::Address));
    let key = fb.emit_instr(Opcode::Keccak, vec![caller, operator], Some(Ty::Hash));
    let map_base = fb.emit_instr(
        Opcode::SLoad(operator_approvals_id),
        vec![],
        Some(Ty::Map(Box::new(Ty::Hash), Box::new(Ty::Bool))),
    );
    fb.emit_instr(Opcode::MapSet, vec![map_base, key, approved], None);
    fb.emit_instr(
        Opcode::Emit(Box::new(Ident {
            name: "ApprovalForAll".into(),
            span,
        })),
        vec![caller, operator, approved],
        None,
    );
    fb.terminate(Terminator::Return(None));
    fb.finish()
}

/// Emit the ERC-721 `_isAuthorized` predicate as a single `bool` Value:
///   caller == owner
///   OR token_approvals[token_id] == caller
///   OR operator_approvals[keccak(owner, caller)]
///
/// Mirrors OpenZeppelin's `_isAuthorized(owner, spender, tokenId)`. Shared by
/// `transferFrom` and `burn` so both gate on identical logic.
fn emit_is_authorized(
    fb: &mut FuncBuilder,
    owner: Value,
    caller: Value,
    token_id: Value,
    token_approvals_id: GlobalId,
    operator_approvals_id: GlobalId,
) -> Value {
    let is_owner = fb.emit_instr(Opcode::Eq, vec![caller, owner], Some(Ty::Bool));

    let approvals_map = fb.emit_instr(
        Opcode::SLoad(token_approvals_id),
        vec![],
        Some(Ty::Map(Box::new(Ty::Amount), Box::new(Ty::Address))),
    );
    let approved = fb.emit_instr(
        Opcode::MapGet,
        vec![approvals_map, token_id],
        Some(Ty::Address),
    );
    let is_approved = fb.emit_instr(Opcode::Eq, vec![approved, caller], Some(Ty::Bool));

    // operator_approvals uses the flattened key keccak(owner, operator) — same
    // derivation as setApprovalForAll / isApprovedForAll.
    let op_key = fb.emit_instr(Opcode::Keccak, vec![owner, caller], Some(Ty::Hash));
    let op_map = fb.emit_instr(
        Opcode::SLoad(operator_approvals_id),
        vec![],
        Some(Ty::Map(Box::new(Ty::Hash), Box::new(Ty::Bool))),
    );
    let is_operator = fb.emit_instr(Opcode::MapGet, vec![op_map, op_key], Some(Ty::Bool));

    let owner_or_approved = fb.emit_instr(
        Opcode::LogicalOr,
        vec![is_owner, is_approved],
        Some(Ty::Bool),
    );
    fb.emit_instr(
        Opcode::LogicalOr,
        vec![owner_or_approved, is_operator],
        Some(Ty::Bool),
    )
}

fn synth_transfer_from(
    next_id: u32,
    owners_id: GlobalId,
    balances_id: GlobalId,
    token_approvals_id: GlobalId,
    operator_approvals_id: GlobalId,
    span: Span,
) -> covenant_ir::IrFunction {
    let mut fb = FuncBuilder::new(
        FunctionId(next_id),
        "transferFrom",
        IrFunctionKind::Action,
        None,
        span,
    );
    let from = fb.add_param("from", Ty::Address);
    let to = fb.add_param("to", Ty::Address);
    let token_id = fb.add_param("token_id", Ty::Amount);

    let zero_addr = fb.emit_const(IrConstant::ZeroAddress, Ty::Address);

    // --- Guard 1: to != address(0) (ERC721InvalidReceiver) ---
    let to_nonzero = fb.emit_instr(Opcode::Ne, vec![to, zero_addr], Some(Ty::Bool));
    let check_owner = fb.new_block();
    let revert_receiver = fb.new_block();
    fb.terminate(Terminator::Branch {
        cond: to_nonzero,
        then_target: check_owner,
        then_args: Vec::new(),
        else_target: revert_receiver,
        else_args: Vec::new(),
    });
    fb.set_current(revert_receiver);
    fb.terminate(Terminator::Revert {
        error: Ident {
            name: "InvalidReceiver".into(),
            span,
        },
        args: Vec::new(),
    });

    // --- Guard 2: ownerOf(token_id) == from (NotTokenOwner) ---
    fb.set_current(check_owner);
    let owners_map = fb.emit_instr(
        Opcode::SLoad(owners_id),
        vec![],
        Some(Ty::Map(Box::new(Ty::Amount), Box::new(Ty::Address))),
    );
    let actual_owner = fb.emit_instr(
        Opcode::MapGet,
        vec![owners_map, token_id],
        Some(Ty::Address),
    );
    let owner_match = fb.emit_instr(Opcode::Eq, vec![actual_owner, from], Some(Ty::Bool));

    let check_auth = fb.new_block();
    let revert_owner = fb.new_block();
    fb.terminate(Terminator::Branch {
        cond: owner_match,
        then_target: check_auth,
        then_args: Vec::new(),
        else_target: revert_owner,
        else_args: Vec::new(),
    });
    fb.set_current(revert_owner);
    fb.terminate(Terminator::Revert {
        error: Ident {
            name: "NotTokenOwner".into(),
            span,
        },
        args: Vec::new(),
    });

    // --- Guard 3: caller is owner OR approved OR operator (NotApprovedOrOwner) ---
    fb.set_current(check_auth);
    let caller = fb.emit_instr(Opcode::LoadCaller, vec![], Some(Ty::Address));
    let authorized = emit_is_authorized(
        &mut fb,
        actual_owner,
        caller,
        token_id,
        token_approvals_id,
        operator_approvals_id,
    );
    let do_transfer = fb.new_block();
    let revert_auth = fb.new_block();
    fb.terminate(Terminator::Branch {
        cond: authorized,
        then_target: do_transfer,
        then_args: Vec::new(),
        else_target: revert_auth,
        else_args: Vec::new(),
    });
    fb.set_current(revert_auth);
    fb.terminate(Terminator::Revert {
        error: Ident {
            name: "NotApprovedOrOwner".into(),
            span,
        },
        args: Vec::new(),
    });

    // --- Effects ---
    fb.set_current(do_transfer);

    // Clear approval for this token (per ERC-721 spec)
    let approvals_map = fb.emit_instr(
        Opcode::SLoad(token_approvals_id),
        vec![],
        Some(Ty::Map(Box::new(Ty::Amount), Box::new(Ty::Address))),
    );
    fb.emit_instr(
        Opcode::MapSet,
        vec![approvals_map, token_id, zero_addr],
        None,
    );

    // owners[token_id] = to
    fb.emit_instr(Opcode::MapSet, vec![owners_map, token_id, to], None);

    // balances[from] -= 1
    let bal_map = fb.emit_instr(
        Opcode::SLoad(balances_id),
        vec![],
        Some(Ty::Map(Box::new(Ty::Address), Box::new(Ty::Amount))),
    );
    let from_bal = fb.emit_instr(Opcode::MapGet, vec![bal_map, from], Some(Ty::Amount));
    let one = fb.emit_const(IrConstant::Integer(1), Ty::Amount);
    let new_from = fb.emit_instr(Opcode::SubChecked, vec![from_bal, one], Some(Ty::Amount));
    fb.emit_instr(Opcode::MapSet, vec![bal_map, from, new_from], None);

    // balances[to] += 1
    let to_bal = fb.emit_instr(Opcode::MapGet, vec![bal_map, to], Some(Ty::Amount));
    let one_again = fb.emit_const(IrConstant::Integer(1), Ty::Amount);
    let new_to = fb.emit_instr(
        Opcode::AddChecked,
        vec![to_bal, one_again],
        Some(Ty::Amount),
    );
    fb.emit_instr(Opcode::MapSet, vec![bal_map, to, new_to], None);

    fb.emit_instr(
        Opcode::Emit(Box::new(Ident {
            name: "Transfer".into(),
            span,
        })),
        vec![from, to, token_id],
        None,
    );
    fb.terminate(Terminator::Return(None));
    fb.finish()
}

fn synth_burn(
    next_id: u32,
    owners_id: GlobalId,
    balances_id: GlobalId,
    token_approvals_id: GlobalId,
    operator_approvals_id: GlobalId,
    span: Span,
) -> covenant_ir::IrFunction {
    let mut fb = FuncBuilder::new(
        FunctionId(next_id),
        "burn",
        IrFunctionKind::Action,
        None,
        span,
    );
    let token_id = fb.add_param("token_id", Ty::Amount);
    let zero_addr = fb.emit_const(IrConstant::ZeroAddress, Ty::Address);

    // --- Guard 1: token exists (owners[id] != 0) → else TokenDoesNotExist ---
    let owners_map = fb.emit_instr(
        Opcode::SLoad(owners_id),
        vec![],
        Some(Ty::Map(Box::new(Ty::Amount), Box::new(Ty::Address))),
    );
    let owner = fb.emit_instr(
        Opcode::MapGet,
        vec![owners_map, token_id],
        Some(Ty::Address),
    );
    let exists = fb.emit_instr(Opcode::Ne, vec![owner, zero_addr], Some(Ty::Bool));

    let check_auth = fb.new_block();
    let revert_missing = fb.new_block();
    fb.terminate(Terminator::Branch {
        cond: exists,
        then_target: check_auth,
        then_args: Vec::new(),
        else_target: revert_missing,
        else_args: Vec::new(),
    });
    fb.set_current(revert_missing);
    fb.terminate(Terminator::Revert {
        error: Ident {
            name: "TokenDoesNotExist".into(),
            span,
        },
        args: Vec::new(),
    });

    // --- Guard 2: caller is owner OR approved OR operator (NotApprovedOrOwner) ---
    fb.set_current(check_auth);
    let caller = fb.emit_instr(Opcode::LoadCaller, vec![], Some(Ty::Address));
    let authorized = emit_is_authorized(
        &mut fb,
        owner,
        caller,
        token_id,
        token_approvals_id,
        operator_approvals_id,
    );
    let do_burn = fb.new_block();
    let revert_auth = fb.new_block();
    fb.terminate(Terminator::Branch {
        cond: authorized,
        then_target: do_burn,
        then_args: Vec::new(),
        else_target: revert_auth,
        else_args: Vec::new(),
    });
    fb.set_current(revert_auth);
    fb.terminate(Terminator::Revert {
        error: Ident {
            name: "NotApprovedOrOwner".into(),
            span,
        },
        args: Vec::new(),
    });

    // --- Effects: clear approval, owners[id] = 0, balances[owner] -= 1,
    //     emit Transfer(owner, 0, id) (ERC-721 burn convention) ---
    fb.set_current(do_burn);
    let approvals_map = fb.emit_instr(
        Opcode::SLoad(token_approvals_id),
        vec![],
        Some(Ty::Map(Box::new(Ty::Amount), Box::new(Ty::Address))),
    );
    fb.emit_instr(
        Opcode::MapSet,
        vec![approvals_map, token_id, zero_addr],
        None,
    );

    // owners[token_id] = address(0)
    fb.emit_instr(Opcode::MapSet, vec![owners_map, token_id, zero_addr], None);

    // balances[owner] -= 1
    let bal_map = fb.emit_instr(
        Opcode::SLoad(balances_id),
        vec![],
        Some(Ty::Map(Box::new(Ty::Address), Box::new(Ty::Amount))),
    );
    let owner_bal = fb.emit_instr(Opcode::MapGet, vec![bal_map, owner], Some(Ty::Amount));
    let one = fb.emit_const(IrConstant::Integer(1), Ty::Amount);
    let new_bal = fb.emit_instr(Opcode::SubChecked, vec![owner_bal, one], Some(Ty::Amount));
    fb.emit_instr(Opcode::MapSet, vec![bal_map, owner, new_bal], None);

    fb.emit_instr(
        Opcode::Emit(Box::new(Ident {
            name: "Transfer".into(),
            span,
        })),
        vec![owner, zero_addr, token_id],
        None,
    );
    fb.terminate(Terminator::Return(None));
    fb.finish()
}

fn synth_mint(
    next_id: u32,
    owners_id: GlobalId,
    balances_id: GlobalId,
    span: Span,
) -> covenant_ir::IrFunction {
    let mut fb = FuncBuilder::new(
        FunctionId(next_id),
        "mint",
        IrFunctionKind::Action,
        None,
        span,
    );
    let to = fb.add_param("to", Ty::Address);
    let token_id = fb.add_param("token_id", Ty::Amount);

    // Check token doesn't exist (owners[id] == 0)
    let owners_map = fb.emit_instr(
        Opcode::SLoad(owners_id),
        vec![],
        Some(Ty::Map(Box::new(Ty::Amount), Box::new(Ty::Address))),
    );
    let cur_owner = fb.emit_instr(
        Opcode::MapGet,
        vec![owners_map, token_id],
        Some(Ty::Address),
    );
    let zero_addr = fb.emit_const(IrConstant::ZeroAddress, Ty::Address);
    let is_unminted = fb.emit_instr(Opcode::Eq, vec![cur_owner, zero_addr], Some(Ty::Bool));

    let do_mint = fb.new_block();
    let revert_block = fb.new_block();
    fb.terminate(Terminator::Branch {
        cond: is_unminted,
        then_target: do_mint,
        then_args: Vec::new(),
        else_target: revert_block,
        else_args: Vec::new(),
    });
    fb.set_current(revert_block);
    fb.terminate(Terminator::Revert {
        error: Ident {
            name: "TokenAlreadyMinted".into(),
            span,
        },
        args: Vec::new(),
    });

    fb.set_current(do_mint);
    fb.emit_instr(Opcode::MapSet, vec![owners_map, token_id, to], None);

    // balances[to] += 1
    let bal_map = fb.emit_instr(
        Opcode::SLoad(balances_id),
        vec![],
        Some(Ty::Map(Box::new(Ty::Address), Box::new(Ty::Amount))),
    );
    let to_bal = fb.emit_instr(Opcode::MapGet, vec![bal_map, to], Some(Ty::Amount));
    let one = fb.emit_const(IrConstant::Integer(1), Ty::Amount);
    let new_to = fb.emit_instr(Opcode::AddChecked, vec![to_bal, one], Some(Ty::Amount));
    fb.emit_instr(Opcode::MapSet, vec![bal_map, to, new_to], None);

    fb.emit_instr(
        Opcode::Emit(Box::new(Ident {
            name: "Transfer".into(),
            span,
        })),
        vec![zero_addr, to, token_id],
        None,
    );
    fb.terminate(Terminator::Return(None));
    fb.finish()
}

/// Canonical ERC-721 selectors (4-byte ABI signatures).
/// Verifiable via `cast sig "<signature>"`. Used by playground / docs.
pub const CANONICAL_SELECTORS: &[(&str, [u8; 4])] = &[
    ("name", [0x06, 0xfd, 0xde, 0x03]),              // name()
    ("symbol", [0x95, 0xd8, 0x9b, 0x41]),            // symbol()
    ("tokenURI", [0xc8, 0x7b, 0x56, 0xdd]),          // tokenURI(uint256)
    ("balanceOf", [0x70, 0xa0, 0x82, 0x31]),         // balanceOf(address)
    ("ownerOf", [0x63, 0x52, 0x21, 0x1e]),           // ownerOf(uint256)
    ("getApproved", [0x08, 0x18, 0x12, 0xfc]),       // getApproved(uint256)
    ("isApprovedForAll", [0xe9, 0x85, 0xe9, 0xc5]),  // isApprovedForAll(address,address)
    ("approve", [0x09, 0x5e, 0xa7, 0xb3]),           // approve(address,uint256)
    ("setApprovalForAll", [0xa2, 0x2c, 0xb4, 0x65]), // setApprovalForAll(address,bool)
    ("transferFrom", [0x23, 0xb8, 0x72, 0xdd]),      // transferFrom(address,address,uint256)
    ("mint", [0x40, 0xc1, 0x0f, 0x19]),              // mint(address,uint256)
    ("burn", [0x42, 0x96, 0x6c, 0x68]),              // burn(uint256)
];

#[allow(dead_code)]
fn _mark(_: Value) {}

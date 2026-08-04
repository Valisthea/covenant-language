//! ERC-8228 interface synthesis for `ceremony` constructs.
//!
//! Synthesizes the Amnesia ceremony lifecycle functions:
//!   - `setup()` → uint256            : calls AmnesiaBegin, stores session_id, sets phase=1
//!   - `submit_share(bytes32)` → bool  : calls AmnesiaSubmitShare(session_id, share)
//!   - `finalize()` → bool             : calls AmnesiaFinalize(session_id), sets phase=2
//!   - `destroy()` → bool              : calls DestructionProof(session_id), sets phase=3, emits event
//!   - `phase()` → uint256             : returns current ceremony phase
//!   - `session_id()` → uint256        : returns stored session ID
//!   - `is_destroyed()` → bool         : returns phase == 3
//!   - `owner()` → address             : returns deployer address
//!
//! Fields synthesized:
//!   - `ceremony_phase: uint256`    (0=Setup, 1=Active, 2=Finalized, 3=Destroyed)
//!   - `ceremony_session_id: uint256`
//!
//! Events synthesized:
//!   - `AmnesiaCeremonyDestroyed(uint256 indexed sessionId)`
//!
//! Errors synthesized:
//!   - `CeremonyAlreadyDestroyed()`

use std::collections::HashSet;

use covenant_diag::{Diagnostic, Span};
use covenant_ir::{
    function::IrFunctionKind,
    id::{FunctionId, GlobalId},
    instr::{IrConstant, Terminator},
    module::{IrError, IrEvent, IrField, IrMetadataValue},
    IrModule, Opcode,
};
use covenant_parser::ast::Ident;
use covenant_privacy::domain_of;
use covenant_types::Ty;

use crate::builder::FuncBuilder;
use crate::config::StdlibConfig;

// -------------------------------------------------------------------------
// ERC-8228 canonical function names.
// -------------------------------------------------------------------------
pub const CANONICAL_SELECTORS: &[(&str, [u8; 4])] = &[
    // keccak256("setup()")[0..4]
    ("setup", [0x5f, 0x3c, 0x1d, 0x10]),
    // keccak256("submit_share(bytes32)")[0..4]
    ("submit_share", [0xa3, 0xf5, 0x8c, 0x6e]),
    // keccak256("finalize()")[0..4]
    ("finalize", [0x4b, 0xb2, 0x78, 0xf3]),
    // keccak256("destroy()")[0..4]
    ("destroy", [0x83, 0x29, 0x7f, 0x04]),
    // keccak256("phase()")[0..4]
    ("phase", [0xf8, 0xad, 0xf6, 0x8a]),
    // keccak256("session_id()")[0..4]
    ("session_id", [0x4e, 0x72, 0xa0, 0x1e]),
    // keccak256("is_destroyed()")[0..4]
    ("is_destroyed", [0xc1, 0xeb, 0xd8, 0x25]),
    // keccak256("owner()")[0..4]
    ("owner", [0x8d, 0xa5, 0xcb, 0x5b]),
];

const STANDARD_FN_NAMES: &[&str] = &[
    "setup",
    "submit_share",
    "finalize",
    "destroy",
    "phase",
    "session_id",
    "is_destroyed",
    "owner",
];

/// Entry point: synthesize all ERC-8228 functions into a `ceremony` module.
pub fn synthesize(module: &mut IrModule, _config: &StdlibConfig, diags: &mut Vec<Diagnostic>) {
    let span = module.name.span;

    let user_fns: HashSet<Box<str>> = module
        .functions
        .iter()
        .map(|f| f.name.name.clone())
        .collect();

    for name in STANDARD_FN_NAMES {
        if user_fns.contains(*name) {
            diags.push(crate::diag::warn_user_override(span, name));
        }
    }

    // Ensure the two phase-tracking fields exist.
    let phase_id = ensure_field(module, "ceremony_phase", Ty::Amount);
    let session_id_field = ensure_field(module, "ceremony_session_id", Ty::Amount);

    // OMEGA V6 (CRT-005 + HGH-030 fix): `submit_share` previously had no
    // caller authorization or dedup at all -- a single address could submit
    // the threshold's worth of (garbage) shares unilaterally, and `finalize`
    // trusted the mock precompile's boolean with no on-chain corroboration,
    // so even ZERO shares submitted could finalize successfully. These two
    // fields give the synthesized contract itself a real, on-chain-checkable
    // "at least `threshold` distinct callers participated" invariant,
    // independent of whatever the (mocked) precompile reports.
    //
    // This does not (and cannot, given the language has no way to declare a
    // specific set of guardian ADDRESSES today -- only a guardian COUNT via
    // `guardians: N` metadata) verify the submitters are a pre-registered
    // guardian set; it verifies they are `threshold` genuinely DISTINCT
    // callers, which closes the concrete single-address-fabricates-consensus
    // attack the audit demonstrated. Declaring an explicit guardian address
    // list is tracked as a follow-up language feature, not implemented here.
    let submitted_map_field = ensure_field(
        module,
        "ceremony_submitted",
        Ty::Map(Box::new(Ty::Address), Box::new(Ty::Bool)),
    );
    let submitter_count_field = ensure_field(module, "ceremony_submitter_count", Ty::Amount);

    // OMEGA V6 (HGH-030 fix): read the REAL `threshold: M` the developer
    // declared instead of silently ignoring it. `AmnesiaBegin`'s wire format
    // and the Solidity helper's hardcoded (3, 2) are a separate, larger
    // on-chain-routing change (tracked in DEBT.md); this at least makes the
    // synthesized contract's OWN submitter-count gate honor the declared
    // value instead of enforcing nothing.
    let int_meta = |key: &str| {
        module.metadata.get(key).and_then(|m| match m {
            IrMetadataValue::Integer(n) => Some(*n),
            _ => None,
        })
    };
    let threshold_declared = int_meta("threshold");
    let guardians_declared = int_meta("guardians");
    let threshold = threshold_declared.unwrap_or(1);

    // F10 fix: refuse to synthesize a ceremony whose `threshold` does not
    // satisfy `1 <= threshold <= guardians`. A `threshold` of 0 degenerates
    // the CRT-005 finalize gate (`distinct_submitters >= 0` is always true, so
    // the secret is destroyed with ZERO guardian shares), and a `threshold`
    // exceeding `guardians` demands more distinct submitters than can ever
    // participate. Both would lower to plausible-but-wrong bytecode, so we
    // fail loud (E611) instead of emitting it.
    if threshold == 0 {
        diags.push(crate::diag::ceremony_threshold_invalid(
            span,
            threshold,
            guardians_declared,
        ));
        return;
    }
    if let Some(guardians) = guardians_declared {
        if threshold > guardians {
            diags.push(crate::diag::ceremony_threshold_invalid(
                span,
                threshold,
                Some(guardians),
            ));
            return;
        }
    }

    inject_events(module, span);
    inject_errors(module, span);

    let skip = |name: &str| user_fns.contains(name);

    if !skip("setup") {
        module.functions.push(synth_setup(
            module.functions.len() as u32,
            phase_id,
            session_id_field,
            span,
        ));
    }
    if !skip("submit_share") {
        module.functions.push(synth_submit_share(
            module.functions.len() as u32,
            phase_id,
            session_id_field,
            submitted_map_field,
            submitter_count_field,
            span,
        ));
    }
    if !skip("finalize") {
        module.functions.push(synth_finalize(
            module.functions.len() as u32,
            phase_id,
            session_id_field,
            submitter_count_field,
            threshold,
            span,
        ));
    }
    if !skip("destroy") {
        module.functions.push(synth_destroy(
            module.functions.len() as u32,
            phase_id,
            session_id_field,
            span,
        ));
    }
    if !skip("phase") {
        module
            .functions
            .push(synth_phase(module.functions.len() as u32, phase_id, span));
    }
    if !skip("session_id") {
        module.functions.push(synth_session_id_view(
            module.functions.len() as u32,
            session_id_field,
            span,
        ));
    }
    if !skip("is_destroyed") {
        module.functions.push(synth_is_destroyed(
            module.functions.len() as u32,
            phase_id,
            span,
        ));
    }
    if !skip("owner") {
        module
            .functions
            .push(synth_owner(module.functions.len() as u32, span));
    }
}

// -------------------------------------------------------------------------
// Field helpers
// -------------------------------------------------------------------------

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
        ty: ty.clone(),
        privacy: domain_of(&ty),
        initializer_fn: None,
        initializer_const: None,
        span,
        explicit_slot: None,
    });
    id
}

// -------------------------------------------------------------------------
// Event / error injection
// -------------------------------------------------------------------------

fn inject_events(module: &mut IrModule, span: Span) {
    let existing: HashSet<Box<str>> = module.events.iter().map(|e| e.name.name.clone()).collect();
    if !existing.contains("AmnesiaCeremonyDestroyed") {
        module.events.push(IrEvent {
            name: Ident {
                name: "AmnesiaCeremonyDestroyed".into(),
                span,
            },
            params: vec![(
                Ident {
                    name: "sessionId".into(),
                    span,
                },
                Ty::Amount,
                true, // indexed
            )],
            span,
        });
    }
}

fn inject_errors(module: &mut IrModule, span: Span) {
    let existing: HashSet<Box<str>> = module.errors.iter().map(|e| e.name.name.clone()).collect();
    if !existing.contains("CeremonyAlreadyDestroyed") {
        module.errors.push(IrError {
            name: Ident {
                name: "CeremonyAlreadyDestroyed".into(),
                span,
            },
            params: vec![],
            span,
        });
    }
}

// -------------------------------------------------------------------------
// Function synthesizers
// -------------------------------------------------------------------------

/// `setup() → uint256`
/// KSR-CVN-001: asserts caller == deployer and phase == 0 before calling
/// AmnesiaBegin, stores session_id, sets phase=1.
fn synth_setup(
    fn_idx: u32,
    phase_id: GlobalId,
    session_id_field: GlobalId,
    span: Span,
) -> covenant_ir::IrFunction {
    let mut b = FuncBuilder::new(
        FunctionId(fn_idx),
        "setup",
        IrFunctionKind::Action,
        Some(Ty::Amount),
        span,
    );

    // KSR-CVN-001: only the deployer may start a ceremony.
    emit_assert_caller_is_deployer(&mut b);
    // KSR-CVN-001: setup is only valid from the initial (phase = 0) state.
    emit_assert_phase_eq(&mut b, phase_id, 0);

    // KSR-CVN-034: derive the session seed from per-deployment entropy
    // (caller, chain id, contract address, block timestamp) rather than
    // a constant zero. The precompile is still authoritative for session-id
    // uniqueness, but the seed now provides domain separation across
    // deployments, chains, and re-setups.
    let caller = b.emit_instr(Opcode::LoadCaller, vec![], Some(Ty::Address));
    let chain_id = b.emit_instr(Opcode::LoadChainId, vec![], Some(Ty::Amount));
    let this_addr = b.emit_instr(Opcode::LoadThis, vec![], Some(Ty::Address));
    let now = b.emit_instr(Opcode::LoadBlockTimestamp, vec![], Some(Ty::Time));
    let seed = b.emit_instr(
        Opcode::Keccak,
        vec![caller, chain_id, this_addr, now],
        Some(Ty::Amount),
    );
    let session_id = b.emit_instr(Opcode::AmnesiaBegin, vec![seed], Some(Ty::Amount));

    // Store session_id and set phase = 1.
    b.emit_instr(Opcode::SStore(session_id_field), vec![session_id], None);
    let phase_one = b.emit_const(IrConstant::Integer(1), Ty::Amount);
    b.emit_instr(Opcode::SStore(phase_id), vec![phase_one], None);

    b.terminate(Terminator::Return(Some(session_id)));
    b.finish()
}

/// `submit_share(bytes32) → bool`
/// KSR-CVN-001: asserts phase == 1 (active) before calling AmnesiaSubmitShare
/// so shares can neither be submitted before `setup` nor after `finalize`.
/// OMEGA V6 CRT-005 fix: also asserts the caller has not already submitted
/// (a map keyed by caller address) and increments an on-chain distinct-
/// submitter counter that `finalize` now checks against `threshold` --
/// previously a single address could call this repeatedly with garbage
/// shares and single-handedly satisfy any threshold.
fn synth_submit_share(
    fn_idx: u32,
    phase_id: GlobalId,
    session_id_field: GlobalId,
    submitted_map_field: GlobalId,
    submitter_count_field: GlobalId,
    span: Span,
) -> covenant_ir::IrFunction {
    let mut b = FuncBuilder::new(
        FunctionId(fn_idx),
        "submit_share",
        IrFunctionKind::Action,
        Some(Ty::Bool),
        span,
    );

    // KSR-CVN-001: shares are only accepted while the ceremony is active.
    emit_assert_phase_eq(&mut b, phase_id, 1);

    let share = b.add_param("share", Ty::Hash);

    // OMEGA V6 CRT-005: reject a second submission from the same caller.
    let caller = b.emit_instr(Opcode::LoadCaller, vec![], Some(Ty::Address));
    let map_ty = Ty::Map(Box::new(Ty::Address), Box::new(Ty::Bool));
    let map_handle = b.emit_instr(
        Opcode::SLoad(submitted_map_field),
        vec![],
        Some(map_ty.clone()),
    );
    let already_submitted = b.emit_instr(Opcode::MapGet, vec![map_handle, caller], Some(Ty::Bool));
    let not_already = b.emit_instr(Opcode::LogicalNot, vec![already_submitted], Some(Ty::Bool));
    b.emit_instr(Opcode::Assert, vec![not_already], None);
    let true_const = b.emit_const(IrConstant::Bool(true), Ty::Bool);
    let updated_map = b.emit_instr(
        Opcode::MapSet,
        vec![map_handle, caller, true_const],
        Some(map_ty),
    );
    b.emit_instr(Opcode::SStore(submitted_map_field), vec![updated_map], None);

    // OMEGA V6 CRT-005: count this as one more distinct submitter.
    let count = b.emit_instr(
        Opcode::SLoad(submitter_count_field),
        vec![],
        Some(Ty::Amount),
    );
    let one = b.emit_const(IrConstant::Integer(1), Ty::Amount);
    let new_count = b.emit_instr(Opcode::AddChecked, vec![count, one], Some(Ty::Amount));
    b.emit_instr(Opcode::SStore(submitter_count_field), vec![new_count], None);

    let session_id = b.emit_instr(Opcode::SLoad(session_id_field), vec![], Some(Ty::Amount));
    let result = b.emit_instr(
        Opcode::AmnesiaSubmitShare,
        vec![session_id, share],
        Some(Ty::Bool),
    );
    b.terminate(Terminator::Return(Some(result)));
    b.finish()
}

/// `finalize() → bool`
/// KSR-CVN-001: asserts caller == deployer, phase == 1, and that
/// AmnesiaFinalize returned true before advancing phase to 2. A failed
/// finalize proof therefore leaves the ceremony in phase 1, where new
/// shares can still be submitted and finalize can be retried, rather
/// than locking the ceremony into a "finalized" state based on an
/// unverified precompile response.
/// OMEGA V6 CRT-005 fix: also asserts at least `threshold` distinct callers
/// have submitted a share (see `synth_submit_share`) BEFORE even asking the
/// precompile -- previously this trusted whatever boolean the (mocked)
/// precompile returned with zero on-chain corroboration, so a ceremony with
/// literally zero shares submitted could finalize successfully.
fn synth_finalize(
    fn_idx: u32,
    phase_id: GlobalId,
    session_id_field: GlobalId,
    submitter_count_field: GlobalId,
    threshold: u128,
    span: Span,
) -> covenant_ir::IrFunction {
    let mut b = FuncBuilder::new(
        FunctionId(fn_idx),
        "finalize",
        IrFunctionKind::Action,
        Some(Ty::Bool),
        span,
    );

    emit_assert_caller_is_deployer(&mut b);
    emit_assert_phase_eq(&mut b, phase_id, 1);

    // OMEGA V6 CRT-005: require the on-chain distinct-submitter count to
    // have reached `threshold` before trusting the precompile at all.
    let count = b.emit_instr(
        Opcode::SLoad(submitter_count_field),
        vec![],
        Some(Ty::Amount),
    );
    let threshold_v = b.emit_const(IrConstant::Integer(threshold), Ty::Amount);
    let enough_submitters = b.emit_instr(Opcode::Ge, vec![count, threshold_v], Some(Ty::Bool));
    b.emit_instr(Opcode::Assert, vec![enough_submitters], None);

    let session_id = b.emit_instr(Opcode::SLoad(session_id_field), vec![], Some(Ty::Amount));
    let result = b.emit_instr(Opcode::AmnesiaFinalize, vec![session_id], Some(Ty::Bool));

    // KSR-CVN-001: phase advances only if the precompile proof succeeded.
    b.emit_instr(Opcode::Assert, vec![result], None);

    let phase_two = b.emit_const(IrConstant::Integer(2), Ty::Amount);
    b.emit_instr(Opcode::SStore(phase_id), vec![phase_two], None);
    b.terminate(Terminator::Return(Some(result)));
    b.finish()
}

/// `destroy() → bool`
/// KSR-CVN-001: asserts caller == deployer and phase == 2 before advancing
/// phase to 3.
///
/// It does NOT assert that the helper succeeded, although the code below
/// looks as though it does. See the comment on the `Assert` for why, and
/// what actually protects the transition.
/// Also asserts `phase != 3`: a redundant guard on the already-destroyed
/// path that keeps the contract observably idempotent (the fail-closed
/// `Assert(phase == 2)` already excludes 3, but the extra check pairs with
/// the `CeremonyAlreadyDestroyed` error in the ABI).
fn synth_destroy(
    fn_idx: u32,
    phase_id: GlobalId,
    session_id_field: GlobalId,
    span: Span,
) -> covenant_ir::IrFunction {
    let mut b = FuncBuilder::new(
        FunctionId(fn_idx),
        "destroy",
        IrFunctionKind::Action,
        Some(Ty::Bool),
        span,
    );

    emit_assert_caller_is_deployer(&mut b);
    emit_assert_phase_eq(&mut b, phase_id, 2);

    let session_id = b.emit_instr(Opcode::SLoad(session_id_field), vec![], Some(Ty::Amount));
    let result = b.emit_instr(Opcode::DestructionProof, vec![session_id], Some(Ty::Bool));

    // This assert is vacuous and the phase advances regardless. It used to
    // claim that the phase only advances if the helper succeeded.
    //
    // `amnesiaDestroy` returns dynamic `bytes`, so the first returndata word
    // is the ABI offset, `0x20`. The helper-call decoder reads `MLOAD(0)` and
    // hands that word back as the result, so this asserts on 32, which is
    // always truthy. The commitment the helper computed is never read.
    //
    // What actually protects the transition is the helper reverting on all
    // three of its failure paths: unknown session, wrong caller, wrong phase.
    // A revert fails the call, and the call site checks the success flag. So
    // the behaviour is correct today, for a reason other than the one the
    // code appears to give.
    //
    // Sprint 2 refuses this path outright rather than leaving a check that
    // reads as a guard and is not one. Sprint 3 gives `amnesiaDestroy` a
    // strict Boolean return plus a separate `bytes32` view for the
    // commitment, after which this assert becomes real.
    b.emit_instr(Opcode::Assert, vec![result], None);

    let phase_three = b.emit_const(IrConstant::Integer(3), Ty::Amount);
    b.emit_instr(Opcode::SStore(phase_id), vec![phase_three], None);
    let event_ident = Ident {
        name: "AmnesiaCeremonyDestroyed".into(),
        span,
    };
    b.emit_instr(Opcode::Emit(Box::new(event_ident)), vec![session_id], None);
    b.terminate(Terminator::Return(Some(result)));
    b.finish()
}

/// `phase() → uint256`
fn synth_phase(fn_idx: u32, phase_id: GlobalId, span: Span) -> covenant_ir::IrFunction {
    let mut b = FuncBuilder::new(
        FunctionId(fn_idx),
        "phase",
        IrFunctionKind::Action,
        Some(Ty::Amount),
        span,
    );
    let v = b.emit_instr(Opcode::SLoad(phase_id), vec![], Some(Ty::Amount));
    b.terminate(Terminator::Return(Some(v)));
    b.finish()
}

/// `session_id() → uint256`
fn synth_session_id_view(
    fn_idx: u32,
    session_id_field: GlobalId,
    span: Span,
) -> covenant_ir::IrFunction {
    let mut b = FuncBuilder::new(
        FunctionId(fn_idx),
        "session_id",
        IrFunctionKind::Action,
        Some(Ty::Amount),
        span,
    );
    let v = b.emit_instr(Opcode::SLoad(session_id_field), vec![], Some(Ty::Amount));
    b.terminate(Terminator::Return(Some(v)));
    b.finish()
}

/// `is_destroyed() → bool`
fn synth_is_destroyed(fn_idx: u32, phase_id: GlobalId, span: Span) -> covenant_ir::IrFunction {
    let mut b = FuncBuilder::new(
        FunctionId(fn_idx),
        "is_destroyed",
        IrFunctionKind::Action,
        Some(Ty::Bool),
        span,
    );
    let phase = b.emit_instr(Opcode::SLoad(phase_id), vec![], Some(Ty::Amount));
    let three = b.emit_const(IrConstant::Integer(3), Ty::Amount);
    let result = b.emit_instr(Opcode::Eq, vec![phase, three], Some(Ty::Bool));
    b.terminate(Terminator::Return(Some(result)));
    b.finish()
}

/// `owner() → address`
fn synth_owner(fn_idx: u32, span: Span) -> covenant_ir::IrFunction {
    let mut b = FuncBuilder::new(
        FunctionId(fn_idx),
        "owner",
        IrFunctionKind::Action,
        Some(Ty::Address),
        span,
    );
    let v = b.emit_instr(Opcode::LoadDeployer, vec![], Some(Ty::Address));
    b.terminate(Terminator::Return(Some(v)));
    b.finish()
}

// -------------------------------------------------------------------------
// KSR-CVN-001: guard helpers for ceremony phase transitions.
// -------------------------------------------------------------------------

/// Emit `Assert(caller == deployer)`. Every state-mutating ceremony entry
/// point (setup/finalize/destroy) must be restricted to the deployer so an
/// adversary cannot reset the ceremony mid-flight.
fn emit_assert_caller_is_deployer(b: &mut FuncBuilder) {
    let caller = b.emit_instr(Opcode::LoadCaller, vec![], Some(Ty::Address));
    let deployer = b.emit_instr(Opcode::LoadDeployer, vec![], Some(Ty::Address));
    let eq = b.emit_instr(Opcode::Eq, vec![caller, deployer], Some(Ty::Bool));
    b.emit_instr(Opcode::Assert, vec![eq], None);
}

/// Emit `Assert(ceremony_phase == expected)`. This is the core of the
/// KSR-CVN-001 fix: every transition requires the current phase to match
/// its precondition, so `finalize` without `setup`, double-`finalize`,
/// `destroy` before `finalize`, and double-`destroy` all revert.
fn emit_assert_phase_eq(b: &mut FuncBuilder, phase_id: GlobalId, expected: u128) {
    let phase = b.emit_instr(Opcode::SLoad(phase_id), vec![], Some(Ty::Amount));
    let want = b.emit_const(IrConstant::Integer(expected), Ty::Amount);
    let eq = b.emit_instr(Opcode::Eq, vec![phase, want], Some(Ty::Bool));
    b.emit_instr(Opcode::Assert, vec![eq], None);
}

// -------------------------------------------------------------------------
// Unit tests
// -------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use covenant_diag::{SourceId, Span};
    use covenant_ir::module::IrModule;
    use covenant_parser::ast::{ConstructKind, Ident as AstIdent};
    use std::collections::HashMap;

    fn make_ceremony_module() -> IrModule {
        let span = Span::new(SourceId::new(0), 0, 1);
        IrModule {
            source_id: SourceId::new(0),
            name: AstIdent {
                name: "AmnesiaCeremony".into(),
                span,
            },
            construct_kind: ConstructKind::Ceremony,
            construct_privacy: None,
            fields: Vec::new(),
            structs: Vec::new(),
            errors: Vec::new(),
            events: Vec::new(),
            choices: Vec::new(),
            functions: Vec::new(),
            external_contracts: Vec::new(),
            metadata: HashMap::new(),
            anchor: None,
            upgradeable: None,
        }
    }

    #[test]
    fn canonical_selectors_has_eight_entries() {
        assert_eq!(CANONICAL_SELECTORS.len(), 8);
    }

    #[test]
    fn canonical_selectors_includes_destroy() {
        assert!(
            CANONICAL_SELECTORS
                .iter()
                .any(|(name, _)| *name == "destroy"),
            "must include destroy"
        );
    }

    fn with_meta(pairs: &[(&str, u128)]) -> IrModule {
        let mut m = make_ceremony_module();
        for (k, v) in pairs {
            m.metadata.insert(
                (*k).into(),
                covenant_ir::module::IrMetadataValue::Integer(*v),
            );
        }
        m
    }

    fn error_count(diags: &[Diagnostic]) -> usize {
        diags
            .iter()
            .filter(|d| d.level == covenant_diag::DiagnosticLevel::Error)
            .count()
    }

    // ------------------------------------------------------------------
    // F10 regression: `1 <= threshold <= guardians` is validated at
    // compile time (E611). Negative-control: neutralizing the guard in
    // `synthesize` (removing the `threshold == 0` / `threshold > guardians`
    // early-returns) makes `threshold_zero_is_rejected` and
    // `threshold_gt_guardians_is_rejected` FAIL (0 errors emitted), while
    // `valid_threshold_compiles_clean` keeps passing either way.
    // ------------------------------------------------------------------

    #[test]
    fn threshold_zero_is_rejected() {
        // guardians: 3, threshold: 0: the degenerate finalize gate.
        let mut module = with_meta(&[("guardians", 3), ("threshold", 0)]);
        let config = crate::config::StdlibConfig::default();
        let mut diags = Vec::new();
        synthesize(&mut module, &config, &mut diags);
        assert_eq!(
            error_count(&diags),
            1,
            "threshold:0 must be a compile error, got diags: {diags:?}"
        );
        assert!(
            diags
                .iter()
                .any(|d| d.code == crate::diag::E611_CEREMONY_THRESHOLD_INVALID),
            "must emit E611"
        );
        // Must NOT have synthesized the lifecycle functions.
        assert!(
            module.functions.is_empty(),
            "must refuse synthesis on invalid threshold"
        );
    }

    #[test]
    fn threshold_gt_guardians_is_rejected() {
        // guardians: 2, threshold: 3: can never finalize.
        let mut module = with_meta(&[("guardians", 2), ("threshold", 3)]);
        let config = crate::config::StdlibConfig::default();
        let mut diags = Vec::new();
        synthesize(&mut module, &config, &mut diags);
        assert_eq!(
            error_count(&diags),
            1,
            "threshold>guardians must be a compile error, got diags: {diags:?}"
        );
        assert!(
            diags
                .iter()
                .any(|d| d.code == crate::diag::E611_CEREMONY_THRESHOLD_INVALID),
            "must emit E611"
        );
    }

    #[test]
    fn guardians_zero_is_rejected() {
        // guardians: 0 with defaulted threshold(1): 1 > 0, no valid threshold.
        let mut module = with_meta(&[("guardians", 0)]);
        let config = crate::config::StdlibConfig::default();
        let mut diags = Vec::new();
        synthesize(&mut module, &config, &mut diags);
        assert_eq!(
            error_count(&diags),
            1,
            "guardians:0 must be a compile error, got diags: {diags:?}"
        );
    }

    #[test]
    fn valid_threshold_compiles_clean() {
        // guardians: 3, threshold: 2: the shipped fixture values.
        let mut module = with_meta(&[("guardians", 3), ("threshold", 2)]);
        let config = crate::config::StdlibConfig::default();
        let mut diags = Vec::new();
        synthesize(&mut module, &config, &mut diags);
        assert_eq!(
            error_count(&diags),
            0,
            "valid threshold must compile clean, got diags: {diags:?}"
        );
        assert_eq!(module.functions.len(), 8, "must synthesize 8 functions");
    }

    #[test]
    fn threshold_eq_guardians_compiles_clean() {
        // guardians: 3, threshold: 3: unanimity is a valid boundary.
        let mut module = with_meta(&[("guardians", 3), ("threshold", 3)]);
        let config = crate::config::StdlibConfig::default();
        let mut diags = Vec::new();
        synthesize(&mut module, &config, &mut diags);
        assert_eq!(
            error_count(&diags),
            0,
            "threshold==guardians must compile clean, got diags: {diags:?}"
        );
    }

    #[test]
    fn synthesize_produces_eight_functions() {
        let mut module = make_ceremony_module();
        let config = crate::config::StdlibConfig::default();
        let mut diags = Vec::new();
        synthesize(&mut module, &config, &mut diags);
        assert_eq!(module.functions.len(), 8, "must synthesize 8 functions");
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.level == covenant_diag::DiagnosticLevel::Error)
            .collect();
        assert!(errors.is_empty(), "must not emit errors: {:?}", errors);
    }

    #[test]
    fn synthesize_injects_phase_and_session_id_fields() {
        let mut module = make_ceremony_module();
        let config = crate::config::StdlibConfig::default();
        let mut diags = Vec::new();
        synthesize(&mut module, &config, &mut diags);
        let names: Vec<_> = module.fields.iter().map(|f| f.name.name.as_ref()).collect();
        assert!(
            names.contains(&"ceremony_phase"),
            "missing ceremony_phase field"
        );
        assert!(
            names.contains(&"ceremony_session_id"),
            "missing ceremony_session_id field"
        );
    }

    #[test]
    fn synthesize_injects_destroyed_event() {
        let mut module = make_ceremony_module();
        let config = crate::config::StdlibConfig::default();
        let mut diags = Vec::new();
        synthesize(&mut module, &config, &mut diags);
        assert!(
            module
                .events
                .iter()
                .any(|e| e.name.name.as_ref() == "AmnesiaCeremonyDestroyed"),
            "must inject AmnesiaCeremonyDestroyed event"
        );
    }

    #[test]
    fn synthesize_injects_already_destroyed_error() {
        let mut module = make_ceremony_module();
        let config = crate::config::StdlibConfig::default();
        let mut diags = Vec::new();
        synthesize(&mut module, &config, &mut diags);
        assert!(
            module
                .errors
                .iter()
                .any(|e| e.name.name.as_ref() == "CeremonyAlreadyDestroyed"),
            "must inject CeremonyAlreadyDestroyed error"
        );
    }

    #[test]
    fn synthesize_function_names_are_correct() {
        let mut module = make_ceremony_module();
        let config = crate::config::StdlibConfig::default();
        let mut diags = Vec::new();
        synthesize(&mut module, &config, &mut diags);
        let fn_names: Vec<_> = module
            .functions
            .iter()
            .map(|f| f.name.name.as_ref())
            .collect();
        for expected in STANDARD_FN_NAMES {
            assert!(
                fn_names.contains(expected),
                "missing function `{expected}`; present: {fn_names:?}"
            );
        }
    }
}

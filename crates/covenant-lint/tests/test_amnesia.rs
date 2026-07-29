//! KSR-CVN-025: C801 ceremony phase monotonicity regression tests.

use std::collections::HashMap;

use covenant_diag::{SourceId, Span};
use covenant_ir::{
    block::IrBlock,
    function::{IrFunction, IrFunctionKind, IrGuard},
    id::{BlockId, FunctionId, GlobalId, Value},
    instr::{Instr, InstrMetadata, IrConstant, Terminator, ValueInfo},
    module::{IrField, IrModule},
    Opcode,
};
use covenant_lint::detectors::amnesia::C801CeremonyPhaseBackwardTransition;
use covenant_lint::framework::Detector;
use covenant_parser::ast::Ident;
use covenant_privacy::PrivacyDomain;
use covenant_types::Ty;

fn span() -> Span {
    Span::new(SourceId::new(0), 0, 0)
}
fn ident(n: &str) -> Ident {
    Ident {
        name: n.into(),
        span: span(),
    }
}

fn module_with_phase() -> (IrModule, GlobalId) {
    let phase_id = GlobalId(0);
    let m = IrModule {
        source_id: SourceId::new(0),
        name: ident("C"),
        construct_kind: covenant_parser::ast::ConstructKind::Module,
        construct_privacy: None,
        fields: vec![IrField {
            id: phase_id,
            name: ident("phase"),
            ty: Ty::Unknown,
            privacy: PrivacyDomain::Plaintext,
            initializer_fn: None,
            initializer_const: None,
            span: span(),
            explicit_slot: None,
        }],
        structs: vec![],
        errors: vec![],
        events: vec![],
        choices: vec![],
        functions: vec![],
        external_contracts: vec![],
        metadata: HashMap::new(),
        anchor: None,
        upgradeable: None,
    };
    (m, phase_id)
}

fn empty_action(name: &str) -> IrFunction {
    IrFunction {
        id: FunctionId(0),
        name: ident(name),
        kind: IrFunctionKind::Action,
        params: vec![],
        returns: None,
        guards: vec![],
        qualifiers: vec![],
        annotations: vec![],
        blocks: vec![IrBlock {
            id: BlockId(0),
            params: vec![],
            instructions: vec![],
            terminator: Terminator::Return(None),
            span: span(),
        }],
        entry: BlockId(0),
        values: vec![],
        value_types: HashMap::new(),
        value_privacy: HashMap::new(),
        local_to_value: HashMap::new(),
        value_spans: HashMap::new(),
        span: span(),
    }
}

fn instr(opcode: Opcode, operands: Vec<Value>, result: Option<Value>) -> Instr {
    Instr {
        result,
        opcode,
        operands,
        metadata: InstrMetadata::default(),
        span: span(),
    }
}

/// Build an action that is guarded by `when phase == k` and writes `phase = n`.
fn guarded_phase_write_action(name: &str, phase_id: GlobalId, k: u128, n: u128) -> IrFunction {
    let mut f = empty_action(name);
    let v_sload = Value(0);
    let v_const_k = Value(1);
    let v_eq = Value(2);
    let v_const_n = Value(3);

    f.values
        .push((v_const_k, ValueInfo::Const(IrConstant::Integer(k))));
    f.values
        .push((v_const_n, ValueInfo::Const(IrConstant::Integer(n))));

    // SLoad(phase_id) -> v_sload
    f.blocks[0]
        .instructions
        .push(instr(Opcode::SLoad(phase_id), vec![], Some(v_sload)));
    // Eq(v_sload, v_const_k) -> v_eq
    f.blocks[0]
        .instructions
        .push(instr(Opcode::Eq, vec![v_sload, v_const_k], Some(v_eq)));
    // SStore(phase_id, v_const_n)
    f.blocks[0]
        .instructions
        .push(instr(Opcode::SStore(phase_id), vec![v_const_n], None));

    f.guards.push(IrGuard::When(v_eq));
    f
}

#[test]
fn c801_fires_on_backward_transition_audit_example() {
    // `action finalize() when phase == 1 { phase = 0 }`: the exact audit example.
    let (mut m, phase_id) = module_with_phase();
    m.functions
        .push(guarded_phase_write_action("finalize", phase_id, 1, 0));

    let findings = C801CeremonyPhaseBackwardTransition.analyze(&m, "");
    assert!(
        !findings.is_empty(),
        "C801 must flag phase=1 → phase=0 backward transition (KSR-CVN-025)"
    );
    assert_eq!(findings[0].detector_code, "C801");
}

#[test]
fn c801_fires_on_stationary_write() {
    // `when phase == 2 { phase = 2 }`: no forward progress.
    let (mut m, phase_id) = module_with_phase();
    m.functions
        .push(guarded_phase_write_action("stay", phase_id, 2, 2));

    let findings = C801CeremonyPhaseBackwardTransition.analyze(&m, "");
    assert!(
        !findings.is_empty(),
        "C801 must flag stationary phase write"
    );
}

#[test]
fn c801_silent_on_forward_transition() {
    // `when phase == 0 { phase = 1 }`: monotonic increase.
    let (mut m, phase_id) = module_with_phase();
    m.functions
        .push(guarded_phase_write_action("start", phase_id, 0, 1));

    let findings = C801CeremonyPhaseBackwardTransition.analyze(&m, "");
    assert!(
        findings.is_empty(),
        "C801 must not fire on forward transition: {findings:?}"
    );
}

#[test]
fn c801_silent_when_no_phase_field_exists() {
    // Module without a phase field should be ignored entirely.
    let mut m = IrModule {
        source_id: SourceId::new(0),
        name: ident("C"),
        construct_kind: covenant_parser::ast::ConstructKind::Module,
        construct_privacy: None,
        fields: vec![], // no phase
        structs: vec![],
        errors: vec![],
        events: vec![],
        choices: vec![],
        functions: vec![],
        external_contracts: vec![],
        metadata: HashMap::new(),
        anchor: None,
        upgradeable: None,
    };
    m.functions.push(empty_action("noop"));

    let findings = C801CeremonyPhaseBackwardTransition.analyze(&m, "");
    assert!(findings.is_empty());
}

#[test]
fn c801_silent_when_action_has_no_phase_guard() {
    // An action with no `when phase == K` guard is not checked.
    let (mut m, phase_id) = module_with_phase();
    let mut f = empty_action("write_phase");
    let v_const = Value(0);
    f.values
        .push((v_const, ValueInfo::Const(IrConstant::Integer(0))));
    f.blocks[0]
        .instructions
        .push(instr(Opcode::SStore(phase_id), vec![v_const], None));
    m.functions.push(f);

    let findings = C801CeremonyPhaseBackwardTransition.analyze(&m, "");
    assert!(
        findings.is_empty(),
        "C801 must not fire without phase guard: {findings:?}"
    );
}

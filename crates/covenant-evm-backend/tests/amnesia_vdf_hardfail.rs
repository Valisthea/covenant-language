//! KSR-CVN-022 / KSR-CVN-023 regression tests.
//!
//! Before Session 2 v0.6.1-rc2:
//!   - `ShamirSplit`, `ShamirReconstruct`, `VdfLock`, `VdfUnlock`, and the
//!     priority-queue stubs produced a diagnostic but no bytecode, a contract
//!     using Amnesia reconstruct compiled "successfully" into a silent no-op.
//!   - `@vdf_locked(delay)` parsed into `IrActionQualifier::VdfLocked` but
//!     was never referenced by codegen, so supposedly-delayed actions ran
//!     immediately.
//!
//! Both are now hard compile errors (E516 / E517). For the opcode case we
//! additionally emit a REVERT stub at the instruction site, so if a caller
//! ignores the diagnostic and deploys the artifact anyway, the runtime traps
//! instead of silently succeeding.

use std::collections::HashMap;

use covenant_diag::{DiagnosticLevel, SourceId, Span};
use covenant_evm_backend::{codegen_evm, EvmConfig};
use covenant_ir::{
    block::IrBlock,
    function::{IrActionQualifier, IrFunction, IrFunctionKind},
    id::{BlockId, FunctionId, Value},
    instr::{Instr, InstrMetadata, Terminator},
    module::IrModule,
    Opcode,
};

const E516: u32 = 516;
const E517: u32 = 517;

fn empty_module() -> IrModule {
    IrModule {
        source_id: SourceId::new(0),
        name: covenant_parser::ast::Ident {
            name: "M".into(),
            span: Span::new(SourceId::new(0), 0, 1),
        },
        construct_kind: covenant_parser::ast::ConstructKind::Module,
        construct_privacy: None,
        fields: vec![],
        structs: vec![],
        errors: vec![],
        events: vec![],
        choices: vec![],
        functions: vec![],
        external_contracts: vec![],
        metadata: HashMap::new(),
        anchor: None,
        upgradeable: None,
    }
}

fn empty_function(name: &str) -> IrFunction {
    let span = Span::new(SourceId::new(0), 0, 0);
    let entry = BlockId(0);
    IrFunction {
        id: FunctionId(0),
        name: covenant_parser::ast::Ident {
            name: name.into(),
            span,
        },
        kind: IrFunctionKind::Action,
        params: vec![],
        returns: None,
        guards: vec![],
        qualifiers: vec![],
        annotations: vec![],
        blocks: vec![IrBlock {
            id: entry,
            params: vec![],
            instructions: vec![],
            terminator: Terminator::Return(None),
            span,
        }],
        entry,
        values: vec![],
        value_types: HashMap::new(),
        value_privacy: HashMap::new(),
        local_to_value: HashMap::new(),
        value_spans: HashMap::new(),
        span,
    }
}

#[test]
fn shamir_split_emits_e516_compile_error() {
    let span = Span::new(SourceId::new(0), 0, 0);
    let mut f = empty_function("splitter");
    f.blocks[0].instructions.push(Instr {
        result: Some(Value(0)),
        opcode: Opcode::ShamirSplit,
        operands: vec![],
        metadata: InstrMetadata::default(),
        span,
    });

    let mut m = empty_module();
    m.functions.push(f);

    let (_artifact, diags) = codegen_evm(m, EvmConfig::default());

    assert!(
        diags
            .iter()
            .any(|d| d.level == DiagnosticLevel::Error && d.code.0 == E516),
        "expected E516 (unlowered amnesia opcode) for ShamirSplit, got {:?}",
        diags
    );
}

#[test]
fn vdf_lock_emits_e516_compile_error() {
    let span = Span::new(SourceId::new(0), 0, 0);
    let mut f = empty_function("locker");
    f.blocks[0].instructions.push(Instr {
        result: Some(Value(0)),
        opcode: Opcode::VdfLock,
        operands: vec![],
        metadata: InstrMetadata::default(),
        span,
    });

    let mut m = empty_module();
    m.functions.push(f);

    let (_artifact, diags) = codegen_evm(m, EvmConfig::default());

    assert!(
        diags
            .iter()
            .any(|d| d.level == DiagnosticLevel::Error && d.code.0 == E516),
        "expected E516 for VdfLock, got {:?}",
        diags
    );
}

#[test]
fn vdf_locked_qualifier_emits_e517_compile_error() {
    let mut f = empty_function("delayed_withdraw");
    f.qualifiers.push(IrActionQualifier::VdfLocked(Value(0)));

    let mut m = empty_module();
    m.functions.push(f);

    let (_artifact, diags) = codegen_evm(m, EvmConfig::default());

    assert!(
        diags
            .iter()
            .any(|d| d.level == DiagnosticLevel::Error && d.code.0 == E517),
        "expected E517 (unlowered @vdf_locked qualifier), got {:?}",
        diags
    );
}

#[test]
fn action_without_vdf_locked_does_not_emit_e517() {
    let f = empty_function("plain_action");
    let mut m = empty_module();
    m.functions.push(f);

    let (_artifact, diags) = codegen_evm(m, EvmConfig::default());

    assert!(
        diags.iter().all(|d| d.code.0 != E517),
        "E517 should only fire for VdfLocked qualifier; got {:?}",
        diags
    );
}

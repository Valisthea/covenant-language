//! Validator and diagnostic-code tests.

use covenant_diag::{DiagCode, Span};
use covenant_ir::{
    block::IrBlock,
    codes,
    function::{IrFunction, IrFunctionKind},
    id::{BlockId, FunctionId, Value},
    instr::{Instr, InstrMetadata, Terminator},
    module::IrModule,
    validate, Opcode,
};

fn empty_module() -> IrModule {
    IrModule {
        source_id: covenant_diag::SourceId::new(0),
        name: covenant_parser::ast::Ident {
            name: "M".into(),
            span: Span::new(covenant_diag::SourceId::new(0), 0, 1),
        },
        construct_kind: covenant_parser::ast::ConstructKind::Record,
        construct_privacy: None,
        fields: vec![],
        structs: vec![],
        errors: vec![],
        events: vec![],
        choices: vec![],
        functions: vec![],
        external_contracts: vec![],
        metadata: std::collections::HashMap::new(),
        anchor: None,
        upgradeable: None,
    }
}

fn empty_function() -> IrFunction {
    let span = Span::new(covenant_diag::SourceId::new(0), 0, 0);
    let entry = BlockId(0);
    IrFunction {
        id: FunctionId(0),
        name: covenant_parser::ast::Ident {
            name: "f".into(),
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
        value_types: std::collections::HashMap::new(),
        value_privacy: std::collections::HashMap::new(),
        local_to_value: std::collections::HashMap::new(),
        value_spans: std::collections::HashMap::new(),
        span,
    }
}

fn has(diags: &[covenant_diag::Diagnostic], c: DiagCode) -> bool {
    diags.iter().any(|d| d.code == c)
}

#[test]
fn validator_accepts_empty_module() {
    let m = empty_module();
    let diags = validate(&m);
    assert!(diags.is_empty());
}

#[test]
fn validator_accepts_well_formed_function() {
    let mut m = empty_module();
    m.functions.push(empty_function());
    let diags = validate(&m);
    assert!(diags.is_empty(), "{diags:?}");
}

#[test]
fn validator_rejects_unknown_jump_target() {
    let mut m = empty_module();
    let mut f = empty_function();
    let span = Span::new(covenant_diag::SourceId::new(0), 0, 0);
    // Replace entry terminator with Jump to a non-existent block.
    f.blocks[0].terminator = Terminator::Jump {
        target: BlockId(999),
        args: vec![],
    };
    f.blocks[0].span = span;
    m.functions.push(f);
    let diags = validate(&m);
    assert!(has(&diags, codes::E405_BLOCK_ARG_MISMATCH));
}

#[test]
fn validator_rejects_arg_count_mismatch() {
    let mut m = empty_module();
    let mut f = empty_function();
    // Add a second block that takes one parameter.
    let second_id = BlockId(1);
    let param_val = Value(0);
    let span = Span::new(covenant_diag::SourceId::new(0), 0, 0);
    f.blocks.push(IrBlock {
        id: second_id,
        params: vec![param_val],
        instructions: vec![],
        terminator: Terminator::Return(None),
        span,
    });
    // Entry jumps to second with no args.
    f.blocks[0].terminator = Terminator::Jump {
        target: second_id,
        args: vec![],
    };
    m.functions.push(f);
    let diags = validate(&m);
    assert!(has(&diags, codes::E405_BLOCK_ARG_MISMATCH));
}

#[test]
fn validator_rejects_opcode_arity_mismatch() {
    let mut m = empty_module();
    let mut f = empty_function();
    let span = Span::new(covenant_diag::SourceId::new(0), 0, 0);
    // `Keccak` has `range(0, None)` so accepts any number, use a strict one.
    // `AddChecked` expects exactly 2 operands; supply 0.
    f.blocks[0].instructions.push(Instr {
        result: None,
        opcode: Opcode::AddChecked,
        operands: vec![],
        metadata: InstrMetadata::default(),
        span,
    });
    m.functions.push(f);
    let diags = validate(&m);
    assert!(has(&diags, codes::E410_OPCODE_ARITY));
}

#[test]
fn validator_rejects_fhe_branch_missing_merge() {
    let mut m = empty_module();
    let mut f = empty_function();
    let span = Span::new(covenant_diag::SourceId::new(0), 0, 0);
    let other = BlockId(1);
    f.blocks.push(IrBlock {
        id: other,
        params: vec![],
        instructions: vec![],
        terminator: Terminator::Return(None),
        span,
    });
    f.blocks[0].terminator = Terminator::FheBranch {
        cond: Value(0),
        then_target: other,
        then_args: vec![],
        else_target: other,
        else_args: vec![],
        merge_target: BlockId(999),
    };
    m.functions.push(f);
    let diags = validate(&m);
    assert!(has(&diags, codes::E420_FHEBRANCH_NO_MERGE));
}

#[test]
fn lambda_in_unsupported_context_emits_diag() {
    // A lambda outside a method-call argument position.
    let src = r#"record R { view v returns amount { (x => x + 1) } }"#;
    let (toks, _) = covenant_lexer::tokenize(src, covenant_diag::SourceId::new(0));
    let (file, _) = covenant_parser::parse(&toks, covenant_diag::SourceId::new(0));
    let (res, _) = covenant_resolver::resolve(file.unwrap(), covenant_diag::SourceId::new(0));
    let (typed, _) = covenant_types::typecheck(res, covenant_diag::SourceId::new(0));
    let (checked, _) = covenant_privacy::analyze_privacy(typed, covenant_diag::SourceId::new(0));
    let (_, diags) = covenant_ir::build_ir(checked, covenant_diag::SourceId::new(0));
    assert!(has(&diags, codes::E404_LAMBDA_UNSUPPORTED));
}

#[test]
fn selective_disclosure_defers_to_v02() {
    // Use `selective_disclosure` in a registry construct; it should emit E416.
    let src = r#"
registry Reg {
    selective_disclosure proof_of_age(age: amount) using AgeCircuit { age > 18 }
}
"#;
    let (toks, _) = covenant_lexer::tokenize(src, covenant_diag::SourceId::new(0));
    let (file, _) = covenant_parser::parse(&toks, covenant_diag::SourceId::new(0));
    let file = file.expect("parse");
    let (res, _) = covenant_resolver::resolve(file, covenant_diag::SourceId::new(0));
    let (typed, _) = covenant_types::typecheck(res, covenant_diag::SourceId::new(0));
    let (checked, _) = covenant_privacy::analyze_privacy(typed, covenant_diag::SourceId::new(0));
    let (_, diags) = covenant_ir::build_ir(checked, covenant_diag::SourceId::new(0));
    assert!(has(&diags, codes::E416_SELECTIVE_DISCLOSURE_DEFERRED));
}

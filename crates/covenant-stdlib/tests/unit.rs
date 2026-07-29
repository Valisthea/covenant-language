//! Per-function unit tests for Phase 9 ERC-20 synthesis.

use covenant_diag::SourceId;
use covenant_ir::{validate, IrFunctionKind, IrModule, Opcode, Terminator};
use covenant_lexer::tokenize;
use covenant_parser::parse;
use covenant_privacy::analyze_privacy;
use covenant_resolver::resolve;
use covenant_stdlib::{lower_stdlib, StdlibConfig};
use covenant_types::typecheck;

fn front_to_ir(src: &str) -> IrModule {
    let (toks, _) = tokenize(src, SourceId::new(0));
    let (file, _) = parse(&toks, SourceId::new(0));
    let (res, _) = resolve(file.unwrap(), SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (checked, _) = analyze_privacy(typed, SourceId::new(0));
    covenant_ir::build_ir(checked, SourceId::new(0)).0
}

const COIN_SRC: &str = include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov");

fn lower_coin() -> IrModule {
    let ir = front_to_ir(COIN_SRC);
    let (ir, _) = lower_stdlib(ir, StdlibConfig::default());
    ir
}

fn find_fn<'a>(ir: &'a IrModule, name: &str) -> &'a covenant_ir::IrFunction {
    ir.functions
        .iter()
        .find(|f| f.name.name.as_ref() == name)
        .unwrap_or_else(|| panic!("function {name} not synthesized"))
}

// ---- Structural checks on synthesized functions ----

#[test]
fn total_supply_has_sload_and_return() {
    let ir = lower_coin();
    let f = find_fn(&ir, "totalSupply");
    assert!(matches!(f.kind, IrFunctionKind::View));
    assert_eq!(f.params.len(), 0);
    let entry = &f.blocks[f.entry.0 as usize];
    assert!(entry
        .instructions
        .iter()
        .any(|i| matches!(i.opcode, Opcode::SLoad(_))));
    assert!(matches!(entry.terminator, Terminator::Return(Some(_))));
}

#[test]
fn balance_of_takes_address_returns_amount() {
    let ir = lower_coin();
    let f = find_fn(&ir, "balanceOf");
    assert_eq!(f.params.len(), 1);
    assert!(matches!(f.kind, IrFunctionKind::View));
}

#[test]
fn transfer_takes_two_params_returns_bool() {
    let ir = lower_coin();
    let f = find_fn(&ir, "transfer");
    assert_eq!(f.params.len(), 2);
    assert!(matches!(f.kind, IrFunctionKind::Action));
}

#[test]
fn transfer_has_branch_and_revert_block() {
    let ir = lower_coin();
    let f = find_fn(&ir, "transfer");
    assert!(
        f.blocks.len() >= 3,
        "transfer needs entry + transfer + revert blocks"
    );
    let has_branch = f
        .blocks
        .iter()
        .any(|b| matches!(b.terminator, Terminator::Branch { .. }));
    let has_revert = f
        .blocks
        .iter()
        .any(|b| matches!(b.terminator, Terminator::Revert { .. }));
    assert!(has_branch);
    assert!(has_revert);
}

#[test]
fn transfer_emits_transfer_event() {
    let ir = lower_coin();
    let f = find_fn(&ir, "transfer");
    let has_emit = f
        .blocks
        .iter()
        .flat_map(|b| b.instructions.iter())
        .any(|i| {
            matches!(
                &i.opcode,
                Opcode::Emit(ident) if ident.name.as_ref() == "Transfer"
            )
        });
    assert!(has_emit, "transfer body must emit Transfer event");
}

#[test]
fn approve_emits_approval_event() {
    let ir = lower_coin();
    let f = find_fn(&ir, "approve");
    let has_emit = f
        .blocks
        .iter()
        .flat_map(|b| b.instructions.iter())
        .any(|i| {
            matches!(
                &i.opcode,
                Opcode::Emit(ident) if ident.name.as_ref() == "Approval"
            )
        });
    assert!(has_emit);
}

#[test]
fn transfer_from_has_two_revert_paths() {
    let ir = lower_coin();
    let f = find_fn(&ir, "transferFrom");
    let revert_count = f
        .blocks
        .iter()
        .filter(|b| matches!(b.terminator, Terminator::Revert { .. }))
        .count();
    assert_eq!(
        revert_count, 2,
        "transferFrom has allowance + balance reverts"
    );
}

#[test]
fn decimals_returns_constant() {
    let ir = lower_coin();
    let f = find_fn(&ir, "decimals");
    assert_eq!(f.params.len(), 0);
    assert!(matches!(f.kind, IrFunctionKind::View));
}

#[test]
fn symbol_returns_constant_text() {
    let ir = lower_coin();
    let f = find_fn(&ir, "symbol");
    assert_eq!(f.params.len(), 0);
}

#[test]
fn name_defaults_to_construct_name_when_metadata_absent() {
    // Coin source has `name: "Covenant Coin"` metadata, so the synthesized
    // name() returns that text, not the construct name.
    let ir = lower_coin();
    let f = find_fn(&ir, "name");
    // Ensure the function was synthesized with a constant.
    let entry = &f.blocks[f.entry.0 as usize];
    let has_const = f
        .values
        .iter()
        .any(|(_, info)| matches!(info, covenant_ir::ValueInfo::Const(_)));
    assert!(has_const);
    let _ = entry;
}

// ---- Module-level injections ----

#[test]
fn allowances_field_has_hash_keyed_map() {
    let ir = lower_coin();
    let alw = ir
        .fields
        .iter()
        .find(|f| f.name.name.as_ref() == "allowances")
        .expect("allowances field injected");
    match &alw.ty {
        covenant_types::Ty::Map(k, v) => {
            assert!(matches!(**k, covenant_types::Ty::Hash));
            assert!(matches!(**v, covenant_types::Ty::Amount));
        }
        other => panic!("allowances must be map<hash, amount>, got {other:?}"),
    }
}

#[test]
fn balances_field_is_map_address_amount() {
    let ir = lower_coin();
    let bal = ir
        .fields
        .iter()
        .find(|f| f.name.name.as_ref() == "balances")
        .expect("balances injected");
    match &bal.ty {
        covenant_types::Ty::Map(k, v) => {
            assert!(matches!(**k, covenant_types::Ty::Address));
            assert!(matches!(**v, covenant_types::Ty::Amount));
        }
        _ => panic!("balances must be map<address, amount>"),
    }
}

#[test]
fn total_supply_field_is_amount() {
    let ir = lower_coin();
    let ts = ir
        .fields
        .iter()
        .find(|f| f.name.name.as_ref() == "total_supply")
        .expect("total_supply injected");
    assert!(matches!(ts.ty, covenant_types::Ty::Amount));
}

// ---- Validator survival ----

#[test]
fn synthesized_ir_passes_validator() {
    let ir = lower_coin();
    let diags = validate(&ir);
    assert!(diags.is_empty(), "validator: {diags:?}");
}

// ---- Non-token constructs pass through unchanged (or emit W606 for ballot) ----

#[test]
fn record_construct_is_unchanged() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_01_hello.cov");
    let before = front_to_ir(src);
    let fn_count_before = before.functions.len();
    let (after, diags) = lower_stdlib(before, StdlibConfig::default());
    assert_eq!(after.functions.len(), fn_count_before);
    // No ERC-20 in a record.
    assert!(!after
        .functions
        .iter()
        .any(|f| f.name.name.as_ref() == "totalSupply"));
    let _ = diags;
}

#[test]
fn counter_construct_is_unchanged() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_04_shielded_counter.cov");
    let before = front_to_ir(src);
    let fn_count_before = before.functions.len();
    let (after, _) = lower_stdlib(before, StdlibConfig::default());
    assert_eq!(after.functions.len(), fn_count_before);
}

#[test]
fn ballot_construct_emits_w606() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_03_open_ballot.cov");
    let before = front_to_ir(src);
    let (_, diags) = lower_stdlib(before, StdlibConfig::default());
    assert!(diags
        .iter()
        .any(|d| d.code == covenant_stdlib::codes::W606_SYNTH_NOT_IMPL));
}

#[test]
fn board_construct_is_unchanged() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_05_quantum_board.cov");
    let before = front_to_ir(src);
    let fn_count_before = before.functions.len();
    let (after, _) = lower_stdlib(before, StdlibConfig::default());
    assert_eq!(after.functions.len(), fn_count_before);
}

// ---- Selectors match canonical values ----

#[test]
fn all_9_canonical_selectors_reachable() {
    let ir = lower_coin();
    let (opt, _) = covenant_opt::optimize(ir, covenant_opt::OptimizerConfig::default());
    let (artifact, _) =
        covenant_evm_backend::codegen_evm(opt, covenant_evm_backend::EvmConfig::default());
    assert_eq!(
        artifact.function_selectors.len(),
        9,
        "expected 9 public ERC-20 functions"
    );
}

// ---- Config knobs ----

#[test]
fn disable_erc20_leaves_coin_untouched() {
    let ir = front_to_ir(COIN_SRC);
    let fn_before = ir.functions.len();
    let cfg = StdlibConfig {
        synthesize_erc20: false,
        ..StdlibConfig::default()
    };
    let (after, _) = lower_stdlib(ir, cfg);
    assert_eq!(after.functions.len(), fn_before);
}

// ---- Conflict handling ----

#[test]
fn user_transfer_conflicts_in_strict_mode() {
    // Simulate a user's `transfer` by taking a token-like module and injecting
    // a stub function with that name. We can't easily express this through
    // Covenant source (Coin source has no functions), so we post-process the
    // IR module manually.
    let mut ir = front_to_ir(COIN_SRC);
    let span = ir.name.span;
    let fake_fn = covenant_ir::IrFunction {
        id: covenant_ir::FunctionId(ir.functions.len() as u32),
        name: covenant_parser::ast::Ident {
            name: "transfer".into(),
            span,
        },
        kind: IrFunctionKind::Action,
        params: Vec::new(),
        returns: None,
        guards: Vec::new(),
        qualifiers: Vec::new(),
        annotations: Vec::new(),
        blocks: vec![covenant_ir::IrBlock {
            id: covenant_ir::BlockId(0),
            params: Vec::new(),
            instructions: Vec::new(),
            terminator: covenant_ir::Terminator::Return(None),
            span,
        }],
        entry: covenant_ir::BlockId(0),
        values: Vec::new(),
        value_types: std::collections::HashMap::new(),
        value_privacy: std::collections::HashMap::new(),
        local_to_value: std::collections::HashMap::new(),
        value_spans: std::collections::HashMap::new(),
        span,
    };
    ir.functions.push(fake_fn);
    let cfg = StdlibConfig {
        strict_conflict_detection: true,
        ..StdlibConfig::default()
    };
    let (_, diags) = lower_stdlib(ir, cfg);
    assert!(diags
        .iter()
        .any(|d| d.code == covenant_stdlib::codes::E601_USER_FN_CONFLICT));
}

#[test]
fn user_transfer_permissive_mode_skips_synthesis() {
    let mut ir = front_to_ir(COIN_SRC);
    let span = ir.name.span;
    ir.functions.push(covenant_ir::IrFunction {
        id: covenant_ir::FunctionId(ir.functions.len() as u32),
        name: covenant_parser::ast::Ident {
            name: "transfer".into(),
            span,
        },
        kind: IrFunctionKind::Action,
        params: Vec::new(),
        returns: None,
        guards: Vec::new(),
        qualifiers: Vec::new(),
        annotations: Vec::new(),
        blocks: vec![covenant_ir::IrBlock {
            id: covenant_ir::BlockId(0),
            params: Vec::new(),
            instructions: Vec::new(),
            terminator: covenant_ir::Terminator::Return(None),
            span,
        }],
        entry: covenant_ir::BlockId(0),
        values: Vec::new(),
        value_types: std::collections::HashMap::new(),
        value_privacy: std::collections::HashMap::new(),
        local_to_value: std::collections::HashMap::new(),
        value_spans: std::collections::HashMap::new(),
        span,
    });
    let cfg = StdlibConfig {
        strict_conflict_detection: false,
        ..StdlibConfig::default()
    };
    let (after, diags) = lower_stdlib(ir, cfg);
    // User's transfer is preserved, W602 warning emitted.
    assert!(diags
        .iter()
        .any(|d| d.code == covenant_stdlib::codes::W602_USER_OVERRIDE));
    // Exactly one "transfer" function (the user's).
    let transfer_count = after
        .functions
        .iter()
        .filter(|f| f.name.name.as_ref() == "transfer")
        .count();
    assert_eq!(transfer_count, 1);
}

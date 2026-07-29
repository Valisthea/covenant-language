//! Regression coverage for the V0.9.6 adversarial review, stdlib-synthesis lane.
//!
//! Findings covered: F-14 (field type reused by name only), F-16 (unauthenticated
//! ERC-721 mint), F-20 (genesis mint to a non-deployer principal is discarded),
//! F-27 (mint accepts the zero receiver), F-28 (field default beats the genesis
//! amount), F-34 (approve refuses an authorized operator), F-35 (a shadowing
//! error or event produces an ABI the runtime does not honour), F-40 (E601 names
//! the wrong standard and an unreachable fix), F-41 (an unrepresentable
//! `decimals`).
//!
//! Everything here is asserted on the module the synthesizer produces, which is
//! where each defect lives: the reported on-chain behaviour is the faithful
//! lowering of exactly these IR shapes.

use covenant_diag::{DiagnosticLevel, SourceId};
use covenant_ir::{
    id::{GlobalId, Value},
    instr::{IrConstant, ValueInfo},
    IrFunction, IrModule, Opcode, Terminator,
};
use covenant_lexer::tokenize;
use covenant_parser::parse;
use covenant_privacy::analyze_privacy;
use covenant_resolver::resolve;
use covenant_stdlib::{codes, lower_stdlib, StdlibConfig};
use covenant_types::typecheck;

fn front_to_ir(src: &str) -> IrModule {
    let (toks, _) = tokenize(src, SourceId::new(0));
    let (file, _) = parse(&toks, SourceId::new(0));
    let (res, _) = resolve(file.unwrap(), SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (checked, _) = analyze_privacy(typed, SourceId::new(0));
    covenant_ir::build_ir(checked, SourceId::new(0)).0
}

fn lower(src: &str) -> (IrModule, Vec<covenant_diag::Diagnostic>) {
    lower_stdlib(front_to_ir(src), StdlibConfig::default())
}

fn has_code(diags: &[covenant_diag::Diagnostic], code: covenant_diag::DiagCode) -> bool {
    diags
        .iter()
        .any(|d| d.code == code && d.level == DiagnosticLevel::Error)
}

fn find_fn<'a>(ir: &'a IrModule, name: &str) -> &'a IrFunction {
    ir.functions
        .iter()
        .find(|f| f.name.name.as_ref() == name)
        .unwrap_or_else(|| panic!("function `{name}` not synthesized"))
}

fn field_id(ir: &IrModule, name: &str) -> GlobalId {
    ir.fields
        .iter()
        .find(|f| f.name.name.as_ref() == name)
        .unwrap_or_else(|| panic!("field `{name}` not present"))
        .id
}

fn has_opcode(f: &IrFunction, want: &Opcode) -> bool {
    f.blocks
        .iter()
        .flat_map(|b| b.instructions.iter())
        .any(|i| &i.opcode == want)
}

/// Condition of the branch whose ELSE arm reverts with `error`.
///
/// Asserting on this rather than on "some block reverts with X" is what makes
/// these tests non-vacuous: a guard that is emitted but not branched on, or
/// branched on with the wrong predicate, does not satisfy it.
fn cond_guarding_revert(f: &IrFunction, error: &str) -> Option<Value> {
    f.blocks.iter().find_map(|b| match &b.terminator {
        Terminator::Branch {
            cond, else_target, ..
        } => {
            let target = f.blocks.get(else_target.0 as usize)?;
            match &target.terminator {
                Terminator::Revert { error: e, .. } if e.name.as_ref() == error => Some(*cond),
                _ => None,
            }
        }
        _ => None,
    })
}

/// The instruction that produced `v`, when `v` is an instruction result.
fn producer(f: &IrFunction, v: Value) -> Option<&covenant_ir::instr::Instr> {
    let info = f.values.iter().find(|(val, _)| *val == v).map(|(_, i)| i)?;
    match info {
        ValueInfo::InstrResult { instr_idx, block } => f
            .blocks
            .get(block.0 as usize)?
            .instructions
            .get(*instr_idx as usize),
        _ => None,
    }
}

/// Every value in `f` produced by an instruction with this opcode.
fn results_of(f: &IrFunction, want: &Opcode) -> Vec<Value> {
    f.blocks
        .iter()
        .flat_map(|b| b.instructions.iter())
        .filter(|i| &i.opcode == want)
        .filter_map(|i| i.result)
        .collect()
}

/// Every zero-address constant value in `f`.
fn zero_address_consts(f: &IrFunction) -> Vec<Value> {
    f.values
        .iter()
        .filter(|(_, info)| matches!(info, ValueInfo::Const(IrConstant::ZeroAddress)))
        .map(|(v, _)| *v)
        .collect()
}

fn param_value(f: &IrFunction, name: &str) -> Value {
    f.params
        .iter()
        .find(|p| p.name.name.as_ref() == name)
        .unwrap_or_else(|| panic!("param `{name}` not found"))
        .value
}

const NFT_SRC: &str = r#"
nft ProbeNFT {
    name: "Probe"
    symbol: "PRB"
    base_uri: "ipfs://p/"
}
"#;

// ---------------------------------------------------------------------------
// F-16: the synthesized ERC-721 mint must be gated on the deployer.
// ---------------------------------------------------------------------------

#[test]
fn f16_synthesized_mint_is_gated_on_the_deployer() {
    let (ir, diags) = lower(NFT_SRC);
    assert!(diags.iter().all(|d| d.level != DiagnosticLevel::Error));
    let mint = find_fn(&ir, "mint");

    let cond = cond_guarding_revert(mint, "NotDeployer")
        .expect("mint must branch to a NotDeployer revert");
    let gate = producer(mint, cond).expect("the gate must be an instruction result");
    assert_eq!(
        gate.opcode,
        Opcode::Eq,
        "the mint gate must be an equality, got {:?}",
        gate.opcode
    );

    let deployer_vals = results_of(mint, &Opcode::LoadDeployer);
    let caller_vals = results_of(mint, &Opcode::LoadCaller);
    assert!(
        gate.operands.iter().any(|o| deployer_vals.contains(o)),
        "the gate must compare against the constructor-captured deployer"
    );
    assert!(
        gate.operands.iter().any(|o| caller_vals.contains(o)),
        "the gate must compare the caller"
    );
}

#[test]
fn f16_not_deployer_error_is_declared_so_the_revert_carries_a_selector() {
    // An undeclared error name makes the backend fall back to an empty REVERT,
    // which still fails closed but tells an integrator nothing.
    let (ir, _) = lower(NFT_SRC);
    assert!(ir
        .errors
        .iter()
        .any(|e| e.name.name.as_ref() == "NotDeployer"));
}

// ---------------------------------------------------------------------------
// F-27: the synthesized ERC-721 mint must refuse the zero receiver.
// ---------------------------------------------------------------------------

#[test]
fn f27_synthesized_mint_refuses_the_zero_receiver() {
    let (ir, _) = lower(NFT_SRC);
    let mint = find_fn(&ir, "mint");

    let cond = cond_guarding_revert(mint, "InvalidReceiver")
        .expect("mint must branch to an InvalidReceiver revert");
    let gate = producer(mint, cond).expect("the receiver check must be an instruction result");
    assert_eq!(
        gate.opcode,
        Opcode::Ne,
        "the receiver check must be `to != 0`, got {:?}",
        gate.opcode
    );

    let to = param_value(mint, "to");
    let zeros = zero_address_consts(mint);
    assert!(
        gate.operands.contains(&to),
        "the receiver check must test the `to` parameter"
    );
    assert!(
        gate.operands.iter().any(|o| zeros.contains(o)),
        "the receiver check must test against address(0)"
    );
}

// ---------------------------------------------------------------------------
// F-34: approve must admit an authorized operator, not only the literal owner.
// ---------------------------------------------------------------------------

#[test]
fn f34_approve_consults_the_operator_approvals_map() {
    let (ir, _) = lower(NFT_SRC);
    let operator_approvals = field_id(&ir, "operator_approvals");
    let approve = find_fn(&ir, "approve");

    assert!(
        has_opcode(approve, &Opcode::SLoad(operator_approvals)),
        "approve must read operator_approvals so an authorized operator passes"
    );

    let cond = cond_guarding_revert(approve, "NotTokenOwner")
        .expect("approve must still refuse an unauthorized caller");
    let gate = producer(approve, cond).expect("the gate must be an instruction result");
    assert_eq!(
        gate.opcode,
        Opcode::LogicalOr,
        "approve's gate must be owner OR operator, not a bare equality, got {:?}",
        gate.opcode
    );

    // The right-hand disjunct has to be the operator lookup, otherwise the
    // SLoad above is dead and the operator still cannot approve.
    let operator_reads: Vec<Value> = approve
        .blocks
        .iter()
        .flat_map(|b| b.instructions.iter())
        .filter(|i| i.opcode == Opcode::MapGet)
        .filter(|i| {
            i.operands.first().is_some_and(|base| {
                producer(approve, *base)
                    .is_some_and(|p| p.opcode == Opcode::SLoad(operator_approvals))
            })
        })
        .filter_map(|i| i.result)
        .collect();
    assert!(
        gate.operands.iter().any(|o| operator_reads.contains(o)),
        "the OR must actually read operator_approvals[keccak(owner, caller)]"
    );
}

// ---------------------------------------------------------------------------
// F-14: a field reused by name must also match the type the surface needs.
// ---------------------------------------------------------------------------

const SCALAR_BALANCES: &str = r#"
token ScalarBal {
    symbol: "SB"
    name: "Scalar Bal"
    decimals: 18
    supply: 1000 to deployer
    field total_supply: amount
    field balances: amount
}
"#;

const WRONG_KEY_BALANCES: &str = r#"
token WrongKey {
    symbol: "WK"
    name: "Wrong Key"
    decimals: 18
    supply: 1000 to deployer
    field total_supply: amount
    field balances: map<amount, amount>
}
"#;

#[test]
fn f14_scalar_balances_is_refused_not_reused() {
    let (ir, diags) = lower(SCALAR_BALANCES);
    assert!(
        has_code(&diags, codes::E609_FIELD_TYPE_CONFLICT),
        "a scalar `balances` must raise E609, got {diags:?}"
    );
    assert!(
        !ir.functions
            .iter()
            .any(|f| f.name.name.as_ref() == "transfer"),
        "synthesis must stop rather than address the slot with the wrong shape"
    );
}

#[test]
fn f14_wrong_key_map_balances_is_refused() {
    // The escalated reproduction: map<amount, amount> shares one keccak base
    // with the real ledger, so an integer key equal to an address writes that
    // address's token balance.
    let (_, diags) = lower(WRONG_KEY_BALANCES);
    assert!(has_code(&diags, codes::E609_FIELD_TYPE_CONFLICT));
}

#[test]
fn f14_correctly_typed_declared_fields_still_synthesize() {
    // Negative side: declaring the fields with the shapes the surface needs is
    // the documented pattern (examples/kairos_coin.cov) and must keep working.
    let src = r#"
token GoodFields {
    symbol: "GF"
    name: "Good Fields"
    decimals: 18
    supply: 1000 to deployer
    field total_supply: amount
    field balances: map<address, amount>
}
"#;
    let (ir, diags) = lower(src);
    assert!(!has_code(&diags, codes::E609_FIELD_TYPE_CONFLICT));
    assert!(ir
        .functions
        .iter()
        .any(|f| f.name.name.as_ref() == "transfer"));
}

// ---------------------------------------------------------------------------
// F-20: a genesis mint to any principal but `deployer` must be refused.
// ---------------------------------------------------------------------------

#[test]
fn f20_non_deployer_genesis_principals_are_refused() {
    for principal in [
        "owner",
        "admin",
        "caller",
        "holders",
        "parties",
        "guardians",
    ] {
        let src = format!(
            r#"
token Supply{principal} {{
    symbol: "S"
    name: "Supply"
    decimals: 18
    supply: 1_000_000 to {principal}
}}
"#
        );
        let (_, diags) = lower(&src);
        assert!(
            has_code(&diags, codes::E640_GENESIS_PRINCIPAL_UNSUPPORTED),
            "`to {principal}` must raise E640, got {diags:?}"
        );
    }
}

#[test]
fn f20_deployer_genesis_still_seeds_total_supply() {
    let src = r#"
token Deploy1 {
    symbol: "D1"
    name: "Deploy One"
    decimals: 18
    supply: 1_000_000 to deployer
}
"#;
    let (ir, diags) = lower(src);
    assert!(!has_code(&diags, codes::E640_GENESIS_PRINCIPAL_UNSUPPORTED));
    let ts = ir
        .fields
        .iter()
        .find(|f| f.name.name.as_ref() == "total_supply")
        .expect("total_supply injected");
    assert!(
        matches!(
            ts.initializer_const,
            Some(covenant_ir::instr::IrConstant::Integer(1_000_000))
        ),
        "the deployer path must still record the genesis amount"
    );
}

// ---------------------------------------------------------------------------
// F-28: a `total_supply` default that contradicts the genesis amount.
// ---------------------------------------------------------------------------

#[test]
fn f28_contradicting_total_supply_default_is_refused() {
    let src = r#"
token SplitCoin {
    symbol: "SPL"
    name: "Split Coin"
    decimals: 18
    supply: 1000 to deployer
    field total_supply: amount = 1_000_000_000
    field balances: map<address, amount>
}
"#;
    let (_, diags) = lower(src);
    assert!(
        has_code(&diags, codes::E641_SUPPLY_INITIALIZER_CONFLICT),
        "totalSupply() = 1e9 beside balanceOf(deployer) = 1000 must not compile, got {diags:?}"
    );
}

#[test]
fn f28_agreeing_total_supply_default_is_accepted() {
    let src = r#"
token AgreeCoin {
    symbol: "AGR"
    name: "Agree Coin"
    decimals: 18
    supply: 1000 to deployer
    field total_supply: amount = 1000
    field balances: map<address, amount>
}
"#;
    let (_, diags) = lower(src);
    assert!(
        !has_code(&diags, codes::E641_SUPPLY_INITIALIZER_CONFLICT),
        "the same number written twice is not a contradiction, got {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// F-35: a shadowing error or event whose shape the synthesized bodies ignore.
// ---------------------------------------------------------------------------

#[test]
fn f35_user_error_with_a_different_arity_is_refused() {
    // The ABI declared two uint256 inputs while the runtime reverted with a
    // zero-byte payload under the very same selector.
    let src = r#"
token EvErr {
    symbol: "EE"
    name: "Ev Err"
    decimals: 18
    supply: 1000 to deployer
    error InsufficientBalance(needed: amount, got: amount)
}
"#;
    let (_, diags) = lower(src);
    assert!(
        has_code(&diags, codes::E643_SHADOWED_SYNTH_SHAPE),
        "a two-word InsufficientBalance must not compile, got {diags:?}"
    );
}

#[test]
fn f35_user_event_with_a_different_arity_is_refused() {
    // topic0 moved off the canonical ERC-20 Transfer hash, so every wallet and
    // indexer missed every transfer.
    let src = r#"
token EvArity {
    symbol: "EA"
    name: "Ev Arity"
    decimals: 18
    supply: 1000 to deployer
    event Transfer(sender: address indexed, recipient: address indexed)
}
"#;
    let (_, diags) = lower(src);
    assert!(
        has_code(&diags, codes::E643_SHADOWED_SYNTH_SHAPE),
        "a two-parameter Transfer must not compile, got {diags:?}"
    );
}

#[test]
fn f35_canonical_shape_redeclaration_is_accepted() {
    // examples/kairos_coin.cov declares Transfer by hand, with different
    // parameter names because `from` / `to` are reserved keywords. Names are
    // not part of the shape, so this must keep compiling.
    let src = r#"
token KairosLike {
    symbol: "KL"
    name: "Kairos Like"
    decimals: 18
    supply: 1000 to deployer
    field total_supply: amount
    field balances: map<address, amount>
    event Transfer(sender: address indexed, recipient: address indexed, value: amount)
}
"#;
    let (ir, diags) = lower(src);
    assert!(
        !has_code(&diags, codes::E643_SHADOWED_SYNTH_SHAPE),
        "an exact-shape redeclaration must stay legal, got {diags:?}"
    );
    assert!(ir
        .functions
        .iter()
        .any(|f| f.name.name.as_ref() == "transfer"));
}

// ---------------------------------------------------------------------------
// F-40: E601 must name the standard the construct actually uses, and must not
// point at permissive mode, which no `.cov` file or CLI flag can reach.
// ---------------------------------------------------------------------------

#[test]
fn f40_e601_names_erc721_for_an_nft_construct() {
    // A `mint` declared inline is unreachable through the frontend (the
    // synthesized `owners` field does not exist at name-resolution time), so
    // the collision is injected directly, exactly as the existing conflict
    // tests in unit.rs do.
    let mut ir = front_to_ir(NFT_SRC);
    let span = ir.name.span;
    ir.functions.push(covenant_ir::IrFunction {
        id: covenant_ir::FunctionId(ir.functions.len() as u32),
        name: covenant_parser::ast::Ident {
            name: "mint".into(),
            span,
        },
        kind: covenant_ir::IrFunctionKind::Action,
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

    let (_, diags) = lower_stdlib(ir, StdlibConfig::default());
    let e601 = diags
        .iter()
        .find(|d| d.code == codes::E601_USER_FN_CONFLICT)
        .expect("E601 raised");

    assert!(
        e601.message.contains("ERC-721"),
        "an nft author must not be told about ERC-20: {}",
        e601.message
    );
    assert!(
        !e601.message.contains("ERC-20"),
        "message still names the wrong standard: {}",
        e601.message
    );
    let help = e601.help.clone().unwrap_or_default();
    assert!(
        !help.contains("permissive"),
        "the help must not offer a mode no CLI flag exposes: {help}"
    );
}

// ---------------------------------------------------------------------------
// F-41: `decimals` outside EIP-20's uint8 range.
// ---------------------------------------------------------------------------

#[test]
fn f41_unrepresentable_decimals_is_refused() {
    let src = r#"
token C01 {
    symbol: "C01"
    name: "C01"
    decimals: 340282366920938463463374607431768211455
    supply: 1 to deployer
}
"#;
    let (_, diags) = lower(src);
    assert!(
        has_code(&diags, codes::E642_DECIMALS_UNREPRESENTABLE),
        "a decimals value that cannot round-trip through uint8 must not compile, got {diags:?}"
    );
}

#[test]
fn f41_unusual_but_representable_decimals_stays_a_warning() {
    let src = r#"
token BigDec {
    symbol: "BD"
    name: "Big Dec"
    decimals: 200
    supply: 1 to deployer
}
"#;
    let (ir, diags) = lower(src);
    assert!(!has_code(&diags, codes::E642_DECIMALS_UNREPRESENTABLE));
    assert!(diags
        .iter()
        .any(|d| d.code == codes::W609_DECIMALS_RANGE && d.level == DiagnosticLevel::Warning));
    assert!(ir
        .functions
        .iter()
        .any(|f| f.name.name.as_ref() == "decimals"));
}

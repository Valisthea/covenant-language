//! KSR-CVN-024 — C700 `pq_verify_without_nonce` regression tests.
//!
//! These tests construct an `IrModule` directly because the lint pipeline's
//! source-level driver does not register the synthetic ops we need
//! (PqVerifyDilithium / MapHas / MapSet) in resolver scope without an actual
//! nullifier-typed map declaration.

use std::collections::HashMap;

use covenant_diag::{SourceId, Span};
use covenant_ir::{
    block::IrBlock,
    function::{IrFunction, IrFunctionKind},
    id::{BlockId, FunctionId, Value},
    instr::{Instr, InstrMetadata, Terminator},
    module::IrModule,
    Opcode,
};
use covenant_lint::detectors::pq::C700PqVerifyWithoutNonce;
use covenant_lint::framework::Detector;

fn span() -> Span {
    Span::new(SourceId::new(0), 0, 0)
}

fn module() -> IrModule {
    IrModule {
        source_id: SourceId::new(0),
        name: covenant_parser::ast::Ident {
            name: "M".into(),
            span: span(),
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

fn action(name: &str) -> IrFunction {
    IrFunction {
        id: FunctionId(0),
        name: covenant_parser::ast::Ident {
            name: name.into(),
            span: span(),
        },
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

// Operand convention: PqVerifyDilithium operands = [msg, sig, pk]
//                     MapHas/MapGet operands     = [map_handle, key]
//                     MapSet operands            = [map_handle, key, value]

#[test]
fn c700_fires_when_no_nullifier_check_at_all() {
    let mut m = module();
    let mut f = action("verify_only");
    let msg = Value(0);
    let sig = Value(1);
    let pk = Value(2);
    f.blocks[0]
        .instructions
        .push(instr(Opcode::PqVerifyDilithium, vec![msg, sig, pk], None));
    m.functions.push(f);

    let findings = C700PqVerifyWithoutNonce.analyze(&m, "");
    assert!(
        !findings.is_empty(),
        "expected C700 finding for unguarded verify"
    );
    assert_eq!(findings[0].detector_code, "C700");
}

#[test]
fn c700_fires_when_only_unrelated_mapset_present_audit_bypass() {
    // KSR-CVN-024: the pre-fix detector accepted ANY MapSet anywhere in the
    // function. Here the MapSet's key (`unrelated_key`) does NOT derive from
    // {msg, sig, pk}, so the strengthened C700 must still flag.
    let mut m = module();
    let mut f = action("verify_with_unrelated_set");
    let msg = Value(0);
    let sig = Value(1);
    let pk = Value(2);
    let unrelated_map = Value(3);
    let unrelated_key = Value(4);
    let unrelated_val = Value(5);
    f.blocks[0]
        .instructions
        .push(instr(Opcode::PqVerifyDilithium, vec![msg, sig, pk], None));
    f.blocks[0].instructions.push(instr(
        Opcode::MapSet,
        vec![unrelated_map, unrelated_key, unrelated_val],
        None,
    ));
    m.functions.push(f);

    let findings = C700PqVerifyWithoutNonce.analyze(&m, "");
    assert!(
        !findings.is_empty(),
        "C700 must flag verify with only an unrelated MapSet (KSR-CVN-024 bypass)"
    );
}

#[test]
fn c700_fires_when_mapset_present_but_no_preceding_maphas() {
    let mut m = module();
    let mut f = action("verify_then_set");
    let msg = Value(0);
    let sig = Value(1);
    let pk = Value(2);
    let nullifier_map = Value(3);
    let null_val = Value(4);
    f.blocks[0]
        .instructions
        .push(instr(Opcode::PqVerifyDilithium, vec![msg, sig, pk], None));
    // MapSet uses sig as key (derives from verify) — but no preceding MapHas.
    f.blocks[0].instructions.push(instr(
        Opcode::MapSet,
        vec![nullifier_map, sig, null_val],
        None,
    ));
    m.functions.push(f);

    let findings = C700PqVerifyWithoutNonce.analyze(&m, "");
    assert!(
        !findings.is_empty(),
        "C700 must flag missing preceding MapHas even when MapSet is keyed correctly"
    );
}

#[test]
fn c700_fires_when_maphas_present_but_no_following_mapset() {
    let mut m = module();
    let mut f = action("check_then_verify");
    let msg = Value(0);
    let sig = Value(1);
    let pk = Value(2);
    let nullifier_map = Value(3);
    let _has_result = Value(4);
    f.blocks[0].instructions.push(instr(
        Opcode::MapHas,
        vec![nullifier_map, sig],
        Some(_has_result),
    ));
    f.blocks[0]
        .instructions
        .push(instr(Opcode::PqVerifyDilithium, vec![msg, sig, pk], None));
    m.functions.push(f);

    let findings = C700PqVerifyWithoutNonce.analyze(&m, "");
    assert!(
        !findings.is_empty(),
        "C700 must flag verify with no following MapSet"
    );
}

#[test]
fn c700_silent_when_proper_bracket_with_direct_sig_key() {
    // Canonical safe pattern:
    //   assert(!nullifiers.has(sig))
    //   PqVerifyDilithium(msg, sig, pk)
    //   nullifiers.set(sig, true)
    let mut m = module();
    let mut f = action("safe_verify");
    let msg = Value(0);
    let sig = Value(1);
    let pk = Value(2);
    let nullifier_map = Value(3);
    let has_result = Value(4);
    let true_val = Value(5);
    f.blocks[0].instructions.push(instr(
        Opcode::MapHas,
        vec![nullifier_map, sig],
        Some(has_result),
    ));
    f.blocks[0]
        .instructions
        .push(instr(Opcode::PqVerifyDilithium, vec![msg, sig, pk], None));
    f.blocks[0].instructions.push(instr(
        Opcode::MapSet,
        vec![nullifier_map, sig, true_val],
        None,
    ));
    m.functions.push(f);

    let findings = C700PqVerifyWithoutNonce.analyze(&m, "");
    assert!(
        findings.is_empty(),
        "C700 must NOT fire on properly bracketed verify: {findings:?}"
    );
}

#[test]
fn c700_silent_when_key_derives_via_keccak_of_sig() {
    // Real-world pattern: hash the signature first to use as nullifier key.
    let mut m = module();
    let mut f = action("safe_verify_hashed_key");
    let msg = Value(0);
    let sig = Value(1);
    let pk = Value(2);
    let nullifier_map = Value(3);
    let key = Value(4); // = Keccak(sig)
    let has_result = Value(5);
    let true_val = Value(6);
    // key = Keccak(sig) — sig is operand → forward taint propagates to key.
    f.blocks[0]
        .instructions
        .push(instr(Opcode::Keccak, vec![sig], Some(key)));
    f.blocks[0].instructions.push(instr(
        Opcode::MapHas,
        vec![nullifier_map, key],
        Some(has_result),
    ));
    f.blocks[0]
        .instructions
        .push(instr(Opcode::PqVerifyDilithium, vec![msg, sig, pk], None));
    f.blocks[0].instructions.push(instr(
        Opcode::MapSet,
        vec![nullifier_map, key, true_val],
        None,
    ));
    m.functions.push(f);

    let findings = C700PqVerifyWithoutNonce.analyze(&m, "");
    assert!(
        findings.is_empty(),
        "C700 must NOT fire when nullifier key derives from sig via Keccak: {findings:?}"
    );
}

#[test]
fn c700_silent_for_function_with_no_pq_verify() {
    let mut m = module();
    let mut f = action("non_pq_action");
    f.blocks[0].instructions.push(instr(
        Opcode::MapSet,
        vec![Value(0), Value(1), Value(2)],
        None,
    ));
    m.functions.push(f);

    let findings = C700PqVerifyWithoutNonce.analyze(&m, "");
    assert!(
        findings.is_empty(),
        "C700 must not fire without a verify: {findings:?}"
    );
}

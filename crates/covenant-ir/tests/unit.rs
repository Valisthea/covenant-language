//! Unit tests for IR lowering.

use covenant_diag::{Diagnostic, DiagnosticLevel, SourceId};
use covenant_ir::{build_ir, validate, IrFunctionKind, IrModule, Opcode, Terminator};
use covenant_lexer::tokenize;
use covenant_parser::parse;
use covenant_privacy::analyze_privacy;
use covenant_resolver::resolve;
use covenant_types::typecheck;

fn pipeline(src: &str) -> (IrModule, Vec<Diagnostic>) {
    let (toks, lex) = tokenize(src, SourceId::new(0));
    assert!(lex.is_empty(), "lex: {lex:?}");
    let (file, pd) = parse(&toks, SourceId::new(0));
    assert!(pd.is_empty(), "parse: {pd:?}");
    let (res, _) = resolve(file.expect("file"), SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (checked, _) = analyze_privacy(typed, SourceId::new(0));
    build_ir(checked, SourceId::new(0))
}

fn errors(d: &[Diagnostic]) -> Vec<&Diagnostic> {
    d.iter()
        .filter(|x| x.level == DiagnosticLevel::Error)
        .collect()
}

fn has_opcode(m: &IrModule, matcher: impl Fn(&Opcode) -> bool) -> bool {
    m.functions.iter().any(|f| {
        f.blocks
            .iter()
            .flat_map(|b| b.instructions.iter())
            .any(|i| matcher(&i.opcode))
    })
}

#[test]
fn builds_empty_record() {
    let (m, d) = pipeline("record R {}\n");
    assert!(errors(&d).is_empty(), "{d:?}");
    assert_eq!(m.functions.len(), 0);
    assert_eq!(m.fields.len(), 0);
}

#[test]
fn lowers_single_field() {
    let (m, _) = pipeline("record R { x: amount }\n");
    assert_eq!(m.fields.len(), 1);
    assert_eq!(m.fields[0].name.name.as_ref(), "x");
}

#[test]
fn lowers_action_with_return_unit() {
    let src = r#"
record R {
    action go() {}
}
"#;
    let (m, _) = pipeline(src);
    let f = &m.functions[0];
    assert!(matches!(f.kind, IrFunctionKind::Action));
    // Entry block terminator is Return(None) by default.
    let entry = &f.blocks[f.entry.0 as usize];
    assert!(matches!(entry.terminator, Terminator::Return(None)));
}

#[test]
fn lowers_view_returning_field() {
    let src = r#"
record R {
    x: amount
    view g returns amount { x }
}
"#;
    let (m, _) = pipeline(src);
    // View has an SLoad and Return(Some(_)).
    assert!(has_opcode(&m, |o| matches!(o, Opcode::SLoad(_))));
    let view = m
        .functions
        .iter()
        .find(|f| matches!(f.kind, IrFunctionKind::View))
        .expect("view present");
    let entry = &view.blocks[view.entry.0 as usize];
    assert!(matches!(entry.terminator, Terminator::Return(Some(_))));
}

#[test]
fn lowers_reveal_one_liner_with_decrypt() {
    let src = r#"
encrypted counter C {
    total: amount
    reveal total to owner
}
"#;
    let (m, _) = pipeline(src);
    assert!(has_opcode(&m, |o| matches!(o, Opcode::RevealDecrypt)));
}

#[test]
fn lowers_plaintext_arithmetic_as_checked() {
    let src = r#"
record R {
    view s returns amount { 1 + 2 }
}
"#;
    let (m, _) = pipeline(src);
    assert!(has_opcode(&m, |o| matches!(o, Opcode::AddChecked)));
}

#[test]
fn lowers_ciphertext_arithmetic_as_fhe() {
    let src = r#"
record R {
    a: ciphertext<amount>
    b: ciphertext<amount>
    view s returns ciphertext<amount> { a + b }
}
"#;
    let (m, _) = pipeline(src);
    assert!(has_opcode(&m, |o| matches!(o, Opcode::FheAdd)));
}

#[test]
fn lowers_mixed_arithmetic_with_lift() {
    let src = r#"
record R {
    a: ciphertext<amount>
    action bump(by: amount) {
        a += by
    }
}
"#;
    let (m, _) = pipeline(src);
    // The compound assign should emit FheEncryptTrivial (from_lift) + FheAdd.
    let has_lift = m.functions.iter().any(|f| {
        f.blocks
            .iter()
            .flat_map(|b| b.instructions.iter())
            .any(|i| matches!(i.opcode, Opcode::FheEncryptTrivial) && i.metadata.from_lift)
    });
    assert!(has_lift);
    assert!(has_opcode(&m, |o| matches!(o, Opcode::FheAdd)));
}

#[test]
fn lowers_encrypted_comparison() {
    let src = r#"
record R {
    a: ciphertext<amount>
    b: ciphertext<amount>
    view cmp returns ciphertext<bool> { a < b }
}
"#;
    let (m, _) = pipeline(src);
    assert!(has_opcode(&m, |o| matches!(o, Opcode::FheCmpLt)));
}

#[test]
fn lowers_emit_event() {
    let src = r#"
record R {
    event E(who: address)
    action go() { emit E(caller) }
}
"#;
    let (m, _) = pipeline(src);
    assert!(has_opcode(&m, |o| matches!(o, Opcode::Emit(_))));
}

#[test]
fn lowers_transfer() {
    let src = r#"
record R {
    action send(to_addr: address) { transfer 100 to to_addr }
}
"#;
    let (m, _) = pipeline(src);
    assert!(has_opcode(&m, |o| matches!(o, Opcode::Transfer)));
}

#[test]
fn lowers_revert_with() {
    let src = r#"
record R {
    error Bad(why: amount)
    action go() { revert_with Bad(42) }
}
"#;
    let (m, _) = pipeline(src);
    // Terminator should be Revert.
    let has_revert = m.functions.iter().any(|f| {
        f.blocks
            .iter()
            .any(|b| matches!(b.terminator, Terminator::Revert { .. }))
    });
    assert!(has_revert);
}

#[test]
fn lowers_pq_signed_qualifier() {
    let src = r#"
record R {
    action go(m: hash, s: bytes, p: pq_key) pq_signed(m, s, p) {}
}
"#;
    let (m, _) = pipeline(src);
    assert!(has_opcode(&m, |o| matches!(o, Opcode::PqVerifyDilithium)));
    assert!(has_opcode(&m, |o| matches!(o, Opcode::Assert)));
}

#[test]
fn lowers_verified_by_qualifier() {
    let src = r#"
record R {
    action go(proof: bytes) verified_by(proof) {}
}
"#;
    let (m, _) = pipeline(src);
    assert!(has_opcode(&m, |o| matches!(o, Opcode::ZkVerify)));
}

#[test]
fn lowers_when_guard_to_assert() {
    let src = r#"
record R {
    action go() when 1 < 2 {}
}
"#;
    let (m, _) = pipeline(src);
    assert!(has_opcode(&m, |o| matches!(o, Opcode::Assert)));
}

#[test]
fn lowers_if_statement_to_branch() {
    let src = r#"
record R {
    x: amount
    action go() {
        if 1 < 2 { x = 1 } else { x = 2 }
    }
}
"#;
    let (m, _) = pipeline(src);
    let has_branch = m.functions.iter().any(|f| {
        f.blocks
            .iter()
            .any(|b| matches!(b.terminator, Terminator::Branch { .. }))
    });
    assert!(has_branch);
}

#[test]
fn if_without_else_does_not_orphan_trailing_statements() {
    // Regression test for OMEGA V6 CRT-002: a bare `if` (no `else`) followed
    // by more statements used to leave the empty else-branch carrying the
    // `new_block()` placeholder `Terminator::Unreachable`, which the old
    // Stmt::If lowering mistook for "already terminated" and never patched
    // in a Jump to the merge block -- silently orphaning every statement
    // after the guard (deleted later by DCE as genuinely unreachable).
    let src = r#"
record R {
    x: amount
    action go() {
        if x > 10 {
            revert_with TooBig()
        }
        x = 1
    }
    error TooBig()
}
"#;
    let (m, _) = pipeline(src);
    for f in &m.functions {
        for b in &f.blocks {
            assert!(
                !matches!(b.terminator, Terminator::Unreachable),
                "function {:?} has an orphaned Unreachable block {:?} -- the no-else \
                 fallthrough Jump to merge was not emitted (CRT-002 regression)",
                f.name,
                b.id
            );
        }
    }
}

#[test]
fn if_with_empty_else_does_not_orphan_trailing_statements() {
    // Same defect, explicit-empty-`else {}` variant.
    let src = r#"
record R {
    x: amount
    action go() {
        if x > 10 {
            revert_with TooBig()
        } else {
        }
        x = 1
    }
    error TooBig()
}
"#;
    let (m, _) = pipeline(src);
    for f in &m.functions {
        for b in &f.blocks {
            assert!(
                !matches!(b.terminator, Terminator::Unreachable),
                "function {:?} has an orphaned Unreachable block {:?} (CRT-002 regression, \
                 explicit-empty-else variant)",
                f.name,
                b.id
            );
        }
    }
}

#[test]
fn lowers_encrypted_when_to_fhe_branch() {
    let src = r#"
record R {
    cond: ciphertext<bool>
    action go() {
        encrypted_when cond {}
    }
}
"#;
    let (m, _) = pipeline(src);
    let has_fhe_branch = m.functions.iter().any(|f| {
        f.blocks
            .iter()
            .any(|b| matches!(b.terminator, Terminator::FheBranch { .. }))
    });
    assert!(has_fhe_branch);
}

#[test]
fn lowers_map_access() {
    let src = r#"
record R {
    balances: map<address, amount>
    view bal(who: address) returns amount { balances[who] }
}
"#;
    let (m, _) = pipeline(src);
    assert!(has_opcode(&m, |o| matches!(o, Opcode::MapGet)));
}

#[test]
fn lowers_stdlib_keccak_call() {
    let src = r#"
record R {
    view h returns hash { keccak(caller) }
}
"#;
    let (m, _) = pipeline(src);
    assert!(has_opcode(&m, |o| matches!(o, Opcode::Keccak)));
}

#[test]
fn lowers_stdlib_module_method() {
    let src = r#"
record R {
    action go(pk: pq_key, h: hash, s: bytes) {
        let ok = PQKeys.verify_dilithium(pk, h, s)
    }
}
"#;
    let (m, _) = pipeline(src);
    assert!(has_opcode(&m, |o| matches!(o, Opcode::PqVerifyDilithium)));
}

#[test]
fn lowers_field_load_via_sload() {
    let src = r#"
record R {
    x: amount
    view g returns amount { x }
}
"#;
    let (m, _) = pipeline(src);
    let has_sload = m.functions.iter().any(|f| {
        f.blocks
            .iter()
            .flat_map(|b| b.instructions.iter())
            .any(|i| matches!(i.opcode, Opcode::SLoad(_)))
    });
    assert!(has_sload);
}

#[test]
fn lowers_field_store_via_sstore() {
    let src = r#"
record R {
    x: amount
    action go(new_val: amount) { x = new_val }
}
"#;
    let (m, _) = pipeline(src);
    let has_sstore = m.functions.iter().any(|f| {
        f.blocks
            .iter()
            .flat_map(|b| b.instructions.iter())
            .any(|i| matches!(i.opcode, Opcode::SStore(_)))
    });
    assert!(has_sstore);
}

#[test]
fn lowers_msg_value_namespace() {
    let src = r#"
record R {
    view v returns amount { msg.value }
}
"#;
    let (m, _) = pipeline(src);
    assert!(has_opcode(&m, |o| matches!(o, Opcode::LoadMsgValue)));
}

#[test]
fn lowers_block_timestamp_namespace() {
    let src = r#"
record R {
    view v returns time { block.timestamp }
}
"#;
    let (m, _) = pipeline(src);
    assert!(has_opcode(&m, |o| matches!(o, Opcode::LoadBlockTimestamp)));
}

#[test]
fn lowers_caller_lang_ident() {
    let src = r#"
record R {
    view v returns address { caller }
}
"#;
    let (m, _) = pipeline(src);
    assert!(has_opcode(&m, |o| matches!(o, Opcode::LoadCaller)));
}

#[test]
fn lowers_compound_assign_plaintext() {
    let src = r#"
record R {
    x: amount
    action go() { x += 1 }
}
"#;
    let (m, _) = pipeline(src);
    // SLoad + AddChecked + SStore.
    assert!(has_opcode(&m, |o| matches!(o, Opcode::AddChecked)));
    assert!(has_opcode(&m, |o| matches!(o, Opcode::SStore(_))));
}

#[test]
fn emits_function_kind_reveal() {
    let src = r#"
encrypted counter C {
    total: amount
    reveal total to owner
}
"#;
    let (m, _) = pipeline(src);
    assert!(m
        .functions
        .iter()
        .any(|f| matches!(f.kind, IrFunctionKind::Reveal)));
}

#[test]
fn validator_accepts_well_formed_module() {
    let src = r#"
record R {
    x: amount
    view g returns amount { x }
    action s(v: amount) { x = v }
}
"#;
    let (m, _) = pipeline(src);
    let vdiags = validate(&m);
    assert!(vdiags.is_empty(), "{vdiags:?}");
}

#[test]
fn lowers_only_caller_guard() {
    let src = r#"
record R {
    action go() only caller {}
}
"#;
    let (m, _) = pipeline(src);
    assert!(m.functions.iter().any(|f| !f.guards.is_empty()));
}

#[test]
fn lowers_annotations() {
    let src = r#"
record R {
    @batch_up_to(32)
    action go() {}
}
"#;
    let (m, _) = pipeline(src);
    assert!(m.functions.iter().any(|f| !f.annotations.is_empty()));
}

#[test]
fn lowers_struct_registration() {
    let src = r#"
record R {
    struct Point { x: amount\n y: amount }
}
"#;
    let src = src.replace("\\n", "\n");
    let (m, _) = pipeline(&src);
    assert!(m.structs.iter().any(|s| s.name.name.as_ref() == "Point"));
}

#[test]
fn lowers_error_registration() {
    let src = r#"
record R {
    error Bad(why: amount)
}
"#;
    let (m, _) = pipeline(src);
    assert!(m.errors.iter().any(|e| e.name.name.as_ref() == "Bad"));
}

#[test]
fn lowers_event_registration() {
    let src = r#"
record R {
    event Transferred(who: address indexed, amt: amount)
}
"#;
    let (m, _) = pipeline(src);
    assert!(m
        .events
        .iter()
        .any(|e| e.name.name.as_ref() == "Transferred"));
}

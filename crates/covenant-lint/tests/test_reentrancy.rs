//! Tests for REE detectors (C001, C002, W003, I004).

mod helpers;
use helpers::run;

use covenant_lint::detectors::reentrancy::{
    C001StateAfterTransfer, C002TransferInLoop, I004TransferInInitializer, W003MissingNonReentrant,
};

// ── C001 ────────────────────────────────────────────────────────────────────

#[test]
fn c001_fires_when_sstore_after_transfer() {
    let src = r#"
record Vuln {
    bal: amount
    action withdraw(recipient: address, amt: amount) {
        transfer amt to recipient
        bal = 0
    }
}
"#;
    let f = run(&C001StateAfterTransfer, src);
    assert!(!f.is_empty(), "expected C001 finding");
    assert!(f.iter().all(|f| f.detector_code == "C001"));
}

#[test]
fn c001_clean_sstore_before_transfer() {
    let src = r#"
record Safe {
    bal: amount
    action withdraw(recipient: address, amt: amount) {
        bal = 0
        transfer amt to recipient
    }
}
"#;
    let f = run(&C001StateAfterTransfer, src);
    assert!(f.is_empty(), "unexpected C001: {f:?}");
}

#[test]
fn c001_no_transfer_no_finding() {
    let src = r#"
record R {
    x: amount
    action go() only caller { x = 1 }
}
"#;
    let f = run(&C001StateAfterTransfer, src);
    assert!(f.is_empty());
}

#[test]
fn c001_view_functions_not_flagged() {
    let src = r#"
record R {
    x: amount
    view get returns amount { x }
}
"#;
    let f = run(&C001StateAfterTransfer, src);
    assert!(f.is_empty());
}

// ── W003 ────────────────────────────────────────────────────────────────────

#[test]
fn w003_fires_without_non_reentrant() {
    let src = r#"
record R {
    action pay(recipient: address) {
        transfer 10 to recipient
    }
}
"#;
    let f = run(&W003MissingNonReentrant, src);
    assert!(!f.is_empty(), "expected W003");
    assert_eq!(f[0].detector_code, "W003");
}

#[test]
fn w003_suppressed_via_allow() {
    // W003 is suppressed at the lint_source level via @allow(W003).
    use covenant_diag::SourceId;
    let src = r#"
@allow(W003, reason: "pull-over-push implemented externally")
record R {
    action pay(recipient: address) {
        transfer 10 to recipient
    }
}
"#;
    let findings = covenant_lint::lint_source(src, SourceId::new(0));
    assert!(
        findings.iter().all(|f| f.detector_code != "W003"),
        "W003 should be suppressed via @allow: {findings:?}"
    );
}

#[test]
fn w003_no_transfer_no_finding() {
    let src = r#"
record R {
    x: amount
    action bump() only caller { x += 1 }
}
"#;
    let f = run(&W003MissingNonReentrant, src);
    assert!(f.is_empty());
}

#[test]
fn w003_only_flags_actions_not_views() {
    let src = r#"
record R {
    x: amount
    view get returns amount { x }
}
"#;
    let f = run(&W003MissingNonReentrant, src);
    assert!(f.is_empty());
}

// ── C002 ────────────────────────────────────────────────────────────────────

#[test]
fn c002_no_loop_no_finding() {
    let src = r#"
record R {
    action pay(recipient: address) {
        transfer 1 to recipient
    }
}
"#;
    let f = run(&C002TransferInLoop, src);
    // Simple action without a back-edge loop → no finding expected.
    assert!(f.is_empty(), "unexpected C002 without a loop: {f:?}");
}

// ── I004 ────────────────────────────────────────────────────────────────────

#[test]
fn i004_no_finding_in_regular_action() {
    let src = r#"
record R {
    action pay(recipient: address) {
        transfer 5 to recipient
    }
}
"#;
    let f = run(&I004TransferInInitializer, src);
    // Regular actions are not FieldInitializers.
    assert!(f.is_empty(), "unexpected I004: {f:?}");
}

#[test]
fn i004_no_finding_without_transfer() {
    let src = r#"
record R {
    x: amount
    action go() only caller { x = 1 }
}
"#;
    let f = run(&I004TransferInInitializer, src);
    assert!(f.is_empty());
}

// ── additional C001 edge cases ───────────────────────────────────────────────

#[test]
fn c001_multiple_sstores_after_transfer_all_flagged() {
    let src = r#"
record R {
    a: amount
    b: amount
    action go(recipient: address) {
        transfer 1 to recipient
        a = 0
        b = 0
    }
}
"#;
    let f = run(&C001StateAfterTransfer, src);
    // Both SStore after Transfer should be flagged.
    assert_eq!(f.len(), 2, "expected 2 C001 findings: {f:?}");
}

#[test]
fn w003_fires_for_each_unguarded_transfer_action() {
    let src = r#"
record R {
    action pay_a(recipient: address) { transfer 1 to recipient }
    action pay_b(recipient: address) { transfer 2 to recipient }
}
"#;
    let f = run(&W003MissingNonReentrant, src);
    assert_eq!(
        f.len(),
        2,
        "expected W003 for each unguarded transfer action: {f:?}"
    );
}

// ── KSR-CVN-018 : `Opcode::ExternalCall` must be treated like `Transfer` ─────
//
// Before the fix, only `transfer ... to ...` (Opcode::Transfer) tripped
// reentrancy detectors. Direct calls to other contracts via `IFoo.at(addr)`
// emit `Opcode::ExternalCall { .. }` and were silently ignored, the most
// common real-world reentrancy shape (e.g. `IERC20.at(t).transfer(to,v)`
// followed by a state write) bypassed every reentrancy lint.
//
// External contracts aren't reachable through the lint test's source-level
// pipeline (the resolver doesn't expose them), so these tests build the IR
// directly with an `ExternalCall` instruction and verify the detector fires.
mod ksr_cvn_018 {
    use super::*;
    use covenant_lint::framework::Detector;
    use std::collections::HashMap;

    use covenant_diag::{SourceId, Span};
    use covenant_ir::{
        block::IrBlock,
        function::{IrAnnotation, IrFunction, IrFunctionKind},
        id::{BlockId, FunctionId, GlobalId, Value},
        instr::{Instr, InstrMetadata, Terminator},
        module::IrModule,
        Opcode,
    };

    fn module() -> IrModule {
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

    fn action(name: &str) -> IrFunction {
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

    fn ext_call() -> Instr {
        Instr {
            result: None,
            opcode: Opcode::ExternalCall {
                abi_sig: "transfer(address,uint256)".into(),
                is_view: false,
                arg_count: 2,
            },
            operands: vec![],
            metadata: InstrMetadata::default(),
            span: Span::new(SourceId::new(0), 0, 0),
        }
    }

    fn sstore() -> Instr {
        Instr {
            result: None,
            opcode: Opcode::SStore(GlobalId(0)),
            operands: vec![Value(0)],
            metadata: InstrMetadata::default(),
            span: Span::new(SourceId::new(0), 0, 0),
        }
    }

    #[test]
    fn c001_fires_when_sstore_after_external_call() {
        let mut m = module();
        let mut f = action("withdraw");
        f.blocks[0].instructions.push(ext_call());
        f.blocks[0].instructions.push(sstore());
        m.functions.push(f);

        let findings = C001StateAfterTransfer.analyze(&m, "");
        assert!(
            !findings.is_empty(),
            "C001 must flag SStore after ExternalCall (KSR-CVN-018): {findings:?}"
        );
        assert!(findings.iter().all(|f| f.detector_code == "C001"));
    }

    #[test]
    fn w003_fires_for_external_call_without_non_reentrant() {
        let mut m = module();
        let mut f = action("route");
        f.blocks[0].instructions.push(ext_call());
        m.functions.push(f);

        let findings = W003MissingNonReentrant.analyze(&m, "");
        assert!(
            !findings.is_empty(),
            "W003 must flag ExternalCall in non-annotated action (KSR-CVN-018): {findings:?}"
        );
        assert_eq!(findings[0].detector_code, "W003");
    }

    #[test]
    fn w003_silent_when_external_call_guarded_by_non_reentrant() {
        let mut m = module();
        let mut f = action("route");
        f.annotations
            .push(IrAnnotation::Unknown("non_reentrant".into()));
        f.blocks[0].instructions.push(ext_call());
        m.functions.push(f);

        let findings = W003MissingNonReentrant.analyze(&m, "");
        assert!(
            findings.is_empty(),
            "W003 must not fire when @non_reentrant is present: {findings:?}"
        );
    }
}

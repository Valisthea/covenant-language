//! IR lowering tests for Phase 17 - external contract interop.

use covenant_diag::{Diagnostic, DiagnosticLevel, SourceId};
use covenant_ir::{build_ir, IrModule, Opcode};
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

fn errors_only(d: &[Diagnostic]) -> Vec<&Diagnostic> {
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

// ---- IrModule.external_contracts registration ----

#[test]
fn external_contract_registered_in_ir_module() {
    let src = r#"
external contract IERC20 {
    function transfer(address, amount)
}
record R {}
"#;
    let (m, d) = pipeline(src);
    assert!(errors_only(&d).is_empty(), "{d:?}");
    assert_eq!(m.external_contracts.len(), 1);
    assert_eq!(m.external_contracts[0].name.name.as_ref(), "IERC20");
}

#[test]
fn external_contract_functions_registered() {
    let src = r#"
external contract IERC20 {
    function transfer(address, amount)
    function balanceOf(address) view returns amount
}
record R {}
"#;
    let (m, d) = pipeline(src);
    assert!(errors_only(&d).is_empty(), "{d:?}");
    let ec = &m.external_contracts[0];
    assert_eq!(ec.functions.len(), 2);
    assert_eq!(ec.functions[0].name.name.as_ref(), "transfer");
    assert_eq!(ec.functions[1].name.name.as_ref(), "balanceOf");
}

#[test]
fn external_contract_view_flag_preserved() {
    let src = r#"
external contract IFoo {
    function set(amount)
    function get() view returns amount
}
record R {}
"#;
    let (m, d) = pipeline(src);
    assert!(errors_only(&d).is_empty(), "{d:?}");
    let ec = &m.external_contracts[0];
    assert!(!ec.functions[0].is_view, "set should not be view");
    assert!(ec.functions[1].is_view, "get should be view");
}

#[test]
fn external_contract_param_count_correct() {
    let src = r#"
external contract IMulti {
    function foo(address, amount, bool)
}
record R {}
"#;
    let (m, d) = pipeline(src);
    assert!(errors_only(&d).is_empty(), "{d:?}");
    assert_eq!(m.external_contracts[0].functions[0].param_count, 3);
}

#[test]
fn external_contract_abi_sig_transfer() {
    let src = r#"
external contract IERC20 {
    function transfer(address, amount)
}
record R {}
"#;
    let (m, d) = pipeline(src);
    assert!(errors_only(&d).is_empty(), "{d:?}");
    let sig = &m.external_contracts[0].functions[0].abi_sig;
    assert_eq!(sig.as_ref(), "transfer(address,uint256)");
}

#[test]
fn external_contract_abi_sig_no_params() {
    let src = r#"
external contract IFoo {
    function ping()
}
record R {}
"#;
    let (m, d) = pipeline(src);
    assert!(errors_only(&d).is_empty(), "{d:?}");
    let sig = &m.external_contracts[0].functions[0].abi_sig;
    assert_eq!(sig.as_ref(), "ping()");
}

#[test]
fn external_contract_abi_sig_balance_of() {
    let src = r#"
external contract IERC20 {
    function balanceOf(address) view returns amount
}
record R {}
"#;
    let (m, d) = pipeline(src);
    assert!(errors_only(&d).is_empty(), "{d:?}");
    let sig = &m.external_contracts[0].functions[0].abi_sig;
    assert_eq!(sig.as_ref(), "balanceOf(address)");
}

#[test]
fn no_external_contracts_field_empty() {
    let src = "record R { x: amount }";
    let (m, d) = pipeline(src);
    assert!(errors_only(&d).is_empty(), "{d:?}");
    assert!(m.external_contracts.is_empty());
}

#[test]
fn multiple_external_contracts_all_registered() {
    let src = r#"
external contract IA {
    function a()
}
external contract IB {
    function b()
}
record R {}
"#;
    let (m, d) = pipeline(src);
    assert!(errors_only(&d).is_empty(), "{d:?}");
    assert_eq!(m.external_contracts.len(), 2);
}

#[test]
fn external_contract_hash_param_abi_sig() {
    let src = r#"
external contract IVerifier {
    function verify(hash, bool) view returns bool
}
record R {}
"#;
    let (m, d) = pipeline(src);
    assert!(errors_only(&d).is_empty(), "{d:?}");
    let sig = &m.external_contracts[0].functions[0].abi_sig;
    assert_eq!(sig.as_ref(), "verify(bytes32,bool)");
}

// ---- ExternalCall opcode emission ----

#[test]
fn external_call_opcode_emitted_for_dot_at_pattern() {
    // IERC20.at(token_addr).transfer(to, val) should emit ExternalCall
    let src = r#"
external contract IERC20 {
    function transfer(address, amount)
}
record R {
    action do_transfer(tok: address, dest: address, val: amount) only caller {
        IERC20.at(tok).transfer(dest, val)
    }
}
"#;
    let (m, d) = pipeline(src);
    // May have non-error diagnostics (e.g. user_call fallback warnings), accept those.
    assert!(errors_only(&d).is_empty(), "{d:?}");
    let found = has_opcode(&m, |op| matches!(op, Opcode::ExternalCall { .. }));
    assert!(found, "ExternalCall opcode should be emitted");
}

#[test]
fn external_call_opcode_is_view_for_view_function() {
    let src = r#"
external contract IERC20 {
    function balanceOf(address) view returns amount
}
record R {
    view check(tok: address, who: address) returns amount {
        IERC20.at(tok).balanceOf(who)
    }
}
"#;
    let (m, d) = pipeline(src);
    assert!(errors_only(&d).is_empty(), "{d:?}");
    let found = m.functions.iter().any(|f| {
        f.blocks
            .iter()
            .flat_map(|b| b.instructions.iter())
            .any(|i| matches!(&i.opcode, Opcode::ExternalCall { is_view, .. } if *is_view))
    });
    assert!(found, "ExternalCall for balanceOf should be is_view=true");
}

#[test]
fn external_call_abi_sig_in_opcode() {
    let src = r#"
external contract IERC20 {
    function transfer(address, amount)
}
record R {
    action do_transfer(tok: address, dest: address, val: amount) only caller {
        IERC20.at(tok).transfer(dest, val)
    }
}
"#;
    let (m, d) = pipeline(src);
    assert!(errors_only(&d).is_empty(), "{d:?}");
    let sig = m.functions.iter().find_map(|f| {
        f.blocks
            .iter()
            .flat_map(|b| b.instructions.iter())
            .find_map(|i| {
                if let Opcode::ExternalCall { abi_sig, .. } = &i.opcode {
                    Some(abi_sig.clone())
                } else {
                    None
                }
            })
    });
    assert!(sig.is_some(), "should have ExternalCall opcode");
    assert_eq!(sig.unwrap().as_ref(), "transfer(address,uint256)");
}

#[test]
fn external_call_arg_count_in_opcode() {
    let src = r#"
external contract IERC20 {
    function transfer(address, amount)
}
record R {
    action send(tok: address, dest: address, val: amount) only caller {
        IERC20.at(tok).transfer(dest, val)
    }
}
"#;
    let (m, d) = pipeline(src);
    assert!(errors_only(&d).is_empty(), "{d:?}");
    let arg_count = m.functions.iter().find_map(|f| {
        f.blocks
            .iter()
            .flat_map(|b| b.instructions.iter())
            .find_map(|i| {
                if let Opcode::ExternalCall { arg_count, .. } = &i.opcode {
                    Some(*arg_count)
                } else {
                    None
                }
            })
    });
    assert_eq!(arg_count, Some(2), "transfer takes 2 args");
}

#[test]
fn external_call_no_arg_function() {
    let src = r#"
external contract IFoo {
    function ping()
}
record R {
    action do_ping(target: address) only caller {
        IFoo.at(target).ping()
    }
}
"#;
    let (m, d) = pipeline(src);
    assert!(errors_only(&d).is_empty(), "{d:?}");
    let arg_count = m.functions.iter().find_map(|f| {
        f.blocks
            .iter()
            .flat_map(|b| b.instructions.iter())
            .find_map(|i| {
                if let Opcode::ExternalCall { arg_count, .. } = &i.opcode {
                    Some(*arg_count)
                } else {
                    None
                }
            })
    });
    assert_eq!(arg_count, Some(0));
}

#[test]
fn opcode_external_call_not_is_view_for_non_view() {
    let src = r#"
external contract IFoo {
    function set(amount)
}
record R {
    action call_set(target: address, val: amount) only caller {
        IFoo.at(target).set(val)
    }
}
"#;
    let (m, d) = pipeline(src);
    assert!(errors_only(&d).is_empty(), "{d:?}");
    let is_view = m.functions.iter().any(|f| {
        f.blocks
            .iter()
            .flat_map(|b| b.instructions.iter())
            .any(|i| matches!(&i.opcode, Opcode::ExternalCall { is_view, .. } if *is_view))
    });
    assert!(!is_view, "set() is not a view: is_view should be false");
}

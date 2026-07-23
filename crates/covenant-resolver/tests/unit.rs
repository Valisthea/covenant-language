//! Per-feature unit tests for the resolver.

use covenant_diag::{DiagCode, Diagnostic, DiagnosticLevel, SourceId};
use covenant_lexer::tokenize;
use covenant_parser::parse;
use covenant_resolver::{
    codes, resolve, Binding, BuiltinPredicate, LangIdent, ResolvedFile, StdlibFn, StdlibModule,
};

fn do_resolve(src: &str) -> (ResolvedFile, Vec<Diagnostic>) {
    let (toks, lex) = tokenize(src, SourceId::new(0));
    assert!(lex.is_empty(), "lex: {lex:?}");
    let (file, pd) = parse(&toks, SourceId::new(0));
    assert!(pd.is_empty(), "parse: {pd:?}");
    resolve(file.expect("file"), SourceId::new(0))
}

fn errors_only(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
    diags
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Error)
        .collect()
}

fn any_binding<F: Fn(&Binding) -> bool>(res: &ResolvedFile, pred: F) -> bool {
    res.bindings.ident_bindings.values().any(pred)
}

fn any_code(diags: &[Diagnostic], code: DiagCode) -> bool {
    diags.iter().any(|d| d.code == code)
}

// ---- Field / argument / let resolution ----

#[test]
fn resolves_field_in_action_body() {
    let src = r#"
record R {
    tally_count: amount
    action inc() {
        tally_count += 1
    }
}
"#;
    let (res, diags) = do_resolve(src);
    assert!(errors_only(&diags).is_empty(), "{diags:?}");
    assert!(any_binding(&res, |b| matches!(b, Binding::Field(_))));
}

#[test]
fn resolves_argument_in_action_body() {
    let src = r#"
record R {
    x: amount
    action set(new_val: amount) {
        x = new_val
    }
}
"#;
    let (res, diags) = do_resolve(src);
    assert!(errors_only(&diags).is_empty(), "{diags:?}");
    assert!(any_binding(&res, |b| matches!(b, Binding::Local(_))));
}

#[test]
fn resolves_let_binding() {
    let src = r#"
record R {
    action do() {
        let tmp = 42
        let doubled = tmp + tmp
    }
}
"#;
    let (_res, diags) = do_resolve(src);
    assert!(errors_only(&diags).is_empty(), "{diags:?}");
}

// ---- Language-provided identifiers ----

#[test]
fn resolves_caller_as_langident() {
    let src = r#"
record R {
    action do() {
        let c = caller
    }
}
"#;
    let (res, _) = do_resolve(src);
    assert!(any_binding(&res, |b| matches!(
        b,
        Binding::LangIdent(LangIdent::Caller)
    )));
}

#[test]
fn resolves_now_as_langident() {
    let src = r#"
record R {
    action do() {
        let t = now
    }
}
"#;
    let (res, _) = do_resolve(src);
    assert!(any_binding(&res, |b| matches!(
        b,
        Binding::LangIdent(LangIdent::Now)
    )));
}

#[test]
fn resolves_deployer_as_langident() {
    let src = r#"
record R {
    field admin_addr: address = deployer
}
"#;
    let (res, _) = do_resolve(src);
    assert!(any_binding(&res, |b| matches!(
        b,
        Binding::LangIdent(LangIdent::Deployer)
    )));
}

#[test]
fn resolves_zero_address_as_langident() {
    let src = r#"
record R {
    field z: address = zero_address
}
"#;
    let (res, _) = do_resolve(src);
    assert!(any_binding(&res, |b| matches!(
        b,
        Binding::LangIdent(LangIdent::ZeroAddress)
    )));
}

// ---- Events / errors ----

#[test]
fn resolves_event_in_emit() {
    let src = r#"
record R {
    event Changed(who: address indexed)
    action go() {
        emit Changed(caller)
    }
}
"#;
    let (res, diags) = do_resolve(src);
    assert!(errors_only(&diags).is_empty(), "{diags:?}");
    assert!(any_binding(&res, |b| matches!(b, Binding::Event(_))));
}

#[test]
fn resolves_error_in_revert_with() {
    let src = r#"
record R {
    error Bad(reason: amount)
    action go() {
        revert_with Bad(1)
    }
}
"#;
    let (res, diags) = do_resolve(src);
    assert!(errors_only(&diags).is_empty(), "{diags:?}");
    assert!(any_binding(&res, |b| matches!(b, Binding::Error(_))));
}

// ---- Stdlib resolution ----

#[test]
fn resolves_stdlib_function_keccak() {
    let src = r#"
record R {
    action go() {
        let h = keccak(caller)
    }
}
"#;
    let (res, _) = do_resolve(src);
    assert!(any_binding(&res, |b| matches!(
        b,
        Binding::StdlibFn(StdlibFn::Keccak)
    )));
}

#[test]
fn resolves_stdlib_function_encode() {
    let src = r#"
record R {
    action go() {
        let b = encode(caller)
    }
}
"#;
    let (res, _) = do_resolve(src);
    assert!(any_binding(&res, |b| matches!(
        b,
        Binding::StdlibFn(StdlibFn::Encode)
    )));
}

#[test]
fn resolves_stdlib_module_pqkeys() {
    let src = r#"
record R {
    action go() {
        let m = PQKeys
    }
}
"#;
    let (res, _) = do_resolve(src);
    assert!(any_binding(&res, |b| matches!(
        b,
        Binding::StdlibModule(StdlibModule::PQKeys)
    )));
}

#[test]
fn resolves_stdlib_module_math() {
    let src = r#"
record R {
    action go() {
        let m = Math
    }
}
"#;
    let (res, _) = do_resolve(src);
    assert!(any_binding(&res, |b| matches!(
        b,
        Binding::StdlibModule(StdlibModule::Math)
    )));
}

// ---- Builtin predicates ----

#[test]
fn resolves_builtin_predicate_first_time_caller() {
    let src = r#"
record R {
    action only_once() only first_time_caller {
        let x = 1
    }
}
"#;
    let (res, _) = do_resolve(src);
    assert!(any_binding(&res, |b| matches!(
        b,
        Binding::BuiltinPredicate(BuiltinPredicate::FirstTimeCaller)
    )));
}

#[test]
fn resolves_builtin_predicate_validator_majority_with_args() {
    let src = r#"
record R {
    action go(sigs: bytes) only validator_majority(sigs) {
        let x = 1
    }
}
"#;
    let (res, _) = do_resolve(src);
    assert!(any_binding(&res, |b| matches!(
        b,
        Binding::BuiltinPredicate(BuiltinPredicate::ValidatorMajority)
    )));
}

#[test]
fn unknown_predicate_emits_e106() {
    let src = r#"
record R {
    action go() only xyzzy {
        let x = 1
    }
}
"#;
    let (_, diags) = do_resolve(src);
    assert!(any_code(&diags, codes::E106_UNKNOWN_PREDICATE));
}

// ---- Principals ----

#[test]
fn caller_principal_always_resolves() {
    let src = r#"
record R {
    action go() only caller {
        let x = 1
    }
}
"#;
    let (_, diags) = do_resolve(src);
    assert!(errors_only(&diags).is_empty(), "{diags:?}");
}

#[test]
fn deployer_principal_always_resolves() {
    let src = r#"
record R {
    action go() only deployer {
        let x = 1
    }
}
"#;
    let (_, diags) = do_resolve(src);
    assert!(errors_only(&diags).is_empty(), "{diags:?}");
}

// ---- Unresolved names and suggestions ----

#[test]
fn unresolved_ident_emits_e102() {
    let src = r#"
record R {
    action go() {
        let x = xyzzyqqq
    }
}
"#;
    let (_, diags) = do_resolve(src);
    assert!(any_code(&diags, codes::E102_UNRESOLVED_IDENT));
}

#[test]
fn unresolved_ident_typo_of_caller_suggests() {
    let src = r#"
record R {
    action go() {
        let who = caler
    }
}
"#;
    let (_, diags) = do_resolve(src);
    let unresolved: Vec<_> = diags
        .iter()
        .filter(|d| d.code == codes::E102_UNRESOLVED_IDENT)
        .collect();
    assert_eq!(unresolved.len(), 1);
    assert!(
        unresolved[0]
            .help
            .as_deref()
            .is_some_and(|h| h.contains("caller")),
        "expected suggestion for `caller`, got help {:?}",
        unresolved[0].help
    );
}

// ---- Shadowing warnings ----

#[test]
fn let_shadows_argument_emits_w201() {
    let src = r#"
record R {
    action go(x: amount) {
        let x = 42
    }
}
"#;
    let (_, diags) = do_resolve(src);
    assert!(any_code(&diags, codes::W201_SHADOWING));
}

#[test]
fn let_shadows_langident_emits_w202() {
    let src = r#"
record R {
    action go() {
        let caller = zero_address
    }
}
"#;
    let (_, diags) = do_resolve(src);
    assert!(any_code(&diags, codes::W202_SHADOWING_LANG));
}

// ---- Block scopes ----

#[test]
fn for_each_binding_scoped_to_body() {
    let src = r#"
record R {
    xs: [amount]
    action go() {
        for each item in xs {
            let d = item
        }
    }
}
"#;
    let (_, diags) = do_resolve(src);
    assert!(errors_only(&diags).is_empty(), "{diags:?}");
}

#[test]
fn if_block_introduces_new_scope() {
    let src = r#"
record R {
    action go() {
        if 1 > 0 {
            let x = 1
        } else {
            let x = 2
        }
    }
}
"#;
    let (_, diags) = do_resolve(src);
    assert!(errors_only(&diags).is_empty(), "{diags:?}");
}

// ---- Duplicate declaration ----

#[test]
fn duplicate_field_emits_e101() {
    let src = r#"
record R {
    x: amount
    x: amount
}
"#;
    let (_, diags) = do_resolve(src);
    assert!(any_code(&diags, codes::E101_DUPLICATE_DECL));
}

#[test]
fn duplicate_action_emits_e101() {
    let src = r#"
record R {
    action foo() {}
    action foo() {}
}
"#;
    let (_, diags) = do_resolve(src);
    assert!(any_code(&diags, codes::E101_DUPLICATE_DECL));
}

// ---- Sanity: reveal one-liner ----

#[test]
fn reveal_one_liner_resolves_field_name() {
    let src = r#"
encrypted counter C {
    total: amount
    reveal total to owner
}
"#;
    let (res, diags) = do_resolve(src);
    assert!(errors_only(&diags).is_empty(), "{diags:?}");
    // `total` in the reveal should resolve to a field.
    assert!(any_binding(&res, |b| matches!(b, Binding::Field(_))));
}

// ---- Annotations ----

#[test]
fn known_annotation_is_accepted() {
    let src = r#"
record R {
    @batch_up_to(32)
    action go() {}
}
"#;
    let (_, diags) = do_resolve(src);
    assert!(errors_only(&diags).is_empty(), "{diags:?}");
}

#[test]
fn unknown_annotation_emits_e110() {
    let src = r#"
record R {
    @fizzbuzz
    action go() {}
}
"#;
    let (_, diags) = do_resolve(src);
    assert!(any_code(&diags, codes::E110_UNKNOWN_ANNOTATION));
}

//! Per-rule unit tests for the privacy analyzer.

use covenant_diag::{DiagCode, Diagnostic, DiagnosticLevel, SourceId};
use covenant_lexer::tokenize;
use covenant_parser::parse;
use covenant_privacy::{analyze_privacy, codes, PrivacyCheckedFile, PrivacyDomain};
use covenant_resolver::resolve;
use covenant_types::typecheck;

fn pipeline(src: &str) -> (PrivacyCheckedFile, Vec<Diagnostic>) {
    let (toks, lex) = tokenize(src, SourceId::new(0));
    assert!(lex.is_empty(), "lex: {lex:?}");
    let (file, pd) = parse(&toks, SourceId::new(0));
    assert!(pd.is_empty(), "parse: {pd:?}");
    let (res, _) = resolve(file.expect("file"), SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    analyze_privacy(typed, SourceId::new(0))
}

fn has_code(d: &[Diagnostic], c: DiagCode) -> bool {
    d.iter().any(|x| x.code == c)
}

fn errors(d: &[Diagnostic]) -> Vec<&Diagnostic> {
    d.iter()
        .filter(|x| x.level == DiagnosticLevel::Error)
        .collect()
}

// ---------- R1: plaintext field from encrypted source ----------

#[test]
fn plaintext_field_from_ciphertext_emits_e301() {
    // A hybrid construct with an encrypted field and a plaintext field,
    // and an action that assigns the encrypted into the plaintext.
    let src = r#"
record R {
    pt: amount
    ct: ciphertext<amount>
    action leak() {
        pt = ct
    }
}
"#;
    let (_, d) = pipeline(src);
    assert!(has_code(&d, codes::E301_PLAINTEXT_FROM_ENCRYPTED));
}

#[test]
fn normal_plaintext_assignment_no_diag() {
    let src = r#"
record R {
    x: amount
    action go(new_val: amount) { x = new_val }
}
"#;
    let (_, d) = pipeline(src);
    assert!(errors(&d).is_empty(), "{d:?}");
}

#[test]
fn ciphertext_to_ciphertext_assign_no_diag() {
    let src = r#"
record R {
    a: ciphertext<amount>
    b: ciphertext<amount>
    action swap() { a = b }
}
"#;
    let (_, d) = pipeline(src);
    assert!(errors(&d).is_empty(), "{d:?}");
}

// ---------- R2: view encrypted body ----------

#[test]
fn view_encrypted_body_emits_e302() {
    let src = r#"
record R {
    ct: ciphertext<amount>
    view peek returns ciphertext<amount> { ct }
}
"#;
    let (_, d) = pipeline(src);
    assert!(has_code(&d, codes::E302_VIEW_ENCRYPTED_BODY));
}

// ---------- R3: reveal plaintext field (one-liner) ----------

#[test]
fn reveal_plaintext_field_emits_w301() {
    let src = r#"
record R {
    x: amount
    reveal x to owner
}
"#;
    let (_, d) = pipeline(src);
    assert!(has_code(&d, codes::W301_REVEAL_ON_PLAINTEXT));
}

#[test]
fn reveal_encrypted_field_no_warning() {
    let src = r#"
encrypted counter C {
    total: amount
    reveal total to owner
}
"#;
    let (_, d) = pipeline(src);
    assert!(errors(&d).is_empty(), "{d:?}");
    assert!(!has_code(&d, codes::W301_REVEAL_ON_PLAINTEXT));
}

// ---------- R4: reveal missing target (encrypted body) ----------

#[test]
fn reveal_encrypted_body_without_target_emits_e303() {
    let src = r#"
record R {
    ct: ciphertext<amount>
    reveal peek returns ciphertext<amount> { ct }
}
"#;
    let (_, d) = pipeline(src);
    // Even though Phase 4 will flag view/reveal concerns, R4 fires because
    // the block-form reveal crosses the boundary without `to`.
    assert!(has_code(&d, codes::E303_REVEAL_MISSING_TARGET));
}

#[test]
fn reveal_plaintext_body_without_target_no_e303() {
    // Block-form reveal of a plaintext body is equivalent to a `view` and
    // doesn't need a target.
    let src = r#"
ballot B {
    options: [choice] = ["a", "b"]
    reveal winner returns choice { options[0] }
}
"#;
    let (_, d) = pipeline(src);
    assert!(!has_code(&d, codes::E303_REVEAL_MISSING_TARGET));
}

// ---------- R5: proof_payload outside verified_by ----------

#[test]
fn proof_payload_outside_verified_by_emits_e304() {
    let src = r#"
record R {
    action go() {
        let x = proof_payload
    }
}
"#;
    let (_, d) = pipeline(src);
    assert!(has_code(&d, codes::E304_PROOF_PAYLOAD_CONTEXT));
}

// ---------- R6: destroy/freeze outside on_destroy ----------

#[test]
fn destroy_outside_on_destroy_emits_e305() {
    let src = r#"
record R {
    k: pq_key
    action go() {
        destroy(k)
    }
}
"#;
    let (_, d) = pipeline(src);
    assert!(has_code(&d, codes::E305_DESTROY_CONTEXT));
}

// ---------- R7 (warn): revert with params in encrypted_when ----------

#[test]
fn revert_with_params_in_encrypted_when_emits_w302() {
    let src = r#"
record R {
    cond: ciphertext<bool>
    error Bad(why: amount)
    action go() {
        encrypted_when cond {
            revert_with Bad(1)
        }
    }
}
"#;
    let (_, d) = pipeline(src);
    assert!(has_code(&d, codes::W302_REVERT_PARAMS_IN_ENC));
}

#[test]
fn revert_without_params_in_encrypted_when_no_warning() {
    let src = r#"
record R {
    cond: ciphertext<bool>
    error Bad()
    action go() {
        encrypted_when cond {
            revert_with Bad()
        }
    }
}
"#;
    let (_, d) = pipeline(src);
    assert!(!has_code(&d, codes::W302_REVERT_PARAMS_IN_ENC));
}

// ---------- R7 elevated: encrypted arg ----------

#[test]
fn revert_with_encrypted_arg_in_encrypted_when_emits_e306() {
    let src = r#"
record R {
    cond: ciphertext<bool>
    ct: ciphertext<amount>
    error Bad(x: ciphertext<amount>)
    action go() {
        encrypted_when cond {
            revert_with Bad(ct)
        }
    }
}
"#;
    let (_, d) = pipeline(src);
    assert!(has_code(&d, codes::E306_REVERT_ENCRYPTED_ARG));
}

// ---------- R8: emit in encrypted_when ----------

#[test]
fn emit_in_encrypted_when_emits_w303() {
    let src = r#"
record R {
    cond: ciphertext<bool>
    event E(who: address)
    action go() {
        encrypted_when cond {
            emit E(caller)
        }
    }
}
"#;
    let (_, d) = pipeline(src);
    assert!(has_code(&d, codes::W303_EMIT_IN_ENC));
}

#[test]
fn emit_outside_encrypted_when_no_warning() {
    let src = r#"
record R {
    event E(who: address)
    action go() { emit E(caller) }
}
"#;
    let (_, d) = pipeline(src);
    assert!(!has_code(&d, codes::W303_EMIT_IN_ENC));
}

// ---------- R8 elevated: encrypted emit arg ----------

#[test]
fn emit_encrypted_arg_in_encrypted_when_emits_e307() {
    let src = r#"
record R {
    cond: ciphertext<bool>
    ct: ciphertext<amount>
    event E(v: ciphertext<amount>)
    action go() {
        encrypted_when cond {
            emit E(ct)
        }
    }
}
"#;
    let (_, d) = pipeline(src);
    assert!(has_code(&d, codes::E307_EMIT_ENCRYPTED_ARG));
}

// ---------- R9: transfer in encrypted_when ----------

#[test]
fn transfer_in_encrypted_when_emits_w304() {
    let src = r#"
record R {
    cond: ciphertext<bool>
    action go(to_addr: address) {
        encrypted_when cond {
            transfer 100 to to_addr
        }
    }
}
"#;
    let (_, d) = pipeline(src);
    assert!(has_code(&d, codes::W304_TRANSFER_IN_ENC));
}

// ---------- R10: many encrypted_when in one action ----------

#[test]
fn four_encrypted_whens_in_one_action_emits_w305() {
    let src = r#"
record R {
    c1: ciphertext<bool>
    c2: ciphertext<bool>
    c3: ciphertext<bool>
    c4: ciphertext<bool>
    action go() {
        encrypted_when c1 {}
        encrypted_when c2 {}
        encrypted_when c3 {}
        encrypted_when c4 {}
    }
}
"#;
    let (_, d) = pipeline(src);
    assert!(has_code(&d, codes::W305_MANY_ENC_COMPARISONS));
}

#[test]
fn three_encrypted_whens_no_warning() {
    let src = r#"
record R {
    c1: ciphertext<bool>
    c2: ciphertext<bool>
    c3: ciphertext<bool>
    action go() {
        encrypted_when c1 {}
        encrypted_when c2 {}
        encrypted_when c3 {}
    }
}
"#;
    let (_, d) = pipeline(src);
    assert!(!has_code(&d, codes::W305_MANY_ENC_COMPARISONS));
}

// ---------- W306: encrypted_when with plaintext condition ----------

#[test]
fn encrypted_when_with_plaintext_cond_emits_w306() {
    let src = r#"
record R {
    action go() {
        encrypted_when (1 < 2) {}
    }
}
"#;
    let (_, d) = pipeline(src);
    assert!(has_code(&d, codes::W306_ENC_WHEN_PLAINTEXT_COND));
}

// ---------- Ceremony checks ----------

#[test]
fn ceremony_without_on_destroy_emits_e309() {
    let src = r#"
ceremony Ce {
    admin_key: pq_key
}
"#;
    let (_, d) = pipeline(src);
    assert!(has_code(&d, codes::E309_CEREMONY_NO_ONDESTROY));
}

#[test]
fn ceremony_with_empty_on_destroy_emits_w309() {
    let src = r#"
ceremony Ce {
    admin_key: pq_key
    on_destroy { }
}
"#;
    let (_, d) = pipeline(src);
    assert!(has_code(&d, codes::W309_CEREMONY_NO_DESTROY_KEY));
}

#[test]
fn ceremony_with_destroy_call_no_warning() {
    let src = r#"
ceremony Ce {
    admin_key: pq_key
    on_destroy {
        destroy(admin_key)
    }
}
"#;
    let (_, d) = pipeline(src);
    assert!(!has_code(&d, codes::W309_CEREMONY_NO_DESTROY_KEY));
    // destroy() is inside on_destroy context → no E305 either.
    assert!(!has_code(&d, codes::E305_DESTROY_CONTEXT));
}

// ---------- Domain correctness spot checks ----------

#[test]
fn encrypted_field_decl_has_encrypted_domain() {
    let src = r#"
encrypted counter C {
    total: amount
}
"#;
    let (c, _) = pipeline(src);
    assert!(c.domains.decl_domains.contains(&PrivacyDomain::Encrypted));
}

#[test]
fn plaintext_field_decl_has_plaintext_domain() {
    let src = "record R { x: amount }\n";
    let (c, _) = pipeline(src);
    assert!(c.domains.decl_domains.contains(&PrivacyDomain::Plaintext));
}

#[test]
fn view_returning_ciphertext_emits_e302() {
    // Direct R2 check: view body is an `encrypted<amount>` field.
    let src = r#"
record R {
    ct: ciphertext<amount>
    view peek returns ciphertext<amount> { ct }
}
"#;
    let (c, d) = pipeline(src);
    assert!(has_code(&d, codes::E302_VIEW_ENCRYPTED_BODY));
    let any_encrypted = c
        .domains
        .expr_domains
        .values()
        .any(|dd| *dd == PrivacyDomain::Encrypted);
    assert!(any_encrypted);
}

#[test]
fn if_statement_is_normal_context_no_warnings() {
    // Emit inside a plain `if` must not trigger W303.
    let src = r#"
record R {
    event E()
    action go() {
        if 1 < 2 {
            emit E()
        }
    }
}
"#;
    let (_, d) = pipeline(src);
    assert!(!has_code(&d, codes::W303_EMIT_IN_ENC));
}

#[test]
fn nested_encrypted_when_maintains_context() {
    // Two nested encrypted_whens: inner emit still flagged.
    let src = r#"
record R {
    c1: ciphertext<bool>
    c2: ciphertext<bool>
    event E()
    action go() {
        encrypted_when c1 {
            encrypted_when c2 {
                emit E()
            }
        }
    }
}
"#;
    let (_, d) = pipeline(src);
    assert!(has_code(&d, codes::W303_EMIT_IN_ENC));
}

#[test]
fn encrypted_plus_plaintext_preserves_encrypted() {
    let src = r#"
encrypted counter C {
    total: amount
    action bump(by: amount) { total += by }
}
"#;
    let (c, d) = pipeline(src);
    // No E301: the target is Encrypted, source is Plaintext (R1 doesn't fire
    // because R1 is the reverse).
    assert!(errors(&d).is_empty(), "{d:?}");
    let has_encrypted_decl = c.domains.decl_domains.contains(&PrivacyDomain::Encrypted);
    assert!(has_encrypted_decl);
}

#[test]
fn revert_without_params_outside_encrypted_when_no_warning() {
    let src = r#"
record R {
    error Bad(why: amount)
    action go() { revert_with Bad(1) }
}
"#;
    let (_, d) = pipeline(src);
    assert!(!has_code(&d, codes::W302_REVERT_PARAMS_IN_ENC));
}

#[test]
fn for_each_body_visited_by_analyzer() {
    // Inside a for_each, the body is walked. This is a smoke test that no
    // panic occurs and typical statements trigger the expected rules.
    let src = r#"
record R {
    event E(x: amount)
    action go(xs: [amount]) {
        for each x in xs {
            emit E(x)
        }
    }
}
"#;
    let (_, d) = pipeline(src);
    assert!(!has_code(&d, codes::W303_EMIT_IN_ENC));
}

#[test]
fn stdlib_keccak_yields_plaintext() {
    let src = r#"
record R {
    view h returns hash { keccak(caller) }
}
"#;
    let (c, d) = pipeline(src);
    assert!(errors(&d).is_empty(), "{d:?}");
    // All expressions in this fixture are plaintext.
    assert!(c
        .domains
        .expr_domains
        .values()
        .all(|dd| *dd == PrivacyDomain::Plaintext || *dd == PrivacyDomain::Unknown));
}

#[test]
fn ciphertext_hash_of_yields_plaintext_hash() {
    // ciphertext_hash_of(ct): Hash
    let src = r#"
record R {
    ct: ciphertext<amount>
    view h returns hash { ciphertext_hash_of(ct) }
}
"#;
    let (_, d) = pipeline(src);
    assert!(errors(&d).is_empty(), "{d:?}");
}

#[test]
fn emit_outside_enc_when_with_plaintext_args_no_warning() {
    let src = r#"
record R {
    event E(amt: amount)
    action go() { emit E(5) }
}
"#;
    let (_, d) = pipeline(src);
    assert!(d.is_empty(), "{d:?}");
}

//! Diagnostic-code coverage tests.

use covenant_diag::{DiagCode, Diagnostic, SourceId};
use covenant_lexer::tokenize;
use covenant_parser::parse;
use covenant_privacy::{analyze_privacy, codes};
use covenant_resolver::resolve;
use covenant_types::typecheck;

fn diags(src: &str) -> Vec<Diagnostic> {
    let (toks, _) = tokenize(src, SourceId::new(0));
    let (file, _) = parse(&toks, SourceId::new(0));
    let (res, _) = resolve(file.expect("file"), SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (_, d) = analyze_privacy(typed, SourceId::new(0));
    d
}

fn has(src: &str, c: DiagCode) -> bool {
    diags(src).iter().any(|d| d.code == c)
}

#[test]
fn e301_plaintext_from_encrypted() {
    assert!(has(
        r#"record R {
            pt: amount
            ct: ciphertext<amount>
            action leak() { pt = ct }
        }"#,
        codes::E301_PLAINTEXT_FROM_ENCRYPTED
    ));
}

#[test]
fn e302_view_encrypted_body() {
    assert!(has(
        r#"record R {
            ct: ciphertext<amount>
            view peek returns ciphertext<amount> { ct }
        }"#,
        codes::E302_VIEW_ENCRYPTED_BODY
    ));
}

#[test]
fn e303_reveal_missing_target() {
    assert!(has(
        r#"record R {
            ct: ciphertext<amount>
            reveal peek returns ciphertext<amount> { ct }
        }"#,
        codes::E303_REVEAL_MISSING_TARGET
    ));
}

#[test]
fn e304_proof_payload_context() {
    assert!(has(
        r#"record R {
            action go() { let x = proof_payload }
        }"#,
        codes::E304_PROOF_PAYLOAD_CONTEXT
    ));
}

#[test]
fn e305_destroy_context() {
    assert!(has(
        r#"record R {
            k: pq_key
            action go() { destroy(k) }
        }"#,
        codes::E305_DESTROY_CONTEXT
    ));
}

#[test]
fn e306_revert_encrypted_arg() {
    assert!(has(
        r#"record R {
            c: ciphertext<bool>
            ct: ciphertext<amount>
            error Bad(x: ciphertext<amount>)
            action go() {
                encrypted_when c { revert_with Bad(ct) }
            }
        }"#,
        codes::E306_REVERT_ENCRYPTED_ARG
    ));
}

#[test]
fn e307_emit_encrypted_arg() {
    assert!(has(
        r#"record R {
            c: ciphertext<bool>
            ct: ciphertext<amount>
            event E(v: ciphertext<amount>)
            action go() {
                encrypted_when c { emit E(ct) }
            }
        }"#,
        codes::E307_EMIT_ENCRYPTED_ARG
    ));
}

#[test]
fn e309_ceremony_no_on_destroy() {
    assert!(has(
        "ceremony Ce { admin_key: pq_key }\n",
        codes::E309_CEREMONY_NO_ONDESTROY
    ));
}

#[test]
fn w301_reveal_on_plaintext() {
    assert!(has(
        "record R { x: amount\n reveal x to owner }\n",
        codes::W301_REVEAL_ON_PLAINTEXT
    ));
}

#[test]
fn w302_revert_params_in_enc() {
    assert!(has(
        r#"record R {
            c: ciphertext<bool>
            error Bad(why: amount)
            action go() { encrypted_when c { revert_with Bad(1) } }
        }"#,
        codes::W302_REVERT_PARAMS_IN_ENC
    ));
}

#[test]
fn w303_emit_in_enc() {
    assert!(has(
        r#"record R {
            c: ciphertext<bool>
            event E()
            action go() { encrypted_when c { emit E() } }
        }"#,
        codes::W303_EMIT_IN_ENC
    ));
}

#[test]
fn w304_transfer_in_enc() {
    assert!(has(
        r#"record R {
            c: ciphertext<bool>
            action go(dest: address) { encrypted_when c { transfer 100 to dest } }
        }"#,
        codes::W304_TRANSFER_IN_ENC
    ));
}

#[test]
fn w305_many_enc_comparisons() {
    assert!(has(
        r#"record R {
            a: ciphertext<bool>
            b: ciphertext<bool>
            c: ciphertext<bool>
            d: ciphertext<bool>
            action go() {
                encrypted_when a {}
                encrypted_when b {}
                encrypted_when c {}
                encrypted_when d {}
            }
        }"#,
        codes::W305_MANY_ENC_COMPARISONS
    ));
}

#[test]
fn w306_enc_when_plaintext_cond() {
    assert!(has(
        r#"record R { action go() { encrypted_when (1 < 2) {} } }"#,
        codes::W306_ENC_WHEN_PLAINTEXT_COND
    ));
}

#[test]
fn w309_ceremony_no_destroy_key() {
    assert!(has(
        "ceremony Ce { k: pq_key\n on_destroy {} }\n",
        codes::W309_CEREMONY_NO_DESTROY_KEY
    ));
}

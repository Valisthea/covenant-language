//! Privacy analyzer diagnostic codes (E301–E320, W301–W310).
#![allow(dead_code)]

use covenant_diag::{DiagCode, Diagnostic, DiagnosticLevel, Span};

pub const E301_PLAINTEXT_FROM_ENCRYPTED: DiagCode = DiagCode(301);
pub const E302_VIEW_ENCRYPTED_BODY: DiagCode = DiagCode(302);
pub const E303_REVEAL_MISSING_TARGET: DiagCode = DiagCode(303);
pub const E304_PROOF_PAYLOAD_CONTEXT: DiagCode = DiagCode(304);
pub const E305_DESTROY_CONTEXT: DiagCode = DiagCode(305);
pub const E306_REVERT_ENCRYPTED_ARG: DiagCode = DiagCode(306);
pub const E307_EMIT_ENCRYPTED_ARG: DiagCode = DiagCode(307);
pub const E308_SD_NON_BOOL: DiagCode = DiagCode(308);
pub const E309_CEREMONY_NO_ONDESTROY: DiagCode = DiagCode(309);
pub const E310_BRIDGE_NO_KEY_SWITCH: DiagCode = DiagCode(310);

pub const W301_REVEAL_ON_PLAINTEXT: DiagCode = DiagCode(301);
pub const W302_REVERT_PARAMS_IN_ENC: DiagCode = DiagCode(302);
pub const W303_EMIT_IN_ENC: DiagCode = DiagCode(303);
pub const W304_TRANSFER_IN_ENC: DiagCode = DiagCode(304);
pub const W305_MANY_ENC_COMPARISONS: DiagCode = DiagCode(305);
pub const W306_ENC_WHEN_PLAINTEXT_COND: DiagCode = DiagCode(306);
pub const W307_REVEAL_ONLY_CALLER: DiagCode = DiagCode(307);
pub const W308_SD_NO_PUBLIC_OUTPUT: DiagCode = DiagCode(308);
pub const W309_CEREMONY_NO_DESTROY_KEY: DiagCode = DiagCode(309);
pub const W310_BRIDGE_ENCRYPTED_FIELD: DiagCode = DiagCode(310);

fn warn(code: DiagCode, msg: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic {
        level: DiagnosticLevel::Warning,
        code,
        message: msg.into(),
        span,
        help: None,
    }
}

pub fn plaintext_from_encrypted(span: Span) -> Diagnostic {
    Diagnostic::error(
        E301_PLAINTEXT_FROM_ENCRYPTED,
        "cannot assign an encrypted value to a plaintext target without a `reveal`",
        span,
    )
    .with_help("use `reveal` to authorize decryption, or change the target to `encrypted`")
}

pub fn view_encrypted_body(span: Span) -> Diagnostic {
    Diagnostic::error(
        E302_VIEW_ENCRYPTED_BODY,
        "view bodies must return plaintext; found encrypted value",
        span,
    )
    .with_help("use `reveal` instead of `view` when the value must be decrypted at read time")
}

pub fn reveal_missing_target(span: Span) -> Diagnostic {
    Diagnostic::error(
        E303_REVEAL_MISSING_TARGET,
        "reveal block form crossing the encrypted boundary must declare `to <principal>`",
        span,
    )
    .with_help("add `to owner`, `to caller`, `to parties`, or `to <address_expr>`")
}

pub fn proof_payload_context(span: Span) -> Diagnostic {
    Diagnostic::error(
        E304_PROOF_PAYLOAD_CONTEXT,
        "`proof_payload` is only available inside an action qualified with `verified_by(...)`",
        span,
    )
}

pub fn destroy_context(span: Span, name: &str) -> Diagnostic {
    Diagnostic::error(
        E305_DESTROY_CONTEXT,
        format!("`{name}` may only be called inside an `on_destroy` block"),
        span,
    )
}

pub fn revert_encrypted_arg(span: Span) -> Diagnostic {
    Diagnostic::error(
        E306_REVERT_ENCRYPTED_ARG,
        "encrypted value used as a `revert_with` parameter would leak plaintext after decryption",
        span,
    )
    .with_help("use a parameter-free error variant, or compute the revert condition outside the encrypted branch")
}

pub fn emit_encrypted_arg(span: Span) -> Diagnostic {
    Diagnostic::error(
        E307_EMIT_ENCRYPTED_ARG,
        "emitting an encrypted value inside an `encrypted_when` branch would leak plaintext",
        span,
    )
}

pub fn sd_non_bool(span: Span) -> Diagnostic {
    Diagnostic::error(
        E308_SD_NON_BOOL,
        "selective_disclosure body must evaluate to `bool`",
        span,
    )
}

pub fn ceremony_no_ondestroy(span: Span, name: &str) -> Diagnostic {
    Diagnostic::error(
        E309_CEREMONY_NO_ONDESTROY,
        format!("ceremony `{name}` must declare an `on_destroy {{ ... }}` block"),
        span,
    )
}

pub fn warn_reveal_plaintext(span: Span) -> Diagnostic {
    warn(
        W301_REVEAL_ON_PLAINTEXT,
        "revealing a plaintext field has no effect; consider using `view` instead",
        span,
    )
}

pub fn warn_revert_params_in_enc(span: Span) -> Diagnostic {
    warn(
        W302_REVERT_PARAMS_IN_ENC,
        "`revert_with` inside an encrypted branch should be parameter-free to minimize information leakage",
        span,
    )
}

pub fn warn_emit_in_enc(span: Span) -> Diagnostic {
    warn(
        W303_EMIT_IN_ENC,
        "emitting an event inside an encrypted branch signals branch traversal to observers",
        span,
    )
}

pub fn warn_transfer_in_enc(span: Span) -> Diagnostic {
    warn(
        W304_TRANSFER_IN_ENC,
        "`transfer` inside an encrypted branch reveals the branch outcome via observable state change",
        span,
    )
}

pub fn warn_many_enc_comparisons(span: Span, count: usize) -> Diagnostic {
    warn(
        W305_MANY_ENC_COMPARISONS,
        format!(
            "this action performs {count} encrypted comparisons; each leaks one bit of information"
        ),
        span,
    )
}

pub fn warn_enc_when_plaintext_cond(span: Span) -> Diagnostic {
    warn(
        W306_ENC_WHEN_PLAINTEXT_COND,
        "`encrypted_when` condition is already plaintext; use `if` instead",
        span,
    )
}

pub fn warn_ceremony_no_destroy_key(span: Span) -> Diagnostic {
    warn(
        W309_CEREMONY_NO_DESTROY_KEY,
        "ceremony `on_destroy` does not call `destroy()` or `freeze()`; the ceremony may be resurrectable",
        span,
    )
}

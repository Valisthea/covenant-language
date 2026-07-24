//! Stdlib-lowering diagnostic codes (E601–E620, W601–W610).
#![allow(dead_code)]

use covenant_diag::{DiagCode, Diagnostic, DiagnosticLevel, Span};

pub const E601_USER_FN_CONFLICT: DiagCode = DiagCode(601);
pub const E602_USER_FIELD_CONFLICT: DiagCode = DiagCode(602);
pub const E603_MISSING_REQUIRED_FIELD: DiagCode = DiagCode(603);
pub const E604_CONFIDENTIAL_MISSING: DiagCode = DiagCode(604);
pub const E605_SYNTHESIZED_INVALID: DiagCode = DiagCode(605);
pub const E606_BALLOT_NOT_IMPL: DiagCode = DiagCode(606);
pub const E607_BRIDGE_NOT_IMPL: DiagCode = DiagCode(607);
pub const E608_NESTED_MAP: DiagCode = DiagCode(608);
pub const E609_FIELD_TYPE_CONFLICT: DiagCode = DiagCode(609);
pub const E610_RESERVED_NAME: DiagCode = DiagCode(610);
pub const E611_CEREMONY_THRESHOLD_INVALID: DiagCode = DiagCode(611);

pub const W601_MISSING_METADATA: DiagCode = DiagCode(601);
pub const W602_USER_OVERRIDE: DiagCode = DiagCode(602);
pub const W603_NO_APPROVE: DiagCode = DiagCode(603);
pub const W604_EVENT_ALREADY_DECLARED: DiagCode = DiagCode(604);
pub const W605_ERC8227_UNEXERCISED: DiagCode = DiagCode(605);
pub const W606_SYNTH_NOT_IMPL: DiagCode = DiagCode(606);
pub const W607_AUTO_INJECTED_FIELD: DiagCode = DiagCode(607);
pub const W608_USER_ERROR_SHADOWS: DiagCode = DiagCode(608);
pub const W609_DECIMALS_RANGE: DiagCode = DiagCode(609);
pub const W610_EMPTY_SYMBOL: DiagCode = DiagCode(610);

fn warn(code: DiagCode, msg: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic {
        level: DiagnosticLevel::Warning,
        code,
        message: msg.into(),
        span,
        help: None,
    }
}

pub fn user_fn_conflict(span: Span, name: &str) -> Diagnostic {
    Diagnostic::error(
        E601_USER_FN_CONFLICT,
        format!("user-declared function `{name}` conflicts with ERC-20 synthesis"),
        span,
    )
    .with_help("rename the function or enable permissive mode to skip synthesis for this name")
}

pub fn warn_user_override(span: Span, name: &str) -> Diagnostic {
    warn(
        W602_USER_OVERRIDE,
        format!("synthesized `{name}` skipped — user's declaration shadows it; verify it conforms to the standard signature"),
        span,
    )
}

/// F10 fix: a `ceremony` construct whose `threshold` does not satisfy
/// `1 <= threshold <= guardians` degenerates the CRT-005-hardened finalize
/// gate. `threshold: 0` makes `distinct_submitters >= threshold` always true
/// (the secret can be destroyed with ZERO guardian shares), and
/// `threshold > guardians` demands more distinct submitters than can ever
/// exist (a ceremony that can never finalize). Both are refused at compile
/// time rather than lowered into plausible-but-wrong bytecode.
pub fn ceremony_threshold_invalid(
    span: Span,
    threshold: u128,
    guardians: Option<u128>,
) -> Diagnostic {
    let message = match guardians {
        Some(g) => format!(
            "ceremony `threshold` = {threshold} is invalid; it must satisfy 1 <= threshold <= guardians ({g})"
        ),
        None => format!(
            "ceremony `threshold` = {threshold} is invalid; it must be at least 1"
        ),
    };
    Diagnostic::error(E611_CEREMONY_THRESHOLD_INVALID, message, span).with_help(
        "set `threshold` to a value between 1 and the declared number of `guardians` (inclusive)",
    )
}

pub fn warn_missing_metadata(span: Span, key: &str) -> Diagnostic {
    warn(
        W601_MISSING_METADATA,
        format!("token construct missing `{key}` metadata — using default"),
        span,
    )
}

pub fn warn_synth_not_impl(span: Span, construct: &str) -> Diagnostic {
    warn(
        W606_SYNTH_NOT_IMPL,
        format!("{construct} standard-interface synthesis not yet implemented — passing construct through unchanged"),
        span,
    )
}

pub fn warn_erc8227_unexercised(span: Span) -> Diagnostic {
    warn(
        W605_ERC8227_UNEXERCISED,
        "ERC-8227 synthesis implemented but not validated on V0 Basics — exercise via Intermediate fixtures",
        span,
    )
}

pub fn warn_empty_symbol(span: Span) -> Diagnostic {
    warn(
        W610_EMPTY_SYMBOL,
        "token `symbol` is empty — wallets may display as \"Unknown\"",
        span,
    )
}

pub fn warn_decimals_range(span: Span, value: u128) -> Diagnostic {
    warn(
        W609_DECIMALS_RANGE,
        format!("decimals = {value} is unusual (expected 0..=18)"),
        span,
    )
}

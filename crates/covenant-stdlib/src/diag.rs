//! Stdlib-lowering diagnostic codes (E601-E620, E640-E659, W601-W610,
//! W640-W659).
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

pub const E640_GENESIS_PRINCIPAL_UNSUPPORTED: DiagCode = DiagCode(640);
pub const E641_SUPPLY_INITIALIZER_CONFLICT: DiagCode = DiagCode(641);
pub const E642_DECIMALS_UNREPRESENTABLE: DiagCode = DiagCode(642);
pub const E643_SHADOWED_SYNTH_SHAPE: DiagCode = DiagCode(643);

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

/// F-40 fix: the message used to be hardcoded to "ERC-20" even though the
/// same constructor is called by the ERC-721, ERC-8227 and ERC-8231
/// synthesizers, so an `nft` author was told about a standard their file does
/// not use. `standard` now names the surface that actually owns the clashing
/// name. The help no longer offers permissive mode: `strict_conflict_detection`
/// is reachable only from the `covenant build` library API, never from a `.cov`
/// file or a CLI flag, so pointing a source author at it named a fix the tool
/// cannot perform.
pub fn user_fn_conflict(span: Span, standard: &str, name: &str) -> Diagnostic {
    Diagnostic::error(
        E601_USER_FN_CONFLICT,
        format!("user-declared function `{name}` conflicts with {standard} synthesis"),
        span,
    )
    .with_help(format!(
        "rename the function: `{name}` is part of the {standard} surface this construct synthesizes, and the synthesized version is what callers reach through the standard selector"
    ))
}

pub fn warn_user_override(span: Span, name: &str) -> Diagnostic {
    warn(
        W602_USER_OVERRIDE,
        format!("synthesized `{name}` skipped: user's declaration shadows it; verify it conforms to the standard signature"),
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

/// F-14 fix: the synthesizers resolve their required state by NAME only, so a
/// user `field balances: amount` was reused verbatim as the ERC-20 balances
/// mapping. The deployed runtime then indexes that slot as a keccak map base
/// while the storage sidecar and the author's own views describe it as
/// whatever was declared, and `covenant layout` diffs a sidecar that
/// misdescribes the region. With a wrong-KEY map (`map<amount, amount>` in
/// place of `map<address, amount>`) it is worse than disconnected: both tables
/// share one base, so an integer key numerically equal to an address writes
/// that address's real token balance. The synthesized surface has exactly one
/// correct shape per field, so a mismatch is refused rather than silently
/// reinterpreted.
pub fn field_type_conflict(
    span: Span,
    name: &str,
    expected: &str,
    found: &str,
    standard: &str,
) -> Diagnostic {
    Diagnostic::error(
        E609_FIELD_TYPE_CONFLICT,
        format!(
            "field `{name}` is declared `{found}` but {standard} synthesis uses it as `{expected}`"
        ),
        span,
    )
    .with_help(format!(
        "declare it as `field {name}: {expected}`, or rename your field so it does not collide with the synthesized one"
    ))
}

/// F-20 fix: `supply: N to <principal>` is lowered only for the literal
/// `deployer`. Every other principal the parser accepts (`owner`, `admin`,
/// `caller`, `holders`, `parties`, `guardians`, a literal address) reached the
/// backend, failed the `is_deployer` test and returned, so nothing was minted:
/// `totalSupply()` read 0, every `balanceOf` read 0, and the ERC-20 surface
/// carries no mint function to repair it. Minting to those principals needs
/// constructor-time principal resolution the backend does not have, so the
/// declaration is refused instead of discarded.
pub fn genesis_principal_unsupported(span: Span, principal: &str) -> Diagnostic {
    Diagnostic::error(
        E640_GENESIS_PRINCIPAL_UNSUPPORTED,
        format!(
            "`supply: N to {principal}` is not supported: only `to deployer` is minted at construction"
        ),
        span,
    )
    .with_help(
        "write `supply: N to deployer`, then move the balance in a deployer-guarded action; other principals do not exist yet when the constructor runs",
    )
}

/// F-28 fix: `supply: N to deployer` and `field total_supply: amount = M` are
/// two independent initializers for the same slot. The field default won for
/// `total_supply` while the backend went on seeding `balances[deployer]` from
/// the supply metadata, so the token deployed with `totalSupply() != sum(balances)`
/// (1000000000 against 1000 on the reported probe) and no warning at all. The
/// compiler cannot know which number the author meant, so a disagreement is
/// refused. Equal values are accepted: they say the same thing twice.
/// `default` is `None` when the field's initializer is not an integer constant
/// and therefore cannot be compared against the genesis amount at all.
pub fn supply_initializer_conflict(span: Span, supply: u128, default: Option<u128>) -> Diagnostic {
    let message = match default {
        Some(d) => format!(
            "`supply: {supply} to deployer` contradicts the `total_supply` field default `= {d}`"
        ),
        None => format!(
            "`supply: {supply} to deployer` collides with a `total_supply` field initializer that is not an integer constant"
        ),
    };
    Diagnostic::error(E641_SUPPLY_INITIALIZER_CONFLICT, message, span)
    .with_help(
        "drop one of the two: the genesis mint credits `balances[deployer]` from `supply`, so a different `total_supply` default breaks `totalSupply() == sum(balances)` at block zero",
    )
}

/// F-41 fix: EIP-20 declares `decimals()` as `uint8`. A value above 255 cannot
/// be represented in the standard's return type, so every consumer decoding
/// against the canonical ERC-20 ABI reads a different number from the one in
/// the source. W609 only said the value "is unusual (expected 0..=18)", which
/// does not name that problem and is a warning, so the contract shipped. Values
/// that cannot round-trip through `uint8` are now refused; 19..=255 stays a
/// W609 warning because it is representable, merely unusual.
pub fn decimals_unrepresentable(span: Span, value: u128) -> Diagnostic {
    Diagnostic::error(
        E642_DECIMALS_UNREPRESENTABLE,
        format!("decimals = {value} cannot be represented: EIP-20 declares `decimals()` as uint8"),
        span,
    )
    .with_help("use a value in 0..=255 (0..=18 is the conventional range)")
}

/// F-35 fix: the synthesizers skipped injecting an event or error whose name a
/// user had already declared, but never reconciled the SHAPE, and the
/// synthesized bodies kept emitting their own fixed arity. A user
/// `error InsufficientBalance(needed: amount, got: amount)` therefore shipped
/// an ABI promising two words beside a runtime that reverts with a zero-byte
/// payload, and a user `event Transfer(a: address indexed, b: address indexed)`
/// moved topic0 off the canonical ERC-20 hash so every wallet and indexer
/// missed every transfer. Reconciling the shape would mean inventing values the
/// synthesizer does not have, so a shadowing declaration whose shape differs
/// from the canonical one is refused. An exact-shape redeclaration is still
/// accepted: parameter NAMES are free, only the types and the `indexed` flags
/// have to match, which is what keeps a hand-written
/// `event Transfer(sender: address indexed, recipient: address indexed, value: amount)`
/// working.
pub fn shadowed_synth_shape(
    span: Span,
    kind: &str,
    name: &str,
    expected: &str,
    found: &str,
) -> Diagnostic {
    Diagnostic::error(
        E643_SHADOWED_SYNTH_SHAPE,
        format!("user-declared {kind} `{name}` does not match the synthesized shape `{expected}`"),
        span,
    )
    .with_help(format!(
        "declare it as `{expected}` (parameter names are free) or rename it; the synthesized body still emits the canonical shape, so `{found}` would ship an ABI the runtime does not honour"
    ))
}

pub fn warn_missing_metadata(span: Span, key: &str) -> Diagnostic {
    warn(
        W601_MISSING_METADATA,
        format!("token construct missing `{key}` metadata, using default"),
        span,
    )
}

pub fn warn_synth_not_impl(span: Span, construct: &str) -> Diagnostic {
    warn(
        W606_SYNTH_NOT_IMPL,
        format!("{construct} standard-interface synthesis not yet implemented, passing construct through unchanged"),
        span,
    )
}

pub fn warn_erc8227_unexercised(span: Span) -> Diagnostic {
    warn(
        W605_ERC8227_UNEXERCISED,
        "ERC-8227 synthesis implemented but not validated on V0 Basics, exercise via Intermediate fixtures",
        span,
    )
}

pub fn warn_empty_symbol(span: Span) -> Diagnostic {
    warn(
        W610_EMPTY_SYMBOL,
        "token `symbol` is empty: wallets may display as \"Unknown\"",
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

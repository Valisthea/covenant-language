//! Lexer diagnostic codes (E001-E010 and E060-E069, cf. Doc 5 §17.2 and Phase 1 spec).

use covenant_diag::{DiagCode, Diagnostic, Span};

pub const E001_UNEXPECTED: DiagCode = DiagCode(1);
pub const E002_UNTERMINATED_STRING: DiagCode = DiagCode(2);
pub const E003_C_LINE_COMMENT: DiagCode = DiagCode(3);
pub const E004_C_BLOCK_COMMENT: DiagCode = DiagCode(4);
pub const E005_INT_OVERFLOW: DiagCode = DiagCode(5);
pub const E006_HEX_ODD: DiagCode = DiagCode(6);
pub const E007_HEX_EMPTY: DiagCode = DiagCode(7);
pub const E008_BARE_CR: DiagCode = DiagCode(8);
pub const E009_UNTERMINATED_NESTED: DiagCode = DiagCode(9);
pub const E010_BAD_ESCAPE: DiagCode = DiagCode(10);

/// A duration literal whose value in seconds does not fit in `u64`.
///
/// The lexer normalizes `<int> <unit>` to seconds on the spot, so the count the
/// author wrote can be in range while the normalized value is not (`u64::MAX
/// weeks`). Refusing is the only honest answer: silently truncating or wrapping
/// the second count would produce a lock or cooldown that expires at an
/// arbitrary instant, which is exactly the class of miscompile the normalization
/// exists to prevent.
pub const E060_DURATION_OVERFLOW: DiagCode = DiagCode(60);

pub fn unexpected(span: Span, ch: &str) -> Diagnostic {
    Diagnostic::error(E001_UNEXPECTED, format!("unexpected character: {ch}"), span)
}

pub fn unterminated_string(span: Span) -> Diagnostic {
    Diagnostic::error(
        E002_UNTERMINATED_STRING,
        "unterminated string literal",
        span,
    )
    .with_help("close the string with `\"` on the same line")
}

pub fn c_line_comment(span: Span) -> Diagnostic {
    Diagnostic::error(
        E003_C_LINE_COMMENT,
        "C-style single-line comments are reserved; use `--` for line comments in Covenant",
        span,
    )
    .with_help("replace `//` with `--`")
}

pub fn c_block_comment(span: Span) -> Diagnostic {
    Diagnostic::error(
        E004_C_BLOCK_COMMENT,
        "C-style multi-line comments are reserved; use `(* ... *)` in Covenant",
        span,
    )
    .with_help("replace `/* ... */` with `(* ... *)`")
}

pub fn int_overflow(span: Span) -> Diagnostic {
    Diagnostic::error(E005_INT_OVERFLOW, "integer literal overflows u128", span)
}

pub fn hex_odd(span: Span) -> Diagnostic {
    Diagnostic::error(
        E006_HEX_ODD,
        "hex literal must have an even digit count",
        span,
    )
}

pub fn hex_empty(span: Span) -> Diagnostic {
    Diagnostic::error(E007_HEX_EMPTY, "empty hex literal", span)
        .with_help("hex literals need at least two digits, e.g. `0x00`")
}

pub fn bare_cr(span: Span) -> Diagnostic {
    Diagnostic::error(E008_BARE_CR, "bare CR not permitted; use LF or CR LF", span)
}

pub fn unterminated_nested(span: Span) -> Diagnostic {
    Diagnostic::error(
        E009_UNTERMINATED_NESTED,
        "unterminated `(* ... *)` comment",
        span,
    )
}

pub fn duration_overflow(span: Span, unit_secs: u64) -> Diagnostic {
    Diagnostic::error(
        E060_DURATION_OVERFLOW,
        format!("duration literal overflows u64 once converted to seconds (x{unit_secs})"),
        span,
    )
    .with_help("durations are counted in seconds; the largest representable value is 18446744073709551615 seconds")
}

pub fn bad_escape(span: Span, escape: &str) -> Diagnostic {
    Diagnostic::error(
        E010_BAD_ESCAPE,
        format!("invalid escape sequence in string literal: `{escape}`"),
        span,
    )
    .with_help("supported escapes are \\\", \\\\, \\n, \\t, and \\u{XXXX}")
}

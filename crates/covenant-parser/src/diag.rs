//! Parser diagnostic codes (E020–E099, cf. Doc 5 §17.2).
//!
//! E001–E010 are reserved by the lexer (Phase 1). The parser uses E020 upward so
//! the numeric range stays debuggable — a diagnostic numbered `E02x` is always a
//! parser error, never a lexer error.
//!
//! Not every helper here is used internally; some (`missing_closing_brace`,
//! `invalid_privacy`, `missing_body`) are reserved for later phases that
//! invoke parser helpers during REPL/LSP integration.
#![allow(dead_code)]

use covenant_diag::{DiagCode, Diagnostic, Span};

pub const E020_UNEXPECTED_TOKEN: DiagCode = DiagCode(20);
pub const E021_ANNOTATION_WRONG_DECL: DiagCode = DiagCode(21);
pub const E022_MISSING_CLOSING_BRACE: DiagCode = DiagCode(22);
pub const E023_UNEXPECTED_EOF: DiagCode = DiagCode(23);
pub const E024_MALFORMED_FIELD: DiagCode = DiagCode(24);
pub const E025_INVALID_PRIVACY: DiagCode = DiagCode(25);
pub const E026_TYPE_EXPECTED: DiagCode = DiagCode(26);
pub const E027_MISSING_BODY: DiagCode = DiagCode(27);
pub const E028_BAD_CONSTRUCT_KEYWORD: DiagCode = DiagCode(28);
pub const E029_BAD_SHARES: DiagCode = DiagCode(29);
pub const E030_BAD_STMT_TERMINATOR: DiagCode = DiagCode(30);
pub const E031_TOO_DEEPLY_NESTED: DiagCode = DiagCode(31);
pub const E032_TYPE_TOO_DEEPLY_NESTED: DiagCode = DiagCode(32);

pub fn type_too_deeply_nested(span: Span) -> Diagnostic {
    // OMEGA V6 (F06 fix): the E031 guard covers the Pratt expression parser and
    // the block parser, but `parse_type` recursed one native stack frame per
    // type-nesting level (`map<address, map<address, ...>>`, `encrypted T`,
    // `[T]`, `priority_queue<...>`) with no counter of its own -- a ~1500-deep
    // nested type overflowed the process stack (an uncatchable
    // `STATUS_STACK_OVERFLOW`, not a normal Rust panic) and crashed
    // `covenant check/build/fmt/lint` and the LSP before any diagnostic could be
    // produced. This bails out with a normal, catchable diagnostic well before
    // that point instead.
    Diagnostic::error(
        E032_TYPE_TOO_DEEPLY_NESTED,
        "type nesting exceeds the maximum supported depth",
        span,
    )
    .with_help("introduce a named type alias for the inner type to reduce nesting")
}

pub fn too_deeply_nested(span: Span) -> Diagnostic {
    // OMEGA V6 (HGH-029 fix): the Pratt expression parser and block parser
    // recursed one native stack frame per AST nesting level with no depth
    // counter -- a few hundred bytes of nested parens/brackets/blocks
    // overflowed the process stack (an uncatchable `STATUS_STACK_OVERFLOW`,
    // not a normal Rust panic) before any diagnostic could be produced.
    // This bails out with a normal, catchable diagnostic well before that
    // point instead.
    Diagnostic::error(
        E031_TOO_DEEPLY_NESTED,
        "expression or block nesting exceeds the maximum supported depth",
        span,
    )
    .with_help("split this into smaller named sub-expressions or statements")
}

pub fn unexpected_token(span: Span, what: impl Into<String>) -> Diagnostic {
    Diagnostic::error(
        E020_UNEXPECTED_TOKEN,
        format!("unexpected token: {}", what.into()),
        span,
    )
}

pub fn expected(span: Span, what: &str, got: &str) -> Diagnostic {
    Diagnostic::error(
        E020_UNEXPECTED_TOKEN,
        format!("expected {what}, got {got}"),
        span,
    )
}

pub fn annotation_wrong_decl(span: Span) -> Diagnostic {
    Diagnostic::error(
        E021_ANNOTATION_WRONG_DECL,
        "annotations are only allowed on `action` and `selective_disclosure` declarations",
        span,
    )
}

pub fn missing_closing_brace(span: Span) -> Diagnostic {
    Diagnostic::error(E022_MISSING_CLOSING_BRACE, "missing closing `}`", span)
}

pub fn unexpected_eof(span: Span, while_doing: &str) -> Diagnostic {
    Diagnostic::error(
        E023_UNEXPECTED_EOF,
        format!("unexpected end of file while {while_doing}"),
        span,
    )
}

pub fn malformed_field(span: Span, detail: impl Into<String>) -> Diagnostic {
    Diagnostic::error(
        E024_MALFORMED_FIELD,
        format!("malformed field declaration: {}", detail.into()),
        span,
    )
}

pub fn invalid_privacy(span: Span, qualifier: &str, construct: &str) -> Diagnostic {
    Diagnostic::error(
        E025_INVALID_PRIVACY,
        format!("privacy qualifier `{qualifier}` not permitted on `{construct}`"),
        span,
    )
}

pub fn type_expected(span: Span) -> Diagnostic {
    Diagnostic::error(E026_TYPE_EXPECTED, "expected a type after `:`", span)
}

pub fn missing_body(span: Span, what: &str) -> Diagnostic {
    Diagnostic::error(
        E027_MISSING_BODY,
        format!("missing body on `{what}` declaration"),
        span,
    )
}

pub fn bad_construct_keyword(span: Span, got: &str) -> Diagnostic {
    Diagnostic::error(
        E028_BAD_CONSTRUCT_KEYWORD,
        format!("expected a top-level construct keyword (record, token, ...), got {got}"),
        span,
    )
}

pub fn bad_shares(span: Span, detail: &str) -> Diagnostic {
    Diagnostic::error(
        E029_BAD_SHARES,
        format!("malformed `shares(...)` type: {detail}"),
        span,
    )
}

pub fn bad_stmt_terminator(span: Span, got: &str) -> Diagnostic {
    Diagnostic::error(
        E030_BAD_STMT_TERMINATOR,
        format!("expected statement terminator (newline, `;`, or `}}`), got {got}"),
        span,
    )
}

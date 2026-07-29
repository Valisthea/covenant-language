//! Type-checker diagnostic codes (E201-E230 per Doc 5 §17.2).
#![allow(dead_code)]

use covenant_diag::{DiagCode, Diagnostic, DiagnosticLevel, Span};

pub const E201_TYPE_MISMATCH: DiagCode = DiagCode(201);
pub const E202_OP_INAPPLICABLE: DiagCode = DiagCode(202);
pub const E203_UNKNOWN_METHOD: DiagCode = DiagCode(203);
pub const E204_AMBIGUOUS_OVERLOAD: DiagCode = DiagCode(204);
pub const E205_ARITY_MISMATCH: DiagCode = DiagCode(205);
pub const E206_NOT_INDEXABLE: DiagCode = DiagCode(206);
pub const E207_NOT_FIELD_ACCESSIBLE: DiagCode = DiagCode(207);
pub const E208_LAMBDA_NEEDS_CONTEXT: DiagCode = DiagCode(208);
pub const E209_INVALID_TYPE_ANNOTATION: DiagCode = DiagCode(209);
pub const E210_GENERIC_MISUSE: DiagCode = DiagCode(210);
pub const E211_DOUBLE_ENCRYPTION: DiagCode = DiagCode(211);
pub const E212_REVEAL_NOT_CIPHERTEXT: DiagCode = DiagCode(212);
pub const E213_REVERT_ARG_MISMATCH: DiagCode = DiagCode(213);
pub const E214_EMIT_ARG_MISMATCH: DiagCode = DiagCode(214);
pub const E215_PATTERN_TYPE_MISMATCH: DiagCode = DiagCode(215);
pub const E216_MATCH_NOT_EXHAUSTIVE: DiagCode = DiagCode(216);
pub const E217_OPERATOR_ARITY: DiagCode = DiagCode(217);
pub const E218_TEST_INTRINSIC: DiagCode = DiagCode(218);
pub const E219_REVEAL_TARGET_FIELD: DiagCode = DiagCode(219);
pub const E220_RETURN_IN_ACTION: DiagCode = DiagCode(220);
pub const E221_DESTRUCTIBLE_EXPECTED: DiagCode = DiagCode(221);
pub const E222_NOMINAL_MISMATCH: DiagCode = DiagCode(222);
pub const E223_CHOICE_NOT_MEMBER: DiagCode = DiagCode(223);
pub const E224_SHARES_MISMATCH: DiagCode = DiagCode(224);
pub const E225_LIFT_BLOCKED: DiagCode = DiagCode(225);
pub const E226_INFERENCE_FAILURE: DiagCode = DiagCode(226);
pub const E227_LAMBDA_MISPLACED: DiagCode = DiagCode(227);
pub const E228_FOREACH_NOT_LIST: DiagCode = DiagCode(228);
pub const E229_APPEND_NOT_LIST: DiagCode = DiagCode(229);
pub const E230_EMPTY_ARRAY_NO_CONTEXT: DiagCode = DiagCode(230);
pub const E231_NOT_A_TYPE: DiagCode = DiagCode(231);
pub const E232_TOO_DEEPLY_NESTED: DiagCode = DiagCode(232);

/// An `append <list> { ... }` literal names a field the element struct does not
/// declare. Refusing is the only faithful option: the IR builds the element's
/// operands by walking the struct's DECLARED field order and looking each
/// declared name up in the literal, so a literal entry whose name matches no
/// declared field is never lowered at all. The value the author wrote is
/// silently discarded and the element is built as if the entry were absent.
/// There is no correct lowering to pick, so the mistake is reported at the
/// point it is made.
pub const E240_APPEND_UNKNOWN_FIELD: DiagCode = DiagCode(240);

pub const W303_TEST_INTRINSIC: DiagCode = DiagCode(303);
pub const W304_REVEAL_ON_PLAINTEXT: DiagCode = DiagCode(304);
pub const W305_MATCH_NOT_EXHAUSTIVE: DiagCode = DiagCode(305);

fn warn(code: DiagCode, message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic {
        level: DiagnosticLevel::Warning,
        code,
        message: message.into(),
        span,
        help: None,
    }
}

pub fn type_mismatch(span: Span, expected: &str, actual: &str) -> Diagnostic {
    Diagnostic::error(
        E201_TYPE_MISMATCH,
        format!("type mismatch: expected `{expected}`, found `{actual}`"),
        span,
    )
}

pub fn op_inapplicable(span: Span, op: &str, lhs: &str, rhs: &str) -> Diagnostic {
    Diagnostic::error(
        E202_OP_INAPPLICABLE,
        format!("operator `{op}` is not applicable to `{lhs}` and `{rhs}`"),
        span,
    )
}

pub fn op_inapplicable_unary(span: Span, op: &str, operand: &str) -> Diagnostic {
    Diagnostic::error(
        E202_OP_INAPPLICABLE,
        format!("unary operator `{op}` is not applicable to `{operand}`"),
        span,
    )
}

pub fn unknown_method(span: Span, module: &str, method: &str) -> Diagnostic {
    Diagnostic::error(
        E203_UNKNOWN_METHOD,
        format!("no method `{method}` on stdlib module `{module}`"),
        span,
    )
}

pub fn arity_mismatch(span: Span, fn_name: &str, expected: usize, actual: usize) -> Diagnostic {
    Diagnostic::error(
        E205_ARITY_MISMATCH,
        format!("`{fn_name}` expects {expected} argument(s), got {actual}"),
        span,
    )
}

pub fn not_indexable(span: Span, ty: &str) -> Diagnostic {
    Diagnostic::error(
        E206_NOT_INDEXABLE,
        format!("value of type `{ty}` is not indexable"),
        span,
    )
}

pub fn not_field_accessible(span: Span, ty: &str, field: &str) -> Diagnostic {
    Diagnostic::error(
        E207_NOT_FIELD_ACCESSIBLE,
        format!("value of type `{ty}` has no field `{field}`"),
        span,
    )
}

pub fn lambda_needs_context(span: Span) -> Diagnostic {
    Diagnostic::error(
        E208_LAMBDA_NEEDS_CONTEXT,
        "cannot infer lambda type without a contextual expected type",
        span,
    )
    .with_help(
        "use `.map(e => ...)` / `.filter(e => ...)` at a call site so the expected type is known",
    )
}

pub fn double_encryption(span: Span) -> Diagnostic {
    Diagnostic::error(
        E211_DOUBLE_ENCRYPTION,
        "cannot encrypt an already-encrypted value",
        span,
    )
}

pub fn revert_arg_mismatch(span: Span, err: &str, expected: usize, actual: usize) -> Diagnostic {
    Diagnostic::error(
        E213_REVERT_ARG_MISMATCH,
        format!("error `{err}` expects {expected} field(s), got {actual}"),
        span,
    )
}

pub fn emit_arg_mismatch(span: Span, event: &str, expected: usize, actual: usize) -> Diagnostic {
    Diagnostic::error(
        E214_EMIT_ARG_MISMATCH,
        format!("event `{event}` expects {expected} argument(s), got {actual}"),
        span,
    )
}

pub fn foreach_not_list(span: Span, ty: &str) -> Diagnostic {
    Diagnostic::error(
        E228_FOREACH_NOT_LIST,
        format!("`for each` iterator must be a list; found `{ty}`"),
        span,
    )
}

pub fn append_not_list(span: Span, detail: &str) -> Diagnostic {
    Diagnostic::error(
        E229_APPEND_NOT_LIST,
        format!("`append` target must be a list of structs; {detail}"),
        span,
    )
}

pub fn append_unknown_field(span: Span, struct_name: &str, field: &str) -> Diagnostic {
    Diagnostic::error(
        E240_APPEND_UNKNOWN_FIELD,
        format!("struct `{struct_name}` has no field `{field}`"),
        span,
    )
    .with_help("remove the entry, or add the field to the struct declaration")
}

pub fn empty_array_no_context(span: Span) -> Diagnostic {
    Diagnostic::error(
        E230_EMPTY_ARRAY_NO_CONTEXT,
        "empty array literal needs a type annotation",
        span,
    )
    .with_help("e.g. `let xs: [amount] = []`")
}

pub fn not_a_type(span: Span, name: &str, kind: &str) -> Diagnostic {
    // OMEGA V6 (HGH-026 fix): `lower_type`'s `AstType::User(ident)` arm used
    // to fall back to `Ty::Unknown` with zero diagnostic whenever `ident`
    // resolved to something other than a struct/credential (e.g. a field,
    // action, event, or error sharing the same flat top-level namespace).
    // `Ty::Unknown` is permissive downstream (privacy analysis, codegen)
    // rather than a hard stop, so a typo'd or misused type reference could
    // silently compile into a field/param of unchecked type instead of
    // failing at the point of the mistake.
    Diagnostic::error(
        E231_NOT_A_TYPE,
        format!("`{name}` is not a type -- it refers to {kind}"),
        span,
    )
}

pub fn too_deeply_nested(span: Span) -> Diagnostic {
    // OMEGA V6 (HGH-029 fix): `synth_expr`/`check_expr` recursed one native
    // stack frame per AST nesting level with no depth counter. Bails out
    // with a normal diagnostic before that overflows the process stack.
    Diagnostic::error(
        E232_TOO_DEEPLY_NESTED,
        "expression nesting exceeds the maximum depth the type checker can walk",
        span,
    )
}

pub fn return_in_action(span: Span) -> Diagnostic {
    Diagnostic::error(
        E220_RETURN_IN_ACTION,
        "`return` is not permitted in action bodies",
        span,
    )
}

pub fn warn_reveal_plaintext(span: Span) -> Diagnostic {
    warn(
        W304_REVEAL_ON_PLAINTEXT,
        "revealing an already-plaintext field has no effect",
        span,
    )
}

pub fn warn_match_not_exhaustive(span: Span, missing: &[Box<str>]) -> Diagnostic {
    let names = missing
        .iter()
        .map(|s| format!("`{}`", s.as_ref()))
        .collect::<Vec<_>>()
        .join(", ");
    warn(
        W305_MATCH_NOT_EXHAUSTIVE,
        format!("match is not exhaustive; missing {names}"),
        span,
    )
}

pub fn warn_test_intrinsic(span: Span, name: &str) -> Diagnostic {
    warn(
        W303_TEST_INTRINSIC,
        format!("`{name}` is a test-only intrinsic; will be rejected in production builds"),
        span,
    )
}

pub fn match_pattern_mismatch(span: Span, expected: &str, actual: &str) -> Diagnostic {
    Diagnostic::error(
        E215_PATTERN_TYPE_MISMATCH,
        format!("match pattern type mismatch: expected `{expected}`, found `{actual}`"),
        span,
    )
}

pub fn choice_not_member(span: Span, value: &str) -> Diagnostic {
    Diagnostic::error(
        E223_CHOICE_NOT_MEMBER,
        format!("`{value}` is not a member of this choice type"),
        span,
    )
}

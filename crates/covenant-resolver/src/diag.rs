//! Resolver diagnostic codes (E101–E199 per Doc 5 §17.2).
//!
//! Not every helper is used internally — some are reserved for future passes
//! that will reuse the same code numbering.
#![allow(dead_code)]

use covenant_diag::{DiagCode, Diagnostic, DiagnosticLevel, Span};

pub const E101_DUPLICATE_DECL: DiagCode = DiagCode(101);
pub const E102_UNRESOLVED_IDENT: DiagCode = DiagCode(102);
pub const E103_PROOF_PAYLOAD_CONTEXT: DiagCode = DiagCode(103);
pub const E104_DESTROY_CONTEXT: DiagCode = DiagCode(104);
pub const E105_PRINCIPAL_FIELD_MISSING: DiagCode = DiagCode(105);
pub const E106_UNKNOWN_PREDICATE: DiagCode = DiagCode(106);
pub const E107_USER_IMPORTS: DiagCode = DiagCode(107);
pub const E108_UNKNOWN_EVENT: DiagCode = DiagCode(108);
pub const E109_UNKNOWN_ERROR: DiagCode = DiagCode(109);
pub const E110_UNKNOWN_ANNOTATION: DiagCode = DiagCode(110);
pub const E111_PHASE_CONTEXT: DiagCode = DiagCode(111);
pub const E112_THIS_CHAIN_CONTEXT: DiagCode = DiagCode(112);
pub const E113_TOO_DEEPLY_NESTED: DiagCode = DiagCode(113);

pub const W201_SHADOWING: DiagCode = DiagCode(201);
pub const W202_SHADOWING_LANG: DiagCode = DiagCode(202);
pub const W301_REDUNDANT_USING: DiagCode = DiagCode(301);

fn warning(code: DiagCode, message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic {
        level: DiagnosticLevel::Warning,
        code,
        message: message.into(),
        span,
        help: None,
    }
}

pub fn duplicate_decl(span: Span, name: &str) -> Diagnostic {
    Diagnostic::error(
        E101_DUPLICATE_DECL,
        format!("duplicate declaration `{name}`"),
        span,
    )
}

pub fn unresolved(span: Span, name: &str, suggestion: Option<&str>) -> Diagnostic {
    let base = Diagnostic::error(
        E102_UNRESOLVED_IDENT,
        format!("unresolved identifier `{name}`"),
        span,
    );
    match suggestion {
        Some(s) => base.with_help(format!("did you mean `{s}`?")),
        None => base,
    }
}

pub fn proof_payload_context(span: Span) -> Diagnostic {
    Diagnostic::error(
        E103_PROOF_PAYLOAD_CONTEXT,
        "`proof_payload` is only available inside actions qualified with `verified_by(...)`",
        span,
    )
}

pub fn destroy_context(span: Span, fn_name: &str) -> Diagnostic {
    Diagnostic::error(
        E104_DESTROY_CONTEXT,
        format!("`{fn_name}` may only be called inside an `on_destroy` block"),
        span,
    )
}

pub fn principal_field_missing(span: Span, principal: &str) -> Diagnostic {
    Diagnostic::error(
        E105_PRINCIPAL_FIELD_MISSING,
        format!(
            "principal `{principal}` requires a field named `{principal}` of type \
             `address` in the enclosing construct"
        ),
        span,
    )
}

pub fn unknown_predicate(span: Span, name: &str, suggestion: Option<&str>) -> Diagnostic {
    let base = Diagnostic::error(
        E106_UNKNOWN_PREDICATE,
        format!("unknown principal predicate `{name}`"),
        span,
    );
    match suggestion {
        Some(s) => base.with_help(format!("did you mean `{s}`?")),
        None => base.with_help(
            "built-in predicates include `first_time_caller`, `registered_key`, \
             `known_issuer`, `validator_majority(...)`",
        ),
    }
}

pub fn user_imports_unsupported(span: Span, module: &str) -> Diagnostic {
    Diagnostic::error(
        E107_USER_IMPORTS,
        format!("user module imports are not yet supported in V0 (`{module}`)"),
        span,
    )
}

pub fn unknown_event(span: Span, name: &str, suggestion: Option<&str>) -> Diagnostic {
    let base = Diagnostic::error(
        E108_UNKNOWN_EVENT,
        format!("event `{name}` is not declared in this construct"),
        span,
    );
    match suggestion {
        Some(s) => base.with_help(format!("did you mean `{s}`?")),
        None => base,
    }
}

pub fn unknown_error(span: Span, name: &str, suggestion: Option<&str>) -> Diagnostic {
    let base = Diagnostic::error(
        E109_UNKNOWN_ERROR,
        format!("error type `{name}` is not declared in this construct"),
        span,
    );
    match suggestion {
        Some(s) => base.with_help(format!("did you mean `{s}`?")),
        None => base,
    }
}

pub fn unknown_annotation(span: Span, name: &str) -> Diagnostic {
    Diagnostic::error(
        E110_UNKNOWN_ANNOTATION,
        format!("unknown annotation `@{name}`"),
        span,
    )
    .with_help("valid annotations: `@precompute`, `@batch_up_to`, `@prove_offchain`, `@gas_budget`")
}

pub fn phase_context(span: Span) -> Diagnostic {
    Diagnostic::error(
        E111_PHASE_CONTEXT,
        "`phase` is only available inside `ceremony` constructs",
        span,
    )
}

pub fn this_chain_context(span: Span, ident: &str) -> Diagnostic {
    Diagnostic::error(
        E112_THIS_CHAIN_CONTEXT,
        format!("`{ident}` is only available inside `bridge` constructs"),
        span,
    )
}

pub fn too_deeply_nested(span: Span) -> Diagnostic {
    // OMEGA V6 (HGH-029 fix): `resolve_expr` recursed one native stack frame
    // per AST nesting level with no depth counter -- a long chain of binary
    // operators (each parses into a left-leaning `Binary` tree, walked
    // recursively down the left spine) overflowed the process stack. This
    // bails out with a normal diagnostic before that point.
    Diagnostic::error(
        E113_TOO_DEEPLY_NESTED,
        "expression nesting exceeds the maximum depth the resolver can walk",
        span,
    )
}

pub fn shadowing(span: Span, name: &str, outer_span: Span) -> Diagnostic {
    let _ = outer_span;
    warning(
        W201_SHADOWING,
        format!("binding `{name}` shadows an outer binding"),
        span,
    )
}

pub fn shadowing_lang(span: Span, name: &str) -> Diagnostic {
    warning(
        W202_SHADOWING_LANG,
        format!("binding `{name}` shadows a language-provided identifier"),
        span,
    )
}

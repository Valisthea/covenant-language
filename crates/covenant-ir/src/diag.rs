//! IR builder diagnostic codes (E401–E420).
#![allow(dead_code)]

use covenant_diag::{DiagCode, Diagnostic, DiagnosticLevel, Span};

pub const E401_SSA_DOMINANCE: DiagCode = DiagCode(401);
pub const E402_BLOCK_NO_TERMINATOR: DiagCode = DiagCode(402);
pub const E403_USER_CALL: DiagCode = DiagCode(403);
pub const E404_LAMBDA_UNSUPPORTED: DiagCode = DiagCode(404);
pub const E405_BLOCK_ARG_MISMATCH: DiagCode = DiagCode(405);
pub const E406_UNKNOWN_EVENT: DiagCode = DiagCode(406);
pub const E407_UNKNOWN_ERROR: DiagCode = DiagCode(407);
pub const E408_FOREACH_NOT_LIST: DiagCode = DiagCode(408);
pub const E409_UNEXPECTED_STMT: DiagCode = DiagCode(409);
pub const E410_OPCODE_ARITY: DiagCode = DiagCode(410);
pub const E411_CIPHERTEXT_TO_PLAINTEXT: DiagCode = DiagCode(411);
pub const E412_UNLOWERABLE_EXPR: DiagCode = DiagCode(412);
pub const E413_UNKNOWN_STRUCT_FIELD: DiagCode = DiagCode(413);
pub const E414_MAP_ON_NON_MAP: DiagCode = DiagCode(414);
pub const E415_MISSING_STDLIB_CALL: DiagCode = DiagCode(415);
pub const E416_SELECTIVE_DISCLOSURE_DEFERRED: DiagCode = DiagCode(416);
pub const E417_ONDESTROY_UNKNOWN_FIELD: DiagCode = DiagCode(417);
pub const E418_MIGRATE_UNKNOWN_FIELD: DiagCode = DiagCode(418);
pub const E419_FHEBRANCH_PHI_MISSING: DiagCode = DiagCode(419);
pub const E420_FHEBRANCH_NO_MERGE: DiagCode = DiagCode(420);
pub const E421_GUARD_UNRESOLVED_PRINCIPAL: DiagCode = DiagCode(421);
pub const E422_SLOT_ANNOTATION_INVALID: DiagCode = DiagCode(422);
pub const E423_SLOT_ANNOTATION_CONFLICT: DiagCode = DiagCode(423);
/// A stdlib math builtin has no real lowering yet. These previously mapped to
/// `Opcode::AddChecked` as a placeholder, so `max(cap, bid)` silently compiled
/// to `cap + bid` — correct-looking source, wrong bytecode, no diagnostic.
/// Failing at compile time is the only safe behaviour until the multi-block
/// lowering (compare + branch) exists.
pub const E424_STDLIB_MATH_UNIMPLEMENTED: DiagCode = DiagCode(424);
/// `map.length` / `map.keys` / `map.values` have no lowering. Covenant maps
/// are bare `keccak(key ‖ slot)` mappings with no companion length word and
/// no key array, so cardinality and enumeration are not representable. The
/// backend answered all three with `PUSH0`, so `.length` read 0 and
/// `for each k in m.keys` ran zero iterations on clean-compiling source.
pub const E425_MAP_INTROSPECTION_UNIMPLEMENTED: DiagCode = DiagCode(425);
/// KSR-CVN-030: an annotation name is not in the canonical set. Warning,
/// not error, because user-defined metadata annotations are legitimate —
/// but a typo on a security-relevant name like `@non_reentrant` would
/// otherwise silently downgrade the action to an unguarded no-op.
pub const W850_UNKNOWN_ANNOTATION: DiagCode = DiagCode(850);

pub fn ssa_dominance(span: Span, value: &str) -> Diagnostic {
    Diagnostic::error(
        E401_SSA_DOMINANCE,
        format!("SSA dominance violation: value `{value}` used before defined"),
        span,
    )
}

pub fn block_no_terminator(span: Span) -> Diagnostic {
    Diagnostic::error(E402_BLOCK_NO_TERMINATOR, "block has no terminator", span)
}

pub fn user_call(span: Span) -> Diagnostic {
    Diagnostic::error(
        E403_USER_CALL,
        "user-defined function calls are not supported at the IR level in V0",
        span,
    )
}

pub fn lambda_unsupported(span: Span) -> Diagnostic {
    Diagnostic::error(
        E404_LAMBDA_UNSUPPORTED,
        "lambda in unsupported context",
        span,
    )
}

pub fn block_arg_mismatch(span: Span, expected: usize, actual: usize) -> Diagnostic {
    Diagnostic::error(
        E405_BLOCK_ARG_MISMATCH,
        format!("block argument count mismatch: expected {expected}, got {actual}"),
        span,
    )
}

pub fn unknown_event(span: Span, name: &str) -> Diagnostic {
    Diagnostic::error(
        E406_UNKNOWN_EVENT,
        format!("event `{name}` not declared in this construct"),
        span,
    )
}

pub fn unknown_error(span: Span, name: &str) -> Diagnostic {
    Diagnostic::error(
        E407_UNKNOWN_ERROR,
        format!("error type `{name}` not declared in this construct"),
        span,
    )
}

pub fn foreach_not_list(span: Span) -> Diagnostic {
    Diagnostic::error(
        E408_FOREACH_NOT_LIST,
        "`for each` iterator must be a list",
        span,
    )
}

pub fn unexpected_stmt(span: Span, what: &str) -> Diagnostic {
    Diagnostic::error(
        E409_UNEXPECTED_STMT,
        format!("unexpected statement at IR level: {what}"),
        span,
    )
}

pub fn opcode_arity(span: Span, name: &str, expected: usize, actual: usize) -> Diagnostic {
    Diagnostic::error(
        E410_OPCODE_ARITY,
        format!("opcode `{name}` expects {expected} operand(s), got {actual}"),
        span,
    )
}

pub fn unlowerable_expr(span: Span) -> Diagnostic {
    Diagnostic::error(E412_UNLOWERABLE_EXPR, "unlowerable expression", span)
}

/// `min` / `max` / `abs` / `pow` / `sqrt` have no real lowering. Refuse to
/// compile rather than emit an addition and let it reach a chain.
pub fn stdlib_math_unimplemented(span: Span, name: &str) -> Diagnostic {
    Diagnostic::error(
        E424_STDLIB_MATH_UNIMPLEMENTED,
        format!(
            "`{name}` has no lowering yet and cannot be compiled. It previously \
             compiled to an addition, which silently produced wrong results. \
             Write the comparison explicitly for now — e.g. `if a > b {{ a }} else {{ b }}` \
             instead of `max(a, b)`."
        ),
        span,
    )
}

/// `map.length` / `.keys` / `.values` cannot be lowered. Refuse rather than
/// answer 0, which is indistinguishable from a genuinely empty map.
pub fn map_introspection_unimplemented(span: Span, member: &str) -> Diagnostic {
    Diagnostic::error(
        E425_MAP_INTROSPECTION_UNIMPLEMENTED,
        format!(
            "`map.{member}` has no lowering and cannot be compiled. It previously \
             compiled to a constant 0 — `.length` always read 0 and \
             `for each k in m.keys` always ran zero iterations, with no diagnostic. \
             Covenant maps carry no length word and no key array. Track size and \
             membership explicitly for now: a `count: amount` field bumped alongside \
             each write, and a list field for iteration."
        ),
        span,
    )
}

pub fn selective_disclosure_deferred(span: Span) -> Diagnostic {
    Diagnostic::error(
        E416_SELECTIVE_DISCLOSURE_DEFERRED,
        "selective_disclosure lowering is deferred to V0.2",
        span,
    )
}

/// KSR-CVN-011: an `only(principal)` guard cannot be lowered to runtime
/// authorization bytecode. The action is being compiled to fail closed
/// (always reverts) so it is not silently open.
pub fn guard_unresolved(span: Span, principal: &crate::function::IrPrincipal) -> Diagnostic {
    let what = match principal {
        crate::function::IrPrincipal::Owner(None) => "`only owner` (no `field owner` declared)",
        crate::function::IrPrincipal::Admin(None) => "`only admin` (no `field admin` declared)",
        crate::function::IrPrincipal::Parties(_) => {
            "`only parties` (collection-typed principal not yet codegenned)"
        }
        crate::function::IrPrincipal::Guardians(_) => {
            "`only guardians` (collection-typed principal not yet codegenned)"
        }
        crate::function::IrPrincipal::Holders => {
            "`only holders` (collection-typed principal not yet codegenned)"
        }
        crate::function::IrPrincipal::Unresolved => "`only` with unresolved principal",
        _ => "`only` guard",
    };
    Diagnostic {
        level: DiagnosticLevel::Warning,
        code: E421_GUARD_UNRESOLVED_PRINCIPAL,
        message: format!(
            "{what} cannot be enforced at runtime; \
             the action will revert on every call (KSR-CVN-011 fail-closed)"
        ),
        span,
        help: None,
    }
}

pub fn slot_annotation_invalid(span: Span, reason: &str) -> Diagnostic {
    Diagnostic::error(
        E422_SLOT_ANNOTATION_INVALID,
        format!("`@slot(...)` annotation is invalid: {reason}"),
        span,
    )
}

pub fn slot_annotation_conflict(span: Span, slot: u32, other: &str) -> Diagnostic {
    Diagnostic::error(
        E423_SLOT_ANNOTATION_CONFLICT,
        format!(
            "`@slot({slot})` conflicts with field `{other}` which is already assigned to the same slot"
        ),
        span,
    )
}

/// KSR-CVN-030: unknown annotation name. `suggestion` is the nearest known
/// name within Levenshtein distance 2, if any.
pub fn unknown_annotation(span: Span, name: &str, suggestion: Option<&str>) -> Diagnostic {
    let help = suggestion.map(|s| format!("did you mean `@{s}`?"));
    Diagnostic {
        level: DiagnosticLevel::Warning,
        code: W850_UNKNOWN_ANNOTATION,
        message: format!(
            "unknown annotation `@{name}`; security-relevant annotations like \
             `@non_reentrant` are silently ignored when misspelled"
        ),
        span,
        help,
    }
}

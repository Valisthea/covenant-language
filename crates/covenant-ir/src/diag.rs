//! IR builder diagnostic codes (E401-E420).
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
/// to `cap + bid`: correct-looking source, wrong bytecode, no diagnostic.
/// Failing at compile time is the only safe behaviour until the multi-block
/// lowering (compare + branch) exists.
pub const E424_STDLIB_MATH_UNIMPLEMENTED: DiagCode = DiagCode(424);
/// `map.length` / `map.keys` / `map.values` have no lowering. Covenant maps
/// are bare `keccak(key ‖ slot)` mappings with no companion length word and
/// no key array, so cardinality and enumeration are not representable. The
/// backend answered all three with `PUSH0`, so `.length` read 0 and
/// `for each k in m.keys` ran zero iterations on clean-compiling source.
pub const E425_MAP_INTROSPECTION_UNIMPLEMENTED: DiagCode = DiagCode(425);
/// The `in` membership operator has no lowering. `given x in list` previously
/// fell through `choose_binop` to `Opcode::Eq`, so `x in [a, b, c]` compiled to
/// a single scalar `x == a`: the guard passed only when `x` equalled the FIRST
/// element and silently rejected every other legitimate member (or, read the
/// other way, a membership guard enforced almost nothing it claimed to). A real
/// lowering is a `ListContains` loop (compare + branch over each element), which
/// the single-opcode `choose_binop` cannot express. Refuse until it exists.
pub const E426_MEMBERSHIP_IN_UNIMPLEMENTED: DiagCode = DiagCode(426);
/// `map.argmax` / `map.argmin` have no lowering. They fell through the map
/// `FieldAccess` arm to `Opcode::StructGet(0)`, so the reduction never iterated:
/// it read field 0 of the map handle and returned a constant, never the key with
/// the maximum/minimum value. Clean-compiling source, wrong key, no diagnostic,
/// the same silent-miscompile class as E424/E425. A Covenant map carries no key
/// array to iterate, so there is nothing correct to emit. List `.argmax` /
/// `.argmin` still lower (`ListArgMax` / `ListArgMin`); this is map-only.
pub const E427_MAP_ARG_REDUCTION_UNIMPLEMENTED: DiagCode = DiagCode(427);
/// `append <collection> { .. }` where `collection` has no storage field. The
/// persistence path (ListAppend + SStore of the new length) lives inside an
/// `if let Some(field)`, so an unbacked collection skipped it entirely: the
/// append executed, reported success and wrote nothing. A `board`'s `posts` is
/// exactly this shape (no construct synthesizes it), which made the one
/// operation the construct exists for a silent no-op.
pub const E430_APPEND_UNBACKED_COLLECTION: DiagCode = DiagCode(430);
/// A construct-implicit collection (`posts` on a `board`, `tally` on a
/// `ballot`) read as a value. Nothing allocates a storage field for these, so
/// `lower_lang_ident` answered with the integer 0 and the backend then treated
/// that 0 as a list handle: `posts.length` read 0 forever and `posts[i].<field>`
/// SLOADed storage slot 0, disclosing the construct's FIRST DECLARED FIELD
/// verbatim for every index. Refusing is the only honest answer until the
/// collection is actually allocated.
pub const E431_IMPLICIT_COLLECTION_UNBACKED: DiagCode = DiagCode(431);
/// `match` used as an expression has no lowering. It evaluated the scrutinee
/// for its side effects and then produced the constant 0, so `n = match n { .. }`
/// did not merely fail to update `n`, it destroyed the value already there.
/// Unlike the statement form (which now lowers to a real comparison chain), the
/// expression form has no answer for a scrutinee that matches no arm: the
/// grammar has no wildcard pattern, so the default value cannot be written down.
pub const E432_MATCH_EXPR_UNIMPLEMENTED: DiagCode = DiagCode(432);
/// `try_action { .. } catch _ { .. }` has no lowering. The builder inlined the
/// try body into the current block and dropped the catch body, so a failure
/// inside the body reverted the whole transaction and the catch never ran.
/// Trapping a revert on the EVM requires an external CALL boundary and a
/// returndata check; there is no `TryCall` terminator anywhere in the IR.
pub const E433_TRY_CATCH_UNIMPLEMENTED: DiagCode = DiagCode(433);
/// A non-empty list literal (`xs = [10, 20, 30]`) has no lowering. It compiled
/// to a placeholder `StructNew`, whose backend arm emits a single `PUSH0`, so
/// the enclosing assignment stored one zero into the field's length word and
/// none of the elements was written anywhere.
pub const E434_LIST_LITERAL_UNIMPLEMENTED: DiagCode = DiagCode(434);
/// `delete <target>` on a shape with no zeroing lowering. `delete` shared an
/// empty match arm with `discard`, so a revocation action compiled to an empty
/// function that still shipped in the ABI and reported success. The supported
/// shapes now emit a real write; everything else must refuse rather than go
/// back to reporting success for a revocation that did not happen.
pub const E435_DELETE_UNSUPPORTED_TARGET: DiagCode = DiagCode(435);
/// An `only <principal>` clause whose principal is not an address. The parser
/// routes any non-keyword token to `Principal::Address(expr)` and the guard
/// lowered `caller == <that value>` verbatim, so `only "owner"` became
/// `caller == 0` and `only 42` became `caller == 42`. Named principals resolved
/// by name only, so `field owner: map<address, bool>` compared the caller with
/// the map's (never-written) base slot. Every one of these compiled clean and
/// produced an action that reverts for every possible caller forever.
pub const E436_PRINCIPAL_NOT_ADDRESS: DiagCode = DiagCode(436);
/// `match` on an encrypted scrutinee. The statement form lowers to a plaintext
/// compare-and-branch chain, which cannot be applied to a ciphertext handle:
/// the branch would test the handle, not the value it hides. `encrypted_when`
/// is the construct that carries encrypted control flow.
pub const E437_MATCH_ENCRYPTED_SCRUTINEE: DiagCode = DiagCode(437);
/// KSR-CVN-030: an annotation name is not in the canonical set. Warning,
/// not error, because user-defined metadata annotations are legitimate,
/// but a typo on a security-relevant name like `@non_reentrant` would
/// otherwise silently downgrade the action to an unguarded no-op.
pub const W850_UNKNOWN_ANNOTATION: DiagCode = DiagCode(850);
/// `given <cond>` is compiled as a PRECONDITION, asserted before the body runs,
/// and is byte-identical to `when <cond>`. The guide shipped in this tree
/// describes it as a postcondition ("checked after the body executes"), so an
/// author writing a conservation invariant gets the opposite of what they read.
/// Warned, not refused: the check is real and enforced, it is just early, and
/// the language has no postcondition construct to redirect them to. Only
/// emitted when the two readings actually diverge, that is when the guard reads
/// a field the body writes.
///
/// Numbered 440, not 430: the rendered prefix comes from the diagnostic LEVEL,
/// not from the constant's name (E421 renders as `W421` because it is a
/// warning), so reusing 430 here would make `E430` and `W430` the same code and
/// send `covenant explain` to the wrong entry.
pub const W440_GIVEN_IS_PRECONDITION: DiagCode = DiagCode(440);

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
             Write the comparison explicitly for now: e.g. `if a > b {{ a }} else {{ b }}` \
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
             compiled to a constant 0: `.length` always read 0 and \
             `for each k in m.keys` always ran zero iterations, with no diagnostic. \
             Covenant maps carry no length word and no key array. Track size and \
             membership explicitly for now: a `count: amount` field bumped alongside \
             each write, and a list field for iteration."
        ),
        span,
    )
}

/// The `in` membership operator cannot be lowered. Refuse rather than emit a
/// scalar `Eq` against the first element, which silently mis-enforces the guard.
pub fn membership_in_unimplemented(span: Span) -> Diagnostic {
    Diagnostic::error(
        E426_MEMBERSHIP_IN_UNIMPLEMENTED,
        "the `in` membership operator has no lowering and cannot be compiled. It \
         previously compiled to a scalar equality against the FIRST element only, \
         so `x in list` silently passed for the first element and rejected every \
         other member, with no diagnostic. A real lowering is a `ListContains` \
         loop, which does not exist yet. Test membership explicitly for now, \
         e.g. `given x == a or x == b or x == c`: instead of `given x in list`."
            .to_string(),
        span,
    )
}

/// `map.argmax` / `map.argmin` cannot be lowered. Refuse rather than answer
/// `StructGet(0)`, which never iterates and returns a constant.
pub fn map_arg_reduction_unimplemented(span: Span, member: &str) -> Diagnostic {
    Diagnostic::error(
        E427_MAP_ARG_REDUCTION_UNIMPLEMENTED,
        format!(
            "`map.{member}` has no lowering and cannot be compiled. It previously \
             compiled to `StructGet(0)`: the reduction never iterated and returned \
             a constant instead of the key with the {} value, with no diagnostic. \
             Covenant maps carry no key array to iterate. List `.{member}` still \
             works; track the winning key explicitly for a map (e.g. update a \
             `leader` field alongside each write) until an enumerable-map \
             convention exists.",
            if member == "argmax" {
                "maximum"
            } else {
                "minimum"
            }
        ),
        span,
    )
}

/// `append` into a collection with no storage field. Refuse rather than build
/// the element value and drop it, which reported success and wrote nothing.
pub fn append_unbacked_collection(span: Span, collection: &str) -> Diagnostic {
    Diagnostic::error(
        E430_APPEND_UNBACKED_COLLECTION,
        format!(
            "`append {collection} {{ .. }}` cannot be compiled: `{collection}` has no storage \
             field, so there is nowhere to write the element. It previously built the element \
             and discarded it, so the append succeeded on chain and stored nothing. Declare the \
             collection as a real field (e.g. `{collection}s: [Entry] = []` on a `record`, with a \
             `struct Entry {{ .. }}`) and append into that."
        ),
        span,
    )
}

/// A construct-implicit collection used as a value. Refuse rather than answer
/// with the integer 0, which the backend reads as a handle onto storage slot 0.
pub fn implicit_collection_unbacked(span: Span, name: &str) -> Diagnostic {
    Diagnostic::error(
        E431_IMPLICIT_COLLECTION_UNBACKED,
        format!(
            "`{name}` has no storage field and cannot be read. Nothing allocates a slot for it, \
             so it previously lowered to the constant 0 and the backend then used that 0 as a \
             list handle: `{name}.length` always read 0, and `{name}[i]` returned the contents of \
             storage slot 0 (the construct's FIRST DECLARED FIELD) for every index. Declare the \
             collection as an explicit field and read that instead."
        ),
        span,
    )
}

/// `match` in expression position cannot be lowered. Refuse rather than answer
/// with a constant 0, which overwrote the destination on assignment.
pub fn match_expr_unimplemented(span: Span) -> Diagnostic {
    Diagnostic::error(
        E432_MATCH_EXPR_UNIMPLEMENTED,
        "`match` used as an expression has no lowering and cannot be compiled. It previously \
         evaluated the scrutinee and then produced the constant 0, so `n = match n { .. }` \
         silently zeroed `n` instead of updating it, and a `match` inside a guard compared \
         against 0. Covenant has no wildcard pattern, so a scrutinee matching no arm has no \
         value to yield. Use an `if`/`else` expression chain, whose `else` you write \
         explicitly: `if n == 1 { 10 } else if n == 2 { 20 } else { 0 }`. The STATEMENT form \
         (`match n { 1 => { .. } }`) does compile."
            .to_string(),
        span,
    )
}

/// `match` on a ciphertext cannot be lowered. The statement form compiles to a
/// plaintext compare-and-branch chain, which would branch on the handle.
pub fn match_encrypted_scrutinee(span: Span) -> Diagnostic {
    Diagnostic::error(
        E437_MATCH_ENCRYPTED_SCRUTINEE,
        "`match` on an encrypted value has no lowering and cannot be compiled. The statement \
         form lowers to a plaintext compare-and-branch chain, which would test the ciphertext \
         HANDLE rather than the value it hides, and the resulting branch would also leak which \
         arm was taken. Use `encrypted_when` for control flow over encrypted data."
            .to_string(),
        span,
    )
}

/// `try_action` / `catch` cannot be lowered. Refuse rather than inline the try
/// body and drop the catch body, which made a failure revert the whole call.
pub fn try_catch_unimplemented(span: Span) -> Diagnostic {
    Diagnostic::error(
        E433_TRY_CATCH_UNIMPLEMENTED,
        "`try_action { .. } catch { .. }` has no lowering and cannot be compiled. The try body \
         was inlined into the surrounding block and the catch body was discarded, so a failure \
         inside the body reverted the entire transaction and the catch never ran: the opposite \
         of what the construct says. Trapping a revert on the EVM needs an external call \
         boundary and a returndata check, which the IR has no terminator for. Check the failure \
         condition up front instead (e.g. `when x != 0` before dividing by `x`)."
            .to_string(),
        span,
    )
}

/// A non-empty list literal cannot be lowered. Refuse rather than store the
/// single zero the placeholder produced.
pub fn list_literal_unimplemented(span: Span, len: usize) -> Diagnostic {
    Diagnostic::error(
        E434_LIST_LITERAL_UNIMPLEMENTED,
        format!(
            "a list literal with {len} element(s) has no lowering and cannot be compiled. It \
             previously compiled to a placeholder that the backend answered with a single zero, \
             so `xs = [..]` wrote one 0 into the list's length word and stored none of the \
             elements. Build the list with `append` instead, one element per statement. The \
             empty literal `[]` still compiles: it is exactly the zero-length list."
        ),
        span,
    )
}

/// `delete <target>` on a shape with no zeroing lowering. Refuse rather than
/// compile the revocation to nothing and report success.
pub fn delete_unsupported_target(span: Span, what: &str) -> Diagnostic {
    Diagnostic::error(
        E435_DELETE_UNSUPPORTED_TARGET,
        format!(
            "`delete` cannot be compiled here: {what}. `delete` used to be a no-op, so a \
             revocation action compiled to an empty function that still shipped in the ABI and \
             reported success on chain while the value survived. Supported targets are a plain \
             field (`delete flag`), a map entry (`delete allowance[spender]`) and a list element \
             (`delete xs[i]`)."
        ),
        span,
    )
}

/// An `only` principal that is not an address. Refuse rather than emit
/// `caller == <non-address>`, which no caller can ever satisfy.
pub fn principal_not_address(span: Span, what: &str, rendered_ty: &str) -> Diagnostic {
    Diagnostic::error(
        E436_PRINCIPAL_NOT_ADDRESS,
        format!(
            "`only {what}` is not an address principal (it is `{rendered_ty}`), so it cannot be \
             compiled. The guard lowered to `caller == <that value>` verbatim, which no caller \
             can ever satisfy: the action compiled clean and then reverted for every caller \
             forever, on an immutable contract. Use an `address`-typed principal (`only owner` \
             with a `field owner: address`, `only deployer`, or `only <address expression>`). \
             For an allowlist held in a map, write the lookup as an ordinary guard: \
             `when allowlist[caller]`."
        ),
        span,
    )
}

/// `given` is enforced before the body, not after. Warn only where the two
/// readings diverge, that is where the guard reads a field the body writes.
pub fn given_is_precondition(span: Span, field: &str) -> Diagnostic {
    Diagnostic {
        level: DiagnosticLevel::Warning,
        code: W440_GIVEN_IS_PRECONDITION,
        message: format!(
            "`given` is compiled as a PRECONDITION, asserted before the body runs, and is \
             byte-identical to `when`. This guard reads `{field}`, which the body writes, so \
             the check sees the OLD value and the invariant is not enforced on the new one"
        ),
        span,
        help: Some(
            "the guide describes `given` as a postcondition checked after the body; the \
             compiler has no postcondition construct. Write the condition over the post-state \
             explicitly, e.g. `when n + v <= 10` instead of `given n <= 10` for a body that \
             does `n = n + v`"
                .to_string(),
        ),
    }
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

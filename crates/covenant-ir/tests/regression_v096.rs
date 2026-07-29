//! Regressions for the v0.9.6 adversarial review, covenant-ir's findings.
//!
//! Every test here pins a construct that used to lower to NOTHING, or to the
//! wrong storage address, with no diagnostic anywhere in the pipeline. Read the
//! module comment on each group before "fixing" a failure: several of these
//! assert the ABSENCE of an opcode, because the defect was a placeholder opcode
//! quietly standing in for a construct that has no lowering.

use covenant_diag::{Diagnostic, DiagnosticLevel, SourceId};
use covenant_ir::{build_ir, codes, validate, IrFunction, IrModule, Opcode, Terminator};
use covenant_lexer::tokenize;
use covenant_parser::parse;
use covenant_privacy::analyze_privacy;
use covenant_resolver::resolve;
use covenant_types::typecheck;

fn lower(src: &str) -> (IrModule, Vec<Diagnostic>) {
    let (toks, lex) = tokenize(src, SourceId::new(0));
    assert!(lex.is_empty(), "lex: {lex:?}");
    let (file, pd) = parse(&toks, SourceId::new(0));
    assert!(pd.is_empty(), "parse: {pd:?}");
    let (res, _) = resolve(file.expect("file"), SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (checked, _) = analyze_privacy(typed, SourceId::new(0));
    build_ir(checked, SourceId::new(0))
}

fn func<'a>(m: &'a IrModule, name: &str) -> &'a IrFunction {
    m.functions
        .iter()
        .find(|f| f.name.name.as_ref() == name)
        .unwrap_or_else(|| panic!("function `{name}` not lowered"))
}

fn opcodes(f: &IrFunction) -> Vec<Opcode> {
    f.blocks
        .iter()
        .flat_map(|b| b.instructions.iter())
        .map(|i| i.opcode.clone())
        .collect()
}

fn has_op(f: &IrFunction, want: &Opcode) -> bool {
    opcodes(f).iter().any(|o| o == want)
}

fn has_code(diags: &[Diagnostic], code: covenant_diag::DiagCode) -> bool {
    diags.iter().any(|d| d.code == code)
}

fn errors(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
    diags
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Error)
        .collect()
}

// ---------------------------------------------------------------------
// F-02: the `match` STATEMENT lowered to nothing.
//
// Every arm vanished, so a `revert_with` arm failed OPEN (the call succeeded
// instead of reverting) and an assigning arm never wrote anything. The
// grammar's only pattern form is a literal, so the correct lowering is the
// `if / else if` chain the builder already emits for `Stmt::If`.
// ---------------------------------------------------------------------

const MATCH_STMT: &str = r#"
record M {
    field paid: amount

    error Blocked()

    action settle(kind: amount) {
        match kind {
            1 => { revert_with Blocked() }
            2 => { paid = 999 }
        }
    }

    view get_paid returns amount { paid }
}
"#;

#[test]
fn f02_match_statement_lowers_its_arms() {
    let (m, diags) = lower(MATCH_STMT);
    assert!(errors(&diags).is_empty(), "{diags:?}");
    assert!(validate(&m).is_empty(), "validator: {:?}", validate(&m));
    let f = func(&m, "settle");

    // One comparison per arm, against the scrutinee.
    let eq_count = opcodes(f).iter().filter(|o| **o == Opcode::Eq).count();
    assert_eq!(eq_count, 2, "expected one Eq per arm, got {eq_count}");

    // The `revert_with` arm must reach a real Revert terminator: this is the
    // arm that used to fail open.
    assert!(
        f.blocks
            .iter()
            .any(|b| matches!(&b.terminator, Terminator::Revert { error, .. } if error.name.as_ref() == "Blocked")),
        "the `revert_with Blocked()` arm did not produce a Revert terminator"
    );
    // The assigning arm must reach a real store.
    assert!(
        has_op(f, &Opcode::SStore(covenant_ir::GlobalId(0))),
        "the `paid = 999` arm did not produce an SStore"
    );
    // And the dispatch must actually branch, not fall straight through.
    assert!(
        f.blocks
            .iter()
            .any(|b| matches!(b.terminator, Terminator::Branch { .. })),
        "no Branch terminator: the match arms are not being selected"
    );
}

#[test]
fn f02_match_statement_is_not_an_empty_function() {
    // The exact shape the review observed: `fn settle` was literally
    // `bb0: Return`, one block and zero instructions.
    let (m, _) = lower(MATCH_STMT);
    let f = func(&m, "settle");
    assert!(
        f.blocks.len() > 1 && !opcodes(f).is_empty(),
        "match statement lowered to an empty function again: {} block(s), {} instruction(s)",
        f.blocks.len(),
        opcodes(f).len()
    );
}

#[test]
fn f02_match_on_an_encrypted_scrutinee_is_refused() {
    // Guard on the fix itself: the arm chain is a PLAINTEXT compare-and-branch,
    // which must never be applied to a ciphertext handle.
    let src = r#"
record ME {
    field secret: encrypted amount

    action pick() {
        match secret {
            1 => { secret = 2 }
        }
    }
}
"#;
    let (_, diags) = lower(src);
    assert!(
        has_code(&diags, codes::E437_MATCH_ENCRYPTED_SCRUTINEE),
        "expected E437 for a match on an encrypted scrutinee, got: {diags:?}"
    );
}

// ---------------------------------------------------------------------
// F-07: `match` as an EXPRESSION evaluated to the constant 0.
//
// `n = match n { .. }` did not merely fail to update `n`, it destroyed the
// value already stored there. Refused rather than lowered: the grammar has no
// wildcard pattern, so a scrutinee matching no arm has no value to yield.
// ---------------------------------------------------------------------

const MATCH_EXPR: &str = r#"
record MX {
    field n: amount

    action classify() {
        n = match n {
            1 => 111
            2 => 222
        }
    }
}
"#;

#[test]
fn f07_match_expression_is_refused() {
    let (_, diags) = lower(MATCH_EXPR);
    assert!(
        has_code(&diags, codes::E432_MATCH_EXPR_UNIMPLEMENTED),
        "expected E432 for `match` in expression position, got: {diags:?}"
    );
}

#[test]
fn f07_match_expression_does_not_silently_store_a_zero() {
    // The regression that matters: the placeholder made the assignment store a
    // constant 0 over a live field. A refusal is only worth anything if it is
    // an ERROR, so the build stops before that store can reach a chain.
    let (_, diags) = lower(MATCH_EXPR);
    assert!(
        errors(&diags)
            .iter()
            .any(|d| d.code == codes::E432_MATCH_EXPR_UNIMPLEMENTED),
        "E432 must be an error, not a warning: a warning still ships the zeroing store"
    );
}

// ---------------------------------------------------------------------
// F-13: a `board`'s `append post { .. }` wrote nothing, and `posts[i].<field>`
// returned the construct's first declared field.
//
// Nothing allocates a storage field for `posts`, so the append's persistence
// path was skipped whole and reads of `posts` lowered to the constant 0, which
// the backend then used as a list handle onto storage slot 0.
// ---------------------------------------------------------------------

const BOARD: &str = r#"
board B {
    post {
        author: address
        score:  amount
    }

    field admin_secret: hash = 0x00000000000000000000000000000000000000000000000000000000DEADBEEF

    action submit(s: amount) {
        append post {
            author: caller
            score:  s
        }
    }

    view count returns amount { posts.length }
}
"#;

#[test]
fn f13_append_into_an_unbacked_collection_is_refused() {
    let (_, diags) = lower(BOARD);
    assert!(
        has_code(&diags, codes::E430_APPEND_UNBACKED_COLLECTION),
        "expected E430 for `append post` with no storage field, got: {diags:?}"
    );
}

#[test]
fn f13_reading_an_unbacked_implicit_collection_is_refused() {
    let (_, diags) = lower(BOARD);
    assert!(
        has_code(&diags, codes::E431_IMPLICIT_COLLECTION_UNBACKED),
        "expected E431 for a read of `posts`, got: {diags:?}"
    );
}

#[test]
fn f13_append_into_a_real_list_field_still_persists() {
    // Positive control: the refusal must be about the MISSING FIELD, not about
    // `append`. A record with a declared `list<Struct>` field still emits the
    // ListAppend and the length store.
    let src = r#"
record R {
    struct Entry {
        who: address
        amt: amount
    }

    entries: [Entry] = []

    action add(a: amount) {
        append entries {
            who: caller
            amt: a
        }
    }
}
"#;
    let (m, diags) = lower(src);
    assert!(errors(&diags).is_empty(), "{diags:?}");
    let f = func(&m, "add");
    assert!(
        has_op(f, &Opcode::ListAppend),
        "a backed `append` must still emit ListAppend"
    );
    assert!(
        has_op(f, &Opcode::SStore(covenant_ir::GlobalId(0))),
        "a backed `append` must still store the new length"
    );
}

// ---------------------------------------------------------------------
// F-15: `delete` lowered to nothing.
//
// It shared an empty match arm with `discard`, which means the opposite, so a
// revocation action compiled to an empty function that still shipped in the ABI
// and reported success while the value survived.
// ---------------------------------------------------------------------

const DELETE_SRC: &str = r#"
record D {
    allowance: map<address, amount>
    flag: amount = 0

    action approve(spender: address, v: amount) { allowance[spender] = v }
    action revoke(spender: address) { delete allowance[spender] }
    action clear_flag() { delete flag }
}
"#;

#[test]
fn f15_delete_on_a_map_entry_emits_a_real_write() {
    let (m, diags) = lower(DELETE_SRC);
    assert!(errors(&diags).is_empty(), "{diags:?}");
    let f = func(&m, "revoke");
    assert!(
        has_op(f, &Opcode::MapDelete),
        "`delete allowance[spender]` emitted no MapDelete: {:?}",
        opcodes(f)
    );
}

#[test]
fn f15_delete_on_a_scalar_field_emits_a_real_write() {
    let (m, diags) = lower(DELETE_SRC);
    assert!(errors(&diags).is_empty(), "{diags:?}");
    let f = func(&m, "clear_flag");
    assert!(
        has_op(f, &Opcode::SStore(covenant_ir::GlobalId(1))),
        "`delete flag` emitted no SStore: {:?}",
        opcodes(f)
    );
}

#[test]
fn f15_delete_is_never_an_empty_function() {
    // The exact shape the review observed: `fn revoke` and `fn clear_flag` were
    // both literally `bb0: Return` with zero instructions, while the ABI still
    // advertised them.
    let (m, _) = lower(DELETE_SRC);
    for name in ["revoke", "clear_flag"] {
        let f = func(&m, name);
        assert!(
            !opcodes(f).is_empty(),
            "`{name}` lowered to an empty function again: a revocation that reports success \
             and does nothing"
        );
    }
}

#[test]
fn f15_delete_of_a_whole_map_is_refused() {
    // A Covenant map has no key list, so there is no set of slots to zero.
    // Writing 0 to the map field's own slot would look like a clear and change
    // nothing: exactly the trap `delete` fell into for every target.
    let src = r#"
record DW {
    allowance: map<address, amount>

    action wipe() { delete allowance }
}
"#;
    let (_, diags) = lower(src);
    assert!(
        has_code(&diags, codes::E435_DELETE_UNSUPPORTED_TARGET),
        "expected E435 for `delete <whole map>`, got: {diags:?}"
    );
}

// ---------------------------------------------------------------------
// F-24: `xs[i] = v` wrote to the MAPPING address while `xs[i]` read from the
// ARRAY address, and the write zeroed the list length.
//
// Indexed assignment took the map lowering for every field type, so the value
// landed at keccak(i ‖ slot) while the read looked at keccak(slot) + i, and the
// trailing SStore of the MapSet result wrote 0 into the field's own slot, which
// for a list is the length word.
// ---------------------------------------------------------------------

const LIST_WRITE: &str = r#"
record LW {
    field xs: [amount]

    action put(i: amount, v: amount) { xs[i] = v }
    action bump(i: amount) { xs[i] += 1 }

    view at(i: amount) returns amount { xs[i] }
    view len returns amount { xs.length }
}
"#;

#[test]
fn f24_list_indexed_write_uses_the_list_address() {
    let (m, diags) = lower(LIST_WRITE);
    assert!(errors(&diags).is_empty(), "{diags:?}");
    let put = func(&m, "put");
    assert!(
        has_op(put, &Opcode::ListSet),
        "`xs[i] = v` must lower to ListSet, got {:?}",
        opcodes(put)
    );
    // ListSet and ListGet share the backend's `emit_list_elem_addr`, so the
    // write and the read agree by construction. MapSet does not.
    assert!(
        !has_op(put, &Opcode::MapSet),
        "`xs[i] = v` still takes the mapping path: the write and the read address \
         different slots"
    );
}

#[test]
fn f24_list_indexed_write_does_not_clobber_the_length_word() {
    let (m, _) = lower(LIST_WRITE);
    let put = func(&m, "put");
    assert!(
        !has_op(put, &Opcode::SStore(covenant_ir::GlobalId(0))),
        "`xs[i] = v` writes the field's own slot, which for a list is the LENGTH word: \
         every indexed write silently truncates the list to empty"
    );
}

#[test]
fn f24_compound_list_write_reads_through_the_list_too() {
    let (m, _) = lower(LIST_WRITE);
    let bump = func(&m, "bump");
    assert!(
        has_op(bump, &Opcode::ListGet) && has_op(bump, &Opcode::ListSet),
        "`xs[i] += 1` must read and write through the list path, got {:?}",
        opcodes(bump)
    );
    assert!(
        !has_op(bump, &Opcode::MapGet) && !has_op(bump, &Opcode::MapSet),
        "`xs[i] += 1` still takes the mapping path"
    );
}

#[test]
fn f24_map_indexed_write_is_unchanged() {
    // Negative side of the same fix: a real map must keep the map lowering.
    let src = r#"
record MW {
    bal: map<address, amount>

    action credit(who: address, v: amount) { bal[who] = v }
}
"#;
    let (m, diags) = lower(src);
    assert!(errors(&diags).is_empty(), "{diags:?}");
    let f = func(&m, "credit");
    assert!(has_op(f, &Opcode::MapSet), "a map write must stay a MapSet");
    assert!(
        !has_op(f, &Opcode::ListSet),
        "a map write must not become a ListSet"
    );
}

// ---------------------------------------------------------------------
// F-25: `xs = [10, 20, 30]` stored nothing.
//
// The literal lowered to a placeholder `StructNew` whose backend arm emits a
// single PUSH0, so the assignment wrote one zero into the list's length word
// and none of the elements was written anywhere.
// ---------------------------------------------------------------------

#[test]
fn f25_non_empty_list_literal_is_refused() {
    let src = r#"
record LL {
    field xs: [amount]

    action fill() { xs = [10, 20, 30] }
}
"#;
    let (_, diags) = lower(src);
    assert!(
        has_code(&diags, codes::E434_LIST_LITERAL_UNIMPLEMENTED),
        "expected E434 for a non-empty list literal, got: {diags:?}"
    );
}

#[test]
fn f25_empty_list_literal_still_compiles() {
    // `[]` is exactly the zero-length list and storing 0 into the length word is
    // its complete lowering, so the refusal must not swallow it.
    let src = r#"
record LE {
    field xs: [amount]

    action clear() { xs = [] }
}
"#;
    let (_, diags) = lower(src);
    assert!(
        !has_code(&diags, codes::E434_LIST_LITERAL_UNIMPLEMENTED),
        "`[]` must still compile: it is the zero-length list, got {diags:?}"
    );
}

// ---------------------------------------------------------------------
// F-26: `&&` and `||` did not short-circuit.
//
// Both operands were lowered as ordinary values and combined with a bitwise EVM
// AND / OR before the branch, so `if x != 0 && 100 / x > 5` divided by zero on
// exactly the input the guard was written to protect against.
// ---------------------------------------------------------------------

const SHORT_CIRCUIT: &str = r#"
record SC {
    field n: amount

    action safe_div(x: amount) {
        n = 0
        if x != 0 && 100 / x > 5 {
            n = 1
        }
    }

    action safe_or(x: amount) {
        n = 0
        if x == 0 || 100 / x > 5 {
            n = 2
        }
    }
}
"#;

#[test]
fn f26_and_does_not_evaluate_its_right_operand_unconditionally() {
    let (m, diags) = lower(SHORT_CIRCUIT);
    assert!(errors(&diags).is_empty(), "{diags:?}");
    assert!(validate(&m).is_empty(), "validator: {:?}", validate(&m));
    let f = func(&m, "safe_div");
    let entry = &f.blocks[f.entry.0 as usize];
    assert!(
        !entry
            .instructions
            .iter()
            .any(|i| matches!(i.opcode, Opcode::Div)),
        "the right operand's Div sits in the entry block: it is evaluated before the \
         branch that was supposed to guard it"
    );
    assert!(
        f.blocks
            .iter()
            .flat_map(|b| b.instructions.iter())
            .any(|i| matches!(i.opcode, Opcode::Div)),
        "the Div disappeared entirely, which is a different bug"
    );
}

#[test]
fn f26_or_does_not_evaluate_its_right_operand_unconditionally() {
    let (m, _) = lower(SHORT_CIRCUIT);
    let f = func(&m, "safe_or");
    let entry = &f.blocks[f.entry.0 as usize];
    assert!(
        !entry
            .instructions
            .iter()
            .any(|i| matches!(i.opcode, Opcode::Div)),
        "`||` still evaluates its right operand before the branch"
    );
}

#[test]
fn f26_logical_and_or_opcodes_are_gone_for_plaintext() {
    // The backend lowers LogicalAnd / LogicalOr to a bitwise AND / OR, which is
    // total by definition. Their presence IS the non-short-circuit bug.
    let (m, _) = lower(SHORT_CIRCUIT);
    for name in ["safe_div", "safe_or"] {
        let f = func(&m, name);
        assert!(
            !has_op(f, &Opcode::LogicalAnd) && !has_op(f, &Opcode::LogicalOr),
            "`{name}` still combines both operands with a total bitwise operator"
        );
    }
}

#[test]
fn f26_encrypted_and_stays_total() {
    // Guard on the fix: an FHE branch cannot skip work based on a ciphertext
    // without leaking which way it went, so encrypted operands must keep the
    // total FheAnd lowering.
    let src = r#"
record SCE {
    field a: encrypted bool
    field b: encrypted bool
    field out: encrypted bool

    action combine() {
        out = a && b
    }
}
"#;
    let (m, _) = lower(src);
    let f = func(&m, "combine");
    assert!(
        has_op(f, &Opcode::FheAnd),
        "an encrypted `&&` must stay a total FheAnd, got {:?}",
        opcodes(f)
    );
}

// ---------------------------------------------------------------------
// F-30: `try_action` / `catch` discarded the catch body.
//
// The try body was inlined into the surrounding block and the catch body was
// dropped, so a failure inside the body reverted the whole transaction and the
// catch never ran: the opposite of what the construct says.
// ---------------------------------------------------------------------

#[test]
fn f30_try_catch_is_refused() {
    let src = r#"
record T {
    field a: amount
    field caught: amount

    action run(x: amount) {
        try_action {
            a = 100 / x
        } catch _ {
            caught = 7
        }
    }
}
"#;
    let (m, diags) = lower(src);
    assert!(
        has_code(&diags, codes::E433_TRY_CATCH_UNIMPLEMENTED),
        "expected E433 for `try_action` / `catch`, got: {diags:?}"
    );
    // And the catch body must not have been quietly folded into the try path.
    let f = func(&m, "run");
    assert!(
        !has_op(f, &Opcode::SStore(covenant_ir::GlobalId(1))),
        "the catch body was lowered inline, which runs it unconditionally"
    );
}

// ---------------------------------------------------------------------
// F-33: `only <non-address>` compiled to an unsatisfiable caller comparison.
//
// The parser routes any non-keyword token to `Principal::Address(expr)` and the
// type checker's `check_principal` is empty, so `only "owner"` became
// `caller == 0` and `only owner` with `field owner: map<address, bool>` compared
// the caller against a slot a Covenant map never writes. Every one compiled
// clean and reverted for every caller forever.
// ---------------------------------------------------------------------

fn only_src(principal: &str) -> String {
    format!(
        "record OP {{\n    field n: amount\n    action f(v: amount) only {principal} {{ n = v }}\n}}\n"
    )
}

#[test]
fn f33_literal_principals_are_refused() {
    for principal in ["\"owner\"", "42", "true", "-1"] {
        let (_, diags) = lower(&only_src(principal));
        assert!(
            has_code(&diags, codes::E436_PRINCIPAL_NOT_ADDRESS),
            "expected E436 for `only {principal}`, got: {diags:?}"
        );
    }
}

#[test]
fn f33_non_address_owner_field_is_refused() {
    for ty in ["map<address, bool>", "bool", "amount"] {
        let src = format!(
            "record OF {{\n    field owner: {ty}\n    field n: amount\n    \
             action set_n(v: amount) only owner {{ n = v }}\n}}\n"
        );
        let (_, diags) = lower(&src);
        assert!(
            has_code(&diags, codes::E436_PRINCIPAL_NOT_ADDRESS),
            "expected E436 for `only owner` with `field owner: {ty}`, got: {diags:?}"
        );
    }
}

#[test]
fn f33_real_address_principals_still_compile() {
    // Positive control. Refusing these would brick every guarded contract.
    let src = r#"
record OK {
    field owner: address
    field n: amount

    action a(v: amount) only owner { n = v }
    action b(v: amount) only deployer { n = v }
    action c(v: amount) only 0x1111111111111111111111111111111111111111 { n = v }
}
"#;
    let (m, diags) = lower(src);
    assert!(
        !has_code(&diags, codes::E436_PRINCIPAL_NOT_ADDRESS),
        "an address principal must not be refused: {diags:?}"
    );
    for name in ["a", "b", "c"] {
        assert!(
            has_op(func(&m, name), &Opcode::LoadCaller),
            "`{name}` lost its caller check"
        );
    }
}

// ---------------------------------------------------------------------
// F-39: `given` is compiled as a precondition, byte-identical to `when`, while
// the guide shipped in this tree calls it a postcondition.
//
// Warned rather than refused: the check is real and enforced, it is just early,
// and the language has no postcondition construct to redirect to. Emitted only
// where the two readings diverge, that is where the guard reads a field the body
// writes.
// ---------------------------------------------------------------------

#[test]
fn f39_given_over_state_the_body_writes_warns() {
    let src = r#"
record G {
    field n: amount

    action bump(v: amount) given n <= 10 {
        n = n + v
    }
}
"#;
    let (_, diags) = lower(src);
    assert!(
        has_code(&diags, codes::W440_GIVEN_IS_PRECONDITION),
        "expected W440: the guard reads `n` and the body writes `n`, so precondition and \
         postcondition disagree. Got: {diags:?}"
    );
    assert!(
        diags
            .iter()
            .filter(|d| d.code == codes::W440_GIVEN_IS_PRECONDITION)
            .all(|d| d.level == DiagnosticLevel::Warning),
        "W440 must stay a warning: the check IS enforced, it is only early"
    );
}

#[test]
fn f39_given_over_a_parameter_does_not_warn() {
    // The shape the repository's own README uses. Precondition and
    // postcondition readings agree here, so the warning would be pure noise.
    let src = r#"
record GP {
    field fee_bps: amount

    action set_fee(bps: amount) given bps <= 500 {
        fee_bps = bps
    }
}
"#;
    let (_, diags) = lower(src);
    assert!(
        !has_code(&diags, codes::W440_GIVEN_IS_PRECONDITION),
        "W440 fired on a guard that reads no written field: {diags:?}"
    );
}

// ---------------------------------------------------------------------
// F-42: W421 carried a zero span, so the only warning that an access-control
// guard cannot be enforced named no file, no line and no action.
// ---------------------------------------------------------------------

#[test]
fn f42_unenforceable_guard_warning_has_a_real_span() {
    let src = r#"
record W {
    field n: amount

    action set_a(v: amount) { n = v }
    action set_b(v: amount) only owner { n = v }
}
"#;
    let (_, diags) = lower(src);
    let w = diags
        .iter()
        .find(|d| d.code == codes::E421_GUARD_UNRESOLVED_PRINCIPAL)
        .expect("expected W421 for `only owner` with no `field owner`");
    assert!(
        w.span.start != 0 || w.span.end != 0,
        "W421 still carries Span(0, 0): it renders with no file, no line and no caret, so \
         a file with several guarded actions cannot say WHICH one always reverts"
    );
    // And it must point INSIDE the guarded action, not at the top of the file.
    let guarded_at = src.find("action set_b").expect("fixture");
    assert!(
        (w.span.start as usize) >= guarded_at,
        "W421 points at byte {} but the guarded action starts at {guarded_at}",
        w.span.start
    );
}

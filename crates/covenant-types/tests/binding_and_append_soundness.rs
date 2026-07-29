//! Soundness coverage for the two ways a value used to escape the type and
//! privacy checks entirely.
//!
//! 1. Binding definition sites (`let`, `for each`, `catch`, lambda parameters,
//!    action/view arguments) are recorded in the resolver's `locals` vector,
//!    not in `ident_bindings`, which only holds use-sites. The checker used to
//!    look them up in `ident_bindings`, so the lookup could never succeed and
//!    the local kept its `Ty::Unknown` default. `Unknown` is compatible with
//!    every type and maps to `PrivacyDomain::Unknown`, so one `let` laundered a
//!    `ciphertext<amount>` into a plaintext `amount` field with no diagnostic
//!    from either phase.
//!
//! 2. `append <list> { field: value }` resolved its element struct by matching
//!    the collection identifier against `Binding::Struct`. A record field
//!    `votes: [Vote]` binds to `Binding::Field`, so the match never fired and
//!    every value in the literal was synth'd but never checked: type confusion,
//!    ciphertext into a plaintext field, and a skipped plaintext-to-ciphertext
//!    auto-lift that stored a field declared `encrypted amount` in the clear.

use covenant_diag::{Diagnostic, DiagnosticLevel, SourceId};
use covenant_lexer::tokenize;
use covenant_parser::parse;
use covenant_resolver::resolve;
use covenant_types::{codes, typecheck, Ty, TypedFile};

fn pipeline(src: &str) -> (TypedFile, Vec<Diagnostic>) {
    let (toks, lex) = tokenize(src, SourceId::new(0));
    assert!(lex.is_empty(), "lex: {lex:?}");
    let (file, pd) = parse(&toks, SourceId::new(0));
    assert!(pd.is_empty(), "parse: {pd:?}");
    let (res, _) = resolve(file.expect("file"), SourceId::new(0));
    typecheck(res, SourceId::new(0))
}

fn errors(d: &[Diagnostic]) -> Vec<&Diagnostic> {
    d.iter()
        .filter(|x| x.level == DiagnosticLevel::Error)
        .collect()
}

fn has(src: &str, c: covenant_diag::DiagCode) -> bool {
    pipeline(src).1.iter().any(|d| d.code == c)
}

// ---------------- Binding definition sites keep their type ----------------

/// The control: written directly, this is refused. One `let` must not change
/// that.
#[test]
fn direct_assignment_of_a_mistyped_value_is_refused() {
    assert!(has(
        r#"record R {
            who: address
            action f(w: amount) { who = w }
        }"#,
        codes::E201_TYPE_MISMATCH
    ));
}

#[test]
fn let_binding_does_not_erase_the_type() {
    assert!(has(
        r#"record R {
            who: address
            action f(w: amount) {
                let x = w
                who = x
            }
        }"#,
        codes::E201_TYPE_MISMATCH
    ));
}

#[test]
fn annotated_let_binding_does_not_erase_the_type() {
    assert!(has(
        r#"record R {
            who: address
            action f(w: amount) {
                let x: amount = w
                who = x
            }
        }"#,
        codes::E201_TYPE_MISMATCH
    ));
}

/// The secrecy face: a `ciphertext<amount>` reaching a plaintext `amount` field
/// that a public view returns.
#[test]
fn let_binding_does_not_erase_the_privacy_domain() {
    assert!(has(
        r#"record R {
            secret: encrypted amount
            total: amount = 0
            action f() {
                let x = secret
                total = x
            }
            view t returns amount { total }
        }"#,
        codes::E201_TYPE_MISMATCH
    ));
}

#[test]
fn for_each_binding_keeps_the_element_type() {
    assert!(has(
        r#"record R {
            struct Rec {
                who: address
                salary: encrypted amount
            }
            ledger: [Rec] = []
            last: amount = 0
            action audit() {
                for each r in ledger {
                    last = r.salary
                }
            }
        }"#,
        codes::E201_TYPE_MISMATCH
    ));
}

#[test]
fn catch_binding_is_typed_text() {
    assert!(has(
        r#"record R {
            n: amount = 0
            action f() {
                try_action {
                    n = 1
                } catch e {
                    n = e
                }
            }
        }"#,
        codes::E201_TYPE_MISMATCH
    ));
}

/// Positive control for the group above: the fix must not reject well-typed
/// code, and the local must carry a real type rather than merely dodging the
/// check. `Ty::Unknown` in `local_types` would satisfy "no errors" while
/// leaving the hole open, so the type itself is asserted.
#[test]
fn well_typed_let_still_compiles_and_the_local_is_typed() {
    let (t, d) = pipeline(
        r#"record R {
            total: amount = 0
            action f(w: amount) {
                let x = w
                total = x
            }
        }"#,
    );
    assert!(errors(&d).is_empty(), "{d:?}");
    let x = t
        .resolved
        .bindings
        .locals
        .iter()
        .find(|l| l.name.name.as_ref() == "x")
        .expect("the resolver registered a local named `x`");
    assert_eq!(
        t.types.local_ty(x.id),
        Ty::Amount,
        "the `let` binding did not carry the initializer's type"
    );
}

// ---------------- `append` checks the struct literal ----------------

#[test]
fn append_checks_each_value_against_the_declared_field_type() {
    assert!(has(
        r#"record R {
            struct Entry {
                who: address
                weight: amount
            }
            entries: [Entry] = []
            action add(w: amount) { append entries { who: w, weight: caller } }
        }"#,
        codes::E201_TYPE_MISMATCH
    ));
}

#[test]
fn append_refuses_ciphertext_into_a_plaintext_struct_field() {
    assert!(has(
        r#"record R {
            struct Entry {
                who: address
                weight: amount
            }
            secret: encrypted amount
            entries: [Entry] = []
            action leak() { append entries { who: caller, weight: secret } }
        }"#,
        codes::E201_TYPE_MISMATCH
    ));
}

#[test]
fn append_refuses_a_field_the_struct_does_not_declare() {
    assert!(has(
        r#"record R {
            struct Entry {
                who: address
                weight: amount
            }
            entries: [Entry] = []
            action add(w: amount) { append entries { who: caller, nonexistent: w } }
        }"#,
        codes::E240_APPEND_UNKNOWN_FIELD
    ));
}

#[test]
fn append_refuses_a_collection_that_is_not_a_list_of_structs() {
    assert!(has(
        r#"record R {
            xs: [amount] = []
            action add(v: amount) { append xs { v: v } }
        }"#,
        codes::E229_APPEND_NOT_LIST
    ));
}

/// Consequence (c) of the same hole: with no expected type, a plaintext
/// argument bound for a field declared `encrypted amount` recorded no lift
/// marker, so Phase 6 emitted no `FheEncryptTrivial` and the cleartext was
/// stored raw. The identical direct assignment did lift.
#[test]
fn append_lifts_plaintext_into_an_encrypted_struct_field() {
    let (t, d) = pipeline(
        r#"record R {
            struct Rec {
                who: address
                salary: encrypted amount
            }
            ledger: [Rec] = []
            action hire(w: address, s: amount) { append ledger { who: w, salary: s } }
        }"#,
    );
    assert!(errors(&d).is_empty(), "{d:?}");
    assert!(
        t.types
            .lifts
            .iter()
            .any(|l| l.from == Ty::Amount && l.to == Ty::Ciphertext(Box::new(Ty::Amount))),
        "no plaintext-to-ciphertext lift recorded for the append: {:?}",
        t.types.lifts
    );
}

/// Positive control for the group above: the canonical list-of-struct shape
/// used by the compiler's own end-to-end fixture must still compile clean.
#[test]
fn well_typed_append_still_compiles() {
    let (_, d) = pipeline(
        r#"record BallotBox {
            struct Vote {
                voter: address
                weight: amount
            }
            votes: [Vote] = []
            action cast(w: amount) { append votes { voter: caller, weight: w } }
            view get_count returns amount { votes.length }
        }"#,
    );
    assert!(errors(&d).is_empty(), "{d:?}");
}

/// A `board`'s `post` block resolves to the struct type itself rather than to a
/// list field. That path already worked and must keep working.
#[test]
fn board_post_append_still_checks_its_fields() {
    assert!(has(
        r#"board B {
            post {
                author: address
                content: hash
            }
            action submit(c: hash) {
                append post {
                    author: c
                    content: c
                }
            }
        }"#,
        codes::E201_TYPE_MISMATCH
    ));
}

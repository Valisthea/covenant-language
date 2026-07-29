//! `min`/`max`/`abs`/`pow`/`sqrt` must refuse to compile.
//!
//! They used to map to `Opcode::AddChecked` as a placeholder, so `max(cap, bid)`
//! compiled cleanly and returned `cap + bid`. Ordinary-looking source, wrong
//! bytecode, no diagnostic: the same class as the ERC-721 authorization
//! bypass. Until a real compare-and-branch lowering exists (the IR has `Lt`/`Gt`
//! but no select, and a stdlib fn maps to exactly one opcode), the only safe
//! behaviour is to reject the call.
//!
//! If someone implements one of these properly, delete its case here and add a
//! semantic test. Do not "fix" this test by restoring a placeholder opcode.

use covenant_diag::{Diagnostic, SourceId};
use covenant_ir::{build_ir, codes};
use covenant_lexer::tokenize;
use covenant_parser::parse;
use covenant_privacy::analyze_privacy;
use covenant_resolver::resolve;
use covenant_types::typecheck;

fn lower(src: &str) -> Vec<Diagnostic> {
    let (toks, lex) = tokenize(src, SourceId::new(0));
    assert!(lex.is_empty(), "lex: {lex:?}");
    let (file, pd) = parse(&toks, SourceId::new(0));
    assert!(pd.is_empty(), "parse: {pd:?}");
    let (res, _) = resolve(file.expect("file"), SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (checked, _) = analyze_privacy(typed, SourceId::new(0));
    let (_, diags) = build_ir(checked, SourceId::new(0));
    diags
}

fn assert_rejected(body: &str, what: &str) {
    let src = format!(
        r#"
record R {{
    cap: amount = 0

    view f(bid: amount) returns amount {{
        {body}
    }}
}}
"#
    );
    let diags = lower(&src);
    assert!(
        diags
            .iter()
            .any(|d| d.code == codes::E424_STDLIB_MATH_UNIMPLEMENTED),
        "`{what}` should be rejected with E424, got: {diags:?}"
    );
}

#[test]
fn max_is_rejected() {
    assert_rejected("max(cap, bid)", "max");
}

#[test]
fn min_is_rejected() {
    assert_rejected("min(cap, bid)", "min");
}

#[test]
fn abs_is_rejected() {
    assert_rejected("abs(cap)", "abs");
}

#[test]
fn pow_is_rejected() {
    assert_rejected("pow(cap, bid)", "pow");
}

#[test]
fn sqrt_is_rejected() {
    assert_rejected("sqrt(cap)", "sqrt");
}

/// The regression that matters: the old placeholder made these compile to an
/// addition. Nothing may lower to `AddChecked` behind the user's back again.
#[test]
fn max_does_not_lower_to_an_addition() {
    let src = r#"
record R {
    cap: amount = 0
    view f(bid: amount) returns amount { max(cap, bid) }
}
"#;
    let (toks, _) = tokenize(src, SourceId::new(0));
    let (file, _) = parse(&toks, SourceId::new(0));
    let (res, _) = resolve(file.expect("file"), SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (checked, _) = analyze_privacy(typed, SourceId::new(0));
    let (module, _) = build_ir(checked, SourceId::new(0));

    let has_add = module.functions.iter().any(|f| {
        f.blocks
            .iter()
            .flat_map(|b| b.instructions.iter())
            .any(|i| matches!(i.opcode, covenant_ir::Opcode::AddChecked))
    });
    assert!(
        !has_add,
        "`max` lowered to AddChecked: the silent-miscompile placeholder is back"
    );
}

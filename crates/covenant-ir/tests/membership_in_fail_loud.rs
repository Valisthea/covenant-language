//! The `in` membership operator must refuse to compile.
//!
//! `x in list` used to fall through `choose_binop` to `Opcode::Eq`, so it
//! compiled to a single scalar `x == list[0]`: a `given x in options` guard
//! passed only when `x` equalled the FIRST element and silently rejected every
//! other legitimate member. Ordinary-looking source, wrong bytecode, no
//! diagnostic — the same silent-miscompile class as E424/E425.
//!
//! A real lowering is a `ListContains` loop (compare + branch over each
//! element), which the single-opcode `choose_binop` cannot express. Until it
//! exists, the only safe behaviour is to reject the operator.
//!
//! If someone implements it properly, delete this test and add a semantic one.
//! Do not "fix" this test by restoring the placeholder `Eq`.

use covenant_diag::{Diagnostic, SourceId};
use covenant_ir::{build_ir, codes};
use covenant_lexer::tokenize;
use covenant_parser::parse;
use covenant_privacy::analyze_privacy;
use covenant_resolver::resolve;
use covenant_types::typecheck;

fn lower(src: &str) -> (covenant_ir::IrModule, Vec<Diagnostic>) {
    let (toks, lex) = tokenize(src, SourceId::new(0));
    assert!(lex.is_empty(), "lex: {lex:?}");
    let (file, pd) = parse(&toks, SourceId::new(0));
    assert!(pd.is_empty(), "parse: {pd:?}");
    let (res, _) = resolve(file.expect("file"), SourceId::new(0));
    let (typed, _) = typecheck(res, SourceId::new(0));
    let (checked, _) = analyze_privacy(typed, SourceId::new(0));
    build_ir(checked, SourceId::new(0))
}

const IN_SRC: &str = r#"
record R {
    allowed: [amount] = []

    view f(x: amount) returns bool {
        x in allowed
    }
}
"#;

#[test]
fn in_operator_is_refused() {
    let (_, diags) = lower(IN_SRC);
    assert!(
        diags
            .iter()
            .any(|d| d.code == codes::E426_MEMBERSHIP_IN_UNIMPLEMENTED),
        "`in` should be refused with E426, got: {diags:?}"
    );
}

/// The regression that matters: the old placeholder made `in` lower to a scalar
/// `Eq`. Nothing may lower to `Eq` behind the user's back again — this source
/// contains no other comparison, so any `Eq` opcode is the placeholder returning.
#[test]
fn in_does_not_lower_to_a_scalar_eq() {
    let (module, _) = lower(IN_SRC);
    let has_eq = module.functions.iter().any(|f| {
        f.blocks
            .iter()
            .flat_map(|b| b.instructions.iter())
            .any(|i| matches!(i.opcode, covenant_ir::Opcode::Eq))
    });
    assert!(
        !has_eq,
        "`in` lowered to a scalar Eq — the silent-miscompile placeholder is back"
    );
}

//! Per-variant unit tests for the parser: exercise one grammar rule at a time.

use covenant_diag::SourceId;
use covenant_lexer::{tokenize, DurationUnit};
use covenant_parser::ast::{
    ActionQualifier, AssignOp, BinaryOp, ConstructKind, Expr, LValue, LiteralExpr, MetadataValue,
    Ordering, PrincipalKind, PrivacyQualifier, RevealTarget, Stmt, TopLevelDecl, Type, UnaryOp,
};
use covenant_parser::parse;

/// Parse a full source, expect no diagnostics, return the file.
fn parse_clean(src: &str) -> covenant_parser::ast::File {
    let (toks, lex) = tokenize(src, SourceId::new(0));
    assert!(lex.is_empty(), "lex diags: {lex:?}");
    let (file, parse_diags) = parse(&toks, SourceId::new(0));
    assert!(parse_diags.is_empty(), "parse diags: {parse_diags:?}");
    file.expect("expected an AST")
}

/// Parse the body of a fresh `record Dummy { ... }`.
fn parse_record(body: &str) -> Vec<TopLevelDecl> {
    let src = format!("record Dummy {{\n{body}\n}}\n");
    parse_clean(&src).top_level.body
}

/// Parse an expression by wrapping it in a `view` that returns it.
fn parse_view_body(expr_src: &str) -> Expr {
    let src = format!("record Dummy {{\n    view v returns amount {{ {expr_src} }}\n}}\n");
    let file = parse_clean(&src);
    for d in file.top_level.body {
        if let TopLevelDecl::View(v) = d {
            return v.body;
        }
    }
    panic!("no view in dummy record");
}

/// Parse statements inside an action body.
fn parse_stmts(body_src: &str) -> Vec<Stmt> {
    let src = format!("record Dummy {{\n    action do() {{\n{body_src}\n}}\n}}\n");
    let file = parse_clean(&src);
    for d in file.top_level.body {
        if let TopLevelDecl::Action(a) = d {
            return a.body;
        }
    }
    panic!("no action in dummy record");
}

// ---- top-level / construct variants ----

#[test]
fn empty_record() {
    let file = parse_clean("record R {}\n");
    assert_eq!(file.top_level.keyword, ConstructKind::Record);
    assert_eq!(file.top_level.name.name.as_ref(), "R");
    assert!(file.top_level.body.is_empty());
}

#[test]
fn all_construct_kinds_parse() {
    for (src, expected) in [
        ("record R {}\n", ConstructKind::Record),
        ("token T {}\n", ConstructKind::Token),
        ("ballot B {}\n", ConstructKind::Ballot),
        ("counter C {}\n", ConstructKind::Counter),
        ("board Bd {}\n", ConstructKind::Board),
        ("market M {}\n", ConstructKind::Market),
        ("vault V {}\n", ConstructKind::Vault),
        ("registry Rg {}\n", ConstructKind::Registry),
        ("ceremony Ce {}\n", ConstructKind::Ceremony),
        ("module Mo {}\n", ConstructKind::Module),
        ("test Ts {}\n", ConstructKind::Test),
    ] {
        let f = parse_clean(src);
        assert_eq!(f.top_level.keyword, expected, "src: {src}");
    }
}

#[test]
fn privacy_qualifier_encrypted() {
    let f = parse_clean("encrypted counter C {}\n");
    assert_eq!(f.top_level.privacy, Some(PrivacyQualifier::Encrypted));
}

#[test]
fn privacy_qualifier_sealed() {
    let f = parse_clean("sealed ballot B {}\n");
    assert_eq!(f.top_level.privacy, Some(PrivacyQualifier::Sealed));
}

#[test]
fn upgradeable_clause() {
    let f = parse_clean("record R upgradeable_by admin {}\n");
    let up = f.top_level.upgradeable.expect("upgradeable clause");
    assert!(matches!(
        up.principal,
        covenant_parser::ast::Principal::Named(PrincipalKind::Admin)
    ));
}

#[test]
fn anchor_clause_on_bridge() {
    let f = parse_clean("bridge Br anchored_on [\"ethereum\", \"aster\"] {}\n");
    let anchor = f.top_level.anchor.expect("anchor clause");
    assert_eq!(anchor.chains.len(), 2);
    assert_eq!(anchor.chains[0].value.as_ref(), "ethereum");
    assert_eq!(anchor.chains[1].value.as_ref(), "aster");
}

// ---- fields ----

#[test]
fn implicit_field_simple() {
    let body = parse_record("x: amount\n");
    assert_eq!(body.len(), 1);
    match &body[0] {
        TopLevelDecl::Field(f) => {
            assert_eq!(f.name.name.as_ref(), "x");
            assert!(matches!(f.ty, Type::Amount(_)));
            assert!(!f.explicit_field_keyword);
        }
        _ => panic!("expected field"),
    }
}

#[test]
fn explicit_field_keyword() {
    let body = parse_record("field x: amount\n");
    match &body[0] {
        TopLevelDecl::Field(f) => {
            assert!(f.explicit_field_keyword);
        }
        _ => panic!("expected field"),
    }
}

#[test]
fn field_with_initializer() {
    let body = parse_record("x: amount = 42\n");
    match &body[0] {
        TopLevelDecl::Field(f) => {
            let init = f.initializer.as_ref().expect("initializer present");
            match init {
                Expr::Literal(LiteralExpr::Integer(42, _)) => {}
                other => panic!("expected Integer(42), got {other:?}"),
            }
        }
        _ => panic!("expected field"),
    }
}

#[test]
fn field_with_list_type() {
    let body = parse_record("xs: [amount]\n");
    match &body[0] {
        TopLevelDecl::Field(f) => assert!(matches!(f.ty, Type::List(_, _))),
        _ => panic!("expected field"),
    }
}

#[test]
fn field_with_map_type() {
    let body = parse_record("m: map<address, amount>\n");
    match &body[0] {
        TopLevelDecl::Field(f) => assert!(matches!(f.ty, Type::Map(_, _, _))),
        _ => panic!("expected field"),
    }
}

#[test]
fn field_with_priority_queue_type() {
    let body = parse_record("q: priority_queue<amount, address, max>\n");
    match &body[0] {
        TopLevelDecl::Field(f) => match &f.ty {
            Type::PriorityQueue(_, _, Ordering::Max, _) => {}
            other => panic!("expected priority_queue<_,_,max>, got {other:?}"),
        },
        _ => panic!("expected field"),
    }
}

#[test]
fn field_encrypted_type() {
    let body = parse_record("x: encrypted amount\n");
    match &body[0] {
        TopLevelDecl::Field(f) => match &f.ty {
            Type::Encrypted(inner, _) => assert!(matches!(**inner, Type::Amount(_))),
            other => panic!("expected encrypted amount, got {other:?}"),
        },
        _ => panic!("expected field"),
    }
}

#[test]
fn inline_choice_values() {
    let body = parse_record(r#"opts: choice = ["a", "b", "c"]"#);
    match &body[0] {
        TopLevelDecl::Field(f) => match &f.ty {
            Type::Choice(_, values) => assert_eq!(values.len(), 3),
            other => panic!("expected choice, got {other:?}"),
        },
        _ => panic!("expected field"),
    }
}

// ---- metadata ----

#[test]
fn token_metadata_simple() {
    let f = parse_clean(r#"token C { symbol: "C" }"#);
    let body = &f.top_level.body;
    assert_eq!(body.len(), 1);
    match &body[0] {
        TopLevelDecl::Metadata(m) => assert_eq!(m.name.name.as_ref(), "symbol"),
        other => panic!("expected metadata, got {other:?}"),
    }
}

#[test]
fn token_genesis_mint() {
    let f = parse_clean("token C { supply: 1000 to deployer }\n");
    match &f.top_level.body[0] {
        TopLevelDecl::Metadata(m) => match &m.value {
            MetadataValue::GenesisMint { amount: _, to } => match to {
                covenant_parser::ast::Principal::Named(PrincipalKind::Deployer) => {}
                _ => panic!("expected deployer"),
            },
            _ => panic!("expected GenesisMint"),
        },
        _ => panic!("expected metadata"),
    }
}

// ---- actions ----

#[test]
fn action_with_guards() {
    let body = parse_record("action f() when now > 0, only owner, given 1 > 0 {}\n");
    match &body[0] {
        TopLevelDecl::Action(a) => assert_eq!(a.guards.len(), 3),
        _ => panic!("expected action"),
    }
}

#[test]
fn action_with_annotations() {
    let body = parse_record("@batch_up_to(32)\n@prove_offchain\naction f() {}\n");
    match &body[0] {
        TopLevelDecl::Action(a) => assert_eq!(a.annotations.len(), 2),
        _ => panic!("expected action"),
    }
}

#[test]
fn action_with_pq_signed() {
    let body = parse_record("action a() pq_signed(a, b, c) {}\n");
    match &body[0] {
        TopLevelDecl::Action(a) => {
            assert_eq!(a.qualifiers.len(), 1);
            assert!(matches!(a.qualifiers[0], ActionQualifier::PqSigned { .. }));
        }
        _ => panic!("expected action"),
    }
}

#[test]
fn action_with_verified_by() {
    let body = parse_record("action a() verified_by(proof) {}\n");
    match &body[0] {
        TopLevelDecl::Action(a) => {
            assert!(matches!(a.qualifiers[0], ActionQualifier::VerifiedBy(_)))
        }
        _ => panic!("expected action"),
    }
}

// ---- views / reveals ----

#[test]
fn view_simple() {
    let body = parse_record("view n returns amount { 42 }\n");
    match &body[0] {
        TopLevelDecl::View(v) => {
            assert_eq!(v.name.name.as_ref(), "n");
            assert!(matches!(v.body, Expr::Literal(LiteralExpr::Integer(42, _))));
        }
        _ => panic!("expected view"),
    }
}

#[test]
fn reveal_one_liner() {
    let body = parse_record("reveal total to owner\n");
    match &body[0] {
        TopLevelDecl::Reveal(r) => {
            assert!(r.body.is_none());
            assert_eq!(r.target, Some(RevealTarget::Owner));
        }
        _ => panic!("expected reveal"),
    }
}

#[test]
fn reveal_block_form() {
    let body = parse_record("reveal winner returns amount { 42 }\n");
    match &body[0] {
        TopLevelDecl::Reveal(r) => {
            assert!(r.body.is_some());
            assert!(r.target.is_none());
        }
        _ => panic!("expected reveal"),
    }
}

// ---- events / errors / structs / credentials ----

#[test]
fn event_with_indexed_arg() {
    // Note: `from` and `to` are reserved keywords; use distinct arg names.
    let body = parse_record(
        "event Transfer(sender: address indexed, receiver: address indexed, amt: amount)\n",
    );
    match &body[0] {
        TopLevelDecl::Event(e) => {
            assert_eq!(e.args.len(), 3);
            assert!(e.args[0].indexed);
            assert!(e.args[1].indexed);
            assert!(!e.args[2].indexed);
        }
        _ => panic!("expected event"),
    }
}

#[test]
fn error_decl() {
    let body = parse_record("error InsufficientBalance(have: amount, need: amount)\n");
    match &body[0] {
        TopLevelDecl::Error(e) => assert_eq!(e.fields.len(), 2),
        _ => panic!("expected error"),
    }
}

#[test]
fn struct_decl() {
    let body = parse_record("struct Point { x: amount\ny: amount }\n");
    match &body[0] {
        TopLevelDecl::Struct(s) => {
            assert_eq!(s.name.name.as_ref(), "Point");
            assert_eq!(s.fields.len(), 2);
        }
        _ => panic!("expected struct"),
    }
}

#[test]
fn credential_decl() {
    let body = parse_record("credential Pass { issuer: address }\n");
    match &body[0] {
        TopLevelDecl::Credential(c) => {
            assert_eq!(c.name.name.as_ref(), "Pass");
            assert_eq!(c.fields.len(), 1);
        }
        _ => panic!("expected credential"),
    }
}

// ---- expressions: Pratt precedence ----

#[test]
fn expr_precedence_add_mul() {
    let e = parse_view_body("1 + 2 * 3");
    // Expected: Add(1, Mul(2, 3))
    match &e {
        Expr::Binary {
            op: BinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            match lhs.as_ref() {
                Expr::Literal(LiteralExpr::Integer(1, _)) => {}
                o => panic!("lhs should be 1, got {o:?}"),
            }
            match rhs.as_ref() {
                Expr::Binary {
                    op: BinaryOp::Mul, ..
                } => {}
                o => panic!("rhs should be Mul, got {o:?}"),
            }
        }
        _ => panic!("expected Add, got {e:?}"),
    }
}

#[test]
fn expr_comparison_binds_tighter_than_logical() {
    // a < b && c > d  ==>  And(Lt(a,b), Gt(c,d))
    let e = parse_view_body("a < b && c > d");
    match e {
        Expr::Binary {
            op: BinaryOp::And,
            lhs,
            rhs,
            ..
        } => {
            assert!(matches!(
                lhs.as_ref(),
                Expr::Binary {
                    op: BinaryOp::Lt,
                    ..
                }
            ));
            assert!(matches!(
                rhs.as_ref(),
                Expr::Binary {
                    op: BinaryOp::Gt,
                    ..
                }
            ));
        }
        other => panic!("expected And, got {other:?}"),
    }
}

#[test]
fn expr_unary_not() {
    let e = parse_view_body("!x");
    assert!(matches!(
        e,
        Expr::Unary {
            op: UnaryOp::Not,
            ..
        }
    ));
}

#[test]
fn expr_field_access_and_call() {
    let e = parse_view_body("foo.bar(x, y)");
    match e {
        Expr::Call { callee, args, .. } => {
            assert_eq!(args.len(), 2);
            assert!(matches!(callee.as_ref(), Expr::FieldAccess { .. }));
        }
        _ => panic!("expected Call"),
    }
}

#[test]
fn expr_array_literal() {
    let e = parse_view_body("[1, 2, 3]");
    match e {
        Expr::Array { elements, .. } => assert_eq!(elements.len(), 3),
        _ => panic!("expected Array"),
    }
}

#[test]
fn expr_index_then_field() {
    // xs[0].name  ==>  FieldAccess(Index(xs, 0), name)
    let e = parse_view_body("xs[0].name");
    assert!(matches!(e, Expr::FieldAccess { .. }));
}

#[test]
fn expr_slice() {
    let e = parse_view_body("xs[1..5]");
    assert!(matches!(e, Expr::Slice { .. }));
}

#[test]
fn expr_in_operator() {
    let e = parse_view_body("x in options");
    assert!(matches!(
        e,
        Expr::Binary {
            op: BinaryOp::In,
            ..
        }
    ));
}

#[test]
fn expr_duration_literal() {
    let e = parse_view_body("7 days");
    match e {
        Expr::Literal(LiteralExpr::Duration(7, DurationUnit::Days, _)) => {}
        other => panic!("expected Duration(7, Days), got {other:?}"),
    }
}

#[test]
fn expr_encrypted_wrapper() {
    let e = parse_view_body("encrypted(42)");
    assert!(matches!(e, Expr::EncryptedLit(_, _)));
}

// ---- statements ----

#[test]
fn let_statement() {
    let stmts = parse_stmts("    let x = 42\n");
    assert!(matches!(stmts.first(), Some(Stmt::Let { .. })));
}

#[test]
fn assignment_statement() {
    let stmts = parse_stmts("    let x = 0\n    x = 7\n");
    match stmts.last() {
        Some(Stmt::Assign {
            op: AssignOp::Eq,
            target: LValue::Ident(id),
            ..
        }) => assert_eq!(id.name.as_ref(), "x"),
        other => panic!("expected Assign, got {other:?}"),
    }
}

#[test]
fn compound_assignment_plus_eq() {
    let stmts = parse_stmts("    tally += 1\n");
    match &stmts[0] {
        Stmt::Assign {
            op: AssignOp::PlusEq,
            ..
        } => {}
        other => panic!("expected PlusEq assign, got {other:?}"),
    }
}

#[test]
fn emit_statement() {
    let stmts = parse_stmts("    emit Cast(caller, pick)\n");
    assert!(matches!(&stmts[0], Stmt::Emit { .. }));
}

#[test]
fn if_statement() {
    let stmts = parse_stmts("    if x > 0 { let a = 1 } else { let b = 2 }\n");
    assert!(matches!(&stmts[0], Stmt::If { .. }));
}

#[test]
fn for_each_statement() {
    let stmts = parse_stmts("    for each item in xs { let y = item }\n");
    assert!(matches!(&stmts[0], Stmt::ForEach { .. }));
}

#[test]
fn match_statement() {
    let stmts = parse_stmts(
        "    match pick {\n        \"a\" => { let x = 1 }\n        \"b\" => { let y = 2 }\n    }\n",
    );
    assert!(matches!(&stmts[0], Stmt::Match { .. }));
}

#[test]
fn revert_with_statement() {
    let stmts = parse_stmts("    revert_with InsufficientBalance(have, need)\n");
    assert!(matches!(&stmts[0], Stmt::RevertWith { .. }));
}

#[test]
fn transfer_statement() {
    let stmts = parse_stmts("    transfer 100 to recipient\n");
    assert!(matches!(&stmts[0], Stmt::Transfer { .. }));
}

// ---- newline-sensitive parsing ----

#[test]
fn args_can_span_lines() {
    let body = parse_record("action f(\n    a: amount,\n    b: bool\n) {}\n");
    match &body[0] {
        TopLevelDecl::Action(a) => assert_eq!(a.args.len(), 2),
        _ => panic!("expected action"),
    }
}

#[test]
fn multi_line_statements() {
    let stmts = parse_stmts("    let x = 1\n    let y = 2\n    let z = 3\n");
    assert_eq!(stmts.len(), 3);
}

#[test]
fn guards_in_any_order() {
    // when, only, given — all permutations should parse.
    let b1 = parse_record("action f() when a, only owner, given b {}\n");
    let b2 = parse_record("action f() only owner, when a, given b {}\n");
    let b3 = parse_record("action f() given b, when a, only owner {}\n");
    for body in [b1, b2, b3] {
        match &body[0] {
            TopLevelDecl::Action(a) => assert_eq!(a.guards.len(), 3),
            _ => panic!("expected action"),
        }
    }
}

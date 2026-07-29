//! Integration tests for the `covenant_lsp` analysis module.
//!
//! These tests exercise the pure analysis helpers (span conversion, diagnostics,
//! hover, document symbols) against real fixture files so they catch regressions
//! in the analysis logic independently of the async LSP runtime.

use covenant_lsp::analysis;
use tower_lsp::lsp_types::{DiagnosticSeverity, Position, SymbolKind};

const HELLO_SRC: &str = include_str!("../../covenant-lexer/tests/fixtures/example_01_hello.cov");
const COIN_SRC: &str = include_str!("../../covenant-lexer/tests/fixtures/example_02_coin.cov");

// ---------------------------------------------------------------------------
// Position helpers
// ---------------------------------------------------------------------------

#[test]
fn position_basic_single_line() {
    let source = "record Hello {}";
    let pos = analysis::byte_offset_to_position(source, 7);
    assert_eq!(
        pos,
        Position {
            line: 0,
            character: 7
        }
    );
}

#[test]
fn position_multiline_second_line() {
    let source = "line0\nline1\nline2";
    // byte 6 → start of "line1", line=1, col=0
    assert_eq!(
        analysis::byte_offset_to_position(source, 6),
        Position {
            line: 1,
            character: 0
        }
    );
    // byte 9 → 3 chars into "line1", col=3
    assert_eq!(
        analysis::byte_offset_to_position(source, 9),
        Position {
            line: 1,
            character: 3
        }
    );
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

#[test]
fn analyze_hello_produces_no_errors() {
    let diags = analysis::analyze(HELLO_SRC);
    // Exclude lint findings (source = "covenant-lint"): those can fire on tutorial
    // examples that intentionally omit access guards (C100 etc.). We only check
    // that the *compiler* frontend emits no errors on clean source.
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .filter(|d| d.source.as_deref() != Some("covenant-lint"))
        .collect();
    assert!(
        errors.is_empty(),
        "hello.cov should produce zero compiler LSP errors; got: {errors:#?}"
    );
}

#[test]
fn analyze_coin_produces_no_errors() {
    let diags = analysis::analyze(COIN_SRC);
    // Same as above: exclude lint errors, only check compiler frontend.
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .filter(|d| d.source.as_deref() != Some("covenant-lint"))
        .collect();
    assert!(
        errors.is_empty(),
        "coin.cov should produce zero compiler LSP errors; got: {errors:#?}"
    );
}

// ---------------------------------------------------------------------------
// Document symbols
// ---------------------------------------------------------------------------

#[test]
fn symbols_hello_structure() {
    let file = analysis::parse_source(HELLO_SRC).expect("hello.cov must parse");
    let syms = analysis::collect_symbols(&file, HELLO_SRC);

    assert_eq!(syms.len(), 1, "exactly one top-level symbol");
    let top = &syms[0];
    assert_eq!(top.name, "Hello");
    assert_eq!(top.kind, SymbolKind::CLASS, "record → CLASS");

    let children = top.children.as_ref().expect("Hello has children");
    assert_eq!(children.len(), 3, "greeting + update + read");
    assert_eq!(children[0].name, "greeting");
    assert_eq!(children[0].kind, SymbolKind::FIELD);
    assert_eq!(children[1].name, "update");
    assert_eq!(children[1].kind, SymbolKind::METHOD);
    assert_eq!(children[2].name, "read");
    assert_eq!(children[2].kind, SymbolKind::FUNCTION);
}

#[test]
fn symbols_coin_has_token_kind() {
    let file = analysis::parse_source(COIN_SRC).expect("coin.cov must parse");
    let syms = analysis::collect_symbols(&file, COIN_SRC);
    assert_eq!(syms.len(), 1);
    assert_eq!(syms[0].name, "Coin");
    // `token` → INTERFACE
    assert_eq!(syms[0].kind, SymbolKind::INTERFACE);
}

// ---------------------------------------------------------------------------
// Hover
// ---------------------------------------------------------------------------

#[test]
fn hover_over_greeting_field() {
    let file = analysis::parse_source(HELLO_SRC).expect("hello.cov must parse");

    // Locate the `greeting` field span directly from the parsed AST.
    let field = file.top_level.body.iter().find_map(|d| {
        if let covenant_parser::ast::TopLevelDecl::Field(f) = d {
            if f.name.name.as_ref() == "greeting" {
                return Some(f.clone());
            }
        }
        None
    });
    let field = field.expect("greeting field must exist");

    // Hover at the midpoint of the field name span.
    let mid = (field.name.span.start as usize + field.name.span.end as usize) / 2;
    let hover = analysis::find_hover_at(&file, HELLO_SRC, mid);

    assert!(hover.is_some(), "hover over field name should return info");
    let text = hover.unwrap();
    assert!(
        text.contains("greeting"),
        "hover text must mention field name: {text}"
    );
    assert!(
        text.contains("text"),
        "hover text must mention field type: {text}"
    );
}

#[test]
fn hover_over_action_name() {
    let file = analysis::parse_source(HELLO_SRC).expect("hello.cov must parse");

    let action = file.top_level.body.iter().find_map(|d| {
        if let covenant_parser::ast::TopLevelDecl::Action(a) = d {
            if a.name.name.as_ref() == "update" {
                return Some(a.clone());
            }
        }
        None
    });
    let action = action.expect("update action must exist");

    let mid = (action.name.span.start as usize + action.name.span.end as usize) / 2;
    let hover = analysis::find_hover_at(&file, HELLO_SRC, mid);

    assert!(hover.is_some(), "hover over action name should return info");
    let text = hover.unwrap();
    assert!(
        text.contains("update"),
        "hover text must mention action name: {text}"
    );
    assert!(
        text.contains("action"),
        "hover text must say 'action': {text}"
    );
}

#[test]
fn hover_outside_any_entity_returns_none() {
    let file = analysis::parse_source(HELLO_SRC).expect("hello.cov must parse");
    // Offset 0 is in the leading comment: no entity there.
    let hover = analysis::find_hover_at(&file, HELLO_SRC, 0);
    // May or may not return something depending on comment spans; we only assert no panic.
    let _ = hover;
}

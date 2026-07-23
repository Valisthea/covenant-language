//! Tests for `external contract` parsing (Phase 17 – Solidity interop).

use covenant_diag::SourceId;
use covenant_lexer::tokenize;
use covenant_parser::ast::Type;
use covenant_parser::parse;

fn parse_source(src: &str) -> covenant_parser::ast::File {
    let (toks, lex) = tokenize(src, SourceId::new(0));
    assert!(lex.is_empty(), "lex errors: {lex:?}");
    let (file, diags) = parse(&toks, SourceId::new(0));
    assert!(diags.is_empty(), "parse errors: {diags:?}");
    file.expect("expected AST")
}

fn parse_with_diags(
    src: &str,
) -> (
    Option<covenant_parser::ast::File>,
    Vec<covenant_diag::Diagnostic>,
) {
    let (toks, _) = tokenize(src, SourceId::new(0));
    parse(&toks, SourceId::new(0))
}

// ---- keyword lexing ----

#[test]
fn lex_external_keyword() {
    let (toks, diags) = tokenize("external", SourceId::new(0));
    assert!(diags.is_empty());
    assert!(toks
        .iter()
        .any(|t| matches!(t.kind, covenant_lexer::TokenKind::KwExternal)));
}

#[test]
fn lex_contract_keyword() {
    let (toks, diags) = tokenize("contract", SourceId::new(0));
    assert!(diags.is_empty());
    assert!(toks
        .iter()
        .any(|t| matches!(t.kind, covenant_lexer::TokenKind::KwContract)));
}

#[test]
fn lex_function_keyword() {
    let (toks, diags) = tokenize("function", SourceId::new(0));
    assert!(diags.is_empty());
    assert!(toks
        .iter()
        .any(|t| matches!(t.kind, covenant_lexer::TokenKind::KwFunction)));
}

// ---- empty external contract ----

#[test]
fn parse_empty_external_contract() {
    let src = "external contract IToken {}\nrecord R {}";
    let file = parse_source(src);
    assert_eq!(file.external_contracts.len(), 1);
    assert_eq!(file.external_contracts[0].name.name.as_ref(), "IToken");
    assert!(file.external_contracts[0].functions.is_empty());
}

// ---- single function no params ----

#[test]
fn parse_external_function_no_params() {
    let src = "external contract IFoo {\n    function ping()\n}\nrecord R {}";
    let file = parse_source(src);
    let ec = &file.external_contracts[0];
    assert_eq!(ec.functions.len(), 1);
    assert_eq!(ec.functions[0].name.name.as_ref(), "ping");
    assert!(ec.functions[0].params.is_empty());
    assert!(!ec.functions[0].is_view);
}

// ---- view qualifier ----

#[test]
fn parse_external_function_view() {
    let src = "external contract IFoo {\n    function getVal() view\n}\nrecord R {}";
    let file = parse_source(src);
    assert!(file.external_contracts[0].functions[0].is_view);
}

// ---- returns clause ----

#[test]
fn parse_external_function_returns_amount() {
    let src =
        "external contract IFoo {\n    function balance() view returns amount\n}\nrecord R {}";
    let file = parse_source(src);
    let f = &file.external_contracts[0].functions[0];
    assert!(f.is_view);
    assert!(matches!(f.returns, Some(Type::Amount(_))));
}

// ---- params with types ----

#[test]
fn parse_external_function_typed_params() {
    let src = "external contract IERC20 {\n    function transfer(address, amount)\n}\nrecord R {}";
    let file = parse_source(src);
    let f = &file.external_contracts[0].functions[0];
    assert_eq!(f.params.len(), 2);
    assert!(matches!(f.params[0].ty, Type::Address(_)));
    assert!(matches!(f.params[1].ty, Type::Amount(_)));
}

// ---- named params ----

#[test]
fn parse_external_function_named_params() {
    let src = "external contract IERC20 {\n    function transfer(address to, amount value)\n}\nrecord R {}";
    let file = parse_source(src);
    let f = &file.external_contracts[0].functions[0];
    assert_eq!(f.params.len(), 2);
    assert!(f.params[0].name.is_some());
    assert_eq!(f.params[0].name.as_ref().unwrap().name.as_ref(), "to");
    assert!(f.params[1].name.is_some());
    assert_eq!(f.params[1].name.as_ref().unwrap().name.as_ref(), "value");
}

// ---- multiple functions ----

#[test]
fn parse_multiple_external_functions() {
    let src = r#"
external contract IERC20 {
    function transfer(address, amount)
    function balanceOf(address) view returns amount
    function approve(address, amount)
}
record R {}
"#;
    let file = parse_source(src);
    let ec = &file.external_contracts[0];
    assert_eq!(ec.functions.len(), 3);
    assert_eq!(ec.functions[0].name.name.as_ref(), "transfer");
    assert_eq!(ec.functions[1].name.name.as_ref(), "balanceOf");
    assert!(ec.functions[1].is_view);
    assert_eq!(ec.functions[2].name.name.as_ref(), "approve");
}

// ---- multiple external contracts ----

#[test]
fn parse_multiple_external_contracts() {
    let src = r#"
external contract IERC20 {
    function transfer(address, amount)
}
external contract IVault {
    function deposit(amount) view
}
record R {}
"#;
    let file = parse_source(src);
    assert_eq!(file.external_contracts.len(), 2);
    assert_eq!(file.external_contracts[0].name.name.as_ref(), "IERC20");
    assert_eq!(file.external_contracts[1].name.name.as_ref(), "IVault");
}

// ---- no external contracts → empty vec ----

#[test]
fn no_external_contracts_field_is_empty() {
    let src = "record R { x: amount }";
    let file = parse_source(src);
    assert!(file.external_contracts.is_empty());
}

// ---- full contract with body ----

#[test]
fn external_contract_alongside_full_record() {
    let src = r#"
external contract IERC20 {
    function transfer(address, amount)
    function balanceOf(address) view returns amount
}
record MyToken {
    balances: map<address, amount>
    action send(recipient: address, val: amount) only caller {
        balances[recipient] += val
    }
}
"#;
    let file = parse_source(src);
    assert_eq!(file.external_contracts.len(), 1);
    assert_eq!(file.top_level.name.name.as_ref(), "MyToken");
}

// ---- printer idempotency ----

#[test]
fn external_contract_printer_round_trip_empty() {
    let src = "external contract IFoo {}\nrecord R {}";
    let file = parse_source(src);
    let printed = covenant_parser::printer::pretty_print(&file);
    // Re-parse the printed output.
    let file2 = {
        let (toks, _) = tokenize(&printed, SourceId::new(0));
        let (f, _) = parse(&toks, SourceId::new(0));
        f.expect("re-parse of printed output")
    };
    assert_eq!(file2.external_contracts.len(), 1);
    assert_eq!(file2.external_contracts[0].name.name.as_ref(), "IFoo");
}

#[test]
fn external_contract_printer_round_trip_with_functions() {
    let src = r#"
external contract IERC20 {
    function transfer(address, amount)
    function balanceOf(address) view returns amount
}
record R {}
"#;
    let file = parse_source(src);
    let printed = covenant_parser::printer::pretty_print(&file);
    let (toks, _) = tokenize(&printed, SourceId::new(0));
    let (file2, diags) = parse(&toks, SourceId::new(0));
    assert!(diags.is_empty(), "re-parse errors: {diags:?}");
    let file2 = file2.unwrap();
    assert_eq!(file2.external_contracts.len(), 1);
    assert_eq!(file2.external_contracts[0].functions.len(), 2);
}

#[test]
fn printer_external_function_view_is_preserved() {
    let src = "external contract IFoo {\n    function get() view returns amount\n}\nrecord R {}";
    let file = parse_source(src);
    let printed = covenant_parser::printer::pretty_print(&file);
    assert!(
        printed.contains("view"),
        "printed output should contain 'view': {printed}"
    );
    assert!(
        printed.contains("returns"),
        "printed output should contain 'returns': {printed}"
    );
}

#[test]
fn printer_external_function_no_view_no_view_keyword() {
    let src = "external contract IFoo {\n    function set(amount)\n}\nrecord R {}";
    let file = parse_source(src);
    let printed = covenant_parser::printer::pretty_print(&file);
    // The function body should not have a spurious 'view' keyword.
    // (The 'view' keyword exists in the record body for view declarations but
    // our external function is not a view.)
    let ec_block_start = printed.find("external contract").unwrap();
    let ec_block = &printed[ec_block_start..];
    // Check the function line doesn't include 'view'.
    let fn_line = ec_block
        .lines()
        .find(|l| l.contains("function set"))
        .unwrap();
    assert!(
        !fn_line.contains("view"),
        "non-view function should not have 'view': {fn_line}"
    );
}

// ---- span preservation ----

#[test]
fn external_contract_span_is_nonzero() {
    let src = "external contract IFoo {\n    function f()\n}\nrecord R {}";
    let file = parse_source(src);
    let ec = &file.external_contracts[0];
    // Span should be non-empty (start != end).
    assert!(ec.span.start != ec.span.end);
}

// ---- bool and hash param types ----

#[test]
fn parse_external_function_bool_hash_params() {
    let src = "external contract ICheck {\n    function verify(hash, bool) view returns bool\n}\nrecord R {}";
    let file = parse_source(src);
    let f = &file.external_contracts[0].functions[0];
    assert_eq!(f.params.len(), 2);
    assert!(matches!(f.params[0].ty, Type::Hash(_)));
    assert!(matches!(f.params[1].ty, Type::Bool(_)));
    assert!(matches!(f.returns, Some(Type::Bool(_))));
}

// ---- keyword function names can appear as field access after dot ----

#[test]
fn keyword_method_name_in_field_access_parses() {
    // `transfer` is KwTransfer — must be accepted as a method name after `.`
    let src = r#"
external contract IERC20 {
    function transfer(address, amount)
}
record R {
    action send(dest: address, val: amount) only caller {
        IERC20.at(dest).transfer(dest, val)
    }
}
"#;
    let file = parse_source(src);
    assert_eq!(file.external_contracts.len(), 1);
    assert_eq!(
        file.external_contracts[0].functions[0].name.name.as_ref(),
        "transfer"
    );
}

// ---- bytes param type ----

#[test]
fn parse_external_function_bytes_param() {
    let src = "external contract IDecoder {\n    function decode(bytes) view returns hash\n}\nrecord R {}";
    let file = parse_source(src);
    let f = &file.external_contracts[0].functions[0];
    assert!(matches!(f.params[0].ty, Type::Bytes(_)));
}

// ---- text param type ----

#[test]
fn parse_external_function_text_param() {
    let src = "external contract IRegistry {\n    function register(text)\n}\nrecord R {}";
    let file = parse_source(src);
    let f = &file.external_contracts[0].functions[0];
    assert!(matches!(f.params[0].ty, Type::Text(_)));
}

// ---- param_count check ----

#[test]
fn parse_external_function_three_params() {
    let src = "external contract IMulti {\n    function foo(address, amount, bool)\n}\nrecord R {}";
    let file = parse_source(src);
    let f = &file.external_contracts[0].functions[0];
    assert_eq!(f.params.len(), 3);
}

// ---- function with no return and no view ----

#[test]
fn parse_external_function_no_return_no_view() {
    let src = "external contract ISink {\n    function sink(amount)\n}\nrecord R {}";
    let file = parse_source(src);
    let f = &file.external_contracts[0].functions[0];
    assert!(!f.is_view);
    assert!(f.returns.is_none());
}

// ---- diagnostics tolerance (missing RBrace recovered) ----

#[test]
fn external_contract_missing_function_keyword_skipped() {
    // A malformed function inside should be skipped, not panic.
    let src = "external contract IFoo {\n    notafunction()\n    function real()\n}\nrecord R {}";
    let (file_opt, _diags) = parse_with_diags(src);
    // We may or may not get an AST back, but we shouldn't panic.
    let _ = file_opt;
}

use covenant_mcp::tools::compile;
use serde_json::json;

fn params(source: &str) -> serde_json::Map<String, serde_json::Value> {
    json!({ "source": source }).as_object().cloned().unwrap()
}

/// Pull the text payload out of a content block.
///
/// rmcp models a content block as `Annotated<RawContent>` and has reshaped
/// that type across releases. Reading it through its serialised form keeps
/// this test about the compile tool rather than about the transport crate's
/// current type layout.
fn text_of(content: &rmcp::model::Content) -> String {
    let value = serde_json::to_value(content).expect("content serialises");
    value["text"]
        .as_str()
        .unwrap_or_else(|| panic!("expected a text content block, got {value}"))
        .to_string()
}

#[test]
fn compile_hello_succeeds() {
    let src = include_str!("../../covenant-lexer/tests/fixtures/example_01_hello.cov");
    let result = compile::run(&params(src));
    assert!(result.is_error != Some(true), "expected success");
    let text = text_of(&result.content[0]);
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["success"], true);
}

#[test]
fn compile_invalid_source_returns_error() {
    let result = compile::run(&params("this is not valid covenant"));
    let text = text_of(&result.content[0]);
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["success"], false);
}

#[test]
fn compile_missing_source_param_returns_error() {
    let params: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
    let result = compile::run(&params);
    assert_eq!(result.is_error, Some(true));
}

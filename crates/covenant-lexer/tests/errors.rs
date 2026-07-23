//! Targeted error-path coverage: every lexer diagnostic code E001-E010 should
//! be triggerable with a minimal input.

use covenant_diag::SourceId;
use covenant_lexer::{codes, tokenize};

fn has_code(src: &str, code: covenant_diag::DiagCode) -> bool {
    let (_, diags) = tokenize(src, SourceId::new(0));
    diags.iter().any(|d| d.code == code)
}

#[test]
fn e001_unexpected_character() {
    // `\0` isn't in any of the lexer's regexes.
    assert!(has_code("\u{0001}", codes::E001_UNEXPECTED));
}

#[test]
fn e002_unterminated_string() {
    assert!(has_code(
        r#""unterminated"#,
        codes::E002_UNTERMINATED_STRING
    ));
}

#[test]
fn e003_c_style_line_comment() {
    assert!(has_code("// reserved", codes::E003_C_LINE_COMMENT));
}

#[test]
fn e004_c_style_block_comment() {
    assert!(has_code("/* reserved */", codes::E004_C_BLOCK_COMMENT));
}

#[test]
fn e005_integer_overflow() {
    // 2^128
    assert!(has_code(
        "340282366920938463463374607431768211456",
        codes::E005_INT_OVERFLOW
    ));
}

#[test]
fn e006_hex_odd_digits() {
    assert!(has_code("0xabc", codes::E006_HEX_ODD));
}

#[test]
fn e007_hex_empty() {
    assert!(has_code("0x", codes::E007_HEX_EMPTY));
}

#[test]
fn e008_bare_cr() {
    assert!(has_code("a\rb", codes::E008_BARE_CR));
}

#[test]
fn e009_unterminated_nested_comment() {
    assert!(has_code("(* unterminated", codes::E009_UNTERMINATED_NESTED));
}

#[test]
fn e010_bad_escape_in_string() {
    assert!(has_code(r#""bad \x escape""#, codes::E010_BAD_ESCAPE));
}

#[test]
fn deep_nested_comment_closes_cleanly() {
    // Depth 4: (* (* (* (* *) *) *) *)
    let src = "(* a (* b (* c (* d *) c *) b *) a *)";
    let (_, diags) = tokenize(src, SourceId::new(0));
    assert!(diags.is_empty(), "expected clean parse, got {diags:?}");
}

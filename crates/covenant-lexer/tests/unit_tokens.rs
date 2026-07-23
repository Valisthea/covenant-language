//! Per-variant unit tests for the Covenant lexer.
//!
//! Each of these tests exercises a narrow feature: one keyword group, one
//! operator, one literal kind. Integration coverage against the full Codex
//! Basics examples lives in `codex_basics.rs`.

use covenant_diag::SourceId;
use covenant_lexer::{codes, tokenize, DurationUnit, TokenKind};

fn kinds(source: &str) -> Vec<TokenKind> {
    let (toks, diags) = tokenize(source, SourceId::new(0));
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
    toks.into_iter().map(|t| t.kind).collect()
}

fn first_non_trivial(source: &str) -> TokenKind {
    let ks = kinds(source);
    ks.into_iter()
        .find(|k| !matches!(k, TokenKind::Newline | TokenKind::Eof))
        .expect("at least one non-trivial token")
}

#[test]
fn eof_always_last() {
    let (toks, _) = tokenize("", SourceId::new(0));
    assert_eq!(toks.len(), 1);
    assert_eq!(toks[0].kind, TokenKind::Eof);
}

// ---------- Keyword groups ----------

#[test]
fn top_level_keywords() {
    for (s, expected) in [
        ("module", TokenKind::KwModule),
        ("record", TokenKind::KwRecord),
        ("token", TokenKind::KwToken),
        ("ballot", TokenKind::KwBallot),
        ("counter", TokenKind::KwCounter),
        ("board", TokenKind::KwBoard),
        ("market", TokenKind::KwMarket),
        ("vault", TokenKind::KwVault),
        ("registry", TokenKind::KwRegistry),
        ("bridge", TokenKind::KwBridge),
        ("ceremony", TokenKind::KwCeremony),
        ("test", TokenKind::KwTest),
    ] {
        assert_eq!(first_non_trivial(s), expected, "bad keyword: {s}");
    }
}

#[test]
fn privacy_keywords() {
    for (s, expected) in [
        ("public", TokenKind::KwPublic),
        ("private", TokenKind::KwPrivate),
        ("encrypted", TokenKind::KwEncrypted),
        ("sealed", TokenKind::KwSealed),
        ("hybrid", TokenKind::KwHybrid),
        ("confidential", TokenKind::KwConfidential),
    ] {
        assert_eq!(first_non_trivial(s), expected);
    }
}

#[test]
fn behavior_verbs() {
    for (s, expected) in [
        ("action", TokenKind::KwAction),
        ("view", TokenKind::KwView),
        ("reveal", TokenKind::KwReveal),
    ] {
        assert_eq!(first_non_trivial(s), expected);
    }
}

#[test]
fn guard_keywords() {
    for (s, expected) in [
        ("when", TokenKind::KwWhen),
        ("only", TokenKind::KwOnly),
        ("given", TokenKind::KwGiven),
    ] {
        assert_eq!(first_non_trivial(s), expected);
    }
}

#[test]
fn type_keywords() {
    for (s, expected) in [
        ("amount", TokenKind::KwAmount),
        ("time", TokenKind::KwTime),
        ("duration", TokenKind::KwDuration),
        ("hash", TokenKind::KwHash),
        ("text", TokenKind::KwTextTy),
        ("address", TokenKind::KwAddress),
        ("choice", TokenKind::KwChoice),
        ("pq_key", TokenKind::KwPqKey),
        ("bytes", TokenKind::KwBytes),
        ("bool", TokenKind::KwBool),
        ("ciphertext", TokenKind::KwCiphertext),
        ("map", TokenKind::KwMap),
        ("priority_queue", TokenKind::KwPriorityQueue),
        ("shares", TokenKind::KwShares),
    ] {
        assert_eq!(first_non_trivial(s), expected, "bad type kw: {s}");
    }
}

#[test]
fn declaration_keywords() {
    for (s, expected) in [
        ("field", TokenKind::KwField),
        ("event", TokenKind::KwEvent),
        ("error", TokenKind::KwError),
        ("struct", TokenKind::KwStruct),
        ("credential", TokenKind::KwCredential),
        ("version", TokenKind::KwVersion),
    ] {
        assert_eq!(first_non_trivial(s), expected);
    }
}

#[test]
fn control_flow_keywords() {
    for (s, expected) in [
        ("match", TokenKind::KwMatch),
        ("let", TokenKind::KwLet),
        ("return", TokenKind::KwReturn),
        ("for", TokenKind::KwFor),
        ("each", TokenKind::KwEach),
        ("in", TokenKind::KwIn),
        ("if", TokenKind::KwIf),
        ("else", TokenKind::KwElse),
        ("encrypted_when", TokenKind::KwEncryptedWhen),
        ("otherwise", TokenKind::KwOtherwise),
        ("try_action", TokenKind::KwTryAction),
        ("catch", TokenKind::KwCatch),
        ("revert_with", TokenKind::KwRevertWith),
    ] {
        assert_eq!(first_non_trivial(s), expected);
    }
}

#[test]
fn ceremony_lifecycle_keywords() {
    for (s, expected) in [
        ("on_destroy", TokenKind::KwOnDestroy),
        ("migrate", TokenKind::KwMigrate),
        ("from", TokenKind::KwFrom),
        ("to", TokenKind::KwTo),
        ("anchored_on", TokenKind::KwAnchoredOn),
        ("upgradeable_by", TokenKind::KwUpgradeableBy),
        ("selective_disclosure", TokenKind::KwSelectiveDisclosure),
        ("using", TokenKind::KwUsing),
        ("as", TokenKind::KwAs),
        ("returns", TokenKind::KwReturns),
        ("import", TokenKind::KwImport),
    ] {
        assert_eq!(first_non_trivial(s), expected);
    }
}

#[test]
fn statement_verbs_and_qualifiers() {
    for (s, expected) in [
        ("emit", TokenKind::KwEmit),
        ("transfer", TokenKind::KwTransfer),
        ("append", TokenKind::KwAppend),
        ("discard", TokenKind::KwDiscard),
        ("delete", TokenKind::KwDelete),
        ("verified_by", TokenKind::KwVerifiedBy),
        ("pq_signed", TokenKind::KwPqSigned),
        ("vdf_locked", TokenKind::KwVdfLocked),
    ] {
        assert_eq!(first_non_trivial(s), expected);
    }
}

#[test]
fn boolean_literals() {
    assert_eq!(first_non_trivial("true"), TokenKind::True);
    assert_eq!(first_non_trivial("false"), TokenKind::False);
}

// ---------- Punctuation and operators ----------

#[test]
fn punctuation_tokens() {
    let expected = vec![
        TokenKind::LParen,
        TokenKind::RParen,
        TokenKind::LBracket,
        TokenKind::RBracket,
        TokenKind::LBrace,
        TokenKind::RBrace,
        TokenKind::Comma,
        TokenKind::Semicolon,
        TokenKind::ColonColon,
        TokenKind::Colon,
        TokenKind::DotDot,
        TokenKind::Dot,
        TokenKind::Arrow,
        TokenKind::FatArrow,
        TokenKind::At,
        TokenKind::Eof,
    ];
    let got = kinds("( ) [ ] { } , ; :: : .. . -> => @");
    assert_eq!(got, expected);
}

#[test]
fn operator_tokens() {
    let expected = vec![
        TokenKind::Plus,
        TokenKind::Minus,
        TokenKind::Star,
        TokenKind::Slash,
        TokenKind::Percent,
        TokenKind::PlusEq,
        TokenKind::MinusEq,
        TokenKind::StarEq,
        TokenKind::SlashEq,
        TokenKind::PercentEq,
        TokenKind::Eq,
        TokenKind::EqEq,
        TokenKind::NotEq,
        TokenKind::Lt,
        TokenKind::LtEq,
        TokenKind::Gt,
        TokenKind::GtEq,
        TokenKind::AmpAmp,
        TokenKind::PipePipe,
        TokenKind::Bang,
        TokenKind::Amp,
        TokenKind::Pipe,
        TokenKind::Caret,
        TokenKind::Tilde,
        TokenKind::ShiftLeft,
        TokenKind::ShiftRight,
        TokenKind::PlusPlus,
        TokenKind::Eof,
    ];
    let got = kinds("+ - * / % += -= *= /= %= = == != < <= > >= && || ! & | ^ ~ << >> ++");
    assert_eq!(got, expected);
}

// ---------- Integer literals ----------

#[test]
fn integer_zero() {
    assert_eq!(first_non_trivial("0"), TokenKind::Integer(0));
}

#[test]
fn integer_forty_two() {
    assert_eq!(first_non_trivial("42"), TokenKind::Integer(42));
}

#[test]
fn integer_underscores() {
    assert_eq!(
        first_non_trivial("1_000_000"),
        TokenKind::Integer(1_000_000)
    );
}

#[test]
fn integer_u64_max() {
    assert_eq!(
        first_non_trivial("18446744073709551615"),
        TokenKind::Integer(u64::MAX as u128)
    );
}

#[test]
fn integer_overflow_emits_e005() {
    // 2^128 — overflows u128 by 1.
    let src = "340282366920938463463374607431768211456";
    let (toks, diags) = tokenize(src, SourceId::new(0));
    assert!(toks.iter().any(|t| t.kind == TokenKind::Error));
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].code, codes::E005_INT_OVERFLOW);
}

// ---------- Hex literals ----------

#[test]
fn hex_deadbeef() {
    let t = first_non_trivial("0xdeadbeef");
    let expected: Box<[u8]> = Box::from([0xDE, 0xAD, 0xBE, 0xEF].as_slice());
    assert_eq!(t, TokenKind::HexBytes(expected));
}

#[test]
fn hex_double_zero() {
    let t = first_non_trivial("0x00");
    let expected: Box<[u8]> = Box::from([0x00].as_slice());
    assert_eq!(t, TokenKind::HexBytes(expected));
}

#[test]
fn hex_odd_length_emits_e006() {
    let (_, diags) = tokenize("0xabc", SourceId::new(0));
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].code, codes::E006_HEX_ODD);
}

#[test]
fn hex_empty_emits_e007() {
    let (_, diags) = tokenize("0x", SourceId::new(0));
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].code, codes::E007_HEX_EMPTY);
}

// ---------- Text literals ----------

#[test]
fn text_simple() {
    let t = first_non_trivial(r#""hello""#);
    assert_eq!(t, TokenKind::Text("hello".into()));
}

#[test]
fn text_with_escaped_quote() {
    let t = first_non_trivial(r#""escape \"quoted\"""#);
    assert_eq!(t, TokenKind::Text("escape \"quoted\"".into()));
}

#[test]
fn text_with_newline_escape() {
    let t = first_non_trivial(r#""multi-line\nstring""#);
    assert_eq!(t, TokenKind::Text("multi-line\nstring".into()));
}

#[test]
fn text_unicode_escape() {
    let t = first_non_trivial(r#""snow \u{2603}!""#);
    assert_eq!(t, TokenKind::Text("snow \u{2603}!".into()));
}

#[test]
fn text_unterminated_emits_e002() {
    let (_, diags) = tokenize(r#""unclosed"#, SourceId::new(0));
    assert!(diags
        .iter()
        .any(|d| d.code == codes::E002_UNTERMINATED_STRING));
}

#[test]
fn text_bad_escape_emits_e010() {
    let (_, diags) = tokenize(r#""bad \x escape""#, SourceId::new(0));
    assert!(diags.iter().any(|d| d.code == codes::E010_BAD_ESCAPE));
}

// ---------- Duration literals ----------

#[test]
fn duration_seven_days() {
    assert_eq!(
        first_non_trivial("7 days"),
        TokenKind::Duration(7, DurationUnit::Days)
    );
}

#[test]
fn duration_one_hour_singular() {
    assert_eq!(
        first_non_trivial("1 hour"),
        TokenKind::Duration(1, DurationUnit::Hours)
    );
}

#[test]
fn duration_one_hours_plural_on_singular() {
    assert_eq!(
        first_non_trivial("1 hours"),
        TokenKind::Duration(1, DurationUnit::Hours)
    );
}

#[test]
fn duration_one_week_singular_on_plural() {
    assert_eq!(
        first_non_trivial("1 week"),
        TokenKind::Duration(1, DurationUnit::Weeks)
    );
}

#[test]
fn duration_fifty_two_weeks() {
    assert_eq!(
        first_non_trivial("52 weeks"),
        TokenKind::Duration(52, DurationUnit::Weeks)
    );
}

#[test]
fn non_unit_keeps_integer_and_ident() {
    // `42 foo` should NOT fuse — it's Integer then Ident.
    let ks = kinds("42 foo");
    assert_eq!(ks[0], TokenKind::Integer(42));
    assert_eq!(ks[1], TokenKind::Ident("foo".into()));
}

// ---------- Comments ----------

#[test]
fn line_comment_skipped() {
    let ks = kinds("-- this is a comment");
    assert_eq!(ks, vec![TokenKind::Eof]);
}

#[test]
fn nested_multiline_comment_skipped() {
    let ks = kinds("(* outer (* inner (* deepest *) inner *) outer *)");
    assert_eq!(ks, vec![TokenKind::Eof]);
}

#[test]
fn nested_comment_preserves_surrounding_tokens() {
    let ks = kinds("a (* comment *) b");
    assert_eq!(
        ks,
        vec![
            TokenKind::Ident("a".into()),
            TokenKind::Ident("b".into()),
            TokenKind::Eof
        ]
    );
}

#[test]
fn reserved_double_slash_emits_e003() {
    let (_, diags) = tokenize("// not allowed", SourceId::new(0));
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].code, codes::E003_C_LINE_COMMENT);
}

#[test]
fn reserved_slash_star_emits_e004() {
    let (_, diags) = tokenize("/* not allowed */", SourceId::new(0));
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].code, codes::E004_C_BLOCK_COMMENT);
}

// ---------- Newlines and whitespace ----------

#[test]
fn consecutive_newlines_collapse() {
    let ks = kinds("a\n\n\nb");
    assert_eq!(
        ks,
        vec![
            TokenKind::Ident("a".into()),
            TokenKind::Newline,
            TokenKind::Ident("b".into()),
            TokenKind::Eof
        ]
    );
}

#[test]
fn crlf_is_one_newline() {
    let ks = kinds("a\r\nb");
    assert_eq!(
        ks,
        vec![
            TokenKind::Ident("a".into()),
            TokenKind::Newline,
            TokenKind::Ident("b".into()),
            TokenKind::Eof
        ]
    );
}

#[test]
fn bare_cr_emits_e008() {
    let (_, diags) = tokenize("a\rb", SourceId::new(0));
    assert!(diags.iter().any(|d| d.code == codes::E008_BARE_CR));
}

// ---------- Underscore vs. identifier ----------

#[test]
fn bare_underscore_is_underscore_token() {
    assert_eq!(first_non_trivial("_"), TokenKind::Underscore);
}

#[test]
fn leading_underscore_ident() {
    assert_eq!(first_non_trivial("_foo"), TokenKind::Ident("_foo".into()));
}

#[test]
fn underscore_then_ident() {
    let ks = kinds("_ foo");
    assert_eq!(ks[0], TokenKind::Underscore);
    assert_eq!(ks[1], TokenKind::Ident("foo".into()));
}

// ---------- User identifiers ----------

#[test]
fn user_identifier_snake_case() {
    assert_eq!(
        first_non_trivial("total_supply"),
        TokenKind::Ident("total_supply".into())
    );
}

#[test]
fn user_identifier_pascal_case() {
    assert_eq!(
        first_non_trivial("SealedAuction"),
        TokenKind::Ident("SealedAuction".into())
    );
}

#[test]
fn keyword_is_not_an_ident() {
    // `caller` is a context-dependent special identifier — NOT reserved at the
    // lexer level, so it should come through as an ordinary Ident.
    assert_eq!(
        first_non_trivial("caller"),
        TokenKind::Ident("caller".into())
    );
}

// ---------- Spans ----------

#[test]
fn integer_span_covers_literal() {
    let (toks, _) = tokenize("  42  ", SourceId::new(0));
    let first = &toks[0];
    assert_eq!(first.kind, TokenKind::Integer(42));
    assert_eq!(first.span.start, 2);
    assert_eq!(first.span.end, 4);
}

#[test]
fn duration_span_covers_both_parts() {
    let (toks, _) = tokenize("7 days", SourceId::new(0));
    let first = &toks[0];
    assert!(matches!(
        first.kind,
        TokenKind::Duration(7, DurationUnit::Days)
    ));
    assert_eq!(first.span.start, 0);
    assert_eq!(first.span.end, 6);
}

#[test]
fn two_char_operator_spans_two_bytes() {
    let (toks, _) = tokenize("==", SourceId::new(0));
    assert_eq!(toks[0].kind, TokenKind::EqEq);
    assert_eq!(toks[0].span.end - toks[0].span.start, 2);
}

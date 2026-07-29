//! Pure analysis helpers: span/position conversion, diagnostic mapping, hover, and symbol extraction.
//!
//! All functions here are synchronous and dependency-free w.r.t. the LSP runtime, they
//! accept source text and AST nodes and return LSP-typed values, making them trivially testable.

use covenant_diag::{DiagnosticLevel, SourceId};
use covenant_lexer::tokenize;
use covenant_parser::{ast, parse};
use tower_lsp::lsp_types::{
    DiagnosticSeverity, DocumentSymbol, NumberOrString, Position, Range, SymbolKind,
};

// ---------------------------------------------------------------------------
// Position helpers
// ---------------------------------------------------------------------------

/// Convert a byte offset into the source string to an LSP `Position` (0-based line + character).
pub fn byte_offset_to_position(source: &str, offset: usize) -> Position {
    let offset = offset.min(source.len());
    let prefix = &source[..offset];
    let line = prefix.bytes().filter(|&b| b == b'\n').count() as u32;
    let col = prefix.rfind('\n').map(|i| offset - i - 1).unwrap_or(offset) as u32;
    Position {
        line,
        character: col,
    }
}

/// Convert an LSP `Position` back to a byte offset.
pub fn position_to_byte_offset(source: &str, pos: Position) -> usize {
    let mut line = 0u32;
    let mut line_start = 0usize;
    for (i, c) in source.char_indices() {
        if line == pos.line {
            // Advance `pos.character` code units from the line start.
            let col_bytes: usize = source[i..]
                .char_indices()
                .take(pos.character as usize)
                .last()
                .map(|(j, ch)| j + ch.len_utf8())
                .unwrap_or(0);
            return i + col_bytes;
        }
        if c == '\n' {
            line += 1;
            line_start = i + 1;
        }
    }
    // If pos.line is beyond the last newline, return the line_start offset.
    line_start
}

// ---------------------------------------------------------------------------
// Diagnostic conversion
// ---------------------------------------------------------------------------

/// Convert a compiler `Diagnostic` to an LSP `Diagnostic`.
pub fn diag_to_lsp(
    d: &covenant_diag::Diagnostic,
    source: &str,
) -> tower_lsp::lsp_types::Diagnostic {
    let start = byte_offset_to_position(source, d.span.start as usize);
    let end = byte_offset_to_position(source, d.span.end as usize);
    let severity = match d.level {
        DiagnosticLevel::Error => DiagnosticSeverity::ERROR,
        DiagnosticLevel::Warning => DiagnosticSeverity::WARNING,
        DiagnosticLevel::Note => DiagnosticSeverity::INFORMATION,
    };
    let mut lsp_diag = tower_lsp::lsp_types::Diagnostic {
        range: Range { start, end },
        severity: Some(severity),
        code: Some(NumberOrString::String(d.code.to_string())),
        message: d.message.clone(),
        source: Some("covenant".to_string()),
        ..Default::default()
    };
    if let Some(help) = &d.help {
        lsp_diag.message = format!("{}\n\nhelp: {}", d.message, help);
    }
    lsp_diag
}

/// Run the frontend pipeline on `source` and return LSP diagnostics.
///
/// If the frontend produces no errors, the security linter is also run and its
/// findings are appended (E4xx / W0xx codes) so they appear as squiggles in
/// the editor alongside parse/type errors.
pub fn analyze(source: &str) -> Vec<tower_lsp::lsp_types::Diagnostic> {
    // `check_deep` runs the whole pipeline, not just the frontend, so the
    // fail-loud diagnostics that live in IR lowering and codegen (E424 `max`,
    // E425 `map.length`, E519 `x / 0`, E521 over-long text) show up as
    // squiggles instead of only failing at build time.
    let full_diags = covenant_driver::check_deep(source, SourceId::new(0));
    let has_errors = full_diags.iter().any(|d| d.level == DiagnosticLevel::Error);

    let mut result: Vec<_> = full_diags.iter().map(|d| diag_to_lsp(d, source)).collect();

    if !has_errors {
        let findings = covenant_lint::lint_source(source, SourceId::new(0));
        result.extend(findings.iter().map(|f| finding_to_lsp(f, source)));
    }

    result
}

/// Convert a lint `Finding` to an LSP `Diagnostic`.
fn finding_to_lsp(
    f: &covenant_lint::framework::Finding,
    source: &str,
) -> tower_lsp::lsp_types::Diagnostic {
    use covenant_lint::framework::Severity;

    let start = byte_offset_to_position(source, f.span.start as usize);
    let end = byte_offset_to_position(source, f.span.end as usize);
    let severity = match f.severity {
        Severity::Critical => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
        Severity::Info => DiagnosticSeverity::HINT,
    };
    let message = if let Some(help) = f.help {
        format!("{}\n\nhelp: {}", f.message, help)
    } else {
        f.message.clone()
    };
    tower_lsp::lsp_types::Diagnostic {
        range: Range { start, end },
        severity: Some(severity),
        code: Some(NumberOrString::String(f.detector_code.to_string())),
        message,
        source: Some("covenant-lint".to_string()),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// AST helpers
// ---------------------------------------------------------------------------

/// Parse `source` and return the AST `File`, or `None` if parsing failed entirely.
pub fn parse_source(source: &str) -> Option<ast::File> {
    let src_id = SourceId::new(0);
    let (tokens, _) = tokenize(source, src_id);
    let (file, _) = parse(&tokens, src_id);
    file
}

/// Format a `Type` as a human-readable string.
pub fn type_to_string(ty: &ast::Type) -> String {
    match ty {
        ast::Type::Amount(_) => "amount".to_string(),
        ast::Type::Time(_) => "time".to_string(),
        ast::Type::Duration(_) => "duration".to_string(),
        ast::Type::Hash(_) => "hash".to_string(),
        ast::Type::Text(_) => "text".to_string(),
        ast::Type::Address(_) => "address".to_string(),
        ast::Type::Bool(_) => "bool".to_string(),
        ast::Type::Bytes(_) => "bytes".to_string(),
        ast::Type::PqKey(_) => "pq_key".to_string(),
        ast::Type::Encrypted(inner, _) => format!("encrypted<{}>", type_to_string(inner)),
        ast::Type::List(inner, _) => format!("list<{}>", type_to_string(inner)),
        ast::Type::Map(k, v, _) => {
            format!("map<{}, {}>", type_to_string(k), type_to_string(v))
        }
        ast::Type::PriorityQueue(k, v, _, _) => {
            format!(
                "priority_queue<{}, {}>",
                type_to_string(k),
                type_to_string(v)
            )
        }
        ast::Type::Shares { n, k, .. } => format!("shares({n}/{k})"),
        ast::Type::Choice(_, vals) => {
            let names: Vec<&str> = vals.iter().map(|v| v.name.name.as_ref()).collect();
            format!("choice<{}>", names.join(" | "))
        }
        ast::Type::User(ident) => ident.name.to_string(),
    }
}

fn span_contains(span: covenant_diag::Span, offset: usize) -> bool {
    offset >= span.start as usize && offset < span.end as usize
}

// ---------------------------------------------------------------------------
// Document symbols
// ---------------------------------------------------------------------------

/// Extract LSP `DocumentSymbol` tree from a parsed file.
#[allow(deprecated)]
pub fn collect_symbols(file: &ast::File, source: &str) -> Vec<DocumentSymbol> {
    let construct = &file.top_level;
    let kind = construct_to_symbol_kind(construct.keyword);

    let outer = span_to_range(construct.span, source);
    let name_range = span_to_range(construct.name.span, source);

    let children: Vec<DocumentSymbol> = construct
        .body
        .iter()
        .filter_map(|decl| decl_to_symbol(decl, source))
        .collect();

    let detail = construct_kind_label(construct.keyword).to_string();

    vec![DocumentSymbol {
        name: construct.name.name.to_string(),
        detail: Some(detail),
        kind,
        tags: None,
        deprecated: None,
        range: outer,
        selection_range: name_range,
        children: if children.is_empty() {
            None
        } else {
            Some(children)
        },
    }]
}

#[allow(deprecated)]
fn decl_to_symbol(decl: &ast::TopLevelDecl, source: &str) -> Option<DocumentSymbol> {
    match decl {
        ast::TopLevelDecl::Field(f) => Some(DocumentSymbol {
            name: f.name.name.to_string(),
            detail: Some(type_to_string(&f.ty)),
            kind: SymbolKind::FIELD,
            tags: None,
            deprecated: None,
            range: span_to_range(f.span, source),
            selection_range: span_to_range(f.name.span, source),
            children: None,
        }),
        ast::TopLevelDecl::Action(a) => {
            let params: Vec<String> = a
                .args
                .iter()
                .map(|arg| format!("{}: {}", arg.name.name, type_to_string(&arg.ty)))
                .collect();
            Some(DocumentSymbol {
                name: a.name.name.to_string(),
                detail: Some(format!("action({})", params.join(", "))),
                kind: SymbolKind::METHOD,
                tags: None,
                deprecated: None,
                range: span_to_range(a.span, source),
                selection_range: span_to_range(a.name.span, source),
                children: None,
            })
        }
        ast::TopLevelDecl::View(v) => {
            let ret = v
                .returns
                .as_ref()
                .map(|t| format!(" → {}", type_to_string(t)))
                .unwrap_or_default();
            Some(DocumentSymbol {
                name: v.name.name.to_string(),
                detail: Some(format!("view{ret}")),
                kind: SymbolKind::FUNCTION,
                tags: None,
                deprecated: None,
                range: span_to_range(v.span, source),
                selection_range: span_to_range(v.name.span, source),
                children: None,
            })
        }
        ast::TopLevelDecl::Event(e) => Some(DocumentSymbol {
            name: e.name.name.to_string(),
            detail: Some("event".to_string()),
            kind: SymbolKind::EVENT,
            tags: None,
            deprecated: None,
            range: span_to_range(e.span, source),
            selection_range: span_to_range(e.name.span, source),
            children: None,
        }),
        ast::TopLevelDecl::Error(e) => Some(DocumentSymbol {
            name: e.name.name.to_string(),
            detail: Some("error".to_string()),
            kind: SymbolKind::STRUCT,
            tags: None,
            deprecated: None,
            range: span_to_range(e.span, source),
            selection_range: span_to_range(e.name.span, source),
            children: None,
        }),
        ast::TopLevelDecl::Struct(s) => Some(DocumentSymbol {
            name: s.name.name.to_string(),
            detail: Some("struct".to_string()),
            kind: SymbolKind::STRUCT,
            tags: None,
            deprecated: None,
            range: span_to_range(s.span, source),
            selection_range: span_to_range(s.name.span, source),
            children: None,
        }),
        _ => None,
    }
}

fn span_to_range(span: covenant_diag::Span, source: &str) -> Range {
    Range {
        start: byte_offset_to_position(source, span.start as usize),
        end: byte_offset_to_position(source, span.end as usize),
    }
}

fn construct_to_symbol_kind(kind: ast::ConstructKind) -> SymbolKind {
    match kind {
        ast::ConstructKind::Module => SymbolKind::MODULE,
        ast::ConstructKind::Record => SymbolKind::CLASS,
        ast::ConstructKind::Token => SymbolKind::INTERFACE,
        ast::ConstructKind::Nft => SymbolKind::INTERFACE,
        ast::ConstructKind::Ballot
        | ast::ConstructKind::Counter
        | ast::ConstructKind::Board
        | ast::ConstructKind::Market
        | ast::ConstructKind::Vault
        | ast::ConstructKind::Registry
        | ast::ConstructKind::Bridge
        | ast::ConstructKind::Ceremony
        | ast::ConstructKind::Test => SymbolKind::CLASS,
    }
}

fn construct_kind_label(kind: ast::ConstructKind) -> &'static str {
    match kind {
        ast::ConstructKind::Module => "module",
        ast::ConstructKind::Record => "record",
        ast::ConstructKind::Token => "token",
        ast::ConstructKind::Ballot => "ballot",
        ast::ConstructKind::Counter => "counter",
        ast::ConstructKind::Board => "board",
        ast::ConstructKind::Market => "market",
        ast::ConstructKind::Vault => "vault",
        ast::ConstructKind::Registry => "registry",
        ast::ConstructKind::Bridge => "bridge",
        ast::ConstructKind::Ceremony => "ceremony",
        ast::ConstructKind::Nft => "nft",
        ast::ConstructKind::Test => "test",
    }
}

// ---------------------------------------------------------------------------
// Hover
// ---------------------------------------------------------------------------

/// Find hover text for the entity at `offset` bytes into `source`.
///
/// Returns `None` when no entity is found at the given position.
pub fn find_hover_at(file: &ast::File, _source: &str, offset: usize) -> Option<String> {
    let construct = &file.top_level;

    if span_contains(construct.name.span, offset) {
        let kind = construct_kind_label(construct.keyword);
        return Some(format!("**{}** `{}`", kind, construct.name.name));
    }

    for decl in &construct.body {
        if let Some(text) = hover_for_decl(decl, offset) {
            return Some(text);
        }
    }

    None
}

fn hover_for_decl(decl: &ast::TopLevelDecl, offset: usize) -> Option<String> {
    match decl {
        ast::TopLevelDecl::Field(f) if span_contains(f.span, offset) => Some(format!(
            "**field** `{}`: `{}`",
            f.name.name,
            type_to_string(&f.ty)
        )),
        ast::TopLevelDecl::Action(a) if span_contains(a.span, offset) => {
            let params: Vec<String> = a
                .args
                .iter()
                .map(|arg| format!("{}: {}", arg.name.name, type_to_string(&arg.ty)))
                .collect();
            Some(format!(
                "**action** `{}({})`",
                a.name.name,
                params.join(", ")
            ))
        }
        ast::TopLevelDecl::View(v) if span_contains(v.span, offset) => {
            let ret = v
                .returns
                .as_ref()
                .map(|t| format!(" → {}", type_to_string(t)))
                .unwrap_or_default();
            Some(format!("**view** `{}{}`", v.name.name, ret))
        }
        ast::TopLevelDecl::Event(e) if span_contains(e.span, offset) => {
            Some(format!("**event** `{}`", e.name.name))
        }
        ast::TopLevelDecl::Error(e) if span_contains(e.span, offset) => {
            Some(format!("**error** `{}`", e.name.name))
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Goto-definition (V0.9 Sprint 38 Phase 38.2)
// ---------------------------------------------------------------------------

/// Result of a goto-definition lookup.
///
/// Returns the **byte span** of the definition's name within the file.
/// The LSP backend converts this to an `lsp_types::Location` by attaching
/// the document URI.
#[derive(Debug, Clone, Copy)]
pub struct DefinitionTarget {
    pub start: u32,
    pub end: u32,
}

impl DefinitionTarget {
    fn from_span(span: covenant_diag::Span) -> Self {
        Self {
            start: span.start,
            end: span.end,
        }
    }
}

/// Find the definition site for the identifier at byte `offset`.
///
/// V0.9 Sprint 38 scope: identifier-name resolution against top-level
/// declarations of the current file. Supports :
///   - field references → field declaration
///   - action / view calls → action / view declaration
///   - event references in `emit Foo(...)` → event declaration
///   - error references in `revert_with Foo(...)` → error declaration
///
/// Limitations (deferred to V1.0) :
///   - local variables (action params, `let` bindings): return None
///   - cross-file imports: single-file V0.9 has no imports
///   - scope-aware shadowing: V0.9 grammar disallows shadowing anyway
///
/// Returns `None` when :
///   - The cursor is not on an identifier
///   - The identifier is the construct's own name (already a definition)
///   - No matching top-level decl exists for the identifier
pub fn find_definition_at(
    file: &ast::File,
    source: &str,
    offset: usize,
) -> Option<DefinitionTarget> {
    // Step 1: extract the identifier word at the offset.
    let word = identifier_at(source, offset)?;

    // Step 2: skip if the cursor is on the construct name itself
    // (it IS its own definition; goto-definition would be a no-op).
    let construct = &file.top_level;
    if span_contains(construct.name.span, offset) {
        return None;
    }

    // Step 3: skip if the cursor is on a top-level decl's NAME span
    // (it's already at the definition; goto-definition is a no-op).
    for decl in &construct.body {
        if let Some(name_span) = decl_name_span(decl) {
            if span_contains(name_span, offset) {
                return None;
            }
        }
    }

    // Step 4: look for a top-level decl whose name matches the cursor word.
    for decl in &construct.body {
        if let Some(target) = decl_definition_for_name(decl, &word) {
            return Some(target);
        }
    }

    None
}

/// Extract the identifier (alphanumeric + underscore) that contains the
/// byte offset. Returns None if the offset is on whitespace or punctuation.
fn identifier_at(source: &str, offset: usize) -> Option<String> {
    if offset >= source.len() {
        return None;
    }
    let bytes = source.as_bytes();
    let is_id_char = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    if !is_id_char(bytes[offset]) {
        return None;
    }
    // Walk left to find the start.
    let mut start = offset;
    while start > 0 && is_id_char(bytes[start - 1]) {
        start -= 1;
    }
    // Walk right to find the end.
    let mut end = offset;
    while end < bytes.len() && is_id_char(bytes[end]) {
        end += 1;
    }
    // Identifiers can't start with a digit: if so, this is a numeric
    // literal, not an identifier.
    if bytes[start].is_ascii_digit() {
        return None;
    }
    Some(source[start..end].to_string())
}

fn decl_name_span(decl: &ast::TopLevelDecl) -> Option<covenant_diag::Span> {
    match decl {
        ast::TopLevelDecl::Field(f) => Some(f.name.span),
        ast::TopLevelDecl::Action(a) => Some(a.name.span),
        ast::TopLevelDecl::View(v) => Some(v.name.span),
        ast::TopLevelDecl::Event(e) => Some(e.name.span),
        ast::TopLevelDecl::Error(e) => Some(e.name.span),
        _ => None,
    }
}

fn decl_definition_for_name(decl: &ast::TopLevelDecl, name: &str) -> Option<DefinitionTarget> {
    let (decl_name, name_span) = match decl {
        ast::TopLevelDecl::Field(f) => (f.name.name.as_ref(), f.name.span),
        ast::TopLevelDecl::Action(a) => (a.name.name.as_ref(), a.name.span),
        ast::TopLevelDecl::View(v) => (v.name.name.as_ref(), v.name.span),
        ast::TopLevelDecl::Event(e) => (e.name.name.as_ref(), e.name.span),
        ast::TopLevelDecl::Error(e) => (e.name.name.as_ref(), e.name.span),
        _ => return None,
    };
    if decl_name == name {
        Some(DefinitionTarget::from_span(name_span))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO_SRC: &str =
        include_str!("../../covenant-lexer/tests/fixtures/example_01_hello.cov");

    #[test]
    fn position_round_trip_single_line() {
        let source = "hello world";
        let pos = byte_offset_to_position(source, 6);
        assert_eq!(
            pos,
            Position {
                line: 0,
                character: 6
            }
        );
        let back = position_to_byte_offset(source, pos);
        assert_eq!(back, 6);
    }

    #[test]
    fn position_multiline() {
        let source = "line0\nline1\nline2";
        // offset 6 is start of "line1"
        let pos = byte_offset_to_position(source, 6);
        assert_eq!(
            pos,
            Position {
                line: 1,
                character: 0
            }
        );
        let pos2 = byte_offset_to_position(source, 9);
        assert_eq!(
            pos2,
            Position {
                line: 1,
                character: 3
            }
        );
    }

    #[test]
    fn analyze_clean_source_returns_no_errors() {
        let diags = analyze(HELLO_SRC);
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
            .filter(|d| d.source.as_deref() != Some("covenant-lint"))
            .collect();
        assert!(
            errors.is_empty(),
            "clean source should yield no LSP errors: {errors:?}"
        );
    }

    /// The editor must surface the fail-loud diagnostics that live past the
    /// frontend. Before `check_deep`, `analyze` ran only lex→typecheck, so
    /// these compiled clean in-editor and only failed at build. Regression for
    /// the whole V0.9.x fail-loud pass being visible in the LSP.
    #[test]
    fn analyze_surfaces_ir_and_codegen_diagnostics() {
        // E424: `max` has no lowering (was: silently `a + b`).
        let max_src =
            "record R { cap: amount = 0\n  view f(bid: amount) returns amount { max(cap, bid) } }";
        assert!(
            analyze(max_src)
                .iter()
                .any(|d| matches!(&d.code, Some(NumberOrString::String(c)) if c == "E424")),
            "editor should flag max() with E424, got: {:?}",
            analyze(max_src)
        );

        // E519: division by a literal zero.
        let div_src = "record R { n: amount = 0\n  view f returns amount { n / 0 } }";
        assert!(
            analyze(div_src)
                .iter()
                .any(|d| matches!(&d.code, Some(NumberOrString::String(c)) if c == "E519")),
            "editor should flag `/ 0` with E519, got: {:?}",
            analyze(div_src)
        );
    }

    #[test]
    fn collect_symbols_hello() {
        let file = parse_source(HELLO_SRC).expect("hello.cov parses");
        let syms = collect_symbols(&file, HELLO_SRC);
        assert_eq!(syms.len(), 1, "one top-level symbol");
        let top = &syms[0];
        assert_eq!(top.name, "Hello");
        assert_eq!(top.kind, SymbolKind::CLASS);
        let children = top.children.as_ref().expect("has children");
        // greeting (field), update (action), read (view)
        assert_eq!(children.len(), 3, "Hello has 3 children: {children:?}");
        assert_eq!(children[0].kind, SymbolKind::FIELD);
        assert_eq!(children[1].kind, SymbolKind::METHOD);
        assert_eq!(children[2].kind, SymbolKind::FUNCTION);
    }

    #[test]
    fn identifier_at_extracts_word() {
        let src = "  hello_world + 42";
        // Mid of "hello_world" → returns "hello_world"
        assert_eq!(identifier_at(src, 5).as_deref(), Some("hello_world"));
        // On the `+` → None
        assert_eq!(identifier_at(src, 14).as_deref(), None);
        // On a digit-leading "literal": not a valid identifier
        assert_eq!(identifier_at("42abc", 0), None);
    }

    #[test]
    fn goto_definition_field_name_returns_none() {
        // Cursor on the field's own name span = no-op (already at definition)
        let file = parse_source(HELLO_SRC).expect("parse");
        let field = file.top_level.body.iter().find_map(|d| {
            if let covenant_parser::ast::TopLevelDecl::Field(f) = d {
                if f.name.name.as_ref() == "greeting" {
                    Some(f.clone())
                } else {
                    None
                }
            } else {
                None
            }
        });
        let f = field.expect("greeting field");
        let mid = (f.name.span.start + f.name.span.end) as usize / 2;
        assert!(find_definition_at(&file, HELLO_SRC, mid).is_none());
    }

    #[test]
    fn goto_definition_field_reference_jumps_to_decl() {
        // The hello example assigns `greeting = new_text` inside `update`.
        // Find the byte offset of `greeting` in that assignment and verify
        // goto-definition returns the field-decl name span.
        let field_decl_offset = HELLO_SRC.find("    greeting: text").expect("decl");
        let assignment_offset = HELLO_SRC
            .find("        greeting = new_text")
            .expect("assignment");
        // +8 to skip past leading whitespace, land on "greeting"
        let cursor = assignment_offset + 8;

        let file = parse_source(HELLO_SRC).expect("parse");
        let target = find_definition_at(&file, HELLO_SRC, cursor).expect("definition found");

        // Target should be the field declaration's name span (4 spaces in)
        let expected_decl_start = field_decl_offset + 4; // skip 4 spaces
        assert_eq!(target.start as usize, expected_decl_start);
    }

    #[test]
    fn hover_over_field_name() {
        let file = parse_source(HELLO_SRC).expect("hello.cov parses");
        // "greeting" starts right after "record Hello {\n    " in HELLO_SRC.
        // Find its byte offset from the actual span.
        let field_decl = file.top_level.body.iter().find_map(|d| {
            if let covenant_parser::ast::TopLevelDecl::Field(f) = d {
                if f.name.name.as_ref() == "greeting" {
                    Some(f.clone())
                } else {
                    None
                }
            } else {
                None
            }
        });
        let field = field_decl.expect("greeting field exists");
        let mid = (field.name.span.start + field.name.span.end) as usize / 2;
        let hover = find_hover_at(&file, HELLO_SRC, mid);
        assert!(hover.is_some(), "hover over field name should return info");
        let text = hover.unwrap();
        assert!(text.contains("greeting"), "hover should mention field name");
        assert!(text.contains("text"), "hover should mention field type");
    }
}

//! Parsing and AST construction: converts a token stream into the Covenant abstract syntax tree.
//!
//! Phase 2 of the compiler pipeline. The public entry point is [`parse`], which
//! consumes a `&[covenant_lexer::Token]` plus a `SourceId` and returns an
//! `Option<File>` together with any diagnostics collected during parsing.
//!
//! The AST types live in the [`ast`] submodule. Parser-level diagnostic codes
//! (E020–E099) are exposed via the [`codes`] module.

pub mod ast;
mod diag;
mod parse_decl;
mod parse_expr;
mod parse_file;
mod parse_stmt;
mod parse_ty;
mod parser;
pub mod printer;

use covenant_diag::{Diagnostic, SourceId};
use covenant_lexer::Token;

pub use ast::File;
pub use parser::Parser;

/// Parse a stream of tokens into a typed AST.
///
/// Always returns a diagnostic vector (possibly empty). Returns
/// `Some(File)` when the parser produced a usable AST (possibly with
/// recoverable errors) ; returns `None` when the input was empty or could not
/// even identify a top-level construct.
pub fn parse(tokens: &[Token], source_id: SourceId) -> (Option<File>, Vec<Diagnostic>) {
    let parser = Parser::new(tokens, source_id);
    parser.parse_file()
}

/// Re-exported parser diagnostic codes (E020–E099).
pub mod codes {
    pub use crate::diag::{
        E020_UNEXPECTED_TOKEN, E021_ANNOTATION_WRONG_DECL, E022_MISSING_CLOSING_BRACE,
        E023_UNEXPECTED_EOF, E024_MALFORMED_FIELD, E025_INVALID_PRIVACY, E026_TYPE_EXPECTED,
        E027_MISSING_BODY, E028_BAD_CONSTRUCT_KEYWORD, E029_BAD_SHARES, E030_BAD_STMT_TERMINATOR,
        E031_TOO_DEEPLY_NESTED, E032_TYPE_TOO_DEEPLY_NESTED, E040_CHAIN_TOO_LONG,
        E041_BODY_TOO_LARGE,
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke() {}
}

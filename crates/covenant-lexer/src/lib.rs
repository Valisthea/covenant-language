//! Tokenization: converts Covenant source text into a stream of tokens.
//!
//! Phase 1 of the compiler pipeline. The public entry point is [`tokenize`], which
//! takes a UTF-8 source buffer and a `SourceId` and returns a (`Vec<Token>`,
//! `Vec<Diagnostic>`) pair. The token vector always ends with `TokenKind::Eof`,
//! even when diagnostics were emitted.
//!
//! See Doc 2 §1 for the normative lexical specification, Doc 5 §4 for the
//! architectural rationale, and Doc 5 §17 for the diagnostic layering.

mod diag;
mod literals;
mod raw;
mod token;
mod transform;

pub use token::{resolve_keyword, DurationUnit, Token, TokenKind};
pub use transform::tokenize;

// Re-export the diagnostic code constants so downstream tests / tools can refer
// to them by name rather than bare integer.
pub mod codes {
    pub use crate::diag::{
        E001_UNEXPECTED, E002_UNTERMINATED_STRING, E003_C_LINE_COMMENT, E004_C_BLOCK_COMMENT,
        E005_INT_OVERFLOW, E006_HEX_ODD, E007_HEX_EMPTY, E008_BARE_CR, E009_UNTERMINATED_NESTED,
        E010_BAD_ESCAPE,
    };
}

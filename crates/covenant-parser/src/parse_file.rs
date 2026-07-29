//! Top-level file parsing: file header (optional), imports, and the single
//! top-level construct declaration.

use covenant_diag::Span;
use covenant_lexer::TokenKind;

use crate::ast::{
    AnchorClause, ConstructKind, ExternalContractDecl, ExternalFunctionDecl, ExternalParam, File,
    FileHeader, Ident, Import, PrivacyQualifier, TextLit, TopLevel, Type, UpgradeableClause,
};
use crate::diag;
use crate::parser::{describe, ParseError, Parser};

impl<'a> Parser<'a> {
    /// Parse an entire file. Returns `Ok(File)` even if individual parts errored
    /// (provided the skeleton could still be constructed) ; returns `Err(...)`
    /// only if we couldn't identify a top-level construct at all.
    pub(crate) fn parse_file(mut self) -> (Option<File>, Vec<covenant_diag::Diagnostic>) {
        // Skip any leading blank lines.
        self.skip_newlines();

        let header = self.try_parse_file_header();
        self.skip_newlines();

        let mut imports = Vec::new();
        while self.at(&TokenKind::KwImport) {
            match self.parse_import() {
                Ok(i) => imports.push(i),
                Err(_) => {
                    self.recover_to(&[
                        TokenKind::Newline,
                        TokenKind::KwImport,
                        TokenKind::KwRecord,
                        TokenKind::KwToken,
                        TokenKind::KwBallot,
                        TokenKind::KwCounter,
                        TokenKind::KwBoard,
                        TokenKind::KwMarket,
                        TokenKind::KwVault,
                        TokenKind::KwRegistry,
                        TokenKind::KwBridge,
                        TokenKind::KwCeremony,
                        TokenKind::KwNft,
                        TokenKind::KwModule,
                        TokenKind::KwTest,
                    ]);
                }
            }
            self.skip_newlines();
        }

        // Parse zero or more `external contract` blocks before the main construct.
        let mut external_contracts = Vec::new();
        while self.at(&TokenKind::KwExternal) {
            match self.parse_external_contract() {
                Ok(ec) => external_contracts.push(ec),
                Err(_) => {
                    // Skip to next known top-level keyword on error.
                    self.recover_to(&[
                        TokenKind::KwExternal,
                        TokenKind::KwRecord,
                        TokenKind::KwToken,
                        TokenKind::KwBallot,
                        TokenKind::KwCounter,
                        TokenKind::KwBoard,
                        TokenKind::KwMarket,
                        TokenKind::KwVault,
                        TokenKind::KwRegistry,
                        TokenKind::KwBridge,
                        TokenKind::KwCeremony,
                        TokenKind::KwNft,
                        TokenKind::KwModule,
                        TokenKind::KwTest,
                    ]);
                }
            }
            self.skip_newlines();
        }

        let top_level_opt = self.parse_top_level().ok();

        self.skip_newlines();

        // Optionally require EOF. We don't hard-error here; trailing tokens
        // after the construct body produce diagnostics but don't tank the AST.
        if !matches!(self.peek_kind(), Some(TokenKind::Eof) | None) {
            let span = self.current_span();
            let got = self
                .peek()
                .map(|t| describe(&t.kind))
                .unwrap_or_else(|| "end of file".into());
            self.push_diag(diag::expected(span, "end of file", &got));
        }

        let source_id = self.source_id;
        let diags = self.into_diagnostics();
        let file = top_level_opt.map(|tl| File {
            source_id,
            header,
            imports,
            external_contracts,
            top_level: tl,
        });
        (file, diags)
    }

    fn try_parse_file_header(&mut self) -> Option<FileHeader> {
        // The file header is not part of the token stream (it was consumed as
        // a line comment by the lexer). We leave this as a stub for Phase 2,
        // pragma-style headers aren't used by Basics fixtures. Returning
        // `None` is always valid.
        None
    }

    fn parse_import(&mut self) -> Result<Import, ParseError> {
        let start = self.current_span();
        self.expect(&TokenKind::KwImport, "`import`")?;
        let mut path = Vec::new();
        path.push(self.expect_ident("import path segment")?);
        while self.eat(&TokenKind::Dot) {
            path.push(self.expect_ident("import path segment after `.`")?);
        }
        let alias = if self.eat(&TokenKind::KwAs) {
            Some(self.expect_ident("alias after `as`")?)
        } else {
            None
        };
        let span = start.join(self.current_span());
        self.expect_stmt_terminator("after import")?;
        Ok(Import { path, alias, span })
    }

    fn parse_top_level(&mut self) -> Result<TopLevel, ParseError> {
        let start = self.current_span();
        let privacy = self.try_parse_privacy();
        let keyword = self.expect_construct_keyword()?;
        let name = self.expect_ident("construct name")?;
        let anchor = if matches!(self.peek_kind(), Some(TokenKind::KwAnchoredOn)) {
            Some(self.parse_anchor_clause()?)
        } else {
            None
        };
        let upgradeable = if matches!(self.peek_kind(), Some(TokenKind::KwUpgradeableBy)) {
            Some(self.parse_upgradeable_clause()?)
        } else {
            None
        };
        let body = self.parse_top_level_body()?;
        let span = start.join(self.current_span());
        Ok(TopLevel {
            span,
            privacy,
            keyword,
            name,
            anchor,
            upgradeable,
            body,
        })
    }

    fn try_parse_privacy(&mut self) -> Option<PrivacyQualifier> {
        let q = match self.peek_kind()? {
            TokenKind::KwPublic => PrivacyQualifier::Public,
            TokenKind::KwPrivate => PrivacyQualifier::Private,
            TokenKind::KwEncrypted => PrivacyQualifier::Encrypted,
            TokenKind::KwSealed => PrivacyQualifier::Sealed,
            TokenKind::KwHybrid => PrivacyQualifier::Hybrid,
            TokenKind::KwConfidential => PrivacyQualifier::Confidential,
            _ => return None,
        };
        self.advance();
        Some(q)
    }

    fn expect_construct_keyword(&mut self) -> Result<ConstructKind, ParseError> {
        let t = self.peek();
        let kind = match t.map(|t| &t.kind) {
            Some(TokenKind::KwModule) => Some(ConstructKind::Module),
            Some(TokenKind::KwRecord) => Some(ConstructKind::Record),
            Some(TokenKind::KwToken) => Some(ConstructKind::Token),
            Some(TokenKind::KwBallot) => Some(ConstructKind::Ballot),
            Some(TokenKind::KwCounter) => Some(ConstructKind::Counter),
            Some(TokenKind::KwBoard) => Some(ConstructKind::Board),
            Some(TokenKind::KwMarket) => Some(ConstructKind::Market),
            Some(TokenKind::KwVault) => Some(ConstructKind::Vault),
            Some(TokenKind::KwRegistry) => Some(ConstructKind::Registry),
            Some(TokenKind::KwBridge) => Some(ConstructKind::Bridge),
            Some(TokenKind::KwCeremony) => Some(ConstructKind::Ceremony),
            Some(TokenKind::KwNft) => Some(ConstructKind::Nft),
            Some(TokenKind::KwTest) => Some(ConstructKind::Test),
            _ => None,
        };
        match kind {
            Some(k) => {
                self.advance();
                Ok(k)
            }
            None => {
                let span = self.current_span();
                let got = t
                    .map(|t| describe(&t.kind))
                    .unwrap_or_else(|| "end of file".into());
                self.push_diag(diag::bad_construct_keyword(span, &got));
                Err(ParseError)
            }
        }
    }

    fn parse_anchor_clause(&mut self) -> Result<AnchorClause, ParseError> {
        let start = self.current_span();
        self.expect(&TokenKind::KwAnchoredOn, "`anchored_on`")?;
        self.expect(&TokenKind::LBracket, "`[`")?;
        let mut chains = Vec::new();
        if !self.at(&TokenKind::RBracket) {
            loop {
                let t = match self.peek() {
                    Some(t) => t,
                    None => {
                        let span = self.current_span();
                        self.push_diag(diag::unexpected_eof(span, "parsing `anchored_on [...]`"));
                        return Err(ParseError);
                    }
                };
                if let TokenKind::Text(s) = &t.kind {
                    chains.push(TextLit {
                        value: s.clone(),
                        span: t.span,
                    });
                    self.advance();
                } else {
                    let span = t.span;
                    let got = describe(&t.kind);
                    self.push_diag(diag::expected(span, "a text literal (chain name)", &got));
                    return Err(ParseError);
                }
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
        }
        let rb = self.expect(&TokenKind::RBracket, "`]`")?;
        Ok(AnchorClause {
            chains,
            span: start.join(rb.span),
        })
    }

    fn parse_upgradeable_clause(&mut self) -> Result<UpgradeableClause, ParseError> {
        let start = self.current_span();
        self.expect(&TokenKind::KwUpgradeableBy, "`upgradeable_by`")?;
        let principal = self.parse_principal()?;
        Ok(UpgradeableClause {
            principal,
            span: start.join(self.current_span()),
        })
    }

    /// Parse `external contract <Name> { <function-decls>* }`.
    fn parse_external_contract(&mut self) -> Result<ExternalContractDecl, ParseError> {
        let start = self.current_span();
        self.expect(&TokenKind::KwExternal, "`external`")?;
        self.expect(&TokenKind::KwContract, "`contract`")?;
        let name = self.expect_ident("external contract name")?;
        self.expect(&TokenKind::LBrace, "`{`")?;
        self.skip_newlines();

        let mut functions = Vec::new();
        while !self.at(&TokenKind::RBrace) {
            if matches!(self.peek_kind(), Some(TokenKind::Eof) | None) {
                let span = self.current_span();
                self.push_diag(diag::unexpected_eof(span, "parsing external contract body"));
                return Err(crate::parser::ParseError);
            }
            match self.parse_external_function() {
                Ok(f) => functions.push(f),
                Err(_) => {
                    self.recover_to(&[TokenKind::KwFunction, TokenKind::RBrace]);
                }
            }
            self.skip_newlines();
        }

        let end = self.expect(&TokenKind::RBrace, "`}`")?;
        Ok(ExternalContractDecl {
            name,
            functions,
            span: start.join(end.span),
        })
    }

    /// Parse `function <name>(<params>) [view] [returns <Type>]`.
    fn parse_external_function(&mut self) -> Result<ExternalFunctionDecl, ParseError> {
        let start = self.current_span();
        self.expect(&TokenKind::KwFunction, "`function`")?;
        let name = self.expect_ident_or_any_keyword("function name")?;
        self.expect(&TokenKind::LParen, "`(`")?;

        let mut params = Vec::new();
        while !self.at(&TokenKind::RParen) {
            if matches!(self.peek_kind(), Some(TokenKind::Eof) | None) {
                let span = self.current_span();
                self.push_diag(diag::unexpected_eof(
                    span,
                    "parsing external function params",
                ));
                return Err(crate::parser::ParseError);
            }
            let p_start = self.current_span();
            let ty = self.parse_type()?;
            // Optional parameter name: may be any ident or keyword (Solidity names
            // like `token`, `to`, `from` coincide with Covenant keywords).
            let param_name = if !self.at(&TokenKind::Comma) && !self.at(&TokenKind::RParen) {
                Some(self.expect_ident_or_any_keyword("parameter name")?)
            } else {
                None
            };
            let p_span = p_start.join(self.current_span());
            params.push(ExternalParam {
                name: param_name,
                ty,
                span: p_span,
            });
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RParen, "`)`")?;

        // Optional `view` qualifier.
        let is_view = self.eat(&TokenKind::KwView);

        // Optional `returns <Type>`.
        let returns: Option<Type> = if self.eat(&TokenKind::KwReturns) {
            Some(self.parse_type()?)
        } else {
            None
        };

        self.expect_stmt_terminator("after external function")?;
        let span = start.join(self.current_span());
        Ok(ExternalFunctionDecl {
            name,
            params,
            returns,
            is_view,
            span,
        })
    }
}

#[allow(dead_code)]
fn _ident_link(_i: Ident) {}
#[allow(dead_code)]
fn _span_link(_s: Span) {}

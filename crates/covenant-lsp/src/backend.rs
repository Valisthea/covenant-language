//! LSP backend: implements `tower_lsp::LanguageServer` for the Covenant language.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp::jsonrpc;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::analysis;

pub struct Backend {
    pub client: Client,
    /// In-memory text store, keyed by document URI.
    files: Arc<RwLock<HashMap<Url, String>>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            files: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn analyze_and_publish(&self, uri: Url, text: &str) {
        let diagnostics = analysis::analyze(text);
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }

    async fn get_text(&self, uri: &Url) -> Option<String> {
        self.files.read().await.get(uri).cloned()
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _params: InitializeParams) -> jsonrpc::Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::FULL),
                        save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                            include_text: Some(true),
                        })),
                        ..Default::default()
                    },
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                definition_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "covenant-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "covenant-lsp initialized")
            .await;
    }

    async fn shutdown(&self) -> jsonrpc::Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        self.files.write().await.insert(uri.clone(), text.clone());
        self.analyze_and_publish(uri, &text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        // Full-text sync: last change event contains the full document text.
        if let Some(change) = params.content_changes.into_iter().last() {
            let text = change.text;
            self.files.write().await.insert(uri.clone(), text.clone());
            self.analyze_and_publish(uri, &text).await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri;
        if let Some(text) = params.text {
            self.files.write().await.insert(uri.clone(), text.clone());
            self.analyze_and_publish(uri, &text).await;
        } else if let Some(text) = self.get_text(&uri).await {
            self.analyze_and_publish(uri, &text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.files.write().await.remove(&uri);
        // Clear diagnostics for closed document.
        self.client.publish_diagnostics(uri, vec![], None).await;
    }

    async fn hover(&self, params: HoverParams) -> jsonrpc::Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

        let text = match self.get_text(uri).await {
            Some(t) => t,
            None => return Ok(None),
        };

        let file = match analysis::parse_source(&text) {
            Some(f) => f,
            None => return Ok(None),
        };

        let offset = analysis::position_to_byte_offset(&text, pos);
        let hover_text = match analysis::find_hover_at(&file, &text, offset) {
            Some(t) => t,
            None => return Ok(None),
        };

        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: hover_text,
            }),
            range: None,
        }))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> jsonrpc::Result<Option<GotoDefinitionResponse>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .clone();
        let pos = params.text_document_position_params.position;

        let text = match self.get_text(&uri).await {
            Some(t) => t,
            None => return Ok(None),
        };

        let file = match analysis::parse_source(&text) {
            Some(f) => f,
            None => return Ok(None),
        };

        let offset = analysis::position_to_byte_offset(&text, pos);
        let target = match analysis::find_definition_at(&file, &text, offset) {
            Some(t) => t,
            None => return Ok(None),
        };

        let start = analysis::byte_offset_to_position(&text, target.start as usize);
        let end = analysis::byte_offset_to_position(&text, target.end as usize);
        let location = Location {
            uri,
            range: Range { start, end },
        };
        Ok(Some(GotoDefinitionResponse::Scalar(location)))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> jsonrpc::Result<Option<DocumentSymbolResponse>> {
        let uri = &params.text_document.uri;

        let text = match self.get_text(uri).await {
            Some(t) => t,
            None => return Ok(None),
        };

        let file = match analysis::parse_source(&text) {
            Some(f) => f,
            None => return Ok(None),
        };

        let symbols = analysis::collect_symbols(&file, &text);
        Ok(Some(DocumentSymbolResponse::Nested(symbols)))
    }
}

use crate::{
    diagnostics,
    document::DocumentData,
    semantic::{build_semantic_tokens, build_symbols, encode, legend_token_types},
};
use dashmap::DashMap;
use ropey::Rope;
use std::sync::Arc;
use tower_lsp_server::{
    Client, LanguageServer,
    ls_types::{
        DocumentFormattingParams, DocumentSymbolParams, DocumentSymbolResponse, InitializeParams,
        InitializeResult, Location, OneOf, Position, Range, SemanticTokens,
        SemanticTokensFullOptions, SemanticTokensLegend, SemanticTokensOptions,
        SemanticTokensParams, SemanticTokensResult, SemanticTokensServerCapabilities,
        ServerCapabilities, SymbolInformation, TextDocumentSyncCapability, TextDocumentSyncKind,
        TextEdit, Uri,
    },
};

type DocumentStore = Arc<DashMap<Uri, DocumentData>>;

#[derive(Debug)]
pub struct Backend {
    pub client: Client,
    pub documents: DocumentStore,
}

impl Backend {
    #[must_use]
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(DashMap::new()),
        }
    }

    //reparse file(s) on every change
    async fn on_change(&self, uri: Uri, version: i32, text: &str) {
        let (ast, diagnostics) = diagnostics::parse_and_check(text);
        let rope = Rope::from(text);

        let text_owned = text.to_string();
        let builder_result = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            tokio::task::spawn_blocking(move || {
                (
                    build_symbols(&text_owned),
                    build_semantic_tokens(&text_owned),
                )
            }),
        )
        .await;

        let (symbols, semantic_tokens) = match builder_result {
            Ok(Ok(result)) => result,
            Ok(Err(_)) | Err(_) => (vec![], vec![]),
        };

        self.documents.insert(
            uri.clone(),
            DocumentData {
                text: rope,
                symbols,
                semantic_tokens,
                ast,
            },
        );

        self.client
            .publish_diagnostics(uri, diagnostics, Some(version))
            .await;
    }

    fn format_text(&self, params: DocumentFormattingParams) -> Option<Vec<TextEdit>> {
        let uri = params.text_document.uri;
        let rope = &self.documents.get(&uri)?.text;

        let new_text = cwl_core::format::format_cwl(&rope.to_string()).unwrap_or(rope.to_string());

        let last_line = rope.len_lines() - 1;
        let last_col = rope.line(last_line).len_chars();

        Some(vec![TextEdit {
            range: Range {
                start: Position::new(0, 0),
                end: Position::new(
                    u32::try_from(last_col).unwrap_or_default(),
                    u32::try_from(last_col).unwrap_or_default(),
                ),
            },
            new_text,
        }])
    }
}

impl LanguageServer for Backend {
    async fn initialize(
        &self,
        _params: InitializeParams,
    ) -> tower_lsp_server::jsonrpc::Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                document_symbol_provider: Some(OneOf::Left(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            legend: SemanticTokensLegend {
                                token_types: legend_token_types(),
                                token_modifiers: vec![],
                            },

                            ..Default::default()
                        },
                    ),
                ),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn shutdown(&self) -> tower_lsp_server::jsonrpc::Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: tower_lsp_server::ls_types::DidOpenTextDocumentParams) {
        self.on_change(
            params.text_document.uri,
            params.text_document.version,
            &params.text_document.text,
        )
        .await;
    }

    async fn did_change(&self, params: tower_lsp_server::ls_types::DidChangeTextDocumentParams) {
        //we only care if - not what changed as full reparse is impl'd
        if let Some(change) = params.content_changes.into_iter().last() {
            self.on_change(
                params.text_document.uri,
                params.text_document.version,
                &change.text,
            )
            .await;
        }
    }
    async fn did_close(&self, params: tower_lsp_server::ls_types::DidCloseTextDocumentParams) {
        self.documents.remove(&params.text_document.uri);

        //clear diagnostics if file is closed
        self.client
            .publish_diagnostics(params.text_document.uri, vec![], None)
            .await;
    }

    async fn formatting(
        &self,
        params: tower_lsp_server::ls_types::DocumentFormattingParams,
    ) -> tower_lsp_server::jsonrpc::Result<Option<Vec<tower_lsp_server::ls_types::TextEdit>>> {
        Ok(self.format_text(params))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> tower_lsp_server::jsonrpc::Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;

        let Some(doc) = self.documents.get(&uri) else {
            return Ok(None);
        };

        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: encode(&doc.semantic_tokens),
        })))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> tower_lsp_server::jsonrpc::Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;

        let Some(doc) = self.documents.get(&uri) else {
            return Ok(None);
        };

        let symbols = doc
            .symbols
            .iter()
            .map(|s| SymbolInformation {
                name: s.name.clone(),
                kind: s.kind,
                location: Location {
                    uri: uri.clone(),
                    range: s.range,
                },
                tags: None,
                container_name: None,
                #[allow(deprecated)]
                deprecated: None,
            })
            .collect();

        Ok(Some(DocumentSymbolResponse::Flat(symbols)))
    }
}

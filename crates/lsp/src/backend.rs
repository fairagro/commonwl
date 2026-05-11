use crate::{
    diagnostics,
    document::DocumentData,
    semantic::{AbsoluteToken, encode},
    symbols::YamlSymbol,
    vocab::*,
};
use dashmap::DashMap;
use granit_parser::{Event, Marker, Parser};
use ropey::Rope;
use std::sync::Arc;
use tower_lsp_server::{
    Client, LanguageServer,
    ls_types::{
        DocumentFormattingParams, DocumentSymbolParams, DocumentSymbolResponse, InitializeParams,
        InitializeResult, Location, OneOf, Position, Range, SemanticTokenType, SemanticTokens,
        SemanticTokensFullOptions, SemanticTokensLegend, SemanticTokensOptions,
        SemanticTokensParams, SemanticTokensResult, SemanticTokensServerCapabilities,
        ServerCapabilities, SymbolInformation, SymbolKind, TextDocumentSyncCapability,
        TextDocumentSyncKind, TextEdit, Uri,
    },
};

type DocumentStore = Arc<DashMap<Uri, DocumentData>>;

#[derive(Debug)]
pub struct Backend {
    pub client: Client,
    pub documents: DocumentStore,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(DashMap::new()),
        }
    }

    //reparse file(s) on every change
    async fn on_change(&self, uri: Uri, text: &str) {
        let (ast, diagnostics) = diagnostics::parse_and_check(text);

        let symbols = build_symbols(text);
        let semantic_tokens = build_semantic_tokens(text);
        let rope = Rope::from(text);

        self.client
            .publish_diagnostics(uri.clone(), diagnostics.clone(), None)
            .await;

        self.documents.insert(
            uri,
            DocumentData {
                text: rope,
                diagnostics,
                symbols,
                semantic_tokens,
                ast,
            },
        );
    }

    async fn format_text(&self, params: DocumentFormattingParams) -> Option<Vec<TextEdit>> {
        let uri = params.text_document.uri;
        let rope = &self.documents.get(&uri)?.text;

        let new_text = cwl_core::format::format_cwl(&rope.to_string()).unwrap_or(rope.to_string());

        let last_line = rope.len_lines() - 1;
        let last_col = rope.line(last_line).len_chars();

        Some(vec![TextEdit {
            range: Range {
                start: Position::new(0, 0),
                end: Position::new(last_line as u32, last_col as u32),
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
                                token_types: vec![
                                    SemanticTokenType::PROPERTY,
                                    SemanticTokenType::STRING,
                                    SemanticTokenType::NUMBER,
                                    SemanticTokenType::KEYWORD,
                                    SemanticTokenType::VARIABLE,
                                    SemanticTokenType::TYPE,
                                    SemanticTokenType::COMMENT,
                                    SemanticTokenType::NAMESPACE,
                                ],

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
        self.on_change(params.text_document.uri, &params.text_document.text)
            .await
    }

    async fn did_change(&self, params: tower_lsp_server::ls_types::DidChangeTextDocumentParams) {
        //we only core if - not what changed as full reparse is impl'd
        if let Some(change) = params.content_changes.into_iter().last() {
            self.on_change(params.text_document.uri, &change.text).await
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
        Ok(self.format_text(params).await)
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

fn marker_to_position(marker: Marker) -> Position {
    Position::new((marker.line() - 1) as u32, marker.col() as u32)
}

fn build_symbols(text: &str) -> Vec<YamlSymbol> {
    let mut symbols = Vec::new();

    for item in Parser::new_from_str(text) {
        let Ok((event, span)) = item else {
            continue;
        };

        if let Event::Scalar(value, _, _, _) = event {
            let start = marker_to_position(span.start);
            let end = marker_to_position(span.end);

            symbols.push(YamlSymbol {
                name: value.to_string(),
                kind: SymbolKind::KEY,
                range: Range { start, end },
            });
        }
    }

    symbols
}

fn build_semantic_tokens(text: &str) -> Vec<AbsoluteToken> {
    let mut tokens = Vec::new();

    for item in Parser::new_from_str(text) {
        let Ok((event, span)) = item else {
            continue;
        };

        if let Event::Scalar(value, _, _, _) = event {
            let start = marker_to_position(span.start);
            let end = marker_to_position(span.end);
            let length = end.character - start.character;
            let token_type = classify(&value);
            tokens.push(AbsoluteToken {
                line: start.line,
                start: start.character,
                length,
                token_type,
                modifiers: 0,
            });
        }
    }

    tokens
}

fn classify(value: &str) -> u32 {
    if CWL_CLASSES.contains(value) {
        return 5;
    }
    if CWL_REQUIREMENTS.contains(value) {
        return 7;
    }
    if CWL_CORE_FIELDS.contains(value) {
        return 3;
    }
    if CWL_IO_FIELDS.contains(value) {
        return 0;
    }
    if CWL_WORKFLOW_FIELDS.contains(value) {
        return 0;
    }
    if CWL_COMMAND_FIELDS.contains(value) {
        return 0;
    }
    if CWL_HINT_FIELDS.contains(value) {
        return 7;
    }
    if CWL_PRIMITIVE_TYPES.contains(value) {
        return 5;
    }
    if CWL_SCATTER_METHODS.contains(value) {
        return 3;
    }
    if CWL_LINK_MERGE.contains(value) {
        return 3;
    }
    if CWL_PICK_VALUE.contains(value) {
        return 3;
    }
    if value.contains(':') {
        return 7;
    }
    if value.parse::<f64>().is_ok() {
        return 2;
    }
    1
}

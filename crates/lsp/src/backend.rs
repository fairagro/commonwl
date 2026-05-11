use crate::{
    diagnostics,
    document::DocumentData,
    semantic::{self, AbsoluteToken, encode, legend_token_types},
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
        InitializeResult, Location, OneOf, Position, Range, SemanticTokens,
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

#[derive(Clone, Copy)]
enum Ctx {
    Key,
    Value,
    Sequence,
}

fn build_semantic_tokens(text: &str) -> Vec<AbsoluteToken> {
    let mut tokens = Vec::new();
    let mut stack: Vec<Ctx> = Vec::new();

    for item in Parser::new_from_str(text) {
        let Ok((event, span)) = item else { continue };

        match event {
            // Push fresh Key context; the nested structure itself consumed
            // the parent Value slot — flip parent Value→Key immediately.
            Event::MappingStart(..) => {
                if let Some(ctx) = stack.last_mut()
                    && matches!(ctx, Ctx::Value)
                {
                    *ctx = Ctx::Key;
                }
                stack.push(Ctx::Key);
            }

            // Same idea: sequence-as-value consumes the parent Value slot.
            Event::SequenceStart(..) => {
                if let Some(ctx) = stack.last_mut()
                    && matches!(ctx, Ctx::Value)
                {
                    *ctx = Ctx::Key;
                }
                stack.push(Ctx::Sequence);
            }

            Event::MappingEnd | Event::SequenceEnd => {
                stack.pop();
            }

            Event::Scalar(value, _, _, _) => {
                let is_key = matches!(stack.last(), Some(Ctx::Key));

                // Advance the mapping alternation; sequences never flip.
                if let Some(ctx) = stack.last_mut() {
                    match ctx {
                        Ctx::Key => *ctx = Ctx::Value,
                        Ctx::Value => *ctx = Ctx::Key,
                        Ctx::Sequence => {}
                    }
                }

                let start = marker_to_position(span.start);
                let end = marker_to_position(span.end);
                let length = end.character - start.character;

                tokens.push(AbsoluteToken {
                    line: start.line,
                    start: start.character,
                    length,
                    token_type: classify(&value, is_key),
                    modifiers: 0,
                });
            }
            _ => {}
        }
    }

    tokens
}

fn classify(value: &str, is_key: bool) -> u32 {
    if is_key {
        classify_key(value)
    } else {
        classify_value(value)
    }
}

/// Keys are field names — they're either schema-defined or user-defined.
fn classify_key(value: &str) -> u32 {
    // Structural schema keywords: class, cwlVersion, doc, label, intent
    if CWL_CORE_FIELDS.contains(value) {
        return semantic::KEYWORD;
    }
    // Named field properties: inputs, outputs, baseCommand, arguments, steps …
    if CWL_IO_FIELDS.contains(value)
        || CWL_WORKFLOW_FIELDS.contains(value)
        || CWL_COMMAND_FIELDS.contains(value)
    {
        return semantic::PROPERTY;
    }
    // Requirements/hints used as mapping keys (shorthand form):
    //   requirements:
    //     DockerRequirement:   ← key
    //       dockerPull: …
    if CWL_REQUIREMENTS.contains(value) || CWL_HINT_FIELDS.contains(value) {
        return semantic::DECORATOR;
    }
    // Everything else is a user-defined identifier: step IDs, port names, tool IDs
    semantic::VARIABLE
}

/// Values are what fields are set to — types, classes, literals, user strings.
fn classify_value(value: &str) -> u32 {
    // CWL class declarations: CommandLineTool, Workflow, ExpressionTool, Operation
    if CWL_CLASSES.contains(value) {
        return semantic::CLASS;
    }
    // Requirements as values (list form):  - class: DockerRequirement
    if CWL_REQUIREMENTS.contains(value) {
        return semantic::DECORATOR;
    }
    // CWL primitive types: string, int, long, float, double, boolean, null, File, Directory
    if CWL_PRIMITIVE_TYPES.contains(value) {
        return semantic::TYPE;
    }
    // Fixed enum members: dotproduct / nested_crossproduct, merge_nested, first_non_null …
    if CWL_SCATTER_METHODS.contains(value)
        || CWL_LINK_MERGE.contains(value)
        || CWL_PICK_VALUE.contains(value)
    {
        return semantic::ENUM_MEMBER;
    }
    // Namespace-prefixed tokens: s:author, edam:data_0006, $namespaces entries
    if value.contains(':') {
        return semantic::NAMESPACE;
    }
    // Numeric literals
    if value.parse::<f64>().is_ok() {
        return semantic::NUMBER;
    }
    // User strings: file paths, expressions, step source references
    semantic::STRING
}

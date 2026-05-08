use crate::diagnostics;
use dashmap::DashMap;
use std::sync::Arc;
use tower_lsp_server::{
    Client, LanguageServer,
    ls_types::{
        InitializeParams, InitializeResult, ServerCapabilities, TextDocumentSyncCapability,
        TextDocumentSyncKind, Uri,
    },
};

type DocumentStore = Arc<DashMap<Uri, String>>;

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
    async fn on_change(&self, uri: Uri, text: String) {
        let diags = diagnostics::parse_and_check(&text);
        self.documents.insert(uri.clone(), text);

        self.client.publish_diagnostics(uri, diags, None).await;
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
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn shutdown(&self) -> tower_lsp_server::jsonrpc::Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: tower_lsp_server::ls_types::DidOpenTextDocumentParams) {
        self.on_change(params.text_document.uri, params.text_document.text)
            .await
    }

    async fn did_change(&self, params: tower_lsp_server::ls_types::DidChangeTextDocumentParams) {
        //we only core if - not what changed as full reparse is impl'd
        if let Some(change) = params.content_changes.into_iter().last() {
            self.on_change(params.text_document.uri, change.text).await
        }
    }

    async fn did_close(&self, params: tower_lsp_server::ls_types::DidCloseTextDocumentParams) {
        self.documents.remove(&params.text_document.uri);

        //clear diagnostics if file is closed
        self.client
            .publish_diagnostics(params.text_document.uri, vec![], None)
            .await;
    }
}

use tower_lsp_server::ls_types::{Range, SymbolKind};

#[derive(Debug, Clone)]
pub struct YamlSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub range: Range,
}

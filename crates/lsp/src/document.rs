use crate::semantic::AbsoluteToken;
use crate::symbols::YamlSymbol;
use cwl_core::documents::CWLDocument;
use ropey::Rope;
use tower_lsp_server::ls_types::Diagnostic;

#[derive(Debug)]
pub struct DocumentData {
    pub text: Rope,
    pub diagnostics: Vec<Diagnostic>,
    pub symbols: Vec<YamlSymbol>,
    pub semantic_tokens: Vec<AbsoluteToken>,
    pub ast: Option<CWLDocument>,
}

use crate::semantic::AbsoluteToken;
use crate::symbols::YamlSymbol;
use cwl_core::documents::CWLDocument;
use ropey::Rope;

#[derive(Debug)]
pub struct DocumentData {
    pub text: Rope,
    pub symbols: Vec<YamlSymbol>,
    pub semantic_tokens: Vec<AbsoluteToken>,
    pub ast: Option<CWLDocument>,
}

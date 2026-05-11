use tower_lsp_server::ls_types::{SemanticToken, SemanticTokenType};

#[derive(Debug, Clone)]
pub struct AbsoluteToken {
    pub line: u32,
    pub start: u32,
    pub length: u32,
    pub token_type: u32,
    pub modifiers: u32,
}

pub fn encode(tokens: &[AbsoluteToken]) -> Vec<SemanticToken> {
    let mut result = Vec::new();
    let mut prev_line = 0;
    let mut prev_start = 0;

    for token in tokens {
        let delta_line = token.line - prev_line;

        let delta_start = if delta_line == 0 {
            token.start - prev_start
        } else {
            token.start
        };

        result.push(SemanticToken {
            delta_line,
            delta_start,
            length: token.length,
            token_type: token.token_type,
            token_modifiers_bitset: token.modifiers,
        });

        prev_line = token.line;
        prev_start = token.start;
    }

    result
}

pub const KEYWORD: u32 = 0;
pub const PROPERTY: u32 = 1;
pub const TYPE: u32 = 2;
pub const CLASS: u32 = 3;
pub const DECORATOR: u32 = 4;
pub const NAMESPACE: u32 = 5;
pub const ENUM_MEMBER: u32 = 6;
pub const NUMBER: u32 = 7;
pub const STRING: u32 = 8;
pub const VARIABLE: u32 = 9;

pub fn legend_token_types() -> Vec<SemanticTokenType> {
    vec![
        SemanticTokenType::KEYWORD,     // 0
        SemanticTokenType::PROPERTY,    // 1
        SemanticTokenType::TYPE,        // 2
        SemanticTokenType::CLASS,       // 3
        SemanticTokenType::DECORATOR,   // 4
        SemanticTokenType::NAMESPACE,   // 5
        SemanticTokenType::ENUM_MEMBER, // 6
        SemanticTokenType::NUMBER,      // 7
        SemanticTokenType::STRING,      // 8
        SemanticTokenType::VARIABLE,    //9
    ]
}

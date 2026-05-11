use tower_lsp_server::ls_types::SemanticToken;

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

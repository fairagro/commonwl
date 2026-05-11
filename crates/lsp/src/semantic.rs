use crate::{symbols::YamlSymbol, vocab::*};
use granit_parser::{Event, Marker, Parser, Tag};
use ropey::Rope;
use tower_lsp_server::ls_types::{Position, Range, SemanticToken, SemanticTokenType, SymbolKind};

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

fn marker_to_position(marker: Marker) -> Position {
    Position::new((marker.line() - 1) as u32, marker.col() as u32)
}

pub(crate) fn build_symbols(text: &str) -> Vec<YamlSymbol> {
    let mut symbols = Vec::new();
    let mut stack: Vec<Ctx> = Vec::new();

    for item in Parser::new_from_str(text) {
        let Ok((event, span)) = item else {
            continue;
        };

        match event {
            Event::MappingStart(..) => stack.push(Ctx::Key),
            Event::SequenceStart(..) => stack.push(Ctx::Sequence),
            Event::MappingEnd | Event::SequenceEnd => {
                stack.pop();
            }
            Event::Scalar(value, _, _, _) => {
                let is_key = matches!(stack.last(), Some(Ctx::Key));

                if let Some(ctx) = stack.last_mut() {
                    match ctx {
                        Ctx::Key => *ctx = Ctx::Value,
                        Ctx::Value => *ctx = Ctx::Key,
                        Ctx::Sequence => {}
                    }
                }

                if is_key {
                    let start = marker_to_position(span.start);
                    let end = marker_to_position(span.end);

                    symbols.push(YamlSymbol {
                        name: value.to_string(),
                        kind: SymbolKind::KEY,
                        range: Range { start, end },
                    });
                }
            }
            Event::Alias(_) => {
                if let Some(ctx) = stack.last_mut() {
                    match ctx {
                        Ctx::Key => *ctx = Ctx::Value,
                        Ctx::Value => *ctx = Ctx::Key,
                        Ctx::Sequence => {}
                    }
                }
            }
            _ => {}
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

pub(crate) fn build_semantic_tokens(text: &str) -> Vec<AbsoluteToken> {
    let rope = Rope::from(text);
    let mut tokens = Vec::new();
    let mut stack: Vec<Ctx> = Vec::new();

    for item in Parser::new_from_str(text) {
        let Ok((event, span)) = item else { continue };

        match event {
            Event::MappingStart(..) => {
                if let Some(ctx) = stack.last_mut()
                    && matches!(ctx, Ctx::Value)
                {
                    *ctx = Ctx::Key;
                }
                stack.push(Ctx::Key);
            }

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

            Event::Scalar(value, _, _, tag) => {
                let is_key = matches!(stack.last(), Some(Ctx::Key));

                if let Some(ctx) = stack.last_mut() {
                    match ctx {
                        Ctx::Key => *ctx = Ctx::Value,
                        Ctx::Value => *ctx = Ctx::Key,
                        Ctx::Sequence => {}
                    }
                }

                let start = marker_to_position(span.start);
                let end = marker_to_position(span.end);
                let token_type = classify_scalar(&value, is_key, tag.as_deref());

                push_span_tokens(&mut tokens, &rope, start, end, token_type);
            }

            Event::Alias(_) => {
                if let Some(ctx) = stack.last_mut() {
                    match ctx {
                        Ctx::Key => *ctx = Ctx::Value,
                        Ctx::Value => *ctx = Ctx::Key,
                        Ctx::Sequence => {}
                    }
                }

                let start = marker_to_position(span.start);
                let end = marker_to_position(span.end);

                push_span_tokens(&mut tokens, &rope, start, end, VARIABLE);
            }
            _ => {}
        }
    }

    tokens.sort_by_key(|t| (t.line, t.start));
    tokens
}

fn push_span_tokens(
    tokens: &mut Vec<AbsoluteToken>,
    rope: &Rope,
    start: Position,
    end: Position,
    token_type: u32,
) {
    if start.line == end.line {
        let length = end.character.saturating_sub(start.character);
        if length > 0 {
            tokens.push(AbsoluteToken {
                line: start.line,
                start: start.character,
                length,
                token_type,
                modifiers: 0,
            });
        }

        return;
    }

    for line in start.line..=end.line {
        let line_text = rope.line(line as usize).to_string();
        let line_len = line_text.trim_end_matches('\n').chars().count() as u32;

        let (segment_start, length) = if line == start.line {
            let start_col = start.character;
            (start_col, line_len.saturating_sub(start_col))
        } else if line == end.line {
            (0, end.character)
        } else {
            (0, line_len)
        };

        if length > 0 {
            tokens.push(AbsoluteToken {
                line,
                start: segment_start,
                length,
                token_type,
                modifiers: 0,
            });
        }
    }
}

fn classify_scalar(value: &str, is_key: bool, tag: Option<&Tag>) -> u32 {
    if is_key {
        classify_key(value)
    } else {
        classify_value(value, tag)
    }
}

fn classify_value(value: &str, tag: Option<&Tag>) -> u32 {
    if is_cwl_expression(value) {
        return VARIABLE;
    }

    if let Some(tag) = tag
        && tag.is_yaml_core_schema()
    {
        return TYPE;
    }

    if matches!(value, "true" | "false" | "null") {
        return TYPE;
    }

    if CWL_CLASSES.contains(value) {
        return CLASS;
    }

    if CWL_REQUIREMENTS.contains(value) {
        return DECORATOR;
    }

    if CWL_PRIMITIVE_TYPES.contains(value) {
        return TYPE;
    }

    if CWL_SCATTER_METHODS.contains(value)
        || CWL_LINK_MERGE.contains(value)
        || CWL_PICK_VALUE.contains(value)
    {
        return ENUM_MEMBER;
    }

    if value.contains(':') {
        return NAMESPACE;
    }

    if value.parse::<f64>().is_ok() {
        return NUMBER;
    }

    STRING
}

fn is_cwl_expression(value: &str) -> bool {
    value.contains("$(") || value.contains("${")
}

fn classify_key(_value: &str) -> u32 {
    KEYWORD
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiline_scalar_is_split_into_line_tokens() {
        let yaml = "value: |\n  line1\n  line2\n";
        let tokens = build_semantic_tokens(yaml);

        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].token_type, KEYWORD);
        assert_eq!(tokens[1].line, 1);
        assert_eq!(tokens[2].line, 2);
        assert_eq!(tokens[1].length, 5);
        assert_eq!(tokens[2].length, 7);
    }

    #[test]
    fn alias_event_generates_variable_token() {
        let yaml = "anchor: &id foo\nalias: *id\n";
        let tokens = build_semantic_tokens(yaml);

        assert!(tokens.iter().any(|token| token.token_type == VARIABLE));
        assert!(
            tokens
                .iter()
                .any(|token| token.line == 1 && token.length > 0)
        );
    }

    #[test]
    fn symbolizes_only_mapping_keys() {
        let yaml = "name: example\ninputs:\n  - id: foo\n";
        let symbols = build_symbols(yaml);

        assert_eq!(symbols.len(), 3);
        assert_eq!(symbols[0].name, "name");
        assert_eq!(symbols[1].name, "inputs");
        assert_eq!(symbols[2].name, "id");
    }

    #[test]
    fn expression_scalar_is_variable() {
        let yaml = "expression: $(inputs.value + 1)\n";
        let tokens = build_semantic_tokens(yaml);

        assert!(tokens.iter().any(|token| token.token_type == VARIABLE));
    }

    #[test]
    fn boolean_and_null_values_are_type_tokens() {
        let yaml = "flag: true\nmissing: null\n";
        let tokens = build_semantic_tokens(yaml);

        assert!(tokens.iter().any(|token| token.token_type == TYPE));
    }
}

use crate::context::Runtime;
use cwl_core::inputs::DefaultValue;
use std::{collections::HashMap, ops::Range};

#[derive(Debug)]
enum ExpressionType {
    Paren,
    Bracket,
}

#[derive(Debug)]
struct Expression {
    expression: String,
    ty: ExpressionType,
    indices: Range<usize>,
}
impl Expression {
    pub fn expression(&self) -> String {
        match self.ty {
            ExpressionType::Paren => self.expression.clone(),
            ExpressionType::Bracket => format!("(() => {{{}}})();", self.expression),
        }
    }
}

pub fn do_eval(
    expression: &str,
    context: Option<serde_json::Value>,
    inputs: &HashMap<String, DefaultValue>,
    runtime: &Runtime,
) -> anyhow::Result<serde_yaml::Value> {
    let expressions = parse_expressions(expression);
    if expressions.is_empty() {
        anyhow::bail!("No Expression")
    }

    let context = context.unwrap_or_default();

    let inputs = serde_json::to_value(inputs)?;
    let runtime = serde_json::to_value(runtime)?;

    let map = HashMap::from([("self", context), ("inputs", inputs), ("runtime", runtime)]);
    if expressions.len() == 1 && expressions[0].indices.start == 0 {
        return simple_expression_eval(&expressions[0].expression(), &map);
    }

    //string interpolation
    let v = replace_expressions(expression, expressions, map)?;
    Ok(v)
}

fn simple_expression_eval(
    expression: &str,
    map: &HashMap<&str, serde_json::Value>,
) -> anyhow::Result<serde_yaml::Value> {
    //simple engine uses jmespath
    let expr = jmespath::compile(expression)?;
    let data = jmespath::Variable::from_serializable(map)?;
    let result = expr.search(data)?;
    Ok(serde_yaml::to_value(&result)?)
}

fn replace_expressions(
    expr: &str,
    expressions: Vec<Expression>,
    map: HashMap<&str, serde_json::Value>,
) -> anyhow::Result<serde_yaml::Value> {
    let evaluations = expressions
        .iter()
        .map(|e| simple_expression_eval(&e.expression(), &map))
        .collect::<anyhow::Result<Vec<serde_yaml::Value>>>()?;

    let mut result = expr.to_string();

    for (i, e) in expressions.iter().enumerate() {
        let expr = &expr[e.indices.clone()];
        result = result.replace(expr, &serde_json::to_string(&evaluations[i])?);
    }

    Ok(serde_yaml::to_value(result)?)
}

fn parse_expressions(expr: &str) -> Vec<Expression> {
    if !expr.contains('$') {
        return vec![];
    }

    //split into substrings
    let slices = split_ranges(expr, '$');
    let map = expr.char_indices().collect::<HashMap<_, _>>();

    let mut expressions = vec![];

    for (start, end) in &slices {
        if map[start] != '$' || end - start < 4 || !['(', '{'].contains(&map[&(start + 1)]) {
            continue;
        }

        let opening = map[&(start + 1)];
        let closing = if opening == '(' { ')' } else { '}' };
        let mut open_braces = 0;

        let extype = if opening == '(' {
            ExpressionType::Paren
        } else {
            ExpressionType::Bracket
        };

        //get expression body
        for i in *start..*end {
            if map[&i] == opening {
                open_braces += 1;
            }
            if map[&i] == closing {
                open_braces -= 1;
                if open_braces == 0 {
                    expressions.push(Expression {
                        expression: expr[*start + 2..i].to_string(),
                        ty: extype,
                        indices: *start..i + 1,
                    });
                    break;
                }
            }
        }
    }

    expressions
}

fn split_ranges(s: &str, delim: char) -> Vec<(usize, usize)> {
    let mut slices = Vec::new();
    let mut last_index = 0;

    for (idx, _) in s.match_indices(delim) {
        if last_index != idx {
            slices.push((last_index, idx));
        }
        last_index = idx;
    }

    if last_index < s.len() {
        slices.push((last_index, s.len()));
    }

    slices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_expressions() {
        let input = "$(runtime.tmpdir)";
        let result = do_eval(input, None, &HashMap::new(), &Runtime::default()).unwrap();
        let str = serde_yaml::to_string(&result).unwrap();
        assert_eq!(str.trim(), ".");
    }
}

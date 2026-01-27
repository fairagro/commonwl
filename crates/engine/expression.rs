use crate::context::Runtime;
use boa_engine::{Context, JsString, JsValue, Source, property::PropertyKey};
use cwl_core::{
    inputs::DefaultValue,
    requirements::{ExpressionLibItem, InlineJavascriptRequirement},
};
use std::{collections::HashMap, fs, ops::Range, path::Path, str::FromStr};

#[derive(Debug, Default)]
pub struct EvaluationContext<'a> {
    pub context: Option<&'a serde_json::Value>,
    pub inputs: Option<&'a HashMap<String, DefaultValue>>,
    pub runtime: Option<&'a Runtime>,
    pub ijsr: Option<&'a InlineJavascriptRequirement>,
    pub workdir: Option<&'a Path>,
}

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
    eval_context: &EvaluationContext,
) -> anyhow::Result<serde_yaml::Value> {
    let expressions = parse_expressions(expression);
    if expressions.is_empty() {
        anyhow::bail!("No Expression")
    }

    let context = eval_context.context.unwrap_or_default();

    let inputs = serde_json::to_value(eval_context.inputs)?;
    let runtime = serde_json::to_value(eval_context.runtime)?;

    let map = HashMap::from([
        ("self", context.clone()),
        ("inputs", inputs),
        ("runtime", runtime),
    ]);

    if expressions.len() == 1 && expressions[0].indices.start == 0 {
        if let Some(ijsr) = eval_context.ijsr {
            return boa_eval(
                &expressions[0].expression(),
                &map,
                ijsr,
                eval_context.workdir,
            );
        } else {
            return simple_expression_eval(&expressions[0].expression(), &map);
        }
    }
    //string interpolation
    let v = replace_expressions(expression, expressions, map, eval_context.ijsr)?;
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

fn boa_eval(
    expression: &str,
    map: &HashMap<&str, serde_json::Value>,
    ijsr: &InlineJavascriptRequirement,
    workdir: Option<&Path>,
) -> anyhow::Result<serde_yaml::Value> {
    let mut context = Context::default();
    for (key, value) in map {
        let value = JsValue::from_json(value, &mut context).map_err(|e| anyhow::anyhow!("{e}"))?;
        let key = PropertyKey::String(JsString::from_str(key)?);
        context
            .global_object()
            .set(key, value, true, &mut context)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
    }
    //handle expressionlib
    let workdir = workdir.unwrap_or(Path::new("."));
    if let Some(lib) = &ijsr.expression_lib {
        for item in lib {
            match item {
                ExpressionLibItem::Include(include) => {
                    let include = &include.include;
                    let contents = fs::read_to_string(workdir.join(include))?;
                    context
                        .eval(Source::from_bytes(&contents))
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                }
                ExpressionLibItem::Expression(expr) => {
                    context
                        .eval(Source::from_bytes(expr))
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                }
            };
        }
    }

    let result = context
        .eval(Source::from_bytes(expression))
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut json = result
        .to_json(&mut context)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    if let Some(value) = &mut json {
        normalize_json_numbers(value);
    }

    Ok(serde_yaml::to_value(json)?)
}

fn normalize_json_numbers(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Number(n) => {
            if let Some(f) = n.as_f64()
                && f.is_finite()
                && f.fract() == 0.0
                && f.abs() <= i64::MAX as f64
            {
                *value = serde_json::Value::Number(serde_json::Number::from(f as i64));
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                normalize_json_numbers(item);
            }
        }
        serde_json::Value::Object(obj) => {
            for (_, v) in obj {
                normalize_json_numbers(v);
            }
        }
        _ => {}
    }
}

fn replace_expressions(
    expr: &str,
    expressions: Vec<Expression>,
    map: HashMap<&str, serde_json::Value>,
    ijsr: Option<&InlineJavascriptRequirement>,
) -> anyhow::Result<serde_yaml::Value> {
    if ijsr.is_some() {
        todo!()
    }

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
    fn test_expression() {
        let expression = "$(parseInt(\"161\"))";
        let result = do_eval(
            expression,
            &EvaluationContext {
                ijsr: Some(&InlineJavascriptRequirement::default()),
                ..Default::default()
            },
        )
        .unwrap_or_default()
        .as_u64()
        .unwrap_or_default();

        assert_eq!(result, 161);
    }

    #[test]
    fn test_parse_expressions() {
        let input = "$(runtime.tmpdir)";
        let runtime = Runtime::default();
        let result = do_eval(
            input,
            &EvaluationContext {
                runtime: Some(&runtime),
                ..Default::default()
            },
        )
        .unwrap();
        let str = serde_yaml::to_string(&result).unwrap();
        assert_eq!(str.trim(), ".");
    }
}

use crate::environment::runtime::Runtime;
use anyhow::Context as _;
use boa_engine::{Context, JsString, JsValue, Source, property::PropertyKey};
use cwl_core::{
    inputs::DefaultValue,
    requirements::{ExpressionLibItem, InlineJavascriptRequirement},
};
use std::{collections::HashMap, fs, ops::Range, path::Path, str::FromStr};

#[derive(Debug, Default, Clone)]
pub struct EvaluationContext<'a> {
    pub context: Option<&'a serde_json::Value>,
    pub inputs: Option<&'a HashMap<String, DefaultValue>>,
    pub runtime: Option<&'a Runtime>,
    pub ijsr: Option<&'a InlineJavascriptRequirement>,
    pub workdir: Option<&'a Path>,
}

impl<'a> EvaluationContext<'a> {
    pub fn with_context(self, context: &'a serde_json::Value) -> Self {
        Self {
            context: Some(context),
            ..self
        }
    }
    pub fn with_runtime(self, runtime: &'a Runtime) -> Self {
        Self {
            runtime: Some(runtime),
            ..self
        }
    }

    pub fn with_inputs(self, inputs: &'a HashMap<String, DefaultValue>) -> Self {
        Self {
            inputs: Some(inputs),
            ..self
        }
    }
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

pub fn do_eval_to_string(expression: &str, eval_context: &EvaluationContext) -> String {
    if let Ok(res) = do_eval(expression, eval_context) {
        res.as_str().unwrap().to_string()
    } else {
        expression.to_string()
    }
}

pub fn do_eval(
    expression: &str,
    eval_context: &EvaluationContext,
) -> anyhow::Result<serde_yaml::Value> {
    let expressions = parse_expressions(expression);

    if expressions.is_empty() {
        // No CWL expressions found, just unescape and return
        return Ok(serde_yaml::to_value(unescape_value(expression))?);
    }

    let context = eval_context.context.unwrap_or(&serde_json::Value::Null);
    let inputs = serde_json::to_value(eval_context.inputs)?;
    let runtime = serde_json::to_value(eval_context.runtime)?;

    let map = HashMap::from([
        ("self", context.clone()),
        ("inputs", inputs),
        ("runtime", runtime),
    ]);

    if expressions.len() == 1
        && expressions[0].indices.start == 0
        && expressions[0].indices.end == expression.trim().len()
    {
        if let Some(ijsr) = eval_context.ijsr {
            return js_eval(
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
    let v = replace_expressions(
        expression,
        expressions,
        map,
        eval_context.ijsr,
        eval_context.workdir,
    )?;
    Ok(v)
}

fn simple_expression_eval(
    expression: &str,
    map: &HashMap<&str, serde_json::Value>,
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

    let result = context
        .eval(Source::from_bytes(expression))
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut json = result
        .to_json(&mut context)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    if let Some(value) = &mut json {
        normalize_json_numbers(value);
    } else {
        anyhow::bail!("Expression did not evaluate to a value");
    }

    Ok(serde_yaml::to_value(json)?)
}

fn js_eval(
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
                    let contents = fs::read_to_string(workdir.join(include))
                        .with_context(|| format!("Could not read expression_lib {include}"))?;
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

    //auto wrap expressions if object notation
    let expression = if expression.trim().starts_with('{') {
        format!("({})", expression)
    } else {
        expression.to_string()
    };

    let result = context
        .eval(Source::from_bytes(&expression))
        .map_err(|e| anyhow::anyhow!("Could not evaluate expression: {expression}: {e}"))?;
    let mut json = result
        .to_json(&mut context)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    if let Some(value) = &mut json {
        normalize_json_numbers(value);
    } else {
        anyhow::bail!("Expression did not evaluate to a value");
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
    workdir: Option<&Path>,
) -> anyhow::Result<serde_yaml::Value> {
    let evaluations = expressions
        .iter()
        .map(|e| {
            if let Some(ijsr) = ijsr {
                js_eval(&e.expression(), &map, ijsr, workdir)
            } else {
                simple_expression_eval(&e.expression(), &map)
            }
        })
        .collect::<anyhow::Result<Vec<serde_yaml::Value>>>()?;

    let mut result = String::new();
    let mut last_end = 0;

    for (i, e) in expressions.iter().enumerate() {
        // Add the part before this expression (with escapes processed)
        result.push_str(&unescape_value(&expr[last_end..e.indices.start]));

        // Add the evaluated expression
        result.push_str(&to_str(&evaluations[i]));

        last_end = e.indices.end;
    }

    // Add the remaining part (with escapes processed)
    result.push_str(&unescape_value(&expr[last_end..]));

    Ok(serde_yaml::to_value(result)?)
}

fn parse_expressions(expr: &str) -> Vec<Expression> {
    if !expr.contains('$') {
        return vec![];
    }

    let chars: Vec<char> = expr.chars().collect();
    let mut expressions = vec![];
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '$' && i + 1 < chars.len() && (chars[i + 1] == '(' || chars[i + 1] == '{') {
            // Count CONSECUTIVE backslashes immediately before the $
            let mut backslash_count = 0;
            if i > 0 {
                let mut j = i - 1;
                loop {
                    if chars[j] == '\\' {
                        backslash_count += 1;
                        if j == 0 {
                            break;
                        }
                        j -= 1;
                    } else {
                        break;
                    }
                }
            }

            // If odd number of backslashes, the $ is escaped
            if backslash_count % 2 == 1 {
                i += 1;
                continue;
            }

            let opening = chars[i + 1];
            let closing = if opening == '(' { ')' } else { '}' };
            let start_idx = i;
            let mut open_braces = 0;

            let extype = if opening == '(' {
                ExpressionType::Paren
            } else {
                ExpressionType::Bracket
            };

            // Start from the opening brace
            let mut k = i + 1;
            let mut found = false;

            while k < chars.len() {
                if chars[k] == opening {
                    open_braces += 1;
                } else if chars[k] == closing {
                    open_braces -= 1;
                    if open_braces == 0 {
                        let expr_body: String = chars[(start_idx + 2)..k].iter().collect();
                        expressions.push(Expression {
                            expression: expr_body,
                            ty: extype,
                            indices: start_idx..(k + 1),
                        });
                        i = k + 1;
                        found = true;
                        break;
                    }
                }
                k += 1;
            }

            if !found {
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    expressions
}

fn unescape_value(expr: &str) -> String {
    let chars: Vec<char> = expr.chars().collect();
    let mut result = String::new();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            // Count consecutive backslashes
            let mut backslash_count = 0;
            let mut j = i;
            while j < chars.len() && chars[j] == '\\' {
                backslash_count += 1;
                j += 1;
            }

            // Check what follows the backslashes
            if j < chars.len()
                && chars[j] == '$'
                && j + 1 < chars.len()
                && (chars[j + 1] == '(' || chars[j + 1] == '{')
            {
                // Before CWL expression: $(... or ${...
                // Each pair \\ becomes \, and if odd, the last one escapes the expression
                let pairs = backslash_count / 2;
                for _ in 0..pairs {
                    result.push('\\');
                }
                if backslash_count % 2 == 1 {
                    // Odd number: last backslash escapes the CWL expression
                    result.push('$');
                    i = j + 1;
                } else {
                    // Even number: CWL expression is not escaped
                    result.push('$');
                    i = j;
                }
            } else {
                // Not before a CWL expression
                // Process pairs: \\ -> \, and keep odd backslash
                let pairs = backslash_count / 2;
                for _ in 0..pairs {
                    result.push('\\');
                }
                if backslash_count % 2 == 1 {
                    result.push('\\');
                }
                i = j;
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

fn to_str(value: &serde_yaml::Value) -> String {
    match value {
        serde_yaml::Value::String(s) => s.to_string(),
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::Bool(b) => b.to_string(),
        serde_yaml::Value::Null => "null".to_string(),
        serde_yaml::Value::Mapping(m) => serde_json::to_string_pretty(m).unwrap_or(String::new()),
        _ => String::new(),
    }
}

pub fn extract_input_name(expr: &str) -> Option<String> {
    let exprs = parse_expressions(expr);
    if exprs.is_empty() {
        return None;
    }
    let expr = &exprs[0].expression;

    expr.find("inputs.").map(|start| {
        let rest = &expr[start + 7..];
        let end = rest
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .unwrap_or(rest.len());
        rest[..end].to_string()
    })
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

    #[test]
    fn test_parse_with_quotes() {
        let input = "'$(inputs.val)'";
        let mut inputs = HashMap::new();
        inputs.insert(
            "val".to_string(),
            DefaultValue::Any(serde_yaml::Value::String("val".to_string())),
        );

        let result = do_eval(
            input,
            &EvaluationContext {
                inputs: Some(&inputs),
                ijsr: Some(&InlineJavascriptRequirement::default()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(result.as_str().unwrap(), "'val'");
    }

    #[test]
    fn test_parse_with_escaped() {
        let input = r"'\$(inputs.val)'";
        let mut inputs = HashMap::new();
        inputs.insert(
            "val".to_string(),
            DefaultValue::Any(serde_yaml::Value::String("val".to_string())),
        );

        let result = do_eval(
            input,
            &EvaluationContext {
                inputs: Some(&inputs),
                ijsr: Some(&InlineJavascriptRequirement::default()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(result.as_str().unwrap(), "'$(inputs.val)'");
    }

    #[test]
    fn test_parse_with_double_backslash() {
        let input = r"'\\$(inputs.val)'";
        let mut inputs = HashMap::new();
        inputs.insert(
            "val".to_string(),
            DefaultValue::Any(serde_yaml::Value::String("val".to_string())),
        );

        let result = do_eval(
            input,
            &EvaluationContext {
                inputs: Some(&inputs),
                ijsr: Some(&InlineJavascriptRequirement::default()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(result.as_str().unwrap(), r"'\val'");
    }
}

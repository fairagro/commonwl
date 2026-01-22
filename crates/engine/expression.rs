use std::collections::HashMap;

pub fn do_eval(
    expression: &str,
    context: Option<serde_json::Value>,
) -> anyhow::Result<serde_yaml::Value> {
    let expr = unwrap_expr(expression);
    
    let context = context.unwrap_or_default();
    let map = HashMap::from([("self", context)]);

    //simple engine uses jmespath
    let expr = jmespath::compile(expr)?;
    let data = jmespath::Variable::from_serializable(map)?;
    let result = expr.search(data)?;
    let yaml = serde_yaml::to_value(&result)?;
    Ok(yaml)
}

fn unwrap_expr(expr: &str) -> &str {
    expr.strip_prefix("$(")
        .and_then(|s| s.strip_suffix(")"))
        .unwrap_or(expr)
}

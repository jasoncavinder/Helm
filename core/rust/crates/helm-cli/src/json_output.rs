use serde_json::{Value, json};

pub(crate) fn build_json_payload_lines(
    schema: &str,
    data: Value,
    ndjson_mode: bool,
    generated_at: i64,
    schema_version: u32,
) -> Vec<Value> {
    let build = |item_data: Value| {
        json!({
            "schema": schema,
            "schema_version": schema_version,
            "generated_at": generated_at,
            "data": item_data
        })
    };

    ndjson_payload_items(data, ndjson_mode)
        .into_iter()
        .map(build)
        .collect()
}

fn ndjson_payload_items(data: Value, ndjson_mode: bool) -> Vec<Value> {
    if !ndjson_mode {
        return vec![data];
    }

    match data {
        Value::Array(items) if items.is_empty() => vec![Value::Array(Vec::new())],
        Value::Array(items) => items,
        other => vec![other],
    }
}

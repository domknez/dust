//! Reading the JSON the sources answer with, which isn't strict about types:
//! ids and counts come as numbers or as text, missing fields as null.

use serde_json::Value;

/// String field, tolerating numbers and nulls.
pub fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Unsigned number field, tolerating numeric strings; 0 when absent.
pub fn number(v: &Value) -> u64 {
    match v {
        Value::Number(n) => n.as_u64().unwrap_or(0),
        Value::String(s) => s.parse().unwrap_or(0),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fields_of_either_type() {
        assert_eq!((text(&json!("a")), text(&json!(12)), text(&json!(null))), ("a".into(), "12".into(), String::new()));
        assert_eq!((number(&json!(7)), number(&json!("7")), number(&json!("x")), number(&json!(null))), (7, 7, 0, 0));
    }
}

//! Strict MCP argument readers — present-but-wrong-type is an error, not a default.

use anyhow::{anyhow, Result};
use serde_json::Value;

pub fn optional_u32(args: &Value, key: &str) -> Result<Option<u32>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => Ok(Some(
            n.as_u64()
                .and_then(|v| u32::try_from(v).ok())
                .ok_or_else(|| anyhow!("{key} must be a non-negative integer"))?,
        )),
        Some(other) => Err(anyhow!(
            "{key} must be a number, not {ty}",
            ty = type_name(other)
        )),
    }
}

pub fn u32_or(args: &Value, key: &str, fallback: u32) -> Result<u32> {
    Ok(optional_u32(args, key)?.unwrap_or(fallback))
}

pub fn optional_bool(args: &Value, key: &str) -> Result<Option<bool>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(other) => Err(anyhow!(
            "{key} must be a boolean, not {ty}",
            ty = type_name(other)
        )),
    }
}

pub fn bool_or(args: &Value, key: &str, fallback: bool) -> Result<bool> {
    Ok(optional_bool(args, key)?.unwrap_or(fallback))
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn missing_uses_default_present_wrong_type_errors() {
        let empty = json!({});
        assert_eq!(u32_or(&empty, "n", 1).unwrap(), 1);
        assert_eq!(bool_or(&empty, "no_wait", false).unwrap(), false);

        let num = json!({"n": 4, "no_wait": true, "duration": 8});
        assert_eq!(u32_or(&num, "n", 1).unwrap(), 4);
        assert_eq!(optional_u32(&num, "duration").unwrap(), Some(8));
        assert!(bool_or(&num, "no_wait", false).unwrap());

        assert!(u32_or(&json!({"n": "4"}), "n", 1).is_err());
        assert!(bool_or(&json!({"no_wait": "true"}), "no_wait", false).is_err());
        assert!(optional_u32(&json!({"duration": true}), "duration").is_err());
    }
}

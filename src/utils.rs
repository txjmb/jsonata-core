// Utility functions and helpers
// Mirrors utils.js from the reference implementation

use serde_json::Value;

/// Check if a value is numeric
pub fn is_numeric(value: &Value) -> bool {
    matches!(value, Value::Number(_))
}

/// Check if a value is a string
pub fn is_string(value: &Value) -> bool {
    matches!(value, Value::String(_))
}

/// Check if a value is an array
pub fn is_array(value: &Value) -> bool {
    matches!(value, Value::Array(_))
}

/// Check if a value is an object
pub fn is_object(value: &Value) -> bool {
    matches!(value, Value::Object(_))
}

/// Check if a value is null or undefined (None)
pub fn is_null(value: &Value) -> bool {
    matches!(value, Value::Null)
}

/// Deep clone a JSON value
pub fn deep_clone(value: &Value) -> Value {
    value.clone()
}

/// Convert value to array (wraps non-arrays)
pub fn to_array(value: &Value) -> Vec<Value> {
    match value {
        Value::Array(arr) => arr.clone(),
        _ => vec![value.clone()],
    }
}

/// Flatten nested arrays
pub fn flatten(arr: &[Value]) -> Vec<Value> {
    let mut result = Vec::new();
    for item in arr {
        if let Value::Array(inner) = item {
            result.extend(flatten(inner));
        } else {
            result.push(item.clone());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_checks() {
        assert!(is_numeric(&serde_json::json!(42)));
        assert!(is_string(&Value::String("hello".to_string())));
        assert!(is_array(&Value::Array(vec![])));
        assert!(is_object(&serde_json::json!({})));
        assert!(is_null(&Value::Null));
    }

    #[test]
    fn test_to_array() {
        let value = serde_json::json!(42);
        let arr = to_array(&value);
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0], value);

        let arr_value = Value::Array(vec![serde_json::json!(1), serde_json::json!(2)]);
        let arr2 = to_array(&arr_value);
        assert_eq!(arr2.len(), 2);
    }

    #[test]
    fn test_flatten() {
        let nested = vec![
            serde_json::json!(1),
            Value::Array(vec![serde_json::json!(2), serde_json::json!(3)]),
            serde_json::json!(4),
        ];
        let flat = flatten(&nested);
        assert_eq!(flat.len(), 4);
    }
}

// Utility functions and helpers
// Mirrors utils.js from the reference implementation
//
// These utilities are currently unused because the evaluator and functions
// modules use inline type checks. They are retained for API parity with
// the JavaScript reference implementation and potential future use.

#![allow(dead_code)]

use serde_json::Value;

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

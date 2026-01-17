// Built-in function implementations
// Mirrors functions.js from the reference implementation

use serde_json::Value;
use thiserror::Error;

/// Function errors
#[derive(Error, Debug)]
pub enum FunctionError {
    #[error("Argument error: {0}")]
    ArgumentError(String),

    #[error("Type error: {0}")]
    TypeError(String),

    #[error("Runtime error: {0}")]
    RuntimeError(String),
}

/// Built-in string functions
pub mod string {
    use super::*;

    /// $string() - Cast value to string
    pub fn string(value: &Value) -> Result<Value, FunctionError> {
        // TODO: Implement
        Ok(Value::String(format!("{}", value)))
    }

    /// $length() - Get string length
    pub fn length(s: &str) -> Result<Value, FunctionError> {
        // TODO: Implement proper Unicode handling
        Ok(Value::Number(s.len().into()))
    }

    /// $uppercase() - Convert to uppercase
    pub fn uppercase(s: &str) -> Result<Value, FunctionError> {
        Ok(Value::String(s.to_uppercase()))
    }

    /// $lowercase() - Convert to lowercase
    pub fn lowercase(s: &str) -> Result<Value, FunctionError> {
        Ok(Value::String(s.to_lowercase()))
    }
}

/// Built-in numeric functions
pub mod numeric {
    use super::*;

    /// $number() - Cast value to number
    pub fn number(value: &Value) -> Result<Value, FunctionError> {
        // TODO: Implement proper conversion logic
        match value {
            Value::Number(n) => Ok(Value::Number(n.clone())),
            Value::String(s) => s
                .parse::<f64>()
                .map(|n| serde_json::json!(n))
                .map_err(|_| FunctionError::TypeError(format!("Cannot convert '{}' to number", s))),
            _ => Err(FunctionError::TypeError("Cannot convert to number".to_string())),
        }
    }

    /// $sum() - Sum array of numbers
    pub fn sum(arr: &[Value]) -> Result<Value, FunctionError> {
        // TODO: Implement
        Ok(Value::Number(0.into()))
    }
}

/// Built-in array functions
pub mod array {
    use super::*;

    /// $count() - Count array elements
    pub fn count(arr: &[Value]) -> Result<Value, FunctionError> {
        Ok(Value::Number(arr.len().into()))
    }

    /// $append() - Append to array
    pub fn append(arr1: &[Value], arr2: &[Value]) -> Result<Value, FunctionError> {
        // TODO: Implement
        Ok(Value::Array(vec![]))
    }
}

/// Built-in object functions
pub mod object {
    use super::*;

    /// $keys() - Get object keys
    pub fn keys(obj: &serde_json::Map<String, Value>) -> Result<Value, FunctionError> {
        let keys: Vec<Value> = obj.keys().map(|k| Value::String(k.clone())).collect();
        Ok(Value::Array(keys))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_functions() {
        assert_eq!(
            string::uppercase("hello").unwrap(),
            Value::String("HELLO".to_string())
        );

        assert_eq!(
            string::lowercase("HELLO").unwrap(),
            Value::String("hello".to_string())
        );
    }
}

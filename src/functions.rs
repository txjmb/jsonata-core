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
    /// Converts a value to a string representation following JSONata semantics
    pub fn string(value: &Value) -> Result<Value, FunctionError> {
        let result = match value {
            Value::String(s) => s.clone(),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    i.to_string()
                } else if let Some(f) = n.as_f64() {
                    // Remove trailing zeros for cleaner output
                    let s = f.to_string();
                    if s.contains('.') {
                        s.trim_end_matches('0').trim_end_matches('.').to_string()
                    } else {
                        s
                    }
                } else {
                    n.to_string()
                }
            }
            Value::Bool(b) => b.to_string(),
            Value::Null => String::new(),
            Value::Array(_) | Value::Object(_) => {
                return Err(FunctionError::TypeError(
                    "Cannot convert array or object to string".to_string(),
                ))
            }
        };
        Ok(Value::String(result))
    }

    /// $length() - Get string length with proper Unicode support
    /// Returns the number of Unicode characters (not bytes)
    pub fn length(s: &str) -> Result<Value, FunctionError> {
        Ok(Value::Number((s.chars().count() as i64).into()))
    }

    /// $uppercase() - Convert to uppercase
    pub fn uppercase(s: &str) -> Result<Value, FunctionError> {
        Ok(Value::String(s.to_uppercase()))
    }

    /// $lowercase() - Convert to lowercase
    pub fn lowercase(s: &str) -> Result<Value, FunctionError> {
        Ok(Value::String(s.to_lowercase()))
    }

    /// $substring(str, start, length) - Extract substring
    /// Extracts a substring from a string using Unicode character positions
    pub fn substring(s: &str, start: i64, length: Option<i64>) -> Result<Value, FunctionError> {
        let chars: Vec<char> = s.chars().collect();
        let total_len = chars.len() as i64;

        // Handle negative start positions (count from end)
        let start_pos = if start < 0 {
            (total_len + start).max(0)
        } else {
            start.min(total_len)
        } as usize;

        let end_pos = if let Some(len) = length {
            if len < 0 {
                return Err(FunctionError::ArgumentError(
                    "Length cannot be negative".to_string(),
                ));
            }
            (start_pos + len as usize).min(chars.len())
        } else {
            chars.len()
        };

        let result: String = chars[start_pos..end_pos].iter().collect();
        Ok(Value::String(result))
    }

    /// $substringBefore(str, separator) - Get substring before separator
    pub fn substring_before(s: &str, separator: &str) -> Result<Value, FunctionError> {
        if separator.is_empty() {
            return Ok(Value::String(String::new()));
        }

        let result = s.split(separator).next().unwrap_or(s).to_string();
        Ok(Value::String(result))
    }

    /// $substringAfter(str, separator) - Get substring after separator
    pub fn substring_after(s: &str, separator: &str) -> Result<Value, FunctionError> {
        if separator.is_empty() {
            return Ok(Value::String(s.to_string()));
        }

        if let Some(pos) = s.find(separator) {
            let result = s[pos + separator.len()..].to_string();
            Ok(Value::String(result))
        } else {
            Ok(Value::String(String::new()))
        }
    }

    /// $trim(str) - Remove leading and trailing whitespace
    pub fn trim(s: &str) -> Result<Value, FunctionError> {
        Ok(Value::String(s.trim().to_string()))
    }

    /// $contains(str, pattern) - Check if string contains substring
    pub fn contains(s: &str, pattern: &str) -> Result<Value, FunctionError> {
        Ok(Value::Bool(s.contains(pattern)))
    }

    /// $split(str, separator, limit) - Split string into array
    pub fn split(s: &str, separator: &str, limit: Option<usize>) -> Result<Value, FunctionError> {
        if separator.is_empty() {
            // Split into individual characters
            let chars: Vec<Value> = s.chars()
                .map(|c| Value::String(c.to_string()))
                .collect();
            return Ok(Value::Array(chars));
        }

        let parts: Vec<Value> = if let Some(lim) = limit {
            s.splitn(lim, separator)
                .map(|p| Value::String(p.to_string()))
                .collect()
        } else {
            s.split(separator)
                .map(|p| Value::String(p.to_string()))
                .collect()
        };

        Ok(Value::Array(parts))
    }

    /// $join(array, separator) - Join array into string
    pub fn join(arr: &[Value], separator: Option<&str>) -> Result<Value, FunctionError> {
        let sep = separator.unwrap_or("");
        let parts: Result<Vec<String>, FunctionError> = arr
            .iter()
            .map(|v| match v {
                Value::String(s) => Ok(s.clone()),
                Value::Number(n) => Ok(n.to_string()),
                Value::Bool(b) => Ok(b.to_string()),
                Value::Null => Ok(String::new()),
                _ => Err(FunctionError::TypeError(
                    "Cannot join array containing objects or nested arrays".to_string(),
                )),
            })
            .collect();

        let parts = parts?;
        Ok(Value::String(parts.join(sep)))
    }

    /// $replace(str, pattern, replacement, limit) - Replace substring
    pub fn replace(
        s: &str,
        pattern: &str,
        replacement: &str,
        limit: Option<usize>,
    ) -> Result<Value, FunctionError> {
        if pattern.is_empty() {
            return Ok(Value::String(s.to_string()));
        }

        let result = if let Some(lim) = limit {
            let mut remaining = s;
            let mut output = String::new();
            let mut count = 0;

            while count < lim {
                if let Some(pos) = remaining.find(pattern) {
                    output.push_str(&remaining[..pos]);
                    output.push_str(replacement);
                    remaining = &remaining[pos + pattern.len()..];
                    count += 1;
                } else {
                    output.push_str(remaining);
                    break;
                }
            }
            if count == lim {
                output.push_str(remaining);
            }
            output
        } else {
            s.replace(pattern, replacement)
        };

        Ok(Value::String(result))
    }
}

/// Built-in numeric functions
pub mod numeric {
    use super::*;

    /// $number() - Cast value to number
    pub fn number(value: &Value) -> Result<Value, FunctionError> {
        match value {
            Value::Number(n) => Ok(Value::Number(n.clone())),
            Value::String(s) => {
                let trimmed = s.trim();
                trimmed
                    .parse::<f64>()
                    .map(|n| serde_json::json!(n))
                    .map_err(|_| {
                        FunctionError::TypeError(format!("Cannot convert '{}' to number", s))
                    })
            }
            Value::Bool(true) => Ok(serde_json::json!(1)),
            Value::Bool(false) => Ok(serde_json::json!(0)),
            Value::Null => Err(FunctionError::TypeError(
                "Cannot convert null to number".to_string(),
            )),
            _ => Err(FunctionError::TypeError(
                "Cannot convert array or object to number".to_string(),
            )),
        }
    }

    /// $sum(array) - Sum array of numbers
    pub fn sum(arr: &[Value]) -> Result<Value, FunctionError> {
        if arr.is_empty() {
            return Ok(serde_json::json!(0));
        }

        let mut total = 0.0;
        for value in arr {
            match value {
                Value::Number(n) => {
                    total += n.as_f64().ok_or_else(|| {
                        FunctionError::TypeError("Invalid number in array".to_string())
                    })?;
                }
                _ => {
                    return Err(FunctionError::TypeError(format!(
                        "sum() requires all array elements to be numbers, got: {:?}",
                        value
                    )))
                }
            }
        }
        Ok(serde_json::json!(total))
    }

    /// $max(array) - Maximum value
    pub fn max(arr: &[Value]) -> Result<Value, FunctionError> {
        if arr.is_empty() {
            return Ok(Value::Null);
        }

        let mut max_val = f64::NEG_INFINITY;
        for value in arr {
            match value {
                Value::Number(n) => {
                    let num = n.as_f64().ok_or_else(|| {
                        FunctionError::TypeError("Invalid number in array".to_string())
                    })?;
                    if num > max_val {
                        max_val = num;
                    }
                }
                _ => {
                    return Err(FunctionError::TypeError(
                        "max() requires all array elements to be numbers".to_string(),
                    ))
                }
            }
        }
        Ok(serde_json::json!(max_val))
    }

    /// $min(array) - Minimum value
    pub fn min(arr: &[Value]) -> Result<Value, FunctionError> {
        if arr.is_empty() {
            return Ok(Value::Null);
        }

        let mut min_val = f64::INFINITY;
        for value in arr {
            match value {
                Value::Number(n) => {
                    let num = n.as_f64().ok_or_else(|| {
                        FunctionError::TypeError("Invalid number in array".to_string())
                    })?;
                    if num < min_val {
                        min_val = num;
                    }
                }
                _ => {
                    return Err(FunctionError::TypeError(
                        "min() requires all array elements to be numbers".to_string(),
                    ))
                }
            }
        }
        Ok(serde_json::json!(min_val))
    }

    /// $average(array) - Average value
    pub fn average(arr: &[Value]) -> Result<Value, FunctionError> {
        if arr.is_empty() {
            return Ok(Value::Null);
        }

        let sum_result = sum(arr)?;
        if let Value::Number(n) = sum_result {
            let avg = n.as_f64().unwrap() / arr.len() as f64;
            Ok(serde_json::json!(avg))
        } else {
            Err(FunctionError::RuntimeError("Sum failed".to_string()))
        }
    }

    /// $abs(number) - Absolute value
    pub fn abs(n: f64) -> Result<Value, FunctionError> {
        Ok(serde_json::json!(n.abs()))
    }

    /// $floor(number) - Floor
    pub fn floor(n: f64) -> Result<Value, FunctionError> {
        Ok(serde_json::json!(n.floor()))
    }

    /// $ceil(number) - Ceiling
    pub fn ceil(n: f64) -> Result<Value, FunctionError> {
        Ok(serde_json::json!(n.ceil()))
    }

    /// $round(number, precision) - Round to specified decimal places
    pub fn round(n: f64, precision: Option<i32>) -> Result<Value, FunctionError> {
        let prec = precision.unwrap_or(0);
        if prec < 0 {
            return Err(FunctionError::ArgumentError(
                "Precision cannot be negative".to_string(),
            ));
        }

        let multiplier = 10_f64.powi(prec);
        let rounded = (n * multiplier).round() / multiplier;
        Ok(serde_json::json!(rounded))
    }

    /// $sqrt(number) - Square root
    pub fn sqrt(n: f64) -> Result<Value, FunctionError> {
        if n < 0.0 {
            return Err(FunctionError::ArgumentError(
                "Cannot take square root of negative number".to_string(),
            ));
        }
        Ok(serde_json::json!(n.sqrt()))
    }

    /// $power(base, exponent) - Power
    pub fn power(base: f64, exponent: f64) -> Result<Value, FunctionError> {
        let result = base.powf(exponent);
        if result.is_nan() || result.is_infinite() {
            return Err(FunctionError::RuntimeError(
                "Power operation resulted in invalid number".to_string(),
            ));
        }
        Ok(serde_json::json!(result))
    }
}

/// Built-in array functions
pub mod array {
    use super::*;

    /// $count(array) - Count array elements
    pub fn count(arr: &[Value]) -> Result<Value, FunctionError> {
        Ok(Value::Number((arr.len() as i64).into()))
    }

    /// $append(array1, array2) - Append arrays/values
    pub fn append(arr1: &[Value], val: &Value) -> Result<Value, FunctionError> {
        let mut result = arr1.to_vec();
        match val {
            Value::Array(arr2) => result.extend_from_slice(arr2),
            other => result.push(other.clone()),
        }
        Ok(Value::Array(result))
    }

    /// $reverse(array) - Reverse array
    pub fn reverse(arr: &[Value]) -> Result<Value, FunctionError> {
        let mut result = arr.to_vec();
        result.reverse();
        Ok(Value::Array(result))
    }

    /// $sort(array) - Sort array
    pub fn sort(arr: &[Value]) -> Result<Value, FunctionError> {
        let mut result = arr.to_vec();

        // Check if all elements are of comparable types
        let all_numbers = result.iter().all(|v| matches!(v, Value::Number(_)));
        let all_strings = result.iter().all(|v| matches!(v, Value::String(_)));

        if all_numbers {
            result.sort_by(|a, b| {
                let a_num = a.as_f64().unwrap();
                let b_num = b.as_f64().unwrap();
                a_num.partial_cmp(&b_num).unwrap_or(std::cmp::Ordering::Equal)
            });
        } else if all_strings {
            result.sort_by(|a, b| {
                let a_str = a.as_str().unwrap();
                let b_str = b.as_str().unwrap();
                a_str.cmp(b_str)
            });
        } else {
            return Err(FunctionError::TypeError(
                "sort() requires all elements to be of the same comparable type".to_string(),
            ));
        }

        Ok(Value::Array(result))
    }

    /// $distinct(array) - Get unique elements
    pub fn distinct(arr: &[Value]) -> Result<Value, FunctionError> {
        let mut result = Vec::new();
        let mut seen = Vec::new();

        for value in arr {
            let mut is_new = true;
            for seen_value in &seen {
                if values_equal(value, seen_value) {
                    is_new = false;
                    break;
                }
            }
            if is_new {
                seen.push(value.clone());
                result.push(value.clone());
            }
        }

        Ok(Value::Array(result))
    }

    /// $exists(value) - Check if value exists (not null/undefined)
    pub fn exists(value: &Value) -> Result<Value, FunctionError> {
        Ok(Value::Bool(!matches!(value, Value::Null)))
    }

    /// Helper function to compare values for equality
    fn values_equal(a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Number(a), Value::Number(b)) => {
                a.as_f64().unwrap() == b.as_f64().unwrap()
            }
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Array(a), Value::Array(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| values_equal(x, y))
            }
            (Value::Object(a), Value::Object(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .all(|(k, v)| b.get(k).map_or(false, |v2| values_equal(v, v2)))
            }
            _ => false,
        }
    }
}

/// Built-in object functions
pub mod object {
    use super::*;

    /// $keys(object) - Get object keys
    pub fn keys(obj: &serde_json::Map<String, Value>) -> Result<Value, FunctionError> {
        let keys: Vec<Value> = obj.keys().map(|k| Value::String(k.clone())).collect();
        Ok(Value::Array(keys))
    }

    /// $lookup(object, key) - Lookup value by key
    pub fn lookup(obj: &serde_json::Map<String, Value>, key: &str) -> Result<Value, FunctionError> {
        Ok(obj.get(key).cloned().unwrap_or(Value::Null))
    }

    /// $spread(object) - Spread object into array of key-value pairs
    pub fn spread(obj: &serde_json::Map<String, Value>) -> Result<Value, FunctionError> {
        let pairs: Vec<Value> = obj
            .iter()
            .map(|(k, v)| {
                let mut pair = serde_json::Map::new();
                pair.insert("key".to_string(), Value::String(k.clone()));
                pair.insert("value".to_string(), v.clone());
                Value::Object(pair)
            })
            .collect();
        Ok(Value::Array(pairs))
    }

    /// $merge(objects) - Merge multiple objects
    pub fn merge(objects: &[Value]) -> Result<Value, FunctionError> {
        let mut result = serde_json::Map::new();

        for obj in objects {
            match obj {
                Value::Object(map) => {
                    for (k, v) in map {
                        result.insert(k.clone(), v.clone());
                    }
                }
                _ => {
                    return Err(FunctionError::TypeError(
                        "merge() requires all arguments to be objects".to_string(),
                    ))
                }
            }
        }

        Ok(Value::Object(result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== String Functions Tests =====

    #[test]
    fn test_string_conversion() {
        // String to string
        assert_eq!(
            string::string(&Value::String("hello".to_string())).unwrap(),
            Value::String("hello".to_string())
        );

        // Number to string
        assert_eq!(
            string::string(&serde_json::json!(42)).unwrap(),
            Value::String("42".to_string())
        );

        // Float to string
        assert_eq!(
            string::string(&serde_json::json!(3.14)).unwrap(),
            Value::String("3.14".to_string())
        );

        // Boolean to string
        assert_eq!(
            string::string(&Value::Bool(true)).unwrap(),
            Value::String("true".to_string())
        );

        // Null to empty string
        assert_eq!(
            string::string(&Value::Null).unwrap(),
            Value::String(String::new())
        );

        // Array should error
        assert!(string::string(&serde_json::json!([1, 2, 3])).is_err());
    }

    #[test]
    fn test_length() {
        assert_eq!(string::length("hello").unwrap(), serde_json::json!(5));
        assert_eq!(string::length("").unwrap(), serde_json::json!(0));
        // Unicode support
        assert_eq!(string::length("Hello 世界").unwrap(), serde_json::json!(8));
        assert_eq!(string::length("🎉🎊").unwrap(), serde_json::json!(2));
    }

    #[test]
    fn test_uppercase_lowercase() {
        assert_eq!(
            string::uppercase("hello").unwrap(),
            Value::String("HELLO".to_string())
        );
        assert_eq!(
            string::lowercase("HELLO").unwrap(),
            Value::String("hello".to_string())
        );
        assert_eq!(
            string::uppercase("Hello World").unwrap(),
            Value::String("HELLO WORLD".to_string())
        );
    }

    #[test]
    fn test_substring() {
        // Basic substring
        assert_eq!(
            string::substring("hello world", 0, Some(5)).unwrap(),
            Value::String("hello".to_string())
        );

        // From position to end
        assert_eq!(
            string::substring("hello world", 6, None).unwrap(),
            Value::String("world".to_string())
        );

        // Negative start position
        assert_eq!(
            string::substring("hello world", -5, Some(5)).unwrap(),
            Value::String("world".to_string())
        );

        // Unicode support
        assert_eq!(
            string::substring("Hello 世界", 6, Some(2)).unwrap(),
            Value::String("世界".to_string())
        );

        // Negative length should error
        assert!(string::substring("hello", 0, Some(-1)).is_err());
    }

    #[test]
    fn test_substring_before_after() {
        // substringBefore
        assert_eq!(
            string::substring_before("hello world", " ").unwrap(),
            Value::String("hello".to_string())
        );
        assert_eq!(
            string::substring_before("hello world", "x").unwrap(),
            Value::String("hello world".to_string())
        );
        assert_eq!(
            string::substring_before("hello world", "").unwrap(),
            Value::String(String::new())
        );

        // substringAfter
        assert_eq!(
            string::substring_after("hello world", " ").unwrap(),
            Value::String("world".to_string())
        );
        assert_eq!(
            string::substring_after("hello world", "x").unwrap(),
            Value::String(String::new())
        );
        assert_eq!(
            string::substring_after("hello world", "").unwrap(),
            Value::String("hello world".to_string())
        );
    }

    #[test]
    fn test_trim() {
        assert_eq!(
            string::trim("  hello  ").unwrap(),
            Value::String("hello".to_string())
        );
        assert_eq!(
            string::trim("hello").unwrap(),
            Value::String("hello".to_string())
        );
        assert_eq!(
            string::trim("\t\nhello\r\n").unwrap(),
            Value::String("hello".to_string())
        );
    }

    #[test]
    fn test_contains() {
        assert_eq!(string::contains("hello world", "world").unwrap(), Value::Bool(true));
        assert_eq!(string::contains("hello world", "xyz").unwrap(), Value::Bool(false));
        assert_eq!(string::contains("hello world", "").unwrap(), Value::Bool(true));
    }

    #[test]
    fn test_split() {
        // Split with separator
        assert_eq!(
            string::split("a,b,c", ",", None).unwrap(),
            serde_json::json!(["a", "b", "c"])
        );

        // Split with limit
        assert_eq!(
            string::split("a,b,c,d", ",", Some(2)).unwrap(),
            serde_json::json!(["a", "b,c,d"])
        );

        // Split with empty separator (split into chars)
        assert_eq!(
            string::split("abc", "", None).unwrap(),
            serde_json::json!(["a", "b", "c"])
        );
    }

    #[test]
    fn test_join() {
        // Join with separator
        let arr = vec![
            Value::String("a".to_string()),
            Value::String("b".to_string()),
            Value::String("c".to_string()),
        ];
        assert_eq!(
            string::join(&arr, Some(",")).unwrap(),
            Value::String("a,b,c".to_string())
        );

        // Join without separator
        assert_eq!(
            string::join(&arr, None).unwrap(),
            Value::String("abc".to_string())
        );

        // Join with numbers
        let arr = vec![serde_json::json!(1), serde_json::json!(2), serde_json::json!(3)];
        assert_eq!(
            string::join(&arr, Some("-")).unwrap(),
            Value::String("1-2-3".to_string())
        );
    }

    #[test]
    fn test_replace() {
        // Replace all occurrences
        assert_eq!(
            string::replace("hello hello", "hello", "hi", None).unwrap(),
            Value::String("hi hi".to_string())
        );

        // Replace with limit
        assert_eq!(
            string::replace("hello hello hello", "hello", "hi", Some(2)).unwrap(),
            Value::String("hi hi hello".to_string())
        );

        // Replace empty pattern (no change)
        assert_eq!(
            string::replace("hello", "", "x", None).unwrap(),
            Value::String("hello".to_string())
        );
    }

    // ===== Numeric Functions Tests =====

    #[test]
    fn test_number_conversion() {
        // Number to number
        assert_eq!(numeric::number(&serde_json::json!(42)).unwrap(), serde_json::json!(42));

        // String to number
        assert_eq!(
            numeric::number(&Value::String("42".to_string())).unwrap(),
            serde_json::json!(42.0)
        );
        assert_eq!(
            numeric::number(&Value::String("3.14".to_string())).unwrap(),
            serde_json::json!(3.14)
        );
        assert_eq!(
            numeric::number(&Value::String("  123  ".to_string())).unwrap(),
            serde_json::json!(123.0)
        );

        // Boolean to number
        assert_eq!(numeric::number(&Value::Bool(true)).unwrap(), serde_json::json!(1));
        assert_eq!(numeric::number(&Value::Bool(false)).unwrap(), serde_json::json!(0));

        // Invalid conversions
        assert!(numeric::number(&Value::Null).is_err());
        assert!(numeric::number(&Value::String("not a number".to_string())).is_err());
    }

    #[test]
    fn test_sum() {
        // Sum of numbers
        let arr = vec![serde_json::json!(1), serde_json::json!(2), serde_json::json!(3)];
        assert_eq!(numeric::sum(&arr).unwrap(), serde_json::json!(6.0));

        // Empty array
        assert_eq!(numeric::sum(&[]).unwrap(), serde_json::json!(0));

        // Array with non-numbers should error
        let arr = vec![serde_json::json!(1), Value::String("2".to_string())];
        assert!(numeric::sum(&arr).is_err());
    }

    #[test]
    fn test_max_min() {
        let arr = vec![serde_json::json!(3), serde_json::json!(1), serde_json::json!(4), serde_json::json!(2)];

        assert_eq!(numeric::max(&arr).unwrap(), serde_json::json!(4.0));
        assert_eq!(numeric::min(&arr).unwrap(), serde_json::json!(1.0));

        // Empty array
        assert_eq!(numeric::max(&[]).unwrap(), Value::Null);
        assert_eq!(numeric::min(&[]).unwrap(), Value::Null);
    }

    #[test]
    fn test_average() {
        let arr = vec![serde_json::json!(1), serde_json::json!(2), serde_json::json!(3), serde_json::json!(4)];
        assert_eq!(numeric::average(&arr).unwrap(), serde_json::json!(2.5));

        // Empty array
        assert_eq!(numeric::average(&[]).unwrap(), Value::Null);
    }

    #[test]
    fn test_math_functions() {
        // abs
        assert_eq!(numeric::abs(-5.5).unwrap(), serde_json::json!(5.5));
        assert_eq!(numeric::abs(5.5).unwrap(), serde_json::json!(5.5));

        // floor
        assert_eq!(numeric::floor(3.7).unwrap(), serde_json::json!(3.0));
        assert_eq!(numeric::floor(-3.7).unwrap(), serde_json::json!(-4.0));

        // ceil
        assert_eq!(numeric::ceil(3.2).unwrap(), serde_json::json!(4.0));
        assert_eq!(numeric::ceil(-3.2).unwrap(), serde_json::json!(-3.0));

        // round
        assert_eq!(numeric::round(3.14159, Some(2)).unwrap(), serde_json::json!(3.14));
        assert_eq!(numeric::round(3.14159, None).unwrap(), serde_json::json!(3.0));
        assert!(numeric::round(3.14, Some(-1)).is_err());

        // sqrt
        assert_eq!(numeric::sqrt(16.0).unwrap(), serde_json::json!(4.0));
        assert!(numeric::sqrt(-1.0).is_err());

        // power
        assert_eq!(numeric::power(2.0, 3.0).unwrap(), serde_json::json!(8.0));
        assert_eq!(numeric::power(9.0, 0.5).unwrap(), serde_json::json!(3.0));
    }

    // ===== Array Functions Tests =====

    #[test]
    fn test_count() {
        let arr = vec![serde_json::json!(1), serde_json::json!(2), serde_json::json!(3)];
        assert_eq!(array::count(&arr).unwrap(), serde_json::json!(3));
        assert_eq!(array::count(&[]).unwrap(), serde_json::json!(0));
    }

    #[test]
    fn test_append() {
        let arr1 = vec![serde_json::json!(1), serde_json::json!(2)];

        // Append a single value
        let result = array::append(&arr1, &serde_json::json!(3)).unwrap();
        assert_eq!(result, serde_json::json!([1, 2, 3]));

        // Append an array
        let arr2 = serde_json::json!([3, 4]);
        let result = array::append(&arr1, &arr2).unwrap();
        assert_eq!(result, serde_json::json!([1, 2, 3, 4]));
    }

    #[test]
    fn test_reverse() {
        let arr = vec![serde_json::json!(1), serde_json::json!(2), serde_json::json!(3)];
        assert_eq!(array::reverse(&arr).unwrap(), serde_json::json!([3, 2, 1]));
    }

    #[test]
    fn test_sort() {
        // Sort numbers
        let arr = vec![serde_json::json!(3), serde_json::json!(1), serde_json::json!(4), serde_json::json!(2)];
        assert_eq!(array::sort(&arr).unwrap(), serde_json::json!([1, 2, 3, 4]));

        // Sort strings
        let arr = vec![
            Value::String("charlie".to_string()),
            Value::String("alice".to_string()),
            Value::String("bob".to_string()),
        ];
        assert_eq!(
            array::sort(&arr).unwrap(),
            serde_json::json!(["alice", "bob", "charlie"])
        );

        // Mixed types should error
        let arr = vec![serde_json::json!(1), Value::String("a".to_string())];
        assert!(array::sort(&arr).is_err());
    }

    #[test]
    fn test_distinct() {
        let arr = vec![
            serde_json::json!(1),
            serde_json::json!(2),
            serde_json::json!(1),
            serde_json::json!(3),
            serde_json::json!(2),
        ];
        assert_eq!(array::distinct(&arr).unwrap(), serde_json::json!([1, 2, 3]));

        // With strings
        let arr = vec![
            Value::String("a".to_string()),
            Value::String("b".to_string()),
            Value::String("a".to_string()),
        ];
        assert_eq!(array::distinct(&arr).unwrap(), serde_json::json!(["a", "b"]));
    }

    #[test]
    fn test_exists() {
        assert_eq!(array::exists(&serde_json::json!(42)).unwrap(), Value::Bool(true));
        assert_eq!(array::exists(&Value::String("hello".to_string())).unwrap(), Value::Bool(true));
        assert_eq!(array::exists(&Value::Null).unwrap(), Value::Bool(false));
    }

    // ===== Object Functions Tests =====

    #[test]
    fn test_keys() {
        let mut obj = serde_json::Map::new();
        obj.insert("name".to_string(), Value::String("Alice".to_string()));
        obj.insert("age".to_string(), serde_json::json!(30));

        let result = object::keys(&obj).unwrap();
        if let Value::Array(keys) = result {
            assert_eq!(keys.len(), 2);
            assert!(keys.contains(&Value::String("name".to_string())));
            assert!(keys.contains(&Value::String("age".to_string())));
        } else {
            panic!("Expected array of keys");
        }
    }

    #[test]
    fn test_lookup() {
        let mut obj = serde_json::Map::new();
        obj.insert("name".to_string(), Value::String("Alice".to_string()));
        obj.insert("age".to_string(), serde_json::json!(30));

        assert_eq!(
            object::lookup(&obj, "name").unwrap(),
            Value::String("Alice".to_string())
        );
        assert_eq!(object::lookup(&obj, "age").unwrap(), serde_json::json!(30));
        assert_eq!(object::lookup(&obj, "missing").unwrap(), Value::Null);
    }

    #[test]
    fn test_spread() {
        let mut obj = serde_json::Map::new();
        obj.insert("a".to_string(), serde_json::json!(1));
        obj.insert("b".to_string(), serde_json::json!(2));

        let result = object::spread(&obj).unwrap();
        if let Value::Array(pairs) = result {
            assert_eq!(pairs.len(), 2);
            // Check that each pair has "key" and "value" fields
            for pair in pairs {
                if let Value::Object(p) = pair {
                    assert!(p.contains_key("key"));
                    assert!(p.contains_key("value"));
                }
            }
        } else {
            panic!("Expected array of key-value pairs");
        }
    }

    #[test]
    fn test_merge() {
        let mut obj1 = serde_json::Map::new();
        obj1.insert("a".to_string(), serde_json::json!(1));
        obj1.insert("b".to_string(), serde_json::json!(2));

        let mut obj2 = serde_json::Map::new();
        obj2.insert("b".to_string(), serde_json::json!(3));
        obj2.insert("c".to_string(), serde_json::json!(4));

        let arr = vec![Value::Object(obj1), Value::Object(obj2)];
        let result = object::merge(&arr).unwrap();

        if let Value::Object(merged) = result {
            assert_eq!(merged.get("a"), Some(&serde_json::json!(1)));
            assert_eq!(merged.get("b"), Some(&serde_json::json!(3))); // Later value wins
            assert_eq!(merged.get("c"), Some(&serde_json::json!(4)));
        } else {
            panic!("Expected merged object");
        }
    }
}

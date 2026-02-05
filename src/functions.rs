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

/// Check if a Value is a function marker (lambda or builtin)
pub fn is_function_value(value: &Value) -> bool {
    if let Value::Object(obj) = value {
        obj.contains_key("__lambda__") || obj.contains_key("__builtin__")
    } else {
        false
    }
}

/// Built-in string functions
pub mod string {
    use super::*;
    use regex::Regex;

    /// Helper to detect and extract regex from a Value object
    pub fn extract_regex(value: &Value) -> Option<(String, String)> {
        if let Value::Object(obj) = value {
            if obj.get("__jsonata_regex__") == Some(&Value::Bool(true)) {
                if let (Some(Value::String(pattern)), Some(Value::String(flags))) =
                    (obj.get("pattern"), obj.get("flags")) {
                    return Some((pattern.clone(), flags.clone()));
                }
            }
        }
        None
    }

    /// Helper to build a Regex from pattern and flags
    pub fn build_regex(pattern: &str, flags: &str) -> Result<Regex, FunctionError> {
        // Convert JSONata flags to Rust regex flags
        let mut regex_pattern = String::new();

        // Add inline flags
        if !flags.is_empty() {
            regex_pattern.push_str("(?");
            if flags.contains('i') {
                regex_pattern.push('i');  // case-insensitive
            }
            if flags.contains('m') {
                regex_pattern.push('m');  // multi-line
            }
            if flags.contains('s') {
                regex_pattern.push('s');  // dot matches newline
            }
            regex_pattern.push(')');
        }

        regex_pattern.push_str(pattern);

        Regex::new(&regex_pattern).map_err(|e|
            FunctionError::ArgumentError(format!("Invalid regex: {}", e))
        )
    }

    /// $string(value, prettify) - Convert value to string
    ///
    /// - undefined inputs return undefined (but this is handled at call site)
    /// - strings returned unchanged
    /// - functions/lambdas return empty string
    /// - non-finite numbers (Infinity, NaN) throw error D3001
    /// - other values use JSON.stringify with number precision
    /// - prettify=true uses 2-space indentation
    pub fn string(value: &Value, prettify: Option<bool>) -> Result<Value, FunctionError> {
        // Check if this is a function or undefined first (before checking other types)
        if let Value::Object(obj) = value {
            if obj.get("__undefined__") == Some(&Value::Bool(true)) {
                return Ok(Value::String(String::new()));
            }
            if super::is_function_value(value) {
                return Ok(Value::String(String::new()));
            }
        }

        let result = match value {
            Value::String(s) => s.clone(),
            Value::Number(n) => {
                let f = n.as_f64().unwrap_or(0.0);
                // Check for non-finite numbers (Infinity, NaN)
                if !f.is_finite() {
                    return Err(FunctionError::RuntimeError(
                        format!("D3001: Attempting to invoke string function with non-finite number: {}", f)
                    ));
                }

                // Format numbers like JavaScript does
                if let Some(i) = n.as_i64() {
                    i.to_string()
                } else {
                    // Non-integer - use precision formatting
                    // JavaScript uses toPrecision(15) for non-integers in JSON.stringify
                    format_number_with_precision(f)
                }
            }
            Value::Bool(b) => b.to_string(),
            Value::Null => {
                // Explicit null goes through JSON.stringify to become "null"
                // Undefined variables are handled at the evaluator level
                "null".to_string()
            }
            Value::Array(_) | Value::Object(_) => {
                // JSON.stringify with optional prettification
                // Uses custom serialization to handle numbers and functions correctly
                let indent = if prettify.unwrap_or(false) { Some(2) } else { None };
                stringify_value_custom(value, indent)?
            }
        };
        Ok(Value::String(result))
    }

    /// Helper to format a number with precision like JavaScript's toPrecision(15)
    ///
    /// JavaScript uses `toPrecision(15)` which formats with 15 significant figures.
    /// This matches that behavior by:
    /// 1. Formatting with 15 significant figures
    /// 2. Removing trailing zeros
    /// 3. Converting back to number to normalize format
    fn format_number_with_precision(f: f64) -> String {
        // Format with 15 significant figures like JavaScript's toPrecision(15)
        // The format uses scientific notation to ensure precision
        let formatted = format!("{:.14e}", f);

        // Parse back to f64 and format normally to get the canonical representation
        // This mimics JavaScript's behavior of normalizing the result
        if let Ok(parsed) = formatted.parse::<f64>() {
            // Convert to string without exponential notation unless necessary
            if parsed.abs() >= 1e-6 && parsed.abs() < 1e21 {
                // Regular notation
                let s = format!("{}", parsed);
                // Ensure we don't have excessive precision
                if s.contains('.') {
                    let parts: Vec<&str> = s.split('.').collect();
                    if parts.len() == 2 {
                        let int_part = parts[0];
                        let frac_part = parts[1];
                        let total_digits = int_part.trim_start_matches('-').len() + frac_part.len();

                        if total_digits > 15 {
                            // Truncate to 15 significant figures
                            let sig_figs = 15 - int_part.trim_start_matches('-').len();
                            if sig_figs > 0 && sig_figs <= frac_part.len() {
                                let truncated_frac = &frac_part[..sig_figs];
                                // Remove trailing zeros
                                let trimmed = truncated_frac.trim_end_matches('0');
                                if trimmed.is_empty() {
                                    return int_part.to_string();
                                } else {
                                    return format!("{}.{}", int_part, trimmed);
                                }
                            }
                        }
                    }
                }
                s
            } else {
                // Use exponential notation for very small or large numbers
                // Format matches JavaScript: always include sign in exponent
                let exp_str = format!("{:e}", parsed);
                // Ensure exponent has + sign: "1e100" -> "1e+100"
                if exp_str.contains('e') && !exp_str.contains("e-") && !exp_str.contains("e+") {
                    exp_str.replace('e', "e+")
                } else {
                    exp_str
                }
            }
        } else {
            // Fallback
            format!("{}", f)
        }
    }

    /// Helper to stringify a value as JSON with custom replacer logic
    ///
    /// Mimics JavaScript's JSON.stringify with a replacer function that:
    /// - Converts non-integer numbers to 15 significant figures
    /// - Keeps integers without decimal point
    /// - Converts functions to empty string
    fn stringify_value_custom(value: &Value, indent: Option<usize>) -> Result<String, FunctionError> {
        // Transform the value recursively before stringifying
        let transformed = transform_for_stringify(value);

        let result = if indent.is_some() {
            serde_json::to_string_pretty(&transformed)
                .map_err(|e| FunctionError::RuntimeError(format!("JSON stringify error: {}", e)))?
        } else {
            serde_json::to_string(&transformed)
                .map_err(|e| FunctionError::RuntimeError(format!("JSON stringify error: {}", e)))?
        };
        Ok(result)
    }

    /// Transform a value for JSON.stringify, applying the replacer logic
    fn transform_for_stringify(value: &Value) -> Value {
        match value {
            Value::Number(n) => {
                // Check if it's an integer first
                if n.is_i64() || n.is_u64() {
                    // Keep as integer - serde_json will serialize without .0
                    value.clone()
                } else {
                    // Check if the f64 value is actually an integer
                    let f = n.as_f64().unwrap_or(0.0);
                    if f.fract() == 0.0 && f.is_finite() && f.abs() < (1i64 << 53) as f64 {
                        // It's a whole number that can be represented as i64
                        if let Some(i) = n.as_i64() {
                            return Value::Number(i.into());
                        }
                    }

                    // Non-integer: apply toPrecision(15) and keep as f64
                    // We don't parse back to avoid losing precision
                    let formatted = format_number_with_precision(f);
                    if let Ok(parsed) = formatted.parse::<f64>() {
                        // Return as f64 but serde_json will format it nicely
                        serde_json::json!(parsed)
                    } else {
                        value.clone()
                    }
                }
            }
            Value::Array(arr) => {
                let transformed: Vec<Value> = arr.iter().map(|v| {
                    if super::is_function_value(v) {
                        return Value::String(String::new());
                    }
                    transform_for_stringify(v)
                }).collect();
                Value::Array(transformed)
            }
            Value::Object(obj) => {
                if super::is_function_value(value) {
                    return Value::String(String::new());
                }

                let transformed: serde_json::Map<String, Value> = obj.iter().map(|(k, v)| {
                    if super::is_function_value(v) {
                        return (k.clone(), Value::String(String::new()));
                    }
                    (k.clone(), transform_for_stringify(v))
                }).collect();
                Value::Object(transformed)
            }
            _ => value.clone()
        }
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
                // Negative length returns empty string
                return Ok(Value::String(String::new()));
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
            // If separator not found, return the original string
            Ok(Value::String(s.to_string()))
        }
    }

    /// $trim(str) - Normalize and trim whitespace
    ///
    /// Normalizes whitespace by replacing runs of whitespace characters (space, tab, newline, etc.)
    /// with a single space, then strips leading and trailing spaces.
    pub fn trim(s: &str) -> Result<Value, FunctionError> {
        use regex::Regex;

        // Normalize whitespace: replace runs of [ \t\n\r]+ with single space
        let ws_regex = Regex::new(r"[ \t\n\r]+").unwrap();
        let mut result = ws_regex.replace_all(s, " ").to_string();

        // Strip leading space
        if result.starts_with(' ') {
            result = result[1..].to_string();
        }

        // Strip trailing space
        if result.ends_with(' ') {
            result = result[..result.len()-1].to_string();
        }

        Ok(Value::String(result))
    }

    /// $contains(str, pattern) - Check if string contains substring or matches regex
    pub fn contains(s: &str, pattern: &Value) -> Result<Value, FunctionError> {
        // Check if pattern is a regex
        if let Some((pat, flags)) = extract_regex(pattern) {
            let re = build_regex(&pat, &flags)?;
            return Ok(Value::Bool(re.is_match(s)));
        }

        // Handle string pattern
        let pat = match pattern {
            Value::String(s) => s.as_str(),
            _ => return Err(FunctionError::TypeError("contains() requires string arguments".to_string())),
        };

        Ok(Value::Bool(s.contains(pat)))
    }

    /// $split(str, separator, limit) - Split string into array
    /// separator can be a string or a regex object
    pub fn split(s: &str, separator: &Value, limit: Option<usize>) -> Result<Value, FunctionError> {
        // Check if separator is a regex
        if let Some((pattern, flags)) = extract_regex(separator) {
            let re = build_regex(&pattern, &flags)?;

            let parts: Vec<Value> = re.split(s)
                .map(|p| Value::String(p.to_string()))
                .collect();

            // Truncate to limit if specified (limit is max number of results)
            let result = if let Some(lim) = limit {
                parts.into_iter().take(lim).collect()
            } else {
                parts
            };

            return Ok(Value::Array(result));
        }

        // Handle string separator
        let sep = match separator {
            Value::String(s) => s.as_str(),
            _ => return Err(FunctionError::TypeError("split() requires string arguments".to_string())),
        };

        if sep.is_empty() {
            // Split into individual characters
            let chars: Vec<Value> = s.chars()
                .map(|c| Value::String(c.to_string()))
                .collect();
            // Truncate to limit if specified
            let result = if let Some(lim) = limit {
                chars.into_iter().take(lim).collect()
            } else {
                chars
            };
            return Ok(Value::Array(result));
        }

        let parts: Vec<Value> = s.split(sep)
            .map(|p| Value::String(p.to_string()))
            .collect();

        // Truncate to limit if specified (limit is max number of results)
        let result = if let Some(lim) = limit {
            parts.into_iter().take(lim).collect()
        } else {
            parts
        };

        Ok(Value::Array(result))
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

    /// Helper to perform capture group substitution in replacement string
    /// Handles $0 (full match), $1, $2, etc. (capture groups), and $$ (literal $)
    fn substitute_capture_groups(
        replacement: &str,
        full_match: &str,
        groups: &[Option<regex::Match>],
    ) -> String {
        let mut result = String::new();
        let mut position = 0;
        let chars: Vec<char> = replacement.chars().collect();

        while position < chars.len() {
            if chars[position] == '$' {
                position += 1;

                if position >= chars.len() {
                    // $ at end of string, treat as literal
                    result.push('$');
                    break;
                }

                let next_ch = chars[position];

                if next_ch == '$' {
                    // $$ → literal $
                    result.push('$');
                    position += 1;
                } else if next_ch == '0' {
                    // $0 → full match
                    result.push_str(full_match);
                    position += 1;
                } else if next_ch.is_ascii_digit() {
                    // Calculate maxDigits based on number of capture groups
                    // This matches the JavaScript implementation's logic
                    let max_digits = if groups.is_empty() {
                        1
                    } else {
                        // floor(log10(groups.len())) + 1
                        ((groups.len() as f64).log10().floor() as usize) + 1
                    };

                    // Collect up to max_digits consecutive digits
                    let mut digits_end = position;
                    let mut digit_count = 0;
                    while digits_end < chars.len()
                        && chars[digits_end].is_ascii_digit()
                        && digit_count < max_digits {
                        digits_end += 1;
                        digit_count += 1;
                    }

                    if digit_count > 0 {
                        // Try to parse as group number
                        let num_str: String = chars[position..digits_end].iter().collect();
                        let mut group_num = num_str.parse::<usize>().unwrap();

                        // If the group number is out of range and we collected more than 1 digit,
                        // try parsing with one fewer digit (fallback logic)
                        let mut used_digits = digit_count;
                        if max_digits > 1 && group_num > groups.len() && digit_count > 1 {
                            let fallback_str: String = chars[position..digits_end-1].iter().collect();
                            if let Ok(fallback_num) = fallback_str.parse::<usize>() {
                                group_num = fallback_num;
                                used_digits = digit_count - 1;
                            }
                        }

                        // Check if this is a valid group reference
                        if groups.is_empty() {
                            // No capture groups at all - $n is replaced with empty string
                            // and position advances past the digits (per JS implementation)
                            position += used_digits;
                        } else if group_num > 0 && group_num <= groups.len() {
                            // Valid group reference
                            if let Some(m) = &groups[group_num - 1] {
                                result.push_str(m.as_str());
                            }
                            // If group didn't match (None), add nothing (empty string)
                            position += used_digits;
                        } else {
                            // Group number out of range - replace with empty string
                            // and advance position (per JS implementation)
                            position += used_digits;
                        }
                    } else {
                        // No digits found (shouldn't happen since we checked next_ch.is_ascii_digit())
                        result.push('$');
                    }
                } else {
                    // $ followed by non-digit, treat as literal $
                    result.push('$');
                    // Don't consume the next character, let it be processed in next iteration
                }
            } else {
                result.push(chars[position]);
                position += 1;
            }
        }

        result
    }

    /// $replace(str, pattern, replacement, limit) - Replace substring or regex matches
    pub fn replace(
        s: &str,
        pattern: &Value,
        replacement: &str,
        limit: Option<usize>,
    ) -> Result<Value, FunctionError> {
        // Check if pattern is a regex
        if let Some((pat, flags)) = extract_regex(pattern) {
            let re = build_regex(&pat, &flags)?;

            let mut count = 0;
            let mut last_match = 0;
            let mut output = String::new();

            for cap in re.captures_iter(s) {
                if limit.is_some_and(|lim| count >= lim) {
                    break;
                }

                let m = cap.get(0).unwrap();

                // D1004: Regular expression matches zero length string
                if m.as_str().is_empty() {
                    return Err(FunctionError::RuntimeError(
                        "D1004: Regular expression matches zero length string".to_string()
                    ));
                }

                output.push_str(&s[last_match..m.start()]);

                // Collect capture groups
                let groups: Vec<Option<regex::Match>> = (1..cap.len())
                    .map(|i| cap.get(i))
                    .collect();

                // Perform capture group substitution
                let substituted = substitute_capture_groups(replacement, m.as_str(), &groups);
                output.push_str(&substituted);

                last_match = m.end();
                count += 1;
            }

            output.push_str(&s[last_match..]);
            return Ok(Value::String(output));
        }

        // Handle string pattern
        let pat = match pattern {
            Value::String(s) => s.as_str(),
            _ => return Err(FunctionError::TypeError("replace() requires string arguments".to_string())),
        };

        if pat.is_empty() {
            return Err(FunctionError::RuntimeError(
                "D3010: Pattern cannot be empty".to_string()
            ));
        }

        let result = if let Some(lim) = limit {
            let mut remaining = s;
            let mut output = String::new();
            let mut count = 0;

            while count < lim {
                if let Some(pos) = remaining.find(pat) {
                    output.push_str(&remaining[..pos]);
                    output.push_str(replacement);
                    remaining = &remaining[pos + pat.len()..];
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
            s.replace(pat, replacement)
        };

        Ok(Value::String(result))
    }
}

/// Built-in boolean functions
pub mod boolean {
    use super::*;

    /// $boolean(value) - Convert value to boolean
    ///
    /// Conversion rules:
    /// - boolean: unchanged
    /// - string: zero-length -> false; otherwise -> true
    /// - number: 0 -> false; otherwise -> true
    /// - null -> false
    /// - array: empty -> false; single element -> recursive; multi-element -> any truthy
    /// - object: empty -> false; non-empty -> true
    /// - function -> false
    pub fn boolean(value: &Value) -> Result<Value, FunctionError> {
        Ok(Value::Bool(to_boolean(value)))
    }

    /// Helper function to recursively convert values to boolean
    fn to_boolean(value: &Value) -> bool {
        match value {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Number(n) => {
                let f = n.as_f64().unwrap_or(0.0);
                f != 0.0
            }
            Value::String(s) => !s.is_empty(),
            Value::Array(arr) => {
                if arr.is_empty() {
                    false
                } else if arr.len() == 1 {
                    // Single element: recursively evaluate
                    to_boolean(&arr[0])
                } else {
                    // Multiple elements: true if any element is truthy
                    arr.iter().any(to_boolean)
                }
            }
            Value::Object(obj) => {
                // Functions are falsy in JSONata
                if super::is_function_value(value) {
                    false
                } else {
                    !obj.is_empty()
                }
            }
        }
    }
}

/// Built-in numeric functions
pub mod numeric {
    use super::*;

    /// $number(value) - Convert value to number
    /// Supports decimal, hex (0x), octal (0o), and binary (0b) formats
    pub fn number(value: &Value) -> Result<Value, FunctionError> {
        match value {
            Value::Number(n) => {
                let f = n.as_f64().unwrap_or(0.0);
                if !f.is_finite() {
                    return Err(FunctionError::RuntimeError(
                        "D3030: Cannot convert infinite number".to_string()
                    ));
                }
                Ok(Value::Number(n.clone()))
            }
            Value::String(s) => {
                let trimmed = s.trim();

                // Try hex, octal, or binary format first (0x, 0o, 0b)
                if let Some(stripped) = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")) {
                    // Hexadecimal
                    return i64::from_str_radix(stripped, 16)
                        .map(|n| serde_json::json!(n))
                        .map_err(|_| FunctionError::RuntimeError(
                            format!("D3030: Cannot convert '{}' to number", s)
                        ));
                } else if let Some(stripped) = trimmed.strip_prefix("0o").or_else(|| trimmed.strip_prefix("0O")) {
                    // Octal
                    return i64::from_str_radix(stripped, 8)
                        .map(|n| serde_json::json!(n))
                        .map_err(|_| FunctionError::RuntimeError(
                            format!("D3030: Cannot convert '{}' to number", s)
                        ));
                } else if let Some(stripped) = trimmed.strip_prefix("0b").or_else(|| trimmed.strip_prefix("0B")) {
                    // Binary
                    return i64::from_str_radix(stripped, 2)
                        .map(|n| serde_json::json!(n))
                        .map_err(|_| FunctionError::RuntimeError(
                            format!("D3030: Cannot convert '{}' to number", s)
                        ));
                }

                // Try decimal format
                match trimmed.parse::<f64>() {
                    Ok(n) => {
                        // Validate the number is finite
                        if !n.is_finite() {
                            return Err(FunctionError::RuntimeError(
                                format!("D3030: Cannot convert '{}' to number", s)
                            ));
                        }
                        Ok(serde_json::json!(n))
                    }
                    Err(_) => Err(FunctionError::RuntimeError(
                        format!("D3030: Cannot convert '{}' to number", s)
                    ))
                }
            }
            Value::Bool(true) => Ok(serde_json::json!(1)),
            Value::Bool(false) => Ok(serde_json::json!(0)),
            Value::Null => Err(FunctionError::RuntimeError(
                "D3030: Cannot convert null to number".to_string(),
            )),
            _ => Err(FunctionError::RuntimeError(
                "D3030: Cannot convert array or object to number".to_string(),
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

    /// $round(number, precision) - Round to precision using "round half to even" (banker's rounding)
    ///
    /// This implements the same rounding behavior as JSONata's JavaScript implementation,
    /// which rounds .5 values to the nearest even number.
    ///
    /// precision can be:
    /// - positive: round to that many decimal places (e.g., 2 -> 0.01)
    /// - zero or omitted: round to nearest integer
    /// - negative: round to powers of 10 (e.g., -2 -> nearest 100)
    pub fn round(n: f64, precision: Option<i32>) -> Result<Value, FunctionError> {
        let prec = precision.unwrap_or(0);

        // Shift decimal place for precision (works for both positive and negative)
        let multiplier = 10_f64.powi(prec);
        let scaled = n * multiplier;

        // Implement round-half-to-even (banker's rounding)
        let floor_val = scaled.floor();
        let frac = scaled - floor_val;

        // Use a small epsilon for floating point comparison
        let epsilon = 1e-10;
        let result = if (frac - 0.5).abs() < epsilon {
            // Exactly at .5 (within tolerance) - round to even
            let floor_int = floor_val as i64;
            if floor_int % 2 == 0 {
                floor_val  // floor is even, stay there
            } else {
                floor_val + 1.0  // floor is odd, round up to even
            }
        } else if frac > 0.5 {
            floor_val + 1.0  // round up
        } else {
            floor_val  // round down
        };

        // Shift back
        let final_result = result / multiplier;

        // Return as integer if it's a whole number
        if final_result.fract() == 0.0 && final_result.abs() < (i64::MAX as f64) {
            Ok(serde_json::json!(final_result as i64))
        } else {
            Ok(serde_json::json!(final_result))
        }
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

    /// $formatNumber(value, picture, options) - Format number with picture string
    /// Implements XPath F&O number formatting specification
    pub fn format_number(value: f64, picture: &str, options: Option<&Value>) -> Result<Value, FunctionError> {
        // Default format properties (can be overridden by options)
        let mut decimal_separator = '.';
        let mut grouping_separator = ',';
        let mut zero_digit = '0';
        let mut percent_symbol = "%".to_string();
        let mut per_mille_symbol = "‰".to_string();
        let digit_char = '#';
        let pattern_separator = ';';

        // Parse options if provided
        if let Some(Value::Object(opts)) = options {
            if let Some(Value::String(s)) = opts.get("decimal-separator") {
                decimal_separator = s.chars().next().unwrap_or('.');
            }
            if let Some(Value::String(s)) = opts.get("grouping-separator") {
                grouping_separator = s.chars().next().unwrap_or(',');
            }
            if let Some(Value::String(s)) = opts.get("zero-digit") {
                zero_digit = s.chars().next().unwrap_or('0');
            }
            if let Some(Value::String(s)) = opts.get("percent") {
                percent_symbol = s.clone();
            }
            if let Some(Value::String(s)) = opts.get("per-mille") {
                per_mille_symbol = s.clone();
            }
        }

        // Split picture into sub-pictures (positive and negative patterns)
        let sub_pictures: Vec<&str> = picture.split(pattern_separator).collect();
        if sub_pictures.len() > 2 {
            return Err(FunctionError::ArgumentError(
                "D3080: Too many pattern separators in picture string".to_string()
            ));
        }

        // Parse and analyze the picture string
        let parts = parse_picture(
            sub_pictures[0],
            decimal_separator,
            grouping_separator,
            zero_digit,
            digit_char,
            &percent_symbol,
            &per_mille_symbol,
        )?;

        // For negative numbers, use second pattern or add minus sign to first pattern
        let is_negative = value < 0.0;
        let mut abs_value = value.abs();

        // Apply percent or per-mille scaling
        if parts.has_percent {
            abs_value *= 100.0;
        } else if parts.has_per_mille {
            abs_value *= 1000.0;
        }

        // Apply the pattern
        let formatted = apply_number_picture(
            abs_value,
            &parts,
            decimal_separator,
            grouping_separator,
            zero_digit,
        )?;

        // Add prefix/suffix and handle negative
        let result = if is_negative {
            if sub_pictures.len() == 2 {
                // Use second pattern for negatives
                let neg_parts = parse_picture(
                    sub_pictures[1],
                    decimal_separator,
                    grouping_separator,
                    zero_digit,
                    digit_char,
                    &percent_symbol,
                    &per_mille_symbol,
                )?;
                let neg_formatted = apply_number_picture(
                    abs_value,
                    &neg_parts,
                    decimal_separator,
                    grouping_separator,
                    zero_digit,
                )?;
                format!("{}{}{}", neg_parts.prefix, neg_formatted, neg_parts.suffix)
            } else {
                // Add minus sign to prefix
                format!("-{}{}{}", parts.prefix, formatted, parts.suffix)
            }
        } else {
            format!("{}{}{}", parts.prefix, formatted, parts.suffix)
        };

        Ok(Value::String(result))
    }

    /// Helper to check if a character is in the digit family (0-9 or custom zero-digit family)
    fn is_digit_in_family(c: char, zero_digit: char) -> bool {
        if c.is_ascii_digit() {
            return true;
        }
        // Check if c is in custom digit family (zero_digit to zero_digit+9)
        let zero_code = zero_digit as u32;
        let c_code = c as u32;
        c_code >= zero_code && c_code < zero_code + 10
    }

    /// Parse a picture string into its components
    fn parse_picture(
        picture: &str,
        decimal_sep: char,
        grouping_sep: char,
        zero_digit: char,
        digit_char: char,
        percent_symbol: &str,
        per_mille_symbol: &str,
    ) -> Result<PictureParts, FunctionError> {
        // Work with character vectors to avoid UTF-8 byte boundary issues
        let chars: Vec<char> = picture.chars().collect();

        // Find prefix (chars before any active char)
        // Active chars for prefix/suffix: decimal sep, grouping sep, digit char, or digit family members
        // NOTE: 'e'/'E' are NOT included here to avoid treating them as exponent markers in prefix/suffix
        let prefix_end = chars.iter().position(|&c| {
            c == decimal_sep || c == grouping_sep || c == digit_char
            || is_digit_in_family(c, zero_digit)
        }).unwrap_or(chars.len());
        let prefix: String = chars[..prefix_end].iter().collect();

        // Find suffix (chars after last active char)
        let suffix_start = chars.iter().rposition(|&c| {
            c == decimal_sep || c == grouping_sep || c == digit_char
            || is_digit_in_family(c, zero_digit)
        }).map(|pos| pos + 1)
        .unwrap_or(chars.len());
        let suffix: String = chars[suffix_start..].iter().collect();

        // Active part (between prefix and suffix)
        let active: String = chars[prefix_end..suffix_start].iter().collect();

        // Check for exponential notation (e.g., "00.000e0")
        let exponent_pos = active.find('e').or_else(|| active.find('E'));
        let (mantissa_part, exponent_part): (String, String) = if let Some(pos) = exponent_pos {
            (
                active[..pos].to_string(),
                active[pos + 1..].to_string()
            )
        } else {
            (active.clone(), String::new())
        };

        // Split mantissa into integer and fractional parts using character positions
        let mantissa_chars: Vec<char> = mantissa_part.chars().collect();
        let decimal_pos = mantissa_chars.iter().position(|&c| c == decimal_sep);
        let (integer_part, fractional_part): (String, String) = if let Some(pos) = decimal_pos {
            (
                mantissa_chars[..pos].iter().collect(),
                mantissa_chars[pos + 1..].iter().collect()
            )
        } else {
            (mantissa_part.clone(), String::new())
        };

        // Validate: only one decimal separator
        if active.matches(decimal_sep).count() > 1 {
            return Err(FunctionError::ArgumentError(
                "D3081: Multiple decimal separators in picture".to_string()
            ));
        }

        // Validate: no grouping separator adjacent to decimal
        if let Some(pos) = decimal_pos {
            if pos > 0 && active.chars().nth(pos - 1) == Some(grouping_sep) {
                return Err(FunctionError::ArgumentError(
                    "D3087: Grouping separator adjacent to decimal separator".to_string()
                ));
            }
            if pos + 1 < active.len() && active.chars().nth(pos + 1) == Some(grouping_sep) {
                return Err(FunctionError::ArgumentError(
                    "D3087: Grouping separator adjacent to decimal separator".to_string()
                ));
            }
        }

        // Validate: no consecutive grouping separators
        let grouping_str = format!("{}{}", grouping_sep, grouping_sep);
        if picture.contains(&grouping_str) {
            return Err(FunctionError::ArgumentError(
                "D3089: Consecutive grouping separators in picture".to_string()
            ));
        }

        // Detect percent and per-mille symbols
        let has_percent = picture.contains(percent_symbol);
        let has_per_mille = picture.contains(per_mille_symbol);

        // Validate: multiple percent signs
        if picture.matches(percent_symbol).count() > 1 {
            return Err(FunctionError::ArgumentError(
                "D3082: Multiple percent signs in picture".to_string()
            ));
        }

        // Validate: multiple per-mille signs
        if picture.matches(per_mille_symbol).count() > 1 {
            return Err(FunctionError::ArgumentError(
                "D3083: Multiple per-mille signs in picture".to_string()
            ));
        }

        // Validate: cannot have both percent and per-mille
        if has_percent && has_per_mille {
            return Err(FunctionError::ArgumentError(
                "D3084: Cannot have both percent and per-mille in picture".to_string()
            ));
        }

        // Validate: integer part cannot end with grouping separator
        if !integer_part.is_empty() && integer_part.ends_with(grouping_sep) {
            return Err(FunctionError::ArgumentError(
                "D3088: Integer part ends with grouping separator".to_string()
            ));
        }

        // Validate: at least one digit in mantissa (integer or fractional part)
        let has_digit_in_integer = integer_part.chars().any(|c| is_digit_in_family(c, zero_digit) || c == digit_char);
        let has_digit_in_fractional = fractional_part.chars().any(|c| is_digit_in_family(c, zero_digit) || c == digit_char);
        if !has_digit_in_integer && !has_digit_in_fractional {
            return Err(FunctionError::ArgumentError(
                "D3085: Picture must contain at least one digit".to_string()
            ));
        }

        // Count minimum integer digits (mandatory digits in digit family)
        let min_integer_digits = integer_part.chars()
            .filter(|&c| is_digit_in_family(c, zero_digit))
            .count();

        // Count minimum and maximum fractional digits
        let min_fractional_digits = fractional_part.chars()
            .filter(|&c| is_digit_in_family(c, zero_digit))
            .count();
        let mut max_fractional_digits = fractional_part.chars()
            .filter(|&c| is_digit_in_family(c, zero_digit) || c == digit_char)
            .count();

        // If there's a decimal point but no fractional digits specified, default to 1
        // This handles cases like "#.e0" where some fractional precision is expected
        if decimal_pos.is_some() && max_fractional_digits == 0 {
            max_fractional_digits = 1;
        }

        // Find grouping positions in integer part
        let mut grouping_positions = Vec::new();
        let int_chars: Vec<char> = integer_part.chars().collect();
        for (i, &c) in int_chars.iter().enumerate() {
            if c == grouping_sep {
                // Count digits to the right of this separator
                let digits_to_right = int_chars[i + 1..]
                    .iter()
                    .filter(|&&ch| is_digit_in_family(ch, zero_digit) || ch == digit_char)
                    .count();
                grouping_positions.push(digits_to_right);
            }
        }

        // Check if grouping is regular (same interval)
        let regular_grouping = if grouping_positions.is_empty() {
            0
        } else if grouping_positions.len() == 1 {
            grouping_positions[0]
        } else {
            // Check if all intervals are the same
            let first_interval = grouping_positions[0];
            if grouping_positions.iter().all(|&p| {
                grouping_positions.iter().filter(|&&x| x == p).count() == grouping_positions.len() / first_interval
                || (p % first_interval == 0 && grouping_positions.contains(&first_interval))
            }) {
                first_interval
            } else {
                0 // Irregular grouping
            }
        };

        // Find grouping positions in fractional part
        let mut fractional_grouping_positions = Vec::new();
        let frac_chars: Vec<char> = fractional_part.chars().collect();
        for (i, &c) in frac_chars.iter().enumerate() {
            if c == grouping_sep {
                // For fractional part, count digits to the left of this separator
                let digits_to_left = frac_chars[..i]
                    .iter()
                    .filter(|&&ch| is_digit_in_family(ch, zero_digit) || ch == digit_char)
                    .count();
                fractional_grouping_positions.push(digits_to_left);
            }
        }

        // Process exponent part if present (recognize both ASCII and custom digit families)
        let min_exponent_digits = if !exponent_part.is_empty() {
            exponent_part.chars()
                .filter(|&c| is_digit_in_family(c, zero_digit))
                .count()
        } else {
            0
        };

        // Validate: exponent part must contain only digit characters (ASCII or custom digit family)
        if !exponent_part.is_empty() && exponent_part.chars().any(|c| !is_digit_in_family(c, zero_digit)) {
            return Err(FunctionError::ArgumentError(
                "D3093: Exponent must contain only digit characters".to_string()
            ));
        }

        // Validate: exponent cannot be empty if 'e' is present
        if exponent_pos.is_some() && min_exponent_digits == 0 {
            return Err(FunctionError::ArgumentError(
                "D3093: Exponent cannot be empty".to_string()
            ));
        }

        // Validate: percent/per-mille not allowed with exponential notation
        if min_exponent_digits > 0 && (has_percent || has_per_mille) {
            return Err(FunctionError::ArgumentError(
                "D3092: Percent/per-mille not allowed with exponential notation".to_string()
            ));
        }

        // Validate: # cannot appear after 0 in integer part
        // In integer part, # must come before 0 (e.g., "##00" valid, "00##" invalid)
        let mut seen_zero_in_integer = false;
        for c in integer_part.chars() {
            if is_digit_in_family(c, zero_digit) {
                seen_zero_in_integer = true;
            } else if c == digit_char && seen_zero_in_integer {
                return Err(FunctionError::ArgumentError(
                    "D3090: Optional digit (#) cannot appear after mandatory digit (0) in integer part".to_string()
                ));
            }
        }

        // Validate: # cannot appear before 0 in fractional part
        // In fractional part, 0 must come before # (e.g., "00##" valid, "##00" invalid)
        let mut seen_hash_in_fractional = false;
        for c in fractional_part.chars() {
            if c == digit_char {
                seen_hash_in_fractional = true;
            } else if is_digit_in_family(c, zero_digit) && seen_hash_in_fractional {
                return Err(FunctionError::ArgumentError(
                    "D3091: Mandatory digit (0) cannot appear after optional digit (#) in fractional part".to_string()
                ));
            }
        }

        // Validate: invalid characters in picture
        // All characters in the active part must be valid (digits, decimal, grouping, or 'e'/'E')
        let valid_chars: Vec<char> = vec![decimal_sep, grouping_sep, zero_digit, digit_char, 'e', 'E'];
        for c in mantissa_part.chars() {
            if !is_digit_in_family(c, zero_digit) && !valid_chars.contains(&c) {
                return Err(FunctionError::ArgumentError(
                    format!("D3086: Invalid character in picture: '{}'", c)
                ));
            }
        }

        // Scaling factor = minimum integer digits in mantissa
        let scaling_factor = min_integer_digits;

        Ok(PictureParts {
            prefix,
            suffix,
            min_integer_digits,
            min_fractional_digits,
            max_fractional_digits,
            grouping_positions,
            fractional_grouping_positions,
            regular_grouping,
            has_decimal: decimal_pos.is_some(),
            has_integer_part: !integer_part.is_empty(),
            has_percent,
            has_per_mille,
            min_exponent_digits,
            scaling_factor,
        })
    }

    /// Apply the picture pattern to format a number
    fn apply_number_picture(
        value: f64,
        parts: &PictureParts,
        decimal_sep: char,
        grouping_sep: char,
        zero_digit: char,
    ) -> Result<String, FunctionError> {
        // Handle exponential notation
        let (mantissa, exponent) = if parts.min_exponent_digits > 0 {
            // Calculate mantissa and exponent: mantissa * 10^exponent = value
            let max_mantissa = 10_f64.powi(parts.scaling_factor as i32);
            let min_mantissa = 10_f64.powi(parts.scaling_factor as i32 - 1);

            let mut m = value;
            let mut e = 0_i32;

            // Scale mantissa to be within [min_mantissa, max_mantissa)
            while m < min_mantissa && m != 0.0 {
                m *= 10.0;
                e -= 1;
            }
            while m >= max_mantissa {
                m /= 10.0;
                e += 1;
            }

            (m, Some(e))
        } else {
            (value, None)
        };

        // Round mantissa to max fractional digits
        let factor = 10_f64.powi(parts.max_fractional_digits as i32);
        let rounded = (mantissa * factor).round() / factor;

        // Convert to string with fixed decimal places
        let mut num_str = format!("{:.prec$}", rounded, prec = parts.max_fractional_digits);

        // Replace '.' with decimal separator
        if decimal_sep != '.' {
            num_str = num_str.replace('.', &decimal_sep.to_string());
        }

        // Split into integer and fractional parts
        let decimal_pos = num_str.find(decimal_sep).unwrap_or(num_str.len());
        let mut integer_str = num_str[..decimal_pos].to_string();
        let mut fractional_str = if decimal_pos < num_str.len() {
            num_str[decimal_pos + 1..].to_string()
        } else {
            String::new()
        };

        // Strip leading zeros from integer part
        while integer_str.len() > 1 && integer_str.starts_with(zero_digit) {
            integer_str.remove(0);
        }
        // If we stripped down to a single zero and picture has no integer part, remove it
        if integer_str == zero_digit.to_string() && !parts.has_integer_part {
            integer_str.clear();
        }
        // If integer part is empty and picture had integer part, add one zero
        if integer_str.is_empty() && parts.has_integer_part {
            integer_str.push(zero_digit);
        }

        // Strip trailing zeros from fractional part
        while !fractional_str.is_empty() && fractional_str.ends_with(zero_digit) {
            fractional_str.pop();
        }

        // Pad integer part to minimum size
        while integer_str.len() < parts.min_integer_digits {
            integer_str.insert(0, zero_digit);
        }

        // Pad fractional part to minimum size
        while fractional_str.len() < parts.min_fractional_digits {
            fractional_str.push(zero_digit);
        }

        // Trim trailing zeros beyond minimum (for optional # digits)
        while fractional_str.len() > parts.min_fractional_digits {
            if fractional_str.ends_with(zero_digit) {
                fractional_str.pop();
            } else {
                break;
            }
        }

        // Add grouping separators to integer part
        if parts.regular_grouping > 0 {
            // Regular grouping (e.g., every 3 digits for "#,###")
            let mut grouped = String::new();
            let chars: Vec<char> = integer_str.chars().collect();
            for (i, &c) in chars.iter().enumerate() {
                grouped.push(c);
                let pos_from_right = chars.len() - i - 1;
                if pos_from_right > 0 && pos_from_right % parts.regular_grouping == 0 {
                    grouped.push(grouping_sep);
                }
            }
            integer_str = grouped;
        } else if !parts.grouping_positions.is_empty() {
            // Irregular grouping (e.g., "9,99,999")
            let mut grouped = String::new();
            let chars: Vec<char> = integer_str.chars().collect();
            for (i, &c) in chars.iter().enumerate() {
                grouped.push(c);
                let pos_from_right = chars.len() - i - 1;
                if parts.grouping_positions.contains(&pos_from_right) {
                    grouped.push(grouping_sep);
                }
            }
            integer_str = grouped;
        }

        // Add grouping separators to fractional part
        if !parts.fractional_grouping_positions.is_empty() {
            let mut grouped = String::new();
            let chars: Vec<char> = fractional_str.chars().collect();
            for (i, &c) in chars.iter().enumerate() {
                grouped.push(c);
                // For fractional grouping, positions are counted from the left
                let pos_from_left = i + 1;
                if parts.fractional_grouping_positions.contains(&pos_from_left) {
                    grouped.push(grouping_sep);
                }
            }
            fractional_str = grouped;
        }

        // Combine integer and fractional parts
        let mut result = if parts.has_decimal || !fractional_str.is_empty() {
            format!("{}{}{}", integer_str, decimal_sep, fractional_str)
        } else {
            integer_str
        };

        // Convert digits to custom zero-digit base if needed (mantissa part)
        if zero_digit != '0' {
            let zero_code = zero_digit as u32;
            result = result.chars().map(|c| {
                if c.is_ascii_digit() {
                    let digit_value = c as u32 - '0' as u32;
                    char::from_u32(zero_code + digit_value).unwrap_or(c)
                } else {
                    c
                }
            }).collect();
        }

        // Append exponent if present
        if let Some(exp) = exponent {
            // Format exponent with minimum digits
            let exp_str = format!("{:0width$}", exp.abs(), width = parts.min_exponent_digits);

            // Convert exponent digits to custom zero-digit base if needed
            let exp_formatted = if zero_digit != '0' {
                let zero_code = zero_digit as u32;
                exp_str.chars().map(|c| {
                    if c.is_ascii_digit() {
                        let digit_value = c as u32 - '0' as u32;
                        char::from_u32(zero_code + digit_value).unwrap_or(c)
                    } else {
                        c
                    }
                }).collect()
            } else {
                exp_str
            };

            // Append 'e' and exponent (with sign if negative)
            result.push('e');
            if exp < 0 {
                result.push('-');
            }
            result.push_str(&exp_formatted);
        }

        Ok(result)
    }

    /// Holds parsed picture pattern components
    #[derive(Debug)]
    struct PictureParts {
        prefix: String,
        suffix: String,
        min_integer_digits: usize,
        min_fractional_digits: usize,
        max_fractional_digits: usize,
        grouping_positions: Vec<usize>,
        fractional_grouping_positions: Vec<usize>,
        regular_grouping: usize,
        has_decimal: bool,
        has_integer_part: bool,
        has_percent: bool,
        has_per_mille: bool,
        min_exponent_digits: usize,
        scaling_factor: usize,
    }

    /// $formatBase(value, radix) - Convert number to string in specified base
    /// radix defaults to 10, must be between 2 and 36
    pub fn format_base(value: f64, radix: Option<i64>) -> Result<Value, FunctionError> {
        // Round to integer
        let int_value = value.round() as i64;

        // Default radix is 10
        let radix = radix.unwrap_or(10);

        // Validate radix is between 2 and 36
        if radix < 2 || radix > 36 {
            return Err(FunctionError::ArgumentError(
                format!("D3100: Radix must be between 2 and 36, got {}", radix)
            ));
        }

        // Handle negative numbers
        let is_negative = int_value < 0;
        let abs_value = int_value.abs() as u64;

        // Convert to string in specified base
        let digits = "0123456789abcdefghijklmnopqrstuvwxyz";
        let mut result = String::new();
        let mut val = abs_value;

        if val == 0 {
            result.push('0');
        } else {
            while val > 0 {
                let digit = (val % radix as u64) as usize;
                result.insert(0, digits.chars().nth(digit).unwrap());
                val /= radix as u64;
            }
        }

        // Add negative sign if needed
        if is_negative {
            result.insert(0, '-');
        }

        Ok(Value::String(result))
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
        let is_missing = matches!(value, Value::Null)
            || matches!(value, Value::Object(map) if map.contains_key("__undefined__"));
        Ok(Value::Bool(!is_missing))
    }

    /// Compare two JSON values for deep equality (JSONata semantics)
    pub fn values_equal(a: &Value, b: &Value) -> bool {
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

    /// $shuffle(array) - Randomly shuffle array elements
    /// Uses Fisher-Yates (inside-out variant) algorithm
    pub fn shuffle(arr: &[Value]) -> Result<Value, FunctionError> {
        if arr.len() <= 1 {
            return Ok(Value::Array(arr.to_vec()));
        }

        use rand::seq::SliceRandom;
        use rand::thread_rng;

        let mut result = arr.to_vec();
        let mut rng = thread_rng();
        result.shuffle(&mut rng);

        Ok(Value::Array(result))
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
        // Each key-value pair becomes a single-key object: {"key": value}
        let pairs: Vec<Value> = obj
            .iter()
            .map(|(k, v)| {
                let mut pair = serde_json::Map::new();
                pair.insert(k.clone(), v.clone());
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

/// Encoding/decoding functions
pub mod encoding {
    use super::*;
    use base64::{Engine as _, engine::general_purpose};

    /// $base64encode(string) - Encode string to base64
    pub fn base64encode(s: &str) -> Result<Value, FunctionError> {
        let encoded = general_purpose::STANDARD.encode(s.as_bytes());
        Ok(Value::String(encoded))
    }

    /// $base64decode(string) - Decode base64 string
    pub fn base64decode(s: &str) -> Result<Value, FunctionError> {
        match general_purpose::STANDARD.decode(s.as_bytes()) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(decoded) => Ok(Value::String(decoded)),
                Err(_) => Err(FunctionError::RuntimeError(
                    "Invalid UTF-8 in decoded base64".to_string()
                )),
            },
            Err(_) => Err(FunctionError::RuntimeError(
                "Invalid base64 string".to_string()
            )),
        }
    }

    /// $encodeUrlComponent(string) - Encode URL component
    pub fn encode_url_component(s: &str) -> Result<Value, FunctionError> {
        let encoded = percent_encoding::utf8_percent_encode(
            s,
            percent_encoding::NON_ALPHANUMERIC
        ).to_string();
        Ok(Value::String(encoded))
    }

    /// $decodeUrlComponent(string) - Decode URL component
    pub fn decode_url_component(s: &str) -> Result<Value, FunctionError> {
        match percent_encoding::percent_decode_str(s).decode_utf8() {
            Ok(decoded) => Ok(Value::String(decoded.to_string())),
            Err(_) => Err(FunctionError::RuntimeError(
                "Invalid percent-encoded string".to_string()
            )),
        }
    }

    /// $encodeUrl(string) - Encode full URL
    /// More permissive than encodeUrlComponent - allows URL structure characters
    pub fn encode_url(s: &str) -> Result<Value, FunctionError> {
        // Use CONTROLS to preserve URL structure (://?#[]@!$&'()*+,;=)
        let encoded = percent_encoding::utf8_percent_encode(
            s,
            percent_encoding::CONTROLS
        ).to_string();
        Ok(Value::String(encoded))
    }

    /// $decodeUrl(string) - Decode full URL
    pub fn decode_url(s: &str) -> Result<Value, FunctionError> {
        match percent_encoding::percent_decode_str(s).decode_utf8() {
            Ok(decoded) => Ok(Value::String(decoded.to_string())),
            Err(_) => Err(FunctionError::RuntimeError(
                "Invalid percent-encoded URL".to_string()
            )),
        }
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
            string::string(&Value::String("hello".to_string()), None).unwrap(),
            Value::String("hello".to_string())
        );

        // Number to string
        assert_eq!(
            string::string(&serde_json::json!(42), None).unwrap(),
            Value::String("42".to_string())
        );

        // Float to string
        assert_eq!(
            string::string(&serde_json::json!(3.14), None).unwrap(),
            Value::String("3.14".to_string())
        );

        // Boolean to string
        assert_eq!(
            string::string(&Value::Bool(true), None).unwrap(),
            Value::String("true".to_string())
        );

        // Null to empty string
        assert_eq!(
            string::string(&Value::Null, None).unwrap(),
            Value::String(String::new())
        );

        // Array should error
        assert!(string::string(&serde_json::json!([1, 2, 3]), None).is_err());
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
        assert_eq!(string::contains("hello world", &Value::String("world".to_string())).unwrap(), Value::Bool(true));
        assert_eq!(string::contains("hello world", &Value::String("xyz".to_string())).unwrap(), Value::Bool(false));
        assert_eq!(string::contains("hello world", &Value::String("".to_string())).unwrap(), Value::Bool(true));
    }

    #[test]
    fn test_split() {
        // Split with separator
        assert_eq!(
            string::split("a,b,c", &Value::String(",".to_string()), None).unwrap(),
            serde_json::json!(["a", "b", "c"])
        );

        // Split with limit
        assert_eq!(
            string::split("a,b,c,d", &Value::String(",".to_string()), Some(2)).unwrap(),
            serde_json::json!(["a", "b,c,d"])
        );

        // Split with empty separator (split into chars)
        assert_eq!(
            string::split("abc", &Value::String("".to_string()), None).unwrap(),
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
            string::replace("hello hello", &Value::String("hello".to_string()), "hi", None).unwrap(),
            Value::String("hi hi".to_string())
        );

        // Replace with limit
        assert_eq!(
            string::replace("hello hello hello", &Value::String("hello".to_string()), "hi", Some(2)).unwrap(),
            Value::String("hi hi hello".to_string())
        );

        // Replace empty pattern (no change)
        assert_eq!(
            string::replace("hello", &Value::String("".to_string()), "x", None).unwrap(),
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

// Built-in function implementations
// Mirrors functions.js from the reference implementation

use crate::value::JValue;
use indexmap::IndexMap;
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
    use regex::Regex;

    /// Helper to detect and extract regex from a JValue
    pub fn extract_regex(value: &JValue) -> Option<(String, String)> {
        if let JValue::Regex { pattern, flags } = value {
            return Some((pattern.to_string(), flags.to_string()));
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
                regex_pattern.push('i'); // case-insensitive
            }
            if flags.contains('m') {
                regex_pattern.push('m'); // multi-line
            }
            if flags.contains('s') {
                regex_pattern.push('s'); // dot matches newline
            }
            regex_pattern.push(')');
        }

        regex_pattern.push_str(pattern);

        Regex::new(&regex_pattern)
            .map_err(|e| FunctionError::ArgumentError(format!("Invalid regex: {}", e)))
    }

    /// $string(value, prettify) - Convert value to string
    ///
    /// - undefined inputs return undefined (but this is handled at call site)
    /// - strings returned unchanged
    /// - functions/lambdas return empty string
    /// - non-finite numbers (Infinity, NaN) throw error D3001
    /// - other values use JSON.stringify with number precision
    /// - prettify=true uses 2-space indentation
    pub fn string(value: &JValue, prettify: Option<bool>) -> Result<JValue, FunctionError> {
        // Check if this is undefined or a function first
        if value.is_undefined() {
            return Ok(JValue::string(""));
        }
        if value.is_function() {
            return Ok(JValue::string(""));
        }

        let result = match value {
            JValue::String(s) => s.to_string(),
            JValue::Number(n) => {
                let f = *n;
                // Check for non-finite numbers (Infinity, NaN)
                if !f.is_finite() {
                    return Err(FunctionError::RuntimeError(format!(
                        "D3001: Attempting to invoke string function with non-finite number: {}",
                        f
                    )));
                }

                // Format numbers like JavaScript does
                if f.fract() == 0.0 && f.abs() < (i64::MAX as f64) {
                    (f as i64).to_string()
                } else {
                    // Non-integer - use precision formatting
                    // JavaScript uses toPrecision(15) for non-integers in JSON.stringify
                    format_number_with_precision(f)
                }
            }
            JValue::Bool(b) => b.to_string(),
            JValue::Null => {
                // Explicit null goes through JSON.stringify to become "null"
                // Undefined variables are handled at the evaluator level
                "null".to_string()
            }
            JValue::Array(_) | JValue::Object(_) => {
                // JSON.stringify with optional prettification
                // Uses custom serialization to handle numbers and functions correctly
                let indent = if prettify.unwrap_or(false) {
                    Some(2)
                } else {
                    None
                };
                stringify_value_custom(value, indent)?
            }
            _ => String::new(),
        };
        Ok(JValue::string(result))
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
    fn stringify_value_custom(
        value: &JValue,
        indent: Option<usize>,
    ) -> Result<String, FunctionError> {
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
    fn transform_for_stringify(value: &JValue) -> JValue {
        match value {
            JValue::Number(n) => {
                let f = *n;
                // Check if it's an integer first
                if f.fract() == 0.0 && f.is_finite() && f.abs() < (1i64 << 53) as f64 {
                    // Keep as integer
                    value.clone()
                } else {
                    // Non-integer: apply toPrecision(15) and keep as f64
                    let formatted = format_number_with_precision(f);
                    if let Ok(parsed) = formatted.parse::<f64>() {
                        JValue::Number(parsed)
                    } else {
                        value.clone()
                    }
                }
            }
            JValue::Array(arr) => {
                let transformed: Vec<JValue> = arr
                    .iter()
                    .map(|v| {
                        if v.is_function() {
                            return JValue::string("");
                        }
                        transform_for_stringify(v)
                    })
                    .collect();
                JValue::array(transformed)
            }
            JValue::Object(obj) => {
                if value.is_function() {
                    return JValue::string("");
                }

                let transformed: IndexMap<String, JValue> = obj
                    .iter()
                    .map(|(k, v)| {
                        if v.is_function() {
                            return (k.clone(), JValue::string(""));
                        }
                        (k.clone(), transform_for_stringify(v))
                    })
                    .collect();
                JValue::object(transformed)
            }
            _ => value.clone(),
        }
    }

    /// $length() - Get string length with proper Unicode support
    /// Returns the number of Unicode characters (not bytes)
    pub fn length(s: &str) -> Result<JValue, FunctionError> {
        Ok(JValue::Number(s.chars().count() as f64))
    }

    /// $uppercase() - Convert to uppercase
    pub fn uppercase(s: &str) -> Result<JValue, FunctionError> {
        Ok(JValue::string(s.to_uppercase()))
    }

    /// $lowercase() - Convert to lowercase
    pub fn lowercase(s: &str) -> Result<JValue, FunctionError> {
        Ok(JValue::string(s.to_lowercase()))
    }

    /// $substring(str, start, length) - Extract substring
    /// Extracts a substring from a string using Unicode character positions
    pub fn substring(s: &str, start: i64, length: Option<i64>) -> Result<JValue, FunctionError> {
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
                return Ok(JValue::string(""));
            }
            (start_pos + len as usize).min(chars.len())
        } else {
            chars.len()
        };

        let result: String = chars[start_pos..end_pos].iter().collect();
        Ok(JValue::string(result))
    }

    /// $substringBefore(str, separator) - Get substring before separator
    pub fn substring_before(s: &str, separator: &str) -> Result<JValue, FunctionError> {
        if separator.is_empty() {
            return Ok(JValue::string(""));
        }

        let result = s.split(separator).next().unwrap_or(s).to_string();
        Ok(JValue::string(result))
    }

    /// $substringAfter(str, separator) - Get substring after separator
    pub fn substring_after(s: &str, separator: &str) -> Result<JValue, FunctionError> {
        if separator.is_empty() {
            return Ok(JValue::string(s));
        }

        if let Some(pos) = s.find(separator) {
            let result = s[pos + separator.len()..].to_string();
            Ok(JValue::string(result))
        } else {
            // If separator not found, return the original string
            Ok(JValue::string(s))
        }
    }

    /// $trim(str) - Normalize and trim whitespace
    ///
    /// Normalizes whitespace by replacing runs of whitespace characters (space, tab, newline, etc.)
    /// with a single space, then strips leading and trailing spaces.
    pub fn trim(s: &str) -> Result<JValue, FunctionError> {
        use regex::Regex;
        use std::sync::OnceLock;

        static WS_REGEX: OnceLock<Regex> = OnceLock::new();
        let ws_regex = WS_REGEX.get_or_init(|| Regex::new(r"[ \t\n\r]+").unwrap());

        let normalized = ws_regex.replace_all(s, " ");
        Ok(JValue::string(normalized.trim()))
    }

    /// $contains(str, pattern) - Check if string contains substring or matches regex
    pub fn contains(s: &str, pattern: &JValue) -> Result<JValue, FunctionError> {
        // Check if pattern is a regex
        if let Some((pat, flags)) = extract_regex(pattern) {
            let re = build_regex(&pat, &flags)?;
            return Ok(JValue::Bool(re.is_match(s)));
        }

        // Handle string pattern
        let pat = match pattern {
            JValue::String(s) => &**s,
            _ => {
                return Err(FunctionError::TypeError(
                    "contains() requires string arguments".to_string(),
                ))
            }
        };

        Ok(JValue::Bool(s.contains(pat)))
    }

    /// $split(str, separator, limit) - Split string into array
    /// separator can be a string or a regex object
    pub fn split(
        s: &str,
        separator: &JValue,
        limit: Option<usize>,
    ) -> Result<JValue, FunctionError> {
        // Check if separator is a regex
        if let Some((pattern, flags)) = extract_regex(separator) {
            let re = build_regex(&pattern, &flags)?;

            let parts: Vec<JValue> = re.split(s).map(JValue::string).collect();

            // Truncate to limit if specified (limit is max number of results)
            let result = if let Some(lim) = limit {
                parts.into_iter().take(lim).collect()
            } else {
                parts
            };

            return Ok(JValue::array(result));
        }

        // Handle string separator
        let sep = match separator {
            JValue::String(s) => &**s,
            _ => {
                return Err(FunctionError::TypeError(
                    "split() requires string arguments".to_string(),
                ))
            }
        };

        if sep.is_empty() {
            // Split into individual characters
            let chars: Vec<JValue> = s.chars().map(|c| JValue::string(c.to_string())).collect();
            // Truncate to limit if specified
            let result = if let Some(lim) = limit {
                chars.into_iter().take(lim).collect()
            } else {
                chars
            };
            return Ok(JValue::array(result));
        }

        let parts: Vec<JValue> = s.split(sep).map(JValue::string).collect();

        // Truncate to limit if specified (limit is max number of results)
        let result = if let Some(lim) = limit {
            parts.into_iter().take(lim).collect()
        } else {
            parts
        };

        Ok(JValue::array(result))
    }

    /// $join(array, separator) - Join array into string
    pub fn join(arr: &[JValue], separator: Option<&str>) -> Result<JValue, FunctionError> {
        let sep = separator.unwrap_or("");
        let parts: Result<Vec<String>, FunctionError> = arr
            .iter()
            .map(|v| match v {
                JValue::String(s) => Ok(s.to_string()),
                JValue::Number(n) => Ok(format_join_number(*n)),
                JValue::Bool(b) => Ok(b.to_string()),
                JValue::Null => Ok(String::new()),
                _ => Err(FunctionError::TypeError(
                    "Cannot join array containing objects or nested arrays".to_string(),
                )),
            })
            .collect();

        let parts = parts?;
        Ok(JValue::string(parts.join(sep)))
    }

    /// Helper to format a number for $join (matching serde_json Number's Display)
    fn format_join_number(n: f64) -> String {
        if n.fract() == 0.0 && n.abs() < (i64::MAX as f64) {
            (n as i64).to_string()
        } else {
            n.to_string()
        }
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
                        && digit_count < max_digits
                    {
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
                            let fallback_str: String =
                                chars[position..digits_end - 1].iter().collect();
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
        pattern: &JValue,
        replacement: &str,
        limit: Option<usize>,
    ) -> Result<JValue, FunctionError> {
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
                        "D1004: Regular expression matches zero length string".to_string(),
                    ));
                }

                output.push_str(&s[last_match..m.start()]);

                // Collect capture groups
                let groups: Vec<Option<regex::Match>> =
                    (1..cap.len()).map(|i| cap.get(i)).collect();

                // Perform capture group substitution
                let substituted = substitute_capture_groups(replacement, m.as_str(), &groups);
                output.push_str(&substituted);

                last_match = m.end();
                count += 1;
            }

            output.push_str(&s[last_match..]);
            return Ok(JValue::string(output));
        }

        // Handle string pattern
        let pat = match pattern {
            JValue::String(s) => &**s,
            _ => {
                return Err(FunctionError::TypeError(
                    "replace() requires string arguments".to_string(),
                ))
            }
        };

        if pat.is_empty() {
            return Err(FunctionError::RuntimeError(
                "D3010: Pattern cannot be empty".to_string(),
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

        Ok(JValue::string(result))
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
    pub fn boolean(value: &JValue) -> Result<JValue, FunctionError> {
        Ok(JValue::Bool(to_boolean(value)))
    }

    /// Helper function to recursively convert values to boolean.
    fn to_boolean(value: &JValue) -> bool {
        match value {
            JValue::Null | JValue::Undefined => false,
            JValue::Bool(b) => *b,
            JValue::Number(n) => *n != 0.0,
            JValue::String(s) => !s.is_empty(),
            JValue::Array(arr) => {
                if arr.len() == 1 {
                    to_boolean(&arr[0])
                } else {
                    // Empty arrays are falsy; multi-element: true if any element is truthy
                    arr.iter().any(to_boolean)
                }
            }
            JValue::Object(obj) => !obj.is_empty(),
            JValue::Lambda { .. } | JValue::Builtin { .. } => false,
            JValue::Regex { .. } => true,
        }
    }
}

/// Built-in numeric functions
pub mod numeric {
    use super::*;

    /// $number(value) - Convert value to number
    /// Supports decimal, hex (0x), octal (0o), and binary (0b) formats
    pub fn number(value: &JValue) -> Result<JValue, FunctionError> {
        match value {
            JValue::Number(n) => {
                let f = *n;
                if !f.is_finite() {
                    return Err(FunctionError::RuntimeError(
                        "D3030: Cannot convert infinite number".to_string(),
                    ));
                }
                Ok(JValue::Number(f))
            }
            JValue::String(s) => {
                let trimmed = s.trim();

                // Try hex, octal, or binary format first (0x, 0o, 0b)
                if let Some(stripped) = trimmed
                    .strip_prefix("0x")
                    .or_else(|| trimmed.strip_prefix("0X"))
                {
                    // Hexadecimal
                    return i64::from_str_radix(stripped, 16)
                        .map(|n| JValue::Number(n as f64))
                        .map_err(|_| {
                            FunctionError::RuntimeError(format!(
                                "D3030: Cannot convert '{}' to number",
                                s
                            ))
                        });
                } else if let Some(stripped) = trimmed
                    .strip_prefix("0o")
                    .or_else(|| trimmed.strip_prefix("0O"))
                {
                    // Octal
                    return i64::from_str_radix(stripped, 8)
                        .map(|n| JValue::Number(n as f64))
                        .map_err(|_| {
                            FunctionError::RuntimeError(format!(
                                "D3030: Cannot convert '{}' to number",
                                s
                            ))
                        });
                } else if let Some(stripped) = trimmed
                    .strip_prefix("0b")
                    .or_else(|| trimmed.strip_prefix("0B"))
                {
                    // Binary
                    return i64::from_str_radix(stripped, 2)
                        .map(|n| JValue::Number(n as f64))
                        .map_err(|_| {
                            FunctionError::RuntimeError(format!(
                                "D3030: Cannot convert '{}' to number",
                                s
                            ))
                        });
                }

                // Try decimal format
                match trimmed.parse::<f64>() {
                    Ok(n) => {
                        // Validate the number is finite
                        if !n.is_finite() {
                            return Err(FunctionError::RuntimeError(format!(
                                "D3030: Cannot convert '{}' to number",
                                s
                            )));
                        }
                        Ok(JValue::Number(n))
                    }
                    Err(_) => Err(FunctionError::RuntimeError(format!(
                        "D3030: Cannot convert '{}' to number",
                        s
                    ))),
                }
            }
            JValue::Bool(true) => Ok(JValue::Number(1.0)),
            JValue::Bool(false) => Ok(JValue::Number(0.0)),
            JValue::Null => Err(FunctionError::RuntimeError(
                "D3030: Cannot convert null to number".to_string(),
            )),
            _ => Err(FunctionError::RuntimeError(
                "D3030: Cannot convert array or object to number".to_string(),
            )),
        }
    }

    /// $sum(array) - Sum array of numbers
    pub fn sum(arr: &[JValue]) -> Result<JValue, FunctionError> {
        if arr.is_empty() {
            return Ok(JValue::Number(0.0));
        }

        let mut total = 0.0;
        for value in arr {
            match value {
                JValue::Number(n) => {
                    total += n;
                }
                _ => {
                    return Err(FunctionError::TypeError(format!(
                        "sum() requires all array elements to be numbers, got: {:?}",
                        value
                    )))
                }
            }
        }
        Ok(JValue::Number(total))
    }

    /// $max(array) - Maximum value
    pub fn max(arr: &[JValue]) -> Result<JValue, FunctionError> {
        if arr.is_empty() {
            return Ok(JValue::Null);
        }

        let mut max_val = f64::NEG_INFINITY;
        for value in arr {
            match value {
                JValue::Number(n) => {
                    if *n > max_val {
                        max_val = *n;
                    }
                }
                _ => {
                    return Err(FunctionError::TypeError(
                        "max() requires all array elements to be numbers".to_string(),
                    ))
                }
            }
        }
        Ok(JValue::Number(max_val))
    }

    /// $min(array) - Minimum value
    pub fn min(arr: &[JValue]) -> Result<JValue, FunctionError> {
        if arr.is_empty() {
            return Ok(JValue::Null);
        }

        let mut min_val = f64::INFINITY;
        for value in arr {
            match value {
                JValue::Number(n) => {
                    if *n < min_val {
                        min_val = *n;
                    }
                }
                _ => {
                    return Err(FunctionError::TypeError(
                        "min() requires all array elements to be numbers".to_string(),
                    ))
                }
            }
        }
        Ok(JValue::Number(min_val))
    }

    /// $average(array) - Average value
    pub fn average(arr: &[JValue]) -> Result<JValue, FunctionError> {
        if arr.is_empty() {
            return Ok(JValue::Null);
        }

        let sum_result = sum(arr)?;
        if let JValue::Number(n) = sum_result {
            let avg = n / arr.len() as f64;
            Ok(JValue::Number(avg))
        } else {
            Err(FunctionError::RuntimeError("Sum failed".to_string()))
        }
    }

    /// $abs(number) - Absolute value
    pub fn abs(n: f64) -> Result<JValue, FunctionError> {
        Ok(JValue::Number(n.abs()))
    }

    /// $floor(number) - Floor
    pub fn floor(n: f64) -> Result<JValue, FunctionError> {
        Ok(JValue::Number(n.floor()))
    }

    /// $ceil(number) - Ceiling
    pub fn ceil(n: f64) -> Result<JValue, FunctionError> {
        Ok(JValue::Number(n.ceil()))
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
    pub fn round(n: f64, precision: Option<i32>) -> Result<JValue, FunctionError> {
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
                floor_val // floor is even, stay there
            } else {
                floor_val + 1.0 // floor is odd, round up to even
            }
        } else if frac > 0.5 {
            floor_val + 1.0 // round up
        } else {
            floor_val // round down
        };

        // Shift back
        let final_result = result / multiplier;

        Ok(JValue::Number(final_result))
    }

    /// $sqrt(number) - Square root
    pub fn sqrt(n: f64) -> Result<JValue, FunctionError> {
        if n < 0.0 {
            return Err(FunctionError::ArgumentError(
                "Cannot take square root of negative number".to_string(),
            ));
        }
        Ok(JValue::Number(n.sqrt()))
    }

    /// $power(base, exponent) - Power
    pub fn power(base: f64, exponent: f64) -> Result<JValue, FunctionError> {
        let result = base.powf(exponent);
        if result.is_nan() || result.is_infinite() {
            return Err(FunctionError::RuntimeError(
                "Power operation resulted in invalid number".to_string(),
            ));
        }
        Ok(JValue::Number(result))
    }

    /// $formatNumber(value, picture, options) - Format number with picture string
    /// Implements XPath F&O number formatting specification
    pub fn format_number(
        value: f64,
        picture: &str,
        options: Option<&JValue>,
    ) -> Result<JValue, FunctionError> {
        // Default format properties (can be overridden by options)
        let mut decimal_separator = '.';
        let mut grouping_separator = ',';
        let mut zero_digit = '0';
        let mut percent_symbol = "%".to_string();
        let mut per_mille_symbol = "\u{2030}".to_string();
        let digit_char = '#';
        let pattern_separator = ';';

        // Parse options if provided
        if let Some(JValue::Object(opts)) = options {
            if let Some(JValue::String(s)) = opts.get("decimal-separator") {
                decimal_separator = s.chars().next().unwrap_or('.');
            }
            if let Some(JValue::String(s)) = opts.get("grouping-separator") {
                grouping_separator = s.chars().next().unwrap_or(',');
            }
            if let Some(JValue::String(s)) = opts.get("zero-digit") {
                zero_digit = s.chars().next().unwrap_or('0');
            }
            if let Some(JValue::String(s)) = opts.get("percent") {
                percent_symbol = s.to_string();
            }
            if let Some(JValue::String(s)) = opts.get("per-mille") {
                per_mille_symbol = s.to_string();
            }
        }

        // Split picture into sub-pictures (positive and negative patterns)
        let sub_pictures: Vec<&str> = picture.split(pattern_separator).collect();
        if sub_pictures.len() > 2 {
            return Err(FunctionError::ArgumentError(
                "D3080: Too many pattern separators in picture string".to_string(),
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

        Ok(JValue::string(result))
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
        let prefix_end = chars
            .iter()
            .position(|&c| {
                c == decimal_sep
                    || c == grouping_sep
                    || c == digit_char
                    || is_digit_in_family(c, zero_digit)
            })
            .unwrap_or(chars.len());
        let prefix: String = chars[..prefix_end].iter().collect();

        // Find suffix (chars after last active char)
        let suffix_start = chars
            .iter()
            .rposition(|&c| {
                c == decimal_sep
                    || c == grouping_sep
                    || c == digit_char
                    || is_digit_in_family(c, zero_digit)
            })
            .map(|pos| pos + 1)
            .unwrap_or(chars.len());
        let suffix: String = chars[suffix_start..].iter().collect();

        // Active part (between prefix and suffix)
        let active: String = chars[prefix_end..suffix_start].iter().collect();

        // Check for exponential notation (e.g., "00.000e0")
        let exponent_pos = active.find('e').or_else(|| active.find('E'));
        let (mantissa_part, exponent_part): (String, String) = if let Some(pos) = exponent_pos {
            (active[..pos].to_string(), active[pos + 1..].to_string())
        } else {
            (active.clone(), String::new())
        };

        // Split mantissa into integer and fractional parts using character positions
        let mantissa_chars: Vec<char> = mantissa_part.chars().collect();
        let decimal_pos = mantissa_chars.iter().position(|&c| c == decimal_sep);
        let (integer_part, fractional_part): (String, String) = if let Some(pos) = decimal_pos {
            (
                mantissa_chars[..pos].iter().collect(),
                mantissa_chars[pos + 1..].iter().collect(),
            )
        } else {
            (mantissa_part.clone(), String::new())
        };

        // Validate: only one decimal separator
        if active.matches(decimal_sep).count() > 1 {
            return Err(FunctionError::ArgumentError(
                "D3081: Multiple decimal separators in picture".to_string(),
            ));
        }

        // Validate: no grouping separator adjacent to decimal
        if let Some(pos) = decimal_pos {
            if pos > 0 && active.chars().nth(pos - 1) == Some(grouping_sep) {
                return Err(FunctionError::ArgumentError(
                    "D3087: Grouping separator adjacent to decimal separator".to_string(),
                ));
            }
            if pos + 1 < active.len() && active.chars().nth(pos + 1) == Some(grouping_sep) {
                return Err(FunctionError::ArgumentError(
                    "D3087: Grouping separator adjacent to decimal separator".to_string(),
                ));
            }
        }

        // Validate: no consecutive grouping separators
        let grouping_str = format!("{}{}", grouping_sep, grouping_sep);
        if picture.contains(&grouping_str) {
            return Err(FunctionError::ArgumentError(
                "D3089: Consecutive grouping separators in picture".to_string(),
            ));
        }

        // Detect percent and per-mille symbols
        let has_percent = picture.contains(percent_symbol);
        let has_per_mille = picture.contains(per_mille_symbol);

        // Validate: multiple percent signs
        if picture.matches(percent_symbol).count() > 1 {
            return Err(FunctionError::ArgumentError(
                "D3082: Multiple percent signs in picture".to_string(),
            ));
        }

        // Validate: multiple per-mille signs
        if picture.matches(per_mille_symbol).count() > 1 {
            return Err(FunctionError::ArgumentError(
                "D3083: Multiple per-mille signs in picture".to_string(),
            ));
        }

        // Validate: cannot have both percent and per-mille
        if has_percent && has_per_mille {
            return Err(FunctionError::ArgumentError(
                "D3084: Cannot have both percent and per-mille in picture".to_string(),
            ));
        }

        // Validate: integer part cannot end with grouping separator
        if !integer_part.is_empty() && integer_part.ends_with(grouping_sep) {
            return Err(FunctionError::ArgumentError(
                "D3088: Integer part ends with grouping separator".to_string(),
            ));
        }

        // Validate: at least one digit in mantissa (integer or fractional part)
        let has_digit_in_integer = integer_part
            .chars()
            .any(|c| is_digit_in_family(c, zero_digit) || c == digit_char);
        let has_digit_in_fractional = fractional_part
            .chars()
            .any(|c| is_digit_in_family(c, zero_digit) || c == digit_char);
        if !has_digit_in_integer && !has_digit_in_fractional {
            return Err(FunctionError::ArgumentError(
                "D3085: Picture must contain at least one digit".to_string(),
            ));
        }

        // Count minimum integer digits (mandatory digits in digit family)
        let min_integer_digits = integer_part
            .chars()
            .filter(|&c| is_digit_in_family(c, zero_digit))
            .count();

        // Count minimum and maximum fractional digits
        let min_fractional_digits = fractional_part
            .chars()
            .filter(|&c| is_digit_in_family(c, zero_digit))
            .count();
        let mut max_fractional_digits = fractional_part
            .chars()
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
                grouping_positions.iter().filter(|&&x| x == p).count()
                    == grouping_positions.len() / first_interval
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
            exponent_part
                .chars()
                .filter(|&c| is_digit_in_family(c, zero_digit))
                .count()
        } else {
            0
        };

        // Validate: exponent part must contain only digit characters (ASCII or custom digit family)
        if !exponent_part.is_empty()
            && exponent_part
                .chars()
                .any(|c| !is_digit_in_family(c, zero_digit))
        {
            return Err(FunctionError::ArgumentError(
                "D3093: Exponent must contain only digit characters".to_string(),
            ));
        }

        // Validate: exponent cannot be empty if 'e' is present
        if exponent_pos.is_some() && min_exponent_digits == 0 {
            return Err(FunctionError::ArgumentError(
                "D3093: Exponent cannot be empty".to_string(),
            ));
        }

        // Validate: percent/per-mille not allowed with exponential notation
        if min_exponent_digits > 0 && (has_percent || has_per_mille) {
            return Err(FunctionError::ArgumentError(
                "D3092: Percent/per-mille not allowed with exponential notation".to_string(),
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
        let valid_chars: Vec<char> =
            vec![decimal_sep, grouping_sep, zero_digit, digit_char, 'e', 'E'];
        for c in mantissa_part.chars() {
            if !is_digit_in_family(c, zero_digit) && !valid_chars.contains(&c) {
                return Err(FunctionError::ArgumentError(format!(
                    "D3086: Invalid character in picture: '{}'",
                    c
                )));
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
            result = result
                .chars()
                .map(|c| {
                    if c.is_ascii_digit() {
                        let digit_value = c as u32 - '0' as u32;
                        char::from_u32(zero_code + digit_value).unwrap_or(c)
                    } else {
                        c
                    }
                })
                .collect();
        }

        // Append exponent if present
        if let Some(exp) = exponent {
            // Format exponent with minimum digits
            let exp_str = format!("{:0width$}", exp.abs(), width = parts.min_exponent_digits);

            // Convert exponent digits to custom zero-digit base if needed
            let exp_formatted = if zero_digit != '0' {
                let zero_code = zero_digit as u32;
                exp_str
                    .chars()
                    .map(|c| {
                        if c.is_ascii_digit() {
                            let digit_value = c as u32 - '0' as u32;
                            char::from_u32(zero_code + digit_value).unwrap_or(c)
                        } else {
                            c
                        }
                    })
                    .collect()
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
    pub fn format_base(value: f64, radix: Option<i64>) -> Result<JValue, FunctionError> {
        // Round to integer
        let int_value = value.round() as i64;

        // Default radix is 10
        let radix = radix.unwrap_or(10);

        // Validate radix is between 2 and 36
        if !(2..=36).contains(&radix) {
            return Err(FunctionError::ArgumentError(format!(
                "D3100: Radix must be between 2 and 36, got {}",
                radix
            )));
        }

        // Handle negative numbers
        let is_negative = int_value < 0;
        let abs_value = int_value.unsigned_abs();

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

        Ok(JValue::string(result))
    }
}

/// Built-in array functions
pub mod array {
    use super::*;

    /// $count(array) - Count array elements
    pub fn count(arr: &[JValue]) -> Result<JValue, FunctionError> {
        Ok(JValue::Number(arr.len() as f64))
    }

    /// $append(array1, array2) - Append arrays/values
    pub fn append(arr1: &[JValue], val: &JValue) -> Result<JValue, FunctionError> {
        let mut result = arr1.to_vec();
        match val {
            JValue::Array(arr2) => result.extend(arr2.iter().cloned()),
            other => result.push(other.clone()),
        }
        Ok(JValue::array(result))
    }

    /// $reverse(array) - Reverse array
    pub fn reverse(arr: &[JValue]) -> Result<JValue, FunctionError> {
        let mut result = arr.to_vec();
        result.reverse();
        Ok(JValue::array(result))
    }

    /// $sort(array) - Sort array
    pub fn sort(arr: &[JValue]) -> Result<JValue, FunctionError> {
        let mut result = arr.to_vec();

        // Check if all elements are of comparable types
        let all_numbers = result.iter().all(|v| matches!(v, JValue::Number(_)));
        let all_strings = result.iter().all(|v| matches!(v, JValue::String(_)));

        if all_numbers {
            result.sort_by(|a, b| {
                let a_num = a.as_f64().unwrap();
                let b_num = b.as_f64().unwrap();
                a_num
                    .partial_cmp(&b_num)
                    .unwrap_or(std::cmp::Ordering::Equal)
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

        Ok(JValue::array(result))
    }

    /// $distinct(array) - Get unique elements
    pub fn distinct(arr: &[JValue]) -> Result<JValue, FunctionError> {
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

        Ok(JValue::array(result))
    }

    /// $exists(value) - Check if value exists (not null/undefined)
    pub fn exists(value: &JValue) -> Result<JValue, FunctionError> {
        let is_missing = matches!(value, JValue::Null) || value.is_undefined();
        Ok(JValue::Bool(!is_missing))
    }

    /// Compare two JSON values for deep equality (JSONata semantics)
    pub fn values_equal(a: &JValue, b: &JValue) -> bool {
        match (a, b) {
            (JValue::Null, JValue::Null) => true,
            (JValue::Bool(a), JValue::Bool(b)) => a == b,
            (JValue::Number(a), JValue::Number(b)) => a == b,
            (JValue::String(a), JValue::String(b)) => a == b,
            (JValue::Array(a), JValue::Array(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| values_equal(x, y))
            }
            (JValue::Object(a), JValue::Object(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .all(|(k, v)| b.get(k).is_some_and(|v2| values_equal(v, v2)))
            }
            _ => false,
        }
    }

    /// $shuffle(array) - Randomly shuffle array elements
    /// Uses Fisher-Yates (inside-out variant) algorithm
    pub fn shuffle(arr: &[JValue]) -> Result<JValue, FunctionError> {
        if arr.len() <= 1 {
            return Ok(JValue::array(arr.to_vec()));
        }

        use rand::seq::SliceRandom;
        use rand::thread_rng;

        let mut result = arr.to_vec();
        let mut rng = thread_rng();
        result.shuffle(&mut rng);

        Ok(JValue::array(result))
    }
}

/// Built-in object functions
pub mod object {
    use super::*;

    /// $keys(object) - Get object keys
    pub fn keys(obj: &IndexMap<String, JValue>) -> Result<JValue, FunctionError> {
        let keys: Vec<JValue> = obj.keys().map(|k| JValue::string(k.as_str())).collect();
        Ok(JValue::array(keys))
    }

    /// $lookup(object, key) - Lookup value by key
    pub fn lookup(obj: &IndexMap<String, JValue>, key: &str) -> Result<JValue, FunctionError> {
        Ok(obj.get(key).cloned().unwrap_or(JValue::Null))
    }

    /// $spread(object) - Spread object into array of key-value pairs
    pub fn spread(obj: &IndexMap<String, JValue>) -> Result<JValue, FunctionError> {
        // Each key-value pair becomes a single-key object: {"key": value}
        let pairs: Vec<JValue> = obj
            .iter()
            .map(|(k, v)| {
                let mut pair = IndexMap::new();
                pair.insert(k.clone(), v.clone());
                JValue::object(pair)
            })
            .collect();
        Ok(JValue::array(pairs))
    }

    /// $merge(objects) - Merge multiple objects
    pub fn merge(objects: &[JValue]) -> Result<JValue, FunctionError> {
        let mut result = IndexMap::new();

        for obj in objects {
            match obj {
                JValue::Object(map) => {
                    for (k, v) in map.iter() {
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

        Ok(JValue::object(result))
    }
}

/// Encoding/decoding functions
pub mod encoding {
    use super::*;
    use base64::{engine::general_purpose, Engine as _};

    /// $base64encode(string) - Encode string to base64
    pub fn base64encode(s: &str) -> Result<JValue, FunctionError> {
        let encoded = general_purpose::STANDARD.encode(s.as_bytes());
        Ok(JValue::string(encoded))
    }

    /// $base64decode(string) - Decode base64 string
    pub fn base64decode(s: &str) -> Result<JValue, FunctionError> {
        match general_purpose::STANDARD.decode(s.as_bytes()) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(decoded) => Ok(JValue::string(decoded)),
                Err(_) => Err(FunctionError::RuntimeError(
                    "Invalid UTF-8 in decoded base64".to_string(),
                )),
            },
            Err(_) => Err(FunctionError::RuntimeError(
                "Invalid base64 string".to_string(),
            )),
        }
    }

    /// $encodeUrlComponent(string) - Encode URL component
    pub fn encode_url_component(s: &str) -> Result<JValue, FunctionError> {
        let encoded = percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC)
            .to_string();
        Ok(JValue::string(encoded))
    }

    /// $decodeUrlComponent(string) - Decode URL component
    pub fn decode_url_component(s: &str) -> Result<JValue, FunctionError> {
        match percent_encoding::percent_decode_str(s).decode_utf8() {
            Ok(decoded) => Ok(JValue::string(decoded.to_string())),
            Err(_) => Err(FunctionError::RuntimeError(
                "Invalid percent-encoded string".to_string(),
            )),
        }
    }

    /// $encodeUrl(string) - Encode full URL
    /// More permissive than encodeUrlComponent - allows URL structure characters
    pub fn encode_url(s: &str) -> Result<JValue, FunctionError> {
        // Use CONTROLS to preserve URL structure (://?#[]@!$&'()*+,;=)
        let encoded =
            percent_encoding::utf8_percent_encode(s, percent_encoding::CONTROLS).to_string();
        Ok(JValue::string(encoded))
    }

    /// $decodeUrl(string) - Decode full URL
    pub fn decode_url(s: &str) -> Result<JValue, FunctionError> {
        match percent_encoding::percent_decode_str(s).decode_utf8() {
            Ok(decoded) => Ok(JValue::string(decoded.to_string())),
            Err(_) => Err(FunctionError::RuntimeError(
                "Invalid percent-encoded URL".to_string(),
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
            string::string(&JValue::string("hello"), None).unwrap(),
            JValue::string("hello")
        );

        // Number to string
        assert_eq!(
            string::string(&JValue::Number(42.0), None).unwrap(),
            JValue::string("42")
        );

        // Float to string
        assert_eq!(
            string::string(&JValue::Number(3.14), None).unwrap(),
            JValue::string("3.14")
        );

        // Boolean to string
        assert_eq!(
            string::string(&JValue::Bool(true), None).unwrap(),
            JValue::string("true")
        );

        // Null becomes "null" via JSON.stringify
        assert_eq!(
            string::string(&JValue::Null, None).unwrap(),
            JValue::string("null")
        );

        // Array gets JSON.stringify'd
        assert_eq!(
            string::string(
                &JValue::array(vec![
                    JValue::from(1i64),
                    JValue::from(2i64),
                    JValue::from(3i64)
                ]),
                None
            )
            .unwrap(),
            JValue::string("[1,2,3]")
        );
    }

    #[test]
    fn test_length() {
        assert_eq!(string::length("hello").unwrap(), JValue::Number(5.0));
        assert_eq!(string::length("").unwrap(), JValue::Number(0.0));
        // Unicode support
        assert_eq!(
            string::length("Hello \u{4e16}\u{754c}").unwrap(),
            JValue::Number(8.0)
        );
        assert_eq!(
            string::length("\u{1f389}\u{1f38a}").unwrap(),
            JValue::Number(2.0)
        );
    }

    #[test]
    fn test_uppercase_lowercase() {
        assert_eq!(string::uppercase("hello").unwrap(), JValue::string("HELLO"));
        assert_eq!(string::lowercase("HELLO").unwrap(), JValue::string("hello"));
        assert_eq!(
            string::uppercase("Hello World").unwrap(),
            JValue::string("HELLO WORLD")
        );
    }

    #[test]
    fn test_substring() {
        // Basic substring
        assert_eq!(
            string::substring("hello world", 0, Some(5)).unwrap(),
            JValue::string("hello")
        );

        // From position to end
        assert_eq!(
            string::substring("hello world", 6, None).unwrap(),
            JValue::string("world")
        );

        // Negative start position
        assert_eq!(
            string::substring("hello world", -5, Some(5)).unwrap(),
            JValue::string("world")
        );

        // Unicode support
        assert_eq!(
            string::substring("Hello \u{4e16}\u{754c}", 6, Some(2)).unwrap(),
            JValue::string("\u{4e16}\u{754c}")
        );

        // Negative length returns empty string
        assert_eq!(
            string::substring("hello", 0, Some(-1)).unwrap(),
            JValue::string("")
        );
    }

    #[test]
    fn test_substring_before_after() {
        // substringBefore
        assert_eq!(
            string::substring_before("hello world", " ").unwrap(),
            JValue::string("hello")
        );
        assert_eq!(
            string::substring_before("hello world", "x").unwrap(),
            JValue::string("hello world")
        );
        assert_eq!(
            string::substring_before("hello world", "").unwrap(),
            JValue::string("")
        );

        // substringAfter
        assert_eq!(
            string::substring_after("hello world", " ").unwrap(),
            JValue::string("world")
        );
        // When separator is not found, return the original string
        assert_eq!(
            string::substring_after("hello world", "x").unwrap(),
            JValue::string("hello world")
        );
        assert_eq!(
            string::substring_after("hello world", "").unwrap(),
            JValue::string("hello world")
        );
    }

    #[test]
    fn test_trim() {
        assert_eq!(string::trim("  hello  ").unwrap(), JValue::string("hello"));
        assert_eq!(string::trim("hello").unwrap(), JValue::string("hello"));
        assert_eq!(
            string::trim("\t\nhello\r\n").unwrap(),
            JValue::string("hello")
        );
    }

    #[test]
    fn test_contains() {
        assert_eq!(
            string::contains("hello world", &JValue::string("world")).unwrap(),
            JValue::Bool(true)
        );
        assert_eq!(
            string::contains("hello world", &JValue::string("xyz")).unwrap(),
            JValue::Bool(false)
        );
        assert_eq!(
            string::contains("hello world", &JValue::string("")).unwrap(),
            JValue::Bool(true)
        );
    }

    #[test]
    fn test_split() {
        // Split with separator
        assert_eq!(
            string::split("a,b,c", &JValue::string(","), None).unwrap(),
            JValue::array(vec![
                JValue::string("a"),
                JValue::string("b"),
                JValue::string("c")
            ])
        );

        // Split with limit - truncates to limit number of results
        assert_eq!(
            string::split("a,b,c,d", &JValue::string(","), Some(2)).unwrap(),
            JValue::array(vec![JValue::string("a"), JValue::string("b")])
        );

        // Split with empty separator (split into chars)
        assert_eq!(
            string::split("abc", &JValue::string(""), None).unwrap(),
            JValue::array(vec![
                JValue::string("a"),
                JValue::string("b"),
                JValue::string("c")
            ])
        );
    }

    #[test]
    fn test_join() {
        // Join with separator
        let arr = vec![
            JValue::string("a"),
            JValue::string("b"),
            JValue::string("c"),
        ];
        assert_eq!(
            string::join(&arr, Some(",")).unwrap(),
            JValue::string("a,b,c")
        );

        // Join without separator
        assert_eq!(string::join(&arr, None).unwrap(), JValue::string("abc"));

        // Join with numbers
        let arr = vec![JValue::from(1i64), JValue::from(2i64), JValue::from(3i64)];
        assert_eq!(
            string::join(&arr, Some("-")).unwrap(),
            JValue::string("1-2-3")
        );
    }

    #[test]
    fn test_replace() {
        // Replace all occurrences
        assert_eq!(
            string::replace("hello hello", &JValue::string("hello"), "hi", None).unwrap(),
            JValue::string("hi hi")
        );

        // Replace with limit
        assert_eq!(
            string::replace("hello hello hello", &JValue::string("hello"), "hi", Some(2)).unwrap(),
            JValue::string("hi hi hello")
        );

        // Replace empty pattern returns error D3010
        assert!(string::replace("hello", &JValue::string(""), "x", None).is_err());
    }

    // ===== Numeric Functions Tests =====

    #[test]
    fn test_number_conversion() {
        // Number to number
        assert_eq!(
            numeric::number(&JValue::Number(42.0)).unwrap(),
            JValue::Number(42.0)
        );

        // String to number
        assert_eq!(
            numeric::number(&JValue::string("42")).unwrap(),
            JValue::Number(42.0)
        );
        assert_eq!(
            numeric::number(&JValue::string("3.14")).unwrap(),
            JValue::Number(3.14)
        );
        assert_eq!(
            numeric::number(&JValue::string("  123  ")).unwrap(),
            JValue::Number(123.0)
        );

        // Boolean to number
        assert_eq!(
            numeric::number(&JValue::Bool(true)).unwrap(),
            JValue::Number(1.0)
        );
        assert_eq!(
            numeric::number(&JValue::Bool(false)).unwrap(),
            JValue::Number(0.0)
        );

        // Invalid conversions
        assert!(numeric::number(&JValue::Null).is_err());
        assert!(numeric::number(&JValue::string("not a number")).is_err());
    }

    #[test]
    fn test_sum() {
        // Sum of numbers
        let arr = vec![JValue::from(1i64), JValue::from(2i64), JValue::from(3i64)];
        assert_eq!(numeric::sum(&arr).unwrap(), JValue::Number(6.0));

        // Empty array
        assert_eq!(numeric::sum(&[]).unwrap(), JValue::Number(0.0));

        // Array with non-numbers should error
        let arr = vec![JValue::from(1i64), JValue::string("2")];
        assert!(numeric::sum(&arr).is_err());
    }

    #[test]
    fn test_max_min() {
        let arr = vec![
            JValue::from(3i64),
            JValue::from(1i64),
            JValue::from(4i64),
            JValue::from(2i64),
        ];

        assert_eq!(numeric::max(&arr).unwrap(), JValue::Number(4.0));
        assert_eq!(numeric::min(&arr).unwrap(), JValue::Number(1.0));

        // Empty array
        assert_eq!(numeric::max(&[]).unwrap(), JValue::Null);
        assert_eq!(numeric::min(&[]).unwrap(), JValue::Null);
    }

    #[test]
    fn test_average() {
        let arr = vec![
            JValue::from(1i64),
            JValue::from(2i64),
            JValue::from(3i64),
            JValue::from(4i64),
        ];
        assert_eq!(numeric::average(&arr).unwrap(), JValue::Number(2.5));

        // Empty array
        assert_eq!(numeric::average(&[]).unwrap(), JValue::Null);
    }

    #[test]
    fn test_math_functions() {
        // abs
        assert_eq!(numeric::abs(-5.5).unwrap(), JValue::Number(5.5));
        assert_eq!(numeric::abs(5.5).unwrap(), JValue::Number(5.5));

        // floor
        assert_eq!(numeric::floor(3.7).unwrap(), JValue::Number(3.0));
        assert_eq!(numeric::floor(-3.7).unwrap(), JValue::Number(-4.0));

        // ceil
        assert_eq!(numeric::ceil(3.2).unwrap(), JValue::Number(4.0));
        assert_eq!(numeric::ceil(-3.2).unwrap(), JValue::Number(-3.0));

        // round - whole number results are returned as numbers
        assert_eq!(
            numeric::round(3.14159, Some(2)).unwrap(),
            JValue::Number(3.14)
        );
        assert_eq!(numeric::round(3.14159, None).unwrap(), JValue::Number(3.0));
        // Negative precision is supported (rounds to powers of 10)
        assert_eq!(numeric::round(3.14, Some(-1)).unwrap(), JValue::Number(0.0));

        // sqrt
        assert_eq!(numeric::sqrt(16.0).unwrap(), JValue::Number(4.0));
        assert!(numeric::sqrt(-1.0).is_err());

        // power
        assert_eq!(numeric::power(2.0, 3.0).unwrap(), JValue::Number(8.0));
        assert_eq!(numeric::power(9.0, 0.5).unwrap(), JValue::Number(3.0));
    }

    // ===== Array Functions Tests =====

    #[test]
    fn test_count() {
        let arr = vec![JValue::from(1i64), JValue::from(2i64), JValue::from(3i64)];
        assert_eq!(array::count(&arr).unwrap(), JValue::Number(3.0));
        assert_eq!(array::count(&[]).unwrap(), JValue::Number(0.0));
    }

    #[test]
    fn test_append() {
        let arr1 = vec![JValue::from(1i64), JValue::from(2i64)];

        // Append a single value
        let result = array::append(&arr1, &JValue::from(3i64)).unwrap();
        assert_eq!(
            result,
            JValue::array(vec![
                JValue::from(1i64),
                JValue::from(2i64),
                JValue::from(3i64)
            ])
        );

        // Append an array
        let arr2 = JValue::array(vec![JValue::from(3i64), JValue::from(4i64)]);
        let result = array::append(&arr1, &arr2).unwrap();
        assert_eq!(
            result,
            JValue::array(vec![
                JValue::from(1i64),
                JValue::from(2i64),
                JValue::from(3i64),
                JValue::from(4i64)
            ])
        );
    }

    #[test]
    fn test_reverse() {
        let arr = vec![JValue::from(1i64), JValue::from(2i64), JValue::from(3i64)];
        assert_eq!(
            array::reverse(&arr).unwrap(),
            JValue::array(vec![
                JValue::from(3i64),
                JValue::from(2i64),
                JValue::from(1i64)
            ])
        );
    }

    #[test]
    fn test_sort() {
        // Sort numbers
        let arr = vec![
            JValue::from(3i64),
            JValue::from(1i64),
            JValue::from(4i64),
            JValue::from(2i64),
        ];
        assert_eq!(
            array::sort(&arr).unwrap(),
            JValue::array(vec![
                JValue::from(1i64),
                JValue::from(2i64),
                JValue::from(3i64),
                JValue::from(4i64)
            ])
        );

        // Sort strings
        let arr = vec![
            JValue::string("charlie"),
            JValue::string("alice"),
            JValue::string("bob"),
        ];
        assert_eq!(
            array::sort(&arr).unwrap(),
            JValue::array(vec![
                JValue::string("alice"),
                JValue::string("bob"),
                JValue::string("charlie")
            ])
        );

        // Mixed types should error
        let arr = vec![JValue::from(1i64), JValue::string("a")];
        assert!(array::sort(&arr).is_err());
    }

    #[test]
    fn test_distinct() {
        let arr = vec![
            JValue::from(1i64),
            JValue::from(2i64),
            JValue::from(1i64),
            JValue::from(3i64),
            JValue::from(2i64),
        ];
        assert_eq!(
            array::distinct(&arr).unwrap(),
            JValue::array(vec![
                JValue::from(1i64),
                JValue::from(2i64),
                JValue::from(3i64)
            ])
        );

        // With strings
        let arr = vec![
            JValue::string("a"),
            JValue::string("b"),
            JValue::string("a"),
        ];
        assert_eq!(
            array::distinct(&arr).unwrap(),
            JValue::array(vec![JValue::string("a"), JValue::string("b")])
        );
    }

    #[test]
    fn test_exists() {
        assert_eq!(
            array::exists(&JValue::Number(42.0)).unwrap(),
            JValue::Bool(true)
        );
        assert_eq!(
            array::exists(&JValue::string("hello")).unwrap(),
            JValue::Bool(true)
        );
        assert_eq!(array::exists(&JValue::Null).unwrap(), JValue::Bool(false));
    }

    // ===== Object Functions Tests =====

    #[test]
    fn test_keys() {
        let mut obj = IndexMap::new();
        obj.insert("name".to_string(), JValue::string("Alice"));
        obj.insert("age".to_string(), JValue::Number(30.0));

        let result = object::keys(&obj).unwrap();
        if let JValue::Array(keys) = result {
            assert_eq!(keys.len(), 2);
            assert!(keys.contains(&JValue::string("name")));
            assert!(keys.contains(&JValue::string("age")));
        } else {
            panic!("Expected array of keys");
        }
    }

    #[test]
    fn test_lookup() {
        let mut obj = IndexMap::new();
        obj.insert("name".to_string(), JValue::string("Alice"));
        obj.insert("age".to_string(), JValue::Number(30.0));

        assert_eq!(
            object::lookup(&obj, "name").unwrap(),
            JValue::string("Alice")
        );
        assert_eq!(object::lookup(&obj, "age").unwrap(), JValue::Number(30.0));
        assert_eq!(object::lookup(&obj, "missing").unwrap(), JValue::Null);
    }

    #[test]
    fn test_spread() {
        let mut obj = IndexMap::new();
        obj.insert("a".to_string(), JValue::from(1i64));
        obj.insert("b".to_string(), JValue::from(2i64));

        let result = object::spread(&obj).unwrap();
        if let JValue::Array(pairs) = result {
            assert_eq!(pairs.len(), 2);
            // Each key-value pair becomes a single-key object: {"key": value}
            for pair in pairs.iter() {
                if let JValue::Object(p) = pair {
                    assert_eq!(
                        p.len(),
                        1,
                        "Each spread element should be a single-key object"
                    );
                } else {
                    panic!("Expected Object in spread result");
                }
            }
            // Verify the actual spread results contain expected keys
            let all_keys: Vec<String> = pairs
                .iter()
                .filter_map(|p| {
                    if let JValue::Object(m) = p {
                        m.keys().next().cloned()
                    } else {
                        None
                    }
                })
                .collect();
            assert!(all_keys.contains(&"a".to_string()));
            assert!(all_keys.contains(&"b".to_string()));
        } else {
            panic!("Expected array of key-value pairs");
        }
    }

    #[test]
    fn test_merge() {
        let mut obj1 = IndexMap::new();
        obj1.insert("a".to_string(), JValue::from(1i64));
        obj1.insert("b".to_string(), JValue::from(2i64));

        let mut obj2 = IndexMap::new();
        obj2.insert("b".to_string(), JValue::from(3i64));
        obj2.insert("c".to_string(), JValue::from(4i64));

        let arr = vec![JValue::object(obj1), JValue::object(obj2)];
        let result = object::merge(&arr).unwrap();

        if let JValue::Object(merged) = result {
            assert_eq!(merged.get("a"), Some(&JValue::from(1i64)));
            assert_eq!(merged.get("b"), Some(&JValue::from(3i64))); // Later value wins
            assert_eq!(merged.get("c"), Some(&JValue::from(4i64)));
        } else {
            panic!("Expected merged object");
        }
    }
}

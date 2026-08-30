// Built-in function implementations
// Mirrors functions.js from the reference implementation

#![allow(clippy::explicit_counter_loop)]
#![allow(clippy::approx_constant)]

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

    /// Python→JValue conversion failed while comparing/materializing a lazy
    /// element (e.g. inside `$distinct`). Mirrors `EvaluatorError::PyConversionError`
    /// -- `impl From<FunctionError> for EvaluatorError` maps this specific
    /// variant to `EvaluatorError::PyConversionError` (surfacing as Python
    /// `TypeError` at the boundary) instead of the generic `EvaluationError`
    /// (`ValueError`) every other `FunctionError` variant collapses to.
    #[cfg(feature = "python")]
    #[error("Type error: {0}")]
    PyConversionError(String),
}

/// Built-in string functions
/// Mimic JS Array.prototype.slice(start, end) semantics.
/// - Negative start/end count from the end of the array.
/// - Out-of-bounds values are clamped.
fn js_slice<T: Clone>(arr: &[T], start: i64, end: Option<i64>) -> Vec<T> {
    let len = arr.len() as i64;
    let s = if start < 0 {
        (len + start).max(0) as usize
    } else {
        start.min(len) as usize
    };
    let e = match end {
        Some(end) => {
            if end < 0 {
                (len + end).max(0) as usize
            } else {
                (end.min(len)) as usize
            }
        }
        None => arr.len(),
    };
    if s >= e {
        return Vec::new();
    }
    arr[s..e].to_vec()
}

pub mod string {
    use super::*;
    use regex::Regex;

    /// Helper to detect and extract regex from a JValue
    pub fn extract_regex(value: &JValue) -> Option<(String, String)> {
        match value {
            JValue::Regex { pattern, flags } => Some((pattern.to_string(), flags.to_string())),
            _ => None,
        }
    }

    /// Helper to build a Regex from pattern and flags
    pub fn build_regex(pattern: &str, flags: &str) -> Result<Regex, FunctionError> {
        // Convert JSONata flags to Rust inline regex flags. Only emit the
        // group for flags Rust regex knows; a bare "(?)" (e.g. from the
        // internal-only `g` flag alone) is a syntax error.
        let mut inline = String::new();
        if flags.contains('i') {
            inline.push('i'); // case-insensitive
        }
        if flags.contains('m') {
            inline.push('m'); // multi-line
        }
        if flags.contains('s') {
            inline.push('s'); // dot matches newline
        }

        let mut regex_pattern = String::new();
        if !inline.is_empty() {
            regex_pattern.push_str("(?");
            regex_pattern.push_str(&inline);
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
            return Ok(JValue::Undefined);
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

                // Format numbers like JavaScript does: integers print as
                // the shortest round-trip decimal; non-integers are rounded
                // to 15 significant digits first (jsonata-js's replacer:
                // `Number.isInteger(val) ? val : Number(val.toPrecision(15))`)
                format_stringified_number(f)
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
            #[cfg(feature = "python")]
            JValue::LazyPyDict(_) => {
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

    /// Round to 15 significant digits, the way jsonata-js's `$string`
    /// replacer treats every non-integer number
    /// (`Number(val.toPrecision(15))`). `{:.14e}` (1 leading digit + 14
    /// decimals) is exactly `toPrecision(15)`; the round-trip parse yields
    /// the normalized rounded value.
    fn round_to_precision_15(f: f64) -> f64 {
        format!("{:.14e}", f).parse().unwrap_or(f)
    }

    /// Print one finite number for `$string`/stringification: integers keep
    /// full (shortest round-trip) precision, non-integers are rounded to 15
    /// significant digits first, and both print via the shared JS number
    /// printer (exponential outside [1e-6, 1e21), `+` on positive exponents,
    /// `-0` as `0`).
    fn format_stringified_number(f: f64) -> String {
        if f.fract() == 0.0 {
            crate::value::js_number_to_string(f)
        } else {
            crate::value::js_number_to_string(round_to_precision_15(f))
        }
    }

    /// Reject a non-finite number anywhere in the value.
    ///
    /// jsonata-js serializes through `isNumeric`, which throws D1001 for
    /// Infinity or NaN. Without this the number becomes JSON `null` and
    /// `$string({"inf": 1/0})` quietly returns `{"inf":null}`. The scalar case
    /// raises D3001 earlier, matching the reference.
    fn reject_non_finite(value: &JValue) -> Result<(), FunctionError> {
        match value {
            JValue::Number(n) if !n.is_finite() => Err(FunctionError::RuntimeError(format!(
                "D1001: Number out of range: {}",
                n
            ))),
            JValue::Array(arr) => arr.iter().try_for_each(reject_non_finite),
            JValue::Object(obj) => obj.values().try_for_each(reject_non_finite),
            _ => Ok(()),
        }
    }

    /// Stringify a value the way jsonata-js's `$string` does
    /// (`JSON.stringify` with the function-to-"" / non-integer-toPrecision(15)
    /// replacer). Hand-rolled rather than routed through serde so numbers
    /// print with JavaScript's exact rules — integers above 2^53 print as the
    /// float's shortest round-trip decimal, not the exact i64 digits serde
    /// would emit.
    fn stringify_value_custom(
        value: &JValue,
        indent: Option<usize>,
    ) -> Result<String, FunctionError> {
        reject_non_finite(value)?;
        let mut out = String::new();
        write_stringified(value, indent, 0, &mut out);
        Ok(out)
    }

    /// Recursive writer for `stringify_value_custom`. `indent` = Some(width)
    /// pretty-prints exactly like `JSON.stringify(v, replacer, width)` (and
    /// serde_json's pretty printer): items one per line, `": "` after keys,
    /// empty containers stay `{}`/`[]`.
    fn write_stringified(value: &JValue, indent: Option<usize>, depth: usize, out: &mut String) {
        let (open_sep, close_sep, item_sep, key_sep): (String, String, &str, &str) = match indent {
            Some(w) => (
                format!("\n{}", " ".repeat(w * (depth + 1))),
                format!("\n{}", " ".repeat(w * depth)),
                ",",
                ": ",
            ),
            None => (String::new(), String::new(), ",", ":"),
        };
        match value {
            // JSON.stringify turns undefined into null in array position; an
            // undefined-valued object key never gets here (construction drops
            // it), so mapping the whole variant to null preserves both.
            JValue::Null | JValue::Undefined => out.push_str("null"),
            JValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            JValue::Number(n) => out.push_str(&format_stringified_number(*n)),
            JValue::String(s) => write_json_escaped(s, out),
            // The replacer maps functions to "" wherever they appear.
            JValue::Lambda { .. } | JValue::Builtin { .. } => out.push_str("\"\""),
            JValue::Regex { pattern, flags } => {
                // Mirrors the serde Serialize impl: a regex value renders as
                // {"pattern": ..., "flags": ...}.
                out.push('{');
                out.push_str(&open_sep);
                write_json_escaped("pattern", out);
                out.push_str(key_sep);
                write_json_escaped(pattern, out);
                out.push_str(item_sep);
                out.push_str(&open_sep);
                write_json_escaped("flags", out);
                out.push_str(key_sep);
                write_json_escaped(flags, out);
                out.push_str(&close_sep);
                out.push('}');
            }
            JValue::Array(arr) => {
                if arr.is_empty() {
                    out.push_str("[]");
                    return;
                }
                out.push('[');
                for (i, v) in arr.iter().enumerate() {
                    if i > 0 {
                        out.push_str(item_sep);
                    }
                    out.push_str(&open_sep);
                    write_stringified(v, indent, depth + 1, out);
                }
                out.push_str(&close_sep);
                out.push(']');
            }
            JValue::Object(obj) => {
                if obj.is_empty() {
                    out.push_str("{}");
                    return;
                }
                out.push('{');
                for (i, (k, v)) in obj.iter().enumerate() {
                    if i > 0 {
                        out.push_str(item_sep);
                    }
                    out.push_str(&open_sep);
                    write_json_escaped(k, out);
                    out.push_str(key_sep);
                    write_stringified(v, indent, depth + 1, out);
                }
                out.push_str(&close_sep);
                out.push('}');
            }
            #[cfg(feature = "python")]
            JValue::LazyPyDict(lazy) => match lazy.to_object_ref() {
                Some(obj) => {
                    write_stringified(&JValue::object(obj.clone()), indent, depth, out);
                }
                None => out.push_str("null"),
            },
        }
    }

    /// Append the JSON string literal (quotes and escapes included) for `s`.
    fn write_json_escaped(s: &str, out: &mut String) {
        // serde_json's string escaping is JSON.stringify's.
        out.push_str(&serde_json::to_string(s).expect("string serialization is infallible"));
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
    /// Extracts a substring from a string using Unicode character positions.
    /// Follows the JSONata spec (which mirrors JS Array.prototype.slice):
    /// - start: zero-based position; negative means count from end
    /// - length: optional max number of characters to extract
    pub fn substring(s: &str, start: i64, length: Option<i64>) -> Result<JValue, FunctionError> {
        let chars: Vec<char> = s.chars().collect();
        let str_len = chars.len() as i64;

        // Clamp start if it goes past the beginning
        // Matches JS: if (strLength + start < 0) { start = 0; }
        let start = if str_len + start < 0 { 0 } else { start };

        if let Some(len) = length {
            // Negative or zero length → empty string (matches JS reference)
            if len <= 0 {
                return Ok(JValue::string(""));
            }
            // Compute end index: mirrors JS reference exactly
            // JS: var end = start >= 0 ? start + length : strLength + start + length;
            let end = if start >= 0 {
                start + len
            } else {
                str_len + start + len
            };
            // JS: strArray.slice(start, end).join('')
            // JS slice handles negative start natively (counts from end)
            let slice = js_slice(&chars, start, Some(end));
            Ok(JValue::string(slice.iter().collect::<String>()))
        } else {
            // No length: take from start to end of string
            // JS: strArray.slice(start).join('')
            let slice = js_slice(&chars, start, None);
            Ok(JValue::string(slice.iter().collect::<String>()))
        }
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

        // Handle string separator. The signature `<s-(sf)n?:a<s>>` admits a
        // string or a function, and the regex/function branch is handled
        // above, so anything left here is a plain function used as a matcher
        // that did not produce the expected structure -- T1010 in jsonata-js.
        let sep = match separator {
            JValue::String(s) => &**s,
            _ => {
                return Err(FunctionError::TypeError(
                    "T1010: The matcher function argument passed to function $split does not \
                     return the correct object structure"
                        .to_string(),
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

    /// Format a number for $join. Unreachable through validated dispatch —
    /// the reference raises T0412 for non-string elements — but kept for
    /// unvalidated callers, printing the JS way like everything else.
    fn format_join_number(n: f64) -> String {
        crate::value::js_number_to_string(n)
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
            #[cfg(feature = "python")]
            JValue::LazyPyDict(lazy) => !lazy.is_empty(),
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
            return Err(FunctionError::ArgumentError(format!(
                "D3060: The sqrt function cannot be applied to a negative number: {}",
                n
            )));
        }
        Ok(JValue::Number(n.sqrt()))
    }

    /// $power(base, exponent) - Power
    pub fn power(base: f64, exponent: f64) -> Result<JValue, FunctionError> {
        let result = base.powf(exponent);
        if result.is_nan() || result.is_infinite() {
            return Err(FunctionError::RuntimeError(format!(
                "D3061: The power function has resulted in a value that cannot be \
                 represented as a JSON number: base={}, exponent={}",
                base, exponent
            )));
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
            // No active character at all means there is no prefix -- the whole
            // sub-picture is the active part, which is what makes `"k"` a
            // D3086 (a passive character in the active part) rather than
            // vanishing into a prefix. jsonata-js's prefix scan returns "" when
            // its loop finds nothing.
            .unwrap_or(0);
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

        // Picture-string validation, F&O 4.7.3.
        //
        // jsonata-js runs EVERY check in sequence, each assigning to one
        // `error` variable, and raises whatever is left at the end -- so the
        // LAST failing check names the error, not the first. These used to
        // return early, which reported an earlier check's code: `"k"` fails
        // both D3085 (no digit) and D3086 (passive character), and the
        // reference calls it D3086. Order below mirrors the reference's
        // exactly; do not sort it (#135).
        let has_percent = picture.contains(percent_symbol);
        let has_per_mille = picture.contains(per_mille_symbol);
        let has_digit_in_integer = integer_part
            .chars()
            .any(|c| is_digit_in_family(c, zero_digit) || c == digit_char);
        let has_digit_in_fractional = fractional_part
            .chars()
            .any(|c| is_digit_in_family(c, zero_digit) || c == digit_char);

        let mut error: Option<String> = None;

        if active.matches(decimal_sep).count() > 1 {
            error = Some("D3081: Multiple decimal separators in picture".to_string());
        }
        if picture.matches(percent_symbol).count() > 1 {
            error = Some("D3082: Multiple percent signs in picture".to_string());
        }
        if picture.matches(per_mille_symbol).count() > 1 {
            error = Some("D3083: Multiple per-mille signs in picture".to_string());
        }
        if has_percent && has_per_mille {
            error = Some("D3084: Cannot have both percent and per-mille in picture".to_string());
        }
        if !has_digit_in_integer && !has_digit_in_fractional {
            error = Some("D3085: Picture must contain at least one digit".to_string());
        }
        // Every character of the active part must be active. Percent and
        // per-mille are deliberately NOT active characters, which is why a
        // picture of only `%%` ends up here rather than at D3082.
        let valid_chars = [decimal_sep, grouping_sep, zero_digit, digit_char, 'e', 'E'];
        if let Some(c) = active
            .chars()
            .find(|&c| !is_digit_in_family(c, zero_digit) && !valid_chars.contains(&c))
        {
            error = Some(format!("D3086: Invalid character in picture: '{}'", c));
        }
        if let Some(pos) = decimal_pos {
            let adjacent = (pos > 0 && active.chars().nth(pos - 1) == Some(grouping_sep))
                || (pos + 1 < active.chars().count()
                    && active.chars().nth(pos + 1) == Some(grouping_sep));
            if adjacent {
                error = Some("D3087: Grouping separator adjacent to decimal separator".to_string());
            }
        } else if !integer_part.is_empty() && integer_part.ends_with(grouping_sep) {
            error = Some("D3088: Integer part ends with grouping separator".to_string());
        }
        if picture.contains(&format!("{}{}", grouping_sep, grouping_sep)) {
            error = Some("D3089: Consecutive grouping separators in picture".to_string());
        }
        let mut seen_zero_in_integer = false;
        for c in integer_part.chars() {
            if is_digit_in_family(c, zero_digit) {
                seen_zero_in_integer = true;
            } else if c == digit_char && seen_zero_in_integer {
                error = Some("D3090: Optional digit (#) cannot appear after mandatory digit (0) in integer part".to_string());
                break;
            }
        }
        let mut seen_hash_in_fractional = false;
        for c in fractional_part.chars() {
            if c == digit_char {
                seen_hash_in_fractional = true;
            } else if is_digit_in_family(c, zero_digit) && seen_hash_in_fractional {
                error = Some("D3091: Mandatory digit (0) cannot appear after optional digit (#) in fractional part".to_string());
                break;
            }
        }
        let exponent_exists = exponent_pos.is_some();
        if exponent_exists && !exponent_part.is_empty() && (has_percent || has_per_mille) {
            error =
                Some("D3092: Percent/per-mille not allowed with exponential notation".to_string());
        }
        if exponent_exists
            && (exponent_part.is_empty()
                || exponent_part
                    .chars()
                    .any(|c| !is_digit_in_family(c, zero_digit)))
        {
            error = Some("D3093: Exponent must contain only digit characters".to_string());
        }

        if let Some(code) = error {
            return Err(FunctionError::ArgumentError(code));
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
        let max_fractional_digits = fractional_part
            .chars()
            .filter(|&c| is_digit_in_family(c, zero_digit) || c == digit_char)
            .count();

        // F&O 4.7.4's adjustments, which jsonata-js applies verbatim. They are
        // what makes `#` an *optional* integer digit: `"#.#"` leaves
        // `min_integer_digits` at 0, so `$formatNumber(0.25, "#.#")` is ".2"
        // with no leading zero, while the third rule below still guarantees a
        // fractional digit so it is not the empty string.
        //
        // `scaling_factor` is captured from the UNADJUSTED count -- it drives
        // the exponent, and taking it after the adjustments would move the
        // decimal point.
        let scaling_factor = min_integer_digits;
        let exponent_present = exponent_pos.is_some();
        let (mut min_integer_digits, mut min_fractional_digits, mut max_fractional_digits) = (
            min_integer_digits,
            min_fractional_digits,
            max_fractional_digits,
        );
        if min_integer_digits == 0 && max_fractional_digits == 0 {
            if exponent_present {
                min_fractional_digits = 1;
                max_fractional_digits = 1;
            } else {
                min_integer_digits = 1;
            }
        }
        if exponent_present && min_integer_digits == 0 && integer_part.contains(digit_char) {
            min_integer_digits = 1;
        }
        if min_integer_digits == 0 && min_fractional_digits == 0 {
            min_fractional_digits = 1;
        }

        // No invented precision. jsonata-js derives the fractional digit counts
        // solely from the digit characters in the picture's fractional part, so
        // a picture like "0." has ZERO fractional digits -- which means the
        // value is rounded to an integer and no separator is shown:
        // `$formatNumber(1.5, "0.")` is "2", not "1.5" (#136).

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

        Ok(PictureParts {
            prefix,
            suffix,
            min_integer_digits,
            min_fractional_digits,
            max_fractional_digits,
            grouping_positions,
            fractional_grouping_positions,
            regular_grouping,
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

            // Magnitudes, and the upper bound is strictly-greater: with
            // `>=`, a picture of `"#.e0"` (scaling factor 0, so
            // max_mantissa 1) pushed a value of exactly 1 to "0.1e1" where
            // the reference leaves it at "1.0e0". Zero keeps exponent 0 --
            // F&O bullet 5 says so, and the loops would not terminate.
            if m != 0.0 {
                while m.abs() < min_mantissa {
                    m *= 10.0;
                    e -= 1;
                }
                while m.abs() > max_mantissa {
                    m /= 10.0;
                    e += 1;
                }
            }

            (m, Some(e))
        } else {
            (value, None)
        };

        // Round mantissa to max fractional digits.
        //
        // jsonata-js routes this through the same `round()` helper `$round`
        // uses, which is half-to-EVEN. `f64::round` is half-away-from-zero, so
        // `$formatNumber(0.25, "0.0")` came out "0.3" where the reference says
        // "0.2", and `$formatNumber(12.345, "#,##0.00")` was "12.35" not
        // "12.34" -- wrong numbers in ordinary pictures, not just edge cases
        // (#136).
        let rounded = match numeric::round(mantissa, Some(parts.max_fractional_digits as i32))? {
            JValue::Number(n) => n,
            _ => mantissa,
        };

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
        // Whether a lone zero is shown is decided by the *minimum* integer
        // digit count, not by whether the picture had an integer part at all.
        // `"#.#"` has an integer part, but `#` is optional and the F&O
        // adjustments leave the minimum at zero, so `$formatNumber(0.25,
        // "#.#")` is ".2" -- no leading zero. `"#"` alone is different: the
        // adjustment raises its minimum to 1, so it keeps the "0".
        if integer_str == zero_digit.to_string() && parts.min_integer_digits == 0 {
            integer_str.clear();
        }
        if integer_str.is_empty() && parts.min_integer_digits > 0 {
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
        // The separator is shown only when there are fractional digits to
        // show -- not merely because the picture contained one.
        let mut result = if !fractional_str.is_empty() {
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
        } else if result.len() < 2 {
            // Nothing to compare, so the element type does not matter:
            // `$sort(true)` is `[true]`. The signature's `a` type wraps a
            // scalar into a singleton before we get here.
        } else {
            return Err(FunctionError::TypeError(
                "D3070: The single argument form of the sort function can only be applied to an \
                 array of strings or an array of numbers.  Use the second argument to specify a \
                 comparison function"
                    .to_string(),
            ));
        }

        Ok(JValue::array(result))
    }

    /// $distinct(array) - Get unique elements
    pub fn distinct(arr: &[JValue]) -> Result<JValue, FunctionError> {
        let mut result = Vec::new();
        let mut seen: Vec<JValue> = Vec::new();

        for value in arr {
            // Materialize a lazy element before comparing so an unconvertible
            // Python value raises `TypeError` here instead of being silently
            // treated as "always distinct from everything else" -- `values_equal`'s
            // lazy arms return `false` (not-equal) on conversion failure, by
            // design (see its doc comment), which would let two references to
            // the very same unconvertible value (e.g. two aliases of one
            // Python `set`-valued field) both survive dedup.
            #[cfg(feature = "python")]
            let compare_value = match value {
                JValue::LazyPyDict(lazy) => JValue::Object(
                    lazy.to_object()
                        .map_err(|e| FunctionError::PyConversionError(e.0))?,
                ),
                other => other.clone(),
            };
            #[cfg(not(feature = "python"))]
            let compare_value = value.clone();

            let mut is_new = true;
            for seen_value in &seen {
                if values_equal(&compare_value, seen_value) {
                    is_new = false;
                    break;
                }
            }
            if is_new {
                seen.push(compare_value);
                result.push(value.clone());
            }
        }

        Ok(JValue::array(result))
    }

    /// $exists(value) - Check if value exists (not null/undefined)
    pub fn exists(value: &JValue) -> Result<JValue, FunctionError> {
        // Only a *missing* value is absent. An explicit null exists.
        let is_missing = value.is_undefined();
        Ok(JValue::Bool(!is_missing))
    }

    /// Compare two JSON values for deep equality (JSONata semantics)
    ///
    /// Cannot return `Result`, so a lazy-conversion failure in the `LazyPyDict` arms
    /// below is swallowed and yields `false` (not-equal), by design -- callers that need
    /// the failure to surface as a Python `TypeError` (the `=`/`!=`/`in` operators) must
    /// call `evaluator::normalize_lazy` on their operands *before* calling this function.
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
            #[cfg(feature = "python")]
            (JValue::LazyPyDict(x), JValue::Object(bm)) => x.to_object_ref().is_some_and(|am| {
                am.len() == bm.len()
                    && am
                        .iter()
                        .all(|(k, v)| bm.get(k).is_some_and(|v2| values_equal(v, v2)))
            }),
            #[cfg(feature = "python")]
            (JValue::Object(am), JValue::LazyPyDict(y)) => y.to_object_ref().is_some_and(|bm| {
                am.len() == bm.len()
                    && am
                        .iter()
                        .all(|(k, v)| bm.get(k).is_some_and(|v2| values_equal(v, v2)))
            }),
            #[cfg(feature = "python")]
            (JValue::LazyPyDict(x), JValue::LazyPyDict(y)) => {
                // Pointer-identity fast path (mirrors `PartialEq for JValue`): the same
                // wrapped Python dict is trivially equal to itself without touching any
                // (possibly unconvertible) field content.
                std::rc::Rc::ptr_eq(x, y)
                    || x.same_object(y)
                    || x.to_object_ref().is_some_and(|am| {
                        y.to_object_ref().is_some_and(|bm| {
                            am.len() == bm.len()
                                && am
                                    .iter()
                                    .all(|(k, v)| bm.get(k).is_some_and(|v2| values_equal(v, v2)))
                        })
                    })
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

        let mut result = arr.to_vec();
        let mut rng = rand::rng();
        result.shuffle(&mut rng);

        Ok(JValue::array(result))
    }
}

/// Built-in object functions
pub mod object {
    use super::*;

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
            #[cfg(feature = "python")]
            if let JValue::LazyPyDict(lazy) = obj {
                match lazy.to_object_ref() {
                    Some(map) => {
                        for (k, v) in map.iter() {
                            result.insert(k.clone(), v.clone());
                        }
                        continue;
                    }
                    None => {
                        return Err(FunctionError::TypeError(
                            "merge() argument could not be converted".to_string(),
                        ))
                    }
                }
            }
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
    /// Decode base64 the way Node's `Buffer.from(str, 'base64')` does, which
    /// is what jsonata-js delegates to.
    ///
    /// Node never rejects input. It ignores every character outside the
    /// alphabet, stops at the first padding character, accepts the URL-safe
    /// alphabet alongside the standard one, and drops an incomplete trailing
    /// quantum instead of erroring -- so `$base64decode("a")` is "" and
    /// `$base64decode("YQ")` is "a". A strict decoder rejects all of those.
    fn lenient_base64_bytes(s: &str) -> Vec<u8> {
        let sextet = |c: char| -> Option<u8> {
            match c {
                'A'..='Z' => Some(c as u8 - b'A'),
                'a'..='z' => Some(c as u8 - b'a' + 26),
                '0'..='9' => Some(c as u8 - b'0' + 52),
                '+' | '-' => Some(62),
                '/' | '_' => Some(63),
                _ => None,
            }
        };

        let mut out = Vec::new();
        let mut acc: u32 = 0;
        let mut bits = 0u32;
        for c in s.chars() {
            if c == '=' {
                break;
            }
            let Some(v) = sextet(c) else { continue };
            acc = (acc << 6) | u32::from(v);
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push(((acc >> bits) & 0xff) as u8);
            }
        }
        // Whatever is left is an incomplete byte and is discarded, which is
        // why a lone trailing character contributes nothing.
        out
    }

    pub fn base64encode(s: &str) -> Result<JValue, FunctionError> {
        let encoded = general_purpose::STANDARD.encode(s.as_bytes());
        Ok(JValue::string(encoded))
    }

    /// $base64decode(string) - Decode base64 string
    pub fn base64decode(s: &str) -> Result<JValue, FunctionError> {
        let bytes = lenient_base64_bytes(s);
        match String::from_utf8(bytes) {
            Ok(decoded) => Ok(JValue::string(decoded)),
            // The decoded bytes are not text. jsonata-js reads them as latin1
            // and always produces *a* string, which round-trips ASCII but
            // mangles everything else -- `$base64decode($base64encode("\u{1f642}"))`
            // is "=B" there. Raising instead is a deliberate divergence; see
            // issue #126 group 3.
            Err(_) => Err(FunctionError::RuntimeError(
                "Invalid UTF-8 in decoded base64".to_string(),
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
            // jsonata-js reports every malformed-URL failure as D3140, naming
            // the function and quoting the offending value.
            Err(_) => Err(FunctionError::RuntimeError(format!(
                "D3140: Malformed URL passed to ${}(): \"{}\"",
                "decodeUrlComponent", s
            ))),
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
            // jsonata-js reports every malformed-URL failure as D3140, naming
            // the function and quoting the offending value.
            Err(_) => Err(FunctionError::RuntimeError(format!(
                "D3140: Malformed URL passed to ${}(): \"{}\"",
                "decodeUrl", s
            ))),
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
        // An explicit null exists; only a *missing* value does not. Verified
        // against jsonata-js: `$exists(null)` is true, `$exists(nothing)` is
        // false. This asserted the opposite (#102).
        assert_eq!(array::exists(&JValue::Null).unwrap(), JValue::Bool(true));
        assert_eq!(
            array::exists(&JValue::Undefined).unwrap(),
            JValue::Bool(false)
        );
    }

    // ===== Object Functions Tests =====

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

    /// $base64 follows jsonata-js in what it *accepts*, not in how it reads
    /// the bytes. Deliberate, and the reasoning is worth keeping because
    /// jsonata's own documentation contradicts itself here.
    ///
    /// `functions.js` delegates to the platform: `window.btoa`/`window.atob`
    /// in a browser, and under Node `Buffer.from(str, 'binary')` /
    /// `.toString('binary')` -- chosen, per its own comment, only to emulate
    /// btoa/atob without pulling in the Buffer polyfill. So latin1 is the
    /// intent, not an accident of Node.
    ///
    /// The docs then say two different things (docs/string-functions.md):
    ///
    ///   $base64encode -- "Each character in the string is treated as a byte
    ///   of binary data. This requires that all characters in the string are
    ///   in the 0x00 to 0xFF range... Unicode characters outside of that range
    ///   are not supported."   -> latin1, out-of-range explicitly unsupported
    ///
    ///   $base64decode -- "Converts base 64 encoded bytes to a string, using a
    ///   UTF-8 Unicode codepage."                                   -> UTF-8
    ///
    /// The decode implementation does not do what the decode docs say. So
    /// there is no reading of the reference that is self-consistent, and
    /// UTF-8 on both sides is the half that matches a documented contract
    /// exactly while also round-tripping.
    ///
    /// Above 0xFF nothing is defined at all: `window.btoa("\u{1f642}")` throws
    /// InvalidCharacterError, while Node truncates each UTF-16 code unit to a
    /// byte and yields "PUI=", which decodes back to "=B". Browser and Node
    /// disagree, so there is no conformance target to hit.
    ///
    /// What this does cost: for 0x80..=0xFF, encode has a documented,
    /// environment-independent answer we do not give --
    /// `$base64encode("h\u{e9}llo")` is "aOlsbG8=" in jsonata-js and
    /// "aMOpbGxv" here. Matching it would require latin1 decode too (or the
    /// round-trip breaks), which would then contradict the decode docs.
    /// See #126 group 3. If this is ever revisited, start here.
    #[test]
    fn base64_round_trips_non_ascii_where_jsonata_js_does_not() {
        for original in [
            "a",
            "hello:world",
            "h\u{e9}llo",
            "\u{65e5}\u{672c}",
            "\u{1f642}",
        ] {
            let JValue::String(encoded) = encoding::base64encode(original).unwrap() else {
                panic!("expected a string");
            };
            let JValue::String(decoded) = encoding::base64decode(&encoded).unwrap() else {
                panic!("expected a string");
            };
            assert_eq!(&*decoded, original, "round-trip failed for {original:?}");
        }

        // The leniency half *is* matched: an incomplete trailing quantum is
        // dropped and characters outside the alphabet are ignored, rather than
        // rejected. See the corpus probes for the full family.
        for (input, want) in [("a", ""), ("YQ", "a"), ("!!!!", ""), ("YQ==YQ==", "a")] {
            let JValue::String(got) = encoding::base64decode(input).unwrap() else {
                panic!("expected a string");
            };
            assert_eq!(&*got, want, "lenient decode of {input:?}");
        }
    }
}

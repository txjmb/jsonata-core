// Function signature validation and type checking
// Mirrors signature.js from the reference implementation

use crate::value::JValue;
use regex::Regex;
use thiserror::Error;

/// Signature validation errors
#[derive(Error, Debug)]
pub enum SignatureError {
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),

    #[error("Argument count mismatch: expected {expected}, got {actual}")]
    ArgumentCountMismatch { expected: usize, actual: usize },

    #[error("T0410: Argument {index} must be {expected}")]
    ArgumentTypeMismatch { index: usize, expected: String },

    #[error("T0412: Argument {index} must be an array of {expected}")]
    ArrayTypeMismatch { index: usize, expected: String },

    #[error("T0411: Context value does not match function signature (expected {expected})")]
    ContextTypeMismatch { index: usize, expected: String },
}

/// Parameter type
#[derive(Debug, Clone, PartialEq)]
pub enum ParamType {
    String,
    Number,
    Boolean,
    Array(Option<Box<ParamType>>), // Array with optional element type
    Object,
    Function(Option<String>), // Function with optional signature subtype like "n:n"
    Any,
    /// `j` — any JSON type. Like `Any` but excludes functions, matching
    /// jsonata-js's `case 'j'` regex `[asnblom]`.
    Json,
    Null,
    Union(Vec<ParamType>), // Union type like (ns) = number or string
}

impl ParamType {
    /// Parse a single type character
    fn from_char(c: char) -> Option<Self> {
        match c {
            's' => Some(ParamType::String),
            'n' => Some(ParamType::Number),
            'b' => Some(ParamType::Boolean),
            'a' => Some(ParamType::Array(None)),
            'o' => Some(ParamType::Object),
            'f' => Some(ParamType::Function(None)),
            'x' => Some(ParamType::Any),
            'j' => Some(ParamType::Json),
            'l' => Some(ParamType::Null),
            _ => None,
        }
    }

    /// Check if a value matches this type
    pub fn matches(&self, value: &JValue) -> bool {
        match (self, value) {
            (ParamType::Any, _) => true,
            (ParamType::Null, JValue::Null) => true,
            (ParamType::String, JValue::String(_)) => true,
            (ParamType::Number, JValue::Number(_)) => true,
            (ParamType::Boolean, JValue::Bool(_)) => true,
            (ParamType::Object, JValue::Object(_)) => true,
            #[cfg(feature = "python")]
            (ParamType::Object, JValue::LazyPyDict(_)) => true,
            (ParamType::Function(_), JValue::Lambda { .. })
            | (ParamType::Function(_), JValue::Builtin { .. }) => true,
            (ParamType::Array(elem_type), JValue::Array(arr)) => {
                if let Some(expected_elem) = elem_type {
                    // Check all elements match the expected type
                    arr.iter().all(|v| expected_elem.matches(v))
                } else {
                    // Any array
                    true
                }
            }
            (ParamType::Union(types), _) => {
                // Union type matches if value matches any of the types
                types.iter().any(|t| t.matches(value))
            }
            _ => false,
        }
    }
}

/// Map a ParamType back to its single-character signature symbol.
/// Used to rebuild a union type's regex character class from its parsed
/// component types.
fn type_char(t: &ParamType) -> char {
    match t {
        ParamType::String => 's',
        ParamType::Number => 'n',
        ParamType::Boolean => 'b',
        ParamType::Null => 'l',
        ParamType::Object => 'o',
        ParamType::Array(_) => 'a',
        ParamType::Function(_) => 'f',
        ParamType::Any => 'x',
        ParamType::Json => 'j',
        // Unreachable in practice: signature.js does not nest unions inside
        // unions, and our parser never constructs one this way either.
        ParamType::Union(_) => 'x',
    }
}

/// Get the single-character type symbol for a value, mirroring signature.js's
/// getSymbol(): used to build the "supplied signature" string that gets
/// matched against a Signature's compiled regex.
fn type_symbol(value: &JValue) -> char {
    match value {
        JValue::Null => 'l',
        JValue::Bool(_) => 'b',
        JValue::Number(_) => 'n',
        JValue::String(_) => 's',
        JValue::Array(_) => 'a',
        JValue::Object(_) => 'o',
        JValue::Undefined => 'm',
        JValue::Lambda { .. } => 'f',
        JValue::Builtin { .. } => 'f',
        // A regex is a *function* in jsonata-js (a regex literal evaluates to
        // one), so it matches the `f` half of unions like `(sf)` in
        // `$match`/`$replace`/`$split`/`$contains`.
        JValue::Regex { .. } => 'f',
        #[cfg(feature = "python")]
        JValue::LazyPyDict(_) => 'o',
    }
}

/// Function parameter definition
#[derive(Debug, Clone)]
pub struct Parameter {
    pub param_type: ParamType,
    pub optional: bool,
    /// Regex fragment for this parameter, e.g. "[nm]", "[nm]+", "[sm]?".
    /// Combined across all params to build a Signature's full_regex.
    regex: String,
    /// True if this parameter was declared with the '+' (one-or-more) modifier.
    repeatable: bool,
    /// True if this parameter was declared with the '-' modifier: when the
    /// caller omits this argument, substitute the evaluation context value
    /// instead (if its type is compatible).
    context: bool,
    /// Regex fragment (without the '-'-induced trailing '?') used to test
    /// whether the context value's type is compatible, when `context` is true.
    context_regex: Option<String>,
}

impl Parameter {
    /// Construct a Parameter directly (not via signature-string parsing).
    /// Used by tests and by Signature::new. Produces a non-repeatable,
    /// non-context parameter with the standard base regex for its type.
    #[allow(dead_code)]
    pub fn new(param_type: ParamType, optional: bool) -> Self {
        let mut regex = Self::base_regex(&param_type);
        if optional {
            regex.push('?');
        }
        Parameter {
            param_type,
            optional,
            regex,
            repeatable: false,
            context: false,
            context_regex: None,
        }
    }

    /// The base (unquantified) regex character class for a parameter type,
    /// mirroring signature.js's per-symbol regex assignment.
    fn base_regex(param_type: &ParamType) -> String {
        match param_type {
            ParamType::Array(_) => "[asnblfom]".to_string(),
            ParamType::Function(_) => "f".to_string(),
            ParamType::Any => "[asnblfom]".to_string(),
            // Any JSON type: everything except a function.
            ParamType::Json => "[asnblom]".to_string(),
            ParamType::String => "[sm]".to_string(),
            ParamType::Number => "[nm]".to_string(),
            ParamType::Boolean => "[bm]".to_string(),
            ParamType::Null => "[lm]".to_string(),
            ParamType::Object => "[om]".to_string(),
            ParamType::Union(types) => {
                let chars: String = types.iter().map(type_char).collect();
                format!("[{}m]", chars)
            }
        }
    }
}

/// Function signature
#[derive(Debug, Clone)]
pub struct Signature {
    pub params: Vec<Parameter>,
    #[allow(dead_code)]
    pub return_type: Option<ParamType>,
    /// The compiled regex matching this signature's full parameter list
    /// against a "supplied signature" type-symbol string, e.g. "^([nm]+)([nm])$".
    full_regex: Regex,
}

impl Signature {
    /// Create a new signature
    #[allow(dead_code)]
    pub fn new(params: Vec<Parameter>, return_type: Option<ParamType>) -> Self {
        let full_regex = Self::compile_full_regex(&params);
        Signature {
            params,
            return_type,
            full_regex,
        }
    }

    /// Build the anchored whole-signature regex from each parameter's fragment.
    fn compile_full_regex(params: &[Parameter]) -> Regex {
        let pattern: String = std::iter::once("^".to_string())
            .chain(params.iter().map(|p| format!("({})", p.regex)))
            .chain(std::iter::once("$".to_string()))
            .collect();
        Regex::new(&pattern)
            .unwrap_or_else(|e| panic!("generated signature regex `{}` is invalid: {}", pattern, e))
    }

    /// Parse a signature string like "<n-n:n>" or "<s?:b>"
    pub fn parse(sig_str: &str) -> Result<Self, SignatureError> {
        let sig_str = sig_str.trim();

        // Signature format: <params:return>
        if !sig_str.starts_with('<') || !sig_str.ends_with('>') {
            return Err(SignatureError::InvalidSignature(
                "Signature must be enclosed in angle brackets".to_string(),
            ));
        }

        let inner = &sig_str[1..sig_str.len() - 1];

        // Find the separator colon, skipping over any nested angle brackets
        // This handles cases like <f<n:n>:f<n:n>> where the first : is inside <n:n>
        let separator_pos = Self::find_separator_colon(inner);

        let (param_str, return_type_str) = if let Some(pos) = separator_pos {
            (&inner[..pos], Some(&inner[pos + 1..]))
        } else {
            (inner, None)
        };

        let return_type = if let Some(rt_str) = return_type_str {
            Some(Self::parse_type(rt_str)?)
        } else {
            None
        };

        // Parse parameters (separated by -)
        let params = if param_str.is_empty() {
            Vec::new()
        } else {
            Self::parse_params(param_str)?
        };

        let full_regex = Self::compile_full_regex(&params);

        Ok(Signature {
            params,
            return_type,
            full_regex,
        })
    }

    /// Find the separator colon that divides params from return type,
    /// skipping over colons that are inside nested angle brackets
    fn find_separator_colon(s: &str) -> Option<usize> {
        let mut depth = 0;
        for (i, c) in s.chars().enumerate() {
            match c {
                '<' => depth += 1,
                '>' => depth -= 1,
                ':' if depth == 0 => return Some(i),
                _ => {}
            }
        }
        None
    }

    /// Parse parameter types from string like "n-n" or "a<s>s?" or "n+n"
    fn parse_params(param_str: &str) -> Result<Vec<Parameter>, SignatureError> {
        let mut params = Vec::new();
        let mut chars = param_str.chars().peekable();

        while chars.peek().is_some() {
            let param_type = Self::parse_type_chars(&mut chars)?;
            let mut regex = Parameter::base_regex(&param_type);
            let mut optional = false;
            let mut repeatable = false;
            let mut context = false;
            let mut context_regex = None;

            match chars.peek() {
                Some('?') => {
                    chars.next();
                    regex.push('?');
                    optional = true;
                }
                Some('+') => {
                    chars.next();
                    regex.push('+');
                    repeatable = true;
                }
                Some('-') => {
                    chars.next();
                    context = true;
                    context_regex = Some(regex.clone());
                    regex.push('?');
                    optional = true;
                }
                _ => {}
            }

            params.push(Parameter {
                param_type,
                optional,
                regex,
                repeatable,
                context,
                context_regex,
            });
        }

        Ok(params)
    }

    /// Parse a type from characters
    fn parse_type_chars(
        chars: &mut std::iter::Peekable<std::str::Chars>,
    ) -> Result<ParamType, SignatureError> {
        // Check for union type: (ns) or (nsb)
        if chars.peek() == Some(&'(') {
            chars.next(); // consume '('
            let mut union_types = Vec::new();

            // Parse all types until we hit ')'
            while chars.peek() != Some(&')') && chars.peek().is_some() {
                let type_char = chars.next().ok_or_else(|| {
                    SignatureError::InvalidSignature("Unexpected end in union type".to_string())
                })?;

                let param_type = ParamType::from_char(type_char).ok_or_else(|| {
                    SignatureError::InvalidSignature(format!(
                        "Invalid type character in union: {}",
                        type_char
                    ))
                })?;

                union_types.push(param_type);
            }

            if chars.next() != Some(')') {
                return Err(SignatureError::InvalidSignature(
                    "Expected ')' after union type".to_string(),
                ));
            }

            return Ok(ParamType::Union(union_types));
        }

        let type_char = chars.next().ok_or_else(|| {
            SignatureError::InvalidSignature("Unexpected end of signature".to_string())
        })?;

        let mut param_type = ParamType::from_char(type_char).ok_or_else(|| {
            SignatureError::InvalidSignature(format!("Invalid type character: {}", type_char))
        })?;

        // Check for subtype: a<s> for array elements, or f<n:n> for function signature
        if chars.peek() == Some(&'<') {
            match param_type {
                ParamType::Array(_) => {
                    chars.next(); // consume '<'
                    let elem_type = Self::parse_type_chars(chars)?;

                    if chars.next() != Some('>') {
                        return Err(SignatureError::InvalidSignature(
                            "Expected '>' after array element type".to_string(),
                        ));
                    }

                    param_type = ParamType::Array(Some(Box::new(elem_type)));
                }
                ParamType::Function(_) => {
                    // Function subtype like f<n:n> - parse the nested signature
                    chars.next(); // consume '<'
                    let mut subtype = String::new();
                    let mut depth = 1;

                    // Collect characters until matching '>'
                    while depth > 0 {
                        match chars.next() {
                            Some('<') => {
                                depth += 1;
                                subtype.push('<');
                            }
                            Some('>') => {
                                depth -= 1;
                                if depth > 0 {
                                    subtype.push('>');
                                }
                            }
                            Some(c) => subtype.push(c),
                            None => {
                                return Err(SignatureError::InvalidSignature(
                                    "Unexpected end in function subtype".to_string(),
                                ))
                            }
                        }
                    }

                    param_type = ParamType::Function(Some(subtype));
                }
                _ => {
                    // '<' not valid after other types
                    return Err(SignatureError::InvalidSignature(format!(
                        "Type parameter '<' not valid after type {:?}",
                        param_type
                    )));
                }
            }
        }

        Ok(param_type)
    }

    /// Parse a type from string
    fn parse_type(type_str: &str) -> Result<ParamType, SignatureError> {
        let mut chars = type_str.chars().peekable();
        Self::parse_type_chars(&mut chars)
    }

    /// Validate argument count
    pub fn validate_arg_count(&self, actual: usize) -> Result<(), SignatureError> {
        let required = self.params.iter().filter(|p| !p.optional).count();
        let unbounded = self.params.iter().any(|p| p.repeatable);
        let max = self.params.len();

        if actual < required || (!unbounded && actual > max) {
            return Err(SignatureError::ArgumentCountMismatch {
                expected: required,
                actual,
            });
        }

        Ok(())
    }

    /// Validate and coerce arguments according to signature rules.
    ///
    /// `context` is the JSONata evaluation context (`$`) at the point of the
    /// call, used for the '-' modifier's fallback-to-context behavior.
    /// Pass `&JValue::Undefined` if there is no meaningful context (e.g. a
    /// signature with no '-'-marked parameters never inspects it).
    ///
    /// Mirrors signature.js's regex-based validate(): build a one-char-per-
    /// argument type-symbol string, match it against this signature's
    /// compiled regex, then walk each parameter's captured group back to
    /// positional arguments (a captured group may span multiple characters
    /// when the parameter is repeatable with '+').
    pub fn validate_and_coerce(
        &self,
        args: &[JValue],
        context: &JValue,
    ) -> Result<Vec<JValue>, SignatureError> {
        self.validate_arg_count(args.len())?;

        let supplied_sig: String = args.iter().map(type_symbol).collect();

        let captures = match self.full_regex.captures(&supplied_sig) {
            Some(c) => c,
            None => return Err(self.arg_type_mismatch_error(&supplied_sig)),
        };

        let mut coerced_args = Vec::with_capacity(args.len());
        let mut arg_index = 0usize;

        for (i, param) in self.params.iter().enumerate() {
            let matched = captures.get(i + 1).map(|m| m.as_str()).unwrap_or("");

            if matched.is_empty() {
                let arg = args.get(arg_index).cloned().unwrap_or(JValue::Undefined);

                if param.context {
                    let context_symbol = type_symbol(context).to_string();
                    let context_regex_str = param.context_regex.as_deref().unwrap_or("");
                    let context_re = Regex::new(&format!("^{}$", context_regex_str))
                        .map_err(|e| SignatureError::InvalidSignature(e.to_string()))?;
                    if context_re.is_match(&context_symbol) {
                        coerced_args.push(context.clone());
                    } else {
                        return Err(SignatureError::ContextTypeMismatch {
                            index: arg_index + 1,
                            expected: Self::type_name(&param.param_type),
                        });
                    }
                } else {
                    // This position was genuinely supplied (not out-of-bounds
                    // padding) only if arg_index is within the original args.
                    // Nothing short-circuits here. Every parameter class
                    // includes `m`, so an undefined argument matches and is
                    // passed through for the function to interpret -- which is
                    // how jsonata-js gets `$count(missing)` = 0 and
                    // `$exists(missing)` = false from the same validator that
                    // rejects `$abs(null)`.
                    coerced_args.push(arg);
                    arg_index += 1;
                }
                continue;
            }

            for single in matched.chars() {
                let arg = args.get(arg_index).cloned().unwrap_or(JValue::Undefined);

                let resolved = if let ParamType::Array(elem_type) = &param.param_type {
                    if single == 'm' {
                        JValue::Undefined
                    } else if let JValue::Array(arr) = &arg {
                        if let Some(expected_elem) = elem_type {
                            if !arr.is_empty() && !arr.iter().all(|v| expected_elem.matches(v)) {
                                return Err(SignatureError::ArrayTypeMismatch {
                                    index: arg_index + 1,
                                    expected: Self::type_name(expected_elem),
                                });
                            }
                        }
                        arg.clone()
                    } else {
                        if let Some(expected_elem) = elem_type {
                            if !expected_elem.matches(&arg) {
                                return Err(SignatureError::ArrayTypeMismatch {
                                    index: arg_index + 1,
                                    expected: Self::type_name(expected_elem),
                                });
                            }
                        }
                        JValue::array(vec![arg.clone()])
                    }
                } else {
                    arg.clone()
                };

                coerced_args.push(resolved);
                arg_index += 1;
            }
        }

        Ok(coerced_args)
    }

    /// Build an ArgumentTypeMismatch error identifying roughly which argument
    /// broke validation, by matching a growing prefix of the joint pattern
    /// (mirrors signature.js's throwValidationError). Exact index parity with
    /// signature.js's backtracking behavior is not guaranteed in all cases —
    /// this is a best-effort diagnostic, not something any test asserts on
    /// precisely (the reference suite only checks error *codes*, not indices).
    fn arg_type_mismatch_error(&self, supplied_sig: &str) -> SignatureError {
        let mut good_to = 0usize;
        let mut partial_pattern = String::from("^");
        let mut last_param_type = ParamType::Any;

        for param in &self.params {
            partial_pattern.push_str(&param.regex);
            last_param_type = param.param_type.clone();
            match Regex::new(&partial_pattern) {
                Ok(re) => match re.find(supplied_sig) {
                    Some(m) if m.start() == 0 => good_to = m.end(),
                    _ => break,
                },
                Err(_) => break,
            }
        }

        SignatureError::ArgumentTypeMismatch {
            index: good_to + 1,
            expected: Self::type_name(&last_param_type),
        }
    }

    /// Get a human-readable name for a parameter type
    fn type_name(param_type: &ParamType) -> String {
        match param_type {
            ParamType::String => "String".to_string(),
            ParamType::Number => "Number".to_string(),
            ParamType::Boolean => "Boolean".to_string(),
            ParamType::Array(None) => "Array".to_string(),
            ParamType::Array(Some(elem)) => format!("Array of {}", Self::type_name(elem)),
            ParamType::Object => "Object".to_string(),
            ParamType::Function(None) => "Function".to_string(),
            ParamType::Function(Some(sig)) => format!("Function<{}>", sig),
            ParamType::Any => "Any".to_string(),
            ParamType::Json => "JSON value".to_string(),
            ParamType::Null => "Null".to_string(),
            ParamType::Union(types) => {
                let names: Vec<_> = types.iter().map(Self::type_name).collect();
                format!("({})", names.join(" or "))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signature_validation() {
        let sig = Signature::new(
            vec![
                Parameter::new(ParamType::String, false),
                Parameter::new(ParamType::Number, true),
            ],
            Some(ParamType::String),
        );

        // Valid: 1 required arg provided
        assert!(sig.validate_arg_count(1).is_ok());

        // Valid: both args provided
        assert!(sig.validate_arg_count(2).is_ok());

        // Invalid: too few args
        assert!(sig.validate_arg_count(0).is_err());

        // Invalid: too many args
        assert!(sig.validate_arg_count(3).is_err());
    }

    #[test]
    fn test_parse_signature_with_repeat_modifier() {
        // "<n+n:o>" must parse into exactly 2 params: a repeatable number,
        // then a required number.
        let sig = Signature::parse("<n+n:o>").expect("valid signature");
        assert_eq!(sig.params.len(), 2);
        assert_eq!(sig.params[0].param_type, ParamType::Number);
        assert!(!sig.params[0].optional);
        assert_eq!(sig.params[1].param_type, ParamType::Number);
        assert!(!sig.params[1].optional);
    }

    #[test]
    fn test_repeat_param_allows_more_args_than_declared_slots() {
        // "<n+n>" (2 type-slots, one repeatable) must accept 3 args, whereas
        // "<nn>" (2 required, non-repeatable slots) must reject 3 args.
        let repeating = Signature::parse("<n+n:o>").expect("valid signature");
        assert!(repeating.validate_arg_count(3).is_ok());

        let non_repeating = Signature::parse("<nn:o>").expect("valid signature");
        assert!(non_repeating.validate_arg_count(3).is_err());
    }

    #[test]
    fn test_repeat_param_coerces_all_matched_args() {
        // <n+n:o> called with (1, 2, 3): the repeat consumes 2 numbers, the
        // final required slot consumes the 3rd. All 3 must appear in order.
        let sig = Signature::parse("<n+n:o>").expect("valid signature");
        let args = vec![
            JValue::Number(1.0),
            JValue::Number(2.0),
            JValue::Number(3.0),
        ];
        let coerced = sig
            .validate_and_coerce(&args, &JValue::Undefined)
            .expect("should validate");
        assert_eq!(coerced, args);
    }

    #[test]
    fn test_repeat_param_rejects_wrong_type_within_repeat() {
        // <n+> with (1, 2, "x"): the 3rd arg breaks the all-numbers repeat,
        // and there's nothing else in the signature to absorb a string, so
        // the whole match fails -> T0410-class error (ArgumentTypeMismatch).
        let sig = Signature::parse("<n+:o>").expect("valid signature");
        let args = vec![
            JValue::Number(1.0),
            JValue::Number(2.0),
            JValue::string("x"),
        ];
        let result = sig.validate_and_coerce(&args, &JValue::Undefined);
        assert!(
            matches!(result, Err(SignatureError::ArgumentTypeMismatch { .. })),
            "expected ArgumentTypeMismatch, got {:?}",
            result
        );
    }

    #[test]
    fn test_context_substitution_success() {
        // <n+s-:a<n>> called with (1, 2) and a string context: the omitted
        // 3rd (context-fallback) argument should be filled from the context.
        let sig = Signature::parse("<n+s-:a<n>>").expect("valid signature");
        let args = vec![JValue::Number(1.0), JValue::Number(2.0)];
        let context = JValue::string("b");
        let coerced = sig
            .validate_and_coerce(&args, &context)
            .expect("should validate using context fallback");
        assert_eq!(
            coerced,
            vec![
                JValue::Number(1.0),
                JValue::Number(2.0),
                JValue::string("b")
            ]
        );
    }

    #[test]
    fn test_context_substitution_type_mismatch() {
        // <s-:s> called with 0 args and a NUMBER context: the context type
        // doesn't match the expected string type -> distinct T0411-class error.
        let sig = Signature::parse("<s-:s>").expect("valid signature");
        let args: Vec<JValue> = vec![];
        let context = JValue::Number(42.0);
        let result = sig.validate_and_coerce(&args, &context);
        assert!(
            matches!(result, Err(SignatureError::ContextTypeMismatch { .. })),
            "expected ContextTypeMismatch, got {:?}",
            result
        );
    }

    #[test]
    fn test_array_subtype_mismatch_within_repeat() {
        // <a<n>+:o> called with an array containing a non-number element:
        // the repeat's array-subtype check must still fire (T0412-class).
        let sig = Signature::parse("<a<n>+:o>").expect("valid signature");
        let bad_array = JValue::array(vec![JValue::string("x")]);
        let args = vec![bad_array];
        let result = sig.validate_and_coerce(&args, &JValue::Undefined);
        assert!(
            matches!(result, Err(SignatureError::ArrayTypeMismatch { .. })),
            "expected ArrayTypeMismatch, got {:?}",
            result
        );
    }

    #[test]
    fn test_repeat_with_leading_optional_does_not_spuriously_error() {
        // <s?n+:a<n>> called with (1, 2, 3): the optional leading string slot
        // matches zero characters (none of the 3 args is a string) and gets
        // "phantom-assigned" args[0] per signature.js's own algorithm, while
        // the repeat consumes args[1] and args[2] AND reads one position past
        // the end of `args` (args[3], which doesn't exist). That out-of-bounds
        // read must resolve to JValue::Undefined WITHOUT tripping the
        // "explicit null/undefined for a non-nullable required type" error,
        // since it was never actually supplied by the caller.
        let sig = Signature::parse("<s?n+:a<n>>").expect("valid signature");
        let args = vec![
            JValue::Number(1.0),
            JValue::Number(2.0),
            JValue::Number(3.0),
        ];
        let coerced = sig
            .validate_and_coerce(&args, &JValue::Undefined)
            .expect("must not error on out-of-bounds repeat padding");
        // Only the first 2 entries matter to callers (lambda binding only
        // uses as many entries as there are declared params), but the full
        // vector must not be an error.
        assert_eq!(coerced[0], JValue::Number(1.0));
        assert_eq!(coerced[1], JValue::Number(2.0));
    }
}

// ── Builtin signature table ─────────────────────────────────────────────────

/// jsonata-js's signature for every built-in function, lifted verbatim from its
/// `staticFrame.bind("name", defineFunction(fn.name, "<sig>"))` declarations.
///
/// These drive the same validation and coercion the reference performs, which is
/// where a lot of our null/undefined and singleton-coercion behaviour is
/// specified rather than in the function bodies:
///
/// - `a` "normally treats any value as a singleton array" (the reference's own
///   comment), so `$reverse(1)` is `[1]` and `$count(null)` is 1.
/// - `l` is null and `m` is missing, and they are distinct: `$abs(null)` fails
///   the `n` check with T0410, while `$abs(missing.x)` passes as `m` and the
///   function returns undefined.
///
/// `random` and `shuffle` are here for completeness even though their results
/// cannot be compared against the reference.
pub(crate) const BUILTIN_SIGNATURES: &[(&str, &str)] = &[
    ("sum", "<a<n>:n>"),
    ("count", "<a:n>"),
    ("max", "<a<n>:n>"),
    ("min", "<a<n>:n>"),
    ("average", "<a<n>:n>"),
    ("string", "<x-b?:s>"),
    ("substring", "<s-nn?:s>"),
    ("substringBefore", "<s-s:s>"),
    ("substringAfter", "<s-s:s>"),
    ("lowercase", "<s-:s>"),
    ("uppercase", "<s-:s>"),
    ("length", "<s-:n>"),
    ("trim", "<s-:s>"),
    ("pad", "<s-ns?:s>"),
    ("match", "<s-f<s:o>n?:a<o>>"),
    ("contains", "<s-(sf):b>"),
    ("replace", "<s-(sf)(sf)n?:s>"),
    ("split", "<s-(sf)n?:a<s>>"),
    ("join", "<a<s>s?:s>"),
    ("formatNumber", "<n-so?:s>"),
    ("formatBase", "<n-n?:s>"),
    ("number", "<(nsb)-:n>"),
    ("floor", "<n-:n>"),
    ("ceil", "<n-:n>"),
    ("round", "<n-n?:n>"),
    ("abs", "<n-:n>"),
    ("sqrt", "<n-:n>"),
    ("power", "<n-n:n>"),
    ("random", "<:n>"),
    ("boolean", "<x-:b>"),
    ("not", "<x-:b>"),
    ("map", "<af>"),
    ("zip", "<a+>"),
    ("filter", "<af>"),
    ("single", "<af?>"),
    ("reduce", "<afj?:j>"),
    ("sift", "<o-f?:o>"),
    ("keys", "<x-:a<s>>"),
    ("lookup", "<x-s:x>"),
    ("append", "<xx:a>"),
    ("exists", "<x:b>"),
    ("spread", "<x-:a<o>>"),
    ("merge", "<a<o>:o>"),
    ("reverse", "<a:a>"),
    ("each", "<o-f:a>"),
    ("error", "<s?:x>"),
    ("assert", "<bs?:x>"),
    ("type", "<x:s>"),
    ("sort", "<af?:a>"),
    ("shuffle", "<a:a>"),
    ("distinct", "<x:x>"),
    ("encodeUrlComponent", "<s-:s>"),
    ("encodeUrl", "<s-:s>"),
    ("decodeUrlComponent", "<s-:s>"),
    ("decodeUrl", "<s-:s>"),
];

/// Look up a builtin's parsed signature, or `None` if it has no declared one.
///
/// Signatures are parsed once on first use: parsing builds a regex, which is far
/// too expensive to repeat per call.
pub(crate) fn builtin_signature(name: &str) -> Option<&'static Signature> {
    static CACHE: std::sync::OnceLock<std::collections::HashMap<&'static str, Signature>> =
        std::sync::OnceLock::new();
    CACHE
        .get_or_init(|| {
            BUILTIN_SIGNATURES
                .iter()
                .filter_map(|(name, sig)| Signature::parse(sig).ok().map(|s| (*name, s)))
                .collect()
        })
        .get(name)
}

#[cfg(test)]
mod builtin_signature_table_tests {
    use super::*;

    #[test]
    fn every_reference_signature_parses() {
        // A signature that fails to parse would silently drop out of the lookup
        // cache and leave that builtin unvalidated, so assert on the table.
        let bad: Vec<String> = BUILTIN_SIGNATURES
            .iter()
            // Explicit format arguments rather than inline `{name}` captures:
            // CodeQL's Rust analysis does not see implicit captures and reports
            // the binding as unused.
            .filter_map(|(name, sig)| {
                Signature::parse(sig)
                    .err()
                    .map(|e| format!("{} {}: {}", name, sig, e))
            })
            .collect();
        assert!(
            bad.is_empty(),
            "{} of {} failed:\n{}",
            bad.len(),
            BUILTIN_SIGNATURES.len(),
            bad.join("\n")
        );
        assert_eq!(BUILTIN_SIGNATURES.len(), 55);
    }

    #[test]
    fn lookup_returns_parsed_signatures() {
        assert!(builtin_signature("count").is_some());
        assert!(builtin_signature("reverse").is_some());
        assert!(builtin_signature("nosuchfunction").is_none());
    }

    #[test]
    fn array_type_accepts_any_single_value() {
        // The property that makes `$reverse(1)` == [1] and `$count(null)` == 1.
        let sig = builtin_signature("count").expect("count has a signature");
        for arg in [
            JValue::Number(1.0),
            JValue::Null,
            JValue::Bool(true),
            JValue::string("s"),
        ] {
            assert!(
                sig.validate_and_coerce(std::slice::from_ref(&arg), &JValue::Undefined)
                    .is_ok(),
                "count should accept {arg:?} as a singleton"
            );
        }
    }

    #[test]
    fn null_and_missing_are_distinct() {
        // Both are rejected by `<n-:n>`, but for different reasons, and callers
        // depend on telling them apart: a null is a genuine type error (T0410),
        // while a missing argument is the signal to return undefined.
        let sig = builtin_signature("abs").expect("abs has a signature");
        assert!(matches!(
            sig.validate_and_coerce(&[JValue::Null], &JValue::Undefined),
            Err(SignatureError::ArgumentTypeMismatch { .. })
        ));
        // A missing argument matches `m` and passes straight through, leaving
        // the answer to the function: `$abs(missing)` is undefined,
        // `$count(missing)` is 0, `$exists(missing)` is false. The validator
        // does not and cannot decide that.
        assert_eq!(
            sig.validate_and_coerce(&[JValue::Undefined], &JValue::Undefined)
                .unwrap(),
            vec![JValue::Undefined]
        );
    }

    #[test]
    fn null_reaches_the_array_class_instead_of_short_circuiting() {
        // The counterpart: `a` accepts null and wraps it, so `$count(null)` is
        // 1 rather than 0. Before, null short-circuited as "undefined argument"
        // and never reached the regex at all.
        let sig = builtin_signature("count").expect("count has a signature");
        assert_eq!(
            sig.validate_and_coerce(&[JValue::Null], &JValue::Undefined)
                .unwrap(),
            vec![JValue::array(vec![JValue::Null])]
        );
    }
}

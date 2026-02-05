// Function signature validation and type checking
// Mirrors signature.js from the reference implementation

use serde_json::Value;
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

    #[error("Undefined argument")]
    UndefinedArgument,
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
            'l' => Some(ParamType::Null),
            _ => None,
        }
    }

    /// Check if a value matches this type
    pub fn matches(&self, value: &Value) -> bool {
        match (self, value) {
            (ParamType::Any, _) => true,
            (ParamType::Null, Value::Null) => true,
            (ParamType::String, Value::String(_)) => true,
            (ParamType::Number, Value::Number(_)) => true,
            (ParamType::Boolean, Value::Bool(_)) => true,
            (ParamType::Object, Value::Object(_)) => true,
            (ParamType::Function(_), Value::Object(map)) => {
                // Functions are represented as objects with special markers
                map.contains_key("__lambda__") || map.contains_key("__builtin__")
            }
            (ParamType::Array(elem_type), Value::Array(arr)) => {
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

    /// Check if this is a function type
    #[allow(dead_code)]
    pub fn is_function(&self) -> bool {
        matches!(self, ParamType::Function(_))
    }

    /// Check if this is an array type
    #[allow(dead_code)]
    pub fn is_array(&self) -> bool {
        matches!(self, ParamType::Array(_))
    }
}

/// Function parameter definition
#[derive(Debug, Clone)]
pub struct Parameter {
    pub param_type: ParamType,
    pub optional: bool,
}

/// Function signature
#[derive(Debug, Clone)]
pub struct Signature {
    pub params: Vec<Parameter>,
    #[allow(dead_code)]
    pub return_type: Option<ParamType>,
}

impl Signature {
    /// Create a new signature
    #[allow(dead_code)]
    pub fn new(params: Vec<Parameter>, return_type: Option<ParamType>) -> Self {
        Signature {
            params,
            return_type,
        }
    }

    /// Parse a signature string like "<n-n:n>" or "<s?:b>"
    pub fn parse(sig_str: &str) -> Result<Self, SignatureError> {
        let sig_str = sig_str.trim();

        // Signature format: <params:return>
        if !sig_str.starts_with('<') || !sig_str.ends_with('>') {
            return Err(SignatureError::InvalidSignature(
                "Signature must be enclosed in angle brackets".to_string()
            ));
        }

        let inner = &sig_str[1..sig_str.len()-1];

        // Find the separator colon, skipping over any nested angle brackets
        // This handles cases like <f<n:n>:f<n:n>> where the first : is inside <n:n>
        let separator_pos = Self::find_separator_colon(inner);

        let (param_str, return_type_str) = if let Some(pos) = separator_pos {
            (&inner[..pos], Some(&inner[pos+1..]))
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

        Ok(Signature { params, return_type })
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

    /// Parse parameter types from string like "n-n" or "a<s>s?"
    fn parse_params(param_str: &str) -> Result<Vec<Parameter>, SignatureError> {
        let mut params = Vec::new();
        let mut chars = param_str.chars().peekable();

        while chars.peek().is_some() {
            // Check for separator
            if chars.peek() == Some(&'-') {
                chars.next();
                continue;
            }

            let param_type = Self::parse_type_chars(&mut chars)?;

            // Check for optional marker
            let optional = if chars.peek() == Some(&'?') {
                chars.next();
                true
            } else {
                false
            };

            params.push(Parameter { param_type, optional });
        }

        Ok(params)
    }

    /// Parse a type from characters
    fn parse_type_chars(chars: &mut std::iter::Peekable<std::str::Chars>) -> Result<ParamType, SignatureError> {
        // Check for union type: (ns) or (nsb)
        if chars.peek() == Some(&'(') {
            chars.next(); // consume '('
            let mut union_types = Vec::new();

            // Parse all types until we hit ')'
            while chars.peek() != Some(&')') && chars.peek().is_some() {
                let type_char = chars.next()
                    .ok_or_else(|| SignatureError::InvalidSignature("Unexpected end in union type".to_string()))?;

                let param_type = ParamType::from_char(type_char)
                    .ok_or_else(|| SignatureError::InvalidSignature(format!("Invalid type character in union: {}", type_char)))?;

                union_types.push(param_type);
            }

            if chars.next() != Some(')') {
                return Err(SignatureError::InvalidSignature("Expected ')' after union type".to_string()));
            }

            return Ok(ParamType::Union(union_types));
        }

        let type_char = chars.next()
            .ok_or_else(|| SignatureError::InvalidSignature("Unexpected end of signature".to_string()))?;

        let mut param_type = ParamType::from_char(type_char)
            .ok_or_else(|| SignatureError::InvalidSignature(format!("Invalid type character: {}", type_char)))?;

        // Check for subtype: a<s> for array elements, or f<n:n> for function signature
        if chars.peek() == Some(&'<') {
            match param_type {
                ParamType::Array(_) => {
                    chars.next(); // consume '<'
                    let elem_type = Self::parse_type_chars(chars)?;

                    if chars.next() != Some('>') {
                        return Err(SignatureError::InvalidSignature("Expected '>' after array element type".to_string()));
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
                            None => return Err(SignatureError::InvalidSignature(
                                "Unexpected end in function subtype".to_string()
                            )),
                        }
                    }

                    param_type = ParamType::Function(Some(subtype));
                }
                _ => {
                    // '<' not valid after other types
                    return Err(SignatureError::InvalidSignature(
                        format!("Type parameter '<' not valid after type {:?}", param_type)
                    ));
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
        let max = self.params.len();

        if actual < required || actual > max {
            return Err(SignatureError::ArgumentCountMismatch {
                expected: required,
                actual,
            });
        }

        Ok(())
    }

    /// Validate argument types (non-coercing version for simple type checking)
    #[allow(dead_code)]
    pub fn validate_args(&self, args: &[Value]) -> Result<(), SignatureError> {
        self.validate_and_coerce(args).map(|_| ())
    }

    /// Validate and coerce arguments according to signature rules
    ///
    /// Like the JavaScript implementation, this:
    /// - Wraps non-array values in arrays when expecting array type
    /// - Checks array element types when specified
    /// - Returns the validated (and possibly coerced) arguments
    pub fn validate_and_coerce(&self, args: &[Value]) -> Result<Vec<Value>, SignatureError> {
        // Check argument count first
        self.validate_arg_count(args.len())?;

        let mut coerced_args = Vec::with_capacity(args.len());

        // Check and coerce each argument type
        for (i, (param, arg)) in self.params.iter().zip(args.iter()).enumerate() {
            // Special case: if argument is null (undefined), return UndefinedArgument
            // This allows the caller to decide whether to return undefined or error
            if matches!(arg, Value::Null) && !matches!(param.param_type, ParamType::Null | ParamType::Any) {
                return Err(SignatureError::UndefinedArgument);
            }

            // Handle array coercion: any value can be coerced to an array
            if let ParamType::Array(elem_type) = &param.param_type {
                let arr = if let Value::Array(arr) = arg {
                    // Already an array - check element types if specified
                    if let Some(expected_elem) = elem_type {
                        if !arr.is_empty() && !arr.iter().all(|v| expected_elem.matches(v)) {
                            return Err(SignatureError::ArrayTypeMismatch {
                                index: i + 1,
                                expected: Self::type_name(expected_elem),
                            });
                        }
                    }
                    arg.clone()
                } else {
                    // Non-array value - coerce by wrapping in array
                    // But first check if the element type matches
                    if let Some(expected_elem) = elem_type {
                        if !expected_elem.matches(arg) {
                            return Err(SignatureError::ArrayTypeMismatch {
                                index: i + 1,
                                expected: Self::type_name(expected_elem),
                            });
                        }
                    }
                    // Wrap the value in an array
                    Value::Array(vec![arg.clone()])
                };
                coerced_args.push(arr);
                continue;
            }

            // Standard type checking for non-array types
            if !param.param_type.matches(arg) {
                return Err(SignatureError::ArgumentTypeMismatch {
                    index: i + 1,
                    expected: Self::type_name(&param.param_type),
                });
            }
            coerced_args.push(arg.clone());
        }

        Ok(coerced_args)
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
            ParamType::Null => "Null".to_string(),
            ParamType::Union(types) => {
                let names: Vec<_> = types.iter().map(|t| Self::type_name(t)).collect();
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
                Parameter {
                    param_type: ParamType::String,
                    optional: false,
                },
                Parameter {
                    param_type: ParamType::Number,
                    optional: true,
                },
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
}

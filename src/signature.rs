// Function signature validation and type checking
// Mirrors signature.js from the reference implementation

use thiserror::Error;

/// Signature validation errors
#[derive(Error, Debug)]
pub enum SignatureError {
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),

    #[error("Argument count mismatch: expected {expected}, got {actual}")]
    ArgumentCountMismatch { expected: usize, actual: usize },

    #[error("Type mismatch: {0}")]
    TypeMismatch(String),
}

/// Parameter type
#[derive(Debug, Clone, PartialEq)]
pub enum ParamType {
    String,
    Number,
    Boolean,
    Array,
    Object,
    Function,
    Any,
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
    pub return_type: ParamType,
}

impl Signature {
    /// Create a new signature
    pub fn new(params: Vec<Parameter>, return_type: ParamType) -> Self {
        Signature {
            params,
            return_type,
        }
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
            ParamType::String,
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

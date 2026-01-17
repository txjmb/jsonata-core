// Expression evaluator
// Mirrors jsonata.js from the reference implementation

use crate::ast::AstNode;
use serde_json::Value;
use thiserror::Error;

/// Evaluator errors
#[derive(Error, Debug)]
pub enum EvaluatorError {
    #[error("Type error: {0}")]
    TypeError(String),

    #[error("Reference error: {0}")]
    ReferenceError(String),

    #[error("Evaluation error: {0}")]
    EvaluationError(String),
}

/// Evaluation context
///
/// Holds variable bindings and other state needed during evaluation
pub struct Context {
    bindings: std::collections::HashMap<String, Value>,
}

impl Context {
    pub fn new() -> Self {
        Context {
            bindings: std::collections::HashMap::new(),
        }
    }

    pub fn bind(&mut self, name: String, value: Value) {
        self.bindings.insert(name, value);
    }

    pub fn lookup(&self, name: &str) -> Option<&Value> {
        self.bindings.get(name)
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

/// Evaluator for JSONata expressions
pub struct Evaluator {
    context: Context,
}

impl Evaluator {
    pub fn new() -> Self {
        Evaluator {
            context: Context::new(),
        }
    }

    pub fn with_context(context: Context) -> Self {
        Evaluator { context }
    }

    /// Evaluate an AST node against data
    pub fn evaluate(&mut self, node: &AstNode, data: &Value) -> Result<Value, EvaluatorError> {
        match node {
            AstNode::String(s) => Ok(Value::String(s.clone())),
            AstNode::Number(n) => Ok(serde_json::json!(n)),
            AstNode::Boolean(b) => Ok(Value::Bool(*b)),
            AstNode::Null => Ok(Value::Null),

            // TODO: Implement remaining node types
            _ => Err(EvaluatorError::EvaluationError(
                "Node type not yet implemented".to_string(),
            )),
        }
    }
}

impl Default for Evaluator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evaluate_literals() {
        let mut evaluator = Evaluator::new();
        let data = Value::Null;

        // String literal
        let result = evaluator.evaluate(&AstNode::string("hello"), &data).unwrap();
        assert_eq!(result, Value::String("hello".to_string()));

        // Number literal
        let result = evaluator.evaluate(&AstNode::number(42.0), &data).unwrap();
        assert_eq!(result, serde_json::json!(42.0));

        // Boolean literal
        let result = evaluator.evaluate(&AstNode::boolean(true), &data).unwrap();
        assert_eq!(result, Value::Bool(true));

        // Null literal
        let result = evaluator.evaluate(&AstNode::null(), &data).unwrap();
        assert_eq!(result, Value::Null);
    }
}

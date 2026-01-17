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
            // === Literals ===
            AstNode::String(s) => Ok(Value::String(s.clone())),
            AstNode::Number(n) => Ok(serde_json::json!(n)),
            AstNode::Boolean(b) => Ok(Value::Bool(*b)),
            AstNode::Null => Ok(Value::Null),

            // === Variables ===
            AstNode::Variable(name) => {
                // Look up variable in context
                self.context
                    .lookup(name)
                    .cloned()
                    .ok_or_else(|| {
                        EvaluatorError::ReferenceError(format!("Undefined variable: ${}", name))
                    })
            }

            // === Path Expressions ===
            AstNode::Path { steps } => self.evaluate_path(steps, data),

            // === Binary Operations ===
            AstNode::Binary { op, lhs, rhs } => {
                self.evaluate_binary_op(*op, lhs, rhs, data)
            }

            // === Unary Operations ===
            AstNode::Unary { op, operand } => {
                self.evaluate_unary_op(*op, operand, data)
            }

            // === Arrays ===
            AstNode::Array(elements) => {
                let mut result = Vec::new();
                for element in elements {
                    let value = self.evaluate(element, data)?;
                    result.push(value);
                }
                Ok(Value::Array(result))
            }

            // === Objects ===
            AstNode::Object(pairs) => {
                let mut result = serde_json::Map::new();
                for (key_node, value_node) in pairs {
                    // Evaluate key (must be a string)
                    let key = match self.evaluate(key_node, data)? {
                        Value::String(s) => s,
                        other => {
                            return Err(EvaluatorError::TypeError(format!(
                                "Object key must be a string, got: {:?}",
                                other
                            )))
                        }
                    };
                    // Evaluate value
                    let value = self.evaluate(value_node, data)?;
                    result.insert(key, value);
                }
                Ok(Value::Object(result))
            }

            // === Function Calls ===
            AstNode::Function { name, args } => {
                self.evaluate_function_call(name, args, data)
            }

            // === Conditional Expressions ===
            AstNode::Conditional {
                condition,
                then_branch,
                else_branch,
            } => {
                let condition_value = self.evaluate(condition, data)?;
                if self.is_truthy(&condition_value) {
                    self.evaluate(then_branch, data)
                } else if let Some(else_branch) = else_branch {
                    self.evaluate(else_branch, data)
                } else {
                    Ok(Value::Null)
                }
            }

            // === Block Expressions ===
            AstNode::Block(expressions) => {
                let mut result = Value::Null;
                for expr in expressions {
                    result = self.evaluate(expr, data)?;
                }
                Ok(result)
            }

            // === Lambda Functions ===
            AstNode::Lambda { params, body } => {
                // For now, return a placeholder
                // Full implementation would store the lambda for later execution
                Err(EvaluatorError::EvaluationError(
                    "Lambda functions not yet fully implemented".to_string(),
                ))
            }
        }
    }

    /// Evaluate a path expression (e.g., foo.bar.baz)
    fn evaluate_path(&mut self, steps: &[AstNode], data: &Value) -> Result<Value, EvaluatorError> {
        let mut current = data.clone();

        for step in steps {
            current = match step {
                AstNode::String(field_name) => {
                    // Navigate into object field
                    match &current {
                        Value::Object(obj) => {
                            obj.get(field_name).cloned().unwrap_or(Value::Null)
                        }
                        Value::Null => Value::Null,
                        _ => {
                            return Err(EvaluatorError::TypeError(format!(
                                "Cannot access field '{}' on non-object: {:?}",
                                field_name, current
                            )))
                        }
                    }
                }
                // Handle complex path steps (e.g., computed properties)
                _ => {
                    let step_value = self.evaluate(step, data)?;
                    match (&current, &step_value) {
                        (Value::Object(obj), Value::String(key)) => {
                            obj.get(key).cloned().unwrap_or(Value::Null)
                        }
                        (Value::Array(arr), Value::Number(n)) => {
                            let index = n.as_f64().unwrap() as i64;
                            if index < 0 || index >= arr.len() as i64 {
                                Value::Null
                            } else {
                                arr[index as usize].clone()
                            }
                        }
                        _ => Value::Null,
                    }
                }
            };
        }

        Ok(current)
    }

    /// Evaluate a binary operation
    fn evaluate_binary_op(
        &mut self,
        op: crate::ast::BinaryOp,
        lhs: &AstNode,
        rhs: &AstNode,
        data: &Value,
    ) -> Result<Value, EvaluatorError> {
        use crate::ast::BinaryOp;

        // Evaluate operands
        let left = self.evaluate(lhs, data)?;
        let right = self.evaluate(rhs, data)?;

        match op {
            // === Arithmetic Operations ===
            BinaryOp::Add => self.add(&left, &right),
            BinaryOp::Subtract => self.subtract(&left, &right),
            BinaryOp::Multiply => self.multiply(&left, &right),
            BinaryOp::Divide => self.divide(&left, &right),
            BinaryOp::Modulo => self.modulo(&left, &right),

            // === Comparison Operations ===
            BinaryOp::Equal => Ok(Value::Bool(self.equals(&left, &right))),
            BinaryOp::NotEqual => Ok(Value::Bool(!self.equals(&left, &right))),
            BinaryOp::LessThan => self.less_than(&left, &right),
            BinaryOp::LessThanOrEqual => self.less_than_or_equal(&left, &right),
            BinaryOp::GreaterThan => self.greater_than(&left, &right),
            BinaryOp::GreaterThanOrEqual => self.greater_than_or_equal(&left, &right),

            // === Logical Operations ===
            BinaryOp::And => {
                // Short-circuit evaluation
                if self.is_truthy(&left) {
                    Ok(right)
                } else {
                    Ok(left)
                }
            }
            BinaryOp::Or => {
                // Short-circuit evaluation
                if self.is_truthy(&left) {
                    Ok(left)
                } else {
                    Ok(right)
                }
            }

            // === String Concatenation ===
            BinaryOp::Concatenate => self.concatenate(&left, &right),

            // === Range Operator ===
            BinaryOp::Range => self.range(&left, &right),

            // === In Operator ===
            BinaryOp::In => self.in_operator(&left, &right),
        }
    }

    /// Evaluate a unary operation
    fn evaluate_unary_op(
        &mut self,
        op: crate::ast::UnaryOp,
        operand: &AstNode,
        data: &Value,
    ) -> Result<Value, EvaluatorError> {
        use crate::ast::UnaryOp;

        let value = self.evaluate(operand, data)?;

        match op {
            UnaryOp::Negate => match value {
                Value::Number(n) => Ok(serde_json::json!(-n.as_f64().unwrap())),
                _ => Err(EvaluatorError::TypeError(format!(
                    "Cannot negate non-number: {:?}",
                    value
                ))),
            },
            UnaryOp::Not => Ok(Value::Bool(!self.is_truthy(&value))),
        }
    }

    /// Evaluate a function call
    fn evaluate_function_call(
        &mut self,
        name: &str,
        args: &[AstNode],
        data: &Value,
    ) -> Result<Value, EvaluatorError> {
        use crate::functions;

        // Evaluate all arguments
        let mut evaluated_args = Vec::new();
        for arg in args {
            evaluated_args.push(self.evaluate(arg, data)?);
        }

        // Call built-in functions
        match name {
            // String functions
            "string" => {
                if evaluated_args.is_empty() {
                    return Err(EvaluatorError::EvaluationError(
                        "string() requires at least 1 argument".to_string(),
                    ));
                }
                functions::string::string(&evaluated_args[0])
                    .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
            }
            "length" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "length() requires exactly 1 argument".to_string(),
                    ));
                }
                match &evaluated_args[0] {
                    Value::String(s) => functions::string::length(s)
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Err(EvaluatorError::TypeError(
                        "length() requires a string argument".to_string(),
                    )),
                }
            }
            "uppercase" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "uppercase() requires exactly 1 argument".to_string(),
                    ));
                }
                match &evaluated_args[0] {
                    Value::String(s) => functions::string::uppercase(s)
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Err(EvaluatorError::TypeError(
                        "uppercase() requires a string argument".to_string(),
                    )),
                }
            }
            "lowercase" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "lowercase() requires exactly 1 argument".to_string(),
                    ));
                }
                match &evaluated_args[0] {
                    Value::String(s) => functions::string::lowercase(s)
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Err(EvaluatorError::TypeError(
                        "lowercase() requires a string argument".to_string(),
                    )),
                }
            }
            // Numeric functions
            "number" => {
                if evaluated_args.is_empty() {
                    return Err(EvaluatorError::EvaluationError(
                        "number() requires at least 1 argument".to_string(),
                    ));
                }
                functions::numeric::number(&evaluated_args[0])
                    .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
            }
            "sum" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "sum() requires exactly 1 argument".to_string(),
                    ));
                }
                match &evaluated_args[0] {
                    Value::Array(arr) => functions::numeric::sum(arr)
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Err(EvaluatorError::TypeError(
                        "sum() requires an array argument".to_string(),
                    )),
                }
            }
            // Array functions
            "count" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "count() requires exactly 1 argument".to_string(),
                    ));
                }
                match &evaluated_args[0] {
                    Value::Array(arr) => functions::array::count(arr)
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Err(EvaluatorError::TypeError(
                        "count() requires an array argument".to_string(),
                    )),
                }
            }
            "keys" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "keys() requires exactly 1 argument".to_string(),
                    ));
                }
                match &evaluated_args[0] {
                    Value::Object(obj) => functions::object::keys(obj)
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Err(EvaluatorError::TypeError(
                        "keys() requires an object argument".to_string(),
                    )),
                }
            }
            _ => Err(EvaluatorError::ReferenceError(format!(
                "Unknown function: {}",
                name
            ))),
        }
    }

    // === Helper methods for operations ===

    /// Check if a value is truthy (JSONata semantics)
    fn is_truthy(&self, value: &Value) -> bool {
        match value {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Number(n) => n.as_f64().unwrap() != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::Array(arr) => !arr.is_empty(),
            Value::Object(obj) => !obj.is_empty(),
        }
    }

    /// Equality comparison (JSONata semantics)
    fn equals(&self, left: &Value, right: &Value) -> bool {
        match (left, right) {
            (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Number(a), Value::Number(b)) => {
                a.as_f64().unwrap() == b.as_f64().unwrap()
            }
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Array(a), Value::Array(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| self.equals(x, y))
            }
            (Value::Object(a), Value::Object(b)) => {
                a.len() == b.len()
                    && a.iter().all(|(k, v)| b.get(k).map_or(false, |v2| self.equals(v, v2)))
            }
            _ => false,
        }
    }

    /// Addition
    fn add(&self, left: &Value, right: &Value) -> Result<Value, EvaluatorError> {
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => {
                Ok(serde_json::json!(a.as_f64().unwrap() + b.as_f64().unwrap()))
            }
            _ => Err(EvaluatorError::TypeError(format!(
                "Cannot add {:?} and {:?}",
                left, right
            ))),
        }
    }

    /// Subtraction
    fn subtract(&self, left: &Value, right: &Value) -> Result<Value, EvaluatorError> {
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => {
                Ok(serde_json::json!(a.as_f64().unwrap() - b.as_f64().unwrap()))
            }
            _ => Err(EvaluatorError::TypeError(format!(
                "Cannot subtract {:?} and {:?}",
                left, right
            ))),
        }
    }

    /// Multiplication
    fn multiply(&self, left: &Value, right: &Value) -> Result<Value, EvaluatorError> {
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => {
                Ok(serde_json::json!(a.as_f64().unwrap() * b.as_f64().unwrap()))
            }
            _ => Err(EvaluatorError::TypeError(format!(
                "Cannot multiply {:?} and {:?}",
                left, right
            ))),
        }
    }

    /// Division
    fn divide(&self, left: &Value, right: &Value) -> Result<Value, EvaluatorError> {
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => {
                let denominator = b.as_f64().unwrap();
                if denominator == 0.0 {
                    return Err(EvaluatorError::EvaluationError(
                        "Division by zero".to_string(),
                    ));
                }
                Ok(serde_json::json!(a.as_f64().unwrap() / denominator))
            }
            _ => Err(EvaluatorError::TypeError(format!(
                "Cannot divide {:?} and {:?}",
                left, right
            ))),
        }
    }

    /// Modulo
    fn modulo(&self, left: &Value, right: &Value) -> Result<Value, EvaluatorError> {
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => {
                let denominator = b.as_f64().unwrap();
                if denominator == 0.0 {
                    return Err(EvaluatorError::EvaluationError(
                        "Division by zero".to_string(),
                    ));
                }
                Ok(serde_json::json!(a.as_f64().unwrap() % denominator))
            }
            _ => Err(EvaluatorError::TypeError(format!(
                "Cannot compute modulo of {:?} and {:?}",
                left, right
            ))),
        }
    }

    /// Less than comparison
    fn less_than(&self, left: &Value, right: &Value) -> Result<Value, EvaluatorError> {
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => {
                Ok(Value::Bool(a.as_f64().unwrap() < b.as_f64().unwrap()))
            }
            (Value::String(a), Value::String(b)) => Ok(Value::Bool(a < b)),
            _ => Err(EvaluatorError::TypeError(format!(
                "Cannot compare {:?} and {:?}",
                left, right
            ))),
        }
    }

    /// Less than or equal comparison
    fn less_than_or_equal(&self, left: &Value, right: &Value) -> Result<Value, EvaluatorError> {
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => {
                Ok(Value::Bool(a.as_f64().unwrap() <= b.as_f64().unwrap()))
            }
            (Value::String(a), Value::String(b)) => Ok(Value::Bool(a <= b)),
            _ => Err(EvaluatorError::TypeError(format!(
                "Cannot compare {:?} and {:?}",
                left, right
            ))),
        }
    }

    /// Greater than comparison
    fn greater_than(&self, left: &Value, right: &Value) -> Result<Value, EvaluatorError> {
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => {
                Ok(Value::Bool(a.as_f64().unwrap() > b.as_f64().unwrap()))
            }
            (Value::String(a), Value::String(b)) => Ok(Value::Bool(a > b)),
            _ => Err(EvaluatorError::TypeError(format!(
                "Cannot compare {:?} and {:?}",
                left, right
            ))),
        }
    }

    /// Greater than or equal comparison
    fn greater_than_or_equal(&self, left: &Value, right: &Value) -> Result<Value, EvaluatorError> {
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => {
                Ok(Value::Bool(a.as_f64().unwrap() >= b.as_f64().unwrap()))
            }
            (Value::String(a), Value::String(b)) => Ok(Value::Bool(a >= b)),
            _ => Err(EvaluatorError::TypeError(format!(
                "Cannot compare {:?} and {:?}",
                left, right
            ))),
        }
    }

    /// String concatenation
    fn concatenate(&self, left: &Value, right: &Value) -> Result<Value, EvaluatorError> {
        // Convert both values to strings and concatenate
        let left_str = match left {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.as_f64().unwrap().to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Null => "null".to_string(),
            _ => return Err(EvaluatorError::TypeError(
                "Cannot concatenate complex types".to_string(),
            )),
        };

        let right_str = match right {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.as_f64().unwrap().to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Null => "null".to_string(),
            _ => return Err(EvaluatorError::TypeError(
                "Cannot concatenate complex types".to_string(),
            )),
        };

        Ok(Value::String(format!("{}{}", left_str, right_str)))
    }

    /// Range operator (e.g., 1..5 produces [1,2,3,4,5])
    fn range(&self, left: &Value, right: &Value) -> Result<Value, EvaluatorError> {
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => {
                let start = a.as_f64().unwrap() as i64;
                let end = b.as_f64().unwrap() as i64;

                let mut result = Vec::new();
                if start <= end {
                    for i in start..=end {
                        result.push(serde_json::json!(i));
                    }
                } else {
                    for i in (end..=start).rev() {
                        result.push(serde_json::json!(i));
                    }
                }
                Ok(Value::Array(result))
            }
            _ => Err(EvaluatorError::TypeError(
                "Range operator requires two numbers".to_string(),
            )),
        }
    }

    /// In operator (checks if left is in right array/object)
    fn in_operator(&self, left: &Value, right: &Value) -> Result<Value, EvaluatorError> {
        match right {
            Value::Array(arr) => {
                Ok(Value::Bool(arr.iter().any(|v| self.equals(left, v))))
            }
            Value::Object(obj) => {
                if let Value::String(key) = left {
                    Ok(Value::Bool(obj.contains_key(key)))
                } else {
                    Ok(Value::Bool(false))
                }
            }
            _ => Err(EvaluatorError::TypeError(
                "In operator requires an array or object on the right side".to_string(),
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
    use crate::ast::{BinaryOp, UnaryOp};

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

    #[test]
    fn test_evaluate_variables() {
        let mut evaluator = Evaluator::new();
        let data = Value::Null;

        // Bind a variable
        evaluator.context.bind("x".to_string(), serde_json::json!(100));

        // Look up the variable
        let result = evaluator.evaluate(&AstNode::variable("x"), &data).unwrap();
        assert_eq!(result, serde_json::json!(100));

        // Undefined variable should error
        let result = evaluator.evaluate(&AstNode::variable("undefined"), &data);
        assert!(result.is_err());
    }

    #[test]
    fn test_evaluate_path() {
        let mut evaluator = Evaluator::new();
        let data = serde_json::json!({
            "foo": {
                "bar": {
                    "baz": 42
                }
            }
        });

        // Simple path
        let path = AstNode::Path {
            steps: vec![AstNode::string("foo")],
        };
        let result = evaluator.evaluate(&path, &data).unwrap();
        assert_eq!(result, serde_json::json!({"bar": {"baz": 42}}));

        // Nested path
        let path = AstNode::Path {
            steps: vec![
                AstNode::string("foo"),
                AstNode::string("bar"),
                AstNode::string("baz"),
            ],
        };
        let result = evaluator.evaluate(&path, &data).unwrap();
        assert_eq!(result, serde_json::json!(42));

        // Missing path returns null
        let path = AstNode::Path {
            steps: vec![AstNode::string("missing")],
        };
        let result = evaluator.evaluate(&path, &data).unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn test_arithmetic_operations() {
        let mut evaluator = Evaluator::new();
        let data = Value::Null;

        // Addition
        let expr = AstNode::Binary {
            op: BinaryOp::Add,
            lhs: Box::new(AstNode::number(10.0)),
            rhs: Box::new(AstNode::number(5.0)),
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, serde_json::json!(15.0));

        // Subtraction
        let expr = AstNode::Binary {
            op: BinaryOp::Subtract,
            lhs: Box::new(AstNode::number(10.0)),
            rhs: Box::new(AstNode::number(5.0)),
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, serde_json::json!(5.0));

        // Multiplication
        let expr = AstNode::Binary {
            op: BinaryOp::Multiply,
            lhs: Box::new(AstNode::number(10.0)),
            rhs: Box::new(AstNode::number(5.0)),
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, serde_json::json!(50.0));

        // Division
        let expr = AstNode::Binary {
            op: BinaryOp::Divide,
            lhs: Box::new(AstNode::number(10.0)),
            rhs: Box::new(AstNode::number(5.0)),
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, serde_json::json!(2.0));

        // Modulo
        let expr = AstNode::Binary {
            op: BinaryOp::Modulo,
            lhs: Box::new(AstNode::number(10.0)),
            rhs: Box::new(AstNode::number(3.0)),
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, serde_json::json!(1.0));
    }

    #[test]
    fn test_division_by_zero() {
        let mut evaluator = Evaluator::new();
        let data = Value::Null;

        let expr = AstNode::Binary {
            op: BinaryOp::Divide,
            lhs: Box::new(AstNode::number(10.0)),
            rhs: Box::new(AstNode::number(0.0)),
        };
        let result = evaluator.evaluate(&expr, &data);
        assert!(result.is_err());
    }

    #[test]
    fn test_comparison_operations() {
        let mut evaluator = Evaluator::new();
        let data = Value::Null;

        // Equal
        let expr = AstNode::Binary {
            op: BinaryOp::Equal,
            lhs: Box::new(AstNode::number(5.0)),
            rhs: Box::new(AstNode::number(5.0)),
        };
        assert_eq!(evaluator.evaluate(&expr, &data).unwrap(), Value::Bool(true));

        // Not equal
        let expr = AstNode::Binary {
            op: BinaryOp::NotEqual,
            lhs: Box::new(AstNode::number(5.0)),
            rhs: Box::new(AstNode::number(3.0)),
        };
        assert_eq!(evaluator.evaluate(&expr, &data).unwrap(), Value::Bool(true));

        // Less than
        let expr = AstNode::Binary {
            op: BinaryOp::LessThan,
            lhs: Box::new(AstNode::number(3.0)),
            rhs: Box::new(AstNode::number(5.0)),
        };
        assert_eq!(evaluator.evaluate(&expr, &data).unwrap(), Value::Bool(true));

        // Greater than
        let expr = AstNode::Binary {
            op: BinaryOp::GreaterThan,
            lhs: Box::new(AstNode::number(5.0)),
            rhs: Box::new(AstNode::number(3.0)),
        };
        assert_eq!(evaluator.evaluate(&expr, &data).unwrap(), Value::Bool(true));
    }

    #[test]
    fn test_logical_operations() {
        let mut evaluator = Evaluator::new();
        let data = Value::Null;

        // And - both true
        let expr = AstNode::Binary {
            op: BinaryOp::And,
            lhs: Box::new(AstNode::boolean(true)),
            rhs: Box::new(AstNode::boolean(true)),
        };
        assert_eq!(evaluator.evaluate(&expr, &data).unwrap(), Value::Bool(true));

        // And - first false
        let expr = AstNode::Binary {
            op: BinaryOp::And,
            lhs: Box::new(AstNode::boolean(false)),
            rhs: Box::new(AstNode::boolean(true)),
        };
        assert_eq!(evaluator.evaluate(&expr, &data).unwrap(), Value::Bool(false));

        // Or - first true
        let expr = AstNode::Binary {
            op: BinaryOp::Or,
            lhs: Box::new(AstNode::boolean(true)),
            rhs: Box::new(AstNode::boolean(false)),
        };
        assert_eq!(evaluator.evaluate(&expr, &data).unwrap(), Value::Bool(true));

        // Or - both false
        let expr = AstNode::Binary {
            op: BinaryOp::Or,
            lhs: Box::new(AstNode::boolean(false)),
            rhs: Box::new(AstNode::boolean(false)),
        };
        assert_eq!(evaluator.evaluate(&expr, &data).unwrap(), Value::Bool(false));
    }

    #[test]
    fn test_string_concatenation() {
        let mut evaluator = Evaluator::new();
        let data = Value::Null;

        let expr = AstNode::Binary {
            op: BinaryOp::Concatenate,
            lhs: Box::new(AstNode::string("Hello")),
            rhs: Box::new(AstNode::string(" World")),
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, Value::String("Hello World".to_string()));
    }

    #[test]
    fn test_range_operator() {
        let mut evaluator = Evaluator::new();
        let data = Value::Null;

        // Forward range
        let expr = AstNode::Binary {
            op: BinaryOp::Range,
            lhs: Box::new(AstNode::number(1.0)),
            rhs: Box::new(AstNode::number(5.0)),
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(
            result,
            serde_json::json!([1, 2, 3, 4, 5])
        );

        // Backward range
        let expr = AstNode::Binary {
            op: BinaryOp::Range,
            lhs: Box::new(AstNode::number(5.0)),
            rhs: Box::new(AstNode::number(1.0)),
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(
            result,
            serde_json::json!([5, 4, 3, 2, 1])
        );
    }

    #[test]
    fn test_in_operator() {
        let mut evaluator = Evaluator::new();
        let data = Value::Null;

        // In array
        let expr = AstNode::Binary {
            op: BinaryOp::In,
            lhs: Box::new(AstNode::number(3.0)),
            rhs: Box::new(AstNode::Array(vec![
                AstNode::number(1.0),
                AstNode::number(2.0),
                AstNode::number(3.0),
            ])),
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, Value::Bool(true));

        // Not in array
        let expr = AstNode::Binary {
            op: BinaryOp::In,
            lhs: Box::new(AstNode::number(5.0)),
            rhs: Box::new(AstNode::Array(vec![
                AstNode::number(1.0),
                AstNode::number(2.0),
                AstNode::number(3.0),
            ])),
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_unary_operations() {
        let mut evaluator = Evaluator::new();
        let data = Value::Null;

        // Negation
        let expr = AstNode::Unary {
            op: UnaryOp::Negate,
            operand: Box::new(AstNode::number(5.0)),
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, serde_json::json!(-5.0));

        // Not
        let expr = AstNode::Unary {
            op: UnaryOp::Not,
            operand: Box::new(AstNode::boolean(true)),
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_array_construction() {
        let mut evaluator = Evaluator::new();
        let data = Value::Null;

        let expr = AstNode::Array(vec![
            AstNode::number(1.0),
            AstNode::number(2.0),
            AstNode::number(3.0),
        ]);
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, serde_json::json!([1, 2, 3]));
    }

    #[test]
    fn test_object_construction() {
        let mut evaluator = Evaluator::new();
        let data = Value::Null;

        let expr = AstNode::Object(vec![
            (AstNode::string("name"), AstNode::string("Alice")),
            (AstNode::string("age"), AstNode::number(30.0)),
        ]);
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(
            result,
            serde_json::json!({"name": "Alice", "age": 30})
        );
    }

    #[test]
    fn test_conditional() {
        let mut evaluator = Evaluator::new();
        let data = Value::Null;

        // True condition
        let expr = AstNode::Conditional {
            condition: Box::new(AstNode::boolean(true)),
            then_branch: Box::new(AstNode::string("yes")),
            else_branch: Some(Box::new(AstNode::string("no"))),
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, Value::String("yes".to_string()));

        // False condition
        let expr = AstNode::Conditional {
            condition: Box::new(AstNode::boolean(false)),
            then_branch: Box::new(AstNode::string("yes")),
            else_branch: Some(Box::new(AstNode::string("no"))),
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, Value::String("no".to_string()));

        // No else branch
        let expr = AstNode::Conditional {
            condition: Box::new(AstNode::boolean(false)),
            then_branch: Box::new(AstNode::string("yes")),
            else_branch: None,
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn test_block_expression() {
        let mut evaluator = Evaluator::new();
        let data = Value::Null;

        let expr = AstNode::Block(vec![
            AstNode::number(1.0),
            AstNode::number(2.0),
            AstNode::number(3.0),
        ]);
        let result = evaluator.evaluate(&expr, &data).unwrap();
        // Block returns the last expression
        assert_eq!(result, serde_json::json!(3.0));
    }

    #[test]
    fn test_function_calls() {
        let mut evaluator = Evaluator::new();
        let data = Value::Null;

        // uppercase function
        let expr = AstNode::Function {
            name: "uppercase".to_string(),
            args: vec![AstNode::string("hello")],
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, Value::String("HELLO".to_string()));

        // lowercase function
        let expr = AstNode::Function {
            name: "lowercase".to_string(),
            args: vec![AstNode::string("HELLO")],
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, Value::String("hello".to_string()));

        // length function
        let expr = AstNode::Function {
            name: "length".to_string(),
            args: vec![AstNode::string("hello")],
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, serde_json::json!(5));

        // sum function
        let expr = AstNode::Function {
            name: "sum".to_string(),
            args: vec![AstNode::Array(vec![
                AstNode::number(1.0),
                AstNode::number(2.0),
                AstNode::number(3.0),
            ])],
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, serde_json::json!(6.0));

        // count function
        let expr = AstNode::Function {
            name: "count".to_string(),
            args: vec![AstNode::Array(vec![
                AstNode::number(1.0),
                AstNode::number(2.0),
                AstNode::number(3.0),
            ])],
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, serde_json::json!(3));
    }

    #[test]
    fn test_complex_nested_data() {
        let mut evaluator = Evaluator::new();
        let data = serde_json::json!({
            "users": [
                {"name": "Alice", "age": 30},
                {"name": "Bob", "age": 25},
                {"name": "Charlie", "age": 35}
            ],
            "metadata": {
                "total": 3,
                "version": "1.0"
            }
        });

        // Access nested field
        let path = AstNode::Path {
            steps: vec![
                AstNode::string("metadata"),
                AstNode::string("version"),
            ],
        };
        let result = evaluator.evaluate(&path, &data).unwrap();
        assert_eq!(result, Value::String("1.0".to_string()));
    }

    #[test]
    fn test_error_handling() {
        let mut evaluator = Evaluator::new();
        let data = Value::Null;

        // Type error: adding string and number
        let expr = AstNode::Binary {
            op: BinaryOp::Add,
            lhs: Box::new(AstNode::string("hello")),
            rhs: Box::new(AstNode::number(5.0)),
        };
        let result = evaluator.evaluate(&expr, &data);
        assert!(result.is_err());

        // Reference error: undefined function
        let expr = AstNode::Function {
            name: "undefined_function".to_string(),
            args: vec![],
        };
        let result = evaluator.evaluate(&expr, &data);
        assert!(result.is_err());
    }

    #[test]
    fn test_truthiness() {
        let evaluator = Evaluator::new();

        assert!(!evaluator.is_truthy(&Value::Null));
        assert!(!evaluator.is_truthy(&Value::Bool(false)));
        assert!(evaluator.is_truthy(&Value::Bool(true)));
        assert!(!evaluator.is_truthy(&serde_json::json!(0)));
        assert!(evaluator.is_truthy(&serde_json::json!(1)));
        assert!(!evaluator.is_truthy(&Value::String("".to_string())));
        assert!(evaluator.is_truthy(&Value::String("hello".to_string())));
        assert!(!evaluator.is_truthy(&Value::Array(vec![])));
        assert!(evaluator.is_truthy(&serde_json::json!([1, 2, 3])));
    }

    #[test]
    fn test_integration_with_parser() {
        use crate::parser::parse;

        let mut evaluator = Evaluator::new();
        let data = serde_json::json!({
            "price": 10,
            "quantity": 5
        });

        // Test simple path
        let ast = parse("price").unwrap();
        let result = evaluator.evaluate(&ast, &data).unwrap();
        assert_eq!(result, serde_json::json!(10));

        // Test arithmetic
        let ast = parse("price * quantity").unwrap();
        let result = evaluator.evaluate(&ast, &data).unwrap();
        assert_eq!(result, serde_json::json!(50));

        // Test comparison
        let ast = parse("price > 5").unwrap();
        let result = evaluator.evaluate(&ast, &data).unwrap();
        assert_eq!(result, Value::Bool(true));
    }
}

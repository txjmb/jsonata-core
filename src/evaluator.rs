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

    pub fn unbind(&mut self, name: &str) {
        self.bindings.remove(name);
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
                // Special case: $ alone (empty name) refers to root context
                if name.is_empty() {
                    return Ok(data.clone());
                }

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
                // Lambda functions are first-class values
                // They need to be stored as a special value type
                // For now, we'll use a JSON representation
                // Full implementation would use a custom Value type
                let lambda_repr = serde_json::json!({
                    "__lambda__": true,
                    "params": params,
                    "body": format!("{:?}", body)  // Store AST node representation
                });
                Ok(lambda_repr)
            }
        }
    }

    /// Evaluate a path expression (e.g., foo.bar.baz)
    fn evaluate_path(&mut self, steps: &[AstNode], data: &Value) -> Result<Value, EvaluatorError> {
        // Avoid cloning by using references and only cloning when necessary
        if steps.is_empty() {
            return Ok(data.clone());
        }

        // Fast path: single field access on object
        // This is a very common pattern, so optimize it
        if steps.len() == 1 {
            if let AstNode::String(field_name) = &steps[0] {
                return match data {
                    Value::Object(obj) => {
                        Ok(obj.get(field_name).cloned().unwrap_or(Value::Null))
                    }
                    Value::Array(arr) => {
                        // Array mapping: extract field from each object
                        // Pre-allocate with exact capacity for better performance
                        let mut mapped = Vec::with_capacity(arr.len());
                        for item in arr {
                            match item {
                                Value::Object(obj) => {
                                    mapped.push(obj.get(field_name).cloned().unwrap_or(Value::Null));
                                }
                                _ => mapped.push(Value::Null),
                            }
                        }
                        Ok(Value::Array(mapped))
                    }
                    _ => Ok(Value::Null),
                };
            }
        }

        // For the first step, work with a reference
        let mut current: Value = match &steps[0] {
            AstNode::String(field_name) => {
                match data {
                    Value::Object(obj) => {
                        obj.get(field_name).cloned().unwrap_or(Value::Null)
                    }
                    Value::Array(arr) => {
                        // Array mapping: extract field from each object in array
                        let mut mapped = Vec::with_capacity(arr.len());
                        for item in arr {
                            match item {
                                Value::Object(obj) => {
                                    mapped.push(obj.get(field_name).cloned().unwrap_or(Value::Null));
                                }
                                _ => mapped.push(Value::Null),
                            }
                        }
                        Value::Array(mapped)
                    }
                    Value::Null => Value::Null,
                    _ => {
                        return Err(EvaluatorError::TypeError(format!(
                            "Cannot access field '{}' on non-object",
                            field_name
                        )))
                    }
                }
            }
            step => {
                // Complex first step - evaluate it
                self.evaluate_path_step(step, data, data)?
            }
        };

        // Process remaining steps
        for step in &steps[1..] {
            current = match step {
                AstNode::String(field_name) => {
                    // Navigate into object field or map over array
                    match &current {
                        Value::Object(obj) => {
                            obj.get(field_name).cloned().unwrap_or(Value::Null)
                        }
                        Value::Array(arr) => {
                            // Array mapping: extract field from each object in array
                            // Pre-allocate vector with exact capacity
                            let mut mapped = Vec::with_capacity(arr.len());
                            for item in arr {
                                match item {
                                    Value::Object(obj) => {
                                        mapped.push(obj.get(field_name).cloned().unwrap_or(Value::Null));
                                    }
                                    _ => mapped.push(Value::Null),
                                }
                            }
                            Value::Array(mapped)
                        }
                        Value::Null => Value::Null,
                        _ => {
                            return Err(EvaluatorError::TypeError(format!(
                                "Cannot access field '{}' on non-object",
                                field_name
                            )))
                        }
                    }
                }
                // Handle complex path steps (e.g., computed properties, object construction)
                _ => self.evaluate_path_step(step, &current, data)?
            };
        }

        Ok(current)
    }

    /// Helper to evaluate a complex path step
    fn evaluate_path_step(&mut self, step: &AstNode, current: &Value, original_data: &Value) -> Result<Value, EvaluatorError> {
        // Special case: array mapping with object construction
        // e.g., items.{"name": name, "price": price}
        if matches!(current, Value::Array(_)) && matches!(step, AstNode::Object(_)) {
            // Map over array, evaluating the object constructor for each item
            match current {
                Value::Array(arr) => {
                    let mapped: Result<Vec<Value>, EvaluatorError> = arr
                        .iter()
                        .map(|item| {
                            // Evaluate the object constructor in the context of this array item
                            self.evaluate(step, item)
                        })
                        .collect();
                    Ok(Value::Array(mapped?))
                }
                _ => unreachable!(),
            }
        } else {
            // For certain operations (Binary, Function calls, Variables), the step evaluates to a new value
            // rather than being used to index/access the current value
            // e.g., items[price > 50] where [price > 50] is a filter operation
            // or $x.price where $x is a variable binding
            if matches!(step, AstNode::Binary { .. } | AstNode::Function { .. } | AstNode::Variable(_)) {
                // Evaluate the step in the context of original_data and return the result directly
                return self.evaluate(step, original_data);
            }

            // Standard path step evaluation for indexing/accessing current value
            let step_value = self.evaluate(step, original_data)?;
            Ok(match (current, &step_value) {
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
            })
        }
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

        // Special handling for 'In' operator - check for array filtering
        // Must evaluate lhs first to determine if this is array filtering
        if op == BinaryOp::In {
            let left = self.evaluate(lhs, data)?;

            // Check if this is array filtering: array[predicate]
            if matches!(left, Value::Array(_)) {
                // Try evaluating rhs in current context to see if it's a simple index
                let right_result = self.evaluate(rhs, data);

                if let Ok(Value::Number(_)) = right_result {
                    // Simple numeric index: array[n]
                    return self.array_index(&left, &right_result.unwrap());
                } else {
                    // This is array filtering: array[predicate]
                    // Evaluate the predicate for each array item
                    return self.array_filter(lhs, rhs, &left, data);
                }
            }
        }

        // Standard evaluation: evaluate both operands
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
            // Note: Array indexing and filtering are handled earlier in evaluate_binary_op
            BinaryOp::In => {
                // This handles the standard 'in' operator for membership testing
                // e.g., "foo" in ["foo", "bar"]
                self.in_operator(&left, &right)
            }
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
            // Additional string functions
            "substring" => {
                if evaluated_args.len() < 2 || evaluated_args.len() > 3 {
                    return Err(EvaluatorError::EvaluationError(
                        "substring() requires 2 or 3 arguments".to_string(),
                    ));
                }
                match (&evaluated_args[0], &evaluated_args[1]) {
                    (Value::String(s), Value::Number(start)) => {
                        let length = if evaluated_args.len() == 3 {
                            match &evaluated_args[2] {
                                Value::Number(l) => Some(l.as_f64().unwrap() as i64),
                                _ => return Err(EvaluatorError::TypeError(
                                    "substring() length must be a number".to_string(),
                                )),
                            }
                        } else {
                            None
                        };
                        functions::string::substring(s, start.as_f64().unwrap() as i64, length)
                            .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
                    }
                    _ => Err(EvaluatorError::TypeError(
                        "substring() requires string and number arguments".to_string(),
                    )),
                }
            }
            "substringBefore" => {
                if evaluated_args.len() != 2 {
                    return Err(EvaluatorError::EvaluationError(
                        "substringBefore() requires exactly 2 arguments".to_string(),
                    ));
                }
                match (&evaluated_args[0], &evaluated_args[1]) {
                    (Value::String(s), Value::String(sep)) => {
                        functions::string::substring_before(s, sep)
                            .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
                    }
                    _ => Err(EvaluatorError::TypeError(
                        "substringBefore() requires string arguments".to_string(),
                    )),
                }
            }
            "substringAfter" => {
                if evaluated_args.len() != 2 {
                    return Err(EvaluatorError::EvaluationError(
                        "substringAfter() requires exactly 2 arguments".to_string(),
                    ));
                }
                match (&evaluated_args[0], &evaluated_args[1]) {
                    (Value::String(s), Value::String(sep)) => {
                        functions::string::substring_after(s, sep)
                            .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
                    }
                    _ => Err(EvaluatorError::TypeError(
                        "substringAfter() requires string arguments".to_string(),
                    )),
                }
            }
            "trim" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "trim() requires exactly 1 argument".to_string(),
                    ));
                }
                match &evaluated_args[0] {
                    Value::String(s) => functions::string::trim(s)
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Err(EvaluatorError::TypeError(
                        "trim() requires a string argument".to_string(),
                    )),
                }
            }
            "contains" => {
                if evaluated_args.len() != 2 {
                    return Err(EvaluatorError::EvaluationError(
                        "contains() requires exactly 2 arguments".to_string(),
                    ));
                }
                match (&evaluated_args[0], &evaluated_args[1]) {
                    (Value::String(s), Value::String(pattern)) => {
                        functions::string::contains(s, pattern)
                            .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
                    }
                    _ => Err(EvaluatorError::TypeError(
                        "contains() requires string arguments".to_string(),
                    )),
                }
            }
            "split" => {
                if evaluated_args.len() < 2 || evaluated_args.len() > 3 {
                    return Err(EvaluatorError::EvaluationError(
                        "split() requires 2 or 3 arguments".to_string(),
                    ));
                }
                match (&evaluated_args[0], &evaluated_args[1]) {
                    (Value::String(s), Value::String(sep)) => {
                        let limit = if evaluated_args.len() == 3 {
                            match &evaluated_args[2] {
                                Value::Number(n) => Some(n.as_f64().unwrap() as usize),
                                _ => return Err(EvaluatorError::TypeError(
                                    "split() limit must be a number".to_string(),
                                )),
                            }
                        } else {
                            None
                        };
                        functions::string::split(s, sep, limit)
                            .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
                    }
                    _ => Err(EvaluatorError::TypeError(
                        "split() requires string arguments".to_string(),
                    )),
                }
            }
            "join" => {
                if evaluated_args.len() < 1 || evaluated_args.len() > 2 {
                    return Err(EvaluatorError::EvaluationError(
                        "join() requires 1 or 2 arguments".to_string(),
                    ));
                }
                match &evaluated_args[0] {
                    Value::Array(arr) => {
                        let separator = if evaluated_args.len() == 2 {
                            match &evaluated_args[1] {
                                Value::String(s) => Some(s.as_str()),
                                _ => return Err(EvaluatorError::TypeError(
                                    "join() separator must be a string".to_string(),
                                )),
                            }
                        } else {
                            None
                        };
                        functions::string::join(arr, separator)
                            .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
                    }
                    _ => Err(EvaluatorError::TypeError(
                        "join() requires an array argument".to_string(),
                    )),
                }
            }
            "replace" => {
                if evaluated_args.len() < 3 || evaluated_args.len() > 4 {
                    return Err(EvaluatorError::EvaluationError(
                        "replace() requires 3 or 4 arguments".to_string(),
                    ));
                }
                match (&evaluated_args[0], &evaluated_args[1], &evaluated_args[2]) {
                    (Value::String(s), Value::String(pattern), Value::String(replacement)) => {
                        let limit = if evaluated_args.len() == 4 {
                            match &evaluated_args[3] {
                                Value::Number(n) => Some(n.as_f64().unwrap() as usize),
                                _ => return Err(EvaluatorError::TypeError(
                                    "replace() limit must be a number".to_string(),
                                )),
                            }
                        } else {
                            None
                        };
                        functions::string::replace(s, pattern, replacement, limit)
                            .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
                    }
                    _ => Err(EvaluatorError::TypeError(
                        "replace() requires string arguments".to_string(),
                    )),
                }
            }
            // Additional numeric functions
            "max" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "max() requires exactly 1 argument".to_string(),
                    ));
                }
                match &evaluated_args[0] {
                    Value::Array(arr) => functions::numeric::max(arr)
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Err(EvaluatorError::TypeError(
                        "max() requires an array argument".to_string(),
                    )),
                }
            }
            "min" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "min() requires exactly 1 argument".to_string(),
                    ));
                }
                match &evaluated_args[0] {
                    Value::Array(arr) => functions::numeric::min(arr)
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Err(EvaluatorError::TypeError(
                        "min() requires an array argument".to_string(),
                    )),
                }
            }
            "average" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "average() requires exactly 1 argument".to_string(),
                    ));
                }
                match &evaluated_args[0] {
                    Value::Array(arr) => functions::numeric::average(arr)
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Err(EvaluatorError::TypeError(
                        "average() requires an array argument".to_string(),
                    )),
                }
            }
            "abs" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "abs() requires exactly 1 argument".to_string(),
                    ));
                }
                match &evaluated_args[0] {
                    Value::Number(n) => functions::numeric::abs(n.as_f64().unwrap())
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Err(EvaluatorError::TypeError(
                        "abs() requires a number argument".to_string(),
                    )),
                }
            }
            "floor" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "floor() requires exactly 1 argument".to_string(),
                    ));
                }
                match &evaluated_args[0] {
                    Value::Number(n) => functions::numeric::floor(n.as_f64().unwrap())
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Err(EvaluatorError::TypeError(
                        "floor() requires a number argument".to_string(),
                    )),
                }
            }
            "ceil" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "ceil() requires exactly 1 argument".to_string(),
                    ));
                }
                match &evaluated_args[0] {
                    Value::Number(n) => functions::numeric::ceil(n.as_f64().unwrap())
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Err(EvaluatorError::TypeError(
                        "ceil() requires a number argument".to_string(),
                    )),
                }
            }
            "round" => {
                if evaluated_args.len() < 1 || evaluated_args.len() > 2 {
                    return Err(EvaluatorError::EvaluationError(
                        "round() requires 1 or 2 arguments".to_string(),
                    ));
                }
                match &evaluated_args[0] {
                    Value::Number(n) => {
                        let precision = if evaluated_args.len() == 2 {
                            match &evaluated_args[1] {
                                Value::Number(p) => Some(p.as_f64().unwrap() as i32),
                                _ => return Err(EvaluatorError::TypeError(
                                    "round() precision must be a number".to_string(),
                                )),
                            }
                        } else {
                            None
                        };
                        functions::numeric::round(n.as_f64().unwrap(), precision)
                            .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
                    }
                    _ => Err(EvaluatorError::TypeError(
                        "round() requires a number argument".to_string(),
                    )),
                }
            }
            "sqrt" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "sqrt() requires exactly 1 argument".to_string(),
                    ));
                }
                match &evaluated_args[0] {
                    Value::Number(n) => functions::numeric::sqrt(n.as_f64().unwrap())
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Err(EvaluatorError::TypeError(
                        "sqrt() requires a number argument".to_string(),
                    )),
                }
            }
            "power" => {
                if evaluated_args.len() != 2 {
                    return Err(EvaluatorError::EvaluationError(
                        "power() requires exactly 2 arguments".to_string(),
                    ));
                }
                match (&evaluated_args[0], &evaluated_args[1]) {
                    (Value::Number(base), Value::Number(exp)) => {
                        functions::numeric::power(base.as_f64().unwrap(), exp.as_f64().unwrap())
                            .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
                    }
                    _ => Err(EvaluatorError::TypeError(
                        "power() requires number arguments".to_string(),
                    )),
                }
            }
            // Additional array functions
            "append" => {
                if evaluated_args.len() != 2 {
                    return Err(EvaluatorError::EvaluationError(
                        "append() requires exactly 2 arguments".to_string(),
                    ));
                }
                match &evaluated_args[0] {
                    Value::Array(arr) => {
                        functions::array::append(arr, &evaluated_args[1])
                            .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
                    }
                    _ => Err(EvaluatorError::TypeError(
                        "append() requires an array as first argument".to_string(),
                    )),
                }
            }
            "reverse" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "reverse() requires exactly 1 argument".to_string(),
                    ));
                }
                match &evaluated_args[0] {
                    Value::Array(arr) => functions::array::reverse(arr)
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Err(EvaluatorError::TypeError(
                        "reverse() requires an array argument".to_string(),
                    )),
                }
            }
            "sort" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "sort() requires exactly 1 argument".to_string(),
                    ));
                }
                match &evaluated_args[0] {
                    Value::Array(arr) => functions::array::sort(arr)
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Err(EvaluatorError::TypeError(
                        "sort() requires an array argument".to_string(),
                    )),
                }
            }
            "distinct" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "distinct() requires exactly 1 argument".to_string(),
                    ));
                }
                match &evaluated_args[0] {
                    Value::Array(arr) => functions::array::distinct(arr)
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Err(EvaluatorError::TypeError(
                        "distinct() requires an array argument".to_string(),
                    )),
                }
            }
            "exists" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "exists() requires exactly 1 argument".to_string(),
                    ));
                }
                functions::array::exists(&evaluated_args[0])
                    .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
            }
            // Object functions
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
            "lookup" => {
                if evaluated_args.len() != 2 {
                    return Err(EvaluatorError::EvaluationError(
                        "lookup() requires exactly 2 arguments".to_string(),
                    ));
                }
                match (&evaluated_args[0], &evaluated_args[1]) {
                    (Value::Object(obj), Value::String(key)) => {
                        functions::object::lookup(obj, key)
                            .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
                    }
                    _ => Err(EvaluatorError::TypeError(
                        "lookup() requires an object and string argument".to_string(),
                    )),
                }
            }
            "spread" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "spread() requires exactly 1 argument".to_string(),
                    ));
                }
                match &evaluated_args[0] {
                    Value::Object(obj) => functions::object::spread(obj)
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Err(EvaluatorError::TypeError(
                        "spread() requires an object argument".to_string(),
                    )),
                }
            }
            "merge" => {
                if evaluated_args.is_empty() {
                    return Err(EvaluatorError::EvaluationError(
                        "merge() requires at least 1 argument".to_string(),
                    ));
                }
                functions::object::merge(&evaluated_args)
                    .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
            }

            // Higher-order functions
            "map" => {
                if args.len() != 2 {
                    return Err(EvaluatorError::EvaluationError(
                        "map() requires exactly 2 arguments".to_string(),
                    ));
                }

                // Evaluate the array argument
                let array = self.evaluate(&args[0], data)?;

                match array {
                    Value::Array(arr) => {
                        let mut result = Vec::with_capacity(arr.len());
                        for item in arr {
                            // Apply function to each item
                            let mapped = self.apply_function(&args[1], &[item], data)?;
                            result.push(mapped);
                        }
                        Ok(Value::Array(result))
                    }
                    Value::Null => Ok(Value::Null),
                    _ => Err(EvaluatorError::TypeError(
                        "map() first argument must be an array".to_string(),
                    )),
                }
            }

            "filter" => {
                if args.len() != 2 {
                    return Err(EvaluatorError::EvaluationError(
                        "filter() requires exactly 2 arguments".to_string(),
                    ));
                }

                // Evaluate the array argument
                let array = self.evaluate(&args[0], data)?;

                match array {
                    Value::Array(arr) => {
                        let mut result = Vec::with_capacity(arr.len() / 2);
                        for item in arr {
                            // Apply predicate function to each item
                            let predicate_result = self.apply_function(&args[1], &[item.clone()], data)?;
                            if self.is_truthy(&predicate_result) {
                                result.push(item);
                            }
                        }
                        Ok(Value::Array(result))
                    }
                    Value::Null => Ok(Value::Null),
                    _ => Err(EvaluatorError::TypeError(
                        "filter() first argument must be an array".to_string(),
                    )),
                }
            }

            "reduce" => {
                if args.len() < 2 || args.len() > 3 {
                    return Err(EvaluatorError::EvaluationError(
                        "reduce() requires 2 or 3 arguments".to_string(),
                    ));
                }

                // Evaluate the array argument
                let array = self.evaluate(&args[0], data)?;

                match array {
                    Value::Array(arr) => {
                        if arr.is_empty() {
                            // Return initial value if provided, otherwise null
                            return if args.len() == 3 {
                                self.evaluate(&args[2], data)
                            } else {
                                Ok(Value::Null)
                            };
                        }

                        // Get initial accumulator
                        let mut accumulator = if args.len() == 3 {
                            self.evaluate(&args[2], data)?
                        } else {
                            arr[0].clone()
                        };

                        let start_idx = if args.len() == 3 { 0 } else { 1 };

                        // Apply function to each element
                        for item in &arr[start_idx..] {
                            // For reduce, the function receives (accumulator, current_value)
                            // Apply lambda with both parameters
                            accumulator = self.apply_function(&args[1], &[accumulator.clone(), item.clone()], data)?;
                        }

                        Ok(accumulator)
                    }
                    Value::Null => Ok(Value::Null),
                    _ => Err(EvaluatorError::TypeError(
                        "reduce() first argument must be an array".to_string(),
                    )),
                }
            }

            "single" => {
                if args.len() != 2 {
                    return Err(EvaluatorError::EvaluationError(
                        "single() requires exactly 2 arguments".to_string(),
                    ));
                }

                // Evaluate the array argument
                let array = self.evaluate(&args[0], data)?;

                match array {
                    Value::Array(arr) => {
                        let mut matches = Vec::new();
                        for item in arr {
                            // Apply predicate function to each item
                            let predicate_result = self.apply_function(&args[1], &[item.clone()], data)?;
                            if self.is_truthy(&predicate_result) {
                                matches.push(item);
                            }
                        }

                        match matches.len() {
                            0 => Err(EvaluatorError::EvaluationError(
                                "single() predicate matches no values".to_string(),
                            )),
                            1 => Ok(matches.into_iter().next().unwrap()),
                            count => Err(EvaluatorError::EvaluationError(
                                format!("single() predicate matches {} values (expected exactly 1)", count),
                            )),
                        }
                    }
                    Value::Null => Ok(Value::Null),
                    _ => Err(EvaluatorError::TypeError(
                        "single() first argument must be an array".to_string(),
                    )),
                }
            }

            "sift" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(EvaluatorError::EvaluationError(
                        "sift() requires 1 or 2 arguments".to_string(),
                    ));
                }

                // Determine which argument is the object and which is the function
                let (obj_value, func_arg) = if args.len() == 1 {
                    // Single argument: use current data as object
                    (data.clone(), &args[0])
                } else {
                    // Two arguments: first is object, second is function
                    (self.evaluate(&args[0], data)?, &args[1])
                };

                match obj_value {
                    Value::Object(obj) => {
                        let mut result = serde_json::Map::new();
                        for (key, value) in obj {
                            // Apply predicate function with the value
                            // In JSONata, the predicate receives the value, not the key-value pair
                            let predicate_result = self.apply_function(func_arg, &[value.clone()], data)?;
                            if self.is_truthy(&predicate_result) {
                                result.insert(key.clone(), value.clone());
                            }
                        }
                        Ok(Value::Object(result))
                    }
                    Value::Null => Ok(Value::Null),
                    _ => Err(EvaluatorError::TypeError(
                        "sift() first argument must be an object".to_string(),
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

    /// Apply a function (lambda or expression) to values
    ///
    /// This handles both:
    /// 1. Lambda nodes: function($x) { $x * 2 } - binds parameters and evaluates body
    /// 2. Simple expressions: price * 2 - evaluates with values as context
    fn apply_function(&mut self, func_node: &AstNode, values: &[Value], data: &Value) -> Result<Value, EvaluatorError> {
        match func_node {
            AstNode::Lambda { params, body } => {
                // Save current bindings
                let saved_bindings: std::collections::HashMap<String, Value> = params
                    .iter()
                    .filter_map(|param| {
                        self.context.lookup(param).map(|v| (param.clone(), v.clone()))
                    })
                    .collect();

                // Bind lambda parameters to provided values
                for (i, param) in params.iter().enumerate() {
                    if let Some(value) = values.get(i) {
                        self.context.bind(param.clone(), value.clone());
                    } else {
                        return Err(EvaluatorError::EvaluationError(
                            format!("Lambda expects {} parameters, got {}", params.len(), values.len())
                        ));
                    }
                }

                // Evaluate lambda body
                let result = self.evaluate(body, data)?;

                // Restore previous bindings or unbind if parameter was new
                for param in params {
                    if let Some(saved_value) = saved_bindings.get(param) {
                        self.context.bind(param.clone(), saved_value.clone());
                    } else {
                        // Parameter was not previously bound, so remove it
                        self.context.unbind(param);
                    }
                }

                Ok(result)
            }
            _ => {
                // For non-lambda expressions, evaluate with first value as context
                if values.is_empty() {
                    self.evaluate(func_node, data)
                } else {
                    self.evaluate(func_node, &values[0])
                }
            }
        }
    }

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
            // JSONata semantics: comparing with null/undefined returns false
            (Value::Null, _) | (_, Value::Null) => Ok(Value::Bool(false)),
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
            // JSONata semantics: comparing with null/undefined returns false
            (Value::Null, _) | (_, Value::Null) => Ok(Value::Bool(false)),
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
            // JSONata semantics: comparing with null/undefined returns false
            (Value::Null, _) | (_, Value::Null) => Ok(Value::Bool(false)),
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
            // JSONata semantics: comparing with null/undefined returns false
            (Value::Null, _) | (_, Value::Null) => Ok(Value::Bool(false)),
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
    /// Array indexing: array[index]
    fn array_index(&self, array: &Value, index: &Value) -> Result<Value, EvaluatorError> {
        match (array, index) {
            (Value::Array(arr), Value::Number(n)) => {
                let idx = n.as_f64().unwrap() as i64;
                if idx < 0 || idx >= arr.len() as i64 {
                    Ok(Value::Null)
                } else {
                    Ok(arr[idx as usize].clone())
                }
            }
            _ => Err(EvaluatorError::TypeError(
                "Array indexing requires array and number".to_string(),
            )),
        }
    }

    /// Array filtering: array[predicate]
    /// Evaluates the predicate for each item in the array and returns items where predicate is true
    fn array_filter(
        &mut self,
        _lhs_node: &AstNode,
        rhs_node: &AstNode,
        array: &Value,
        _original_data: &Value,
    ) -> Result<Value, EvaluatorError> {
        match array {
            Value::Array(arr) => {
                // Pre-allocate with estimated capacity (assume ~50% will match)
                let mut filtered = Vec::with_capacity(arr.len() / 2);

                for item in arr {
                    // Evaluate the predicate in the context of this array item
                    // The item becomes the new "current context" ($)
                    let predicate_result = self.evaluate(rhs_node, item)?;

                    // Check if the predicate is truthy
                    if self.is_truthy(&predicate_result) {
                        filtered.push(item.clone());
                    }
                }

                Ok(Value::Array(filtered))
            }
            _ => Err(EvaluatorError::TypeError(
                "Array filtering requires an array".to_string(),
            )),
        }
    }

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
        // Note: serde_json represents numbers as f64 internally, so 1.0, 2.0, 3.0
        // This is semantically correct - JSON doesn't distinguish int vs float
        assert_eq!(result, serde_json::json!([1.0, 2.0, 3.0]));
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
        // Note: serde_json represents numbers as f64 - semantically equivalent to int
        assert_eq!(
            result,
            serde_json::json!({"name": "Alice", "age": 30.0})
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
        // Note: Arithmetic operations produce f64 results in JSON
        assert_eq!(result, serde_json::json!(50.0));

        // Test comparison
        let ast = parse("price > 5").unwrap();
        let result = evaluator.evaluate(&ast, &data).unwrap();
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn test_evaluate_dollar_function_uppercase() {
        use crate::parser::parse;
        use serde_json::json;

        let mut evaluator = Evaluator::new();
        let ast = parse(r#"$uppercase("hello")"#).unwrap();
        let result = evaluator.evaluate(&ast, &json!({})).unwrap();
        assert_eq!(result, json!("HELLO"));
    }

    #[test]
    fn test_evaluate_dollar_function_sum() {
        use crate::parser::parse;
        use serde_json::json;

        let mut evaluator = Evaluator::new();
        let ast = parse("$sum([1, 2, 3, 4, 5])").unwrap();
        let result = evaluator.evaluate(&ast, &json!({})).unwrap();
        assert_eq!(result, json!(15.0));
    }

    #[test]
    fn test_evaluate_nested_dollar_functions() {
        use crate::parser::parse;
        use serde_json::json;

        let mut evaluator = Evaluator::new();
        let ast = parse(r#"$length($lowercase("HELLO"))"#).unwrap();
        let result = evaluator.evaluate(&ast, &json!({})).unwrap();
        // length() returns an integer, not a float
        assert_eq!(result, json!(5));
    }

    #[test]
    fn test_array_mapping() {
        use crate::parser::parse;
        use serde_json::json;

        let mut evaluator = Evaluator::new();
        let data = json!({
            "products": [
                {"id": 1, "name": "Laptop", "price": 999.99},
                {"id": 2, "name": "Mouse", "price": 29.99},
                {"id": 3, "name": "Keyboard", "price": 79.99}
            ]
        });

        // Test mapping over array to extract field
        let ast = parse("products.name").unwrap();
        let result = evaluator.evaluate(&ast, &data).unwrap();
        assert_eq!(result, json!(["Laptop", "Mouse", "Keyboard"]));

        // Test mapping over array to extract prices
        let ast = parse("products.price").unwrap();
        let result = evaluator.evaluate(&ast, &data).unwrap();
        assert_eq!(result, json!([999.99, 29.99, 79.99]));

        // Test with $sum function on mapped array
        let ast = parse("$sum(products.price)").unwrap();
        let result = evaluator.evaluate(&ast, &data).unwrap();
        assert_eq!(result, json!(1109.97));
    }
}

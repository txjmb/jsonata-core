// Expression evaluator
// Mirrors jsonata.js from the reference implementation

use crate::ast::{AstNode, BinaryOp, PathStep, Stage};
use crate::parser;
use serde_json::Value;
use thiserror::Error;

/// Create an undefined marker value
/// JSONata distinguishes between undefined (no value) and null (explicit null)
/// We represent undefined with a special marker object
pub fn undefined_value() -> Value {
    serde_json::json!({"__undefined__": true})
}

/// Check if a value is the undefined marker
pub fn is_undefined(value: &Value) -> bool {
    if let Value::Object(obj) = value {
        obj.get("__undefined__").map(|v| v == &Value::Bool(true)).unwrap_or(false)
    } else {
        false
    }
}

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

/// Lambda storage
/// Stores the AST of a lambda function along with its parameters, optional signature,
/// and captured environment for closures
#[derive(Clone, Debug)]
pub struct StoredLambda {
    pub params: Vec<String>,
    pub body: AstNode,
    pub signature: Option<String>,
    /// Captured environment bindings for closures
    pub captured_env: std::collections::HashMap<String, Value>,
}

/// Evaluation context
///
/// Holds variable bindings and other state needed during evaluation
pub struct Context {
    pub(crate) bindings: std::collections::HashMap<String, Value>,
    pub(crate) lambdas: std::collections::HashMap<String, StoredLambda>,
    parent_data: Option<Value>,
}

impl Context {
    pub fn new() -> Self {
        Context {
            bindings: std::collections::HashMap::new(),
            lambdas: std::collections::HashMap::new(),
            parent_data: None,
        }
    }

    pub fn bind(&mut self, name: String, value: Value) {
        self.bindings.insert(name, value);
    }

    pub fn bind_lambda(&mut self, name: String, lambda: StoredLambda) {
        self.lambdas.insert(name, lambda);
    }

    pub fn unbind(&mut self, name: &str) {
        self.bindings.remove(name);
        self.lambdas.remove(name);
    }

    pub fn lookup(&self, name: &str) -> Option<&Value> {
        self.bindings.get(name)
    }

    pub fn lookup_lambda(&self, name: &str) -> Option<&StoredLambda> {
        self.lambdas.get(name)
    }

    pub fn set_parent(&mut self, data: Value) {
        self.parent_data = Some(data);
    }

    pub fn get_parent(&self) -> Option<&Value> {
        self.parent_data.as_ref()
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
    recursion_depth: usize,
    max_recursion_depth: usize,
}

impl Evaluator {
    pub fn new() -> Self {
        Evaluator {
            context: Context::new(),
            recursion_depth: 0,
            max_recursion_depth: 200, // Increased to handle mutual recursion in tests
        }
    }

    pub fn with_context(context: Context) -> Self {
        Evaluator {
            context,
            recursion_depth: 0,
            max_recursion_depth: 200,
        }
    }

    /// Evaluate an AST node against data
    ///
    /// This is the main entry point for evaluation. It sets up the parent context
    /// to be the root data if not already set.
    pub fn evaluate(&mut self, node: &AstNode, data: &Value) -> Result<Value, EvaluatorError> {
        // Set parent context to root data if not already set
        if self.context.get_parent().is_none() {
            self.context.set_parent(data.clone());
        }

        self.evaluate_internal(node, data)
    }

    /// Internal evaluation method
    fn evaluate_internal(&mut self, node: &AstNode, data: &Value) -> Result<Value, EvaluatorError> {
        // Check recursion depth to prevent stack overflow
        self.recursion_depth += 1;
        if self.recursion_depth > self.max_recursion_depth {
            self.recursion_depth -= 1;
            return Err(EvaluatorError::EvaluationError(
                format!("U1001: Stack overflow - maximum recursion depth ({}) exceeded", self.max_recursion_depth)
            ));
        }

        let result = self.evaluate_internal_impl(node, data);

        self.recursion_depth -= 1;
        result
    }

    /// Internal evaluation implementation (separated to allow depth tracking)
    fn evaluate_internal_impl(&mut self, node: &AstNode, data: &Value) -> Result<Value, EvaluatorError> {
        match node {
            // === Literals ===
            AstNode::String(s) => Ok(Value::String(s.clone())),

            // === Field/Property Name ===
            // Name nodes represent field access on the current data
            // (Should normally be wrapped in a Path, but handle direct evaluation too)
            AstNode::Name(field_name) => {
                match data {
                    Value::Object(obj) => Ok(obj.get(field_name).cloned().unwrap_or(Value::Null)),
                    Value::Array(arr) => {
                        // Map over array
                        let mut result = Vec::new();
                        for item in arr {
                            if let Value::Object(obj) = item {
                                if let Some(val) = obj.get(field_name) {
                                    result.push(val.clone());
                                }
                            }
                        }
                        if result.is_empty() {
                            Ok(Value::Null)
                        } else if result.len() == 1 {
                            Ok(result.into_iter().next().unwrap())
                        } else {
                            Ok(Value::Array(result))
                        }
                    }
                    _ => Ok(Value::Null),
                }
            }

            AstNode::Number(n) => {
                // Preserve integer-ness: if the number is a whole number, create an integer Value
                if n.fract() == 0.0 && n.is_finite() && n.abs() < (1i64 << 53) as f64 {
                    // It's a whole number that can be represented as i64
                    Ok(serde_json::json!(*n as i64))
                } else {
                    Ok(serde_json::json!(*n))
                }
            }
            AstNode::Boolean(b) => Ok(Value::Bool(*b)),
            AstNode::Null => Ok(Value::Null),
            AstNode::Undefined => Ok(undefined_value()),
            AstNode::Placeholder => {
                // Placeholders should only appear as function arguments
                // If we reach here, it's an error
                Err(EvaluatorError::EvaluationError(
                    "Placeholder '?' can only be used as a function argument".to_string()
                ))
            }
            AstNode::Regex { pattern, flags } => {
                // Return a regex object as a special JSON value
                // This will be recognized by functions like $split, $match, $replace
                Ok(serde_json::json!({
                    "__jsonata_regex__": true,
                    "pattern": pattern,
                    "flags": flags
                }))
            }

            // === Variables ===
            AstNode::Variable(name) => {
                // Special case: $ alone (empty name) refers to current context
                if name.is_empty() {
                    return Ok(data.clone());
                }

                // First check if this is a stored lambda (user-defined functions)
                if let Some(stored_lambda) = self.context.lookup_lambda(name) {
                    // Return a lambda representation that can be passed to higher-order functions
                    // Include _lambda_id pointing to the stored lambda so it can be found
                    // when captured in closures
                    let lambda_repr = serde_json::json!({
                        "__lambda__": true,
                        "params": stored_lambda.params,
                        "body": format!("{:?}", stored_lambda.body),
                        "_name": name,  // Store the name for later invocation
                        "_lambda_id": name  // Same as name for named lambdas
                    });
                    return Ok(lambda_repr);
                }

                // Check variable bindings BEFORE built-in functions
                // This allows user-defined variables to shadow built-in functions
                // (e.g., $length := $count($arr) should work and not resolve to $length built-in)
                if let Some(value) = self.context.lookup(name) {
                    return Ok(value.clone());
                }

                // Check if this is a built-in function reference (only if not shadowed)
                if self.is_builtin_function(name) {
                    // Return a marker for built-in functions
                    // This allows built-in functions to be passed to higher-order functions
                    let builtin_repr = serde_json::json!({
                        "__builtin__": true,
                        "_name": name
                    });
                    return Ok(builtin_repr);
                }

                // Undefined variable - return null (undefined in JSONata semantics)
                // This allows expressions like `$not(undefined_var)` to return undefined
                // and comparisons like `3 > $undefined` to return undefined
                Ok(Value::Null)
            }

            // === Parent Variables ===
            AstNode::ParentVariable(name) => {
                // Special case: $$ alone (empty name) refers to parent/root context
                if name.is_empty() {
                    return self.context
                        .get_parent()
                        .cloned()
                        .ok_or_else(|| {
                            EvaluatorError::ReferenceError(
                                "Parent context not available".to_string()
                            )
                        });
                }

                // For $$name, we need to evaluate name against parent context
                // This is similar to $.name but using parent data
                let parent_data = self.context
                    .get_parent()
                    .ok_or_else(|| {
                        EvaluatorError::ReferenceError(
                            "Parent context not available".to_string()
                        )
                    })?;

                // Access field on parent context
                match parent_data {
                    Value::Object(obj) => {
                        Ok(obj.get(name).cloned().unwrap_or(Value::Null))
                    }
                    _ => Ok(Value::Null)
                }
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
                // Array constructor with special handling for range operations
                // Range results are flattened, but other arrays are preserved
                // Null (undefined) values are excluded from the result per JSONata semantics
                let mut result = Vec::new();
                for element in elements {
                    // Check if this element is a range operation
                    let is_range = matches!(
                        element,
                        AstNode::Binary { op: BinaryOp::Range, .. }
                    );

                    let value = self.evaluate_internal(element, data)?;

                    // Skip null (undefined) values in array constructors
                    // Check both explicit null and the undefined marker
                    if matches!(value, Value::Null) || is_undefined(&value) {
                        continue;
                    }

                    if is_range {
                        // Flatten range results
                        match value {
                            Value::Array(arr) => result.extend(arr),
                            _ => result.push(value),
                        }
                    } else {
                        // Preserve other values as-is
                        result.push(value);
                    }
                }
                Ok(Value::Array(result))
            }

            // === Objects ===
            AstNode::Object(pairs) => {
                let mut result = serde_json::Map::new();
                for (key_node, value_node) in pairs {
                    // Evaluate key (must be a string)
                    let key = match self.evaluate_internal(key_node, data)? {
                        Value::String(s) => s,
                        Value::Null => continue, // Skip null keys
                        other => {
                            // Skip undefined keys
                            if is_undefined(&other) {
                                continue;
                            }
                            return Err(EvaluatorError::TypeError(format!(
                                "Object key must be a string, got: {:?}",
                                other
                            )))
                        }
                    };
                    // Evaluate value - skip undefined values, include null
                    let value = self.evaluate_internal(value_node, data)?;
                    // Skip key-value pairs where the value is undefined
                    if is_undefined(&value) {
                        continue;
                    }
                    result.insert(key, value);
                }
                Ok(Value::Object(result))
            }

            // === Object Transform ===
            AstNode::ObjectTransform { input, pattern } => {
                // Evaluate the input expression
                let input_value = self.evaluate_internal(input, data)?;

                // If input is undefined, return undefined (not empty object)
                if is_undefined(&input_value) {
                    return Ok(undefined_value());
                }

                // The object transform groups results by keys
                let mut result = serde_json::Map::new();

                // Handle array input - process each item
                let items = match input_value {
                    Value::Array(ref arr) => arr.clone(),
                    Value::Null => return Ok(Value::Null),
                    other => vec![other],
                };

                for item in items {
                    for (key_node, value_node) in pattern {
                        // Evaluate key with current item as context
                        let key = match self.evaluate_internal(key_node, &item)? {
                            Value::String(s) => s,
                            Value::Null => continue, // Skip null keys
                            other => {
                                // Skip undefined keys
                                if is_undefined(&other) {
                                    continue;
                                }
                                return Err(EvaluatorError::TypeError(format!(
                                    "Object key must be a string, got: {:?}",
                                    other
                                )))
                            }
                        };

                        // Evaluate value with current item as context
                        let value = self.evaluate_internal(value_node, &item)?;

                        // Skip undefined values
                        if is_undefined(&value) {
                            continue;
                        }

                        // If key already exists, merge values into array
                        if let Some(existing) = result.get_mut(&key) {
                            match existing {
                                Value::Array(arr) => {
                                    if !matches!(value, Value::Null) {
                                        arr.push(value);
                                    }
                                }
                                _ => {
                                    let old_value = existing.clone();
                                    *existing = Value::Array(vec![old_value, value]);
                                }
                            }
                        } else {
                            result.insert(key, value);
                        }
                    }
                }

                Ok(Value::Object(result))
            }

            // === Function Calls ===
            AstNode::Function { name, args, is_builtin } => {
                self.evaluate_function_call(name, args, *is_builtin, data)
            }

            // === Conditional Expressions ===
            AstNode::Conditional {
                condition,
                then_branch,
                else_branch,
            } => {
                let condition_value = self.evaluate_internal(condition, data)?;
                if self.is_truthy(&condition_value) {
                    self.evaluate_internal(then_branch, data)
                } else if let Some(else_branch) = else_branch {
                    self.evaluate_internal(else_branch, data)
                } else {
                    // No else branch - return undefined (not null)
                    // This allows $map to filter out results from conditionals without else
                    Ok(undefined_value())
                }
            }

            // === Block Expressions ===
            AstNode::Block(expressions) => {
                // Blocks create a new scope - save current bindings
                let saved_bindings = self.context.bindings.clone();
                let saved_lambdas = self.context.lambdas.clone();

                let mut result = Value::Null;
                for expr in expressions {
                    result = self.evaluate_internal(expr, data)?;
                }

                // Restore original bindings after block completes
                self.context.bindings = saved_bindings;
                self.context.lambdas = saved_lambdas;

                Ok(result)
            }

            // === Lambda Functions ===
            AstNode::Lambda { params, body, signature } => {
                // Lambda functions are first-class values (closures)
                // They capture the current environment for later invocation
                //
                // Store the lambda with captured environment for closure support
                // Generate a unique name for anonymous lambdas
                let lambda_id = format!("__lambda_{}_{:p}", params.len(), body.as_ref());

                let stored_lambda = StoredLambda {
                    params: params.clone(),
                    body: (**body).clone(),
                    signature: signature.clone(),
                    captured_env: self.capture_current_environment(),
                };
                self.context.bind_lambda(lambda_id.clone(), stored_lambda);

                let mut lambda_obj = serde_json::json!({
                    "__lambda__": true,
                    "params": params,
                    "body": format!("{:?}", body),
                    "_lambda_id": lambda_id  // Reference to stored lambda with captured env
                });

                // Add signature if present
                if let Some(sig) = signature {
                    lambda_obj["signature"] = serde_json::json!(sig);
                }

                Ok(lambda_obj)
            }

            // === Wildcard ===
            AstNode::Wildcard => {
                // Wildcard in path expressions - collect all values from current object
                match data {
                    Value::Object(obj) => {
                        let mut result = Vec::new();
                        for value in obj.values() {
                            // Flatten arrays into the result
                            match value {
                                Value::Array(arr) => result.extend(arr.clone()),
                                _ => result.push(value.clone()),
                            }
                        }
                        Ok(Value::Array(result))
                    }
                    Value::Array(arr) => {
                        // For arrays, wildcard returns all elements
                        Ok(Value::Array(arr.clone()))
                    }
                    _ => Ok(Value::Null),
                }
            }

            // === Descendant ===
            AstNode::Descendant => {
                // Descendant operator - recursively traverse all nested values
                let descendants = self.collect_descendants(data);
                if descendants.is_empty() {
                    Ok(Value::Null) // No descendants means undefined
                } else {
                    Ok(Value::Array(descendants))
                }
            }

            // === Predicate ===
            AstNode::Predicate(pred_expr) => {
                // Predicates should only appear in path expressions
                // If we get here, something is wrong with the AST
                Err(EvaluatorError::EvaluationError(
                    "Predicate can only be used in path expressions".to_string()
                ))
            }

            // === Array Grouping ===
            AstNode::ArrayGroup(elements) => {
                // Same as Array but used in path contexts to prevent flattening
                let mut result = Vec::new();
                for element in elements {
                    let value = self.evaluate_internal(element, data)?;
                    result.push(value);
                }
                Ok(Value::Array(result))
            }

            // === Function Application ===
            AstNode::FunctionApplication(expr) => {
                // Function application should only appear in path expressions
                // If we get here, something is wrong with the AST
                Err(EvaluatorError::EvaluationError(
                    "Function application can only be used in path expressions".to_string()
                ))
            }

            // === Sort ===
            AstNode::Sort { input, terms } => {
                // Sort operator - evaluate input then sort by terms
                let value = self.evaluate_internal(input, data)?;
                self.evaluate_sort(&value, terms)
            }

            // === Transform ===
            AstNode::Transform { location, update, delete } => {
                // Transform operator: |location|update[,delete]|
                // Creates a function that transforms objects

                // Check if $ is bound (meaning we're being invoked as a lambda)
                if self.context.lookup("$").is_some() {
                    // Execute the transformation
                    self.execute_transform(location, update, delete.as_deref(), data)
                } else {
                    // Return a lambda representation
                    // The transform will be executed when the lambda is invoked
                    let transform_lambda = StoredLambda {
                        params: vec!["$".to_string()],
                        body: AstNode::Transform {
                            location: location.clone(),
                            update: update.clone(),
                            delete: delete.clone(),
                        },
                        signature: None,
                        captured_env: std::collections::HashMap::new(),
                    };

                    // Store with a generated unique name
                    let lambda_name = format!("__transform_{:p}", &*location);
                    self.context.bind_lambda(lambda_name, transform_lambda);

                    // Return lambda marker
                    Ok(Value::String("<lambda>".to_string()))
                }
            }
        }
    }

    /// Apply stages (filters/predicates) to a value during field extraction
    /// Non-array values are wrapped in an array before filtering (JSONata semantics)
    /// This matches the JavaScript reference where stages apply to sequences
    fn apply_stages(&mut self, value: Value, stages: &[Stage]) -> Result<Value, EvaluatorError> {
        // Wrap non-arrays in an array for filtering (JSONata semantics)
        let mut result = match value {
            Value::Array(arr) => Value::Array(arr),
            Value::Null => return Ok(Value::Null), // Null passes through unchanged
            other => Value::Array(vec![other]),
        };

        for stage in stages {
            match stage {
                Stage::Filter(predicate_expr) => {
                    // When applying stages, use stage-specific predicate logic
                    result = self.evaluate_predicate_as_stage(&result, predicate_expr)?;
                }
            }
        }
        Ok(result)
    }

    /// Evaluate a predicate as a stage during field extraction
    /// This has different semantics than standalone predicates:
    /// - Maps index operations over arrays of extracted values
    fn evaluate_predicate_as_stage(&mut self, current: &Value, predicate: &AstNode) -> Result<Value, EvaluatorError> {
        // Special case: empty brackets [] (represented as Boolean(true))
        if matches!(predicate, AstNode::Boolean(true)) {
            return match current {
                Value::Array(arr) => Ok(Value::Array(arr.clone())),
                Value::Null => Ok(Value::Null),
                other => Ok(Value::Array(vec![other.clone()])),
            };
        }

        match current {
            Value::Array(arr) => {
                // For stages: if we have an array of values (from field extraction),
                // apply the predicate to each value if appropriate

                // Check if predicate is a numeric index
                if let AstNode::Number(n) = predicate {
                    // Check if this is an array of arrays (extracted array fields)
                    let is_array_of_arrays = arr.iter().any(|item| matches!(item, Value::Array(_)));

                    if !is_array_of_arrays {
                        // Simple values: just index normally
                        return self.array_index(current, &serde_json::json!(n));
                    }

                    // Array of arrays: map index access over each extracted array
                    let mut result = Vec::new();
                    for item in arr {
                        match item {
                            Value::Array(_) => {
                                let indexed = self.array_index(item, &serde_json::json!(n))?;
                                if !matches!(indexed, Value::Null) {
                                    result.push(indexed);
                                }
                            }
                            _ => {
                                if *n == 0.0 {
                                    result.push(item.clone());
                                }
                            }
                        }
                    }
                    return Ok(Value::Array(result));
                }

                // Try to evaluate the predicate to see if it's a numeric index
                // If evaluation succeeds and yields a number, use it as an index
                // If evaluation fails (e.g., comparison error), treat as filter
                match self.evaluate_internal(predicate, current) {
                    Ok(Value::Number(n)) => {
                        let n_val = n.as_f64().unwrap();
                        let is_array_of_arrays = arr.iter().any(|item| matches!(item, Value::Array(_)));

                        if !is_array_of_arrays {
                            let pred_result = serde_json::json!(n_val);
                            return self.array_index(current, &pred_result);
                        }

                        // Array of arrays: map index access
                        let mut result = Vec::new();
                        let pred_result = serde_json::json!(n_val);
                        for item in arr {
                            match item {
                                Value::Array(_) => {
                                    let indexed = self.array_index(item, &pred_result)?;
                                    if !matches!(indexed, Value::Null) {
                                        result.push(indexed);
                                    }
                                }
                                _ => {
                                    if n_val == 0.0 {
                                        result.push(item.clone());
                                    }
                                }
                            }
                        }
                        return Ok(Value::Array(result));
                    }
                    Ok(_) => {
                        // Evaluated successfully but not a number - might be a filter
                        // Fall through to filter logic
                    }
                    Err(_) => {
                        // Evaluation failed - it's likely a filter expression
                        // Fall through to filter logic
                    }
                }

                // It's a filter expression
                let mut filtered = Vec::new();
                for item in arr {
                    let item_result = self.evaluate_internal(predicate, item)?;
                    if self.is_truthy(&item_result) {
                        filtered.push(item.clone());
                    }
                }
                Ok(Value::Array(filtered))
            }
            Value::Null => {
                // Null: return null
                Ok(Value::Null)
            }
            other => {
                // Non-array values: treat as single-element conceptual array
                // For numeric predicates: index 0 returns the value, other indices return null
                // For boolean predicates: if truthy, return value; if falsy, return null

                // Check if predicate is a numeric index
                if let AstNode::Number(n) = predicate {
                    // Index 0 returns the value, other indices return null
                    if *n == 0.0 {
                        return Ok(other.clone());
                    } else {
                        return Ok(Value::Null);
                    }
                }

                // Try to evaluate the predicate to see if it's a numeric index
                match self.evaluate_internal(predicate, other) {
                    Ok(Value::Number(n)) => {
                        // Index 0 returns the value, other indices return null
                        if n.as_f64().unwrap_or(0.0) == 0.0 {
                            Ok(other.clone())
                        } else {
                            Ok(Value::Null)
                        }
                    }
                    Ok(pred_result) => {
                        // Boolean filter: return value if truthy, null if falsy
                        if self.is_truthy(&pred_result) {
                            Ok(other.clone())
                        } else {
                            Ok(Value::Null)
                        }
                    }
                    Err(e) => Err(e),
                }
            }
        }
    }

    /// Evaluate a path expression (e.g., foo.bar.baz)
    fn evaluate_path(&mut self, steps: &[PathStep], data: &Value) -> Result<Value, EvaluatorError> {
        // Avoid cloning by using references and only cloning when necessary
        if steps.is_empty() {
            return Ok(data.clone());
        }

        // Fast path: single field access on object
        // This is a very common pattern, so optimize it
        if steps.len() == 1 {
            if let AstNode::Name(field_name) = &steps[0].node {
                return match data {
                    Value::Object(obj) => {
                        Ok(obj.get(field_name).cloned().unwrap_or(Value::Null))
                    }
                    Value::Array(arr) => {
                        // Array mapping: extract field from each element
                        let mut result = Vec::new();
                        for item in arr {
                            match item {
                                Value::Object(obj) => {
                                    let val = obj.get(field_name).cloned().unwrap_or(Value::Null);
                                    // Flatten array values, push scalars (matching multi-step path behavior)
                                    if !matches!(val, Value::Null) {
                                        match val {
                                            Value::Array(arr_val) => result.extend(arr_val),
                                            other => result.push(other),
                                        }
                                    }
                                }
                                Value::Array(inner_arr) => {
                                    // Recursively map over nested array
                                    let nested_result = self.evaluate_path(&[PathStep::new(AstNode::Name(field_name.clone()))], &Value::Array(inner_arr.clone()))?;
                                    // Add nested result to our results
                                    match nested_result {
                                        Value::Array(nested) => {
                                            // Flatten nested arrays from recursive mapping
                                            result.extend(nested);
                                        }
                                        Value::Null => {}, // Skip nulls from nested arrays
                                        other => result.push(other),
                                    }
                                }
                                _ => {}, // Skip non-object items
                            }
                        }

                        // Return array result
                        // JSONata singleton unwrapping: if we have exactly one result,
                        // unwrap it (even if it's an array)
                        if result.is_empty() {
                            Ok(Value::Null)
                        } else if result.len() == 1 {
                            Ok(result.into_iter().next().unwrap())
                        } else {
                            Ok(Value::Array(result))
                        }
                    }
                    _ => Ok(Value::Null),
                };
            }
        }

        // Track whether we did array mapping (for singleton unwrapping)
        let mut did_array_mapping = false;

        // For the first step, work with a reference
        let mut current: Value = match &steps[0].node {
            AstNode::Wildcard => {
                // Wildcard as first step
                match data {
                    Value::Object(obj) => {
                        let mut result = Vec::new();
                        for value in obj.values() {
                            // Flatten arrays into the result
                            match value {
                                Value::Array(arr) => result.extend(arr.clone()),
                                _ => result.push(value.clone()),
                            }
                        }
                        Value::Array(result)
                    }
                    Value::Array(arr) => Value::Array(arr.clone()),
                    _ => Value::Null,
                }
            }
            AstNode::Descendant => {
                // Descendant as first step
                let descendants = self.collect_descendants(data);
                Value::Array(descendants)
            }
            AstNode::ParentVariable(name) => {
                // Parent variable as first step
                let parent_data = self.context
                    .get_parent()
                    .ok_or_else(|| {
                        EvaluatorError::ReferenceError(
                            "Parent context not available".to_string()
                        )
                    })?;

                if name.is_empty() {
                    // $$ alone returns parent context
                    parent_data.clone()
                } else {
                    // $$field accesses field on parent
                    match parent_data {
                        Value::Object(obj) => {
                            obj.get(name).cloned().unwrap_or(Value::Null)
                        }
                        _ => Value::Null
                    }
                }
            }
            AstNode::Name(field_name) => {
                // Field/property access - get the stages for this step
                let stages = &steps[0].stages;

                match data {
                    Value::Object(obj) => {
                        let val = obj.get(field_name).cloned().unwrap_or(Value::Null);
                        // Apply any stages to the extracted value
                        if !stages.is_empty() {
                            self.apply_stages(val, stages)?
                        } else {
                            val
                        }
                    }
                    Value::Array(arr) => {
                        // Array mapping: extract field from each element and apply stages
                        let mut result = Vec::new();
                        for item in arr {
                            match item {
                                Value::Object(obj) => {
                                    let val = obj.get(field_name).cloned().unwrap_or(Value::Null);
                                    if !matches!(val, Value::Null) {
                                        if !stages.is_empty() {
                                            // Apply stages to the extracted value
                                            let processed_val = self.apply_stages(val, stages)?;
                                            // Stages always return an array (or null); extend results
                                            match processed_val {
                                                Value::Array(arr) => result.extend(arr),
                                                Value::Null => {}, // Skip nulls from stage application
                                                other => result.push(other), // Shouldn't happen, but handle it
                                            }
                                        } else {
                                            // No stages: flatten arrays, push scalars
                                            match val {
                                                Value::Array(arr) => result.extend(arr),
                                                other => result.push(other),
                                            }
                                        }
                                    }
                                }
                                Value::Array(inner_arr) => {
                                    // Recursively map over nested array
                                    let nested_result = self.evaluate_path(&[steps[0].clone()], &Value::Array(inner_arr.clone()))?;
                                    match nested_result {
                                        Value::Array(nested) => result.extend(nested),
                                        Value::Null => {}, // Skip nulls from nested arrays
                                        other => result.push(other),
                                    }
                                }
                                _ => {}, // Skip non-object items
                            }
                        }
                        Value::Array(result)
                    }
                    Value::Null => Value::Null,
                    // Accessing field on non-object returns undefined (not an error)
                    _ => undefined_value(),
                }
            }
            AstNode::String(string_literal) => {
                // String literal in path context - evaluate as literal and apply stages
                // This handles cases like "Red"[true] where "Red" is a literal, not a field access
                let stages = &steps[0].stages;
                let val = Value::String(string_literal.clone());

                if !stages.is_empty() {
                    // Apply stages (predicates) to the string literal
                    let result = self.apply_stages(val, stages)?;
                    // Unwrap single-element arrays back to scalar
                    // (string literals with predicates should return scalar or null, not arrays)
                    match result {
                        Value::Array(arr) if arr.len() == 1 => arr.into_iter().next().unwrap(),
                        Value::Array(arr) if arr.is_empty() => Value::Null,
                        other => other,
                    }
                } else {
                    val
                }
            }
            AstNode::Predicate(pred_expr) => {
                // Predicate as first step
                self.evaluate_predicate(data, pred_expr)?
            }
            _ => {
                // Complex first step - evaluate it
                self.evaluate_path_step(&steps[0].node, data, data)?
            }
        };

        // Process remaining steps
        for (idx, step) in steps[1..].iter().enumerate() {
            current = match &step.node {
                AstNode::Wildcard => {
                    // Wildcard in path
                    match &current {
                        Value::Object(obj) => {
                            let mut result = Vec::new();
                            for value in obj.values() {
                                // Flatten arrays into the result
                                match value {
                                    Value::Array(arr) => result.extend(arr.clone()),
                                    _ => result.push(value.clone()),
                                }
                            }
                            Value::Array(result)
                        }
                        Value::Array(arr) => {
                            // Map wildcard over array
                            let mut all_values = Vec::new();
                            for item in arr {
                                match item {
                                    Value::Object(obj) => {
                                        for value in obj.values() {
                                            // Flatten arrays
                                            match value {
                                                Value::Array(arr) => all_values.extend(arr.clone()),
                                                _ => all_values.push(value.clone()),
                                            }
                                        }
                                    }
                                    Value::Array(inner) => {
                                        all_values.extend(inner.clone());
                                    }
                                    _ => {}
                                }
                            }
                            Value::Array(all_values)
                        }
                        _ => Value::Null,
                    }
                }
                AstNode::Descendant => {
                    // Descendant in path
                    match &current {
                        Value::Array(arr) => {
                            // Collect descendants from all array elements
                            let mut all_descendants = Vec::new();
                            for item in arr {
                                all_descendants.extend(self.collect_descendants(item));
                            }
                            Value::Array(all_descendants)
                        }
                        _ => {
                            // Collect descendants from current value
                            let descendants = self.collect_descendants(&current);
                            Value::Array(descendants)
                        }
                    }
                }
                AstNode::Name(field_name) => {
                    // Navigate into object field or map over array, applying stages
                    let stages = &step.stages;

                    match &current {
                        Value::Object(obj) => {
                            let val = obj.get(field_name).cloned().unwrap_or(Value::Null);
                            // Apply stages if present
                            if !stages.is_empty() {
                                self.apply_stages(val, stages)?
                            } else {
                                val
                            }
                        }
                        Value::Array(arr) => {
                            // Array mapping: extract field from each element and apply stages
                            did_array_mapping = true; // Track that we did array mapping
                            let mut result = Vec::new();

                            for item in arr {
                                match item {
                                    Value::Object(obj) => {
                                        let val = obj.get(field_name).cloned().unwrap_or(Value::Null);

                                        if !matches!(val, Value::Null) {
                                            if !stages.is_empty() {
                                                // Apply stages to the extracted value
                                                let processed_val = self.apply_stages(val, stages)?;
                                                // Stages always return an array (or null); extend results
                                                match processed_val {
                                                    Value::Array(arr) => result.extend(arr),
                                                    Value::Null => {}, // Skip nulls from stage application
                                                    other => result.push(other), // Shouldn't happen, but handle it
                                                }
                                            } else {
                                                // No stages: flatten arrays, push scalars
                                                match val {
                                                    Value::Array(arr) => result.extend(arr),
                                                    other => result.push(other),
                                                }
                                            }
                                        }
                                    }
                                    Value::Array(inner_arr) => {
                                        // Recursively map over nested array
                                        let nested_result = self.evaluate_path(&[step.clone()], item)?;
                                        match nested_result {
                                            Value::Array(nested) => result.extend(nested),
                                            Value::Null => {}, // Skip nulls from nested arrays
                                            other => result.push(other),
                                        }
                                    }
                                    _ => {}, // Skip non-object items
                                }
                            }

                            Value::Array(result)
                        }
                        Value::Null => Value::Null,
                        // Accessing field on non-object returns undefined (not an error)
                        _ => undefined_value(),
                    }
                }
                AstNode::String(string_literal) => {
                    // String literal as a path step - evaluate as literal and apply stages
                    let stages = &step.stages;
                    let val = Value::String(string_literal.clone());

                    if !stages.is_empty() {
                        // Apply stages (predicates) to the string literal
                        let result = self.apply_stages(val, stages)?;
                        // Unwrap single-element arrays back to scalar
                        match result {
                            Value::Array(arr) if arr.len() == 1 => arr.into_iter().next().unwrap(),
                            Value::Array(arr) if arr.is_empty() => Value::Null,
                            other => other,
                        }
                    } else {
                        val
                    }
                }
                AstNode::Predicate(pred_expr) => {
                    // Predicate in path - filter or index into current value
                    self.evaluate_predicate(&current, pred_expr)?
                }
                AstNode::ArrayGroup(elements) => {
                    // Array grouping: map expression over array but keep results grouped
                    // .[expr] means evaluate expr for each array element
                    match &current {
                        Value::Array(arr) => {
                            let mut result = Vec::new();
                            for item in arr {
                                // For each array item, evaluate all elements and collect results
                                let mut group_values = Vec::new();
                                for element in elements {
                                    let value = self.evaluate_internal(element, item)?;
                                    // Flatten the value into group_values
                                    match value {
                                        Value::Array(arr) => group_values.extend(arr),
                                        other => group_values.push(other),
                                    }
                                }
                                // Each array element gets its own sub-array with all values
                                result.push(Value::Array(group_values));
                            }
                            Value::Array(result)
                        }
                        _ => {
                            // For non-arrays, just evaluate the array constructor normally
                            let mut result = Vec::new();
                            for element in elements {
                                let value = self.evaluate_internal(element, &current)?;
                                result.push(value);
                            }
                            Value::Array(result)
                        }
                    }
                }
                AstNode::FunctionApplication(expr) => {
                    // Function application: map expr over the current value
                    // .(expr) means evaluate expr for each element, with $ bound to that element
                    // Null/undefined results are filtered out
                    match &current {
                        Value::Array(arr) => {
                            let mut result = Vec::new();
                            for item in arr {
                                // Save the current $ binding
                                let saved_dollar = self.context.lookup("$").cloned();

                                // Bind $ to the current item
                                self.context.bind("$".to_string(), item.clone());

                                // Evaluate the expression in the context of this item
                                let value = self.evaluate_internal(expr, item)?;

                                // Restore the previous $ binding
                                if let Some(saved) = saved_dollar {
                                    self.context.bind("$".to_string(), saved);
                                } else {
                                    self.context.unbind("$");
                                }

                                // Only include non-null/undefined values
                                if !matches!(value, Value::Null) && !is_undefined(&value) {
                                    result.push(value);
                                }
                            }
                            // Singleton sequence unwrapping
                            if result.len() == 1 {
                                result.into_iter().next().unwrap()
                            } else {
                                Value::Array(result)
                            }
                        }
                        _ => {
                            // For non-arrays, bind $ and evaluate
                            let saved_dollar = self.context.lookup("$").cloned();
                            self.context.bind("$".to_string(), current.clone());

                            let value = self.evaluate_internal(expr, &current)?;

                            if let Some(saved) = saved_dollar {
                                self.context.bind("$".to_string(), saved);
                            } else {
                                self.context.unbind("$");
                            }

                            value
                        }
                    }
                }
                AstNode::Sort { input, terms } => {
                    // Sort as a path step - the input should be evaluated in the context of current
                    // But since Sort has its own input from being a postfix operator,
                    // we need to evaluate the input relative to current, then sort
                    // Actually, when Sort appears in a path, its input is what comes before the ^
                    // So we should sort 'current' by the terms
                    self.evaluate_sort(&current, terms)?
                }
                // Handle complex path steps (e.g., computed properties, object construction)
                _ => self.evaluate_path_step(&step.node, &current, data)?
            };
        }

        // JSONata singleton unwrapping: singleton results are unwrapped when we did array operations
        // BUT NOT when there's an explicit array-keeping operation like [] (empty predicate)

        // Check for explicit array-keeping operations
        // Empty predicate [] can be represented as:
        // 1. Predicate(Boolean(true)) as a path step node
        // 2. Stage::Filter(Boolean(true)) as a stage
        let has_explicit_array_keep = steps.iter().any(|step| {
            // Check if the step itself is an empty predicate
            if let AstNode::Predicate(pred) = &step.node {
                if matches!(**pred, AstNode::Boolean(true)) {
                    return true;
                }
            }
            // Check if any stage is an empty predicate
            step.stages.iter().any(|stage| {
                if let crate::ast::Stage::Filter(pred) = stage {
                    matches!(**pred, AstNode::Boolean(true))
                } else {
                    false
                }
            })
        });

        // Unwrap when:
        // 1. Original data was an array (array mapping scenario), OR
        // 2. Any step has stages (predicates, sorts, etc.) which are array operations, OR
        // 3. We did array mapping during step evaluation (tracked via did_array_mapping flag)
        // BUT NOT when there's an explicit array-keeping operation
        let should_unwrap = !has_explicit_array_keep && (
            matches!(data, Value::Array(_)) ||
            steps.iter().any(|step| !step.stages.is_empty()) ||
            did_array_mapping
        );

        let result = match &current {
            // Empty arrays become null/undefined
            Value::Array(arr) if arr.is_empty() => Value::Null,
            // Unwrap singleton arrays when appropriate
            Value::Array(arr) if arr.len() == 1 && should_unwrap => {
                arr[0].clone()
            }
            // Keep arrays otherwise
            _ => current
        };

        Ok(result)
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
                            self.evaluate_internal(step, item)
                        })
                        .collect();
                    Ok(Value::Array(mapped?))
                }
                _ => unreachable!(),
            }
        } else {
            // Special case: function calls on arrays should be mapped
            if matches!(step, AstNode::Function { .. }) && matches!(current, Value::Array(_)) {
                // Map the function call over each array element
                if let Value::Array(arr) = current {
                    let mut result = Vec::new();
                    for item in arr {
                        let value = self.evaluate_internal(step, item)?;
                        result.push(value);
                    }
                    return Ok(Value::Array(result));
                }
            }

            // For certain operations (Binary, Function calls, Variables, Arrays, Objects, Sort, Blocks), the step evaluates to a new value
            // rather than being used to index/access the current value
            // e.g., items[price > 50] where [price > 50] is a filter operation
            // or $x.price where $x is a variable binding
            // or [0..9] where it's an array constructor
            // or $^(field) where it's a sort operator
            // or (expr).field where (expr) is a block that evaluates to a value
            if matches!(step, AstNode::Binary { .. } | AstNode::Function { .. } | AstNode::Variable(_) | AstNode::Array(_) | AstNode::Object(_) | AstNode::Sort { .. } | AstNode::Block(_)) {
                // Evaluate the step in the context of original_data and return the result directly
                return self.evaluate_internal(step, original_data);
            }

            // Standard path step evaluation for indexing/accessing current value
            let step_value = self.evaluate_internal(step, original_data)?;
            Ok(match (current, &step_value) {
                (Value::Object(obj), Value::String(key)) => {
                    obj.get(key).cloned().unwrap_or(Value::Null)
                }
                (Value::Array(arr), Value::Number(n)) => {
                    let index = n.as_f64().unwrap() as i64;
                    let len = arr.len() as i64;

                    // Handle negative indexing (offset from end)
                    let actual_idx = if index < 0 {
                        len + index
                    } else {
                        index
                    };

                    if actual_idx < 0 || actual_idx >= len {
                        Value::Null
                    } else {
                        arr[actual_idx as usize].clone()
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

        // Special handling for coalescing operator (??)
        // Returns right side if left is undefined (produces no value)
        // Note: literal null is a value, so it's NOT replaced
        if op == BinaryOp::Coalesce {
            // Try to evaluate the left side
            return match self.evaluate_internal(lhs, data) {
                Ok(value) => {
                    // Successfully evaluated to a value (even if it's null)
                    // Check if LHS is a literal null - keep it (null is a value, not undefined)
                    if matches!(lhs, AstNode::Null) {
                        Ok(value)
                    }
                    // For paths and variables, null means undefined - use RHS
                    else if matches!(value, Value::Null) &&
                       (matches!(lhs, AstNode::Path { .. }) ||
                        matches!(lhs, AstNode::String(_)) ||
                        matches!(lhs, AstNode::Variable(_))) {
                        self.evaluate_internal(rhs, data)
                    } else {
                        Ok(value)
                    }
                }
                Err(_) => {
                    // Evaluation failed (e.g., undefined variable) - use RHS
                    self.evaluate_internal(rhs, data)
                }
            };
        }

        // Special handling for default operator (?:)
        // Returns right side if left is falsy or a non-value (like a function)
        if op == BinaryOp::Default {
            let left = self.evaluate_internal(lhs, data)?;
            if self.is_truthy_for_default(&left) {
                return Ok(left);
            }
            return self.evaluate_internal(rhs, data);
        }

        // Special handling for chain/pipe operator (~>)
        // Pipes the LHS result to the RHS function as the first argument
        // e.g., expr ~> func(arg2) becomes func(expr, arg2)
        if op == BinaryOp::ChainPipe {
            // Handle regex on RHS - treat as $match(lhs, regex)
            if let AstNode::Regex { pattern, flags } = rhs {
                // Evaluate LHS
                let lhs_value = self.evaluate_internal(lhs, data)?;
                // Do regex match inline
                return match lhs_value {
                    Value::String(s) => {
                        // Build the regex
                        let case_insensitive = flags.contains('i');
                        let regex_pattern = if case_insensitive {
                            format!("(?i){}", pattern)
                        } else {
                            pattern.clone()
                        };
                        match regex::Regex::new(&regex_pattern) {
                            Ok(re) => {
                                if let Some(m) = re.find(&s) {
                                    // Return match object
                                    let mut result = serde_json::Map::new();
                                    result.insert("match".to_string(), Value::String(m.as_str().to_string()));
                                    result.insert("start".to_string(), serde_json::json!(m.start()));
                                    result.insert("end".to_string(), serde_json::json!(m.end()));

                                    // Capture groups
                                    let mut groups = Vec::new();
                                    for cap in re.captures_iter(&s).take(1) {
                                        for i in 1..cap.len() {
                                            if let Some(c) = cap.get(i) {
                                                groups.push(Value::String(c.as_str().to_string()));
                                            }
                                        }
                                    }
                                    if !groups.is_empty() {
                                        result.insert("groups".to_string(), Value::Array(groups));
                                    }

                                    Ok(Value::Object(result))
                                } else {
                                    Ok(Value::Null)
                                }
                            }
                            Err(e) => Err(EvaluatorError::EvaluationError(format!("Invalid regex: {}", e))),
                        }
                    }
                    Value::Null => Ok(Value::Null),
                    _ => Err(EvaluatorError::TypeError(
                        "Left side of ~> /regex/ must be a string".to_string(),
                    )),
                };
            }

            // Handle different RHS types
            match rhs {
                AstNode::Function { name, args, is_builtin } => {
                    // RHS is a function call
                    // Build the complete args list with LHS (piped value) as first argument
                    let mut all_args = vec![lhs.clone()];
                    all_args.extend_from_slice(args);
                    return self.evaluate_function_call(name, &all_args, *is_builtin, data);
                }
                AstNode::Variable(var_name) => {
                    // RHS is a function reference (no parens)
                    // e.g., $average($tempReadings) ~> $round
                    let all_args = vec![lhs.clone()];
                    return self.evaluate_function_call(var_name, &all_args, true, data);
                }
                AstNode::Binary { op: BinaryOp::ChainPipe, .. } => {
                    // RHS is another chain pipe - evaluate LHS first, then pipe through RHS
                    // e.g., x ~> (f1 ~> f2) => (x ~> f1) ~> f2
                    let lhs_value = self.evaluate_internal(lhs, data)?;
                    return self.evaluate_internal(rhs, &lhs_value);
                }
                AstNode::Transform { .. } => {
                    // RHS is a transform - invoke it with LHS as input
                    // Evaluate LHS first
                    let lhs_value = self.evaluate_internal(lhs, data)?;

                    // Bind $ to the LHS value, then evaluate the transform
                    let saved_binding = self.context.lookup("$").cloned();
                    self.context.bind("$".to_string(), lhs_value.clone());

                    let result = self.evaluate_internal(rhs, data);

                    // Restore $ binding
                    if let Some(saved) = saved_binding {
                        self.context.bind("$".to_string(), saved);
                    } else {
                        self.context.unbind("$");
                    }

                    return result;
                }
                AstNode::Lambda { params, body, signature } => {
                    // RHS is a lambda - invoke it with LHS as argument
                    let lhs_value = self.evaluate_internal(lhs, data)?;
                    return self.invoke_lambda(params, body, signature.as_ref(), &[lhs_value], data);
                }
                _ => {
                    return Err(EvaluatorError::TypeError(
                        "Right side of ~> must be a function call or function reference".to_string(),
                    ));
                }
            }
        }

        // Special handling for variable binding (:=)
        if op == BinaryOp::ColonEqual {
            // Extract variable name from LHS
            let var_name = match lhs {
                AstNode::Variable(name) => name.clone(),
                _ => {
                    return Err(EvaluatorError::TypeError(
                        "Left side of := must be a variable".to_string(),
                    ))
                }
            };

            // Check if RHS is a lambda - store it specially
            if let AstNode::Lambda { params, body, signature } = rhs {
                // Store the lambda AST for later invocation
                // Capture current environment for closure support
                let captured_env = self.capture_current_environment();
                let stored_lambda = StoredLambda {
                    params: params.clone(),
                    body: (**body).clone(),
                    signature: signature.clone(),
                    captured_env,
                };
                self.context.bind_lambda(var_name, stored_lambda);

                // Return a lambda marker value
                let lambda_repr = serde_json::json!({
                    "__lambda__": true,
                    "params": params,
                });
                return Ok(lambda_repr);
            }

            // Check if RHS is a function composition (ChainPipe between function references)
            // e.g., $uppertrim := $trim ~> $uppercase
            if let AstNode::Binary { op: BinaryOp::ChainPipe, lhs: chain_lhs, rhs: chain_rhs } = rhs {
                // Create a lambda: function($) { ($ ~> firstFunc) ~> restOfChain }
                // The original chain is $trim ~> $uppercase (left-associative)
                // We want to create: ($ ~> $trim) ~> $uppercase
                let param_name = "$".to_string();

                // First create $ ~> $trim
                let first_pipe = AstNode::Binary {
                    op: BinaryOp::ChainPipe,
                    lhs: Box::new(AstNode::Variable(param_name.clone())),
                    rhs: chain_lhs.clone(),
                };

                // Then wrap with ~> $uppercase (or the rest of the chain)
                let composed_body = AstNode::Binary {
                    op: BinaryOp::ChainPipe,
                    lhs: Box::new(first_pipe),
                    rhs: chain_rhs.clone(),
                };

                let stored_lambda = StoredLambda {
                    params: vec![param_name],
                    body: composed_body,
                    signature: None,
                    captured_env: self.capture_current_environment(),
                };
                self.context.bind_lambda(var_name.clone(), stored_lambda);

                // Return a lambda marker value
                let lambda_repr = serde_json::json!({
                    "__lambda__": true,
                    "params": ["$"],
                });
                return Ok(lambda_repr);
            }

            // Evaluate the RHS
            let value = self.evaluate_internal(rhs, data)?;

            // Check if the value is a lambda with captured environment (closure)
            // If so, we need to copy the stored lambda to the new variable name
            if let Value::Object(map) = &value {
                if map.contains_key("__lambda__") {
                    if let Some(Value::String(lambda_id)) = map.get("_lambda_id") {
                        // This is a lambda with captured environment
                        // Copy the stored lambda to the new variable name
                        if let Some(stored) = self.context.lookup_lambda(lambda_id).cloned() {
                            self.context.bind_lambda(var_name.clone(), stored);
                        }
                    }
                }
            }

            // Bind the variable in the current scope
            // Even if the value is undefined (null), we create the binding
            // This allows inner scopes to shadow outer variables
            self.context.bind(var_name, value.clone());

            // Return the value
            return Ok(value);
        }

        // Special handling for 'In' operator - check for array filtering
        // Must evaluate lhs first to determine if this is array filtering
        if op == BinaryOp::In {
            let left = self.evaluate_internal(lhs, data)?;

            // Check if this is array filtering: array[predicate]
            if matches!(left, Value::Array(_)) {
                // Try evaluating rhs in current context to see if it's a simple index
                let right_result = self.evaluate_internal(rhs, data);

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

        // Special handling for logical operators (short-circuit evaluation)
        if op == BinaryOp::And {
            let left = self.evaluate_internal(lhs, data)?;
            if !self.is_truthy(&left) {
                // Short-circuit: if left is falsy, return false without evaluating right
                return Ok(Value::Bool(false));
            }
            let right = self.evaluate_internal(rhs, data)?;
            return Ok(Value::Bool(self.is_truthy(&right)));
        }

        if op == BinaryOp::Or {
            let left = self.evaluate_internal(lhs, data)?;
            if self.is_truthy(&left) {
                // Short-circuit: if left is truthy, return true without evaluating right
                return Ok(Value::Bool(true));
            }
            let right = self.evaluate_internal(rhs, data)?;
            return Ok(Value::Bool(self.is_truthy(&right)));
        }

        // Check if operands are explicit null literals (vs undefined from variables)
        let left_is_explicit_null = matches!(lhs, AstNode::Null);
        let right_is_explicit_null = matches!(rhs, AstNode::Null);

        // Standard evaluation: evaluate both operands
        let left = self.evaluate_internal(lhs, data)?;
        let right = self.evaluate_internal(rhs, data)?;

        match op {
            // === Arithmetic Operations ===
            BinaryOp::Add => self.add(&left, &right, left_is_explicit_null, right_is_explicit_null),
            BinaryOp::Subtract => self.subtract(&left, &right, left_is_explicit_null, right_is_explicit_null),
            BinaryOp::Multiply => self.multiply(&left, &right, left_is_explicit_null, right_is_explicit_null),
            BinaryOp::Divide => self.divide(&left, &right, left_is_explicit_null, right_is_explicit_null),
            BinaryOp::Modulo => self.modulo(&left, &right, left_is_explicit_null, right_is_explicit_null),

            // === Comparison Operations ===
            BinaryOp::Equal => Ok(Value::Bool(self.equals(&left, &right))),
            BinaryOp::NotEqual => Ok(Value::Bool(!self.equals(&left, &right))),
            BinaryOp::LessThan => self.less_than(&left, &right, left_is_explicit_null, right_is_explicit_null),
            BinaryOp::LessThanOrEqual => self.less_than_or_equal(&left, &right, left_is_explicit_null, right_is_explicit_null),
            BinaryOp::GreaterThan => self.greater_than(&left, &right, left_is_explicit_null, right_is_explicit_null),
            BinaryOp::GreaterThanOrEqual => self.greater_than_or_equal(&left, &right, left_is_explicit_null, right_is_explicit_null),

            // === Logical Operations ===
            // Note: And/Or are handled above with short-circuit evaluation
            BinaryOp::And | BinaryOp::Or => {
                unreachable!("And/Or should be handled earlier with short-circuit evaluation")
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

            // === Variable Binding ===
            // Note: ColonEqual is handled earlier in evaluate_binary_op as a special case
            BinaryOp::ColonEqual => {
                unreachable!("ColonEqual should be handled earlier in evaluate_binary_op")
            }

            // === Coalescing Operator ===
            // Note: Coalesce is handled earlier in evaluate_binary_op as a special case
            BinaryOp::Coalesce => {
                unreachable!("Coalesce should be handled earlier in evaluate_binary_op")
            }

            // === Default Operator ===
            // Note: Default is handled earlier in evaluate_binary_op as a special case
            BinaryOp::Default => {
                unreachable!("Default should be handled earlier in evaluate_binary_op")
            }

            // === Chain/Pipe Operator ===
            // Note: ChainPipe is handled earlier in evaluate_binary_op as a special case
            BinaryOp::ChainPipe => {
                unreachable!("ChainPipe should be handled earlier in evaluate_binary_op")
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

        let value = self.evaluate_internal(operand, data)?;

        match op {
            UnaryOp::Negate => match value {
                // undefined returns undefined
                Value::Null => Ok(Value::Null),
                Value::Number(n) => Ok(serde_json::json!(-n.as_f64().unwrap())),
                _ => Err(EvaluatorError::TypeError(format!(
                    "D1002: Cannot negate non-number value"
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
        is_builtin: bool,
        data: &Value,
    ) -> Result<Value, EvaluatorError> {
        use crate::functions;

        // Check for partial application (any argument is a Placeholder)
        let has_placeholder = args.iter().any(|arg| matches!(arg, AstNode::Placeholder));
        if has_placeholder {
            return self.create_partial_application(name, args, is_builtin, data);
        }

        // First, check if this "function name" is actually a stored lambda
        if let Some(stored_lambda) = self.context.lookup_lambda(name).cloned() {
            // This is a lambda stored in a variable - invoke it
            // Evaluate the arguments
            let mut evaluated_args = Vec::new();
            for arg in args {
                evaluated_args.push(self.evaluate_internal(arg, data)?);
            }

            // Invoke with captured environment
            let captured_env = if stored_lambda.captured_env.is_empty() {
                None
            } else {
                Some(&stored_lambda.captured_env)
            };
            return self.invoke_lambda_with_env(
                &stored_lambda.params,
                &stored_lambda.body,
                stored_lambda.signature.as_ref(),
                &evaluated_args,
                data,
                captured_env,
            );
        }

        // Check if this variable holds a lambda value (JSON object with __lambda__)
        // This handles closures where a lambda was captured in the environment
        if let Some(value) = self.context.lookup(name).cloned() {
            if let Value::Object(ref map) = value {
                if map.contains_key("__lambda__") {
                    // This is a lambda value - look up the stored lambda by its ID
                    if let Some(Value::String(lambda_id)) = map.get("_lambda_id") {
                        if let Some(stored_lambda) = self.context.lookup_lambda(lambda_id).cloned() {
                            // Evaluate the arguments
                            let mut evaluated_args = Vec::new();
                            for arg in args {
                                evaluated_args.push(self.evaluate_internal(arg, data)?);
                            }

                            // Invoke with captured environment
                            let captured_env = if stored_lambda.captured_env.is_empty() {
                                None
                            } else {
                                Some(&stored_lambda.captured_env)
                            };
                            return self.invoke_lambda_with_env(
                                &stored_lambda.params,
                                &stored_lambda.body,
                                stored_lambda.signature.as_ref(),
                                &evaluated_args,
                                data,
                                captured_env,
                            );
                        }
                    }
                }
            }
        }

        // If the function was called without $ prefix and it's not a stored lambda,
        // it's an error (unknown function without $ prefix)
        if !is_builtin && name != "__lambda__" {
            return Err(EvaluatorError::ReferenceError(format!(
                "Unknown function: {}",
                name
            )));
        }

        // Special handling for $exists function
        // It needs to know if the argument is explicit null vs undefined
        if name == "exists" && args.len() == 1 {
            let arg = &args[0];

            // Check if it's an explicit null literal
            if matches!(arg, AstNode::Null) {
                return Ok(Value::Bool(true)); // Explicit null exists
            }

            // Check if it's a function reference
            if let AstNode::Variable(var_name) = arg {
                // Check if it's a built-in function name
                let builtin_functions = [
                    "abs", "append", "average", "boolean", "ceil", "contains", "count",
                    "exists", "filter", "floor", "join", "keys", "length", "lowercase",
                    "map", "max", "merge", "min", "not", "number", "pad", "power",
                    "reduce", "replace", "reverse", "round", "shuffle", "sort", "split",
                    "sqrt", "string", "substring", "substringAfter", "substringBefore",
                    "sum", "trim", "uppercase", "zip", "each", "sift", "type", "assert",
                    "error", "single", "lookup", "spread", "formatNumber", "formatBase",
                    "toMillis", "fromMillis", "now", "millis", "parseInteger",
                    "encodeUrl", "encodeUrlComponent", "decodeUrl", "decodeUrlComponent",
                    "base64encode", "base64decode", "eval"
                ];

                if builtin_functions.contains(&var_name.as_str()) {
                    return Ok(Value::Bool(true)); // Built-in function exists
                }

                // Check if it's a stored lambda
                if self.context.lookup_lambda(var_name).is_some() {
                    return Ok(Value::Bool(true)); // Lambda exists
                }

                // Check if the variable is defined
                if let Some(_) = self.context.lookup(var_name) {
                    return Ok(Value::Bool(true)); // Variable is defined (even if null)
                } else {
                    return Ok(Value::Bool(false)); // Variable is undefined
                }
            }

            // For other expressions, evaluate and check if non-null
            let value = self.evaluate_internal(arg, data)?;
            return Ok(Value::Bool(!matches!(value, Value::Null)));
        }

        // Check if any arguments are undefined variables or undefined paths
        // Functions like $not() should return undefined when given undefined values
        for arg in args {
            // Check for undefined variable (e.g., $undefined_var)
            if let AstNode::Variable(var_name) = arg {
                // Skip built-in function names - they're function references, not undefined variables
                if !var_name.is_empty() && !self.is_builtin_function(var_name) && self.context.lookup(var_name).is_none() {
                    // Undefined variable - for functions that should propagate undefined
                    if ["not", "boolean", "length", "number", "uppercase", "lowercase", "substring", "substringBefore", "substringAfter", "string"].contains(&name) {
                        return Ok(Value::Null); // Return undefined
                    }
                }
            }
            // Note: AstNode::String represents string literals (e.g., "hello"), not field accesses.
            // Field accesses are represented as AstNode::Path. String literals should never
            // be checked for undefined propagation.
            // Check for Path expressions that evaluate to undefined
            if let AstNode::Path { steps } = arg {
                // For paths that evaluate to null, we need to determine if it's because:
                // 1. A field doesn't exist (undefined) - should propagate as undefined
                // 2. A field exists with value null - should throw T0410
                //
                // We can distinguish these by checking if the path is accessing a field
                // that doesn't exist on an object vs one that has an explicit null value.
                if let Ok(Value::Null) = self.evaluate_internal(arg, data) {
                    // Path evaluated to null - now check if it's truly undefined
                    // For single-step paths, check if the field exists
                    if steps.len() == 1 {
                        if let AstNode::String(_field_name) = &steps[0].node {
                            match data {
                                Value::Object(obj) => {
                                    if !obj.contains_key(_field_name) {
                                        // Field doesn't exist - return undefined
                                        if ["not", "boolean", "length", "number", "uppercase", "lowercase", "substring", "substringBefore", "substringAfter", "string"].contains(&name) {
                                            return Ok(Value::Null);
                                        }
                                    }
                                    // Field exists with value null - continue to throw T0410
                                }
                                Value::Null => {
                                    // Trying to access field on null data - return undefined
                                    if ["not", "boolean", "length", "number", "uppercase", "lowercase", "substring", "substringBefore", "substringAfter", "string"].contains(&name) {
                                        return Ok(Value::Null);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    // For multi-step paths, check if any intermediate step failed
                    else if steps.len() > 1 {
                        // Evaluate each step to find where it breaks
                        let mut current = data;
                        let mut failed_due_to_missing_field = false;

                        for (i, step) in steps.iter().enumerate() {
                            if let AstNode::Name(field_name) = &step.node {
                                match current {
                                    Value::Object(obj) => {
                                        if let Some(val) = obj.get(field_name) {
                                            current = val;
                                        } else {
                                            // Field doesn't exist
                                            failed_due_to_missing_field = true;
                                            break;
                                        }
                                    }
                                    Value::Array(_) => {
                                        // Array access - evaluate normally
                                        break;
                                    }
                                    Value::Null => {
                                        // Hit null in the middle of the path
                                        if i > 0 {
                                            // Previous field had null value - not undefined
                                            failed_due_to_missing_field = false;
                                        }
                                        break;
                                    }
                                    _ => break,
                                }
                            }
                        }

                        if failed_due_to_missing_field {
                            if ["not", "boolean", "length", "number", "uppercase", "lowercase", "substring", "substringBefore", "substringAfter", "string"].contains(&name) {
                                return Ok(Value::Null);
                            }
                        }
                    }
                }
            }
        }

        // Evaluate all arguments
        let mut evaluated_args = Vec::new();
        for arg in args {
            evaluated_args.push(self.evaluate_internal(arg, data)?);
        }

        // JSONata feature: when a function is called with no arguments but expects
        // at least one, use the current context value (data) as the implicit first argument
        // This also applies when functions expecting N arguments receive N-1 arguments,
        // in which case the context value becomes the first argument
        let context_functions_zero_arg = ["string", "number", "boolean", "uppercase", "lowercase"];
        let context_functions_missing_first = ["substringBefore", "substringAfter", "contains", "split", "replace"];

        if evaluated_args.is_empty() && context_functions_zero_arg.contains(&name) {
            // Use the current context value as the implicit argument
            evaluated_args.push(data.clone());
        } else if evaluated_args.len() == 1 && context_functions_missing_first.contains(&name) {
            // These functions expect 2+ arguments, but received 1
            // Only insert context if it's a compatible type (string for string functions)
            // Otherwise, let the function throw T0411 for wrong argument count
            if matches!(data, Value::String(_)) {
                evaluated_args.insert(0, data.clone());
            }
        }

        // Special handling for $string() with no explicit arguments
        // After context insertion, check if the argument is null (undefined context)
        if name == "string" && args.is_empty() && !evaluated_args.is_empty() {
            if matches!(evaluated_args[0], Value::Null) {
                // Context was null/undefined, so return undefined
                return Ok(Value::Null);
            }
        }

        // Call built-in functions
        match name {
            // String functions
            "string" => {
                if evaluated_args.len() > 2 {
                    return Err(EvaluatorError::EvaluationError(
                        "string() takes at most 2 arguments".to_string(),
                    ));
                }

                let prettify = if evaluated_args.len() == 2 {
                    match &evaluated_args[1] {
                        Value::Bool(b) => Some(*b),
                        _ => return Err(EvaluatorError::TypeError(
                            "string() prettify parameter must be a boolean".to_string(),
                        )),
                    }
                } else {
                    None
                };

                functions::string::string(&evaluated_args[0], prettify)
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
                        "T0410: Argument 1 of function length does not match function signature".to_string(),
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
                        "T0410: Argument 1 of function uppercase does not match function signature".to_string(),
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
                        "T0410: Argument 1 of function lowercase does not match function signature".to_string(),
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
                if evaluated_args.len() > 1 {
                    return Err(EvaluatorError::TypeError(
                        "T0410: Argument 2 of function number does not match function signature".to_string(),
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
                    Value::Null => Ok(Value::Null),
                    Value::Array(arr) => functions::numeric::sum(arr)
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
                    // Non-array values are treated as single-element arrays
                    other => functions::numeric::sum(&[other.clone()])
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
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
                    Value::Null => Ok(serde_json::json!(0)), // undefined counts as 0
                    Value::Array(arr) => functions::array::count(arr)
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Ok(serde_json::json!(1)), // Non-array value counts as 1
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
                                    "T0410: Argument 3 of function substring does not match function signature".to_string(),
                                )),
                            }
                        } else {
                            None
                        };
                        functions::string::substring(s, start.as_f64().unwrap() as i64, length)
                            .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
                    }
                    (Value::String(_), _) => Err(EvaluatorError::TypeError(
                        "T0410: Argument 2 of function substring does not match function signature".to_string(),
                    )),
                    _ => Err(EvaluatorError::TypeError(
                        "T0410: Argument 1 of function substring does not match function signature".to_string(),
                    )),
                }
            }
            "substringBefore" => {
                if evaluated_args.len() != 2 {
                    return Err(EvaluatorError::TypeError(
                        "T0411: Context value is not a compatible type with argument 2 of function substringBefore".to_string(),
                    ));
                }
                match (&evaluated_args[0], &evaluated_args[1]) {
                    (Value::String(s), Value::String(sep)) => {
                        functions::string::substring_before(s, sep)
                            .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
                    }
                    (Value::String(_), _) => Err(EvaluatorError::TypeError(
                        "T0410: Argument 2 of function substringBefore does not match function signature".to_string(),
                    )),
                    _ => Err(EvaluatorError::TypeError(
                        "T0410: Argument 1 of function substringBefore does not match function signature".to_string(),
                    )),
                }
            }
            "substringAfter" => {
                if evaluated_args.len() != 2 {
                    return Err(EvaluatorError::TypeError(
                        "T0411: Context value is not a compatible type with argument 2 of function substringAfter".to_string(),
                    ));
                }
                match (&evaluated_args[0], &evaluated_args[1]) {
                    (Value::String(s), Value::String(sep)) => {
                        functions::string::substring_after(s, sep)
                            .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
                    }
                    (Value::String(_), _) => Err(EvaluatorError::TypeError(
                        "T0410: Argument 2 of function substringAfter does not match function signature".to_string(),
                    )),
                    _ => Err(EvaluatorError::TypeError(
                        "T0410: Argument 1 of function substringAfter does not match function signature".to_string(),
                    )),
                }
            }
            "pad" => {
                if evaluated_args.is_empty() || evaluated_args.len() > 3 {
                    return Err(EvaluatorError::EvaluationError(
                        "pad() requires 2 or 3 arguments".to_string(),
                    ));
                }

                // First argument: string to pad
                let string = match &evaluated_args[0] {
                    Value::String(s) => s.clone(),
                    Value::Null => return Ok(Value::Null),
                    _ => return Err(EvaluatorError::TypeError(
                        "pad() first argument must be a string".to_string(),
                    )),
                };

                // Second argument: width (negative = left pad, positive = right pad)
                let width = match &evaluated_args.get(1) {
                    Some(Value::Number(n)) => n.as_f64().unwrap() as i32,
                    _ => return Err(EvaluatorError::TypeError(
                        "pad() second argument must be a number".to_string(),
                    )),
                };

                // Third argument: padding string (optional, defaults to space)
                let pad_string = match evaluated_args.get(2) {
                    Some(Value::String(s)) if !s.is_empty() => s.clone(),
                    _ => " ".to_string(),
                };

                let abs_width = width.abs() as usize;
                // Count Unicode characters (code points), not bytes
                let char_count = string.chars().count();

                if char_count >= abs_width {
                    // String is already long enough
                    return Ok(Value::String(string));
                }

                let padding_needed = abs_width - char_count;

                // Build padding by repeating the pad_string
                let mut padding = String::new();
                let pad_string_len = pad_string.chars().count();
                for i in 0..padding_needed {
                    let pad_index = i % pad_string_len;
                    padding.push(pad_string.chars().nth(pad_index).unwrap());
                }

                let result = if width < 0 {
                    // Left pad (negative width)
                    format!("{}{}", padding, string)
                } else {
                    // Right pad (positive width)
                    format!("{}{}", string, padding)
                };

                Ok(Value::String(result))
            }

            "trim" => {
                if evaluated_args.is_empty() {
                    return Ok(Value::Null); // undefined
                }
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "trim() requires at most 1 argument".to_string(),
                    ));
                }
                match &evaluated_args[0] {
                    Value::Null => Ok(Value::Null),
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
                if matches!(evaluated_args[0], Value::Null) {
                    return Ok(Value::Null);
                }
                match &evaluated_args[0] {
                    Value::String(s) => {
                        functions::string::contains(s, &evaluated_args[1])
                            .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
                    }
                    _ => Err(EvaluatorError::TypeError(
                        "contains() requires a string as the first argument".to_string(),
                    )),
                }
            }
            "split" => {
                if evaluated_args.len() < 2 || evaluated_args.len() > 3 {
                    return Err(EvaluatorError::EvaluationError(
                        "split() requires 2 or 3 arguments".to_string(),
                    ));
                }
                if matches!(evaluated_args[0], Value::Null) {
                    return Ok(Value::Null);
                }
                match &evaluated_args[0] {
                    Value::String(s) => {
                        let limit = if evaluated_args.len() == 3 {
                            match &evaluated_args[2] {
                                Value::Number(n) => {
                                    let f = n.as_f64().unwrap();
                                    // Negative limit is an error
                                    if f < 0.0 {
                                        return Err(EvaluatorError::EvaluationError(
                                            "D3020: Third argument of split function must be a positive number".to_string(),
                                        ));
                                    }
                                    // Floor the value for non-integer limits
                                    Some(f.floor() as usize)
                                }
                                _ => return Err(EvaluatorError::TypeError(
                                    "split() limit must be a number".to_string(),
                                )),
                            }
                        } else {
                            None
                        };
                        functions::string::split(s, &evaluated_args[1], limit)
                            .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
                    }
                    _ => Err(EvaluatorError::TypeError(
                        "split() requires a string as the first argument".to_string(),
                    )),
                }
            }
            "join" => {
                // Special case: if first arg is undefined, return undefined
                // But if separator (2nd arg) is undefined, use empty string (default)
                if evaluated_args.is_empty() {
                    return Err(EvaluatorError::TypeError(
                        "T0410: Argument 1 of function $join does not match function signature".to_string()
                    ));
                }
                if matches!(evaluated_args[0], Value::Null) {
                    return Ok(Value::Null);
                }

                // Signature: <a<s>s?:s> - array of strings, optional separator, returns string
                // The signature handles coercion and validation
                use crate::signature::Signature;

                let signature = Signature::parse("<a<s>s?:s>")
                    .map_err(|e| EvaluatorError::EvaluationError(format!("Invalid signature: {}", e)))?;

                let coerced_args = match signature.validate_and_coerce(&evaluated_args) {
                    Ok(args) => args,
                    Err(crate::signature::SignatureError::UndefinedArgument) => {
                        // This can happen if the separator is undefined
                        // In that case, just validate the first arg and use default separator
                        let sig_first_arg = Signature::parse("<a<s>:a<s>>")
                            .map_err(|e| EvaluatorError::EvaluationError(format!("Invalid signature: {}", e)))?;

                        match sig_first_arg.validate_and_coerce(&evaluated_args[0..1]) {
                            Ok(args) => args,
                            Err(crate::signature::SignatureError::ArrayTypeMismatch { index, expected }) => {
                                return Err(EvaluatorError::TypeError(
                                    format!("T0412: Argument {} of function $join must be an array of {}", index, expected)
                                ));
                            }
                            Err(e) => {
                                return Err(EvaluatorError::TypeError(format!("Signature validation failed: {}", e)));
                            }
                        }
                    }
                    Err(crate::signature::SignatureError::ArgumentTypeMismatch { index, expected }) => {
                        return Err(EvaluatorError::TypeError(
                            format!("T0410: Argument {} of function $join does not match function signature (expected {})", index, expected)
                        ));
                    }
                    Err(crate::signature::SignatureError::ArrayTypeMismatch { index, expected }) => {
                        return Err(EvaluatorError::TypeError(
                            format!("T0412: Argument {} of function $join must be an array of {}", index, expected)
                        ));
                    }
                    Err(e) => {
                        return Err(EvaluatorError::TypeError(format!("Signature validation failed: {}", e)));
                    }
                };

                // After coercion, first arg is guaranteed to be an array of strings
                match &coerced_args[0] {
                    Value::Array(arr) => {
                        let separator = if coerced_args.len() == 2 {
                            match &coerced_args[1] {
                                Value::String(s) => Some(s.as_str()),
                                Value::Null => None,  // Undefined separator -> use empty string
                                _ => None,  // Signature should have validated this
                            }
                        } else {
                            None  // No separator provided -> use empty string
                        };
                        functions::string::join(arr, separator)
                            .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
                    }
                    Value::Null => Ok(Value::Null),
                    _ => unreachable!("Signature validation should ensure array type"),
                }
            }
            "replace" => {
                if evaluated_args.len() < 3 || evaluated_args.len() > 4 {
                    return Err(EvaluatorError::EvaluationError(
                        "replace() requires 3 or 4 arguments".to_string(),
                    ));
                }
                if matches!(evaluated_args[0], Value::Null) {
                    return Ok(Value::Null);
                }
                match (&evaluated_args[0], &evaluated_args[2]) {
                    (Value::String(s), Value::String(replacement)) => {
                        let limit = if evaluated_args.len() == 4 {
                            match &evaluated_args[3] {
                                Value::Number(n) => {
                                    let lim_f64 = n.as_f64().unwrap();
                                    if lim_f64 < 0.0 {
                                        return Err(EvaluatorError::EvaluationError(
                                            format!("D3011: Limit must be non-negative, got {}", lim_f64)
                                        ));
                                    }
                                    Some(lim_f64 as usize)
                                }
                                _ => return Err(EvaluatorError::TypeError(
                                    "replace() limit must be a number".to_string(),
                                )),
                            }
                        } else {
                            None
                        };
                        functions::string::replace(s, &evaluated_args[1], replacement, limit)
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
                // Check for undefined
                if is_undefined(&evaluated_args[0]) {
                    return Ok(undefined_value());
                }
                match &evaluated_args[0] {
                    Value::Null => Ok(Value::Null),
                    Value::Array(arr) => functions::numeric::max(arr)
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
                    Value::Number(_) => Ok(evaluated_args[0].clone()), // Single number returns itself
                    _ => Err(EvaluatorError::TypeError(
                        "max() requires an array or number argument".to_string(),
                    )),
                }
            }
            "min" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "min() requires exactly 1 argument".to_string(),
                    ));
                }
                // Check for undefined
                if is_undefined(&evaluated_args[0]) {
                    return Ok(undefined_value());
                }
                match &evaluated_args[0] {
                    Value::Null => Ok(Value::Null),
                    Value::Array(arr) => functions::numeric::min(arr)
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
                    Value::Number(_) => Ok(evaluated_args[0].clone()), // Single number returns itself
                    _ => Err(EvaluatorError::TypeError(
                        "min() requires an array or number argument".to_string(),
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
                    Value::Null => Ok(Value::Null),
                    Value::Array(arr) => functions::numeric::average(arr)
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
                    Value::Number(_) => Ok(evaluated_args[0].clone()), // Single number returns itself
                    _ => Err(EvaluatorError::TypeError(
                        "average() requires an array or number argument".to_string(),
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
                    Value::Null => Ok(Value::Null),
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
                    Value::Null => Ok(Value::Null),
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
                    Value::Null => Ok(Value::Null),
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
                    Value::Null => Ok(Value::Null),
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
                    Value::Null => Ok(Value::Null),
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
                if matches!(evaluated_args[0], Value::Null) {
                    return Ok(Value::Null);
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
            "formatNumber" => {
                if evaluated_args.len() < 2 || evaluated_args.len() > 3 {
                    return Err(EvaluatorError::EvaluationError(
                        "formatNumber() requires 2 or 3 arguments".to_string(),
                    ));
                }
                if matches!(evaluated_args[0], Value::Null) {
                    return Ok(Value::Null);
                }
                match (&evaluated_args[0], &evaluated_args[1]) {
                    (Value::Number(num), Value::String(picture)) => {
                        let options = if evaluated_args.len() == 3 {
                            Some(&evaluated_args[2])
                        } else {
                            None
                        };
                        functions::numeric::format_number(num.as_f64().unwrap(), picture, options)
                            .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
                    }
                    _ => Err(EvaluatorError::TypeError(
                        "formatNumber() requires a number and a string".to_string(),
                    )),
                }
            }
            "formatBase" => {
                if evaluated_args.is_empty() || evaluated_args.len() > 2 {
                    return Err(EvaluatorError::EvaluationError(
                        "formatBase() requires 1 or 2 arguments".to_string(),
                    ));
                }
                // Handle undefined input
                if matches!(evaluated_args[0], Value::Null) {
                    return Ok(Value::Null);
                }
                match &evaluated_args[0] {
                    Value::Number(num) => {
                        let radix = if evaluated_args.len() == 2 {
                            match &evaluated_args[1] {
                                Value::Number(r) => Some(r.as_f64().unwrap().trunc() as i64),
                                _ => return Err(EvaluatorError::TypeError(
                                    "formatBase() radix must be a number".to_string(),
                                )),
                            }
                        } else {
                            None
                        };
                        functions::numeric::format_base(num.as_f64().unwrap(), radix)
                            .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
                    }
                    _ => Err(EvaluatorError::TypeError(
                        "formatBase() requires a number".to_string(),
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
                // Handle null/undefined arguments
                let first = &evaluated_args[0];
                let second = &evaluated_args[1];

                // If second arg is null, return first as-is (no change)
                if matches!(second, Value::Null) {
                    return Ok(first.clone());
                }

                // If first arg is null, return second as-is (appending to nothing gives second)
                if matches!(first, Value::Null) {
                    return Ok(second.clone());
                }

                // Convert both to arrays if needed, then append
                let arr = match first {
                    Value::Array(a) => a.clone(),
                    other => vec![other.clone()], // Wrap non-array in array
                };

                functions::array::append(&arr, second)
                    .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
            }
            "reverse" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "reverse() requires exactly 1 argument".to_string(),
                    ));
                }
                match &evaluated_args[0] {
                    Value::Null => Ok(Value::Null), // undefined returns undefined
                    Value::Array(arr) => functions::array::reverse(arr)
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Err(EvaluatorError::TypeError(
                        "reverse() requires an array argument".to_string(),
                    )),
                }
            }
            "shuffle" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "shuffle() requires exactly 1 argument".to_string(),
                    ));
                }
                if matches!(evaluated_args[0], Value::Null) {
                    return Ok(Value::Null);
                }
                match &evaluated_args[0] {
                    Value::Array(arr) => functions::array::shuffle(arr)
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Err(EvaluatorError::TypeError(
                        "shuffle() requires an array argument".to_string(),
                    )),
                }
            }

            "sift" => {
                // $sift(object, function) or $sift(function) - filter object by predicate
                if evaluated_args.is_empty() || evaluated_args.len() > 2 {
                    return Err(EvaluatorError::EvaluationError(
                        "sift() requires 1 or 2 arguments".to_string(),
                    ));
                }

                // Helper function to sift a single object
                let sift_object = |evaluator: &mut Self, obj: &serde_json::Map<String, Value>, func_node: &AstNode, context_data: &Value| -> Result<Value, EvaluatorError> {
                    let obj_value = Value::Object(obj.clone());
                    let mut result = serde_json::Map::new();
                    for (key, value) in obj.iter() {
                        let pred_result = evaluator.apply_function(
                            func_node,
                            &[value.clone(), Value::String(key.clone()), obj_value.clone()],
                            context_data
                        )?;
                        if evaluator.is_truthy(&pred_result) {
                            result.insert(key.clone(), value.clone());
                        }
                    }
                    Ok(Value::Object(result))
                };

                // Handle partial application - if only 1 arg, use current context as object
                if evaluated_args.len() == 1 {
                    // $sift(function) - use current context data as object
                    match data {
                        Value::Object(o) => {
                            return sift_object(self, o, &args[0], data);
                        }
                        Value::Array(arr) => {
                            // Map sift over each object in the array
                            let mut results = Vec::new();
                            for item in arr {
                                if let Value::Object(o) = item {
                                    let sifted = sift_object(self, o, &args[0], item)?;
                                    results.push(sifted);
                                }
                            }
                            return Ok(Value::Array(results));
                        }
                        Value::Null => return Ok(Value::Null),
                        _ => return Ok(undefined_value()),
                    }
                } else {
                    // $sift(object, function)
                    match &evaluated_args[0] {
                        Value::Object(o) => {
                            return sift_object(self, o, &args[1], data);
                        }
                        Value::Null => return Ok(Value::Null),
                        _ => return Err(EvaluatorError::TypeError(
                            "sift() first argument must be an object".to_string(),
                        )),
                    }
                }
            }

            "zip" => {
                if evaluated_args.is_empty() {
                    return Err(EvaluatorError::EvaluationError(
                        "zip() requires at least 1 argument".to_string(),
                    ));
                }

                // Convert arguments to arrays (wrapping non-arrays in single-element arrays)
                // If any argument is null/undefined, return empty array
                let mut arrays: Vec<Vec<Value>> = Vec::new();
                for arg in &evaluated_args {
                    match arg {
                        Value::Array(arr) => {
                            if arr.is_empty() {
                                // Empty array means result is empty
                                return Ok(Value::Array(vec![]));
                            }
                            arrays.push(arr.clone());
                        }
                        Value::Null => {
                            // Null/undefined means result is empty
                            return Ok(Value::Array(vec![]));
                        }
                        other => {
                            // Wrap non-array values in single-element array
                            arrays.push(vec![other.clone()]);
                        }
                    }
                }

                if arrays.is_empty() {
                    return Ok(Value::Array(vec![]));
                }

                // Find the length of the shortest array
                let min_len = arrays.iter().map(|a| a.len()).min().unwrap_or(0);

                // Zip the arrays together
                let mut result = Vec::with_capacity(min_len);
                for i in 0..min_len {
                    let mut tuple = Vec::with_capacity(arrays.len());
                    for array in &arrays {
                        tuple.push(array[i].clone());
                    }
                    result.push(Value::Array(tuple));
                }

                Ok(Value::Array(result))
            }

            "sort" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(EvaluatorError::EvaluationError(
                        "sort() requires 1 or 2 arguments".to_string(),
                    ));
                }

                // Evaluate the array argument
                let array_value = self.evaluate_internal(&args[0], data)?;

                // Handle undefined input
                if matches!(array_value, Value::Null) {
                    return Ok(Value::Null);
                }

                // Convert non-array to single-element array
                let mut arr = match array_value {
                    Value::Array(arr) => arr,
                    other => vec![other],
                };

                if args.len() == 2 {
                    // Custom comparator function provided
                    // Sort using the comparator: function($a, $b) returns true if $a should come before $b
                    let comparator = &args[1];

                    // Use a simple bubble sort to allow using the comparator function
                    // JSONata comparator: returns true if $a should come AFTER $b
                    let n = arr.len();
                    for i in 0..n {
                        for j in 0..(n - i - 1) {
                            // Apply comparator function with (a, b)
                            let cmp_result = self.apply_function(
                                comparator,
                                &[arr[j].clone(), arr[j + 1].clone()],
                                data
                            )?;

                            // If comparator returns true (a should come AFTER b), swap
                            if self.is_truthy(&cmp_result) {
                                arr.swap(j, j + 1);
                            }
                        }
                    }

                    Ok(Value::Array(arr))
                } else {
                    // Default sort (no comparator)
                    functions::array::sort(&arr)
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
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

                // Helper to unwrap single-element arrays
                let unwrap_single = |keys: Vec<Value>| -> Value {
                    if keys.len() == 1 {
                        keys.into_iter().next().unwrap()
                    } else {
                        Value::Array(keys)
                    }
                };

                match &evaluated_args[0] {
                    Value::Null => Ok(Value::Null),
                    Value::Object(obj) => {
                        // Check if this is a lambda (internal representation)
                        if obj.contains_key("__lambda__") {
                            return Ok(Value::Null);
                        }
                        // Return undefined for empty objects
                        if obj.is_empty() {
                            Ok(Value::Null)
                        } else {
                            let keys: Vec<Value> = obj.keys()
                                .map(|k| Value::String(k.clone()))
                                .collect();
                            Ok(unwrap_single(keys))
                        }
                    }
                    Value::Array(arr) => {
                        // For arrays, collect keys from all objects
                        let mut all_keys = Vec::new();
                        for item in arr {
                            if let Value::Object(obj) = item {
                                // Skip lambda objects
                                if obj.contains_key("__lambda__") {
                                    continue;
                                }
                                for key in obj.keys() {
                                    if !all_keys.contains(&Value::String(key.clone())) {
                                        all_keys.push(Value::String(key.clone()));
                                    }
                                }
                            }
                        }
                        if all_keys.is_empty() {
                            Ok(Value::Null)
                        } else {
                            Ok(unwrap_single(all_keys))
                        }
                    }
                    // Non-object types return undefined
                    _ => Ok(Value::Null),
                }
            }
            "lookup" => {
                if evaluated_args.len() != 2 {
                    return Err(EvaluatorError::EvaluationError(
                        "lookup() requires exactly 2 arguments".to_string(),
                    ));
                }
                if matches!(evaluated_args[0], Value::Null) {
                    return Ok(Value::Null);
                }

                let key = match &evaluated_args[1] {
                    Value::String(k) => k.as_str(),
                    _ => return Err(EvaluatorError::TypeError(
                        "lookup() requires a string key".to_string(),
                    )),
                };

                // Helper function to recursively lookup in values
                fn lookup_recursive(val: &Value, key: &str) -> Vec<Value> {
                    match val {
                        Value::Array(arr) => {
                            let mut results = Vec::new();
                            for item in arr {
                                let nested = lookup_recursive(item, key);
                                results.extend(nested);
                            }
                            results
                        }
                        Value::Object(obj) => {
                            if let Some(v) = obj.get(key) {
                                vec![v.clone()]
                            } else {
                                vec![]
                            }
                        }
                        _ => vec![],
                    }
                }

                let results = lookup_recursive(&evaluated_args[0], key);
                if results.is_empty() {
                    Ok(Value::Null)
                } else if results.len() == 1 {
                    Ok(results[0].clone())
                } else {
                    Ok(Value::Array(results))
                }
            }
            "spread" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "spread() requires exactly 1 argument".to_string(),
                    ));
                }
                match &evaluated_args[0] {
                    Value::Null => Ok(Value::Null),
                    Value::Object(obj) => {
                        // Check if this is a lambda/function - return undefined
                        if obj.get("__lambda__").is_some() {
                            return Ok(undefined_value());
                        }
                        functions::object::spread(obj)
                            .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
                    }
                    Value::Array(arr) => {
                        // Spread each object in the array
                        let mut result = Vec::new();
                        for item in arr {
                            match item {
                                Value::Object(obj) => {
                                    if obj.get("__lambda__").is_some() {
                                        // Skip lambdas in array
                                        continue;
                                    }
                                    let spread_result = functions::object::spread(obj)
                                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))?;
                                    if let Value::Array(spread_items) = spread_result {
                                        result.extend(spread_items);
                                    } else {
                                        result.push(spread_result);
                                    }
                                }
                                // Non-objects in array are returned unchanged
                                other => result.push(other.clone()),
                            }
                        }
                        Ok(Value::Array(result))
                    }
                    // Non-objects are returned unchanged
                    other => Ok(other.clone()),
                }
            }
            "merge" => {
                if evaluated_args.is_empty() {
                    return Err(EvaluatorError::EvaluationError(
                        "merge() requires at least 1 argument".to_string(),
                    ));
                }
                // Handle the case where a single array of objects is passed: $merge([obj1, obj2])
                // vs multiple object arguments: $merge(obj1, obj2)
                if evaluated_args.len() == 1 {
                    match &evaluated_args[0] {
                        Value::Array(arr) => {
                            // Merge array of objects
                            functions::object::merge(arr)
                                .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
                        }
                        Value::Null => Ok(Value::Null), // $merge(undefined) returns undefined
                        Value::Object(_) => {
                            // Single object - just return it
                            Ok(evaluated_args[0].clone())
                        }
                        _ => Err(EvaluatorError::TypeError(
                            "merge() requires objects or an array of objects".to_string(),
                        )),
                    }
                } else {
                    // Multiple arguments - merge them directly
                    functions::object::merge(&evaluated_args)
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
                }
            }

            // Higher-order functions
            "map" => {
                if args.len() != 2 {
                    return Err(EvaluatorError::EvaluationError(
                        "map() requires exactly 2 arguments".to_string(),
                    ));
                }

                // Evaluate the array argument
                let array = self.evaluate_internal(&args[0], data)?;

                match array {
                    Value::Array(arr) => {
                        let arr_value = Value::Array(arr.clone());
                        let mut result = Vec::with_capacity(arr.len());
                        for (index, item) in arr.into_iter().enumerate() {
                            // Apply function with (item, index, array) - callbacks may use any subset
                            let mapped = self.apply_function(
                                &args[1],
                                &[item, Value::Number(serde_json::Number::from(index)), arr_value.clone()],
                                data
                            )?;
                            // Filter out undefined results but keep explicit null (JSONata map semantics)
                            // undefined comes from missing else clause, null is explicit
                            if !is_undefined(&mapped) {
                                result.push(mapped);
                            }
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
                let array = self.evaluate_internal(&args[0], data)?;

                match array {
                    Value::Array(arr) => {
                        let arr_value = Value::Array(arr.clone());
                        let mut result = Vec::with_capacity(arr.len() / 2);
                        for (index, item) in arr.into_iter().enumerate() {
                            // Apply predicate function with (item, index, array)
                            let predicate_result = self.apply_function(
                                &args[1],
                                &[item.clone(), Value::Number(serde_json::Number::from(index)), arr_value.clone()],
                                data
                            )?;
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

                // Check that the callback function has at least 2 parameters
                if let AstNode::Lambda { params, .. } = &args[1] {
                    if params.len() < 2 {
                        return Err(EvaluatorError::EvaluationError(
                            "D3050: The second argument of reduce must be a function with at least two arguments".to_string(),
                        ));
                    }
                } else if let AstNode::Function { name, .. } = &args[1] {
                    // For now, we can't validate built-in function signatures here
                    // But user-defined functions via lambda will be validated above
                    let _ = name; // avoid unused warning
                }

                // Evaluate the array argument
                let array = self.evaluate_internal(&args[0], data)?;

                // Convert single value to array (JSONata reduce accepts single values)
                let arr = match array {
                    Value::Array(arr) => arr,
                    Value::Null => return Ok(Value::Null),
                    single => vec![single],
                };

                if arr.is_empty() {
                    // Return initial value if provided, otherwise null
                    return if args.len() == 3 {
                        self.evaluate_internal(&args[2], data)
                    } else {
                        Ok(Value::Null)
                    };
                }

                // Get initial accumulator
                let mut accumulator = if args.len() == 3 {
                    self.evaluate_internal(&args[2], data)?
                } else {
                    arr[0].clone()
                };

                let start_idx = if args.len() == 3 { 0 } else { 1 };
                let arr_value = Value::Array(arr.clone());

                        // Apply function to each element
                        for (idx, item) in arr[start_idx..].iter().enumerate() {
                            // For reduce, the function receives (accumulator, value, index, array)
                            // Callbacks may use any subset of these parameters
                            let actual_idx = start_idx + idx;
                            accumulator = self.apply_function(
                                &args[1],
                                &[
                                    accumulator.clone(),
                                    item.clone(),
                                    Value::Number(serde_json::Number::from(actual_idx)),
                                    arr_value.clone()
                                ],
                                data
                            )?;
                        }

                Ok(accumulator)
            }

            "single" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(EvaluatorError::EvaluationError(
                        "single() requires 1 or 2 arguments".to_string(),
                    ));
                }

                // Evaluate the array argument
                let array = self.evaluate_internal(&args[0], data)?;

                // Convert to array (wrap single values)
                let arr = match array {
                    Value::Array(arr) => arr,
                    Value::Null => return Ok(Value::Null),
                    other => vec![other],
                };

                if args.len() == 1 {
                    // No predicate - array must have exactly 1 element
                    match arr.len() {
                        0 => Err(EvaluatorError::EvaluationError(
                            "single() argument is empty".to_string(),
                        )),
                        1 => Ok(arr.into_iter().next().unwrap()),
                        count => Err(EvaluatorError::EvaluationError(
                            format!("single() argument has {} values (expected exactly 1)", count),
                        )),
                    }
                } else {
                    // With predicate - find exactly 1 matching element
                    let arr_value = Value::Array(arr.clone());
                    let mut matches = Vec::new();
                    for (index, item) in arr.into_iter().enumerate() {
                        // Apply predicate function with (item, index, array)
                        let predicate_result = self.apply_function(
                            &args[1],
                            &[item.clone(), Value::Number(serde_json::Number::from(index)), arr_value.clone()],
                            data
                        )?;
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
                    (self.evaluate_internal(&args[0], data)?, &args[1])
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

            "each" => {
                // $each(object, function) - iterate over object, applying function to each value/key pair
                // Returns an array of the function results
                if args.is_empty() || args.len() > 2 {
                    return Err(EvaluatorError::EvaluationError(
                        "each() requires 1 or 2 arguments".to_string(),
                    ));
                }

                // Determine which argument is the object and which is the function
                let (obj_value, func_arg) = if args.len() == 1 {
                    // Single argument: use current data as object
                    (data.clone(), &args[0])
                } else {
                    // Two arguments: first is object, second is function
                    (self.evaluate_internal(&args[0], data)?, &args[1])
                };

                match obj_value {
                    Value::Object(obj) => {
                        let mut result = Vec::new();
                        for (key, value) in obj {
                            // Apply function with (value, key) arguments
                            // The callback receives the value as the first argument and key as second
                            let fn_result = self.apply_function(func_arg, &[value.clone(), Value::String(key.clone())], data)?;
                            // Skip undefined results (similar to map behavior)
                            if !matches!(fn_result, Value::Null) && !is_undefined(&fn_result) {
                                result.push(fn_result);
                            }
                        }
                        Ok(Value::Array(result))
                    }
                    Value::Null => Ok(Value::Null),
                    _ => Err(EvaluatorError::TypeError(
                        "each() first argument must be an object".to_string(),
                    )),
                }
            }

            // Boolean and type functions
            "not" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "not() requires exactly 1 argument".to_string(),
                    ));
                }
                // $not(x) returns the logical negation of x
                // null is falsy, so $not(null) = true
                Ok(Value::Bool(!self.is_truthy(&evaluated_args[0])))
            }
            "boolean" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "boolean() requires exactly 1 argument".to_string(),
                    ));
                }
                // Undefined variables are handled by the early check on line 1725
                // Explicit null should convert to false, not undefined
                functions::boolean::boolean(&evaluated_args[0])
                    .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
            }
            "type" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "type() requires exactly 1 argument".to_string(),
                    ));
                }
                // Return type string
                // In JavaScript: $type(undefined) returns undefined, $type(null) returns "null"
                // We use a special marker object to distinguish undefined from null
                match &evaluated_args[0] {
                    Value::Null => Ok(Value::String("null".to_string())), // explicit null
                    Value::Bool(_) => Ok(Value::String("boolean".to_string())),
                    Value::Number(_) => Ok(Value::String("number".to_string())),
                    Value::String(_) => Ok(Value::String("string".to_string())),
                    Value::Array(_) => Ok(Value::String("array".to_string())),
                    Value::Object(obj) => {
                        // Check if this is the undefined marker
                        if is_undefined(&evaluated_args[0]) {
                            Ok(undefined_value()) // undefined returns undefined
                        // Check if this is a function (lambda or built-in)
                        } else if obj.contains_key("__lambda__") || obj.contains_key("__builtin__") {
                            Ok(Value::String("function".to_string()))
                        } else {
                            Ok(Value::String("object".to_string()))
                        }
                    }
                }
            }

            // Encoding/Decoding functions
            "base64encode" => {
                if evaluated_args.is_empty() || matches!(evaluated_args[0], Value::Null) {
                    return Ok(Value::Null);
                }
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "base64encode() requires exactly 1 argument".to_string(),
                    ));
                }
                match &evaluated_args[0] {
                    Value::String(s) => functions::encoding::base64encode(s)
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Err(EvaluatorError::TypeError(
                        "base64encode() requires a string argument".to_string(),
                    )),
                }
            }
            "base64decode" => {
                if evaluated_args.is_empty() || matches!(evaluated_args[0], Value::Null) {
                    return Ok(Value::Null);
                }
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "base64decode() requires exactly 1 argument".to_string(),
                    ));
                }
                match &evaluated_args[0] {
                    Value::String(s) => functions::encoding::base64decode(s)
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Err(EvaluatorError::TypeError(
                        "base64decode() requires a string argument".to_string(),
                    )),
                }
            }
            "encodeUrlComponent" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "encodeUrlComponent() requires exactly 1 argument".to_string(),
                    ));
                }
                if matches!(evaluated_args[0], Value::Null) {
                    return Ok(Value::Null);
                }
                match &evaluated_args[0] {
                    Value::String(s) => functions::encoding::encode_url_component(s)
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Err(EvaluatorError::TypeError(
                        "encodeUrlComponent() requires a string argument".to_string(),
                    )),
                }
            }
            "decodeUrlComponent" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "decodeUrlComponent() requires exactly 1 argument".to_string(),
                    ));
                }
                if matches!(evaluated_args[0], Value::Null) {
                    return Ok(Value::Null);
                }
                match &evaluated_args[0] {
                    Value::String(s) => functions::encoding::decode_url_component(s)
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Err(EvaluatorError::TypeError(
                        "decodeUrlComponent() requires a string argument".to_string(),
                    )),
                }
            }
            "encodeUrl" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "encodeUrl() requires exactly 1 argument".to_string(),
                    ));
                }
                if matches!(evaluated_args[0], Value::Null) {
                    return Ok(Value::Null);
                }
                match &evaluated_args[0] {
                    Value::String(s) => functions::encoding::encode_url(s)
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Err(EvaluatorError::TypeError(
                        "encodeUrl() requires a string argument".to_string(),
                    )),
                }
            }
            "decodeUrl" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "decodeUrl() requires exactly 1 argument".to_string(),
                    ));
                }
                if matches!(evaluated_args[0], Value::Null) {
                    return Ok(Value::Null);
                }
                match &evaluated_args[0] {
                    Value::String(s) => functions::encoding::decode_url(s)
                        .map_err(|e| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Err(EvaluatorError::TypeError(
                        "decodeUrl() requires a string argument".to_string(),
                    )),
                }
            }

            // Control flow functions
            "error" => {
                // $error(message) - throw error with custom message
                if evaluated_args.is_empty() {
                    // No message provided
                    return Err(EvaluatorError::EvaluationError("D3137: $error() function evaluated".to_string()));
                }

                match &evaluated_args[0] {
                    Value::String(s) => {
                        return Err(EvaluatorError::EvaluationError(format!("D3137: {}", s)));
                    }
                    _ => {
                        return Err(EvaluatorError::TypeError(
                            "T0410: Argument 1 of function error does not match function signature".to_string()
                        ));
                    }
                }
            }
            "assert" => {
                // $assert(condition, message) - throw error if condition is false
                if evaluated_args.is_empty() || evaluated_args.len() > 2 {
                    return Err(EvaluatorError::EvaluationError(
                        "assert() requires 1 or 2 arguments".to_string(),
                    ));
                }

                // First argument must be a boolean
                let condition = match &evaluated_args[0] {
                    Value::Bool(b) => *b,
                    _ => {
                        return Err(EvaluatorError::TypeError(
                            "T0410: Argument 1 of function $assert does not match function signature".to_string(),
                        ));
                    }
                };

                if !condition {
                    let message = if evaluated_args.len() == 2 {
                        match &evaluated_args[1] {
                            Value::String(s) => s.clone(),
                            _ => "$assert() statement failed".to_string(),
                        }
                    } else {
                        "$assert() statement failed".to_string()
                    };
                    return Err(EvaluatorError::EvaluationError(format!("D3141: {}", message)));
                }

                Ok(Value::Null)
            }

            "eval" => {
                // $eval(expression [, context]) - parse and evaluate a JSONata expression at runtime
                if evaluated_args.is_empty() || evaluated_args.len() > 2 {
                    return Err(EvaluatorError::EvaluationError(
                        "T0410: Argument 1 of function $eval must be a string".to_string(),
                    ));
                }

                // If the first argument is null/undefined, return undefined
                if matches!(evaluated_args[0], Value::Null) {
                    return Ok(Value::Null);
                }

                // First argument must be a string expression
                let expr_str = match &evaluated_args[0] {
                    Value::String(s) => s.as_str(),
                    _ => {
                        return Err(EvaluatorError::EvaluationError(
                            "T0410: Argument 1 of function $eval must be a string".to_string(),
                        ));
                    }
                };

                // Parse the expression
                let parsed_ast = match parser::parse(expr_str) {
                    Ok(ast) => ast,
                    Err(e) => {
                        // D3120 is the error code for parse errors in $eval
                        return Err(EvaluatorError::EvaluationError(format!(
                            "D3120: The expression passed to $eval cannot be parsed: {}",
                            e
                        )));
                    }
                };

                // Determine the context to use for evaluation
                let eval_context = if evaluated_args.len() == 2 {
                    &evaluated_args[1]
                } else {
                    data
                };

                // Evaluate the parsed expression
                match self.evaluate_internal(&parsed_ast, eval_context) {
                    Ok(result) => Ok(result),
                    Err(e) => {
                        // D3121 is the error code for evaluation errors in $eval
                        let err_msg = e.to_string();
                        if err_msg.starts_with("D3121") || err_msg.contains("Unknown function") {
                            Err(EvaluatorError::EvaluationError(format!(
                                "D3121: {}",
                                err_msg
                            )))
                        } else {
                            Err(e)
                        }
                    }
                }
            }

            // DateTime functions
            "now" => {
                if !evaluated_args.is_empty() {
                    return Err(EvaluatorError::EvaluationError(
                        "now() takes no arguments".to_string(),
                    ));
                }
                Ok(crate::datetime::now())
            }

            "millis" => {
                if !evaluated_args.is_empty() {
                    return Err(EvaluatorError::EvaluationError(
                        "millis() takes no arguments".to_string(),
                    ));
                }
                Ok(crate::datetime::millis())
            }

            "toMillis" => {
                if evaluated_args.is_empty() || evaluated_args.len() > 2 {
                    return Err(EvaluatorError::EvaluationError(
                        "toMillis() requires 1 or 2 arguments".to_string(),
                    ));
                }

                match &evaluated_args[0] {
                    Value::String(s) => {
                        // Optional second argument is a picture string for custom parsing
                        let _picture = if evaluated_args.len() == 2 {
                            match &evaluated_args[1] {
                                Value::String(p) => Some(p.as_str()),
                                _ => None,
                            }
                        } else {
                            None
                        };

                        // For now, only support ISO 8601 format (picture string ignored)
                        // TODO: Implement custom picture string parsing
                        crate::datetime::to_millis(s)
                            .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
                    }
                    Value::Null => Ok(Value::Null),
                    _ => Err(EvaluatorError::TypeError(
                        "toMillis() requires a string argument".to_string(),
                    )),
                }
            }

            "fromMillis" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "fromMillis() requires exactly 1 argument".to_string(),
                    ));
                }

                match &evaluated_args[0] {
                    Value::Number(n) => {
                        let millis = n.as_i64().ok_or_else(|| {
                            EvaluatorError::TypeError("fromMillis() requires an integer".to_string())
                        })?;
                        crate::datetime::from_millis(millis)
                            .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
                    }
                    Value::Null => Ok(Value::Null),
                    _ => Err(EvaluatorError::TypeError(
                        "fromMillis() requires a number argument".to_string(),
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
            AstNode::Lambda { params, body, signature } => {
                // Direct lambda - invoke it
                self.invoke_lambda(params, body, signature.as_ref(), values, data)
            }
            AstNode::Variable(var_name) => {
                // Check if this variable holds a stored lambda
                if let Some(stored_lambda) = self.context.lookup_lambda(var_name).cloned() {
                    // Invoke the stored lambda with its captured environment
                    let captured_env = if stored_lambda.captured_env.is_empty() {
                        None
                    } else {
                        Some(&stored_lambda.captured_env)
                    };
                    self.invoke_lambda_with_env(
                        &stored_lambda.params,
                        &stored_lambda.body,
                        stored_lambda.signature.as_ref(),
                        values,
                        data,
                        captured_env,
                    )
                } else if let Some(value) = self.context.lookup(var_name).cloned() {
                    // Check if this variable holds a lambda value (JSON object with __lambda__)
                    // This handles lambdas passed as bound arguments in partial applications
                    if let Value::Object(ref map) = value {
                        if map.contains_key("__lambda__") {
                            // This is a lambda value - look up the stored lambda by its ID
                            if let Some(Value::String(lambda_id)) = map.get("_lambda_id") {
                                if let Some(stored_lambda) = self.context.lookup_lambda(lambda_id).cloned() {
                                    // Invoke with captured environment
                                    let captured_env = if stored_lambda.captured_env.is_empty() {
                                        None
                                    } else {
                                        Some(&stored_lambda.captured_env)
                                    };
                                    return self.invoke_lambda_with_env(
                                        &stored_lambda.params,
                                        &stored_lambda.body,
                                        stored_lambda.signature.as_ref(),
                                        values,
                                        data,
                                        captured_env,
                                    );
                                }
                            }
                        }
                    }
                    // Regular variable value - evaluate with first value as context
                    if values.is_empty() {
                        self.evaluate_internal(func_node, data)
                    } else {
                        self.evaluate_internal(func_node, &values[0])
                    }
                } else if self.is_builtin_function(var_name) {
                    // This is a built-in function reference (e.g., $string, $number)
                    // Call it directly with the provided values (already evaluated)
                    self.call_builtin_with_values(var_name, values)
                } else {
                    // Unknown variable - evaluate with first value as context
                    if values.is_empty() {
                        self.evaluate_internal(func_node, data)
                    } else {
                        self.evaluate_internal(func_node, &values[0])
                    }
                }
            }
            _ => {
                // For non-lambda expressions, evaluate with first value as context
                if values.is_empty() {
                    self.evaluate_internal(func_node, data)
                } else {
                    self.evaluate_internal(func_node, &values[0])
                }
            }
        }
    }

    /// Execute a transform operator on the bound $ value
    fn execute_transform(
        &mut self,
        location: &AstNode,
        update: &AstNode,
        delete: Option<&AstNode>,
        original_data: &Value,
    ) -> Result<Value, EvaluatorError> {
        // Get the input value from $ binding
        let input = self.context.lookup("$")
            .ok_or_else(|| EvaluatorError::EvaluationError("Transform requires $ binding".to_string()))?
            .clone();

        // Evaluate location expression on the input to get objects to transform
        let located_objects = self.evaluate_internal(location, &input)?;

        // Collect target objects into a vector for comparison
        let targets: Vec<Value> = match located_objects {
            Value::Array(arr) => arr,
            Value::Object(_) => vec![located_objects],
            Value::Null => Vec::new(),
            other => vec![other],
        };

        // Validate update parameter - must be an object constructor
        // We need to check this before evaluation in case of errors
        // For now, we'll validate after evaluation in the transform helper

        // Parse delete field names if provided
        let delete_fields: Vec<String> = if let Some(delete_node) = delete {
            let delete_val = self.evaluate_internal(delete_node, &input)?;
            match delete_val {
                Value::Array(arr) => arr.iter()
                    .filter_map(|v| match v {
                        Value::String(s) => Some(s.clone()),
                        _ => None,
                    })
                    .collect(),
                Value::String(s) => vec![s],
                Value::Null => Vec::new(), // Undefined variable is treated as no deletion
                _ => {
                    // Delete parameter must be an array of strings or a string
                    return Err(EvaluatorError::EvaluationError(
                        "T2012: The third argument of the transform operator must be an array of strings".to_string()
                    ));
                }
            }
        } else {
            Vec::new()
        };

        // Recursive helper to apply transformation throughout the structure
        fn apply_transform_deep(
            evaluator: &mut Evaluator,
            value: &Value,
            targets: &[Value],
            update: &AstNode,
            delete_fields: &[String],
        ) -> Result<Value, EvaluatorError> {
            // Check if this value is one of the targets to transform
            // Use Value's PartialEq for semantic equality comparison
            if targets.iter().any(|t| t == value) {
                // Transform this object
                if let Value::Object(mut map) = value.clone() {
                    let update_val = evaluator.evaluate_internal(update, value)?;
                    // Validate that update evaluates to an object or null (undefined)
                    match update_val {
                        Value::Object(update_map) => {
                            for (key, val) in update_map {
                                map.insert(key, val);
                            }
                        }
                        Value::Null => {
                            // Null/undefined means no updates, just continue to deletions
                        }
                        _ => {
                            return Err(EvaluatorError::EvaluationError(
                                "T2011: The second argument of the transform operator must evaluate to an object".to_string()
                            ));
                        }
                    }
                    for field in delete_fields {
                        map.remove(field);
                    }
                    return Ok(Value::Object(map));
                }
                return Ok(value.clone());
            }

            // Otherwise, recursively process children to find and transform targets
            match value {
                Value::Object(map) => {
                    let mut new_map = serde_json::Map::new();
                    for (k, v) in map {
                        new_map.insert(
                            k.clone(),
                            apply_transform_deep(evaluator, v, targets, update, delete_fields)?
                        );
                    }
                    Ok(Value::Object(new_map))
                }
                Value::Array(arr) => {
                    let mut new_arr = Vec::new();
                    for item in arr {
                        new_arr.push(apply_transform_deep(evaluator, item, targets, update, delete_fields)?);
                    }
                    Ok(Value::Array(new_arr))
                }
                _ => Ok(value.clone()),
            }
        }

        // Apply transformation recursively starting from input
        apply_transform_deep(self, &input, &targets, update, &delete_fields)
    }

    /// Helper to invoke a lambda with given parameters
    fn invoke_lambda(&mut self, params: &[String], body: &AstNode, signature: Option<&String>, values: &[Value], data: &Value) -> Result<Value, EvaluatorError> {
        self.invoke_lambda_with_env(params, body, signature, values, data, None)
    }

    /// Invoke a lambda with optional captured environment (for closures)
    fn invoke_lambda_with_env(
        &mut self,
        params: &[String],
        body: &AstNode,
        signature: Option<&String>,
        values: &[Value],
        data: &Value,
        captured_env: Option<&std::collections::HashMap<String, Value>>,
    ) -> Result<Value, EvaluatorError> {
        // Validate signature if present, and get coerced arguments
        let coerced_values = if let Some(sig_str) = signature {
            match crate::signature::Signature::parse(sig_str) {
                Ok(sig) => {
                    // Validate and coerce arguments
                    match sig.validate_and_coerce(values) {
                        Ok(coerced) => coerced,
                        Err(e) => {
                            match e {
                                // Undefined argument - return undefined (silent failure)
                                crate::signature::SignatureError::UndefinedArgument => {
                                    return Ok(Value::Null);
                                }
                                // Explicit type mismatch - throw error
                                crate::signature::SignatureError::ArgumentTypeMismatch { index, expected } => {
                                    return Err(EvaluatorError::TypeError(
                                        format!("T0410: Argument {} of function does not match function signature (expected {})", index, expected)
                                    ));
                                }
                                // Array element type mismatch - throw error
                                crate::signature::SignatureError::ArrayTypeMismatch { index, expected } => {
                                    return Err(EvaluatorError::TypeError(
                                        format!("T0412: Argument {} of function must be an array of {}", index, expected)
                                    ));
                                }
                                // Other errors - throw generic error
                                _ => {
                                    return Err(EvaluatorError::TypeError(format!("Signature validation failed: {}", e)));
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    return Err(EvaluatorError::EvaluationError(format!("Invalid signature: {}", e)));
                }
            }
        } else {
            // No signature - be lenient about argument count:
            // - If more values than params: only use first N values (ignore extras)
            // - If fewer values than params: remaining params get undefined (null)
            // This matches JSONata's flexible function calling behavior
            values.to_vec()
        };

        // Save current bindings for all variables we might modify
        // This includes both parameters and any captured environment variables
        let mut vars_to_restore: Vec<String> = params.to_vec();
        if let Some(env) = captured_env {
            vars_to_restore.extend(env.keys().cloned());
        }

        let saved_bindings: std::collections::HashMap<String, Option<Value>> = vars_to_restore
            .iter()
            .map(|name| (name.clone(), self.context.lookup(name).cloned()))
            .collect();

        // First apply captured environment (for closures)
        if let Some(env) = captured_env {
            for (name, value) in env {
                self.context.bind(name.clone(), value.clone());
            }
        }

        // Then bind lambda parameters to provided values (these override captured env)
        // If there are more params than values, extra params get undefined (null)
        for (i, param) in params.iter().enumerate() {
            let value = coerced_values.get(i).cloned().unwrap_or(Value::Null);
            self.context.bind(param.clone(), value);
        }

        // Check if this is a partial application (body is a special marker string)
        if let AstNode::String(body_str) = body {
            if body_str.starts_with("__partial_call:") {
                // Parse the partial call info
                let parts: Vec<&str> = body_str.split(':').collect();
                if parts.len() >= 4 {
                    let func_name = parts[1];
                    let is_builtin = parts[2] == "true";
                    let total_args: usize = parts[3].parse().unwrap_or(0);

                    // Get placeholder positions from captured env
                    let placeholder_positions: Vec<usize> = if let Some(env) = captured_env {
                        if let Some(Value::Array(positions)) = env.get("__placeholder_positions") {
                            positions.iter()
                                .filter_map(|v| v.as_u64().map(|n| n as usize))
                                .collect()
                        } else {
                            vec![]
                        }
                    } else {
                        vec![]
                    };

                    // Reconstruct the full argument list
                    let mut full_args: Vec<Value> = vec![Value::Null; total_args];

                    // Fill in bound arguments from captured environment
                    if let Some(env) = captured_env {
                        for (key, value) in env {
                            if key.starts_with("__bound_arg_") {
                                if let Ok(pos) = key[12..].parse::<usize>() {
                                    if pos < total_args {
                                        full_args[pos] = value.clone();
                                    }
                                }
                            }
                        }
                    }

                    // Fill in placeholder positions with provided values
                    for (i, &pos) in placeholder_positions.iter().enumerate() {
                        if pos < total_args {
                            let value = coerced_values.get(i).cloned().unwrap_or(Value::Null);
                            full_args[pos] = value;
                        }
                    }

                    // Restore bindings before calling the function
                    for (name, saved_value) in &saved_bindings {
                        if let Some(value) = saved_value {
                            self.context.bind(name.clone(), value.clone());
                        } else {
                            self.context.unbind(name);
                        }
                    }

                    // Build AST nodes for the function call arguments
                    // We need to convert the Values to a form that evaluate_function_call can use
                    // Since we already have evaluated Values, we'll bind them temporarily and reference them
                    let mut temp_args: Vec<AstNode> = Vec::new();
                    for (i, value) in full_args.iter().enumerate() {
                        let temp_name = format!("__temp_arg_{}", i);
                        self.context.bind(temp_name.clone(), value.clone());
                        temp_args.push(AstNode::Variable(temp_name));
                    }

                    // Call the original function
                    let result = self.evaluate_function_call(func_name, &temp_args, is_builtin, data);

                    // Clean up temp bindings
                    for i in 0..full_args.len() {
                        self.context.unbind(&format!("__temp_arg_{}", i));
                    }

                    return result;
                }
            }
        }

        // Evaluate lambda body (normal case)
        let result = self.evaluate_internal(body, data)?;

        // Restore previous bindings
        for (name, saved_value) in saved_bindings {
            if let Some(value) = saved_value {
                self.context.bind(name, value);
            } else {
                self.context.unbind(&name);
            }
        }

        Ok(result)
    }

    /// Capture the current environment bindings for closure support
    fn capture_current_environment(&self) -> std::collections::HashMap<String, Value> {
        self.context.bindings.clone()
    }

    /// Check if a name refers to a built-in function
    fn is_builtin_function(&self, name: &str) -> bool {
        matches!(name,
            // String functions
            "string" | "length" | "substring" | "substringBefore" | "substringAfter" |
            "uppercase" | "lowercase" | "trim" | "pad" | "contains" | "split" |
            "join" | "match" | "replace" | "eval" | "base64encode" | "base64decode" |
            "encodeUrlComponent" | "encodeUrl" | "decodeUrlComponent" | "decodeUrl" |

            // Numeric functions
            "number" | "abs" | "floor" | "ceil" | "round" | "power" | "sqrt" |
            "random" | "formatNumber" | "formatBase" | "formatInteger" | "parseInteger" |

            // Aggregation functions
            "sum" | "max" | "min" | "average" |

            // Boolean/logic functions
            "boolean" | "not" | "exists" |

            // Array functions
            "count" | "append" | "sort" | "reverse" | "shuffle" | "distinct" | "zip" |

            // Object functions
            "keys" | "lookup" | "spread" | "merge" | "sift" | "each" | "error" | "assert" | "type" |

            // Higher-order functions
            "map" | "filter" | "reduce" | "singletonArray" |

            // Date/time functions
            "now" | "millis" | "fromMillis" | "toMillis"
        )
    }

    /// Call a built-in function directly with pre-evaluated Values
    /// This is used when passing built-in functions to higher-order functions like $map
    fn call_builtin_with_values(&mut self, name: &str, values: &[Value]) -> Result<Value, EvaluatorError> {
        use crate::functions;

        // Most built-in functions expect a single argument
        if values.is_empty() {
            return Err(EvaluatorError::EvaluationError(
                format!("{}() requires at least 1 argument", name)
            ));
        }

        let arg = &values[0];

        match name {
            // String functions with single argument
            "string" => functions::string::string(arg, None)
                .map_err(|e: functions::FunctionError| EvaluatorError::EvaluationError(e.to_string())),
            "number" => functions::numeric::number(arg)
                .map_err(|e: functions::FunctionError| EvaluatorError::EvaluationError(e.to_string())),
            "boolean" => functions::boolean::boolean(arg)
                .map_err(|e: functions::FunctionError| EvaluatorError::EvaluationError(e.to_string())),
            "not" => {
                let b = functions::boolean::boolean(arg)
                    .map_err(|e: functions::FunctionError| EvaluatorError::EvaluationError(e.to_string()))?;
                match b {
                    Value::Bool(val) => Ok(Value::Bool(!val)),
                    _ => Err(EvaluatorError::TypeError("not() requires a boolean".to_string())),
                }
            }
            "exists" => {
                Ok(Value::Bool(!matches!(arg, Value::Null)))
            }
            "abs" => {
                match arg {
                    Value::Number(n) => functions::numeric::abs(n.as_f64().unwrap_or(0.0))
                        .map_err(|e: functions::FunctionError| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Err(EvaluatorError::TypeError("abs() requires a number argument".to_string())),
                }
            }
            "floor" => {
                match arg {
                    Value::Number(n) => functions::numeric::floor(n.as_f64().unwrap_or(0.0))
                        .map_err(|e: functions::FunctionError| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Err(EvaluatorError::TypeError("floor() requires a number argument".to_string())),
                }
            }
            "ceil" => {
                match arg {
                    Value::Number(n) => functions::numeric::ceil(n.as_f64().unwrap_or(0.0))
                        .map_err(|e: functions::FunctionError| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Err(EvaluatorError::TypeError("ceil() requires a number argument".to_string())),
                }
            }
            "round" => {
                match arg {
                    Value::Number(n) => functions::numeric::round(n.as_f64().unwrap_or(0.0), None)
                        .map_err(|e: functions::FunctionError| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Err(EvaluatorError::TypeError("round() requires a number argument".to_string())),
                }
            }
            "sqrt" => {
                match arg {
                    Value::Number(n) => functions::numeric::sqrt(n.as_f64().unwrap_or(0.0))
                        .map_err(|e: functions::FunctionError| EvaluatorError::EvaluationError(e.to_string())),
                    _ => Err(EvaluatorError::TypeError("sqrt() requires a number argument".to_string())),
                }
            }
            "uppercase" => {
                match arg {
                    Value::String(s) => Ok(Value::String(s.to_uppercase())),
                    Value::Null => Ok(Value::Null),
                    _ => Err(EvaluatorError::TypeError("uppercase() requires a string argument".to_string())),
                }
            }
            "lowercase" => {
                match arg {
                    Value::String(s) => Ok(Value::String(s.to_lowercase())),
                    Value::Null => Ok(Value::Null),
                    _ => Err(EvaluatorError::TypeError("lowercase() requires a string argument".to_string())),
                }
            }
            "trim" => {
                match arg {
                    Value::String(s) => Ok(Value::String(s.trim().to_string())),
                    Value::Null => Ok(Value::Null),
                    _ => Err(EvaluatorError::TypeError("trim() requires a string argument".to_string())),
                }
            }
            "length" => {
                match arg {
                    Value::String(s) => Ok(Value::Number(serde_json::Number::from(s.chars().count()))),
                    Value::Array(arr) => Ok(Value::Number(serde_json::Number::from(arr.len()))),
                    Value::Null => Ok(Value::Null),
                    _ => Err(EvaluatorError::TypeError("length() requires a string or array argument".to_string())),
                }
            }
            "sum" => {
                match arg {
                    Value::Array(arr) => {
                        let mut total = 0.0;
                        for item in arr {
                            match item {
                                Value::Number(n) => {
                                    total += n.as_f64().unwrap_or(0.0);
                                }
                                _ => {
                                    return Err(EvaluatorError::TypeError(
                                        "sum() requires all array elements to be numbers".to_string()
                                    ));
                                }
                            }
                        }
                        Ok(serde_json::json!(total))
                    }
                    Value::Number(n) => Ok(Value::Number(n.clone())),
                    Value::Null => Ok(Value::Null),
                    _ => Err(EvaluatorError::TypeError("sum() requires an array of numbers".to_string())),
                }
            }
            "count" => {
                match arg {
                    Value::Array(arr) => Ok(Value::Number(serde_json::Number::from(arr.len()))),
                    Value::Null => Ok(Value::Number(serde_json::Number::from(0))),
                    _ => Ok(Value::Number(serde_json::Number::from(1))), // Single value counts as 1
                }
            }
            "max" => {
                match arg {
                    Value::Array(arr) => {
                        let mut max_val: Option<f64> = None;
                        for item in arr {
                            if let Value::Number(n) = item {
                                let f = n.as_f64().unwrap_or(f64::NEG_INFINITY);
                                max_val = Some(max_val.map_or(f, |m| m.max(f)));
                            }
                        }
                        max_val.map_or(Ok(Value::Null), |m| Ok(serde_json::json!(m)))
                    }
                    Value::Number(n) => Ok(Value::Number(n.clone())),
                    Value::Null => Ok(Value::Null),
                    _ => Err(EvaluatorError::TypeError("max() requires an array of numbers".to_string())),
                }
            }
            "min" => {
                match arg {
                    Value::Array(arr) => {
                        let mut min_val: Option<f64> = None;
                        for item in arr {
                            if let Value::Number(n) = item {
                                let f = n.as_f64().unwrap_or(f64::INFINITY);
                                min_val = Some(min_val.map_or(f, |m| m.min(f)));
                            }
                        }
                        min_val.map_or(Ok(Value::Null), |m| Ok(serde_json::json!(m)))
                    }
                    Value::Number(n) => Ok(Value::Number(n.clone())),
                    Value::Null => Ok(Value::Null),
                    _ => Err(EvaluatorError::TypeError("min() requires an array of numbers".to_string())),
                }
            }
            "average" => {
                match arg {
                    Value::Array(arr) => {
                        let nums: Vec<f64> = arr.iter()
                            .filter_map(|v| v.as_f64())
                            .collect();
                        if nums.is_empty() {
                            Ok(Value::Null)
                        } else {
                            let avg = nums.iter().sum::<f64>() / nums.len() as f64;
                            Ok(serde_json::json!(avg))
                        }
                    }
                    Value::Number(n) => Ok(Value::Number(n.clone())),
                    Value::Null => Ok(Value::Null),
                    _ => Err(EvaluatorError::TypeError("average() requires an array of numbers".to_string())),
                }
            }
            "append" => {
                // append(array1, array2) - append second array to first
                if values.len() < 2 {
                    return Err(EvaluatorError::EvaluationError(
                        "append() requires 2 arguments".to_string()
                    ));
                }
                let first = &values[0];
                let second = &values[1];

                // Convert first to array if needed
                let mut result = match first {
                    Value::Array(arr) => arr.clone(),
                    Value::Null => vec![],
                    other => vec![other.clone()],
                };

                // Append second (flatten if array)
                match second {
                    Value::Array(arr) => result.extend(arr.clone()),
                    Value::Null => {}
                    other => result.push(other.clone()),
                }

                Ok(Value::Array(result))
            }
            "reverse" => {
                match arg {
                    Value::Array(arr) => {
                        let mut reversed = arr.clone();
                        reversed.reverse();
                        Ok(Value::Array(reversed))
                    }
                    Value::Null => Ok(Value::Null),
                    _ => Err(EvaluatorError::TypeError("reverse() requires an array".to_string())),
                }
            }
            "keys" => {
                match arg {
                    Value::Object(obj) => {
                        let keys: Vec<Value> = obj.keys()
                            .map(|k| Value::String(k.clone()))
                            .collect();
                        Ok(Value::Array(keys))
                    }
                    Value::Null => Ok(Value::Null),
                    _ => Err(EvaluatorError::TypeError("keys() requires an object".to_string())),
                }
            }

            // Add more functions as needed
            _ => Err(EvaluatorError::ReferenceError(
                format!("Built-in function {} cannot be called with values directly", name)
            )),
        }
    }

    /// Collect all descendant values recursively
    fn collect_descendants(&self, value: &Value) -> Vec<Value> {
        let mut descendants = Vec::new();

        match value {
            Value::Null => {
                // Null has no descendants, return empty
                return descendants;
            }
            Value::Object(obj) => {
                // Include the current object
                descendants.push(value.clone());

                for val in obj.values() {
                    // Recursively collect descendants
                    descendants.extend(self.collect_descendants(val));
                }
            }
            Value::Array(arr) => {
                // Include the current array
                descendants.push(value.clone());

                for val in arr {
                    // Recursively collect descendants
                    descendants.extend(self.collect_descendants(val));
                }
            }
            _ => {
                // For primitives (string, number, boolean), just include the value itself
                descendants.push(value.clone());
            }
        }

        descendants
    }

    /// Evaluate a predicate (array filter or index)
    fn evaluate_predicate(&mut self, current: &Value, predicate: &AstNode) -> Result<Value, EvaluatorError> {
        // Special case: empty brackets [] (represented as Boolean(true))
        // This forces the value to be wrapped in an array
        if matches!(predicate, AstNode::Boolean(true)) {
            return match current {
                Value::Array(arr) => Ok(Value::Array(arr.clone())),
                Value::Null => Ok(Value::Null),
                other => Ok(Value::Array(vec![other.clone()])),
            };
        }

        match current {
            Value::Array(_arr) => {
                // Standalone predicates do simple array operations (no mapping over sub-arrays)

                // First, try to evaluate predicate as a simple number (array index)
                if let AstNode::Number(n) = predicate {
                    // Direct array indexing
                    return self.array_index(current, &serde_json::json!(n));
                }

                // Try to evaluate the predicate to see if it's a numeric index
                // If evaluation succeeds and yields a number, use it as an index
                // If evaluation fails (e.g., comparison error), treat as filter
                match self.evaluate_internal(predicate, current) {
                    Ok(Value::Number(_)) => {
                        // It's a numeric index
                        let pred_result = self.evaluate_internal(predicate, current)?;
                        return self.array_index(current, &pred_result);
                    }
                    Ok(_) => {
                        // Evaluated successfully but not a number - might be a filter
                        // Fall through to filter logic
                    }
                    Err(_) => {
                        // Evaluation failed - it's likely a filter expression
                        // Fall through to filter logic
                    }
                }

                // It's a filter expression - evaluate the predicate for each array element
                let mut filtered = Vec::new();
                for item in _arr {
                    let item_result = self.evaluate_internal(predicate, item)?;

                    // If result is truthy, include this item
                    if self.is_truthy(&item_result) {
                        filtered.push(item.clone());
                    }
                }

                Ok(Value::Array(filtered))
            }
            Value::Object(_) => {
                // For objects, predicate is like accessing a computed property
                let pred_result = self.evaluate_internal(predicate, current)?;

                // If it's a string, use it as a key
                if let Value::String(key) = pred_result {
                    if let Value::Object(obj) = current {
                        return Ok(obj.get(&key).cloned().unwrap_or(Value::Null));
                    }
                }

                Ok(Value::Null)
            }
            _ => {
                // For primitive values (string, number, boolean), the predicate acts as a filter:
                // value[true] returns value, value[false] returns undefined
                // This enables patterns like: $k[$v>2] which returns $k if $v>2, otherwise undefined
                let pred_result = self.evaluate_internal(predicate, current)?;
                if self.is_truthy(&pred_result) {
                    Ok(current.clone())
                } else {
                    // Return undefined (not null) so $map can filter it out
                    Ok(undefined_value())
                }
            }
        }
    }

    /// Evaluate sort operator
    fn evaluate_sort(&mut self, data: &Value, terms: &[(AstNode, bool)]) -> Result<Value, EvaluatorError> {
        // If data is null, return null
        if matches!(data, Value::Null) {
            return Ok(Value::Null);
        }

        // If data is not an array, return it as-is (can't sort a single value)
        let array = match data {
            Value::Array(arr) => arr.clone(),
            other => return Ok(other.clone()),
        };

        // If empty array, return as-is
        if array.is_empty() {
            return Ok(Value::Array(array));
        }

        // Evaluate sort keys for each element
        let mut indexed_array: Vec<(usize, Vec<Value>)> = Vec::new();

        for (idx, element) in array.iter().enumerate() {
            let mut sort_keys = Vec::new();

            // Evaluate each sort term with $ bound to the element
            for (term_expr, _ascending) in terms {
                // Save current $ binding
                let saved_dollar = self.context.lookup("$").cloned();

                // Bind $ to current element
                self.context.bind("$".to_string(), element.clone());

                // Evaluate the sort expression
                let sort_value = self.evaluate_internal(term_expr, element)?;

                // Restore $ binding
                if let Some(val) = saved_dollar {
                    self.context.bind("$".to_string(), val);
                } else {
                    self.context.unbind("$");
                }

                sort_keys.push(sort_value);
            }

            indexed_array.push((idx, sort_keys));
        }

        // Sort the indexed array
        indexed_array.sort_by(|a, b| {
            // Compare sort keys in order
            for (i, (_term_expr, ascending)) in terms.iter().enumerate() {
                let left = &a.1[i];
                let right = &b.1[i];

                let cmp = self.compare_values(left, right);

                if cmp != std::cmp::Ordering::Equal {
                    return if *ascending {
                        cmp
                    } else {
                        cmp.reverse()
                    };
                }
            }

            // If all keys are equal, maintain original order (stable sort)
            a.0.cmp(&b.0)
        });

        // Extract sorted elements
        let sorted: Vec<Value> = indexed_array
            .iter()
            .map(|(idx, _)| array[*idx].clone())
            .collect();

        Ok(Value::Array(sorted))
    }

    /// Compare two values for sorting (JSONata semantics)
    fn compare_values(&self, left: &Value, right: &Value) -> std::cmp::Ordering {
        use std::cmp::Ordering;

        match (left, right) {
            // Nulls sort first
            (Value::Null, Value::Null) => Ordering::Equal,
            (Value::Null, _) => Ordering::Less,
            (_, Value::Null) => Ordering::Greater,

            // Numbers
            (Value::Number(a), Value::Number(b)) => {
                let a_f64 = a.as_f64().unwrap();
                let b_f64 = b.as_f64().unwrap();
                a_f64.partial_cmp(&b_f64).unwrap_or(Ordering::Equal)
            }

            // Strings
            (Value::String(a), Value::String(b)) => a.cmp(b),

            // Booleans
            (Value::Bool(a), Value::Bool(b)) => a.cmp(b),

            // Arrays (lexicographic comparison)
            (Value::Array(a), Value::Array(b)) => {
                for (a_elem, b_elem) in a.iter().zip(b.iter()) {
                    let cmp = self.compare_values(a_elem, b_elem);
                    if cmp != Ordering::Equal {
                        return cmp;
                    }
                }
                a.len().cmp(&b.len())
            }

            // Different types: use type ordering
            // null < bool < number < string < array < object
            (Value::Bool(_), Value::Number(_)) => Ordering::Less,
            (Value::Bool(_), Value::String(_)) => Ordering::Less,
            (Value::Bool(_), Value::Array(_)) => Ordering::Less,
            (Value::Bool(_), Value::Object(_)) => Ordering::Less,

            (Value::Number(_), Value::Bool(_)) => Ordering::Greater,
            (Value::Number(_), Value::String(_)) => Ordering::Less,
            (Value::Number(_), Value::Array(_)) => Ordering::Less,
            (Value::Number(_), Value::Object(_)) => Ordering::Less,

            (Value::String(_), Value::Bool(_)) => Ordering::Greater,
            (Value::String(_), Value::Number(_)) => Ordering::Greater,
            (Value::String(_), Value::Array(_)) => Ordering::Less,
            (Value::String(_), Value::Object(_)) => Ordering::Less,

            (Value::Array(_), Value::Bool(_)) => Ordering::Greater,
            (Value::Array(_), Value::Number(_)) => Ordering::Greater,
            (Value::Array(_), Value::String(_)) => Ordering::Greater,
            (Value::Array(_), Value::Object(_)) => Ordering::Less,

            (Value::Object(_), _) => Ordering::Greater,
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

    /// Check if a value is truthy for the default operator (?:)
    /// This has special semantics:
    /// - Lambda/function objects are not values, so they're falsy
    /// - Arrays containing only falsy elements are falsy
    /// - Otherwise, use standard truthiness
    fn is_truthy_for_default(&self, value: &Value) -> bool {
        match value {
            // Check if this is a lambda object (has __lambda__ marker)
            Value::Object(obj) if obj.contains_key("__lambda__") => false,
            // Arrays need special handling - check if all elements are falsy
            Value::Array(arr) => {
                if arr.is_empty() {
                    return false;
                }
                // Array is truthy only if it contains at least one truthy element
                arr.iter().any(|elem| self.is_truthy(elem))
            }
            // For all other types, use standard truthiness
            _ => self.is_truthy(value),
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
    fn add(&self, left: &Value, right: &Value, left_is_explicit_null: bool, right_is_explicit_null: bool) -> Result<Value, EvaluatorError> {
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => {
                Ok(serde_json::json!(a.as_f64().unwrap() + b.as_f64().unwrap()))
            }
            // Explicit null literal with number -> T2002 error
            (Value::Null, Value::Number(_)) if left_is_explicit_null => {
                Err(EvaluatorError::TypeError("T2002: The left side of the + operator must evaluate to a number".to_string()))
            }
            (Value::Number(_), Value::Null) if right_is_explicit_null => {
                Err(EvaluatorError::TypeError("T2002: The right side of the + operator must evaluate to a number".to_string()))
            }
            (Value::Null, Value::Null) if left_is_explicit_null || right_is_explicit_null => {
                Err(EvaluatorError::TypeError("T2002: The left side of the + operator must evaluate to a number".to_string()))
            }
            // Undefined variables with number -> undefined result
            (Value::Null, _) | (_, Value::Null) => Ok(Value::Null),
            _ => Err(EvaluatorError::TypeError(format!(
                "Cannot add {:?} and {:?}",
                left, right
            ))),
        }
    }

    /// Subtraction
    fn subtract(&self, left: &Value, right: &Value, left_is_explicit_null: bool, right_is_explicit_null: bool) -> Result<Value, EvaluatorError> {
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => {
                Ok(serde_json::json!(a.as_f64().unwrap() - b.as_f64().unwrap()))
            }
            // Explicit null literal -> error
            (Value::Null, _) if left_is_explicit_null => {
                Err(EvaluatorError::TypeError("T2002: The left side of the - operator must evaluate to a number".to_string()))
            }
            (_, Value::Null) if right_is_explicit_null => {
                Err(EvaluatorError::TypeError("T2002: The right side of the - operator must evaluate to a number".to_string()))
            }
            // Undefined variables -> undefined result
            (Value::Null, _) | (_, Value::Null) => Ok(Value::Null),
            _ => Err(EvaluatorError::TypeError(format!(
                "Cannot subtract {:?} and {:?}",
                left, right
            ))),
        }
    }

    /// Multiplication
    fn multiply(&self, left: &Value, right: &Value, left_is_explicit_null: bool, right_is_explicit_null: bool) -> Result<Value, EvaluatorError> {
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => {
                Ok(serde_json::json!(a.as_f64().unwrap() * b.as_f64().unwrap()))
            }
            // Explicit null literal -> error
            (Value::Null, _) if left_is_explicit_null => {
                Err(EvaluatorError::TypeError("T2002: The left side of the * operator must evaluate to a number".to_string()))
            }
            (_, Value::Null) if right_is_explicit_null => {
                Err(EvaluatorError::TypeError("T2002: The right side of the * operator must evaluate to a number".to_string()))
            }
            // Undefined variables -> undefined result
            (Value::Null, _) | (_, Value::Null) => Ok(Value::Null),
            _ => Err(EvaluatorError::TypeError(format!(
                "Cannot multiply {:?} and {:?}",
                left, right
            ))),
        }
    }

    /// Division
    fn divide(&self, left: &Value, right: &Value, left_is_explicit_null: bool, right_is_explicit_null: bool) -> Result<Value, EvaluatorError> {
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
            // Explicit null literal -> error
            (Value::Null, _) if left_is_explicit_null => {
                Err(EvaluatorError::TypeError("T2002: The left side of the / operator must evaluate to a number".to_string()))
            }
            (_, Value::Null) if right_is_explicit_null => {
                Err(EvaluatorError::TypeError("T2002: The right side of the / operator must evaluate to a number".to_string()))
            }
            // Undefined variables -> undefined result
            (Value::Null, _) | (_, Value::Null) => Ok(Value::Null),
            _ => Err(EvaluatorError::TypeError(format!(
                "Cannot divide {:?} and {:?}",
                left, right
            ))),
        }
    }

    /// Modulo
    fn modulo(&self, left: &Value, right: &Value, left_is_explicit_null: bool, right_is_explicit_null: bool) -> Result<Value, EvaluatorError> {
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
            // Explicit null literal -> error
            (Value::Null, _) if left_is_explicit_null => {
                Err(EvaluatorError::TypeError("T2002: The left side of the % operator must evaluate to a number".to_string()))
            }
            (_, Value::Null) if right_is_explicit_null => {
                Err(EvaluatorError::TypeError("T2002: The right side of the % operator must evaluate to a number".to_string()))
            }
            // Undefined variables -> undefined result
            (Value::Null, _) | (_, Value::Null) => Ok(Value::Null),
            _ => Err(EvaluatorError::TypeError(format!(
                "Cannot compute modulo of {:?} and {:?}",
                left, right
            ))),
        }
    }

    /// Get human-readable type name for error messages
    fn type_name(value: &Value) -> &'static str {
        match value {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        }
    }

    /// Less than comparison
    fn less_than(&self, left: &Value, right: &Value, left_is_explicit_null: bool, right_is_explicit_null: bool) -> Result<Value, EvaluatorError> {
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => {
                Ok(Value::Bool(a.as_f64().unwrap() < b.as_f64().unwrap()))
            }
            (Value::String(a), Value::String(b)) => Ok(Value::Bool(a < b)),
            // Null comparisons - distinguish explicit null from undefined
            (Value::Null, Value::Null) => {
                // Both null/undefined -> return undefined
                Ok(Value::Null)
            }
            // Explicit null literal with any type (except null) -> T2010 error
            (Value::Null, _) if left_is_explicit_null => {
                Err(EvaluatorError::EvaluationError("T2010: Type mismatch in comparison".to_string()))
            }
            (_, Value::Null) if right_is_explicit_null => {
                Err(EvaluatorError::EvaluationError("T2010: Type mismatch in comparison".to_string()))
            }
            // Boolean with undefined -> T2010 error
            (Value::Bool(_), Value::Null) | (Value::Null, Value::Bool(_)) => {
                Err(EvaluatorError::EvaluationError("T2010: Type mismatch in comparison".to_string()))
            }
            // Number or String with undefined (not explicit null) -> undefined result
            (Value::Number(_), Value::Null) | (Value::Null, Value::Number(_)) |
            (Value::String(_), Value::Null) | (Value::Null, Value::String(_)) => {
                Ok(Value::Null)
            }
            // String vs Number -> T2009
            (Value::String(_), Value::Number(_)) | (Value::Number(_), Value::String(_)) => {
                Err(EvaluatorError::EvaluationError("T2009: The expressions on either side of operator \"<\" must be of the same data type".to_string()))
            }
            // Boolean comparisons -> T2010
            (Value::Bool(_), _) | (_, Value::Bool(_)) => {
                Err(EvaluatorError::EvaluationError(format!(
                    "T2010: Cannot compare {} and {}",
                    Self::type_name(left), Self::type_name(right)
                )))
            }
            // Other type mismatches
            _ => Err(EvaluatorError::EvaluationError(format!(
                "T2010: Cannot compare {} and {}",
                Self::type_name(left), Self::type_name(right)
            ))),
        }
    }

    /// Less than or equal comparison
    fn less_than_or_equal(&self, left: &Value, right: &Value, left_is_explicit_null: bool, right_is_explicit_null: bool) -> Result<Value, EvaluatorError> {
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => {
                Ok(Value::Bool(a.as_f64().unwrap() <= b.as_f64().unwrap()))
            }
            (Value::String(a), Value::String(b)) => Ok(Value::Bool(a <= b)),
            // Null comparisons - distinguish explicit null from undefined
            (Value::Null, Value::Null) => Ok(Value::Null),
            // Explicit null literal with any type (except null) -> T2010 error
            (Value::Null, _) if left_is_explicit_null => {
                Err(EvaluatorError::EvaluationError("T2010: Type mismatch in comparison".to_string()))
            }
            (_, Value::Null) if right_is_explicit_null => {
                Err(EvaluatorError::EvaluationError("T2010: Type mismatch in comparison".to_string()))
            }
            // Boolean with undefined -> T2010 error
            (Value::Bool(_), Value::Null) | (Value::Null, Value::Bool(_)) => {
                Err(EvaluatorError::EvaluationError("T2010: Type mismatch in comparison".to_string()))
            }
            // Number or String with undefined (not explicit null) -> undefined result
            (Value::Number(_), Value::Null) | (Value::Null, Value::Number(_)) |
            (Value::String(_), Value::Null) | (Value::Null, Value::String(_)) => {
                Ok(Value::Null)
            }
            // String vs Number -> T2009
            (Value::String(_), Value::Number(_)) | (Value::Number(_), Value::String(_)) => {
                Err(EvaluatorError::EvaluationError("T2009: The expressions on either side of operator \"<=\" must be of the same data type".to_string()))
            }
            // Boolean comparisons -> T2010
            (Value::Bool(_), _) | (_, Value::Bool(_)) => {
                Err(EvaluatorError::EvaluationError(format!(
                    "T2010: Cannot compare {} and {}",
                    Self::type_name(left), Self::type_name(right)
                )))
            }
            // Other type mismatches
            _ => Err(EvaluatorError::EvaluationError(format!(
                "T2010: Cannot compare {} and {}",
                Self::type_name(left), Self::type_name(right)
            ))),
        }
    }

    /// Greater than comparison
    fn greater_than(&self, left: &Value, right: &Value, left_is_explicit_null: bool, right_is_explicit_null: bool) -> Result<Value, EvaluatorError> {
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => {
                Ok(Value::Bool(a.as_f64().unwrap() > b.as_f64().unwrap()))
            }
            (Value::String(a), Value::String(b)) => Ok(Value::Bool(a > b)),
            // Null comparisons - distinguish explicit null from undefined
            (Value::Null, Value::Null) => Ok(Value::Null),
            // Explicit null literal with any type (except null) -> T2010 error
            (Value::Null, _) if left_is_explicit_null => {
                Err(EvaluatorError::EvaluationError("T2010: Type mismatch in comparison".to_string()))
            }
            (_, Value::Null) if right_is_explicit_null => {
                Err(EvaluatorError::EvaluationError("T2010: Type mismatch in comparison".to_string()))
            }
            // Boolean with undefined -> T2010 error
            (Value::Bool(_), Value::Null) | (Value::Null, Value::Bool(_)) => {
                Err(EvaluatorError::EvaluationError("T2010: Type mismatch in comparison".to_string()))
            }
            // Number or String with undefined (not explicit null) -> undefined result
            (Value::Number(_), Value::Null) | (Value::Null, Value::Number(_)) |
            (Value::String(_), Value::Null) | (Value::Null, Value::String(_)) => {
                Ok(Value::Null)
            }
            // String vs Number -> T2009
            (Value::String(_), Value::Number(_)) | (Value::Number(_), Value::String(_)) => {
                Err(EvaluatorError::EvaluationError("T2009: The expressions on either side of operator \">\" must be of the same data type".to_string()))
            }
            // Boolean comparisons -> T2010
            (Value::Bool(_), _) | (_, Value::Bool(_)) => {
                Err(EvaluatorError::EvaluationError(format!(
                    "T2010: Cannot compare {} and {}",
                    Self::type_name(left), Self::type_name(right)
                )))
            }
            // Other type mismatches
            _ => Err(EvaluatorError::EvaluationError(format!(
                "T2010: Cannot compare {} and {}",
                Self::type_name(left), Self::type_name(right)
            ))),
        }
    }

    /// Greater than or equal comparison
    fn greater_than_or_equal(&self, left: &Value, right: &Value, left_is_explicit_null: bool, right_is_explicit_null: bool) -> Result<Value, EvaluatorError> {
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => {
                Ok(Value::Bool(a.as_f64().unwrap() >= b.as_f64().unwrap()))
            }
            (Value::String(a), Value::String(b)) => Ok(Value::Bool(a >= b)),
            // Null comparisons - distinguish explicit null from undefined
            (Value::Null, Value::Null) => Ok(Value::Null),
            // Explicit null literal with any type (except null) -> T2010 error
            (Value::Null, _) if left_is_explicit_null => {
                Err(EvaluatorError::EvaluationError("T2010: Type mismatch in comparison".to_string()))
            }
            (_, Value::Null) if right_is_explicit_null => {
                Err(EvaluatorError::EvaluationError("T2010: Type mismatch in comparison".to_string()))
            }
            // Boolean with undefined -> T2010 error
            (Value::Bool(_), Value::Null) | (Value::Null, Value::Bool(_)) => {
                Err(EvaluatorError::EvaluationError("T2010: Type mismatch in comparison".to_string()))
            }
            // Number or String with undefined (not explicit null) -> undefined result
            (Value::Number(_), Value::Null) | (Value::Null, Value::Number(_)) |
            (Value::String(_), Value::Null) | (Value::Null, Value::String(_)) => {
                Ok(Value::Null)
            }
            // String vs Number -> T2009
            (Value::String(_), Value::Number(_)) | (Value::Number(_), Value::String(_)) => {
                Err(EvaluatorError::EvaluationError("T2009: The expressions on either side of operator \">=\" must be of the same data type".to_string()))
            }
            // Boolean comparisons -> T2010
            (Value::Bool(_), _) | (_, Value::Bool(_)) => {
                Err(EvaluatorError::EvaluationError(format!(
                    "T2010: Cannot compare {} and {}",
                    Self::type_name(left), Self::type_name(right)
                )))
            }
            // Other type mismatches
            _ => Err(EvaluatorError::EvaluationError(format!(
                "T2010: Cannot compare {} and {}",
                Self::type_name(left), Self::type_name(right)
            ))),
        }
    }

    /// String concatenation
    fn concatenate(&self, left: &Value, right: &Value) -> Result<Value, EvaluatorError> {
        // Convert both values to strings and concatenate
        // Use $string() function for proper number formatting
        let left_str = match left {
            Value::String(s) => s.clone(),
            Value::Number(_) | Value::Bool(_) | Value::Array(_) | Value::Object(_) => {
                // Use $string() function for proper formatting
                match crate::functions::string::string(left, None) {
                    Ok(Value::String(s)) => s,
                    Ok(Value::Null) => String::new(),
                    Ok(_) => return Err(EvaluatorError::TypeError(
                        "Cannot concatenate complex types".to_string(),
                    )),
                    Err(_) => return Err(EvaluatorError::TypeError(
                        "Cannot concatenate complex types".to_string(),
                    )),
                }
            }
            Value::Null => String::new(), // null becomes empty string
        };

        let right_str = match right {
            Value::String(s) => s.clone(),
            Value::Number(_) | Value::Bool(_) | Value::Array(_) | Value::Object(_) => {
                // Use $string() function for proper formatting
                match crate::functions::string::string(right, None) {
                    Ok(Value::String(s)) => s,
                    Ok(Value::Null) => String::new(),
                    Ok(_) => return Err(EvaluatorError::TypeError(
                        "Cannot concatenate complex types".to_string(),
                    )),
                    Err(_) => return Err(EvaluatorError::TypeError(
                        "Cannot concatenate complex types".to_string(),
                    )),
                }
            }
            Value::Null => String::new(), // null becomes empty string
        };

        Ok(Value::String(format!("{}{}", left_str, right_str)))
    }

    /// Range operator (e.g., 1..5 produces [1,2,3,4,5])
    fn range(&self, left: &Value, right: &Value) -> Result<Value, EvaluatorError> {
        // Check left operand is a number or null
        let start_f64 = match left {
            Value::Number(n) => Some(n.as_f64().unwrap()),
            Value::Null => None,
            _ => {
                return Err(EvaluatorError::EvaluationError(
                    "T2003: Left operand of range operator must be a number".to_string(),
                ));
            }
        };

        // Check left operand is an integer (if it's a number)
        if let Some(val) = start_f64 {
            if val.fract() != 0.0 {
                return Err(EvaluatorError::EvaluationError(
                    "T2003: Left operand of range operator must be an integer".to_string(),
                ));
            }
        }

        // Check right operand is a number or null
        let end_f64 = match right {
            Value::Number(n) => Some(n.as_f64().unwrap()),
            Value::Null => None,
            _ => {
                return Err(EvaluatorError::EvaluationError(
                    "T2004: Right operand of range operator must be a number".to_string(),
                ));
            }
        };

        // Check right operand is an integer (if it's a number)
        if let Some(val) = end_f64 {
            if val.fract() != 0.0 {
                return Err(EvaluatorError::EvaluationError(
                    "T2004: Right operand of range operator must be an integer".to_string(),
                ));
            }
        }

        // If either operand is null, return empty array
        if start_f64.is_none() || end_f64.is_none() {
            return Ok(Value::Array(vec![]));
        }

        let start = start_f64.unwrap() as i64;
        let end = end_f64.unwrap() as i64;

        let mut result = Vec::new();
        if start <= end {
            for i in start..=end {
                result.push(serde_json::json!(i));
            }
        }
        // Note: if start > end, return empty array (not reversed)
        Ok(Value::Array(result))
    }

    /// In operator (checks if left is in right array/object)
    /// Array indexing: array[index]
    fn array_index(&self, array: &Value, index: &Value) -> Result<Value, EvaluatorError> {
        match (array, index) {
            (Value::Array(arr), Value::Number(n)) => {
                let idx = n.as_f64().unwrap() as i64;
                let len = arr.len() as i64;

                // Handle negative indexing (offset from end)
                let actual_idx = if idx < 0 {
                    len + idx
                } else {
                    idx
                };

                if actual_idx < 0 || actual_idx >= len {
                    Ok(Value::Null)
                } else {
                    Ok(arr[actual_idx as usize].clone())
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
                    let predicate_result = self.evaluate_internal(rhs_node, item)?;

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
        // If either side is undefined/null, return false (not an error)
        // This matches JavaScript behavior
        if matches!(left, Value::Null) || matches!(right, Value::Null) {
            return Ok(Value::Bool(false));
        }

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
            // If right side is not an array or object (e.g., string, number),
            // wrap it in an array for comparison
            other => {
                Ok(Value::Bool(self.equals(left, other)))
            }
        }
    }

    /// Create a partially applied function from a function call with placeholder arguments
    /// This evaluates non-placeholder arguments and creates a new lambda that takes
    /// the placeholder positions as parameters.
    fn create_partial_application(
        &mut self,
        name: &str,
        args: &[AstNode],
        is_builtin: bool,
        data: &Value,
    ) -> Result<Value, EvaluatorError> {
        // First, look up the function to ensure it exists
        let is_lambda = self.context.lookup_lambda(name).is_some() ||
            (self.context.lookup(name).map(|v| {
                if let Value::Object(map) = v {
                    map.contains_key("__lambda__")
                } else {
                    false
                }
            }).unwrap_or(false));

        // Built-in functions must be called with $ prefix for partial application
        // Without $, it's an error (T1007) suggesting the user forgot the $
        if !is_lambda && !is_builtin {
            // Check if it's a built-in function called without $
            if self.is_builtin_function(name) {
                return Err(EvaluatorError::EvaluationError(
                    format!("T1007: Attempted to partially apply a non-function. Did you mean ${}?", name)
                ));
            }
            return Err(EvaluatorError::EvaluationError(
                "T1008: Attempted to partially apply a non-function".to_string()
            ));
        }

        // Evaluate non-placeholder arguments and track placeholder positions
        let mut bound_args: Vec<(usize, Value)> = Vec::new();
        let mut placeholder_positions: Vec<usize> = Vec::new();

        for (i, arg) in args.iter().enumerate() {
            if matches!(arg, AstNode::Placeholder) {
                placeholder_positions.push(i);
            } else {
                let value = self.evaluate_internal(arg, data)?;
                bound_args.push((i, value));
            }
        }

        // Generate parameter names for each placeholder
        let param_names: Vec<String> = placeholder_positions
            .iter()
            .enumerate()
            .map(|(i, _)| format!("__p{}", i))
            .collect();

        // Store the partial application info as a special lambda
        // When invoked, it will call the original function with bound + placeholder args
        let partial_id = format!(
            "__partial_{}_{}_{}",
            name,
            placeholder_positions.len(),
            bound_args.len()
        );

        // Create a stored lambda that represents this partial application
        // The body is a marker that we'll interpret specially during invocation
        let stored_lambda = StoredLambda {
            params: param_names.clone(),
            body: AstNode::String(format!(
                "__partial_call:{}:{}:{}",
                name,
                is_builtin,
                args.len()
            )),
            signature: None,
            captured_env: {
                let mut env = self.capture_current_environment();
                // Store the bound arguments in the captured environment
                for (pos, value) in &bound_args {
                    env.insert(format!("__bound_arg_{}", pos), value.clone());
                }
                // Store placeholder positions
                env.insert(
                    "__placeholder_positions".to_string(),
                    Value::Array(
                        placeholder_positions
                            .iter()
                            .map(|p| Value::Number(serde_json::Number::from(*p)))
                            .collect()
                    )
                );
                // Store total argument count
                env.insert(
                    "__total_args".to_string(),
                    Value::Number(serde_json::Number::from(args.len()))
                );
                env
            },
        };

        self.context.bind_lambda(partial_id.clone(), stored_lambda);

        // Return a lambda object that can be invoked
        let lambda_obj = serde_json::json!({
            "__lambda__": true,
            "params": param_names,
            "body": format!("partial({})", name),
            "_lambda_id": partial_id,
            "_is_partial": true
        });

        Ok(lambda_obj)
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
        assert_eq!(result, serde_json::json!(42));

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
            steps: vec![PathStep::new(AstNode::Name("foo".to_string()))],
        };
        let result = evaluator.evaluate(&path, &data).unwrap();
        assert_eq!(result, serde_json::json!({"bar": {"baz": 42}}));

        // Nested path
        let path = AstNode::Path {
            steps: vec![
                PathStep::new(AstNode::Name("foo".to_string())),
                PathStep::new(AstNode::Name("bar".to_string())),
                PathStep::new(AstNode::Name("baz".to_string())),
            ],
        };
        let result = evaluator.evaluate(&path, &data).unwrap();
        assert_eq!(result, serde_json::json!(42));

        // Missing path returns null
        let path = AstNode::Path {
            steps: vec![PathStep::new(AstNode::Name("missing".to_string()))],
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
            is_builtin: true,
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, Value::String("HELLO".to_string()));

        // lowercase function
        let expr = AstNode::Function {
            name: "lowercase".to_string(),
            args: vec![AstNode::string("HELLO")],
            is_builtin: true,
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, Value::String("hello".to_string()));

        // length function
        let expr = AstNode::Function {
            name: "length".to_string(),
            args: vec![AstNode::string("hello")],
            is_builtin: true,
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
            is_builtin: true,
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
            is_builtin: true,
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
                PathStep::new(AstNode::Name("metadata".to_string())),
                PathStep::new(AstNode::Name("version".to_string())),
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
            is_builtin: false,
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

    #[test]
    fn test_empty_brackets() {
        use crate::parser::parse;
        use serde_json::json;

        let mut evaluator = Evaluator::new();

        // Test empty brackets on simple value - should wrap in array
        let data = json!({"foo": "bar"});
        let ast = parse("foo[]").unwrap();
        let result = evaluator.evaluate(&ast, &data).unwrap();
        assert_eq!(result, json!(["bar"]), "Empty brackets should wrap value in array");

        // Test empty brackets on array - should return array as-is
        let data2 = json!({"arr": [1, 2, 3]});
        let ast2 = parse("arr[]").unwrap();
        let result2 = evaluator.evaluate(&ast2, &data2).unwrap();
        assert_eq!(result2, json!([1, 2, 3]), "Empty brackets should preserve array");
    }
}

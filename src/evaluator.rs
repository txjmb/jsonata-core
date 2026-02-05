// Expression evaluator
// Mirrors jsonata.js from the reference implementation

use std::collections::HashMap;

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
    matches!(value, Value::Object(obj) if obj.get("__undefined__") == Some(&Value::Bool(true)))
}

/// Functions that propagate undefined (return undefined when given an undefined argument).
/// These functions should return null/undefined when their input path doesn't exist,
/// rather than throwing a type error.
const UNDEFINED_PROPAGATING_FUNCTIONS: &[&str] = &[
    "not", "boolean", "length", "number", "uppercase", "lowercase",
    "substring", "substringBefore", "substringAfter", "string",
];

/// Check whether a function propagates undefined values
fn propagates_undefined(name: &str) -> bool {
    UNDEFINED_PROPAGATING_FUNCTIONS.contains(&name)
}

/// Flatten an array for aggregation functions (sum, average, max, min)
/// JSONata aggregation functions work on flattened arrays
fn flatten_for_aggregation(arr: &[Value]) -> Vec<Value> {
    let mut result = Vec::new();
    for value in arr {
        match value {
            Value::Array(inner) => {
                // Recursively flatten nested arrays
                result.extend(flatten_for_aggregation(inner));
            }
            _ => result.push(value.clone()),
        }
    }
    result
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

impl From<crate::functions::FunctionError> for EvaluatorError {
    fn from(e: crate::functions::FunctionError) -> Self {
        EvaluatorError::EvaluationError(e.to_string())
    }
}

impl From<crate::datetime::DateTimeError> for EvaluatorError {
    fn from(e: crate::datetime::DateTimeError) -> Self {
        EvaluatorError::EvaluationError(e.to_string())
    }
}

/// Result of evaluating a lambda body that may be a tail call
/// Used for trampoline-based tail call optimization
enum LambdaResult {
    /// Final value - evaluation is complete
    Value(Value),
    /// Tail call - need to continue with another lambda invocation
    TailCall {
        /// The lambda to call
        lambda: StoredLambda,
        /// Arguments for the call
        args: Vec<Value>,
        /// Data context for the call
        data: Value,
    },
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
    pub captured_env: HashMap<String, Value>,
    /// Captured data context for lexical scoping of bare field names
    pub captured_data: Option<Value>,
    /// Whether this lambda's body contains tail calls that can be optimized
    pub thunk: bool,
}

/// Evaluation context
///
/// Holds variable bindings and other state needed during evaluation
pub struct Context {
    pub(crate) bindings: HashMap<String, Value>,
    pub(crate) lambdas: HashMap<String, StoredLambda>,
    parent_data: Option<Value>,
}

impl Context {
    pub fn new() -> Self {
        Context {
            bindings: HashMap::new(),
            lambdas: HashMap::new(),
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
            // Limit recursion depth to prevent stack overflow
            // True TCO would allow deeper recursion but requires parser-level thunk marking
            max_recursion_depth: 302,
        }
    }

    pub fn with_context(context: Context) -> Self {
        Evaluator {
            context,
            recursion_depth: 0,
            max_recursion_depth: 302,
        }
    }

    /// Invoke a stored lambda with its captured environment and data.
    /// This is the standard way to call a StoredLambda, handling the
    /// captured_env and captured_data extraction boilerplate.
    fn invoke_stored_lambda(
        &mut self,
        stored: &StoredLambda,
        args: &[Value],
        data: &Value,
    ) -> Result<Value, EvaluatorError> {
        let captured_env = if stored.captured_env.is_empty() {
            None
        } else {
            Some(&stored.captured_env)
        };
        let captured_data = stored.captured_data.as_ref();
        self.invoke_lambda_with_env(
            &stored.params,
            &stored.body,
            stored.signature.as_ref(),
            args,
            data,
            captured_env,
            captured_data,
            stored.thunk,
        )
    }

    /// Look up a StoredLambda from a Value that may be a lambda marker object.
    /// Returns the cloned StoredLambda if the value is an object with `__lambda__`
    /// and a valid `_lambda_id` that references a stored lambda.
    fn lookup_lambda_from_value(&self, value: &Value) -> Option<StoredLambda> {
        if let Value::Object(map) = value {
            if map.contains_key("__lambda__") {
                if let Some(Value::String(lambda_id)) = map.get("_lambda_id") {
                    return self.context.lookup_lambda(lambda_id).cloned();
                }
            }
        }
        None
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
            AstNode::String(s) => Ok(Value::String(s.clone())),

            // Name nodes represent field access on the current data
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

            AstNode::Variable(name) => {
                // Special case: $ alone (empty name) refers to current context
                // First check if $ is bound in the context (for closures that captured $)
                // Otherwise, use the data parameter
                if name.is_empty() {
                    if let Some(value) = self.context.lookup("$") {
                        return Ok(value.clone());
                    }
                    // If data is a tuple, return the @ value
                    if let Value::Object(obj) = data {
                        if obj.get("__tuple__") == Some(&Value::Bool(true)) {
                            if let Some(inner) = obj.get("@") {
                                return Ok(inner.clone());
                            }
                        }
                    }
                    return Ok(data.clone());
                }

                // Check variable bindings FIRST
                // This allows function parameters to shadow outer lambdas with the same name
                // Critical for Y-combinator pattern: function($g){$g($g)} where $g shadows outer $g
                if let Some(value) = self.context.lookup(name) {
                    return Ok(value.clone());
                }

                // Check tuple bindings in data (for index binding operator #$var)
                // When iterating over a tuple stream, $var can reference the bound index
                if let Value::Object(obj) = data {
                    if obj.get("__tuple__") == Some(&Value::Bool(true)) {
                        // Check for the variable in tuple bindings (stored as "$name")
                        let binding_key = format!("${}", name);
                        if let Some(binding_value) = obj.get(&binding_key) {
                            return Ok(binding_value.clone());
                        }
                    }
                }

                // Then check if this is a stored lambda (user-defined functions)
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

            AstNode::Path { steps } => self.evaluate_path(steps, data),

            AstNode::Binary { op, lhs, rhs } => {
                self.evaluate_binary_op(*op, lhs, rhs, data)
            }

            AstNode::Unary { op, operand } => {
                self.evaluate_unary_op(*op, operand, data)
            }

            // Array constructor - JSONata semantics:
            AstNode::Array(elements) => {
                // - If element is itself an array constructor [...], keep it nested
                // - Otherwise, if element evaluates to an array, flatten it
                // - Undefined values are excluded
                let mut result = Vec::new();
                for element in elements {
                    // Check if this element is itself an explicit array constructor
                    let is_array_constructor = matches!(element, AstNode::Array(_));

                    let value = self.evaluate_internal(element, data)?;

                    // Skip undefined values in array constructors
                    // Note: explicit null is preserved, only undefined (no value) is filtered
                    if is_undefined(&value) {
                        continue;
                    }

                    if is_array_constructor {
                        // Explicit array constructor - keep nested
                        result.push(value);
                    } else if let Value::Array(arr) = value {
                        // Non-array-constructor that evaluated to array - flatten it
                        result.extend(arr);
                    } else {
                        // Non-array value - add as-is
                        result.push(value);
                    }
                }
                Ok(Value::Array(result))
            }

            AstNode::Object(pairs) => {
                let mut result = serde_json::Map::new();
                // Track which pair index produced each key (for D1009 checking)
                let mut key_sources: HashMap<String, usize> =
                    HashMap::new();

                for (pair_index, (key_node, value_node)) in pairs.iter().enumerate() {
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
                            )));
                        }
                    };

                    // Check for D1009: multiple key expressions evaluate to same key
                    if let Some(&existing_idx) = key_sources.get(&key) {
                        if existing_idx != pair_index {
                            return Err(EvaluatorError::EvaluationError(format!(
                                "D1009: Multiple key expressions evaluate to same key: {}",
                                key
                            )));
                        }
                    }
                    key_sources.insert(key.clone(), pair_index);

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

            // Object transform: group items by key, then evaluate value once per group
            AstNode::ObjectTransform { input, pattern } => {
                // Evaluate the input expression
                let input_value = self.evaluate_internal(input, data)?;

                // If input is undefined, return undefined (not empty object)
                if is_undefined(&input_value) {
                    return Ok(undefined_value());
                }

                // Handle array input - process each item
                let items: Vec<Value> = match input_value {
                    Value::Array(ref arr) => arr.clone(),
                    Value::Null => return Ok(Value::Null),
                    other => vec![other],
                };

                // If array is empty, add undefined to enable literal JSON object generation
                let items = if items.is_empty() {
                    vec![undefined_value()]
                } else {
                    items
                };

                // Phase 1: Group items by key expression
                // groups maps key -> (grouped_data, expr_index)
                // When multiple items have same key, their data is appended together
                let mut groups: HashMap<String, (Vec<Value>, usize)> =
                    HashMap::new();

                // Save the current $ binding to restore later
                let saved_dollar = self.context.lookup("$").cloned();

                for item in &items {
                    // Bind $ to the current item for key evaluation
                    self.context.bind("$".to_string(), item.clone());

                    for (pair_index, (key_node, _value_node)) in pattern.iter().enumerate() {
                        // Evaluate key with current item as context
                        let key = match self.evaluate_internal(key_node, item)? {
                            Value::String(s) => s,
                            Value::Null => continue, // Skip null keys
                            other => {
                                // Skip undefined keys
                                if is_undefined(&other) {
                                    continue;
                                }
                                return Err(EvaluatorError::TypeError(format!(
                                    "T1003: Object key must be a string, got: {:?}",
                                    other
                                )));
                            }
                        };

                        // Group items by key
                        if let Some((existing_data, existing_idx)) = groups.get_mut(&key) {
                            // Key already exists - check if from same expression index
                            if *existing_idx != pair_index {
                                // D1009: multiple key expressions evaluate to same key
                                return Err(EvaluatorError::EvaluationError(format!(
                                    "D1009: Multiple key expressions evaluate to same key: {}",
                                    key
                                )));
                            }
                            // Append item to the group
                            existing_data.push(item.clone());
                        } else {
                            // New key - create new group
                            groups.insert(key, (vec![item.clone()], pair_index));
                        }
                    }
                }

                // Phase 2: Evaluate value expression for each group
                let mut result = serde_json::Map::new();

                for (key, (grouped_data, expr_index)) in groups {
                    // Get the value expression for this group
                    let (_key_node, value_node) = &pattern[expr_index];

                    // Determine the context for value evaluation:
                    // - If single item, use that item directly
                    // - If multiple items, use the array of items
                    let context = if grouped_data.len() == 1 {
                        grouped_data.into_iter().next().unwrap()
                    } else {
                        Value::Array(grouped_data)
                    };

                    // Bind $ to the context for value evaluation
                    self.context.bind("$".to_string(), context.clone());

                    // Evaluate value expression with grouped context
                    let value = self.evaluate_internal(value_node, &context)?;

                    // Skip undefined values
                    if !is_undefined(&value) {
                        result.insert(key, value);
                    }
                }

                // Restore the previous $ binding
                if let Some(saved) = saved_dollar {
                    self.context.bind("$".to_string(), saved);
                } else {
                    self.context.unbind("$");
                }

                Ok(Value::Object(result))
            }

            AstNode::Function { name, args, is_builtin } => {
                self.evaluate_function_call(name, args, *is_builtin, data)
            }

            // Call: invoke an arbitrary expression as a function
            // Used for IIFE patterns like (function($x){...})(5) or chained calls
            AstNode::Call { procedure, args } => {
                // Evaluate the procedure to get the callable value
                let callable = self.evaluate_internal(procedure, data)?;

                // Check if it's a lambda (object with __lambda__ key)
                if let Some(stored_lambda) = self.lookup_lambda_from_value(&callable) {
                    let mut evaluated_args = Vec::with_capacity(args.len());
                    for arg in args.iter() {
                        evaluated_args.push(self.evaluate_internal(arg, data)?);
                    }
                    return self.invoke_stored_lambda(&stored_lambda, &evaluated_args, data);
                }

                // Not a callable value
                Err(EvaluatorError::TypeError(format!(
                    "Cannot call non-function value: {:?}",
                    callable
                )))
            }

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

            AstNode::Block(expressions) => {
                // Blocks create a new scope - save current bindings
                let saved_bindings = self.context.bindings.clone();
                let saved_lambdas = self.context.lambdas.clone();

                let mut result = Value::Null;
                for expr in expressions {
                    result = self.evaluate_internal(expr, data)?;
                }

                // Before restoring, preserve any lambdas referenced by the result
                // This is essential for closures returned from blocks (IIFE pattern)
                let lambdas_to_keep = self.extract_lambda_ids(&result);
                let current_lambdas = self.context.lambdas.clone();

                // Restore original bindings after block completes
                self.context.bindings = saved_bindings;
                self.context.lambdas = saved_lambdas;

                // Re-add any lambdas that are referenced by the returned value
                for lambda_id in lambdas_to_keep {
                    if let Some(stored_lambda) = current_lambdas.get(&lambda_id) {
                        self.context.lambdas.insert(lambda_id, stored_lambda.clone());
                    }
                }

                Ok(result)
            }

            // Lambda: capture current environment for closure support
            AstNode::Lambda { params, body, signature, thunk } => {
                let lambda_id = format!("__lambda_{}_{:p}", params.len(), body.as_ref());

                let stored_lambda = StoredLambda {
                    params: params.clone(),
                    body: (**body).clone(),
                    signature: signature.clone(),
                    captured_env: self.capture_current_environment(),
                    captured_data: Some(data.clone()),
                    thunk: *thunk,
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

            // Wildcard: collect all values from current object
            AstNode::Wildcard => {
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

            // Descendant: recursively traverse all nested values
            AstNode::Descendant => {
                let descendants = self.collect_descendants(data);
                if descendants.is_empty() {
                    Ok(Value::Null) // No descendants means undefined
                } else {
                    Ok(Value::Array(descendants))
                }
            }

            AstNode::Predicate(_) => {
                Err(EvaluatorError::EvaluationError(
                    "Predicate can only be used in path expressions".to_string()
                ))
            }

            // Array grouping: same as Array but prevents flattening in path contexts
            AstNode::ArrayGroup(elements) => {
                let mut result = Vec::new();
                for element in elements {
                    let value = self.evaluate_internal(element, data)?;
                    result.push(value);
                }
                Ok(Value::Array(result))
            }

            AstNode::FunctionApplication(_) => {
                Err(EvaluatorError::EvaluationError(
                    "Function application can only be used in path expressions".to_string()
                ))
            }

            AstNode::Sort { input, terms } => {
                let value = self.evaluate_internal(input, data)?;
                self.evaluate_sort(&value, terms)
            }

            // Index binding: evaluates input and creates tuple stream with index variable
            AstNode::IndexBind { input, variable } => {
                let value = self.evaluate_internal(input, data)?;

                // Store the variable name and create indexed results
                // This is a simplified implementation - full tuple stream would require more work
                match value {
                    Value::Array(arr) => {
                        // Store the index binding metadata in a special wrapper
                        let mut result = Vec::new();
                        for (idx, item) in arr.iter().enumerate() {
                            // Create wrapper object with value and index
                            let mut wrapper = serde_json::Map::new();
                            wrapper.insert("@".to_string(), item.clone());
                            wrapper.insert(format!("${}", variable), serde_json::json!(idx));
                            wrapper.insert("__tuple__".to_string(), Value::Bool(true));
                            result.push(Value::Object(wrapper));
                        }
                        Ok(Value::Array(result))
                    }
                    // Single value: just return as-is with index 0
                    other => {
                        let mut wrapper = serde_json::Map::new();
                        wrapper.insert("@".to_string(), other);
                        wrapper.insert(format!("${}", variable), serde_json::json!(0));
                        wrapper.insert("__tuple__".to_string(), Value::Bool(true));
                        Ok(Value::Object(wrapper))
                    }
                }
            }

            // Transform: |location|update[,delete]|
            AstNode::Transform { location, update, delete } => {

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
                        captured_env: HashMap::new(),
                        captured_data: None, // Transform takes $ as parameter
                        thunk: false,
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

                // Try to evaluate the predicate to see if it's a numeric index or array of indices
                // If evaluation succeeds and yields a number, use it as an index
                // If it yields an array of numbers, use them as multiple indices
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
                    Ok(Value::Array(indices)) => {
                        // Array of values - could be indices or filter results
                        // Check if all values are numeric
                        let has_non_numeric = indices.iter().any(|v| !matches!(v, Value::Number(_)));

                        if has_non_numeric {
                            // Non-numeric values - treat as filter, fall through
                        } else {
                            // All numeric - use as indices
                            let arr_len = arr.len() as i64;
                            let mut resolved_indices: Vec<i64> = indices
                                .iter()
                                .filter_map(|v| {
                                    if let Value::Number(n) = v {
                                        let idx = n.as_f64().unwrap() as i64;
                                        // Resolve negative indices
                                        let actual_idx = if idx < 0 { arr_len + idx } else { idx };
                                        // Only include valid indices
                                        if actual_idx >= 0 && actual_idx < arr_len {
                                            Some(actual_idx)
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    }
                                })
                                .collect();

                            // Sort and deduplicate indices
                            resolved_indices.sort();
                            resolved_indices.dedup();

                            // Select elements at each sorted index
                            let result: Vec<Value> = resolved_indices
                                .iter()
                                .map(|&idx| arr[idx as usize].clone())
                                .collect();

                            return Ok(Value::Array(result));
                        }
                    }
                    Ok(_) => {
                        // Evaluated successfully but not a number or array - might be a filter
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
                        // Check if this is a tuple - extract '@' value
                        if obj.get("__tuple__") == Some(&Value::Bool(true)) {
                            if let Some(Value::Object(inner)) = obj.get("@") {
                                Ok(inner.get(field_name).cloned().unwrap_or(Value::Null))
                            } else {
                                Ok(Value::Null)
                            }
                        } else {
                            Ok(obj.get(field_name).cloned().unwrap_or(Value::Null))
                        }
                    }
                    Value::Array(arr) => {
                        // Array mapping: extract field from each element
                        let mut result = Vec::new();
                        for item in arr {
                            match item {
                                Value::Object(obj) => {
                                    // Check if this is a tuple - extract '@' value and preserve bindings
                                    let (actual_obj, tuple_bindings) = if obj.get("__tuple__") == Some(&Value::Bool(true)) {
                                        if let Some(Value::Object(inner)) = obj.get("@") {
                                            let bindings: Vec<(String, Value)> = obj.iter()
                                                .filter(|(k, _)| k.starts_with('$'))
                                                .map(|(k, v)| (k.clone(), v.clone()))
                                                .collect();
                                            (inner.clone(), Some(bindings))
                                        } else {
                                            continue;
                                        }
                                    } else {
                                        (obj.clone(), None)
                                    };

                                    let val = actual_obj.get(field_name).cloned().unwrap_or(Value::Null);
                                    // Flatten array values, push scalars (matching multi-step path behavior)
                                    if !matches!(val, Value::Null) {
                                        // Helper to wrap in tuple
                                        let wrap = |v: Value| -> Value {
                                            if let Some(ref b) = tuple_bindings {
                                                let mut wrapper = serde_json::Map::new();
                                                wrapper.insert("@".to_string(), v);
                                                wrapper.insert("__tuple__".to_string(), Value::Bool(true));
                                                for (k, val) in b {
                                                    wrapper.insert(k.clone(), val.clone());
                                                }
                                                Value::Object(wrapper)
                                            } else {
                                                v
                                            }
                                        };

                                        match val {
                                            Value::Array(arr_val) => {
                                                for item in arr_val {
                                                    result.push(wrap(item));
                                                }
                                            },
                                            other => result.push(wrap(other)),
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
            AstNode::IndexBind { .. } => {
                // Index binding as first step - evaluate the IndexBind to create tuple array
                self.evaluate_internal(&steps[0].node, data)?
            }
            _ => {
                // Complex first step - evaluate it
                self.evaluate_path_step(&steps[0].node, data, data)?
            }
        };

        // Process remaining steps
        for step in steps[1..].iter() {
            // Early return if current is null/undefined - no point continuing
            // This handles cases like `blah.{}` where blah doesn't exist
            if matches!(current, Value::Null) || is_undefined(&current) {
                return Ok(Value::Null);
            }

            // Check if current is a tuple array - if so, we need to bind tuple variables
            // to context so they're available in nested expressions (like predicates)
            let is_tuple_array = if let Value::Array(arr) = &current {
                arr.first().map_or(false, |first| {
                    if let Value::Object(obj) = first {
                        obj.get("__tuple__") == Some(&Value::Bool(true))
                    } else {
                        false
                    }
                })
            } else {
                false
            };

            // For tuple arrays with certain step types, we need special handling to bind
            // tuple variables to context so they're available in nested expressions.
            // This is needed for:
            // - Object constructors: {"label": $$.items[$i]} needs $i in context
            // - Function applications: .($$.items[$i]) needs $i in context
            // - Variable lookups: .$i needs to find the tuple binding
            //
            // Steps like Name (field access) already have proper tuple handling in their
            // specific cases, so we don't intercept those here.
            let needs_tuple_context_binding = is_tuple_array && matches!(
                &step.node,
                AstNode::Object(_) | AstNode::FunctionApplication(_) | AstNode::Variable(_)
            );

            if needs_tuple_context_binding {
                if let Value::Array(arr) = &current {
                    let mut results = Vec::new();

                    for tuple in arr {
                        if let Value::Object(tuple_obj) = tuple {
                            // Extract tuple bindings (variables starting with $)
                            let bindings: Vec<(String, Value)> = tuple_obj.iter()
                                .filter(|(k, _)| k.starts_with('$') && k.len() > 1)  // $i, $j, etc.
                                .map(|(k, v)| (k[1..].to_string(), v.clone()))  // Remove $ prefix for context binding
                                .collect();

                            // Save current bindings
                            let saved_bindings: Vec<(String, Option<Value>)> = bindings.iter()
                                .map(|(name, _)| (name.clone(), self.context.lookup(name).cloned()))
                                .collect();

                            // Bind tuple variables to context
                            for (name, value) in &bindings {
                                self.context.bind(name.clone(), value.clone());
                            }

                            // Get the actual value from the tuple (@ field)
                            let actual_data = tuple_obj.get("@").cloned().unwrap_or(Value::Null);

                            // Evaluate the step
                            let step_result = match &step.node {
                                AstNode::Variable(_) => {
                                    // Variable lookup - check context (which now has bindings)
                                    self.evaluate_internal(&step.node, tuple)?
                                }
                                AstNode::Object(_) | AstNode::FunctionApplication(_) => {
                                    // Object constructor or function application - evaluate on actual data
                                    self.evaluate_internal(&step.node, &actual_data)?
                                }
                                _ => unreachable!()  // We only match specific types above
                            };

                            // Restore previous bindings
                            for (name, saved_value) in &saved_bindings {
                                if let Some(value) = saved_value {
                                    self.context.bind(name.clone(), value.clone());
                                } else {
                                    self.context.unbind(name);
                                }
                            }

                            // Collect result
                            if !matches!(step_result, Value::Null) && !is_undefined(&step_result) {
                                // For object constructors, collect results directly
                                // For other steps, handle arrays
                                if matches!(&step.node, AstNode::Object(_)) {
                                    results.push(step_result);
                                } else if matches!(step_result, Value::Array(_)) {
                                    if let Value::Array(arr) = step_result {
                                        results.extend(arr);
                                    }
                                } else {
                                    results.push(step_result);
                                }
                            }
                        }
                    }

                    current = Value::Array(results);
                    continue;  // Skip the regular step processing
                }
            }

            current = match &step.node {
                AstNode::Wildcard => {
                    // Wildcard in path
                    let stages = &step.stages;
                    let wildcard_result = match &current {
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
                    };

                    // Apply stages (predicates) if present
                    if !stages.is_empty() {
                        self.apply_stages(wildcard_result, stages)?
                    } else {
                        wildcard_result
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
                            // Single object field extraction - NOT array mapping
                            // This resets did_array_mapping because we're extracting from
                            // a single value, not mapping over an array. The field's value
                            // (even if it's an array) should be preserved as-is.
                            did_array_mapping = false;
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
                                        // Check if this is a tuple stream element
                                        let (actual_obj, tuple_bindings) = if obj.get("__tuple__") == Some(&Value::Bool(true)) {
                                            // This is a tuple - extract '@' value and preserve bindings
                                            if let Some(Value::Object(inner)) = obj.get("@") {
                                                // Collect index bindings (variables starting with $)
                                                let bindings: Vec<(String, Value)> = obj.iter()
                                                    .filter(|(k, _)| k.starts_with('$'))
                                                    .map(|(k, v)| (k.clone(), v.clone()))
                                                    .collect();
                                                (inner.clone(), Some(bindings))
                                            } else {
                                                continue; // Invalid tuple
                                            }
                                        } else {
                                            (obj.clone(), None)
                                        };

                                        let val = actual_obj.get(field_name).cloned().unwrap_or(Value::Null);

                                        if !matches!(val, Value::Null) {
                                            // Helper to wrap value in tuple if we have bindings
                                            let wrap_in_tuple = |v: Value, bindings: &Option<Vec<(String, Value)>>| -> Value {
                                                if let Some(b) = bindings {
                                                    let mut wrapper = serde_json::Map::new();
                                                    wrapper.insert("@".to_string(), v);
                                                    wrapper.insert("__tuple__".to_string(), Value::Bool(true));
                                                    for (k, val) in b {
                                                        wrapper.insert(k.clone(), val.clone());
                                                    }
                                                    Value::Object(wrapper)
                                                } else {
                                                    v
                                                }
                                            };

                                            if !stages.is_empty() {
                                                // Apply stages to the extracted value
                                                let processed_val = self.apply_stages(val, stages)?;
                                                // Stages always return an array (or null); extend results
                                                match processed_val {
                                                    Value::Array(arr) => {
                                                        for item in arr {
                                                            result.push(wrap_in_tuple(item, &tuple_bindings));
                                                        }
                                                    },
                                                    Value::Null => {}, // Skip nulls from stage application
                                                    other => result.push(wrap_in_tuple(other, &tuple_bindings)),
                                                }
                                            } else {
                                                // No stages: flatten arrays, push scalars
                                                // But preserve tuple bindings!
                                                match val {
                                                    Value::Array(arr) => {
                                                        for item in arr {
                                                            result.push(wrap_in_tuple(item, &tuple_bindings));
                                                        }
                                                    },
                                                    other => result.push(wrap_in_tuple(other, &tuple_bindings)),
                                                }
                                            }
                                        }
                                    }
                                    Value::Array(_) => {
                                        // Recursively map over nested array
                                        let nested_result = self.evaluate_path(&[step.clone()], item)?;
                                        match nested_result {
                                            Value::Array(nested) => result.extend(nested),
                                            Value::Null => {},
                                            other => result.push(other),
                                        }
                                    }
                                    _ => {},
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
                                    // If the element is an Array/ArrayGroup, preserve its structure (don't flatten)
                                    // This ensures [[expr]] produces properly nested arrays
                                    let should_preserve_array = matches!(element,
                                        AstNode::Array(_) | AstNode::ArrayGroup(_));

                                    if should_preserve_array {
                                        // Keep the array as a single element to preserve nesting
                                        group_values.push(value);
                                    } else {
                                        // Flatten the value into group_values
                                        match value {
                                            Value::Array(arr) => group_values.extend(arr),
                                            other => group_values.push(other),
                                        }
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
                            // Don't do singleton unwrapping here - let the path result
                            // handling deal with it, which respects has_explicit_array_keep
                            Value::Array(result)
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
                AstNode::Sort { terms, .. } => {
                    // Sort as a path step - sort 'current' by the terms
                    self.evaluate_sort(&current, terms)?
                }
                AstNode::IndexBind { variable, .. } => {
                    // Index binding as a path step - creates tuple stream from current
                    // This wraps each element with an index binding
                    match &current {
                        Value::Array(arr) => {
                            let mut result = Vec::new();
                            for (idx, item) in arr.iter().enumerate() {
                                let mut wrapper = serde_json::Map::new();
                                wrapper.insert("@".to_string(), item.clone());
                                wrapper.insert(format!("${}", variable), serde_json::json!(idx));
                                wrapper.insert("__tuple__".to_string(), Value::Bool(true));
                                result.push(Value::Object(wrapper));
                            }
                            Value::Array(result)
                        }
                        other => {
                            // Single value: wrap with index 0
                            let mut wrapper = serde_json::Map::new();
                            wrapper.insert("@".to_string(), other.clone());
                            wrapper.insert(format!("${}", variable), serde_json::json!(0));
                            wrapper.insert("__tuple__".to_string(), Value::Bool(true));
                            Value::Object(wrapper)
                        }
                    }
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
                let crate::ast::Stage::Filter(pred) = stage;
                matches!(**pred, AstNode::Boolean(true))
            })
        });

        // Unwrap when:
        // 1. Any step has stages (predicates, sorts, etc.) which are array operations, OR
        // 2. We did array mapping during step evaluation (tracked via did_array_mapping flag)
        //    Note: did_array_mapping is reset to false when extracting from a single object,
        //    so a[0].b where a[0] returns a single object and .b extracts a field will NOT unwrap.
        // BUT NOT when there's an explicit array-keeping operation
        //
        // Important: We DON'T unwrap just because original data was an array - what matters is
        // whether the final extraction was from an array mapping context or a single object.
        let should_unwrap = !has_explicit_array_keep && (
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
            // Special case: array.$ should map $ over the array, returning each element
            // e.g., [1, 2, 3].$ returns [1, 2, 3]
            if let AstNode::Variable(name) = step {
                if name.is_empty() {
                    // Bare $ - map over array if current is an array
                    if let Value::Array(arr) = current {
                        // Map $ over each element - $ refers to each element in turn
                        return Ok(Value::Array(arr.clone()));
                    } else {
                        // For non-arrays, $ refers to the current value
                        return Ok(current.clone());
                    }
                }
            }

            // Special case: Variable access on tuple arrays (from index binding #$var)
            // When current is a tuple array, we need to evaluate the variable against each tuple
            // so that tuple bindings ($i, etc.) can be found
            if matches!(step, AstNode::Variable(_)) {
                if let Value::Array(arr) = current {
                    // Check if this is a tuple array
                    let is_tuple_array = arr.first().map_or(false, |first| {
                        if let Value::Object(obj) = first {
                            obj.get("__tuple__") == Some(&Value::Bool(true))
                        } else {
                            false
                        }
                    });

                    if is_tuple_array {
                        // Map the variable lookup over each tuple
                        let mut results = Vec::new();
                        for tuple in arr {
                            // Evaluate the variable in the context of this tuple
                            // This allows tuple bindings ($i, etc.) to be found
                            let val = self.evaluate_internal(step, tuple)?;
                            if !matches!(val, Value::Null) && !is_undefined(&val) {
                                results.push(val);
                            }
                        }
                        return Ok(Value::Array(results));
                    }
                }
            }

            // For certain operations (Binary, Function calls, Variables, ParentVariables, Arrays, Objects, Sort, Blocks), the step evaluates to a new value
            // rather than being used to index/access the current value
            // e.g., items[price > 50] where [price > 50] is a filter operation
            // or $x.price where $x is a variable binding
            // or $$.field where $$ is the parent context
            // or [0..9] where it's an array constructor
            // or $^(field) where it's a sort operator
            // or (expr).field where (expr) is a block that evaluates to a value
            if matches!(step, AstNode::Binary { .. } | AstNode::Function { .. } | AstNode::Variable(_) | AstNode::ParentVariable(_) | AstNode::Array(_) | AstNode::Object(_) | AstNode::Sort { .. } | AstNode::Block(_)) {
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

            // Early check: if LHS evaluates to undefined, return undefined
            // This matches JSONata behavior where undefined ~> anyFunc returns undefined
            let lhs_value_for_check = self.evaluate_internal(lhs, data)?;
            if is_undefined(&lhs_value_for_check) || matches!(lhs_value_for_check, Value::Null) {
                return Ok(undefined_value());
            }

            // Handle different RHS types
            match rhs {
                AstNode::Function { name, args, is_builtin } => {
                    // RHS is a function call
                    // Check if the function call has placeholder arguments (partial application)
                    let has_placeholder = args.iter().any(|arg| matches!(arg, AstNode::Placeholder));

                    if has_placeholder {
                        // Partial application: replace the first placeholder with LHS value
                        let lhs_value = self.evaluate_internal(lhs, data)?;
                        let mut filled_args = Vec::new();
                        let mut lhs_used = false;

                        for arg in args.iter() {
                            if matches!(arg, AstNode::Placeholder) && !lhs_used {
                                // Replace first placeholder with evaluated LHS
                                // We need to create a temporary binding to pass the value
                                let temp_name = format!("__pipe_arg_{}", filled_args.len());
                                self.context.bind(temp_name.clone(), lhs_value.clone());
                                filled_args.push(AstNode::Variable(temp_name));
                                lhs_used = true;
                            } else {
                                filled_args.push(arg.clone());
                            }
                        }

                        // Evaluate the function with filled args
                        let result = self.evaluate_function_call(name, &filled_args, *is_builtin, data);

                        // Clean up temp bindings
                        for (i, arg) in args.iter().enumerate() {
                            if matches!(arg, AstNode::Placeholder) {
                                self.context.unbind(&format!("__pipe_arg_{}", i));
                            }
                        }

                        // Unwrap singleton results from chain operator
                        return result.map(|v| self.unwrap_singleton(v));
                    } else {
                        // No placeholders: build args list with LHS as first argument
                        let mut all_args = vec![lhs.clone()];
                        all_args.extend_from_slice(args);
                        // Unwrap singleton results from chain operator
                        return self.evaluate_function_call(name, &all_args, *is_builtin, data)
                            .map(|v| self.unwrap_singleton(v));
                    }
                }
                AstNode::Variable(var_name) => {
                    // RHS is a function reference (no parens)
                    // e.g., $average($tempReadings) ~> $round
                    let all_args = vec![lhs.clone()];
                    // Unwrap singleton results from chain operator
                    return self.evaluate_function_call(var_name, &all_args, true, data)
                        .map(|v| self.unwrap_singleton(v));
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

                    // Unwrap singleton results from chain operator
                    return result.map(|v| self.unwrap_singleton(v));
                }
                AstNode::Lambda { params, body, signature, thunk } => {
                    // RHS is a lambda - invoke it with LHS as argument
                    let lhs_value = self.evaluate_internal(lhs, data)?;
                    // Unwrap singleton results from chain operator
                    return self.invoke_lambda(params, body, signature.as_ref(), &[lhs_value], data, *thunk)
                        .map(|v| self.unwrap_singleton(v));
                }
                AstNode::Path { steps } => {
                    // RHS is a path expression (e.g., function call with predicate: $map($f)[])
                    // If the first step is a function call, we need to add LHS as first argument
                    if let Some(first_step) = steps.first() {
                        match &first_step.node {
                            AstNode::Function { name, args, is_builtin } => {
                                // Prepend LHS to the function arguments
                                let mut all_args = vec![lhs.clone()];
                                all_args.extend_from_slice(args);

                                // Call the function
                                let mut result = self.evaluate_function_call(name, &all_args, *is_builtin, data)?;

                                // Apply stages from the first step (e.g., predicates)
                                for stage in &first_step.stages {
                                    match stage {
                                        Stage::Filter(filter_expr) => {
                                            result = self.evaluate_predicate_as_stage(&result, filter_expr)?;
                                        }
                                    }
                                }

                                // Apply remaining path steps if any
                                if steps.len() > 1 {
                                    let remaining_path = AstNode::Path {
                                        steps: steps[1..].to_vec(),
                                    };
                                    result = self.evaluate_internal(&remaining_path, &result)?;
                                }

                                // Unwrap singleton results from chain operator, unless there are stages
                                // Stages (like predicates) indicate we want to preserve array structure
                                if !first_step.stages.is_empty() || steps.len() > 1 {
                                    return Ok(result);
                                } else {
                                    return Ok(self.unwrap_singleton(result));
                                }
                            }
                            AstNode::Variable(var_name) => {
                                // Variable that should resolve to a function
                                let all_args = vec![lhs.clone()];
                                let mut result = self.evaluate_function_call(var_name, &all_args, true, data)?;

                                // Apply stages from the first step
                                for stage in &first_step.stages {
                                    match stage {
                                        Stage::Filter(filter_expr) => {
                                            result = self.evaluate_predicate_as_stage(&result, filter_expr)?;
                                        }
                                    }
                                }

                                // Apply remaining path steps if any
                                if steps.len() > 1 {
                                    let remaining_path = AstNode::Path {
                                        steps: steps[1..].to_vec(),
                                    };
                                    result = self.evaluate_internal(&remaining_path, &result)?;
                                }

                                // Unwrap singleton results from chain operator, unless there are stages
                                // Stages (like predicates) indicate we want to preserve array structure
                                if !first_step.stages.is_empty() || steps.len() > 1 {
                                    return Ok(result);
                                } else {
                                    return Ok(self.unwrap_singleton(result));
                                }
                            }
                            _ => {
                                // Other path types - just evaluate normally with LHS as context
                                let lhs_value = self.evaluate_internal(lhs, data)?;
                                return self.evaluate_internal(rhs, &lhs_value)
                                    .map(|v| self.unwrap_singleton(v));
                            }
                        }
                    }

                    // Empty path? Shouldn't happen, but handle it
                    let lhs_value = self.evaluate_internal(lhs, data)?;
                    return self.evaluate_internal(rhs, &lhs_value)
                        .map(|v| self.unwrap_singleton(v));
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
            if let AstNode::Lambda { params, body, signature, thunk } = rhs {
                // Store the lambda AST for later invocation
                // Capture current environment for closure support
                let captured_env = self.capture_current_environment();
                let stored_lambda = StoredLambda {
                    params: params.clone(),
                    body: (**body).clone(),
                    signature: signature.clone(),
                    captured_env,
                    captured_data: Some(data.clone()),
                    thunk: *thunk,
                };
                self.context.bind_lambda(var_name.clone(), stored_lambda);

                // Return a lambda marker value (include _lambda_id so it can be found later)
                let lambda_repr = serde_json::json!({
                    "__lambda__": true,
                    "params": params,
                    "_name": var_name,
                    "_lambda_id": var_name,
                });
                return Ok(lambda_repr);
            }

            // Check if RHS is a pure function composition (ChainPipe between function references)
            // e.g., $uppertrim := $trim ~> $uppercase
            // This creates a lambda that composes the functions.
            // But NOT for data ~> function, which should be evaluated immediately.
            // e.g., $result := data ~> $map($fn) should evaluate the pipe
            if let AstNode::Binary { op: BinaryOp::ChainPipe, lhs: chain_lhs, rhs: chain_rhs } = rhs {
                // Only wrap in lambda if LHS is a function reference (Variable pointing to a function)
                // If LHS is data (array, object, function call result, etc.), evaluate the pipe
                let is_function_composition = match chain_lhs.as_ref() {
                    // LHS is a function reference like $trim or $sum
                    AstNode::Variable(name) if self.is_builtin_function(name) || self.context.lookup_lambda(name).is_some() => true,
                    // LHS is another ChainPipe (nested composition like $f ~> $g ~> $h)
                    AstNode::Binary { op: BinaryOp::ChainPipe, .. } => true,
                    // A function call with placeholder creates a partial application
                    // e.g., $substringAfter(?, "@") ~> $substringBefore(?, ".")
                    AstNode::Function { args, .. } if args.iter().any(|a| matches!(a, AstNode::Placeholder)) => true,
                    // Anything else (data, function calls, arrays, etc.) is not pure composition
                    _ => false,
                };

                if is_function_composition {
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
                        captured_data: Some(data.clone()),
                        thunk: false,
                    };
                    self.context.bind_lambda(var_name.clone(), stored_lambda);

                    // Return a lambda marker value (include _lambda_id for later lookup)
                    let lambda_repr = serde_json::json!({
                        "__lambda__": true,
                        "params": ["$"],
                        "_name": var_name,
                        "_lambda_id": var_name,
                    });
                    return Ok(lambda_repr);
                }
                // If not function composition, fall through to normal evaluation below
            }

            // Evaluate the RHS
            let value = self.evaluate_internal(rhs, data)?;

            // If the value is a lambda, copy the stored lambda to the new variable name
            if let Some(stored) = self.lookup_lambda_from_value(&value) {
                self.context.bind_lambda(var_name.clone(), stored);
            }

            // Bind even if undefined (null) so inner scopes can shadow outer variables
            self.context.bind(var_name, value.clone());
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
            BinaryOp::Add => self.add(&left, &right, left_is_explicit_null, right_is_explicit_null),
            BinaryOp::Subtract => self.subtract(&left, &right, left_is_explicit_null, right_is_explicit_null),
            BinaryOp::Multiply => self.multiply(&left, &right, left_is_explicit_null, right_is_explicit_null),
            BinaryOp::Divide => self.divide(&left, &right, left_is_explicit_null, right_is_explicit_null),
            BinaryOp::Modulo => self.modulo(&left, &right, left_is_explicit_null, right_is_explicit_null),

            BinaryOp::Equal => Ok(Value::Bool(self.equals(&left, &right))),
            BinaryOp::NotEqual => Ok(Value::Bool(!self.equals(&left, &right))),
            BinaryOp::LessThan => self.less_than(&left, &right, left_is_explicit_null, right_is_explicit_null),
            BinaryOp::LessThanOrEqual => self.less_than_or_equal(&left, &right, left_is_explicit_null, right_is_explicit_null),
            BinaryOp::GreaterThan => self.greater_than(&left, &right, left_is_explicit_null, right_is_explicit_null),
            BinaryOp::GreaterThanOrEqual => self.greater_than_or_equal(&left, &right, left_is_explicit_null, right_is_explicit_null),

            // And/Or handled above with short-circuit evaluation
            BinaryOp::And | BinaryOp::Or => unreachable!(),

            BinaryOp::Concatenate => self.concatenate(&left, &right),
            BinaryOp::Range => self.range(&left, &right),
            BinaryOp::In => self.in_operator(&left, &right),

            // These operators are all handled as special cases earlier in evaluate_binary_op
            BinaryOp::ColonEqual | BinaryOp::Coalesce | BinaryOp::Default | BinaryOp::ChainPipe => {
                unreachable!()
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
                _ => Err(EvaluatorError::TypeError(
                    "D1002: Cannot negate non-number value".to_string()
                )),
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

        // FIRST check if this variable holds a function value (lambda or builtin reference)
        // This is critical for:
        // 1. Allowing function parameters to shadow stored lambdas
        //    (e.g., Y-combinator pattern: function($g){$g($g)} where parameter $g shadows outer $g)
        // 2. Calling built-in functions passed as parameters
        //    (e.g., λ($f){$f(5)}($sum) where $f is bound to $sum reference)
        if let Some(value) = self.context.lookup(name).cloned() {
            if let Some(stored_lambda) = self.lookup_lambda_from_value(&value) {
                let mut evaluated_args = Vec::with_capacity(args.len());
                for arg in args {
                    evaluated_args.push(self.evaluate_internal(arg, data)?);
                }
                return self.invoke_stored_lambda(&stored_lambda, &evaluated_args, data);
            }
            if let Value::Object(ref map) = value {
                if map.contains_key("__builtin__") {
                    // This is a built-in function reference (e.g., $f bound to $sum)
                    if let Some(Value::String(builtin_name)) = map.get("_name") {
                        let mut evaluated_args = Vec::with_capacity(args.len());
                        for arg in args {
                            evaluated_args.push(self.evaluate_internal(arg, data)?);
                        }
                        return self.call_builtin_with_values(builtin_name, &evaluated_args);
                    }
                }
            }
        }

        // THEN check if this is a stored lambda (user-defined function by name)
        // This only applies if not shadowed by a binding above
        if let Some(stored_lambda) = self.context.lookup_lambda(name).cloned() {
            let mut evaluated_args = Vec::with_capacity(args.len());
            for arg in args {
                evaluated_args.push(self.evaluate_internal(arg, data)?);
            }
            return self.invoke_stored_lambda(&stored_lambda, &evaluated_args, data);
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
                if self.is_builtin_function(var_name) {
                    return Ok(Value::Bool(true)); // Built-in function exists
                }

                // Check if it's a stored lambda
                if self.context.lookup_lambda(var_name).is_some() {
                    return Ok(Value::Bool(true)); // Lambda exists
                }

                // Check if the variable is defined
                if let Some(val) = self.context.lookup(var_name) {
                    // A variable bound to the undefined marker doesn't "exist"
                    if is_undefined(&val) {
                        return Ok(Value::Bool(false));
                    }
                    return Ok(Value::Bool(true)); // Variable is defined (even if null)
                } else {
                    return Ok(Value::Bool(false)); // Variable is undefined
                }
            }

            // For other expressions, evaluate and check if non-null/non-undefined
            let value = self.evaluate_internal(arg, data)?;
            return Ok(Value::Bool(!matches!(value, Value::Null) && !is_undefined(&value)));
        }

        // Check if any arguments are undefined variables or undefined paths
        // Functions like $not() should return undefined when given undefined values
        for arg in args {
            // Check for undefined variable (e.g., $undefined_var)
            if let AstNode::Variable(var_name) = arg {
                // Skip built-in function names - they're function references, not undefined variables
                if !var_name.is_empty() && !self.is_builtin_function(var_name) && self.context.lookup(var_name).is_none() {
                    // Undefined variable - for functions that should propagate undefined
                    if propagates_undefined(name) {
                        return Ok(Value::Null); // Return undefined
                    }
                }
            }
            // Check for simple field name (e.g., blah) that evaluates to undefined
            if let AstNode::Name(field_name) = arg {
                let field_exists = matches!(data, Value::Object(obj) if obj.contains_key(field_name));
                if !field_exists && propagates_undefined(name) {
                    return Ok(Value::Null);
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
                        // Get field name - could be Name (identifier) or String (quoted)
                        let field_name = match &steps[0].node {
                            AstNode::Name(n) => Some(n.as_str()),
                            AstNode::String(s) => Some(s.as_str()),
                            _ => None,
                        };
                        if let Some(field) = field_name {
                            match data {
                                Value::Object(obj) => {
                                    if !obj.contains_key(field) {
                                        // Field doesn't exist - return undefined
                                        if propagates_undefined(name) {
                                            return Ok(Value::Null);
                                        }
                                    }
                                    // Field exists with value null - continue to throw T0410
                                }
                                Value::Null => {
                                    // Trying to access field on null data - return undefined
                                    if propagates_undefined(name) {
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
                            if propagates_undefined(name) {
                                return Ok(Value::Null);
                            }
                        }
                    }
                }
            }
        }

        let mut evaluated_args = Vec::with_capacity(args.len());
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

        match name {
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

                Ok(functions::string::string(&evaluated_args[0], prettify)?)
            }
            "length" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "length() requires exactly 1 argument".to_string(),
                    ));
                }
                match &evaluated_args[0] {
                    Value::String(s) => Ok(functions::string::length(s)?),
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
                    Value::String(s) => Ok(functions::string::uppercase(s)?),
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
                    Value::String(s) => Ok(functions::string::lowercase(s)?),
                    _ => Err(EvaluatorError::TypeError(
                        "T0410: Argument 1 of function lowercase does not match function signature".to_string(),
                    )),
                }
            }
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
                Ok(functions::numeric::number(&evaluated_args[0])?)
            }
            "sum" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "sum() requires exactly 1 argument".to_string(),
                    ));
                }
                // Return undefined if argument is undefined
                if is_undefined(&evaluated_args[0]) {
                    return Ok(undefined_value());
                }
                match &evaluated_args[0] {
                    Value::Null => Ok(Value::Null),
                    Value::Array(arr) => {
                        // Flatten nested arrays for sum
                        let flattened = flatten_for_aggregation(arr);
                        Ok(functions::numeric::sum(&flattened)?)
                    }
                    // Non-array values are treated as single-element arrays
                    other => Ok(functions::numeric::sum(&[other.clone()])?),
                }
            }
            "count" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "count() requires exactly 1 argument".to_string(),
                    ));
                }
                // Return 0 if argument is undefined
                if is_undefined(&evaluated_args[0]) {
                    return Ok(serde_json::json!(0));
                }
                match &evaluated_args[0] {
                    Value::Null => Ok(serde_json::json!(0)), // null counts as 0
                    Value::Array(arr) => Ok(functions::array::count(arr)?),
                    _ => Ok(serde_json::json!(1)), // Non-array value counts as 1
                }
            }
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
                        Ok(functions::string::substring(s, start.as_f64().unwrap() as i64, length)?)
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
                    (Value::String(s), Value::String(sep)) => Ok(functions::string::substring_before(s, sep)?),
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
                    (Value::String(s), Value::String(sep)) => Ok(functions::string::substring_after(s, sep)?),
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

                let pad_chars: Vec<char> = pad_string.chars().collect();
                let mut padding = String::with_capacity(padding_needed);
                for i in 0..padding_needed {
                    padding.push(pad_chars[i % pad_chars.len()]);
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
                    Value::String(s) => Ok(functions::string::trim(s)?),
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
                    Value::String(s) => Ok(functions::string::contains(s, &evaluated_args[1])?),
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
                        Ok(functions::string::split(s, &evaluated_args[1], limit)?)
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
                        Ok(functions::string::join(arr, separator)?)
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

                // Check if replacement (3rd arg) is a function/lambda
                let replacement_is_lambda = if let Value::Object(ref map) = evaluated_args[2] {
                    map.contains_key("__lambda__") || map.contains_key("__builtin__")
                } else {
                    false
                };

                if replacement_is_lambda {
                    // Lambda replacement mode
                    return self.replace_with_lambda(
                        &evaluated_args[0],
                        &evaluated_args[1],
                        &evaluated_args[2],
                        if evaluated_args.len() == 4 { Some(&evaluated_args[3]) } else { None },
                        data
                    );
                }

                // String replacement mode
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
                        Ok(functions::string::replace(s, &evaluated_args[1], replacement, limit)?)
                    }
                    _ => Err(EvaluatorError::TypeError(
                        "replace() requires string arguments".to_string(),
                    )),
                }
            }
            "match" => {
                // $match(str, pattern [, limit])
                // Returns array of match objects for regex matches or custom matcher function
                if evaluated_args.is_empty() || evaluated_args.len() > 3 {
                    return Err(EvaluatorError::EvaluationError(
                        "match() requires 1 to 3 arguments".to_string(),
                    ));
                }
                if matches!(evaluated_args[0], Value::Null) {
                    return Ok(Value::Null);
                }

                let s = match &evaluated_args[0] {
                    Value::String(s) => s.clone(),
                    _ => return Err(EvaluatorError::TypeError(
                        "match() first argument must be a string".to_string(),
                    )),
                };

                // Get optional limit
                let limit = if evaluated_args.len() == 3 {
                    match &evaluated_args[2] {
                        Value::Number(n) => Some(n.as_f64().unwrap() as usize),
                        Value::Null => None,
                        _ => return Err(EvaluatorError::TypeError(
                            "match() limit must be a number".to_string(),
                        )),
                    }
                } else {
                    None
                };

                // Check if second argument is a custom matcher function (lambda)
                let pattern_value = evaluated_args.get(1);
                let is_custom_matcher = pattern_value.map_or(false, |val| {
                    if let Value::Object(map) = val {
                        map.contains_key("__lambda__") || map.contains_key("__builtin__")
                    } else {
                        false
                    }
                });

                if is_custom_matcher {
                    // Custom matcher function support
                    // Call the matcher with the string, get match objects with {match, start, end, groups, next}
                    return self.match_with_custom_matcher(&s, &args[1], limit, data);
                }

                // Get regex pattern from second argument
                let (pattern, flags) = match pattern_value {
                    Some(val) => crate::functions::string::extract_regex(val)
                        .ok_or_else(|| EvaluatorError::TypeError(
                            "match() second argument must be a regex pattern or matcher function".to_string()
                        ))?,
                    None => {
                        (".*".to_string(), "".to_string())
                    }
                };

                // Build regex
                let is_global = flags.contains('g');
                let regex_pattern = if flags.contains('i') {
                    format!("(?i){}", pattern)
                } else {
                    pattern.clone()
                };

                let re = regex::Regex::new(&regex_pattern)
                    .map_err(|e| EvaluatorError::EvaluationError(
                        format!("Invalid regex pattern: {}", e)
                    ))?;

                let mut results = Vec::new();
                let mut count = 0;

                for caps in re.captures_iter(&s) {
                    if let Some(lim) = limit {
                        if count >= lim {
                            break;
                        }
                    }

                    let full_match = caps.get(0).unwrap();
                    let mut match_obj = serde_json::Map::new();
                    match_obj.insert("match".to_string(), Value::String(full_match.as_str().to_string()));
                    match_obj.insert("index".to_string(), Value::Number(serde_json::Number::from(full_match.start())));

                    // Collect capture groups
                    let mut groups: Vec<Value> = Vec::new();
                    for i in 1..caps.len() {
                        if let Some(group) = caps.get(i) {
                            groups.push(Value::String(group.as_str().to_string()));
                        } else {
                            groups.push(Value::Null);
                        }
                    }
                    if !groups.is_empty() {
                        match_obj.insert("groups".to_string(), Value::Array(groups));
                    }

                    results.push(Value::Object(match_obj));
                    count += 1;

                    // If not global, only return first match
                    if !is_global {
                        break;
                    }
                }

                if results.is_empty() {
                    Ok(Value::Null)
                } else if results.len() == 1 && !is_global {
                    // Single match (non-global) returns the match object directly
                    Ok(results.into_iter().next().unwrap())
                } else {
                    Ok(Value::Array(results))
                }
            }
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
                    Value::Array(arr) => {
                        let flattened = flatten_for_aggregation(arr);
                        Ok(functions::numeric::max(&flattened)?)
                    }
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
                    Value::Array(arr) => {
                        let flattened = flatten_for_aggregation(arr);
                        Ok(functions::numeric::min(&flattened)?)
                    }
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
                // Return undefined if argument is undefined
                if is_undefined(&evaluated_args[0]) {
                    return Ok(undefined_value());
                }
                match &evaluated_args[0] {
                    Value::Null => Ok(Value::Null),
                    Value::Array(arr) => {
                        let flattened = flatten_for_aggregation(arr);
                        Ok(functions::numeric::average(&flattened)?)
                    }
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
                    Value::Number(n) => Ok(functions::numeric::abs(n.as_f64().unwrap())?),
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
                    Value::Number(n) => Ok(functions::numeric::floor(n.as_f64().unwrap())?),
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
                    Value::Number(n) => Ok(functions::numeric::ceil(n.as_f64().unwrap())?),
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
                        Ok(functions::numeric::round(n.as_f64().unwrap(), precision)?)
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
                    Value::Number(n) => Ok(functions::numeric::sqrt(n.as_f64().unwrap())?),
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
                        Ok(functions::numeric::power(base.as_f64().unwrap(), exp.as_f64().unwrap())?)
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
                        Ok(functions::numeric::format_number(num.as_f64().unwrap(), picture, options)?)
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
                        Ok(functions::numeric::format_base(num.as_f64().unwrap(), radix)?)
                    }
                    _ => Err(EvaluatorError::TypeError(
                        "formatBase() requires a number".to_string(),
                    )),
                }
            }
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

                Ok(functions::array::append(&arr, second)?)
            }
            "reverse" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "reverse() requires exactly 1 argument".to_string(),
                    ));
                }
                match &evaluated_args[0] {
                    Value::Null => Ok(Value::Null), // undefined returns undefined
                    Value::Array(arr) => Ok(functions::array::reverse(arr)?),
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
                    Value::Array(arr) => Ok(functions::array::shuffle(arr)?),
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
                    // Return undefined for empty results (will be filtered by function application)
                    if result.is_empty() {
                        Ok(undefined_value())
                    } else {
                        Ok(Value::Object(result))
                    }
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
                                    // sift_object returns undefined for empty results
                                    if !is_undefined(&sifted) {
                                        results.push(sifted);
                                    }
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
                let mut arrays: Vec<Vec<Value>> = Vec::with_capacity(evaluated_args.len());
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

                let array_value = self.evaluate_internal(&args[0], data)?;

                // Handle undefined input
                if matches!(array_value, Value::Null) {
                    return Ok(Value::Null);
                }

                let mut arr = match array_value {
                    Value::Array(arr) => arr,
                    other => vec![other],
                };

                if args.len() == 2 {
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
                    Ok(functions::array::sort(&arr)?)
                }
            }
            "distinct" => {
                if evaluated_args.len() != 1 {
                    return Err(EvaluatorError::EvaluationError(
                        "distinct() requires exactly 1 argument".to_string(),
                    ));
                }
                match &evaluated_args[0] {
                    Value::Array(arr) => Ok(functions::array::distinct(arr)?),
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
                Ok(functions::array::exists(&evaluated_args[0])?)
            }
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
                        if obj.get("__lambda__").is_some() {
                            return Ok(undefined_value());
                        }
                        Ok(functions::object::spread(obj)?)
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
                                    let spread_result = functions::object::spread(obj)?;
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
                        Value::Array(arr) => Ok(functions::object::merge(arr)?),
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
                    Ok(functions::object::merge(&evaluated_args)?)
                }
            }

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

                // Handle undefined input - return undefined
                if is_undefined(&array) {
                    return Ok(undefined_value());
                }

                // Handle null input
                if matches!(array, Value::Null) {
                    return Ok(undefined_value());
                }

                // Coerce non-array values to single-element arrays
                // Track if input was a single value to unwrap result appropriately
                let (arr, was_single_value) = match array {
                    Value::Array(arr) => (arr, false),
                    single_value => (vec![single_value], true),
                };

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

                // If input was a single value, return the single matching item
                // (or undefined if no match)
                if was_single_value {
                    if result.len() == 1 {
                        return Ok(result.remove(0));
                    } else if result.is_empty() {
                        return Ok(undefined_value());
                    }
                }

                Ok(Value::Array(result))
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
                Ok(functions::boolean::boolean(&evaluated_args[0])?)
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
                    Value::String(s) => Ok(functions::encoding::base64encode(s)?),
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
                    Value::String(s) => Ok(functions::encoding::base64decode(s)?),
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
                    Value::String(s) => Ok(functions::encoding::encode_url_component(s)?),
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
                    Value::String(s) => Ok(functions::encoding::decode_url_component(s)?),
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
                    Value::String(s) => Ok(functions::encoding::encode_url(s)?),
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
                    Value::String(s) => Ok(functions::encoding::decode_url(s)?),
                    _ => Err(EvaluatorError::TypeError(
                        "decodeUrl() requires a string argument".to_string(),
                    )),
                }
            }

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
                        if evaluated_args.len() == 2 {
                            match &evaluated_args[1] {
                                Value::String(picture) => {
                                    // Use custom picture format parsing
                                    Ok(crate::datetime::to_millis_with_picture(s, picture)?)
                                }
                                Value::Null => Ok(Value::Null),
                                _ => Err(EvaluatorError::TypeError(
                                    "toMillis() second argument must be a string".to_string(),
                                )),
                            }
                        } else {
                            // Use ISO 8601 partial date parsing
                            Ok(crate::datetime::to_millis(s)?)
                        }
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
                        Ok(crate::datetime::from_millis(millis)?)
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

    /// Apply a function (lambda or expression) to values
    ///
    /// This handles both:
    /// 1. Lambda nodes: function($x) { $x * 2 } - binds parameters and evaluates body
    /// 2. Simple expressions: price * 2 - evaluates with values as context
    fn apply_function(&mut self, func_node: &AstNode, values: &[Value], data: &Value) -> Result<Value, EvaluatorError> {
        match func_node {
            AstNode::Lambda { params, body, signature, thunk } => {
                // Direct lambda - invoke it
                self.invoke_lambda(params, body, signature.as_ref(), values, data, *thunk)
            }
            AstNode::Function { name, args, is_builtin } => {
                // Function call - check if it has placeholders (partial application)
                let has_placeholder = args.iter().any(|arg| matches!(arg, AstNode::Placeholder));

                if has_placeholder {
                    // This is a partial application - evaluate it to get the lambda value
                    let partial_lambda = self.create_partial_application(name, args, *is_builtin, data)?;

                    // Now invoke the partial lambda with the provided values
                    if let Some(stored) = self.lookup_lambda_from_value(&partial_lambda) {
                        return self.invoke_stored_lambda(&stored, values, data);
                    }
                    return Err(EvaluatorError::EvaluationError(
                        "Failed to apply partial application".to_string()
                    ));
                } else {
                    // Regular function call without placeholders
                    // Evaluate it and apply if it returns a function
                    let result = self.evaluate_internal(func_node, data)?;

                    // Check if result is a lambda value
                    if let Some(stored) = self.lookup_lambda_from_value(&result) {
                        return self.invoke_stored_lambda(&stored, values, data);
                    }

                    // Otherwise just return the result
                    Ok(result)
                }
            }
            AstNode::Variable(var_name) => {
                // Check if this variable holds a stored lambda
                if let Some(stored_lambda) = self.context.lookup_lambda(var_name).cloned() {
                    self.invoke_stored_lambda(&stored_lambda, values, data)
                } else if let Some(value) = self.context.lookup(var_name).cloned() {
                    // Check if this variable holds a lambda value (JSON object with __lambda__)
                    // This handles lambdas passed as bound arguments in partial applications
                    if let Some(stored) = self.lookup_lambda_from_value(&value) {
                        return self.invoke_stored_lambda(&stored, values, data);
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
        _original_data: &Value,
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
    fn invoke_lambda(&mut self, params: &[String], body: &AstNode, signature: Option<&String>, values: &[Value], data: &Value, thunk: bool) -> Result<Value, EvaluatorError> {
        self.invoke_lambda_with_env(params, body, signature, values, data, None, None, thunk)
    }

    /// Invoke a lambda with optional captured environment (for closures)
    fn invoke_lambda_with_env(
        &mut self,
        params: &[String],
        body: &AstNode,
        signature: Option<&String>,
        values: &[Value],
        data: &Value,
        captured_env: Option<&HashMap<String, Value>>,
        captured_data: Option<&Value>,
        thunk: bool,
    ) -> Result<Value, EvaluatorError> {
        // If this is a thunk (has tail calls), use TCO trampoline
        if thunk {
            let stored = StoredLambda {
                params: params.to_vec(),
                body: body.clone(),
                signature: signature.cloned(),
                captured_env: captured_env.cloned().unwrap_or_default(),
                captured_data: captured_data.cloned(),
                thunk,
            };
            return self.invoke_lambda_with_tco(&stored, values, data);
        }

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

        let saved_bindings: HashMap<String, Option<Value>> = vars_to_restore
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
        // If there are more params than values, extra params get undefined
        for (i, param) in params.iter().enumerate() {
            let value = coerced_values.get(i).cloned().unwrap_or_else(undefined_value);
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
        // Use captured_data for lexical scoping if available, otherwise use call-site data
        let body_data = captured_data.unwrap_or(data);
        let result = self.evaluate_internal(body, body_data)?;

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

    /// Invoke a lambda with tail call optimization using a trampoline
    /// This method uses an iterative loop to handle tail-recursive calls without
    /// growing the stack, enabling deep recursion for tail-recursive functions.
    fn invoke_lambda_with_tco(
        &mut self,
        stored_lambda: &StoredLambda,
        initial_args: &[Value],
        data: &Value,
    ) -> Result<Value, EvaluatorError> {
        let mut current_lambda = stored_lambda.clone();
        let mut current_args = initial_args.to_vec();
        let mut current_data = data.clone();

        // Maximum number of tail call iterations to prevent infinite loops
        // This is much higher than non-TCO depth limit since TCO doesn't grow the stack
        const MAX_TCO_ITERATIONS: usize = 100_000;
        let mut iterations = 0;

        // Trampoline loop - keeps evaluating until we get a final value
        loop {
            iterations += 1;
            if iterations > MAX_TCO_ITERATIONS {
                return Err(EvaluatorError::EvaluationError(
                    "U1001: Stack overflow - maximum recursion depth (500) exceeded".to_string()
                ));
            }

            // Evaluate the lambda body
            let result = self.invoke_lambda_body_for_tco(
                &current_lambda,
                &current_args,
                &current_data,
            )?;

            match result {
                LambdaResult::Value(v) => return Ok(v),
                LambdaResult::TailCall { lambda, args, data } => {
                    // Continue with the tail call - no stack growth
                    current_lambda = lambda;
                    current_args = args;
                    current_data = data;
                }
            }
        }
    }

    /// Evaluate a lambda body, detecting tail calls for TCO
    /// Returns either a final value or a tail call continuation
    fn invoke_lambda_body_for_tco(
        &mut self,
        lambda: &StoredLambda,
        values: &[Value],
        data: &Value,
    ) -> Result<LambdaResult, EvaluatorError> {
        // Validate signature if present
        let coerced_values = if let Some(sig_str) = &lambda.signature {
            match crate::signature::Signature::parse(sig_str) {
                Ok(sig) => {
                    match sig.validate_and_coerce(values) {
                        Ok(coerced) => coerced,
                        Err(e) => {
                            match e {
                                crate::signature::SignatureError::UndefinedArgument => {
                                    return Ok(LambdaResult::Value(Value::Null));
                                }
                                crate::signature::SignatureError::ArgumentTypeMismatch { index, expected } => {
                                    return Err(EvaluatorError::TypeError(
                                        format!("T0410: Argument {} of function does not match function signature (expected {})", index, expected)
                                    ));
                                }
                                crate::signature::SignatureError::ArrayTypeMismatch { index, expected } => {
                                    return Err(EvaluatorError::TypeError(
                                        format!("T0412: Argument {} of function must be an array of {}", index, expected)
                                    ));
                                }
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
            values.to_vec()
        };

        // Save and bind parameters
        let mut vars_to_restore: Vec<String> = lambda.params.clone();
        if !lambda.captured_env.is_empty() {
            vars_to_restore.extend(lambda.captured_env.keys().cloned());
        }

        let saved_bindings: HashMap<String, Option<Value>> = vars_to_restore
            .iter()
            .map(|name| (name.clone(), self.context.lookup(name).cloned()))
            .collect();

        // Apply captured environment
        for (name, value) in &lambda.captured_env {
            self.context.bind(name.clone(), value.clone());
        }

        // Bind parameters
        for (i, param) in lambda.params.iter().enumerate() {
            let value = coerced_values.get(i).cloned().unwrap_or(Value::Null);
            self.context.bind(param.clone(), value);
        }

        // Evaluate the body with tail call detection
        let body_data = lambda.captured_data.as_ref().unwrap_or(data);
        let result = self.evaluate_for_tco(&lambda.body, body_data)?;

        // Restore bindings
        for (name, saved_value) in saved_bindings {
            if let Some(value) = saved_value {
                self.context.bind(name, value);
            } else {
                self.context.unbind(&name);
            }
        }

        Ok(result)
    }

    /// Evaluate an expression for TCO, detecting tail calls
    /// Returns LambdaResult::TailCall if the expression is a function call to a user lambda
    fn evaluate_for_tco(&mut self, node: &AstNode, data: &Value) -> Result<LambdaResult, EvaluatorError> {
        match node {
            // Conditional: evaluate condition, then evaluate the chosen branch for TCO
            AstNode::Conditional { condition, then_branch, else_branch } => {
                let cond_value = self.evaluate_internal(condition, data)?;
                let is_truthy = self.is_truthy(&cond_value);

                if is_truthy {
                    self.evaluate_for_tco(then_branch, data)
                } else if let Some(else_expr) = else_branch {
                    self.evaluate_for_tco(else_expr, data)
                } else {
                    Ok(LambdaResult::Value(Value::Null))
                }
            }

            // Block: evaluate all but last normally, last for TCO
            AstNode::Block(exprs) => {
                if exprs.is_empty() {
                    return Ok(LambdaResult::Value(Value::Null));
                }

                // Evaluate all expressions except the last
                let mut result = Value::Null;
                for (i, expr) in exprs.iter().enumerate() {
                    if i == exprs.len() - 1 {
                        // Last expression - evaluate for TCO
                        return self.evaluate_for_tco(expr, data);
                    } else {
                        result = self.evaluate_internal(expr, data)?;
                    }
                }
                Ok(LambdaResult::Value(result))
            }

            // Variable binding: evaluate value, bind, then evaluate result for TCO if present
            AstNode::Binary { op: BinaryOp::ColonEqual, lhs, rhs } => {
                // This is var := value; get the variable name
                let var_name = match lhs.as_ref() {
                    AstNode::Variable(name) => name.clone(),
                    _ => {
                        // Not a simple variable binding, evaluate normally
                        let result = self.evaluate_internal(node, data)?;
                        return Ok(LambdaResult::Value(result));
                    }
                };

                // Check if RHS is a lambda - store it specially
                if let AstNode::Lambda { params, body, signature, thunk } = rhs.as_ref() {
                    let captured_env = self.capture_current_environment();
                    let stored_lambda = StoredLambda {
                        params: params.clone(),
                        body: (**body).clone(),
                        signature: signature.clone(),
                        captured_env,
                        captured_data: Some(data.clone()),
                        thunk: *thunk,
                    };
                    self.context.bind_lambda(var_name, stored_lambda);
                    let lambda_repr = serde_json::json!({
                        "__lambda__": true,
                        "params": params,
                    });
                    return Ok(LambdaResult::Value(lambda_repr));
                }

                // Evaluate the RHS
                let value = self.evaluate_internal(rhs, data)?;
                self.context.bind(var_name, value.clone());
                Ok(LambdaResult::Value(value))
            }

            // Function call - this is where TCO happens
            AstNode::Function { name, args, .. } => {
                // Check if this is a call to a stored lambda (user function)
                if let Some(stored_lambda) = self.context.lookup_lambda(name).cloned() {
                    if stored_lambda.thunk {
                        let mut evaluated_args = Vec::with_capacity(args.len());
                        for arg in args {
                            evaluated_args.push(self.evaluate_internal(arg, data)?);
                        }
                        return Ok(LambdaResult::TailCall {
                            lambda: stored_lambda,
                            args: evaluated_args,
                            data: data.clone(),
                        });
                    }
                }
                // Not a thunk lambda - evaluate normally
                let result = self.evaluate_internal(node, data)?;
                Ok(LambdaResult::Value(result))
            }

            // Call node (calling a lambda value)
            AstNode::Call { procedure, args } => {
                // Evaluate the procedure to get the callable
                let callable = self.evaluate_internal(procedure, data)?;

                // Check if it's a lambda with TCO
                if let Value::Object(ref map) = callable {
                    if map.contains_key("__lambda__") {
                        if let Some(Value::String(lambda_id)) = map.get("_lambda_id") {
                            if let Some(stored_lambda) = self.context.lookup_lambda(lambda_id).cloned() {
                                if stored_lambda.thunk {
                                    let mut evaluated_args = Vec::with_capacity(args.len());
                                    for arg in args {
                                        evaluated_args.push(self.evaluate_internal(arg, data)?);
                                    }
                                    return Ok(LambdaResult::TailCall {
                                        lambda: stored_lambda,
                                        args: evaluated_args,
                                        data: data.clone(),
                                    });
                                }
                            }
                        }
                    }
                }
                // Not a thunk - evaluate normally
                let result = self.evaluate_internal(node, data)?;
                Ok(LambdaResult::Value(result))
            }

            // Variable reference that might be a function call
            // This handles cases like $f($x) where $f is referenced by name
            AstNode::Variable(_) => {
                let result = self.evaluate_internal(node, data)?;
                Ok(LambdaResult::Value(result))
            }

            // Any other expression - evaluate normally
            _ => {
                let result = self.evaluate_internal(node, data)?;
                Ok(LambdaResult::Value(result))
            }
        }
    }

    /// Match with custom matcher function
    ///
    /// Implements custom matcher support for $match(str, matcherFunction, limit?)
    /// The matcher function is called with the string and returns:
    /// { match: string, start: number, end: number, groups: [], next: function }
    /// The next function is called repeatedly to get subsequent matches
    fn match_with_custom_matcher(
        &mut self,
        str_value: &str,
        matcher_node: &AstNode,
        limit: Option<usize>,
        data: &Value,
    ) -> Result<Value, EvaluatorError> {
        let mut results = Vec::new();
        let mut count = 0;

        // Call the matcher function with the string
        let str_val = Value::String(str_value.to_string());
        let mut current_match = self.apply_function(matcher_node, &[str_val], data)?;

        // Iterate through matches following the 'next' chain
        while !is_undefined(&current_match) && !matches!(current_match, Value::Null) {
            // Check limit
            if let Some(lim) = limit {
                if count >= lim {
                    break;
                }
            }

            // Extract match information from the result object
            if let Value::Object(ref match_obj) = current_match {
                // Validate that this is a proper match object
                let has_match = match_obj.contains_key("match");
                let has_start = match_obj.contains_key("start");
                let has_end = match_obj.contains_key("end");
                let has_groups = match_obj.contains_key("groups");
                let has_next = match_obj.contains_key("next");

                if !has_match && !has_start && !has_end && !has_groups && !has_next {
                    // Invalid matcher result - T1010 error
                    return Err(EvaluatorError::EvaluationError(
                        "T1010: The matcher function did not return the correct object structure".to_string()
                    ));
                }

                // Build the result match object (match, index, groups)
                let mut result_obj = serde_json::Map::new();

                if let Some(match_val) = match_obj.get("match") {
                    result_obj.insert("match".to_string(), match_val.clone());
                }

                if let Some(start_val) = match_obj.get("start") {
                    result_obj.insert("index".to_string(), start_val.clone());
                }

                if let Some(groups_val) = match_obj.get("groups") {
                    result_obj.insert("groups".to_string(), groups_val.clone());
                }

                results.push(Value::Object(result_obj));
                count += 1;

                // Get the next match by calling the 'next' function
                if let Some(next_func) = match_obj.get("next") {
                    if let Some(stored) = self.lookup_lambda_from_value(next_func) {
                        current_match = self.invoke_stored_lambda(&stored, &[], data)?;
                        continue;
                    }
                }

                // No next function or couldn't call it - stop iteration
                break;
            } else {
                // Not a valid match object
                break;
            }
        }

        // Return results
        if results.is_empty() {
            Ok(undefined_value())
        } else {
            Ok(Value::Array(results))
        }
    }

    /// Replace with lambda/function callback
    ///
    /// Implements lambda replacement for $replace(str, pattern, function, limit?)
    /// The function receives a match object with: match, start, end, groups
    fn replace_with_lambda(
        &mut self,
        str_value: &Value,
        pattern_value: &Value,
        lambda_value: &Value,
        limit_value: Option<&Value>,
        data: &Value,
    ) -> Result<Value, EvaluatorError> {
        // Extract string
        let s = match str_value {
            Value::String(s) => s.as_str(),
            _ => return Err(EvaluatorError::TypeError(
                "replace() requires string arguments".to_string()
            )),
        };

        // Extract regex pattern
        let (pattern, flags) = crate::functions::string::extract_regex(pattern_value)
            .ok_or_else(|| EvaluatorError::TypeError(
                "replace() pattern must be a regex when using lambda replacement".to_string()
            ))?;

        // Build regex
        let re = crate::functions::string::build_regex(&pattern, &flags)?;

        // Parse limit
        let limit = if let Some(lim_val) = limit_value {
            match lim_val {
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

        // Iterate through matches and replace using lambda
        let mut result = String::new();
        let mut last_end = 0;
        let mut count = 0;

        for cap in re.captures_iter(s) {
            // Check limit
            if let Some(lim) = limit {
                if count >= lim {
                    break;
                }
            }

            let m = cap.get(0).unwrap();
            let match_start = m.start();
            let match_end = m.end();
            let match_str = m.as_str();

            // Add text before match
            result.push_str(&s[last_end..match_start]);

            // Build match object
            let groups: Vec<Value> = (1..cap.len())
                .map(|i| {
                    cap.get(i)
                        .map(|m| Value::String(m.as_str().to_string()))
                        .unwrap_or(Value::Null)
                })
                .collect();

            let match_obj = serde_json::json!({
                "match": match_str,
                "start": match_start,
                "end": match_end,
                "groups": groups,
            });

            // Invoke lambda with match object
            let stored_lambda = self.lookup_lambda_from_value(&lambda_value)
                .ok_or_else(|| EvaluatorError::TypeError(
                    "Replacement must be a lambda function".to_string()
                ))?;
            let lambda_result = self.invoke_stored_lambda(&stored_lambda, &[match_obj], data)?;
            let replacement_str = match lambda_result {
                Value::String(s) => s,
                _ => return Err(EvaluatorError::TypeError(
                    format!("D3012: Replacement function must return a string, got {:?}", lambda_result)
                )),
            };

            // Add replacement
            result.push_str(&replacement_str);

            last_end = match_end;
            count += 1;
        }

        // Add remaining text after last match
        result.push_str(&s[last_end..]);

        Ok(Value::String(result))
    }

    /// Capture the current environment bindings for closure support
    fn capture_current_environment(&self) -> HashMap<String, Value> {
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

        if values.is_empty() {
            return Err(EvaluatorError::EvaluationError(
                format!("{}() requires at least 1 argument", name)
            ));
        }

        let arg = &values[0];

        match name {
            "string" => Ok(functions::string::string(arg, None)?),
            "number" => Ok(functions::numeric::number(arg)?),
            "boolean" => Ok(functions::boolean::boolean(arg)?),
            "not" => {
                let b = functions::boolean::boolean(arg)?;
                match b {
                    Value::Bool(val) => Ok(Value::Bool(!val)),
                    _ => Err(EvaluatorError::TypeError("not() requires a boolean".to_string())),
                }
            }
            "exists" => Ok(Value::Bool(!matches!(arg, Value::Null))),
            "abs" => match arg {
                Value::Number(n) => Ok(functions::numeric::abs(n.as_f64().unwrap_or(0.0))?),
                _ => Err(EvaluatorError::TypeError("abs() requires a number argument".to_string())),
            },
            "floor" => match arg {
                Value::Number(n) => Ok(functions::numeric::floor(n.as_f64().unwrap_or(0.0))?),
                _ => Err(EvaluatorError::TypeError("floor() requires a number argument".to_string())),
            },
            "ceil" => match arg {
                Value::Number(n) => Ok(functions::numeric::ceil(n.as_f64().unwrap_or(0.0))?),
                _ => Err(EvaluatorError::TypeError("ceil() requires a number argument".to_string())),
            },
            "round" => match arg {
                Value::Number(n) => Ok(functions::numeric::round(n.as_f64().unwrap_or(0.0), None)?),
                _ => Err(EvaluatorError::TypeError("round() requires a number argument".to_string())),
            },
            "sqrt" => match arg {
                Value::Number(n) => Ok(functions::numeric::sqrt(n.as_f64().unwrap_or(0.0))?),
                _ => Err(EvaluatorError::TypeError("sqrt() requires a number argument".to_string())),
            },
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
                // DO NOT include the array itself - only recurse into elements
                // This matches JavaScript behavior: arrays are traversed but not collected
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
                    Ok(Value::Array(indices)) => {
                        // Multiple array selectors [[indices]]
                        // Check if array contains any non-numeric values
                        let has_non_numeric = indices.iter().any(|v| !matches!(v, Value::Number(_)));

                        if has_non_numeric {
                            // If array contains non-numeric values, return entire array
                            return Ok(current.clone());
                        }

                        // Collect numeric indices, handling negative indices
                        let arr_len = _arr.len() as i64;
                        let mut resolved_indices: Vec<i64> = indices
                            .iter()
                            .filter_map(|v| {
                                if let Value::Number(n) = v {
                                    let idx = n.as_f64().unwrap() as i64;
                                    // Resolve negative indices
                                    let actual_idx = if idx < 0 { arr_len + idx } else { idx };
                                    // Only include valid indices
                                    if actual_idx >= 0 && actual_idx < arr_len {
                                        Some(actual_idx)
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            })
                            .collect();

                        // Sort and deduplicate indices
                        resolved_indices.sort();
                        resolved_indices.dedup();

                        // Select elements at each sorted index
                        let result: Vec<Value> = resolved_indices
                            .iter()
                            .map(|&idx| _arr[idx as usize].clone())
                            .collect();

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
            Value::Object(obj) => {
                // For objects, predicate can be either:
                // 1. A string - property access (computed property name)
                // 2. A boolean expression - filter (return object if truthy)
                let pred_result = self.evaluate_internal(predicate, current)?;

                // If it's a string, use it as a key for property access
                if let Value::String(key) = &pred_result {
                    return Ok(obj.get(key).cloned().unwrap_or(Value::Null));
                }

                // Otherwise, treat as a filter expression
                // If the predicate is truthy, return the object; otherwise return undefined
                if self.is_truthy(&pred_result) {
                    Ok(current.clone())
                } else {
                    Ok(undefined_value())
                }
            }
            _ => {
                // For primitive values (string, number, boolean):
                // In JSONata, scalars are treated as single-element arrays when indexed.
                // So value[0] returns value, value[1] returns undefined.

                // First check if predicate is a numeric literal
                if let AstNode::Number(n) = predicate {
                    // For scalars, index 0 or -1 returns the value, others return undefined
                    let idx = n.floor() as i64;
                    if idx == 0 || idx == -1 {
                        return Ok(current.clone());
                    } else {
                        return Ok(undefined_value());
                    }
                }

                // Try to evaluate the predicate to see if it's a numeric index
                let pred_result = self.evaluate_internal(predicate, current)?;

                if let Value::Number(n) = &pred_result {
                    // It's a numeric index - treat scalar as single-element array
                    let idx = n.as_f64().unwrap().floor() as i64;
                    if idx == 0 || idx == -1 {
                        return Ok(current.clone());
                    } else {
                        return Ok(undefined_value());
                    }
                }

                // For non-numeric predicates, treat as a filter:
                // value[true] returns value, value[false] returns undefined
                // This enables patterns like: $k[$v>2] which returns $k if $v>2, otherwise undefined
                if self.is_truthy(&pred_result) {
                    Ok(current.clone())
                } else {
                    // Return undefined (not null) so $map can filter it out
                    Ok(undefined_value())
                }
            }
        }
    }

    /// Evaluate a sort term expression, distinguishing missing fields from explicit null
    /// Returns undefined_value() for missing fields, Value::Null for explicit null
    fn evaluate_sort_term(&mut self, term_expr: &AstNode, element: &Value) -> Result<Value, EvaluatorError> {
        // For tuples (from index binding), extract the actual value from @ field
        let actual_element = if let Value::Object(obj) = element {
            if obj.get("__tuple__") == Some(&Value::Bool(true)) {
                obj.get("@").cloned().unwrap_or(Value::Null)
            } else {
                element.clone()
            }
        } else {
            element.clone()
        };

        // For simple field access (Path with single Name step), check if field exists
        if let AstNode::Path { steps } = term_expr {
            if steps.len() == 1 && steps[0].stages.is_empty() {
                if let AstNode::Name(field_name) = &steps[0].node {
                    // Check if the field exists in the element
                    if let Value::Object(obj) = &actual_element {
                        return match obj.get(field_name) {
                            Some(val) => Ok(val.clone()),  // Field exists (may be null)
                            None => Ok(undefined_value()), // Field is missing
                        };
                    } else {
                        // Not an object - return undefined
                        return Ok(undefined_value());
                    }
                }
            }
        }

        // For complex expressions, evaluate normally against the actual element
        // but with the full tuple as the data context (so index bindings are accessible)
        let result = self.evaluate_internal(term_expr, element)?;

        // If the result is null from a complex expression, we can't easily tell if it's
        // "missing field" or "explicit null". For now, treat null results as undefined
        // to maintain compatibility with existing tests.
        // TODO: For full JS compatibility, would need deeper analysis of the expression
        if matches!(result, Value::Null) {
            return Ok(undefined_value());
        }

        Ok(result)
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

                // Evaluate the sort expression, distinguishing missing fields from explicit null
                let sort_value = self.evaluate_sort_term(term_expr, element)?;

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

        // Validate that all sort keys are comparable (same type, or undefined)
        // Undefined values (missing fields) are allowed and sort to the end
        // Null values (explicit null in data) are NOT allowed (typeof null === 'object' in JS, triggers T2008)
        for term_idx in 0..terms.len() {
            let mut first_valid_type: Option<&str> = None;

            for (_idx, sort_keys) in &indexed_array {
                let sort_value = &sort_keys[term_idx];

                // Skip undefined markers (missing fields) - these are allowed and sort to end
                if is_undefined(sort_value) {
                    continue;
                }

                // Get the type name for this value
                // Note: explicit null is NOT allowed - typeof null === 'object' in JS
                let value_type = match sort_value {
                    Value::Number(_) => "number",
                    Value::String(_) => "string",
                    Value::Bool(_) => "boolean",
                    Value::Array(_) => "array",
                    Value::Object(_) => "object",  // This catches non-undefined objects
                    Value::Null => "null",         // Explicit null from data
                };

                // Check that sort keys are only numbers or strings
                // Null, boolean, array, and object types are not valid for sorting
                if value_type != "number" && value_type != "string" {
                    return Err(EvaluatorError::TypeError(format!(
                        "T2008: The expressions within an order-by clause must evaluate to numeric or string values"
                    )));
                }

                // Check if this matches the first valid type we saw
                if let Some(first_type) = first_valid_type {
                    if first_type != value_type {
                        return Err(EvaluatorError::TypeError(format!(
                            "T2007: Type mismatch when comparing values in order-by clause: {} and {}",
                            first_type, value_type
                        )));
                    }
                } else {
                    first_valid_type = Some(value_type);
                }
            }
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

        // Handle undefined markers first - they sort to the end
        let left_undef = is_undefined(left);
        let right_undef = is_undefined(right);

        if left_undef && right_undef {
            return Ordering::Equal;
        }
        if left_undef {
            return Ordering::Greater;  // Undefined sorts last
        }
        if right_undef {
            return Ordering::Less;
        }

        match (left, right) {
            // Nulls also sort last (explicit null in data)
            (Value::Null, Value::Null) => Ordering::Equal,
            (Value::Null, _) => Ordering::Greater,
            (_, Value::Null) => Ordering::Less,

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
        if is_undefined(value) {
            return false;
        }
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

    /// Unwrap singleton arrays to scalar values
    /// This is used when no explicit array-keeping operation (like []) was used
    fn unwrap_singleton(&self, value: Value) -> Value {
        match value {
            Value::Array(ref arr) if arr.len() == 1 => arr[0].clone(),
            _ => value,
        }
    }

    /// Extract lambda IDs from a value (used for closure preservation)
    /// Finds any _lambda_id references in the value so they can be preserved
    /// when exiting a block scope
    fn extract_lambda_ids(&self, value: &Value) -> Vec<String> {
        let mut ids = Vec::new();
        self.collect_lambda_ids(value, &mut ids);
        ids
    }

    fn collect_lambda_ids(&self, value: &Value, ids: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                // Check if this is a lambda value
                if map.contains_key("__lambda__") {
                    if let Some(Value::String(lambda_id)) = map.get("_lambda_id") {
                        ids.push(lambda_id.clone());
                    }
                }
                // Recurse into object values
                for v in map.values() {
                    self.collect_lambda_ids(v, ids);
                }
            }
            Value::Array(arr) => {
                // Recurse into array elements
                for v in arr {
                    self.collect_lambda_ids(v, ids);
                }
            }
            _ => {}
        }
    }

    /// Equality comparison (JSONata semantics)
    fn equals(&self, left: &Value, right: &Value) -> bool {
        crate::functions::array::values_equal(left, right)
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
            // Undefined variable (null) with number -> undefined result
            (Value::Null, Value::Number(_)) | (Value::Number(_), Value::Null) => Ok(Value::Null),
            // Boolean with anything (including undefined) -> T2001 error
            (Value::Bool(_), _) => {
                Err(EvaluatorError::TypeError("T2001: The left side of the '+' operator must evaluate to a number or a string".to_string()))
            }
            (_, Value::Bool(_)) => {
                Err(EvaluatorError::TypeError("T2001: The right side of the '+' operator must evaluate to a number or a string".to_string()))
            }
            // Undefined with undefined -> undefined
            (Value::Null, Value::Null) => Ok(Value::Null),
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
                let result = a.as_f64().unwrap() * b.as_f64().unwrap();
                // Check for overflow to Infinity
                if result.is_infinite() {
                    return Err(EvaluatorError::EvaluationError(
                        "D1001: Number out of range".to_string()
                    ));
                }
                Ok(serde_json::json!(result))
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

    /// Ordered comparison with null/type checking shared across <, <=, >, >=
    ///
    /// `compare_nums` receives (left_f64, right_f64) for numeric operands.
    /// `compare_strs` receives (left_str, right_str) for string operands.
    /// `op_symbol` is used in the T2009 error message (e.g. "<", ">=").
    fn ordered_compare(
        &self,
        left: &Value,
        right: &Value,
        left_is_explicit_null: bool,
        right_is_explicit_null: bool,
        op_symbol: &str,
        compare_nums: fn(f64, f64) -> bool,
        compare_strs: fn(&str, &str) -> bool,
    ) -> Result<Value, EvaluatorError> {
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => {
                Ok(Value::Bool(compare_nums(a.as_f64().unwrap(), b.as_f64().unwrap())))
            }
            (Value::String(a), Value::String(b)) => Ok(Value::Bool(compare_strs(a, b))),
            // Both null/undefined -> return undefined
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
                Err(EvaluatorError::EvaluationError(format!(
                    "T2009: The expressions on either side of operator \"{}\" must be of the same data type",
                    op_symbol
                )))
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

    /// Less than comparison
    fn less_than(&self, left: &Value, right: &Value, left_is_explicit_null: bool, right_is_explicit_null: bool) -> Result<Value, EvaluatorError> {
        self.ordered_compare(left, right, left_is_explicit_null, right_is_explicit_null, "<", |a, b| a < b, |a, b| a < b)
    }

    /// Less than or equal comparison
    fn less_than_or_equal(&self, left: &Value, right: &Value, left_is_explicit_null: bool, right_is_explicit_null: bool) -> Result<Value, EvaluatorError> {
        self.ordered_compare(left, right, left_is_explicit_null, right_is_explicit_null, "<=", |a, b| a <= b, |a, b| a <= b)
    }

    /// Greater than comparison
    fn greater_than(&self, left: &Value, right: &Value, left_is_explicit_null: bool, right_is_explicit_null: bool) -> Result<Value, EvaluatorError> {
        self.ordered_compare(left, right, left_is_explicit_null, right_is_explicit_null, ">", |a, b| a > b, |a, b| a > b)
    }

    /// Greater than or equal comparison
    fn greater_than_or_equal(&self, left: &Value, right: &Value, left_is_explicit_null: bool, right_is_explicit_null: bool) -> Result<Value, EvaluatorError> {
        self.ordered_compare(left, right, left_is_explicit_null, right_is_explicit_null, ">=", |a, b| a >= b, |a, b| a >= b)
    }

    /// Convert a value to a string for concatenation
    fn value_to_concat_string(value: &Value) -> Result<String, EvaluatorError> {
        match value {
            Value::String(s) => Ok(s.clone()),
            Value::Null => Ok(String::new()),
            Value::Number(_) | Value::Bool(_) | Value::Array(_) | Value::Object(_) => {
                match crate::functions::string::string(value, None) {
                    Ok(Value::String(s)) => Ok(s),
                    Ok(Value::Null) => Ok(String::new()),
                    _ => Err(EvaluatorError::TypeError(
                        "Cannot concatenate complex types".to_string(),
                    )),
                }
            }
        }
    }

    /// String concatenation
    fn concatenate(&self, left: &Value, right: &Value) -> Result<Value, EvaluatorError> {
        let left_str = Self::value_to_concat_string(left)?;
        let right_str = Self::value_to_concat_string(right)?;
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

        // Check range size limit (10 million elements max)
        let size = if start <= end {
            (end - start + 1) as usize
        } else {
            0
        };
        if size > 10_000_000 {
            return Err(EvaluatorError::EvaluationError(
                "D2014: Range operator results in too many elements (> 10,000,000)".to_string(),
            ));
        }

        let mut result = Vec::with_capacity(size);
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
            captured_data: Some(data.clone()),
            thunk: false,
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

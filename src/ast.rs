// Abstract Syntax Tree definitions
// Mirrors the AST structure from jsonata.js

use serde::{Deserialize, Serialize};

/// Stage types that can be attached to path steps
///
/// In JSONata, predicates following path segments become "stages" that are applied
/// during the extraction process, not as separate steps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Stage {
    /// Filter/predicate stage [expr]
    Filter(Box<AstNode>),
}

/// A step in a path expression with optional stages
///
/// Stages are operations (like predicates) that apply during the step evaluation,
/// not after all steps are complete.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathStep {
    /// The main step node (field name, wildcard, etc.)
    pub node: AstNode,
    /// Stages to apply during this step (e.g., predicates)
    pub stages: Vec<Stage>,
    /// Set by `@$var` (focus binding): binds the step's per-element value to
    /// this variable name (without the `$` prefix) during tuple-stream
    /// evaluation. Mirrors jsonata-js's `step.focus`.
    pub focus: Option<String>,
    /// Set by `#$var` (index binding): binds the step's per-element index to
    /// this variable name (without the `$` prefix). Mirrors jsonata-js's
    /// `step.index`. Replaces the retired `AstNode::IndexBind` wrapping node.
    pub index_var: Option<String>,
    /// Set by ast_transform when a later `%` reference needs this step's
    /// *input* value preserved. The label is synthetic (e.g. "!0", "!1", ...)
    /// and used as a tuple-dict key at runtime. Mirrors jsonata-js's
    /// `step.ancestor.label`.
    pub ancestor_label: Option<String>,
    /// True when this step must participate in tuple-stream evaluation
    /// (because of its own `focus`/`index_var`/`ancestor_label`, or because
    /// an earlier step in the same path already entered tuple-stream mode).
    /// Mirrors jsonata-js's `step.tuple`.
    pub is_tuple: bool,
}

/// AST Node types
///
/// This enum represents all possible node types in a JSONata expression AST.
/// The structure closely mirrors the JavaScript implementation to facilitate
/// maintenance and upstream synchronization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AstNode {
    /// String literal (e.g., "hello", 'world')
    String(String),

    /// Field/property name in path expressions (e.g., foo in foo.bar)
    /// This is distinct from String: Name is a field access, String is a literal value
    Name(String),

    /// Number literal
    Number(f64),

    /// Boolean literal
    Boolean(bool),

    /// Null literal
    Null,

    /// Undefined literal (distinct from null in JavaScript semantics)
    /// In JSONata, undefined represents "no value" and propagates through expressions
    Undefined,

    /// Placeholder for partial application (?)
    /// When used as a function argument, creates a partially applied function
    Placeholder,

    /// Regex literal (e.g., /pattern/flags)
    Regex { pattern: String, flags: String },

    /// Variable reference (e.g., $var)
    Variable(String),

    /// Parent variable reference (e.g., $$)
    ParentVariable(String),

    /// Path expression (e.g., foo.bar)
    /// Each step can have stages (like predicates) attached
    Path { steps: Vec<PathStep> },

    /// Binary operation
    Binary {
        op: BinaryOp,
        lhs: Box<AstNode>,
        rhs: Box<AstNode>,
    },

    /// Unary operation
    Unary { op: UnaryOp, operand: Box<AstNode> },

    /// Function call by name
    Function {
        name: String,
        args: Vec<AstNode>,
        /// Whether this was called with $ prefix (built-in function)
        /// True for $string(x), false for string(x)
        is_builtin: bool,
    },

    /// Call an arbitrary expression as a function
    /// Used for IIFE patterns like `(function($x){...})(5)` or chained calls
    /// The procedure can be any expression that evaluates to a function
    Call {
        procedure: Box<AstNode>,
        args: Vec<AstNode>,
    },

    /// Lambda function definition
    Lambda {
        params: Vec<String>,
        body: Box<AstNode>,
        /// Optional signature for type checking (e.g., "<n-n:n>")
        signature: Option<String>,
        /// Whether this lambda's body is a thunk (contains tail call that should be optimized)
        /// A thunk wraps a tail-position function call for TCO
        #[serde(default)]
        thunk: bool,
    },

    /// Array constructor
    Array(Vec<AstNode>),

    /// Object constructor
    Object(Vec<(AstNode, AstNode)>),

    /// Object transform (postfix object constructor): expr{key: value}
    /// Transforms the input using the object pattern
    ObjectTransform {
        input: Box<AstNode>,
        pattern: Vec<(AstNode, AstNode)>,
    },

    /// Block expression
    Block(Vec<AstNode>),

    /// Conditional expression (? :)
    Conditional {
        condition: Box<AstNode>,
        then_branch: Box<AstNode>,
        else_branch: Option<Box<AstNode>>,
    },

    /// Wildcard operator (*) in path expressions
    Wildcard,

    /// Descendant operator (**) in path expressions
    Descendant,

    /// Parent-reference operator (%) in path expressions
    /// Resolved to a specific ancestor slot by the post-parse ast_transform pass;
    /// unit variant at parse time, matching jsonata-js's bare `{type: 'parent'}`.
    Parent,

    /// Array filter/predicate [condition]
    /// Can be an index (number) or a predicate (boolean expression)
    Predicate(Box<AstNode>),

    /// Array grouping in path expression .[expr]
    /// Like Array but doesn't flatten when used in paths
    ArrayGroup(Vec<AstNode>),

    /// Function application in path expression .(expr)
    /// Maps expr over the current value, with $ referring to each element
    FunctionApplication(Box<AstNode>),

    /// Sort operator in path expression ^(expr)
    /// Sorts the current value by evaluating expr for each element
    /// expr can be prefixed with < (ascending, default) or > (descending)
    Sort {
        /// The input expression to sort
        input: Box<AstNode>,
        /// Sort terms - list of (expression, ascending) tuples
        terms: Vec<(AstNode, bool)>,
    },

    /// Index binding operator #$var
    /// Binds the current array index to the specified variable during path traversal
    /// For example: arr#$i.field binds the index to $i for each element
    IndexBind {
        /// The input expression being indexed
        input: Box<AstNode>,
        /// The variable name to bind the index to (without the $ prefix)
        variable: String,
    },

    /// Transform operator |location|update[,delete]|
    /// Creates a function that transforms objects by:
    /// 1. Evaluating location to find objects to modify
    /// 2. Applying update (object constructor) to each matched object
    /// 3. Optionally deleting fields specified in delete array
    Transform {
        /// Expression to locate objects to transform
        location: Box<AstNode>,
        /// Object constructor expression for updates
        update: Box<AstNode>,
        /// Optional array of field names to delete
        delete: Option<Box<AstNode>>,
    },
}

/// Binary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryOp {
    // Arithmetic
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,

    // Comparison
    Equal,
    NotEqual,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,

    // Logical
    And,
    Or,

    // String
    Concatenate,

    // Range
    Range,

    // Other
    In,

    // Variable binding
    ColonEqual, // :=

    // Focus binding (raw parse-time marker for @$var; resolved into a
    // PathStep.focus flag by ast_transform, matching jsonata-js's own
    // parser.js:834-847, which also produces a generic binary node here)
    FocusBind,

    // Coalescing
    Coalesce, // ??

    // Default
    Default, // ?:

    // Function chaining/piping
    ChainPipe, // ~>
}

/// Unary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOp {
    /// Negation (-)
    Negate,

    /// Logical NOT
    Not,
}

impl PathStep {
    /// Create a path step from a node without stages
    pub fn new(node: AstNode) -> Self {
        PathStep {
            node,
            stages: Vec::new(),
            focus: None,
            index_var: None,
            ancestor_label: None,
            is_tuple: false,
        }
    }

    /// Create a path step with stages
    pub fn with_stages(node: AstNode, stages: Vec<Stage>) -> Self {
        PathStep {
            node,
            stages,
            focus: None,
            index_var: None,
            ancestor_label: None,
            is_tuple: false,
        }
    }
}

impl AstNode {
    /// Create a string literal node
    pub fn string(s: impl Into<String>) -> Self {
        AstNode::String(s.into())
    }

    /// Create a number literal node
    pub fn number(n: f64) -> Self {
        AstNode::Number(n)
    }

    /// Create a boolean literal node
    pub fn boolean(b: bool) -> Self {
        AstNode::Boolean(b)
    }

    /// Create a null literal node
    pub fn null() -> Self {
        AstNode::Null
    }

    /// Create an undefined literal node
    pub fn undefined() -> Self {
        AstNode::Undefined
    }

    /// Create a variable reference node
    pub fn variable(name: impl Into<String>) -> Self {
        AstNode::Variable(name.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ast_node_creation() {
        let str_node = AstNode::string("hello");
        assert!(matches!(str_node, AstNode::String(_)));

        let num_node = AstNode::number(42.0);
        assert!(matches!(num_node, AstNode::Number(_)));

        let bool_node = AstNode::boolean(true);
        assert!(matches!(bool_node, AstNode::Boolean(_)));

        let null_node = AstNode::null();
        assert!(matches!(null_node, AstNode::Null));
    }

    #[test]
    fn test_binary_op() {
        let node = AstNode::Binary {
            op: BinaryOp::Add,
            lhs: Box::new(AstNode::number(1.0)),
            rhs: Box::new(AstNode::number(2.0)),
        };
        assert!(matches!(node, AstNode::Binary { .. }));
    }

    #[test]
    fn test_path_step_new_defaults_new_fields_to_none() {
        let step = PathStep::new(AstNode::Name("foo".to_string()));
        assert_eq!(step.focus, None);
        assert_eq!(step.index_var, None);
        assert_eq!(step.ancestor_label, None);
        assert!(!step.is_tuple);
    }
}

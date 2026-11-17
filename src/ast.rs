// Abstract Syntax Tree definitions
// Mirrors the AST structure from jsonata.js

use serde::{Deserialize, Serialize};

/// Stage types that can be attached to path steps
///
/// In JSONata, predicates following path segments become "stages" that are applied
/// during the extraction process, not as separate steps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Stage {
    /// Filter/predicate stage `[expr]`
    Filter(Box<AstNode>),
    /// Empty-bracket stage `[]` -- jsonata's keepSingleton marker. Distinct
    /// from `Filter(Boolean(true))`, the literal filter `[true]`: `[]` keeps
    /// the result an array (`num[]` is `[5]`), while `[true]` keeps every
    /// item and then unwraps a lone result (`num[true]` is `5`).
    KeepArray,
    /// Positional index stage `#$var` that lands on a step already carrying an
    /// index/stages (jsonata-js's `{type: 'index', value}` stage). Binds the
    /// variable to each surviving tuple's position in the CURRENT (post-earlier-
    /// stages) result, so e.g. `books@$b#$ib[$l.isbn=$b.isbn]#$ib2` gives `$ib`
    /// the pre-filter book position and `$ib2` the post-filter position.
    Index(String),
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
    /// Set by ast_transform on a block or `.(...)` step that a `%` resolves
    /// through. jsonata-js's seekParent marks such a block's tail path
    /// `tuple`, so the step's result is that path's tuple stream, carrying
    /// its bindings, rather than its values.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub inner_tuple: bool,
    /// This step's identity, which the evaluator keys its cache of compiled
    /// step bodies by (#197). Each new step gets an id no other step has had
    /// in this process; a clone keeps it, being the same step. Not part of
    /// the step's meaning: ignored by equality, and serialization skips it
    /// (a deserialized step gets a new one). Code that changes the `node` of
    /// a step must give it a new id (`StepId::default()`) if a clone of the
    /// old step may be evaluated by the same evaluator.
    #[serde(skip)]
    pub id: StepId,
    /// Whether the evaluator runs this step's filters on nothing (#260),
    /// worked out from `stages` the first time the step runs, rather than
    /// each time (#288). A cache, not part of the step's meaning: ignored by
    /// equality and serialization. A clone keeps it, having the same stages;
    /// code that changes the `stages` of a step that has run must give it a
    /// new one (`StepCache::default()`).
    #[serde(skip)]
    pub runs_on_nothing: StepCache,
}

/// A value the evaluator works out once from a `PathStep` and keeps on it;
/// see `PathStep::runs_on_nothing`. Equal to any other, like `StepId`.
#[derive(Clone, Default)]
pub struct StepCache(std::sync::OnceLock<bool>);

impl StepCache {
    pub(crate) fn get_or_init(&self, init: impl FnOnce() -> bool) -> bool {
        *self.0.get_or_init(init)
    }
}

impl PartialEq for StepCache {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl std::fmt::Debug for StepCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StepCache")
    }
}

/// The identity of a path step; see `PathStep::id`. `StepId::default()` is
/// a new id, never handed out before in this process. It compares equal to
/// any other id, so that `PathStep`s are compared by content.
#[derive(Clone, Copy)]
pub struct StepId(u64);

impl StepId {
    pub(crate) fn get(self) -> u64 {
        self.0
    }
}

impl Default for StepId {
    fn default() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        StepId(NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }
}

impl PartialEq for StepId {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl std::fmt::Debug for StepId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StepId({})", self.0)
    }
}

/// AST Node types
///
/// This enum represents all possible node types in a JSONata expression AST.
/// The structure closely mirrors the JavaScript implementation to facilitate
/// maintenance and upstream synchronization.
///
/// `Clone` is implemented by hand (below) so that a deep tree is cloned
/// without overflowing the native stack.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
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

    /// Parent-reference operator (%) in path expressions, resolved.
    /// Carries the synthetic ancestor label ("!0", "!1", ...) assigned by
    /// ast_transform -- looked up at runtime via the same tuple/scope
    /// mechanism as $-variables. The parser produces `Parent(String::new())`
    /// (an empty placeholder never observed by the evaluator); ast_transform
    /// fills every occurrence with a real label or raises S0217. A `%` in a
    /// group-by's key or value gets a label nothing binds, so it evaluates
    /// to undefined, as in jsonata-js (#192).
    Parent(String),

    /// Array grouping in path expression `.[expr]`
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

    /// Transform operator `|location|update[,delete]|`
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

/// A clone of a tree as deep as the parser allows (about 1000 levels) needs
/// more native stack than a red zone leaves: a lambda's body is cloned each
/// time the lambda is created, which can happen deep inside a recursion with
/// only `EVAL_RED_ZONE` (src/evaluator.rs) of stack left, and the derived
/// `Clone`'s frame is a few hundred bytes per level (more on Windows). So
/// each level moves to a new stack segment when the current one runs low
/// (`value::with_stack`), as the value walkers do. Every variant is listed
/// with no wildcard, so a new variant must be added here to compile.
impl Clone for AstNode {
    fn clone(&self) -> Self {
        crate::value::with_stack(|| match self {
            AstNode::String(s) => AstNode::String(s.clone()),
            AstNode::Name(s) => AstNode::Name(s.clone()),
            AstNode::Number(n) => AstNode::Number(*n),
            AstNode::Boolean(b) => AstNode::Boolean(*b),
            AstNode::Null => AstNode::Null,
            AstNode::Undefined => AstNode::Undefined,
            AstNode::Placeholder => AstNode::Placeholder,
            AstNode::Regex { pattern, flags } => AstNode::Regex {
                pattern: pattern.clone(),
                flags: flags.clone(),
            },
            AstNode::Variable(s) => AstNode::Variable(s.clone()),
            AstNode::ParentVariable(s) => AstNode::ParentVariable(s.clone()),
            AstNode::Path { steps } => AstNode::Path {
                steps: steps.clone(),
            },
            AstNode::Binary { op, lhs, rhs } => AstNode::Binary {
                op: *op,
                lhs: lhs.clone(),
                rhs: rhs.clone(),
            },
            AstNode::Unary { op, operand } => AstNode::Unary {
                op: *op,
                operand: operand.clone(),
            },
            AstNode::Function {
                name,
                args,
                is_builtin,
            } => AstNode::Function {
                name: name.clone(),
                args: args.clone(),
                is_builtin: *is_builtin,
            },
            AstNode::Call { procedure, args } => AstNode::Call {
                procedure: procedure.clone(),
                args: args.clone(),
            },
            AstNode::Lambda {
                params,
                body,
                signature,
                thunk,
            } => AstNode::Lambda {
                params: params.clone(),
                body: body.clone(),
                signature: signature.clone(),
                thunk: *thunk,
            },
            AstNode::Array(items) => AstNode::Array(items.clone()),
            AstNode::Object(pairs) => AstNode::Object(pairs.clone()),
            AstNode::ObjectTransform { input, pattern } => AstNode::ObjectTransform {
                input: input.clone(),
                pattern: pattern.clone(),
            },
            AstNode::Block(items) => AstNode::Block(items.clone()),
            AstNode::Conditional {
                condition,
                then_branch,
                else_branch,
            } => AstNode::Conditional {
                condition: condition.clone(),
                then_branch: then_branch.clone(),
                else_branch: else_branch.clone(),
            },
            AstNode::Wildcard => AstNode::Wildcard,
            AstNode::Descendant => AstNode::Descendant,
            AstNode::Parent(label) => AstNode::Parent(label.clone()),
            AstNode::ArrayGroup(items) => AstNode::ArrayGroup(items.clone()),
            AstNode::FunctionApplication(inner) => AstNode::FunctionApplication(inner.clone()),
            AstNode::Sort { input, terms } => AstNode::Sort {
                input: input.clone(),
                terms: terms.clone(),
            },
            AstNode::Transform {
                location,
                update,
                delete,
            } => AstNode::Transform {
                location: location.clone(),
                update: update.clone(),
                delete: delete.clone(),
            },
        })
    }
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

    // Index binding (raw parse-time marker for #$var; resolved into a
    // PathStep.index_var flag by ast_transform). Reuses the same generic
    // Binary-node representation as FocusBind (rather than a dedicated
    // `AstNode::IndexBind` struct variant) so the retired `IndexBind`
    // variant has zero remaining construction sites once removed from
    // ast.rs. ast_transform's `process_path` resolves it.
    IndexBind,

    // Raw parse-time path operators, as jsonata-js's parser leaves them for
    // `processAST`: `lhs.rhs` (Map), `lhs[rhs]` (Filter) and `lhs[]`
    // (KeepArray, whose rhs is `Undefined`). ast_transform turns them into
    // `Path` steps and stages; the evaluator never sees them.
    Map,
    Filter,
    KeepArray,

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
            inner_tuple: false,
            id: StepId::default(),
            runs_on_nothing: StepCache::default(),
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
            inner_tuple: false,
            id: StepId::default(),
            runs_on_nothing: StepCache::default(),
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

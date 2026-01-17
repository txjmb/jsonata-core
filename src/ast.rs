// Abstract Syntax Tree definitions
// Mirrors the AST structure from jsonata.js

use serde::{Deserialize, Serialize};

/// AST Node types
///
/// This enum represents all possible node types in a JSONata expression AST.
/// The structure closely mirrors the JavaScript implementation to facilitate
/// maintenance and upstream synchronization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AstNode {
    /// String literal
    String(String),

    /// Number literal
    Number(f64),

    /// Boolean literal
    Boolean(bool),

    /// Null literal
    Null,

    /// Variable reference (e.g., $var)
    Variable(String),

    /// Path expression (e.g., foo.bar)
    Path {
        steps: Vec<AstNode>,
    },

    /// Binary operation
    Binary {
        op: BinaryOp,
        lhs: Box<AstNode>,
        rhs: Box<AstNode>,
    },

    /// Unary operation
    Unary {
        op: UnaryOp,
        operand: Box<AstNode>,
    },

    /// Function call
    Function {
        name: String,
        args: Vec<AstNode>,
    },

    /// Lambda function definition
    Lambda {
        params: Vec<String>,
        body: Box<AstNode>,
    },

    /// Array constructor
    Array(Vec<AstNode>),

    /// Object constructor
    Object(Vec<(AstNode, AstNode)>),

    /// Block expression
    Block(Vec<AstNode>),

    /// Conditional expression (? :)
    Conditional {
        condition: Box<AstNode>,
        then_branch: Box<AstNode>,
        else_branch: Option<Box<AstNode>>,
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
}

/// Unary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOp {
    /// Negation (-)
    Negate,

    /// Logical NOT
    Not,
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
}

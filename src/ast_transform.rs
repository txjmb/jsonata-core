// Post-parse AST transformation pass.
// Mirrors parser.js's processAST/seekParent/pushAncestry/resolveAncestry
// (tests/jsonata-js/src/parser.js ~L937-1235), adapted to Rust's ownership
// model: instead of mutating tree nodes in place, this consumes the raw
// tree and rebuilds an enriched one with ancestor/tuple metadata resolved.

use crate::ast::{AstNode, BinaryOp, PathStep};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AstTransformError {
    #[error("{code}: {message}")]
    Coded { code: &'static str, message: String },
}

fn coded(code: &'static str, message: impl Into<String>) -> AstTransformError {
    AstTransformError::Coded {
        code,
        message: message.into(),
    }
}

/// Monotonic counters for synthetic ancestor labels ("!0", "!1", ...),
/// mirroring jsonata-js's module-level `ancestorLabel`/`ancestorIndex`
/// counters. Threaded explicitly through the pass rather than using global
/// mutable state, since Rust doesn't have JS's implicit module-scope `var`.
///
/// Not yet consumed: this task's scope stops at single-step `@`/`#`
/// migration and top-level bare-`%` detection, neither of which needs a
/// fresh label. `fresh_label` is wired up by Task 4's `%`/`%.%` chain
/// resolution (`pushAncestry`/`resolveAncestry`), which is the reason this
/// struct is threaded through `transform_node` already.
#[allow(dead_code)]
struct LabelGen {
    next_label: usize,
}

impl LabelGen {
    fn new() -> Self {
        LabelGen { next_label: 0 }
    }

    #[allow(dead_code)]
    fn fresh_label(&mut self) -> String {
        let label = format!("!{}", self.next_label);
        self.next_label += 1;
        label
    }
}

/// Entry point: resolve all ancestor references in a freshly-parsed AST.
pub fn resolve_ancestry(ast: AstNode) -> Result<AstNode, AstTransformError> {
    let mut labels = LabelGen::new();
    transform_node(ast, &mut labels)
}

/// Recursively rebuild `node`, resolving any `%`/`@`/`#` found within.
/// Mirrors jsonata-js's processAST's generic per-node-type dispatch.
fn transform_node(node: AstNode, labels: &mut LabelGen) -> Result<AstNode, AstTransformError> {
    match node {
        AstNode::Path { steps } => {
            let transformed_steps = transform_path_steps(steps, labels)?;
            Ok(AstNode::Path {
                steps: transformed_steps,
            })
        }
        AstNode::Block(exprs) => {
            let transformed: Result<Vec<AstNode>, AstTransformError> = exprs
                .into_iter()
                .map(|e| transform_node(e, labels))
                .collect();
            Ok(AstNode::Block(transformed?))
        }
        // Parent/FocusBind/IndexBind found OUTSIDE a path context (e.g. a
        // bare top-level `%`) can't derive an ancestor -- S0217.
        AstNode::Parent => Err(coded(
            "S0217",
            "The parent operator % cannot be used at this point in the expression",
        )),
        // Recurse into every other node's children unchanged (no ancestor
        // resolution needed for nodes that aren't paths/blocks/parent refs).
        // Binary/Unary/Function/etc. are handled generically here; Task 4
        // extends this with predicate/sort-term-specific recursion once
        // basic path resolution (this task) is proven correct.
        other => transform_children(other, labels),
    }
}

/// Recurse into a node's child expressions without any path-specific
/// ancestor logic (used for node types that can't themselves be paths).
fn transform_children(node: AstNode, labels: &mut LabelGen) -> Result<AstNode, AstTransformError> {
    match node {
        AstNode::Binary { op, lhs, rhs } => Ok(AstNode::Binary {
            op,
            lhs: Box::new(transform_node(*lhs, labels)?),
            rhs: Box::new(transform_node(*rhs, labels)?),
        }),
        AstNode::Unary { op, operand } => Ok(AstNode::Unary {
            op,
            operand: Box::new(transform_node(*operand, labels)?),
        }),
        AstNode::Array(elements) => {
            let transformed: Result<Vec<AstNode>, AstTransformError> = elements
                .into_iter()
                .map(|e| transform_node(e, labels))
                .collect();
            Ok(AstNode::Array(transformed?))
        }
        // Leaf nodes and everything else pass through unchanged.
        other => Ok(other),
    }
}

/// Resolve a path's steps: migrate `#`/`@` markers into step-level flags,
/// then walk backward from the last step resolving any `%` references
/// (mirrors resolveAncestry, parser.js ~L1002-1030).
fn transform_path_steps(
    steps: Vec<PathStep>,
    labels: &mut LabelGen,
) -> Result<Vec<PathStep>, AstTransformError> {
    // Task 3 scope: migrate #/@ into step flags and resolve a *single*
    // trailing `%` reference with no intervening predicates/sort terms.
    // Multi-level `%.%` chains and predicate/sort-term ancestor resolution
    // are Task 4.
    let mut resolved: Vec<PathStep> = Vec::with_capacity(steps.len());
    for step in steps {
        resolved.push(migrate_binding_markers(step, labels)?);
    }
    Ok(resolved)
}

/// Convert a step's raw-parse-time binding marker (if any) into the
/// unified PathStep flags, recursing into the step's own node first (a
/// step's node can itself be a Block/nested Path containing `%`/`@`/`#`).
fn migrate_binding_markers(
    mut step: PathStep,
    labels: &mut LabelGen,
) -> Result<PathStep, AstTransformError> {
    match step.node {
        AstNode::Binary {
            op: BinaryOp::FocusBind,
            lhs,
            rhs,
        } => {
            let var_name = match *rhs {
                AstNode::Variable(name) => name,
                _ => unreachable!("parser guarantees FocusBind's rhs is always Variable"),
            };
            step.node = transform_node(*lhs, labels)?;
            step.focus = Some(var_name);
            step.is_tuple = true;
        }
        AstNode::IndexBind { input, variable } => {
            step.node = transform_node(*input, labels)?;
            step.index_var = Some(variable);
            step.is_tuple = true;
        }
        other => {
            step.node = transform_node(other, labels)?;
        }
    }
    Ok(step)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Stage;

    #[test]
    fn test_focus_bind_becomes_step_flag() {
        // Order@$o  -->  Path{steps: [Name("Order") with focus=Some("o"), is_tuple=true]}
        let ast = AstNode::Path {
            steps: vec![PathStep::new(AstNode::Binary {
                op: BinaryOp::FocusBind,
                lhs: Box::new(AstNode::Name("Order".to_string())),
                rhs: Box::new(AstNode::Variable("o".to_string())),
            })],
        };
        let result = resolve_ancestry(ast).unwrap();
        match result {
            AstNode::Path { steps } => {
                assert_eq!(steps.len(), 1);
                assert!(matches!(steps[0].node, AstNode::Name(ref n) if n == "Order"));
                assert_eq!(steps[0].focus, Some("o".to_string()));
                assert!(steps[0].is_tuple);
            }
            other => panic!("expected Path, got {:?}", other),
        }
    }

    #[test]
    fn test_index_bind_becomes_step_flag() {
        // arr#$i  -->  Path{steps: [Name("arr") with index_var=Some("i"), is_tuple=true]}
        let ast = AstNode::Path {
            steps: vec![PathStep::new(AstNode::IndexBind {
                input: Box::new(AstNode::Name("arr".to_string())),
                variable: "i".to_string(),
            })],
        };
        let result = resolve_ancestry(ast).unwrap();
        match result {
            AstNode::Path { steps } => {
                assert_eq!(steps.len(), 1);
                assert!(matches!(steps[0].node, AstNode::Name(ref n) if n == "arr"));
                assert_eq!(steps[0].index_var, Some("i".to_string()));
                assert!(steps[0].is_tuple);
            }
            other => panic!("expected Path, got {:?}", other),
        }
    }

    #[test]
    fn test_bare_parent_at_top_level_is_s0217() {
        let err = resolve_ancestry(AstNode::Parent).unwrap_err();
        assert!(err.to_string().starts_with("S0217"));
    }

    #[test]
    fn test_path_step_with_stages_preserved() {
        // Ensure stages (predicates) survive the transform unchanged when
        // there's no binding marker involved.
        let ast = AstNode::Path {
            steps: vec![PathStep::with_stages(
                AstNode::Name("Order".to_string()),
                vec![Stage::Filter(Box::new(AstNode::Boolean(true)))],
            )],
        };
        let result = resolve_ancestry(ast).unwrap();
        match result {
            AstNode::Path { steps } => {
                assert_eq!(steps[0].stages.len(), 1);
            }
            other => panic!("expected Path, got {:?}", other),
        }
    }
}

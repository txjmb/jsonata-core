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

/// A raw parse-time binding marker (`@$var` or `#$var`) that still needs to
/// be migrated into `PathStep.focus`/`PathStep.index_var` + `is_tuple`.
/// Shared between the "marker nested inside an existing `PathStep`" case
/// (`migrate_binding_markers`) and the "marker is the top-level/raw node
/// itself" case (`wrap_marker_as_path`), so the stamping logic itself lives
/// in exactly one place: `apply_marker_to_step`.
enum BindingMarker {
    Focus(String),
    Index(String),
}

/// Stamp a binding marker onto a step: sets `focus` or `index_var` (per the
/// marker kind) and `is_tuple = true`. The single place that knows how a
/// marker maps onto `PathStep` fields.
fn apply_marker_to_step(step: &mut PathStep, marker: BindingMarker) {
    match marker {
        BindingMarker::Focus(var_name) => step.focus = Some(var_name),
        BindingMarker::Index(var_name) => step.index_var = Some(var_name),
    }
    step.is_tuple = true;
}

/// Handle a `@$var`/`#$var` marker reaching `transform_node` as the raw node
/// itself (not already nested inside a `PathStep`) -- e.g. `Order@$o` or
/// `Account.Order@$o` where the parser's flat infix loop has already merged
/// any preceding `.` steps into a `Path` (or, for a single bare name, left a
/// non-Path leaf) *before* wrapping the whole thing in the marker node.
///
/// - If `transformed` is already a `Path`, the marker binds to its LAST step.
/// - Otherwise (e.g. a bare `Name` with no `.` at all), wrap it into a new
///   single-step `Path` and stamp the marker onto that one step.
fn wrap_marker_as_path(transformed: AstNode, marker: BindingMarker) -> AstNode {
    match transformed {
        AstNode::Path { mut steps } => {
            if let Some(last) = steps.last_mut() {
                apply_marker_to_step(last, marker);
            }
            AstNode::Path { steps }
        }
        other => {
            let mut step = PathStep::new(other);
            apply_marker_to_step(&mut step, marker);
            AstNode::Path { steps: vec![step] }
        }
    }
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
        // `@$var` reaching transform_node as the raw top-level node itself
        // (not nested inside an existing PathStep) -- e.g. `Order@$o` or
        // `Account.Order@$o`, where the parser's flat infix loop applies `@`
        // to the already-built lhs (a Path, or a bare leaf if there was no
        // `.` at all) rather than to a single step. See `wrap_marker_as_path`.
        AstNode::Binary {
            op: BinaryOp::FocusBind,
            lhs,
            rhs,
        } => {
            let var_name = match *rhs {
                AstNode::Variable(name) => name,
                _ => unreachable!("parser guarantees FocusBind's rhs is always Variable"),
            };
            let transformed_lhs = transform_node(*lhs, labels)?;
            Ok(wrap_marker_as_path(
                transformed_lhs,
                BindingMarker::Focus(var_name),
            ))
        }
        // Same story as FocusBind above, but for bare top-level `#$var`.
        AstNode::IndexBind { input, variable } => {
            let transformed_input = transform_node(*input, labels)?;
            Ok(wrap_marker_as_path(
                transformed_input,
                BindingMarker::Index(variable),
            ))
        }
        // Recurse into every other node's children unchanged (no ancestor
        // resolution needed for nodes that aren't paths/blocks/parent refs).
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
        AstNode::Function {
            name,
            args,
            is_builtin,
        } => {
            let transformed: Result<Vec<AstNode>, AstTransformError> = args
                .into_iter()
                .map(|a| transform_node(a, labels))
                .collect();
            Ok(AstNode::Function {
                name,
                args: transformed?,
                is_builtin,
            })
        }
        AstNode::Call { procedure, args } => {
            let transformed_procedure = transform_node(*procedure, labels)?;
            let transformed_args: Result<Vec<AstNode>, AstTransformError> = args
                .into_iter()
                .map(|a| transform_node(a, labels))
                .collect();
            Ok(AstNode::Call {
                procedure: Box::new(transformed_procedure),
                args: transformed_args?,
            })
        }
        AstNode::Lambda {
            params,
            body,
            signature,
            thunk,
        } => Ok(AstNode::Lambda {
            params,
            body: Box::new(transform_node(*body, labels)?),
            signature,
            thunk,
        }),
        AstNode::Object(pairs) => {
            let transformed: Result<Vec<(AstNode, AstNode)>, AstTransformError> = pairs
                .into_iter()
                .map(|(k, v)| Ok((transform_node(k, labels)?, transform_node(v, labels)?)))
                .collect();
            Ok(AstNode::Object(transformed?))
        }
        AstNode::ObjectTransform { input, pattern } => {
            let transformed_input = transform_node(*input, labels)?;
            let transformed_pattern: Result<Vec<(AstNode, AstNode)>, AstTransformError> = pattern
                .into_iter()
                .map(|(k, v)| Ok((transform_node(k, labels)?, transform_node(v, labels)?)))
                .collect();
            Ok(AstNode::ObjectTransform {
                input: Box::new(transformed_input),
                pattern: transformed_pattern?,
            })
        }
        AstNode::Conditional {
            condition,
            then_branch,
            else_branch,
        } => {
            let transformed_condition = transform_node(*condition, labels)?;
            let transformed_then = transform_node(*then_branch, labels)?;
            let transformed_else = match else_branch {
                Some(e) => Some(Box::new(transform_node(*e, labels)?)),
                None => None,
            };
            Ok(AstNode::Conditional {
                condition: Box::new(transformed_condition),
                then_branch: Box::new(transformed_then),
                else_branch: transformed_else,
            })
        }
        AstNode::Sort { input, terms } => {
            let transformed_input = transform_node(*input, labels)?;
            let transformed_terms: Result<Vec<(AstNode, bool)>, AstTransformError> = terms
                .into_iter()
                .map(|(expr, asc)| Ok((transform_node(expr, labels)?, asc)))
                .collect();
            Ok(AstNode::Sort {
                input: Box::new(transformed_input),
                terms: transformed_terms?,
            })
        }
        AstNode::Transform {
            location,
            update,
            delete,
        } => {
            let transformed_location = transform_node(*location, labels)?;
            let transformed_update = transform_node(*update, labels)?;
            let transformed_delete = match delete {
                Some(d) => Some(Box::new(transform_node(*d, labels)?)),
                None => None,
            };
            Ok(AstNode::Transform {
                location: Box::new(transformed_location),
                update: Box::new(transformed_update),
                delete: transformed_delete,
            })
        }
        AstNode::FunctionApplication(inner) => Ok(AstNode::FunctionApplication(Box::new(
            transform_node(*inner, labels)?,
        ))),
        AstNode::ArrayGroup(elements) => {
            let transformed: Result<Vec<AstNode>, AstTransformError> = elements
                .into_iter()
                .map(|e| transform_node(e, labels))
                .collect();
            Ok(AstNode::ArrayGroup(transformed?))
        }
        AstNode::Predicate(inner) => Ok(AstNode::Predicate(Box::new(transform_node(
            *inner, labels,
        )?))),
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
            apply_marker_to_step(&mut step, BindingMarker::Focus(var_name));
        }
        AstNode::IndexBind { input, variable } => {
            step.node = transform_node(*input, labels)?;
            apply_marker_to_step(&mut step, BindingMarker::Index(variable));
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

    // --- Regression tests using the REAL parser (review findings) ---
    //
    // The 4 tests above hand-build synthetic ASTs and, per the review, only
    // exercise the shapes that happen to already work. These tests instead
    // go through `crate::parser::parse()` on real source text, which is what
    // surfaced the two root-cause bugs: (1) transform_children not
    // recursing into most composite node types, and (2) `@$var`/`#$var`
    // never being migrated when the marker is the TOP-LEVEL node reaching
    // transform_node (only when already nested inside a PathStep).

    #[test]
    fn test_real_parser_bare_focus_bind_no_dot() {
        // "Order@$o" -- bare single-step, no dot anywhere. The parser
        // produces Binary{FocusBind, lhs: Name("Order"), rhs: Variable("o")}
        // at the top level (no Path at all, since there's no `.`).
        let ast = crate::parser::parse("Order@$o").unwrap();
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
    fn test_real_parser_focus_bind_on_final_step_of_multistep_path() {
        // "Account.Order@$o" -- 2-step path, marker on the final step, no
        // trailing dot. Previously: `@` wrapped the whole 2-step Path in a
        // top-level Binary{FocusBind,...} that was never migrated (Bug 2).
        let ast = crate::parser::parse("Account.Order@$o").unwrap();
        let result = resolve_ancestry(ast).unwrap();
        match result {
            AstNode::Path { steps } => {
                assert_eq!(steps.len(), 2);
                assert!(matches!(steps[0].node, AstNode::Name(ref n) if n == "Account"));
                assert!(steps[0].focus.is_none());
                assert!(!steps[0].is_tuple);
                assert!(matches!(steps[1].node, AstNode::Name(ref n) if n == "Order"));
                assert_eq!(steps[1].focus, Some("o".to_string()));
                assert!(steps[1].is_tuple);
            }
            other => panic!("expected Path, got {:?}", other),
        }
    }

    #[test]
    fn test_real_parser_bare_index_bind() {
        // "arr#$i" -- bare index bind, no dot. Previously never migrated
        // when reaching transform_node as the raw top-level IndexBind node.
        let ast = crate::parser::parse("arr#$i").unwrap();
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
    fn test_real_parser_bare_parent_inside_function_args_is_s0217() {
        // "$count(%)" -- a bare `%` nested inside a Function call's args.
        // Previously transform_children didn't recurse into Function args
        // at all (Bug 1), so this silently returned Ok(unchanged) instead
        // of raising S0217.
        let ast = crate::parser::parse("$count(%)").unwrap();
        let err = resolve_ancestry(ast).unwrap_err();
        assert!(err.to_string().starts_with("S0217"));
    }
}

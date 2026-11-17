// Post-parse AST transformation pass.
// Mirrors parser.js's processAST/seekParent/pushAncestry/resolveAncestry
// (tests/jsonata-js/src/parser.js ~L937-1235), adapted to Rust's ownership
// model: instead of mutating tree nodes in place, this consumes the raw
// tree and rebuilds an enriched one with ancestor/tuple metadata resolved.
//
// ── Recursion-depth safety (the invariant this file must preserve) ──────────
//
// `parser::parse()` pipes every parse through `resolve_ancestry`, so a deeply
// nested input can overflow the native stack HERE even when the Pratt parse
// succeeded (confirmed empirically with a 200,000-term `1+1+1+...` chain).
// See docs/superpowers/plans/2026-07-07-parser-depth-and-u16-truncation-fixes-plan.md
// for the full derivation. The rules, in short:
//
// 1. There are three recursive cycles but only TWO independent stack budgets.
//    Cycle 1 (transform_node / transform_children / process_path /
//    process_base / apply_map / apply_filter / apply_group / apply_sort) and
//    cycle 3 (walk_backward / seek_parent_step /
//    seek_parent_wrapped) nest on the SAME native stack — cycle 3 is reached
//    while cycle 1's frames are live — so they must share ONE depth counter.
//    Cycle 2 (substitute_labels) runs only after cycle 1 has fully unwound
//    and gets its own counter.
//
// 2. Dropping an abandoned deep subtree recurses in the Drop glue,
//    independently of any depth counter. Every owning bail-out (an `Err`
//    return that still holds a subtree) must dismantle it iteratively via
//    `push_ast_node_children`, never let it fall out of scope. That function
//    deliberately has no `_` wildcard arm so a new AST variant cannot
//    silently opt out.
//
// Functions are referenced by NAME throughout this file — line numbers rot.

use crate::ast::{AstNode, BinaryOp, PathStep, Stage, StepId};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AstTransformError {
    #[error("{code}: {message}")]
    Coded { code: &'static str, message: String },
    /// A syntax error whose message starts with its code (S0209, S0210,
    /// S0213), reported as the parser's `InvalidSyntax`.
    #[error("{0}")]
    Syntax(String),
}

/// jsonata-js's message for S0217 (`errorCodes` in jsonata.js), used for
/// every place an ancestor can't be derived.
const S0217_MESSAGE: &str =
    "The object representing the 'parent' cannot be derived from this expression";

fn coded(code: &'static str, message: impl Into<String>) -> AstTransformError {
    AstTransformError::Coded {
        code,
        message: message.into(),
    }
}

// --- Recursion-depth safety (Task 2 of the plan referenced in the module
// doc comment above) ---
//
// Two INDEPENDENT depth counters, per the module doc comment's analysis:
// one shared by cycle 1 (transform_node/transform_children/process_path
// and the apply_* functions it calls) AND cycle 3
// (walk_backward/seek_parent_step/seek_parent_wrapped) -- because cycle 3 is
// reached from WITHIN live cycle-1 stack frames, their depths add and must
// share one counter -- and a second, wholly separate counter for
// substitute_labels (cycle 2), which only ever runs after cycle 1/3 have
// fully unwound (see `resolve_ancestry`). Do NOT let these two counters
// influence each other.
//
// A LATER guard was added directly in `parser.rs` (`MAX_PARSE_DEPTH`, also
// 1000) that bounds the parser's OWN recursion/loop-iteration counter --
// this is NOT the same thing as "real AST tree depth is always <=1000".
// Calibrated empirically (throwaway harness, this session) across several
// shapes: for simple ones (flat Binary/Unary/Array/Object chains) the
// parser's counter tracks real tree depth 1:1, so `ast_transform`'s own
// ceiling below fires first, well before the parser's. But for COMPOUND
// shapes where one parser-guarded call/iteration corresponds to more than
// one real `AstNode`-nesting hop (e.g. `a.(a.(a.(...)))`, where each level
// costs one recursive `parse_expression` call for the `FunctionApplication`
// body PLUS the `Path` wrapper it returns into), real tree depth reachable
// via a parser-accepted expression can run up to ~2x the parser's own
// counter value (observed: parser counter 999 -> real tree depth 2000).
// This means DO NOT assume "the parser already bounds real depth to
// <=1000" as a reason to raise this file's ceiling to just above 1000 --
// that reasoning is unsound for compound shapes and was corrected before
// shipping (see PR discussion). This file's own ceiling stays genuinely
// load-bearing for those shapes, not just cosmetic defense-in-depth.
//
// Ceiling rationale: chosen empirically (see
// `test_deeply_nested_arithmetic_does_not_overflow_native_stack_at_parse_time`
// and `test_reasonable_nesting_still_parses_successfully` in
// tests/integration_test.rs) to be comfortably safe on a 1MB-stack thread
// (matching Windows' default, the same constraint `evaluate_internal`'s
// analogous guard in evaluator.rs was built for). `stacker::maybe_grow`
// below helps DURING this file's own guarded traversal, but it is NOT the
// only thing keeping this ceiling load-bearing:
//
// A tree that successfully passes this guard (returns `Ok` all the way out
// of `parser::parse()`) is handed to whatever caller holds it next (the
// evaluator, the Python `JsonataExpression`, a test's local variable) and
// is eventually dropped there via Rust's ORDINARY recursive `Drop` glue --
// NOT the iterative teardown this file uses on its own bail-out path (see
// `push_ast_node_children`'s doc comment below). Confirmed empirically this
// session: simply constructing then normally-dropping a `Box<AstNode>`
// chain somewhere between ~5,000 and ~20,000 levels deep overflows a
// 1MB-stack thread, with ZERO ast_transform code involved. This ceiling
// (1000, i.e. at most ~1000 levels of ACTUAL AST nesting, since cycle 1
// costs >=1 depth unit per level) stays far enough below that downstream
// threshold that any successfully-returned tree is safe for a caller to
// drop normally. RAISING this ceiling without separately re-verifying the
// downstream-drop threshold would silently reintroduce a native-stack
// crash on the SUCCESS path -- a different crash than the one this task
// fixes, and not exercised by either test above (one only exercises the
// bail path at n=200,000; the other only exercises a SHALLOW successful
// tree, not a near-ceiling one).
const MAX_TRANSFORM_DEPTH: usize = 1000;
// Same ceiling as MAX_TRANSFORM_DEPTH, for a DIFFERENT reason than "2x
// headroom" might suggest: cycle 1 costs >=1 depth unit per level of ACTUAL
// AST nesting (2 units for a Binary/Unary/etc. level via the
// transform_node+transform_children pair, but exactly 1 unit for a level
// that's pure nested blocks/parens, e.g. `((((...))))`, which only ever
// enters transform_node's Block arm -- no transform_children hop). For
// THAT shape, this counter and MAX_TRANSFORM_DEPTH are checking the exact
// same per-level cost, so a tree that just barely passes cycle 1 (depth
// ~1000) can arrive at substitute_labels already ~1000 deep too -- zero
// margin, not 2x. It's still safe (substitute_labels's own bail-out lets
// its `node` drop normally, unlike cycle 1/3's iterative-teardown bail
// path, but by the time cycle 2 runs, cycle 1 already guaranteed the tree
// is <= this same ceiling deep, well below the ~5,000-20,000-level
// downstream-drop threshold noted above) -- just not defended by a margin,
// so don't raise this independently of MAX_TRANSFORM_DEPTH without
// re-checking this reasoning.
const MAX_LABEL_SUBSTITUTION_DEPTH: usize = 1000;

// Same constants `evaluate_internal` (src/evaluator.rs) uses for its
// analogous native-stack safety net -- see that function's doc comment for
// the full rationale. Kept as separate constants (rather than reused from
// evaluator.rs) since this module has no dependency on evaluator.rs and the
// two guards are conceptually independent safety nets.
const AST_TRANSFORM_RED_ZONE: usize = 128 * 1024;
const AST_TRANSFORM_GROW_STACK_SIZE: usize = 8 * 1024 * 1024;

/// New error code `U1002` (no jsonata-js equivalent, like its sibling
/// `U1001` in evaluator.rs -- JS has no comparable native-stack-overflow
/// failure mode): a `U`-prefixed, not `S`-prefixed, code since this is a
/// Rust-implementation-specific resource guard on an otherwise syntactically
/// valid expression, not a JSONata syntax error. Using an `S0`-numbered slot
/// (the next unused being S0218) would risk colliding with a future upstream
/// jsonata-js `S0218` that means something else entirely; `U1001` is already
/// taken by evaluator.rs's analogous stack-depth guard, so this is `U1002`.
fn check_transform_depth(depth: usize) -> Result<(), AstTransformError> {
    if depth > MAX_TRANSFORM_DEPTH {
        Err(coded(
            "U1002",
            format!(
                "Stack overflow - maximum expression nesting depth ({}) exceeded while post-processing the parsed expression",
                MAX_TRANSFORM_DEPTH
            ),
        ))
    } else {
        Ok(())
    }
}

/// See `check_transform_depth` -- same error code, separate counter/ceiling
/// for `substitute_labels`'s own independent recursion (cycle 2).
fn check_label_substitution_depth(depth: usize) -> Result<(), AstTransformError> {
    if depth > MAX_LABEL_SUBSTITUTION_DEPTH {
        Err(coded(
            "U1002",
            format!(
                "Stack overflow - maximum expression nesting depth ({}) exceeded while finalizing the parsed expression",
                MAX_LABEL_SUBSTITUTION_DEPTH
            ),
        ))
    } else {
        Ok(())
    }
}

// --- Iterative teardown for the depth guard's bail-out path ---
//
// A SECOND, independent stack-overflow vector from the traversal recursion
// the depth guard above protects against, found empirically while
// validating this task's fix: when `check_transform_depth`/
// `check_label_substitution_depth` trips inside `transform_node`/
// `transform_children`/`process_path`, that function's OWN
// `node`/`steps` parameter can still hold an ENORMOUS unprocessed remainder
// (we bail out before ever destructuring it -- e.g. a `1+1+1+...` chain 200
// levels past the ceiling still has ~199,800 more nested `Binary` levels
// hanging off the node we're about to return `Err` for). If we just let
// that `node`/`steps` value drop normally as the function returns, Rust's
// compiler-generated recursive `Drop` glue walks the WHOLE remaining chain
// one native stack frame per nesting level -- completely bypassing the
// depth counter above, since that glue isn't a function call this file
// controls. Confirmed empirically, independent of any ast_transform code:
// simply constructing then normally-dropping a ~20,000-deep `Box<AstNode>`
// chain overflows a 1MB-stack thread outright.
//
// The fix is an explicit heap work-list (`Vec<AstNode>`) instead of the
// call stack: pop one node, move its `Box<AstNode>`/`Vec<AstNode>`/
// `Vec<PathStep>` children onto the list (dropping only that one node's own
// shallow, non-recursive fields for free as the match arm's temporaries go
// out of scope), repeat. Stack usage is O(1) regardless of how deep the
// abandoned remainder is, because no recursive function call (guarded or
// not) is ever made -- only a `while` loop over a heap-allocated `Vec`.
//
// Every function that OWNS an `AstNode`/`Vec<PathStep>`/`PathStep`
// parameter and can bail out with that parameter's traversal still
// incomplete (`transform_node`, `transform_children`, `process_path` with
// the path operators it has yet to apply, and the `apply_*` functions with
// the steps they were handed) must route the abandoned
// value through here instead of letting it drop implicitly. Anything that
// already came back out of a SUCCESSFUL (`Ok`) call is guaranteed
// depth-bounded (that call would have hit the guard above otherwise) and is
// always safe to drop normally.
fn push_ast_node_children(node: AstNode, stack: &mut Vec<AstNode>) {
    match node {
        AstNode::Path { steps } => {
            for step in steps {
                push_path_step_children(step, stack);
            }
        }
        AstNode::Binary { lhs, rhs, .. } => {
            stack.push(*lhs);
            stack.push(*rhs);
        }
        AstNode::Unary { operand, .. } => stack.push(*operand),
        AstNode::Function { args, .. } => stack.extend(args),
        AstNode::Call { procedure, args } => {
            stack.push(*procedure);
            stack.extend(args);
        }
        AstNode::Lambda { body, .. } => stack.push(*body),
        AstNode::Array(elements) | AstNode::Block(elements) | AstNode::ArrayGroup(elements) => {
            stack.extend(elements);
        }
        AstNode::Object(pairs) => {
            for (k, v) in pairs {
                stack.push(k);
                stack.push(v);
            }
        }
        AstNode::ObjectTransform { input, pattern } => {
            stack.push(*input);
            for (k, v) in pattern {
                stack.push(k);
                stack.push(v);
            }
        }
        AstNode::Conditional {
            condition,
            then_branch,
            else_branch,
        } => {
            stack.push(*condition);
            stack.push(*then_branch);
            if let Some(e) = else_branch {
                stack.push(*e);
            }
        }
        AstNode::Sort { input, terms } => {
            stack.push(*input);
            for (e, _asc) in terms {
                stack.push(e);
            }
        }
        AstNode::Transform {
            location,
            update,
            delete,
        } => {
            stack.push(*location);
            stack.push(*update);
            if let Some(d) = delete {
                stack.push(*d);
            }
        }
        AstNode::FunctionApplication(inner) => stack.push(*inner),
        // Leaf nodes: no nested AstNode -- whatever's left of `node` (a
        // String, an f64, ...) is dropped here for free, in O(1), as this
        // match arm ends. Listed EXPLICITLY (no `_` wildcard) on purpose:
        // this function is the stack-overflow-on-drop guard for abandoned
        // deep subtrees, and a future variant holding a `Box<AstNode>` that
        // fell into a wildcard would silently reintroduce the recursive-drop
        // crash. A new variant must fail compilation here and be classified.
        AstNode::String(_)
        | AstNode::Name(_)
        | AstNode::Number(_)
        | AstNode::Boolean(_)
        | AstNode::Null
        | AstNode::Undefined
        | AstNode::Placeholder
        | AstNode::Regex { .. }
        | AstNode::Variable(_)
        | AstNode::ParentVariable(_)
        | AstNode::Wildcard
        | AstNode::Descendant
        | AstNode::Parent(_) => {}
    }
}

/// See `push_ast_node_children` -- the `PathStep`-flavored equivalent
/// (a step's `.node` plus any `Stage::Filter` predicate expressions its
/// `.stages` carries; the step's other fields -- `focus`/`index_var`/
/// `ancestor_label`/`is_tuple` -- are never recursive, so they drop for
/// free here too).
fn push_path_step_children(step: PathStep, stack: &mut Vec<AstNode>) {
    stack.push(step.node);
    push_stage_children(step.stages, stack);
}

/// See `push_ast_node_children` -- pulls the `Box<AstNode>` out of every
/// `Stage::Filter` (a step's predicate expressions) onto the work-list;
/// `Stage::Index` carries only a variable name, nothing recursive.
fn push_stage_children(stages: Vec<Stage>, stack: &mut Vec<AstNode>) {
    for stage in stages {
        if let Stage::Filter(e) = stage {
            stack.push(*e);
        }
    }
}

/// Entry point: drop `node` iteratively rather than via the ordinary
/// recursive `Drop` glue. See the doc comment above `push_ast_node_children`
/// for why this exists.
fn drop_ast_node_iteratively(node: AstNode) {
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        push_ast_node_children(n, &mut stack);
    }
}

/// See `drop_ast_node_iteratively` -- the `Vec<PathStep>`-flavored entry
/// point, used by the `apply_*` functions' bail-outs (they own a path's
/// `Vec<PathStep>`, not a single `AstNode`).
fn drop_path_steps_iteratively(steps: Vec<PathStep>) {
    let mut stack = Vec::new();
    for step in steps {
        push_path_step_children(step, &mut stack);
    }
    while let Some(n) = stack.pop() {
        push_ast_node_children(n, &mut stack);
    }
}

/// Builds the `U1002` error without needing ownership of the abandoned
/// node/steps -- kept separate from `drop_ast_node_iteratively`/
/// `drop_path_steps_iteratively` so callers can drop first, then construct
/// the error, in either order convenient at the call site.
fn max_transform_depth_error() -> AstTransformError {
    coded(
        "U1002",
        format!(
            "Stack overflow - maximum expression nesting depth ({}) exceeded while post-processing the parsed expression",
            MAX_TRANSFORM_DEPTH
        ),
    )
}

/// A `%` reference still seeking its ancestor step, mirroring jsonata-js's
/// `slot` object (`{label, level, index}` -- we don't need `index`, since
/// that's only used by jsonata-js to index into its global mutable
/// `ancestry` array for the in-place relabeling trick; see `AncestryState`
/// for how we get the same "reuse an existing label" behavior without it).
#[derive(Debug, Clone)]
struct PendingAncestor {
    label: String,
    /// Remaining backward steps needed before this reference resolves.
    /// A fresh `%` starts at level 1 (its own immediately-preceding step);
    /// walking backward over ANOTHER not-yet-resolved `%` step increments
    /// this (mirrors seekParent's `case 'parent': slot.level++`).
    level: usize,
}

/// Threaded through the whole pass: generates fresh synthetic ancestor
/// labels ("!0", "!1", ...) and records label aliases.
///
/// Rust's immutable-rebuild model can't replicate jsonata-js's in-place
/// mutation `ancestry[slot.index].slot.label = node.ancestor.label` (used
/// when a *second* `%` resolves to a step some *earlier* `%` already
/// tagged -- jsonata-js renames the second slot's label to match the first,
/// by mutating a shared JS object referenced from both the `ancestry` array
/// and the corresponding `AstNode::Parent` node already sitting in the
/// tree). Since our tree nodes are owned values already moved by the time a
/// later reuse is discovered, we can't reach back in and rewrite an
/// already-built `Parent(label)` node in place. Instead: record the alias
/// (`new_label -> canonical_label`) here as resolution proceeds, then run
/// one final substitution pass (`substitute_labels`, called from
/// `resolve_ancestry` after the whole tree is built) that rewrites every
/// `AstNode::Parent(label)` to its canonical form. `PathStep.ancestor_label`
/// itself never needs substitution: it's set at most once per step (the
/// first `%` to resolve there), so it's always already canonical.
struct AncestryState {
    next_label: usize,
    aliases: HashMap<String, String>,
}

impl AncestryState {
    fn new() -> Self {
        AncestryState {
            next_label: 0,
            aliases: HashMap::new(),
        }
    }

    fn fresh_label(&mut self) -> String {
        let label = format!("!{}", self.next_label);
        self.next_label += 1;
        label
    }

    /// Follow the alias chain to a label's canonical form. Chains longer
    /// than one hop shouldn't arise (a step's `ancestor_label`, once set, is
    /// never itself replaced -- only newcomers get aliased to it) but this
    /// still follows the chain defensively rather than assuming depth 1.
    fn canonical(&self, label: &str) -> String {
        let mut cur = label;
        while let Some(next) = self.aliases.get(cur) {
            cur = next;
        }
        cur.to_string()
    }
}

/// The result of transforming a node: the rebuilt node, plus any `%`
/// references within it that are still seeking an ancestor step, bubbling
/// up to whatever contains this node -- mirrors jsonata-js's `seekingParent`
/// array property, attached to whatever node `pushAncestry` was called on.
struct Transformed {
    node: AstNode,
    pending: Vec<PendingAncestor>,
}

impl Transformed {
    fn leaf(node: AstNode) -> Self {
        Transformed {
            node,
            pending: Vec::new(),
        }
    }
}

/// Entry point: resolve all ancestor references in a freshly-parsed AST.
/// Crate-private: only the parser's raw tree (`Parser::parse_raw`) can be
/// passed in.
///
/// It takes the parser's raw tree, before paths are built: `.`, `[`, `{`,
/// `^`, `@` and `#` as raw operators, and a name as a one-step path.
/// `Parser::parse` and `parser::parse` already call it; a tree they
/// return can't be passed in again.
pub(crate) fn resolve_ancestry(ast: AstNode) -> Result<AstNode, AstTransformError> {
    let mut state = AncestryState::new();
    // Depth 0: the root of cycle 1+3's shared counter.
    let transformed = transform_node(ast, &mut state, 0)?;
    // Mirrors jsonata-js's final check (parser.js ~L1404): a bare `%` as the
    // WHOLE expression, or any dangling (never-resolved) pending ancestor
    // reference that bubbled all the way to the top, means there was no
    // enclosing path to derive an ancestor from.
    if !transformed.pending.is_empty() || matches!(transformed.node, AstNode::Parent(_)) {
        return Err(coded("S0217", S0217_MESSAGE));
    }
    // Depth 0 again: substitute_labels is a SEPARATE, independent counter --
    // cycle 1+3's frames are gone by now (transform_node above already
    // returned), so this is not a continuation of the depth above.
    substitute_labels(transformed.node, &state, 0)
}

/// Final pass: rewrite every `AstNode::Parent(label)` in the tree to its
/// canonical (alias-resolved) label. See `AncestryState` for why this is a
/// separate pass rather than done inline. Mirrors `transform_children`'s
/// traversal shape exactly (every composite node type), since by this point
/// there's no error case left to handle -- the tree is already fully valid.
fn substitute_labels(
    node: AstNode,
    state: &AncestryState,
    depth: usize,
) -> Result<AstNode, AstTransformError> {
    check_label_substitution_depth(depth)?;
    stacker::maybe_grow(
        AST_TRANSFORM_RED_ZONE,
        AST_TRANSFORM_GROW_STACK_SIZE,
        || substitute_labels_impl(node, state, depth),
    )
}

fn substitute_labels_impl(
    node: AstNode,
    state: &AncestryState,
    depth: usize,
) -> Result<AstNode, AstTransformError> {
    let depth = depth + 1;
    Ok(match node {
        AstNode::Parent(label) => AstNode::Parent(state.canonical(&label)),
        AstNode::Path { steps } => AstNode::Path {
            steps: steps
                .into_iter()
                .map(|s| -> Result<PathStep, AstTransformError> {
                    Ok(PathStep {
                        node: substitute_labels(s.node, state, depth)?,
                        // Stages (predicates) can contain `%` references whose
                        // labels were aliased during resolution (e.g. a second
                        // predicate reusing a step an earlier one already tagged),
                        // so they must be substituted too -- otherwise the
                        // pre-alias label survives and evaluates against the wrong
                        // tuple key.
                        stages: s
                            .stages
                            .into_iter()
                            .map(|st| -> Result<Stage, AstTransformError> {
                                Ok(match st {
                                    Stage::Filter(e) => Stage::Filter(Box::new(substitute_labels(
                                        *e, state, depth,
                                    )?)),
                                    Stage::Index(v) => Stage::Index(v),
                                    Stage::KeepArray => Stage::KeepArray,
                                })
                            })
                            .collect::<Result<Vec<_>, _>>()?,
                        // `node` changed: a new step as far as the
                        // evaluator's compiled-step cache is concerned.
                        id: crate::ast::StepId::default(),
                        runs_on_nothing: crate::ast::StepCache::default(),
                        ..s
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        },
        AstNode::Block(exprs) => AstNode::Block(
            exprs
                .into_iter()
                .map(|e| substitute_labels(e, state, depth))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        AstNode::Binary { op, lhs, rhs } => AstNode::Binary {
            op,
            lhs: Box::new(substitute_labels(*lhs, state, depth)?),
            rhs: Box::new(substitute_labels(*rhs, state, depth)?),
        },
        AstNode::Unary { op, operand } => AstNode::Unary {
            op,
            operand: Box::new(substitute_labels(*operand, state, depth)?),
        },
        AstNode::Array(elements) => AstNode::Array(
            elements
                .into_iter()
                .map(|e| substitute_labels(e, state, depth))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        AstNode::Function {
            name,
            args,
            is_builtin,
        } => AstNode::Function {
            name,
            args: args
                .into_iter()
                .map(|a| substitute_labels(a, state, depth))
                .collect::<Result<Vec<_>, _>>()?,
            is_builtin,
        },
        AstNode::Call { procedure, args } => AstNode::Call {
            procedure: Box::new(substitute_labels(*procedure, state, depth)?),
            args: args
                .into_iter()
                .map(|a| substitute_labels(a, state, depth))
                .collect::<Result<Vec<_>, _>>()?,
        },
        AstNode::Lambda {
            params,
            body,
            signature,
            thunk,
        } => AstNode::Lambda {
            params,
            body: Box::new(substitute_labels(*body, state, depth)?),
            signature,
            thunk,
        },
        AstNode::Object(pairs) => AstNode::Object(
            pairs
                .into_iter()
                .map(|(k, v)| -> Result<(AstNode, AstNode), AstTransformError> {
                    Ok((
                        substitute_labels(k, state, depth)?,
                        substitute_labels(v, state, depth)?,
                    ))
                })
                .collect::<Result<Vec<_>, _>>()?,
        ),
        AstNode::ObjectTransform { input, pattern } => AstNode::ObjectTransform {
            input: Box::new(substitute_labels(*input, state, depth)?),
            pattern: pattern
                .into_iter()
                .map(|(k, v)| -> Result<(AstNode, AstNode), AstTransformError> {
                    Ok((
                        substitute_labels(k, state, depth)?,
                        substitute_labels(v, state, depth)?,
                    ))
                })
                .collect::<Result<Vec<_>, _>>()?,
        },
        AstNode::Conditional {
            condition,
            then_branch,
            else_branch,
        } => AstNode::Conditional {
            condition: Box::new(substitute_labels(*condition, state, depth)?),
            then_branch: Box::new(substitute_labels(*then_branch, state, depth)?),
            else_branch: match else_branch {
                Some(e) => Some(Box::new(substitute_labels(*e, state, depth)?)),
                None => None,
            },
        },
        AstNode::Sort { input, terms } => AstNode::Sort {
            input: Box::new(substitute_labels(*input, state, depth)?),
            terms: terms
                .into_iter()
                .map(|(e, asc)| -> Result<(AstNode, bool), AstTransformError> {
                    Ok((substitute_labels(e, state, depth)?, asc))
                })
                .collect::<Result<Vec<_>, _>>()?,
        },
        AstNode::Transform {
            location,
            update,
            delete,
        } => AstNode::Transform {
            location: Box::new(substitute_labels(*location, state, depth)?),
            update: Box::new(substitute_labels(*update, state, depth)?),
            delete: match delete {
                Some(d) => Some(Box::new(substitute_labels(*d, state, depth)?)),
                None => None,
            },
        },
        AstNode::FunctionApplication(inner) => {
            AstNode::FunctionApplication(Box::new(substitute_labels(*inner, state, depth)?))
        }
        AstNode::ArrayGroup(elements) => AstNode::ArrayGroup(
            elements
                .into_iter()
                .map(|e| substitute_labels(e, state, depth))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        // Leaf nodes and everything else pass through unchanged.
        other => other,
    })
}

/// jsonata-js's `@` checks (`processAST`, binary `@`): S0215 when the step
/// it binds has predicates (`[]` is not one), S0216 when it is a sort. A
/// group-by on a node that isn't a path wraps the node, so the check looks
/// at the node it groups (`X[p]{...}@$v` is S0215).
fn check_focus_bind_target(
    target_stages: &[Stage],
    target_node: &AstNode,
) -> Result<(), AstTransformError> {
    // `[]` is a flag on the step in jsonata-js, not one of its predicates.
    if target_stages
        .iter()
        .any(|stage| !matches!(stage, Stage::KeepArray))
    {
        return Err(coded(
            "S0215",
            "A context variable binding must precede any predicates on a step",
        ));
    }
    if let AstNode::ObjectTransform { input, .. } = target_node {
        return match &**input {
            AstNode::Path { steps } => match steps.last() {
                Some(last) => check_focus_bind_target(&last.stages, &last.node),
                None => Ok(()),
            },
            other => check_focus_bind_target(&[], other),
        };
    }
    if matches!(target_node, AstNode::Sort { .. }) {
        return Err(coded(
            "S0216",
            "A context variable binding must precede the 'order-by' clause on a step",
        ));
    }
    Ok(())
}

/// What jsonata-js's `processAST` gives for a node that the path operators
/// work on: a path (`is_path`, the `{type: 'path'}` node, with its steps and
/// its own group-by) or any other node, held as the one step in `steps`
/// so that the flags those operators set on it (`stages` for its
/// predicates, `focus`, ...) have somewhere to go. A group-by on a node
/// that isn't a path wraps the node at once, as the step's
/// `ObjectTransform`. `pending` is the node's `seekingParent`: for a path,
/// the `%` slots its steps couldn't resolve; for another node, every slot
/// it carries, including predicate slots that already resolved (#211).
struct Proc {
    steps: Vec<PathStep>,
    is_path: bool,
    /// Whether a `[]` on the path's step keeps its result an array: false
    /// for a path that `#` made of another node, which jsonata-js gives no
    /// `keepSingletonArray`.
    dotted: bool,
    group: Option<Vec<(AstNode, AstNode)>>,
    pending: Vec<PendingAncestor>,
    /// The `%` slots the path's last step seeks itself (jsonata-js's
    /// `seekingParent` on that step rather than on the path): from a node
    /// that `#` made a path of, or a predicate on a path's step. Only a `.`
    /// after the path looks at them, and only while that step is last:
    /// another step after it, or the path used as a value, forgets them.
    step_pending: Vec<PendingAncestor>,
    /// How many of the leading steps the S0213 check after `.` has already
    /// passed, so a long path isn't checked again at every `.`.
    checked: usize,
}

impl Proc {
    fn value(t: Transformed) -> Self {
        Proc {
            steps: vec![PathStep::new(t.node)],
            is_path: false,
            dotted: false,
            group: None,
            pending: t.pending,
            step_pending: Vec::new(),
            checked: 0,
        }
    }

    /// The step a predicate, `[]` or `@` after this node applies to: a
    /// path's last step, or the node itself.
    fn target_step(&mut self) -> &mut PathStep {
        self.steps.last_mut().expect("a processed node has a step")
    }

    /// `#` and `^` on a bare `%` (`%#$i`, `%^(x)`): jsonata-js gives a
    /// parent node no `seekingParent` of its own (only `.` carries its
    /// slot on, and `pushAncestry` gives it to what holds it), so these
    /// leave the `%` unresolved.
    fn forget_bare_parent(&mut self) {
        if self.is_path {
            return;
        }
        if let [step] = self.steps.as_slice() {
            if let Some(label) = parent_step_label(step) {
                let label = label.to_string();
                self.pending.retain(|slot| slot.label != label);
            }
        }
    }

    /// This node as a path, as jsonata-js's `.`, `^` and `#` make one of a
    /// node that isn't: the node becomes its one step. jsonata-js gives the
    /// new path no `seekingParent` of its own (only `.` on a `%` gives it
    /// the `%`'s slot), so the slots the node seeks stay on its step, and
    /// only a `.` while that step is last looks at them (see
    /// `step_pending`). Otherwise they are never resolved, and their `%`
    /// evaluates to nothing: `(objs)[%.%.x]#$j` is undefined, not S0217.
    fn make_path(&mut self) {
        if !self.is_path {
            self.is_path = true;
            let own = parent_step_label(&self.steps[0]).map(str::to_string);
            let (own_slots, step_slots): (Vec<_>, Vec<_>) = std::mem::take(&mut self.pending)
                .into_iter()
                .filter(|slot| slot.level > 0)
                .partition(|slot| Some(&slot.label) == own.as_ref());
            self.pending = own_slots;
            self.step_pending = step_slots;
        }
    }
}

/// Whether a step carries anything beyond its node.
fn step_is_plain(step: &PathStep) -> bool {
    step.stages.is_empty()
        && step.focus.is_none()
        && step.index_var.is_none()
        && step.ancestor_label.is_none()
        && !step.is_tuple
        && !step.inner_tuple
}

/// A step as a node of its own: the bare node when the step carries
/// nothing else, or a one-step path that keeps its flags.
fn step_as_node(step: PathStep) -> AstNode {
    if step_is_plain(&step) {
        step.node
    } else {
        AstNode::Path { steps: vec![step] }
    }
}

/// Whether `step` is a node that isn't a path with a group-by on it, which
/// `{` wraps the node in (see `apply_group`).
fn is_group_step(step: &PathStep) -> bool {
    matches!(step.node, AstNode::ObjectTransform { .. })
}

/// The label of a step that is a `%` (jsonata-js's `type: 'parent'`), with
/// any flags on it and under a group-by on it.
fn parent_step_label(step: &PathStep) -> Option<&str> {
    let node = match &step.node {
        AstNode::ObjectTransform { input, .. } if is_group_step(step) => match input.as_ref() {
            AstNode::Path { steps } if steps.len() == 1 => &steps[0].node,
            other => other,
        },
        other => other,
    };
    match node {
        AstNode::Parent(label) => Some(label),
        _ => None,
    }
}

/// A step's stages split into its filters (jsonata-js's `predicate` on a
/// node that isn't a path) and the rest.
fn split_filters(stages: Vec<Stage>) -> (Vec<Stage>, Vec<Stage>) {
    stages
        .into_iter()
        .partition(|stage| matches!(stage, Stage::Filter(_)))
}

/// Whether evaluating `node` as a step that a `%` sought through can give
/// a tuple stream: its tail is a path with a step that binds something.
/// A tail made only of `%`s or a bare `*` binds nothing (jsonata-js's
/// `seekParent` only counts levels through them).
fn yields_tuples(mut node: &AstNode) -> bool {
    loop {
        node = match node {
            AstNode::Block(exprs) => match exprs.last() {
                Some(last) => last,
                None => return false,
            },
            AstNode::FunctionApplication(inner) => inner,
            AstNode::ObjectTransform { input, .. } => input,
            AstNode::Path { steps } => {
                return steps.iter().any(|step| {
                    step.is_tuple
                        || step.ancestor_label.is_some()
                        || step.focus.is_some()
                        || step.index_var.is_some()
                })
            }
            _ => return false,
        };
    }
}

/// jsonata-js's `#` turns a node's `predicate` into its `stages`, which
/// `evaluateTupleStep` applies after the node is evaluated, group-by and
/// all. On a node with a group-by that moves its filters from before the
/// group to after it: `(objs)[false]{"a": 1}` is `{"a": 1}`, but
/// `(objs)[false]{"a": 1}#$j` is undefined. (`.` does the same to its right
/// side, but a group-by binds looser than `.`, so that side never has one.)
fn lift_group_filters(step: &mut PathStep) {
    let AstNode::ObjectTransform { input, .. } = &mut step.node else {
        return;
    };
    let AstNode::Path { steps } = input.as_mut() else {
        return;
    };
    let [inner] = steps.as_mut_slice() else {
        return;
    };
    let (filters, rest) = split_filters(std::mem::take(&mut inner.stages));
    inner.stages = rest;
    if step_is_plain(inner) {
        let inner = steps.pop().expect("one step");
        **input = inner.node;
    }
    let mut stages = filters;
    stages.append(&mut step.stages);
    step.stages = stages;
    // A step with a new node is a new step (see `PathStep::id`).
    step.id = StepId::default();
}

/// The node a processed operand of a path operator evaluates to, for any
/// place that uses it as a value (an operand, an argument, the whole
/// expression...). A path is a `Path` (a lone sort step is the bare
/// `Sort`), under its group-by. Any other node is settled as jsonata-js
/// evaluates a node by itself (#213): `@$v` binds nothing there, and a `[]`
/// before a `#` keeps nothing.
fn lower_value(proc: Proc) -> Transformed {
    let Proc {
        mut steps,
        is_path,
        dotted,
        group,
        pending,
        step_pending: _,
        checked: _,
    } = proc;
    // A path `#` made of another node has no `keepSingletonArray`, so a
    // `[]` on the node before the `#` keeps nothing (#213).
    if is_path && !dotted {
        if let [step] = steps.as_mut_slice() {
            finish_value_step(step);
        }
    }
    let node = if is_path {
        let lone_sort = matches!(steps.as_slice(), [step]
            if step_is_plain(step) && matches!(step.node, AstNode::Sort { .. }));
        if lone_sort {
            steps.pop().expect("one step").node
        } else {
            AstNode::Path { steps }
        }
    } else {
        let mut step = steps.pop().expect("one step");
        finish_value_step(&mut step);
        if step_is_plain(&step) {
            step.node
        } else {
            AstNode::Path { steps: vec![step] }
        }
    };
    let node = match group {
        Some(pattern) => AstNode::ObjectTransform {
            input: Box::new(node),
            pattern,
        },
        None => node,
    };
    Transformed { node, pending }
}

/// Settle the step of a node that isn't a path, once nothing around it can
/// make it one (#213). Without a `#`, jsonata-js evaluates the node by
/// itself, which never applies an `@$v` binding. With one, `#` wraps the
/// node in a path that has no `keepSingletonArray`, so a `[]` before the
/// `#` (the node's own `keepArray`) keeps nothing. A group-by's operand is
/// settled the same way.
fn finish_value_step(step: &mut PathStep) {
    if let AstNode::ObjectTransform { input, .. } = &mut step.node {
        if let AstNode::Path { steps } = &mut **input {
            if let [inner] = steps.as_mut_slice() {
                finish_value_step(inner);
            }
        }
    }
    if step
        .stages
        .iter()
        .any(|stage| matches!(stage, Stage::Index(_)))
    {
        let mut before_index = true;
        step.stages.retain(|stage| {
            before_index &= !matches!(stage, Stage::Index(_));
            !(before_index && matches!(stage, Stage::KeepArray))
        });
    } else if step.index_var.is_none() {
        step.focus = None;
        step.is_tuple = step.ancestor_label.is_some() || step.inner_tuple;
    }
}

/// One path operator still to apply to the node under it, outermost last.
enum PathOp {
    Map(AstNode),
    Filter(AstNode),
    KeepArray,
    Group(Vec<(AstNode, AstNode)>),
    Sort(Vec<(AstNode, bool)>),
    Focus(String),
    Index(String),
}

impl PathOp {
    /// Hand an operator that won't be applied to the iterative teardown.
    fn drop_iteratively(self) {
        match self {
            PathOp::Map(node) | PathOp::Filter(node) => drop_ast_node_iteratively(node),
            PathOp::Group(pairs) => {
                drop_ast_node_iteratively(AstNode::Object(pairs));
            }
            PathOp::Sort(terms) => drop_ast_node_iteratively(AstNode::Array(
                terms.into_iter().map(|(expr, _)| expr).collect(),
            )),
            PathOp::KeepArray | PathOp::Focus(_) | PathOp::Index(_) => {}
        }
    }
}

/// Whether `node` is a raw path operator that `process_path` handles.
fn is_path_operator(node: &AstNode) -> bool {
    matches!(
        node,
        AstNode::Binary {
            op: BinaryOp::Map
                | BinaryOp::Filter
                | BinaryOp::KeepArray
                | BinaryOp::FocusBind
                | BinaryOp::IndexBind,
            ..
        } | AstNode::ObjectTransform { .. }
            | AstNode::Sort { .. }
            | AstNode::Path { .. }
    )
}

/// jsonata-js's `processAST` for a node that the path operators `.`, `[`,
/// `{`, `^`, `@` and `#` build on. The chain of operators is unwound
/// without recursion (a long path is a deep left-nested chain), its base is
/// processed, and the operators are applied from the innermost out, each
/// after its operand, so their errors come out in jsonata-js's order.
fn process_path(
    node: AstNode,
    state: &mut AncestryState,
    depth: usize,
) -> Result<Proc, AstTransformError> {
    let mut ops = Vec::new();
    let mut node = node;
    let base = loop {
        node = match node {
            AstNode::Binary { op, lhs, rhs } => {
                let rhs = *rhs;
                ops.push(match op {
                    BinaryOp::Map => PathOp::Map(rhs),
                    BinaryOp::Filter => PathOp::Filter(rhs),
                    BinaryOp::KeepArray => PathOp::KeepArray,
                    BinaryOp::FocusBind | BinaryOp::IndexBind => {
                        // The parser only builds a binding with a variable
                        // (S0214); a tree from elsewhere may not.
                        let AstNode::Variable(var) = rhs else {
                            drop_ast_node_iteratively(rhs);
                            drop_ast_node_iteratively(*lhs);
                            ops.into_iter().for_each(PathOp::drop_iteratively);
                            return Err(AstTransformError::Syntax(
                                "S0214: The right side of a binding must be a variable name"
                                    .to_string(),
                            ));
                        };
                        if op == BinaryOp::FocusBind {
                            PathOp::Focus(var)
                        } else {
                            PathOp::Index(var)
                        }
                    }
                    other => {
                        let node = AstNode::Binary {
                            op: other,
                            lhs,
                            rhs: Box::new(rhs),
                        };
                        break node;
                    }
                });
                *lhs
            }
            AstNode::ObjectTransform { input, pattern } => {
                ops.push(PathOp::Group(pattern));
                *input
            }
            AstNode::Sort { input, terms } => {
                ops.push(PathOp::Sort(terms));
                *input
            }
            other => break other,
        };
    };
    let mut proc = match process_base(base, state, depth) {
        Ok(proc) => proc,
        Err(e) => {
            ops.into_iter().for_each(PathOp::drop_iteratively);
            return Err(e);
        }
    };
    // A sort, and a group-by on a node that isn't a path, wrap what came
    // before them in new nodes, so a chain of them builds a deep tree
    // without any recursion here. Count that nesting against the depth
    // limit, as the passes after this one recurse through it.
    let mut nesting = 0;
    while let Some(op) = ops.pop() {
        let wraps = match &op {
            PathOp::Sort(_) => true,
            PathOp::Group(_) => !proc.is_path,
            _ => false,
        };
        if wraps {
            nesting += 2;
            if let Err(e) = check_transform_depth(depth + nesting) {
                op.drop_iteratively();
                ops.into_iter().for_each(PathOp::drop_iteratively);
                drop_proc_iteratively(proc);
                return Err(e);
            }
        }
        if let Err(e) = apply_path_op(&mut proc, op, state, depth + nesting) {
            ops.into_iter().for_each(PathOp::drop_iteratively);
            drop_proc_iteratively(proc);
            return Err(e);
        }
    }
    Ok(proc)
}

/// The node at the bottom of a chain of path operators. A name is a path
/// (the parser makes it a one-step `Path`, as a quoted field that is
/// called after `.` is); any other node is processed as a value.
fn process_base(
    node: AstNode,
    state: &mut AncestryState,
    depth: usize,
) -> Result<Proc, AstTransformError> {
    match node {
        // The parser's one-step path for a name.
        AstNode::Path { steps }
            if matches!(steps.as_slice(), [step]
                if step_is_plain(step) && matches!(step.node, AstNode::Name(_))) =>
        {
            Ok(Proc {
                steps,
                is_path: true,
                dotted: true,
                group: None,
                pending: Vec::new(),
                step_pending: Vec::new(),
                checked: 0,
            })
        }
        // Any other path was built by something else: this pass takes the
        // parser's raw tree, where every other path is still made of path
        // operators.
        AstNode::Path { steps } => {
            drop_path_steps_iteratively(steps);
            Err(AstTransformError::Syntax(
                "A path must come from the parser's raw tree".to_string(),
            ))
        }
        // `process_path` unwinds every other path operator.
        other => Ok(Proc::value(transform_node(other, state, depth + 1)?)),
    }
}

/// Hand a processed node whose processing failed to the iterative
/// teardown.
fn drop_proc_iteratively(proc: Proc) {
    drop_path_steps_iteratively(proc.steps);
    if let Some(pairs) = proc.group {
        drop_ast_node_iteratively(AstNode::Object(pairs));
    }
}

/// Apply one path operator to `proc`. On an error, `proc` may be left
/// part-built, for the caller to tear down.
fn apply_path_op(
    proc: &mut Proc,
    op: PathOp,
    state: &mut AncestryState,
    depth: usize,
) -> Result<(), AstTransformError> {
    match op {
        PathOp::Map(rhs) => apply_map(proc, rhs, state, depth),
        PathOp::Filter(predicate) => apply_filter(proc, predicate, state, depth),
        PathOp::KeepArray => {
            proc.target_step().stages.push(Stage::KeepArray);
            Ok(())
        }
        PathOp::Group(pattern) => apply_group(proc, pattern, state, depth),
        PathOp::Sort(terms) => apply_sort(proc, terms, state, depth),
        PathOp::Focus(var) => {
            let proc_is_path = proc.is_path;
            let step = proc.target_step();
            check_focus_bind_target(&step.stages, &step.node)?;
            // A group-by that `@` binds applies to its operand evaluated by
            // itself, where an `@` binding on the operand binds nothing.
            if !proc_is_path && is_group_step(step) {
                let focus = step.focus.take();
                finish_value_step(step);
                step.focus = focus;
            }
            step.focus = Some(var);
            step.is_tuple = true;
            Ok(())
        }
        PathOp::Index(var) => {
            proc.forget_bare_parent();
            if !proc.is_path {
                lift_group_filters(proc.target_step());
            }
            proc.make_path();
            let step = proc.target_step();
            // An index binding on a step that already has stages
            // (`x[0]#$i`, `books#$ib[...]#$ib2`) is an ordered index stage,
            // numbering the positions after those stages. Otherwise it is
            // the step's index, replacing an earlier one (`a#$i#$j`), as
            // jsonata-js's `#` sets `step.index`. A `[]` before it counts as
            // a stage here: the index stage after it is how the evaluator
            // keeps the `[]` (see `finish_value_step`).
            if !step.stages.is_empty() {
                step.stages.push(Stage::Index(var));
            } else {
                step.index_var = Some(var);
            }
            step.is_tuple = true;
            Ok(())
        }
    }
}

/// Walk each slot back through `steps`, adding the ones that run off its
/// front to `pending`.
fn seek_back(
    steps: &mut [PathStep],
    slots: Vec<PendingAncestor>,
    pending: &mut Vec<PendingAncestor>,
    state: &mut AncestryState,
    depth: usize,
) -> Result<(), AstTransformError> {
    for slot in slots {
        let remaining = walk_backward(steps, &slot.label, slot.level, state, depth + 1)?;
        if remaining > 0 {
            pending.push(PendingAncestor {
                label: slot.label,
                level: remaining,
            });
        }
    }
    Ok(())
}

/// jsonata-js's `.` (`processAST`, binary `.`): the left side as a path,
/// with the right side's steps, or the right side as one step, after it.
/// Every step that is a literal is then S0213, and a string step becomes a
/// name; the `%` slots the new steps seek are resolved against the steps
/// before them.
fn apply_map(
    path: &mut Proc,
    rhs: AstNode,
    state: &mut AncestryState,
    depth: usize,
) -> Result<(), AstTransformError> {
    path.make_path();
    // The left side's last step is last no more.
    path.step_pending.clear();
    path.dotted = true;
    // The right side's operators are this path's steps: no deeper than
    // the left side's, so they cost no extra depth. (A `.` never has
    // another `.` directly on its right, so this can't recurse unchecked.)
    let rest = if is_path_operator(&rhs) {
        process_path(rhs, state, depth)?
    } else {
        Proc::value(transform_node(rhs, state, depth + 1)?)
    };
    let first_new = path.steps.len();
    // jsonata-js's `resolveAncestry` seeks the slots the new last step
    // seeks: a path's last step's, or the node's.
    let new_slots = if rest.is_path {
        path.steps.extend(rest.steps);
        let mut slots = rest.pending;
        slots.extend(rest.step_pending);
        slots
    } else {
        let mut step = rest.steps.into_iter().next().expect("one step");
        step.node = match step.node {
            // A `.(...)` step is a block applied to each context item, and
            // a call is made once per item: both are `FunctionApplication`
            // steps. `.[...]` builds an array per item.
            node @ (AstNode::Block(_) | AstNode::Call { .. } | AstNode::Function { .. }) => {
                AstNode::FunctionApplication(Box::new(node))
            }
            AstNode::Array(items) => AstNode::ArrayGroup(items),
            other => other,
        };
        path.steps.push(step);
        rest.pending
    };
    // jsonata-js's `resolveAncestry` after `.` seeks the last step's own
    // slot when it is a `%`, even one that `#` or `^` forgot
    // (`a.%#$i`, see `Proc::forget_bare_parent`).
    let mut new_slots = new_slots;
    if let Some(label) = path.steps.last().and_then(parent_step_label) {
        if !new_slots.iter().any(|slot| slot.label == label) {
            new_slots.push(PendingAncestor {
                label: label.to_string(),
                level: 1,
            });
        }
    }
    let checked = path.checked;
    check_literal_steps(&mut path.steps[checked..])?;
    path.checked = path.steps.len();
    seek_back(
        &mut path.steps[..first_new],
        new_slots,
        &mut path.pending,
        state,
        depth,
    )
}

/// jsonata-js's S0213 check after `.`, over every step of the path: a step
/// that is a number (a negated one too, which jsonata-js folds into the
/// number) or `true`, `false` or `null` can't be a step. A string step
/// becomes a name. A group-by or sort on a step is looked through, as the
/// step is the node they apply to in jsonata-js.
fn check_literal_steps(steps: &mut [PathStep]) -> Result<(), AstTransformError> {
    for step in steps.iter_mut() {
        if let Some(literal) = check_literal_step(&mut step.node) {
            return Err(AstTransformError::Syntax(format!(
                "S0213: The literal value {} cannot be used as a step within a path expression",
                literal
            )));
        }
    }
    Ok(())
}

fn check_literal_step(node: &mut AstNode) -> Option<String> {
    match node {
        AstNode::Sort { input, .. } | AstNode::ObjectTransform { input, .. } => {
            check_literal_step(input)
        }
        AstNode::Path { steps } => steps
            .iter_mut()
            .find_map(|step| check_literal_step(&mut step.node)),
        AstNode::String(field_name) => {
            *node = AstNode::Name(std::mem::take(field_name));
            None
        }
        _ => literal_step_value(node),
    }
}

/// The value of `node` when it is a literal jsonata-js rejects as a path
/// step (S0213).
fn literal_step_value(node: &AstNode) -> Option<String> {
    match node {
        AstNode::Boolean(b) => Some(b.to_string()),
        AstNode::Null => Some("null".to_string()),
        _ => literal_number(node).map(|n| n.to_string()),
    }
}

/// The operand of a unary minus as a number literal. jsonata-js folds the
/// minus into the number node, which keeps its predicates and `[]`, so a
/// number with them (`-1[0]`, here a one-step path) is still a literal.
fn negated_literal_number(node: &AstNode) -> Option<f64> {
    match node {
        AstNode::Path { steps } => match steps.as_slice() {
            [step] if step.focus.is_none() && step.index_var.is_none() => {
                negated_literal_number(&step.node)
            }
            _ => None,
        },
        other => literal_number(other),
    }
}

/// A number literal, possibly under any number of unary minuses (`--2`).
fn literal_number(node: &AstNode) -> Option<f64> {
    match node {
        AstNode::Number(n) => Some(*n),
        AstNode::Unary {
            op: crate::ast::UnaryOp::Negate,
            operand,
        } => negated_literal_number(operand).map(|n| -n),
        _ => None,
    }
}

/// jsonata-js's `[` (`processAST`, binary `[`): a predicate on a path's
/// last step, or on the node itself. S0209 when that step has a group-by.
/// A `%` slot of the predicate one level up resolves against the step
/// itself; one further up skips it. On a path, what remains stays on the
/// step (`step_pending`), never seeking the steps before it, for a `.`
/// after the path; a node that isn't a path keeps every slot, resolved or
/// not (#211).
fn apply_filter(
    proc: &mut Proc,
    predicate: AstNode,
    state: &mut AncestryState,
    depth: usize,
) -> Result<(), AstTransformError> {
    if is_group_step(proc.target_step()) {
        drop_ast_node_iteratively(predicate);
        return Err(AstTransformError::Syntax(
            "S0209: A predicate cannot follow a grouping expression in a step".to_string(),
        ));
    }
    let mut predicate = transform_node(predicate, state, depth + 1)?;
    // A `%` predicate, with its own predicates, `[]` or `@` or not, is
    // still a parent node in jsonata-js, which has no `seekingParent` of its
    // own (only `pushAncestry` gives a parent's slot to what holds it), so
    // `[` never seeks it, and it is left unresolved (`x[%]`, `x[%[0]]`).
    let parent_label = match &predicate.node {
        AstNode::Parent(label) => Some(label),
        AstNode::Path { steps } => match steps.as_slice() {
            [PathStep {
                node: AstNode::Parent(label),
                index_var: None,
                ..
            }] => Some(label),
            _ => None,
        },
        _ => None,
    };
    if let Some(label) = parent_label.cloned() {
        predicate.pending.retain(|slot| slot.label != label);
    }
    let last = proc.steps.len() - 1;
    let step = &mut proc.steps[last];
    let mut slots = Vec::with_capacity(predicate.pending.len());
    for slot in predicate.pending {
        let level = match slot.level {
            0 => 0,
            1 => seek_parent_step(step, &slot.label, 1, state, depth + 1)?,
            level => level - 1,
        };
        slots.push(PendingAncestor {
            label: slot.label,
            level,
        });
    }
    step.stages.push(Stage::Filter(Box::new(predicate.node)));
    if proc.is_path {
        // jsonata-js's `[` on a path leaves the slots on its last step
        // (`pushAncestry(step, predicate)`) without seeking the steps
        // before it: only a `.` after the step does (`a.b{"k": 1}[%.%.a]`
        // leaves `%.%` unresolved).
        proc.step_pending
            .extend(slots.into_iter().filter(|slot| slot.level > 0));
    } else {
        proc.pending.extend(slots);
    }
    Ok(())
}

/// jsonata-js's `{` as a group-by (`processAST`, binary `{`): S0210 when
/// the node already has one. A path keeps it as its own group, applied
/// after all its steps; a node that isn't a path is wrapped in it, the
/// step's `ObjectTransform`. A `%` slot left in a key or value is dropped:
/// jsonata-js processes the pairs without `pushAncestry` (#192).
fn apply_group(
    proc: &mut Proc,
    pattern: Vec<(AstNode, AstNode)>,
    state: &mut AncestryState,
    depth: usize,
) -> Result<(), AstTransformError> {
    let has_group = if proc.is_path {
        proc.group.is_some()
    } else {
        is_group_step(proc.target_step())
    };
    if has_group {
        drop_ast_node_iteratively(AstNode::Object(pattern));
        return Err(AstTransformError::Syntax(
            "S0210: Each step can only have one grouping expression".to_string(),
        ));
    }
    let mut pairs = Vec::with_capacity(pattern.len());
    let mut pattern = pattern.into_iter();
    while let Some((k, v)) = pattern.next() {
        let k_t = match transform_node(k, state, depth + 1) {
            Ok(t) => t,
            Err(e) => {
                drop_ast_node_iteratively(v);
                drop_ast_node_iteratively(AstNode::Object(pattern.collect()));
                return Err(e);
            }
        };
        let v_t = match transform_node(v, state, depth + 1) {
            Ok(t) => t,
            Err(e) => {
                drop_ast_node_iteratively(AstNode::Object(pattern.collect()));
                return Err(e);
            }
        };
        pairs.push((k_t.node, v_t.node));
    }
    if proc.is_path {
        proc.group = Some(pairs);
    } else {
        // jsonata-js groups a node that isn't a path inside `evaluate`, after
        // its predicates and before the step binds its focus, index or
        // ancestor (`evaluateTupleStep`) or keeps a singleton array. So the
        // group's input is the node with its filters, and the step around
        // the group keeps the rest (#219).
        let step = proc.target_step();
        let (filters, rest) = split_filters(std::mem::take(&mut step.stages));
        step.stages = rest;
        let node = std::mem::replace(&mut step.node, AstNode::Undefined);
        let mut inner = PathStep::with_stages(node, filters);
        // A block whose tail a `%` resolves through, at a step it tags,
        // gives a tuple stream, which its filters and the group read as
        // tuples. A tail that tags nothing (`(%)`, `(*)`, `(%.%)`) gives
        // plain values, which jsonata-js groups as such.
        if step.inner_tuple && yields_tuples(&inner.node) {
            inner.inner_tuple = true;
            inner.is_tuple = step.is_tuple;
        }
        step.inner_tuple = false;
        step.node = AstNode::ObjectTransform {
            input: Box::new(step_as_node(inner)),
            pattern: pairs,
        };
        // A step with a new node is a new step (see `PathStep::id`).
        step.id = StepId::default();
    }
    Ok(())
}

/// jsonata-js's `^` (`processAST`, binary `^`): a sort step after the
/// node's steps, as a path. Its terms' `%` slots resolve against the steps
/// before it. The steps sorted become the `Sort`'s input, and the sort is
/// one step in their place.
fn apply_sort(
    path: &mut Proc,
    terms: Vec<(AstNode, bool)>,
    state: &mut AncestryState,
    depth: usize,
) -> Result<(), AstTransformError> {
    let was_path = path.is_path;
    path.forget_bare_parent();
    path.make_path();
    // The sort is the path's last step now.
    path.step_pending.clear();
    let mut transformed_terms = Vec::with_capacity(terms.len());
    let mut terms = terms.into_iter();
    while let Some((expr, ascending)) = terms.next() {
        let t = match transform_node(expr, state, depth + 1) {
            Ok(t) => t,
            Err(e) => {
                drop_ast_node_iteratively(AstNode::Array(terms.map(|(e, _)| e).collect()));
                return Err(e);
            }
        };
        transformed_terms.push((t.node, ascending));
        if let Err(e) = seek_back(&mut path.steps, t.pending, &mut path.pending, state, depth) {
            drop_ast_node_iteratively(AstNode::Array(
                transformed_terms
                    .into_iter()
                    .map(|(e, _)| e)
                    .chain(terms.map(|(e, _)| e))
                    .collect(),
            ));
            return Err(e);
        }
    }
    // The sort step is checked for S0213 by looking into what it sorts, so
    // it has passed when every step it sorts has.
    let input_checked = path.checked == path.steps.len();
    let input = if was_path {
        AstNode::Path {
            steps: std::mem::take(&mut path.steps),
        }
    } else {
        step_as_node(path.steps.pop().expect("one step"))
    };
    path.steps = vec![PathStep::new(AstNode::Sort {
        input: Box::new(input),
        terms: transformed_terms,
    })];
    path.checked = usize::from(input_checked);
    // A `[]` on the sort keeps the sorted result an array.
    path.dotted = true;
    Ok(())
}

/// Recursively rebuild `node`, resolving any `%`/`@`/`#` found within.
/// Mirrors jsonata-js's processAST's generic per-node-type dispatch.
///
/// `depth` is the shared cycle-1+cycle-3 counter (see the module doc comment
/// and the constants above `coded`) -- every recursive call anywhere in
/// cycle 1 OR cycle 3 must pass `depth + 1`, never a fresh `0`.
fn transform_node(
    node: AstNode,
    state: &mut AncestryState,
    depth: usize,
) -> Result<Transformed, AstTransformError> {
    if depth > MAX_TRANSFORM_DEPTH {
        // `node` may still hold an enormous unprocessed remainder here --
        // see the doc comment above `push_ast_node_children` for why this
        // can't just be allowed to drop normally.
        drop_ast_node_iteratively(node);
        return Err(max_transform_depth_error());
    }
    stacker::maybe_grow(
        AST_TRANSFORM_RED_ZONE,
        AST_TRANSFORM_GROW_STACK_SIZE,
        || transform_node_impl(node, state, depth),
    )
}

fn transform_node_impl(
    node: AstNode,
    state: &mut AncestryState,
    depth: usize,
) -> Result<Transformed, AstTransformError> {
    let depth = depth + 1;
    match node {
        // A path and the operators that build one: jsonata-js's `processAST`
        // cases for `.`, `[`, `{`, `^`, `@` and `#`.
        node if is_path_operator(&node) => Ok(lower_value(process_path(node, state, depth)?)),
        AstNode::Block(exprs) => {
            let mut pending = Vec::new();
            let mut transformed_exprs = Vec::with_capacity(exprs.len());
            for e in exprs {
                let t = transform_node(e, state, depth)?;
                pending.extend(t.pending);
                transformed_exprs.push(t.node);
            }
            Ok(Transformed {
                node: AstNode::Block(transformed_exprs),
                pending,
            })
        }
        // A bare `%` -- mirrors jsonata-js's `case 'parent'`, which assigns
        // a fresh slot the MOMENT any recursive processAST call first sees a
        // 'parent'-type node (not just at the top of transform_node), i.e.
        // eagerly, before any backward walk starts. The one pending
        // reference this produces starts at level 1 (its own immediately
        // preceding step); `%.%` chains extend the level as the backward
        // walk crosses further `%` steps (see `seek_parent_step`).
        AstNode::Parent(_) => {
            let label = state.fresh_label();
            Ok(Transformed {
                node: AstNode::Parent(label.clone()),
                pending: vec![PendingAncestor { label, level: 1 }],
            })
        }
        // Recurse into every other node's children unchanged (no ancestor
        // resolution needed for nodes that aren't paths/blocks/parent refs).
        other => transform_children(other, state, depth),
    }
}

/// Recurse into a node's child expressions without any path-specific
/// ancestor logic (used for node types that can't themselves be paths),
/// aggregating pending ancestor references from every child -- mirrors
/// jsonata-js's per-case `pushAncestry` calls in processAST.
///
/// Three deliberate asymmetries with the generic "bubble everything" rule,
/// all matching jsonata-js exactly:
/// - `Call`'s `procedure` does NOT bubble (only `args` do) -- jsonata-js's
///   function/partial case never calls `pushAncestry` on `result.procedure`.
/// - `Lambda`'s `body` does NOT bubble at all -- jsonata-js's lambda case
///   has no `pushAncestry` call for the body. A `%` inside a lambda body
///   refers to that lambda's OWN invocation-time ancestry chain (irrelevant
///   at definition/parse time), so it's correctly not resolved here; it
///   simply remains an inert `AstNode::Parent(label)` in the body until the
///   lambda is invoked (matching jsonata-js: `function(){%}` parses fine,
///   with the raw `%` untouched inside the body).
/// - An `ObjectTransform`'s (group-by's) `pattern` does NOT bubble -- the
///   `{` group case processes its pairs without `pushAncestry`, so a `%`
///   there stays unresolved and evaluates to undefined (#192).
///
/// `depth` follows the same shared cycle-1+cycle-3 convention as
/// `transform_node` (see its doc comment) -- `transform_children` is a full
/// participant of cycle 1, so it checks depth itself rather than relying
/// solely on `transform_node`'s check.
fn transform_children(
    node: AstNode,
    state: &mut AncestryState,
    depth: usize,
) -> Result<Transformed, AstTransformError> {
    if depth > MAX_TRANSFORM_DEPTH {
        // See transform_node's identical check: `node` may still hold an
        // enormous unprocessed remainder here.
        drop_ast_node_iteratively(node);
        return Err(max_transform_depth_error());
    }
    stacker::maybe_grow(
        AST_TRANSFORM_RED_ZONE,
        AST_TRANSFORM_GROW_STACK_SIZE,
        || transform_children_impl(node, state, depth),
    )
}

fn transform_children_impl(
    node: AstNode,
    state: &mut AncestryState,
    depth: usize,
) -> Result<Transformed, AstTransformError> {
    let depth = depth + 1;
    match node {
        AstNode::Binary { op, lhs, rhs } => {
            let lhs_t = transform_node(*lhs, state, depth)?;
            let rhs_t = transform_node(*rhs, state, depth)?;
            // jsonata-js's `~>` case never calls `pushAncestry`, so a `%`
            // still seeking on either side is dropped rather than raised as
            // S0217: `%.id ~> $count` is 0.
            let mut pending = Vec::new();
            if op != BinaryOp::ChainPipe {
                pending = lhs_t.pending;
                pending.extend(rhs_t.pending);
            }
            Ok(Transformed {
                node: AstNode::Binary {
                    op,
                    lhs: Box::new(lhs_t.node),
                    rhs: Box::new(rhs_t.node),
                },
                pending,
            })
        }
        AstNode::Unary { op, operand } => {
            let t = transform_node(*operand, state, depth)?;
            Ok(Transformed {
                node: AstNode::Unary {
                    op,
                    operand: Box::new(t.node),
                },
                pending: t.pending,
            })
        }
        AstNode::Array(elements) => {
            let mut pending = Vec::new();
            let mut transformed = Vec::with_capacity(elements.len());
            for e in elements {
                let t = transform_node(e, state, depth)?;
                pending.extend(t.pending);
                transformed.push(t.node);
            }
            Ok(Transformed {
                node: AstNode::Array(transformed),
                pending,
            })
        }
        AstNode::Function {
            name,
            args,
            is_builtin,
        } => {
            let mut pending = Vec::new();
            let mut transformed = Vec::with_capacity(args.len());
            for a in args {
                let t = transform_node(a, state, depth)?;
                pending.extend(t.pending);
                transformed.push(t.node);
            }
            Ok(Transformed {
                node: AstNode::Function {
                    name,
                    args: transformed,
                    is_builtin,
                },
                pending,
            })
        }
        AstNode::Call { procedure, args } => {
            // Only args bubble (see doc comment above) -- procedure is
            // still structurally transformed, just doesn't contribute to
            // this Call's own pending.
            // jsonata-js processes the arguments before the procedure, so
            // their errors come first. Calling a value that isn't a function
            // is T1006 when the call runs, as in jsonata-js.
            let mut pending = Vec::new();
            let mut transformed_args = Vec::with_capacity(args.len());
            let mut args = args.into_iter();
            while let Some(a) = args.next() {
                match transform_node(a, state, depth) {
                    Ok(t) => {
                        pending.extend(t.pending);
                        transformed_args.push(t.node);
                    }
                    Err(e) => {
                        drop_ast_node_iteratively(*procedure);
                        drop_ast_node_iteratively(AstNode::Array(args.collect()));
                        return Err(e);
                    }
                }
            }
            let procedure_t = transform_node(*procedure, state, depth)?;
            Ok(Transformed {
                node: AstNode::Call {
                    procedure: Box::new(procedure_t.node),
                    args: transformed_args,
                },
                pending,
            })
        }
        AstNode::Lambda {
            params,
            body,
            signature,
            thunk,
        } => {
            // body's pending is deliberately dropped -- see doc comment above.
            let body_t = transform_node(*body, state, depth)?;
            Ok(Transformed::leaf(AstNode::Lambda {
                params,
                body: Box::new(body_t.node),
                signature,
                thunk,
            }))
        }
        AstNode::Object(pairs) => {
            let mut pending = Vec::new();
            let mut transformed = Vec::with_capacity(pairs.len());
            for (k, v) in pairs {
                let k_t = transform_node(k, state, depth)?;
                pending.extend(k_t.pending);
                let v_t = transform_node(v, state, depth)?;
                pending.extend(v_t.pending);
                transformed.push((k_t.node, v_t.node));
            }
            Ok(Transformed {
                node: AstNode::Object(transformed),
                pending,
            })
        }
        AstNode::Conditional {
            condition,
            then_branch,
            else_branch,
        } => {
            let condition_t = transform_node(*condition, state, depth)?;
            let then_t = transform_node(*then_branch, state, depth)?;
            let mut pending = condition_t.pending;
            pending.extend(then_t.pending);
            let else_t = match else_branch {
                Some(e) => Some(transform_node(*e, state, depth)?),
                None => None,
            };
            let else_node = else_t.map(|t| {
                pending.extend(t.pending);
                Box::new(t.node)
            });
            Ok(Transformed {
                node: AstNode::Conditional {
                    condition: Box::new(condition_t.node),
                    then_branch: Box::new(then_t.node),
                    else_branch: else_node,
                },
                pending,
            })
        }
        // `transform_node` hands every path operator to `process_path`.
        AstNode::Sort { .. } | AstNode::ObjectTransform { .. } | AstNode::Path { .. } => {
            unreachable!("transform_node processes path operators")
        }
        AstNode::Transform {
            location,
            update,
            delete,
        } => {
            let location_t = transform_node(*location, state, depth)?;
            let update_t = transform_node(*update, state, depth)?;
            let mut pending = location_t.pending;
            pending.extend(update_t.pending);
            let delete_t = match delete {
                Some(d) => Some(transform_node(*d, state, depth)?),
                None => None,
            };
            let delete_node = delete_t.map(|t| {
                pending.extend(t.pending);
                Box::new(t.node)
            });
            Ok(Transformed {
                node: AstNode::Transform {
                    location: Box::new(location_t.node),
                    update: Box::new(update_t.node),
                    delete: delete_node,
                },
                pending,
            })
        }
        AstNode::FunctionApplication(inner) => {
            let t = transform_node(*inner, state, depth)?;
            Ok(Transformed {
                node: AstNode::FunctionApplication(Box::new(t.node)),
                pending: t.pending,
            })
        }
        AstNode::ArrayGroup(elements) => {
            let mut pending = Vec::new();
            let mut transformed = Vec::with_capacity(elements.len());
            for e in elements {
                let t = transform_node(e, state, depth)?;
                pending.extend(t.pending);
                transformed.push(t.node);
            }
            Ok(Transformed {
                node: AstNode::ArrayGroup(transformed),
                pending,
            })
        }
        // Leaf nodes and everything else pass through unchanged.
        other => Ok(Transformed::leaf(other)),
    }
}

/// Walk backward through `steps` (from its last element) trying to resolve
/// a single pending ancestor reference at `level`. Returns the remaining
/// level: 0 means fully resolved (some step in `steps` was tagged); >0 means
/// `steps` ran out before the reference resolved, so the caller must keep
/// walking further back through whatever contains `steps` (or, if there is
/// nothing further back, treat it as still-pending / bubble it up).
///
/// `depth` is the SAME shared cycle-1+cycle-3 counter used by
/// `transform_node`/`transform_children`/`process_path` (see the
/// module doc comment) -- `walk_backward` is reached FROM live cycle-1
/// frames, so it must never reset to a fresh counter here.
fn walk_backward(
    steps: &mut [PathStep],
    label: &str,
    level: usize,
    state: &mut AncestryState,
    depth: usize,
) -> Result<usize, AstTransformError> {
    check_transform_depth(depth)?;
    stacker::maybe_grow(
        AST_TRANSFORM_RED_ZONE,
        AST_TRANSFORM_GROW_STACK_SIZE,
        || walk_backward_impl(steps, label, level, state, depth),
    )
}

fn walk_backward_impl(
    steps: &mut [PathStep],
    label: &str,
    mut level: usize,
    state: &mut AncestryState,
    depth: usize,
) -> Result<usize, AstTransformError> {
    // Computed once, not per-iteration: the `while` loop below is bounded
    // iteration (each seek_parent_step call fully returns before the next
    // iteration starts, so their stack usage never overlaps) -- only the
    // *nesting* into seek_parent_step/seek_parent_wrapped's own recursion
    // adds a real stack frame, which is what `depth + 1` accounts for.
    let depth = depth + 1;
    let mut index = steps.len();
    while level > 0 {
        if index == 0 {
            return Ok(level);
        }
        index -= 1;
        // Skip over a run of contiguous focus-bound (`@$var`) steps,
        // treating them as a SINGLE ancestor hop -- mirrors jsonata-js
        // resolveAncestry (parser.js ~L1023-1025): `while(index >= 0 &&
        // step.focus && path.steps[index].focus) { step = path.steps[index--] }`.
        // So in `library.loans@$L.books@$B[...].customers@$C[...].{ $keys(%.%) }`
        // all three focus steps collapse into one hop and `%.%` reaches the root.
        while index > 0 && steps[index].focus.is_some() && steps[index - 1].focus.is_some() {
            index -= 1;
        }
        level = seek_parent_step(&mut steps[index], label, level, state, depth)?;
    }
    Ok(0)
}

/// Try to resolve one level of a pending ancestor reference against a
/// single candidate step. Returns the remaining level (0 = tagged here).
/// Mirrors jsonata-js's seekParent (parser.js ~L941-986).
///
/// `depth` is the same shared cycle-1+cycle-3 counter (see the module doc
/// comment).
fn seek_parent_step(
    step: &mut PathStep,
    label: &str,
    level: usize,
    state: &mut AncestryState,
    depth: usize,
) -> Result<usize, AstTransformError> {
    check_transform_depth(depth)?;
    stacker::maybe_grow(
        AST_TRANSFORM_RED_ZONE,
        AST_TRANSFORM_GROW_STACK_SIZE,
        || seek_parent_step_impl(step, label, level, state, depth),
    )
}

/// One level of seekParent's `name`/`wildcard` case: the level drops, and at
/// zero `step` is tagged as the ancestor the label binds to. Returns the
/// remaining level.
fn tag_ancestor_step(
    step: &mut PathStep,
    label: &str,
    level: usize,
    state: &mut AncestryState,
) -> usize {
    let remaining = level - 1;
    if remaining == 0 {
        match &step.ancestor_label {
            // Reuse: an earlier `%` already tagged this exact step.
            // Record the alias instead of overwriting (see
            // AncestryState's doc comment).
            Some(existing) => {
                state.aliases.insert(label.to_string(), existing.clone());
            }
            None => {
                step.ancestor_label = Some(label.to_string());
            }
        }
        step.is_tuple = true;
    }
    remaining
}

fn seek_parent_step_impl(
    step: &mut PathStep,
    label: &str,
    level: usize,
    state: &mut AncestryState,
    depth: usize,
) -> Result<usize, AstTransformError> {
    let depth = depth + 1;
    if is_group_step(step) && parent_step_label(step).is_some() {
        return Ok(level + 1);
    }
    match &mut step.node {
        AstNode::Name(_) | AstNode::Wildcard => Ok(tag_ancestor_step(step, label, level, state)),
        // Chained %.%: this step is itself another (already independently
        // resolved-or-pending) `%` -- extend the level and keep walking
        // further back, exactly mirroring seekParent's `case 'parent':
        // slot.level++` (which notably does NOT set `.tuple` here).
        AstNode::Parent(_) => Ok(level + 1),
        // Parenthesized sub-path as a path step (e.g. `Account.(Order.Product).%`
        // parses `(Order.Product)` as `FunctionApplication(Block([Path{...}]))`) --
        // mirrors seekParent's 'block'/'path' cases layered together: this
        // outer step becomes tuple-producing regardless of where inside the
        // parens the actual ancestor tag lands, and we recurse inward to
        // find it.
        AstNode::FunctionApplication(inner) => {
            step.is_tuple = true;
            step.inner_tuple = true;
            seek_parent_wrapped(inner.as_mut(), label, level, state, depth)
        }
        // A parenthesized block reached directly as a path step (e.g. a
        // leading `(Account.Order)` with no `.` before it, or a multi-
        // statement `(...)`) -- mirrors seekParent's 'block' case: recurse
        // into the LAST expression.
        AstNode::Block(exprs) => match exprs.last_mut() {
            Some(last) => {
                step.is_tuple = true;
                step.inner_tuple = true;
                seek_parent_wrapped(last, label, level, state, depth)
            }
            // An empty block `()` produces no ancestor and no tuple; the walk
            // simply steps over it with the level unchanged (mirrors jsonata-js
            // seekParent's `if(node.expressions.length > 0)` guard, which leaves
            // the slot untouched for an empty block). Lets `Account.Order.().%`
            // resolve `%` against `Order` rather than raising S0217.
            None => Ok(level),
        },
        // A group-by on a bare `*` or `%` as a step (`*{...}.%`, with
        // predicates too: `*[x]{...}.%`): jsonata-js puts the group on that
        // node, so seekParent sees a wildcard, tagged here as one (the step
        // binds the ancestor after the group, as for any node), or a
        // parent, which adds a level (#209). A string literal heading a path
        // is a name step there, group and all (`"s"{k: v}.%`), so it is
        // tagged as a name is.
        AstNode::ObjectTransform { input, .. }
            if matches!(**input, AstNode::Wildcard | AstNode::Name(_))
                || matches!(&**input, AstNode::Path { steps }
                    if steps.len() == 1
                        && matches!(steps[0].node, AstNode::Name(_) | AstNode::Wildcard)) =>
        {
            Ok(tag_ancestor_step(step, label, level, state))
        }
        // A group-by reached as a step (`(a.b{...}).%`): seek through its
        // input, as seek_parent_wrapped does.
        AstNode::ObjectTransform { .. } => {
            step.is_tuple = true;
            step.inner_tuple = true;
            seek_parent_wrapped(&mut step.node, label, level, state, depth)
        }
        _ => Err(coded("S0217", S0217_MESSAGE)),
    }
}

/// Recurse into a "wrapped" target (a `FunctionApplication`'s inner node,
/// for a `.(...)` step a `Block`, or a `Block`'s last expression) that must
/// itself resolve to
/// a nested `Path` for us to walk backward through it -- mirrors how
/// jsonata-js's block/path seekParent cases can be layered on top of each
/// other for doubly-nested parens (e.g. `Account.(Order.(Product)).%`).
/// Anything else (a literal, a function call, ...) can't derive an
/// ancestor: S0217.
///
/// `depth` is the same shared cycle-1+cycle-3 counter (see the module doc
/// comment).
fn seek_parent_wrapped(
    node: &mut AstNode,
    label: &str,
    level: usize,
    state: &mut AncestryState,
    depth: usize,
) -> Result<usize, AstTransformError> {
    check_transform_depth(depth)?;
    stacker::maybe_grow(
        AST_TRANSFORM_RED_ZONE,
        AST_TRANSFORM_GROW_STACK_SIZE,
        || seek_parent_wrapped_impl(node, label, level, state, depth),
    )
}

fn seek_parent_wrapped_impl(
    node: &mut AstNode,
    label: &str,
    level: usize,
    state: &mut AncestryState,
    depth: usize,
) -> Result<usize, AstTransformError> {
    let depth = depth + 1;
    match node {
        AstNode::Path { steps } => walk_backward(steps, label, level, state, depth),
        // A nested block (e.g. the inner `()` of `.()`, or `(a; b)`): recurse
        // into its last expression, or -- for an empty block -- step over it
        // leaving the level unchanged (jsonata-js seekParent's block guard).
        AstNode::Block(exprs) => match exprs.last_mut() {
            Some(last) => seek_parent_wrapped(last, label, level, state, depth),
            None => Ok(level),
        },
        // A bare `*` or `%` that isn't a path step, such as the tail of
        // `acct.(*).%` or `acct.(*{...}).%`. seekParent counts the level as
        // it does for a step, but evaluating the node binds nothing, so a
        // `%` that resolves at a `*` here is undefined, as in jsonata-js
        // (#209).
        AstNode::Wildcard => Ok(level - 1),
        AstNode::Parent(_) => Ok(level + 1),
        // A group-by. jsonata-js keeps the group on the path node, so
        // seekParent walks the grouped path's steps; here that path is the
        // group-by's input.
        AstNode::ObjectTransform { input, .. } => {
            seek_parent_wrapped(input.as_mut(), label, level, state, depth)
        }
        _ => Err(coded("S0217", S0217_MESSAGE)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Task 6: `%` inside filter predicates and sort terms ---
    //
    // Mechanism ported from jsonata-js processAST (parser.js). Ground truth
    // for every tag target below was dumped from jsonata-js's own `.ast()`
    // (via `node -e 'jsonata(expr).ast()'` in tests/jsonata-js).
    //
    // PREDICATE (`case '['`, parser.js ~L1097-1130): each slot the predicate
    // is still seeking is examined -- a level-1 slot resolves against the
    // STEP the predicate is attached to (`seekParent(step, slot)`, which tags
    // that step, or bumps the level if the step is itself a `%`); a level>N>1
    // slot is decremented and then resolved by walking backward through the
    // steps BEFORE the attached step. In our flat-path model this is: for a
    // predicate slot on step i, level==1 -> seek_parent_step(resolved[i]);
    // level>1 -> walk_backward(resolved[..i], level-1).
    //
    // SORT (`case '^'`, parser.js ~L1151-1170): jsonata appends a synthetic
    // `sort` step to the input path, bubbles every term's own seeking slots
    // onto it, then runs resolveAncestry -- which walks backward starting at
    // the step BEFORE the sort step, i.e. the LAST real input step. So a
    // level-1 sort-term slot resolves against the last input step (no
    // predicate-style "attach to the step itself" special case is needed;
    // it's a uniform backward walk over the input steps).

    // Helper: locate the ancestor_label a resolved path assigns to a given
    // step index, panicking with context if the shape is wrong.
    fn resolve_path(expr: &str) -> Vec<PathStep> {
        let ast = crate::parser::Parser::new(expr.to_string())
            .unwrap()
            .parse_raw()
            .unwrap();
        match resolve_ancestry(ast).unwrap() {
            AstNode::Path { steps } => steps,
            other => panic!("expected Path, got {:?}", other),
        }
    }

    #[test]
    fn test_parent_inside_predicate_resolves_against_enclosing_step() {
        // Account.Order.Product[%.OrderID='order104'].SKU
        // Ground truth (jsonata-js .ast()): the `%` inside the predicate
        // tags the Product step (steps[2]) -- i.e. `%` resolves to Product's
        // own input (Order), and Product itself carries the ancestor label.
        let steps = resolve_path("Account.Order.Product[%.OrderID='order104'].SKU");
        assert_eq!(steps.len(), 4);
        assert!(matches!(steps[2].node, AstNode::Name(ref n) if n == "Product"));
        let product_label = steps[2].ancestor_label.clone();
        assert!(product_label.is_some(), "Product must be tagged");
        assert!(steps[2].is_tuple);
        assert!(
            steps[1].ancestor_label.is_none(),
            "Order must NOT be tagged"
        );
        // The `%` inside the predicate must carry Product's label.
        match &steps[2].stages[0] {
            Stage::Index(_) | Stage::KeepArray => {
                unreachable!("no index or keep-array stage in this test")
            }
            Stage::Filter(expr) => match expr.as_ref() {
                AstNode::Binary { lhs, .. } => match lhs.as_ref() {
                    AstNode::Path { steps: inner } => match &inner[0].node {
                        AstNode::Parent(label) => {
                            assert_eq!(Some(label.clone()), product_label)
                        }
                        other => panic!("expected Parent, got {:?}", other),
                    },
                    other => panic!("expected inner Path, got {:?}", other),
                },
                other => panic!("expected Binary, got {:?}", other),
            },
        }
    }

    #[test]
    fn test_parent_chain_inside_predicate_resolves_two_levels() {
        // Account.Order.Product[%.%.`Account Name`='Firefly'].SKU
        // Ground truth: first `%` tags Product (steps[2]), second `%` tags
        // Order (steps[1]).
        let steps = resolve_path("Account.Order.Product[%.%.`Account Name`='Firefly'].SKU");
        assert_eq!(steps.len(), 4);
        let product_label = steps[2].ancestor_label.clone();
        let order_label = steps[1].ancestor_label.clone();
        assert!(product_label.is_some(), "Product must be tagged");
        assert!(order_label.is_some(), "Order must be tagged");
        assert_ne!(product_label, order_label);
        match &steps[2].stages[0] {
            Stage::Index(_) | Stage::KeepArray => {
                unreachable!("no index or keep-array stage in this test")
            }
            Stage::Filter(expr) => match expr.as_ref() {
                AstNode::Binary { lhs, .. } => match lhs.as_ref() {
                    AstNode::Path { steps: inner } => {
                        // inner = [Parent, Parent, Name("Account Name")]
                        match &inner[0].node {
                            AstNode::Parent(l) => assert_eq!(Some(l.clone()), product_label),
                            other => panic!("expected Parent, got {:?}", other),
                        }
                        match &inner[1].node {
                            AstNode::Parent(l) => assert_eq!(Some(l.clone()), order_label),
                            other => panic!("expected Parent, got {:?}", other),
                        }
                    }
                    other => panic!("expected inner Path, got {:?}", other),
                },
                other => panic!("expected Binary, got {:?}", other),
            },
        }
    }

    #[test]
    fn test_parent_predicate_on_parent_step_itself() {
        // Account.Order.Product.Price.%[%.OrderID='order103'].SKU
        // Ground truth: the trailing `.%` step's own reference tags Price
        // (steps[3]); the predicate's `%` (attached to a `%` step, so bumped
        // one level) tags Product (steps[2]).
        let steps = resolve_path("Account.Order.Product.Price.%[%.OrderID='order103'].SKU");
        // [Account, Order, Product, Price, %(stages), SKU]
        assert_eq!(steps.len(), 6);
        assert!(matches!(steps[4].node, AstNode::Parent(_)));
        let price_label = steps[3].ancestor_label.clone();
        let product_label = steps[2].ancestor_label.clone();
        assert!(
            price_label.is_some(),
            "Price must be tagged (by the % step)"
        );
        assert!(
            product_label.is_some(),
            "Product must be tagged (by the predicate %)"
        );
        assert_ne!(price_label, product_label);
    }

    #[test]
    fn test_two_predicates_share_and_differ_labels() {
        // Account.Order.Product[%.OrderID='order104'][%.%.`Account Name`='Firefly'].SKU
        // Ground truth: first predicate's `%` -> Product; second predicate's
        // first `%` -> Product (REUSE same label); second `%` -> Order.
        let steps = resolve_path(
            "Account.Order.Product[%.OrderID='order104'][%.%.`Account Name`='Firefly'].SKU",
        );
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[2].stages.len(), 2);
        let product_label = steps[2].ancestor_label.clone();
        let order_label = steps[1].ancestor_label.clone();
        assert!(product_label.is_some());
        assert!(order_label.is_some());
        assert_ne!(product_label, order_label);
        // first predicate: %  -> Product
        match &steps[2].stages[0] {
            Stage::Index(_) | Stage::KeepArray => {
                unreachable!("no index or keep-array stage in this test")
            }
            Stage::Filter(expr) => match expr.as_ref() {
                AstNode::Binary { lhs, .. } => match lhs.as_ref() {
                    AstNode::Path { steps: inner } => match &inner[0].node {
                        AstNode::Parent(l) => assert_eq!(Some(l.clone()), product_label),
                        other => panic!("{:?}", other),
                    },
                    other => panic!("{:?}", other),
                },
                other => panic!("{:?}", other),
            },
        }
        // second predicate: %.% -> Product (reuse), Order
        match &steps[2].stages[1] {
            Stage::Index(_) | Stage::KeepArray => {
                unreachable!("no index or keep-array stage in this test")
            }
            Stage::Filter(expr) => match expr.as_ref() {
                AstNode::Binary { lhs, .. } => match lhs.as_ref() {
                    AstNode::Path { steps: inner } => {
                        match &inner[0].node {
                            AstNode::Parent(l) => assert_eq!(Some(l.clone()), product_label),
                            other => panic!("{:?}", other),
                        }
                        match &inner[1].node {
                            AstNode::Parent(l) => assert_eq!(Some(l.clone()), order_label),
                            other => panic!("{:?}", other),
                        }
                    }
                    other => panic!("{:?}", other),
                },
                other => panic!("{:?}", other),
            },
        }
    }

    #[test]
    fn test_parent_inside_sort_term_resolves_to_last_input_step() {
        // Account.Order.Product.SKU^(%.Price)
        // Ground truth: the sort term's `%` tags SKU (the last input step).
        let ast = crate::parser::Parser::new("Account.Order.Product.SKU^(%.Price)".to_string())
            .unwrap()
            .parse_raw()
            .unwrap();
        match resolve_ancestry(ast).unwrap() {
            AstNode::Sort { input, terms } => {
                let steps = match input.as_ref() {
                    AstNode::Path { steps } => steps,
                    other => panic!("expected Path input, got {:?}", other),
                };
                assert_eq!(steps.len(), 4);
                assert!(matches!(steps[3].node, AstNode::Name(ref n) if n == "SKU"));
                let sku_label = steps[3].ancestor_label.clone();
                assert!(sku_label.is_some(), "SKU must be tagged");
                // term = (Path[Parent, Name("Price")], asc)
                match &terms[0].0 {
                    AstNode::Path { steps: inner } => match &inner[0].node {
                        AstNode::Parent(l) => assert_eq!(Some(l.clone()), sku_label),
                        other => panic!("{:?}", other),
                    },
                    other => panic!("{:?}", other),
                }
            }
            other => panic!("expected Sort, got {:?}", other),
        }
    }

    #[test]
    fn test_two_sort_terms_share_and_differ_labels() {
        // Account.Order.Product.SKU^(%.Price, >%.%.OrderID)
        // Ground truth: term1 `%` -> SKU; term2 `%.%` -> SKU (reuse), Product.
        let ast = crate::parser::Parser::new(
            "Account.Order.Product.SKU^(%.Price, >%.%.OrderID)".to_string(),
        )
        .unwrap()
        .parse_raw()
        .unwrap();
        match resolve_ancestry(ast).unwrap() {
            AstNode::Sort { input, terms } => {
                let steps = match input.as_ref() {
                    AstNode::Path { steps } => steps,
                    other => panic!("{:?}", other),
                };
                let sku_label = steps[3].ancestor_label.clone();
                let product_label = steps[2].ancestor_label.clone();
                assert!(sku_label.is_some());
                assert!(product_label.is_some());
                assert_ne!(sku_label, product_label);
                assert_eq!(terms.len(), 2);
                // term2 = %.%.OrderID
                match &terms[1].0 {
                    AstNode::Path { steps: inner } => {
                        match &inner[0].node {
                            AstNode::Parent(l) => assert_eq!(Some(l.clone()), sku_label),
                            other => panic!("{:?}", other),
                        }
                        match &inner[1].node {
                            AstNode::Parent(l) => assert_eq!(Some(l.clone()), product_label),
                            other => panic!("{:?}", other),
                        }
                    }
                    other => panic!("{:?}", other),
                }
            }
            other => panic!("expected Sort, got {:?}", other),
        }
    }

    #[test]
    fn test_focus_bind_becomes_step_flag() {
        // Order@$o  -->  Path{steps: [Name("Order") with focus=Some("o"), is_tuple=true]}
        let ast = AstNode::Binary {
            op: BinaryOp::FocusBind,
            lhs: Box::new(AstNode::Path {
                steps: vec![PathStep::new(AstNode::Name("Order".to_string()))],
            }),
            rhs: Box::new(AstNode::Variable("o".to_string())),
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
        let ast = AstNode::Binary {
            op: BinaryOp::IndexBind,
            lhs: Box::new(AstNode::Path {
                steps: vec![PathStep::new(AstNode::Name("arr".to_string()))],
            }),
            rhs: Box::new(AstNode::Variable("i".to_string())),
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
        let err = resolve_ancestry(AstNode::Parent(String::new())).unwrap_err();
        assert!(err.to_string().starts_with("S0217"));
    }

    #[test]
    fn test_path_step_with_stages_preserved() {
        let ast = crate::parser::Parser::new("Order[true]".to_string())
            .unwrap()
            .parse_raw()
            .unwrap();
        match resolve_ancestry(ast).unwrap() {
            AstNode::Path { steps } => {
                assert_eq!(steps[0].stages.len(), 1);
            }
            other => panic!("expected Path, got {:?}", other),
        }
    }

    #[test]
    fn test_path_not_from_the_parser_is_rejected() {
        let ast = AstNode::Path {
            steps: vec![PathStep::with_stages(
                AstNode::Name("Order".to_string()),
                vec![Stage::Filter(Box::new(AstNode::Boolean(true)))],
            )],
        };
        assert!(resolve_ancestry(ast).is_err());
        assert!(resolve_ancestry(AstNode::Path { steps: vec![] }).is_err());
    }

    // --- Regression tests using the REAL parser (Task 3 review findings) ---
    //
    // Hand-built synthetic ASTs only exercise the shapes that happen to
    // already work. These tests go through `crate::parser::parse()` on real
    // source text, which is what surfaced two root-cause bugs in Task 3:
    // (1) transform_children not recursing into most composite node types,
    // and (2) `@$var`/`#$var` never being migrated when the marker is the
    // TOP-LEVEL node reaching transform_node (only when already nested
    // inside a PathStep). The same discipline applies to Task 4's `%`
    // resolution below: expected label/level assertions are checked for
    // internal consistency (same target step -> same label; different
    // targets -> different labels) rather than against jsonata-js's exact
    // "!0"/"!1"/... strings, since those are implementation-internal and
    // arbitrary -- but the STEPS that get tagged are cross-checked against
    // jsonata-js's actual `.ast()` output (see comments below).

    #[test]
    fn test_real_parser_bare_focus_bind_no_dot() {
        // "Order@$o" -- bare single-step, no dot anywhere. The parser
        // produces Binary{FocusBind, lhs: Name("Order"), rhs: Variable("o")}
        // at the top level (no Path at all, since there's no `.`).
        let ast = crate::parser::Parser::new("Order@$o".to_string())
            .unwrap()
            .parse_raw()
            .unwrap();
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
        let ast = crate::parser::Parser::new("Account.Order@$o".to_string())
            .unwrap()
            .parse_raw()
            .unwrap();
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
        let ast = crate::parser::Parser::new("arr#$i".to_string())
            .unwrap()
            .parse_raw()
            .unwrap();
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
        let ast = crate::parser::Parser::new("$count(%)".to_string())
            .unwrap()
            .parse_raw()
            .unwrap();
        let err = resolve_ancestry(ast).unwrap_err();
        assert!(err.to_string().starts_with("S0217"));
    }

    // --- Regression tests for the "nested Path from multi-step @/# marker"
    // finding (Task 3, second review round) ---

    #[test]
    fn test_real_parser_focus_bind_multistep_prefix_and_suffix_is_flat() {
        // "Account.Order@$o.Product" must produce a FLAT 3-step path, not a
        // 2-step path whose first step's node is itself a nested 2-step Path.
        let ast = crate::parser::Parser::new("Account.Order@$o.Product".to_string())
            .unwrap()
            .parse_raw()
            .unwrap();
        let result = resolve_ancestry(ast).unwrap();
        match result {
            AstNode::Path { steps } => {
                assert_eq!(steps.len(), 3, "expected a flat 3-step path");
                assert!(matches!(steps[0].node, AstNode::Name(ref n) if n == "Account"));
                assert!(steps[0].focus.is_none());
                assert!(!steps[0].is_tuple);
                assert!(matches!(steps[1].node, AstNode::Name(ref n) if n == "Order"));
                assert_eq!(steps[1].focus, Some("o".to_string()));
                assert!(steps[1].is_tuple);
                assert!(matches!(steps[2].node, AstNode::Name(ref n) if n == "Product"));
                assert!(steps[2].focus.is_none());
            }
            other => panic!("expected flat Path, got {:?}", other),
        }
    }

    #[test]
    fn test_real_parser_index_bind_multistep_prefix_and_suffix_is_flat() {
        // Same shape as above but for `#$i` (IndexBind) instead of `@$o`.
        let ast = crate::parser::Parser::new("Account.Order#$i.Product".to_string())
            .unwrap()
            .parse_raw()
            .unwrap();
        let result = resolve_ancestry(ast).unwrap();
        match result {
            AstNode::Path { steps } => {
                assert_eq!(steps.len(), 3, "expected a flat 3-step path");
                assert!(matches!(steps[0].node, AstNode::Name(ref n) if n == "Account"));
                assert!(steps[0].index_var.is_none());
                assert!(!steps[0].is_tuple);
                assert!(matches!(steps[1].node, AstNode::Name(ref n) if n == "Order"));
                assert_eq!(steps[1].index_var, Some("i".to_string()));
                assert!(steps[1].is_tuple);
                assert!(matches!(steps[2].node, AstNode::Name(ref n) if n == "Product"));
                assert!(steps[2].index_var.is_none());
            }
            other => panic!("expected flat Path, got {:?}", other),
        }
    }

    // --- Task 4: `%`/`%.%` ancestor resolution, real-parser-based ---
    //
    // Ground truth for every test below was independently verified against
    // jsonata-js's OWN `.ast()` output (`node -e 'jsonata(expr).ast()'` in
    // tests/jsonata-js), not derived by hand. This is what caught the task
    // brief's off-by-one (it asserted the wrong target steps for a `%.%`
    // chain) before any code was written against it.

    #[test]
    fn test_real_parser_single_level_parent_resolves_to_previous_step() {
        // "Account.Order.%" -- jsonata-js tags `Order` (steps[1]), and the
        // trailing `%` step (steps[2]) carries the matching label. `%`
        // refers to Order's own INPUT (i.e. what produced it, Account) --
        // confirmed by live evaluation: Account.Order.% evaluates to the
        // Account object, not the Order object.
        let ast = crate::parser::Parser::new("Account.Order.%".to_string())
            .unwrap()
            .parse_raw()
            .unwrap();
        let result = resolve_ancestry(ast).unwrap();
        match result {
            AstNode::Path { steps } => {
                assert_eq!(steps.len(), 3);
                assert!(matches!(steps[0].node, AstNode::Name(ref n) if n == "Account"));
                assert!(steps[0].ancestor_label.is_none());
                assert!(matches!(steps[1].node, AstNode::Name(ref n) if n == "Order"));
                assert!(steps[1].ancestor_label.is_some());
                assert!(steps[1].is_tuple);
                match &steps[2].node {
                    AstNode::Parent(label) => {
                        assert_eq!(Some(label.clone()), steps[1].ancestor_label);
                    }
                    other => panic!("expected Parent(label), got {:?}", other),
                }
            }
            other => panic!("expected Path, got {:?}", other),
        }
    }

    #[test]
    fn test_real_parser_chained_parent_resolves_two_levels_back() {
        // "Account.Order.Product.%.%" (mirrors parent002.jsonata's shape).
        // Ground truth from jsonata-js: the FIRST `%` tags Product
        // (steps[2]), the SECOND `%` tags Order (steps[1]) -- NOT Order and
        // Account as a naive reading might suggest. Each `%` targets the
        // step whose INPUT it refers to: the first `%`'s target is Product
        // (whose input is Order), the second `%` walks one step further
        // back to Order (whose input is Account).
        let ast = crate::parser::Parser::new("Account.Order.Product.%.%".to_string())
            .unwrap()
            .parse_raw()
            .unwrap();
        let result = resolve_ancestry(ast).unwrap();
        match result {
            AstNode::Path { steps } => {
                assert_eq!(steps.len(), 5);
                assert!(steps[0].ancestor_label.is_none(), "Account untagged");
                let order_label = steps[1].ancestor_label.clone();
                let product_label = steps[2].ancestor_label.clone();
                assert!(order_label.is_some(), "Order must be tagged");
                assert!(product_label.is_some(), "Product must be tagged");
                assert_ne!(
                    order_label, product_label,
                    "two distinct % chains must get distinct labels"
                );
                match &steps[3].node {
                    AstNode::Parent(label) => assert_eq!(Some(label.clone()), product_label),
                    other => panic!("expected Parent(label), got {:?}", other),
                }
                match &steps[4].node {
                    AstNode::Parent(label) => assert_eq!(Some(label.clone()), order_label),
                    other => panic!("expected Parent(label), got {:?}", other),
                }
            }
            other => panic!("expected Path, got {:?}", other),
        }
    }

    #[test]
    fn test_real_parser_object_constructor_percent_tags_preceding_step() {
        // parent000.jsonata's shape: "Account.Order.Product.{'order': %.OrderID}"
        // -- % lives INSIDE the object constructor's value, not as its own
        // trailing path step. Ground truth (jsonata-js): Product (steps[2])
        // gets tagged, and the nested %'s label matches.
        let ast =
            crate::parser::Parser::new("Account.Order.Product.{'order': %.OrderID}".to_string())
                .unwrap()
                .parse_raw()
                .unwrap();
        let result = resolve_ancestry(ast).unwrap();
        match result {
            AstNode::Path { steps } => {
                assert_eq!(steps.len(), 4);
                assert!(matches!(steps[2].node, AstNode::Name(ref n) if n == "Product"));
                let product_label = steps[2].ancestor_label.clone();
                assert!(product_label.is_some());
                assert!(steps[2].is_tuple);
                match &steps[3].node {
                    AstNode::Object(pairs) => {
                        assert_eq!(pairs.len(), 1);
                        match &pairs[0].1 {
                            AstNode::Path { steps: inner } => {
                                assert_eq!(inner.len(), 2);
                                match &inner[0].node {
                                    AstNode::Parent(label) => {
                                        assert_eq!(Some(label.clone()), product_label)
                                    }
                                    other => panic!("expected Parent(label), got {:?}", other),
                                }
                            }
                            other => panic!("expected inner Path, got {:?}", other),
                        }
                    }
                    other => panic!("expected Object, got {:?}", other),
                }
            }
            other => panic!("expected Path, got {:?}", other),
        }
    }

    #[test]
    fn test_real_parser_object_constructor_two_percent_chains_share_and_differ() {
        // parent002.jsonata's actual shape:
        // "Account.Order.Product.{'Product':`Product Name`,'Order':%.OrderID,'Account':%.%.`Account Name`}"
        // Ground truth (jsonata-js, verified via live .ast() dump): the
        // 'Order' value's single `%` and the 'Account' value's FIRST `%`
        // (of its `%.%` chain) both resolve to Product -- i.e. they share
        // ONE label (the "reuse an existing label" mechanic) -- while the
        // 'Account' value's SECOND `%` resolves to Order, getting a
        // DIFFERENT label.
        let ast = crate::parser::Parser::new(
            "Account.Order.Product.{'Product':`Product Name`,'Order':%.OrderID,'Account':%.%.`Account Name`}"
                .to_string(),
        )
        .unwrap()
        .parse_raw()
        .unwrap();
        let result = resolve_ancestry(ast).unwrap();
        match result {
            AstNode::Path { steps } => {
                assert_eq!(steps.len(), 4);
                let product_label = steps[2].ancestor_label.clone();
                let order_label = steps[1].ancestor_label.clone();
                assert!(product_label.is_some(), "Product must be tagged");
                assert!(order_label.is_some(), "Order must be tagged");
                assert_ne!(product_label, order_label);

                match &steps[3].node {
                    AstNode::Object(pairs) => {
                        assert_eq!(pairs.len(), 3);
                        // pairs[1] = 'Order': %.OrderID
                        match &pairs[1].1 {
                            AstNode::Path { steps: inner } => match &inner[0].node {
                                AstNode::Parent(label) => {
                                    assert_eq!(Some(label.clone()), product_label)
                                }
                                other => panic!("expected Parent(label), got {:?}", other),
                            },
                            other => panic!("expected inner Path, got {:?}", other),
                        }
                        // pairs[2] = 'Account': %.%.`Account Name`
                        match &pairs[2].1 {
                            AstNode::Path { steps: inner } => {
                                assert_eq!(inner.len(), 3);
                                match &inner[0].node {
                                    AstNode::Parent(label) => {
                                        // Reuse: same label as the 'Order'
                                        // value's % (both target Product).
                                        assert_eq!(Some(label.clone()), product_label)
                                    }
                                    other => panic!("expected Parent(label), got {:?}", other),
                                }
                                match &inner[1].node {
                                    AstNode::Parent(label) => {
                                        assert_eq!(Some(label.clone()), order_label)
                                    }
                                    other => panic!("expected Parent(label), got {:?}", other),
                                }
                            }
                            other => panic!("expected inner Path, got {:?}", other),
                        }
                    }
                    other => panic!("expected Object, got {:?}", other),
                }
            }
            other => panic!("expected Path, got {:?}", other),
        }
    }

    /// The one expression of a `.(...)` step's Block.
    fn paren_body(node: &AstNode) -> &AstNode {
        match node {
            AstNode::Block(exprs) if exprs.len() == 1 => &exprs[0],
            other => panic!("expected a one-expression Block, got {:?}", other),
        }
    }

    #[test]
    fn test_real_parser_percent_through_parenthesized_step_function_application() {
        // parent001.jsonata's shape: "Account.(Order.Product).%" -- parens
        // around a multi-step sub-path parse as a FunctionApplication step
        // wrapping a Block of a nested Path. `%` must walk INTO that nested path to
        // find Product (its last step) as the target, exactly as if the
        // parens weren't there.
        let ast = crate::parser::Parser::new("Account.(Order.Product).%".to_string())
            .unwrap()
            .parse_raw()
            .unwrap();
        let result = resolve_ancestry(ast).unwrap();
        match result {
            AstNode::Path { steps } => {
                assert_eq!(steps.len(), 3);
                assert!(steps[1].is_tuple, "the wrapping step must be flagged tuple");
                match &steps[1].node {
                    AstNode::FunctionApplication(inner) => match paren_body(inner) {
                        AstNode::Path { steps: inner_steps } => {
                            assert_eq!(inner_steps.len(), 2);
                            assert!(
                                matches!(inner_steps[0].node, AstNode::Name(ref n) if n == "Order")
                            );
                            assert!(
                                matches!(inner_steps[1].node, AstNode::Name(ref n) if n == "Product")
                            );
                            let product_label = inner_steps[1].ancestor_label.clone();
                            assert!(product_label.is_some(), "Product must be tagged");
                            match &steps[2].node {
                                AstNode::Parent(label) => {
                                    assert_eq!(Some(label.clone()), product_label)
                                }
                                other => panic!("expected Parent(label), got {:?}", other),
                            }
                        }
                        other => panic!("expected inner Path, got {:?}", other),
                    },
                    other => panic!("expected FunctionApplication, got {:?}", other),
                }
            }
            other => panic!("expected Path, got {:?}", other),
        }
    }

    #[test]
    fn test_real_parser_percent_through_leading_paren_block() {
        // parent006.jsonata's shape: "(Account.Order).(Product).{...}" -- a
        // LEADING bare paren (not preceded by `.`) parses as a generic
        // `Block` (not FunctionApplication), then becomes the first step of
        // the outer path via the normal-dot fallback; `.(Product)` becomes a
        // second, `FunctionApplication`-wrapped step.
        //
        // Ground truth from jsonata-js (verified via live `.ast()` dump,
        // NOT hand-derived -- an earlier draft of this test wrongly assumed
        // % must walk past the `(Product)` step into the `(Account.Order)`
        // step to find Order; jsonata-js instead resolves it in ONE level,
        // same as the un-parenthesized `Account.Order.Product.%` case):
        // `%` (level 1) resolves entirely WITHIN the immediately preceding
        // step -- the `(Product)` FunctionApplication -- tagging Product
        // itself. The `(Account.Order)` block is never even visited.
        let ast =
            crate::parser::Parser::new("(Account.Order).(Product).{'x': %.OrderID}".to_string())
                .unwrap()
                .parse_raw()
                .unwrap();
        let result = resolve_ancestry(ast).unwrap();
        match result {
            AstNode::Path { steps } => {
                assert_eq!(steps.len(), 3);
                // steps[0]: the untouched (Account.Order) block.
                match &steps[0].node {
                    AstNode::Block(exprs) => {
                        assert_eq!(exprs.len(), 1);
                        match &exprs[0] {
                            AstNode::Path { steps: inner } => {
                                assert_eq!(inner.len(), 2);
                                assert!(
                                    matches!(inner[0].node, AstNode::Name(ref n) if n == "Account")
                                );
                                assert!(
                                    matches!(inner[1].node, AstNode::Name(ref n) if n == "Order")
                                );
                                assert!(
                                    inner[1].ancestor_label.is_none(),
                                    "Order must NOT be tagged -- % resolves one level back, at Product"
                                );
                            }
                            other => panic!("expected inner Path, got {:?}", other),
                        }
                    }
                    other => panic!("expected Block, got {:?}", other),
                }
                assert!(!steps[0].is_tuple);
                // steps[1]: the (Product) FunctionApplication -- this is
                // where % actually resolves.
                assert!(steps[1].is_tuple, "the wrapping step must be flagged tuple");
                match &steps[1].node {
                    AstNode::FunctionApplication(inner) => match paren_body(inner) {
                        AstNode::Path { steps: inner_steps } => {
                            assert_eq!(inner_steps.len(), 1);
                            assert!(
                                matches!(inner_steps[0].node, AstNode::Name(ref n) if n == "Product")
                            );
                            let product_label = inner_steps[0].ancestor_label.clone();
                            assert!(product_label.is_some(), "Product must be tagged");
                            match &steps[2].node {
                                AstNode::Object(pairs) => match &pairs[0].1 {
                                    AstNode::Path { steps: value_steps } => {
                                        match &value_steps[0].node {
                                            AstNode::Parent(label) => {
                                                assert_eq!(Some(label.clone()), product_label)
                                            }
                                            other => {
                                                panic!("expected Parent(label), got {:?}", other)
                                            }
                                        }
                                    }
                                    other => panic!("expected value Path, got {:?}", other),
                                },
                                other => panic!("expected Object, got {:?}", other),
                            }
                        }
                        other => panic!("expected inner Path, got {:?}", other),
                    },
                    other => panic!("expected FunctionApplication, got {:?}", other),
                }
            }
            other => panic!("expected Path, got {:?}", other),
        }
    }

    #[test]
    fn test_real_parser_percent_cannot_derive_ancestor_from_literal() {
        // A `%` immediately after a step that isn't name/wildcard/block/path
        // (here, a string literal step is folded to a Name by the parser's
        // own S0213-adjacent handling, so use a case that stays non-
        // resolvable: % with nothing at all before it in an enclosing path).
        let ast = crate::parser::Parser::new("%.OrderID".to_string())
            .unwrap()
            .parse_raw()
            .unwrap();
        let err = resolve_ancestry(ast).unwrap_err();
        assert!(err.to_string().starts_with("S0217"));
    }

    #[test]
    fn test_real_parser_percent_in_group_by_is_left_unresolved() {
        // jsonata-js never resolves a `%` in a group-by's key or value
        // (#192): the input steps get no ancestor tag and nothing is raised.
        for expr in ["a.b{%.id: c}", "a.b{c: %.id}", "a.b{c: %.%.x}"] {
            let ast = crate::parser::Parser::new(expr.to_string())
                .unwrap()
                .parse_raw()
                .unwrap();
            match resolve_ancestry(ast).unwrap() {
                AstNode::ObjectTransform { input, .. } => match *input {
                    AstNode::Path { steps } => {
                        assert!(steps.iter().all(|s| s.ancestor_label.is_none()), "{expr}");
                    }
                    other => panic!("expected Path, got {:?}", other),
                },
                other => panic!("expected ObjectTransform, got {:?}", other),
            }
        }
    }

    #[test]
    fn test_s0217_uses_jsonata_js_wording() {
        // The top-level check, a seek that reaches a non-path step, and a
        // seek into a non-path expression.
        for expr in ["%", "[1].%", "a.b{c: (1).%}"] {
            let ast = crate::parser::Parser::new(expr.to_string())
                .unwrap()
                .parse_raw()
                .unwrap();
            let err = resolve_ancestry(ast).unwrap_err();
            assert_eq!(err.to_string(), format!("S0217: {S0217_MESSAGE}"), "{expr}");
        }
    }

    #[test]
    fn test_real_parser_percent_seeks_through_group_by_input() {
        // `(a.b{...}).%`: jsonata-js's seekParent walks the grouped path's
        // steps, so `b` is tagged rather than raising S0217.
        let ast = crate::parser::Parser::new("(a.b{c: d}).%".to_string())
            .unwrap()
            .parse_raw()
            .unwrap();
        let steps = match resolve_ancestry(ast).unwrap() {
            AstNode::Path { steps } => steps,
            other => panic!("expected Path, got {:?}", other),
        };
        assert!(steps[0].is_tuple && steps[0].inner_tuple);
        let AstNode::Block(exprs) = &steps[0].node else {
            panic!("expected Block, got {:?}", steps[0].node);
        };
        let AstNode::ObjectTransform { input, .. } = &exprs[0] else {
            panic!("expected ObjectTransform, got {:?}", exprs[0]);
        };
        let AstNode::Path { steps: inner } = input.as_ref() else {
            panic!("expected Path, got {:?}", input);
        };
        assert!(inner[0].ancestor_label.is_none());
        assert!(inner[1].ancestor_label.is_some() && inner[1].is_tuple);
    }

    #[test]
    fn test_real_parser_lambda_body_percent_does_not_bubble_or_error() {
        // "function(){ % }" -- jsonata-js parses this successfully (the raw
        // `%` is left untouched inside the lambda body, only failing at
        // runtime when/if the lambda is invoked). Confirms Lambda bodies
        // don't bubble their pending to the enclosing scope.
        let ast = crate::parser::Parser::new("function(){ % }".to_string())
            .unwrap()
            .parse_raw()
            .unwrap();
        let result = resolve_ancestry(ast).unwrap();
        match result {
            AstNode::Lambda { body, .. } => {
                assert!(matches!(*body, AstNode::Parent(_)));
            }
            other => panic!("expected Lambda, got {:?}", other),
        }
    }

    // --- S0215/S0216: `@` (focus binding) rejects a step that already has
    // predicates or is a sort step. These checks were a pre-existing gap
    // (never implemented, not even by Task 3) that only surfaced once
    // ast_transform started running unconditionally via parser::parse():
    // previously an unresolved `@`/`#` reaching the evaluator always threw
    // (for the WRONG reason -- "must be resolved by ast_transform pass"),
    // and the reference-suite harness lenient-accepts any error without an
    // extractable code, masking the missing S0215/S0216 checks entirely.
    // Ground truth: tests/jsonata-js/test/test-suite/groups/joins/errors.json.

    #[test]
    fn test_real_parser_focus_bind_after_predicate_is_s0215() {
        let ast = crate::parser::Parser::new("Account.Order[1]@$o.Product".to_string())
            .unwrap()
            .parse_raw()
            .unwrap();
        let err = resolve_ancestry(ast).unwrap_err();
        assert!(err.to_string().starts_with("S0215"), "got: {}", err);
    }

    #[test]
    fn test_real_parser_focus_bind_after_sort_is_s0216() {
        let ast = crate::parser::Parser::new(
            "Account.Order^(>OrderID)@$o.Product.{ 'name':`Product Name`, 'orderid':$o.OrderID }"
                .to_string(),
        )
        .unwrap()
        .parse_raw()
        .unwrap();
        let err = resolve_ancestry(ast).unwrap_err();
        assert!(err.to_string().starts_with("S0216"), "got: {}", err);
    }

    #[test]
    fn test_real_parser_index_bind_after_predicate_is_not_an_error() {
        // Unlike `@`, `#` has NO S0215-equivalent restriction in jsonata-js
        // -- it's allowed after predicates (it just appends an index stage
        // rather than setting a plain `index` field when stages already
        // exist). Confirms `check_focus_bind_target`'s marker-kind guard
        // correctly only fires for Focus, not Index.
        let ast = crate::parser::Parser::new("Account.Order[1]#$o.Product".to_string())
            .unwrap()
            .parse_raw()
            .unwrap();
        assert!(resolve_ancestry(ast).is_ok());
    }
}

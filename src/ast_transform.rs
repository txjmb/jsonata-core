// Post-parse AST transformation pass.
// Mirrors parser.js's processAST/seekParent/pushAncestry/resolveAncestry
// (tests/jsonata-js/src/parser.js ~L937-1235), adapted to Rust's ownership
// model: instead of mutating tree nodes in place, this consumes the raw
// tree and rebuilds an enriched one with ancestor/tuple metadata resolved.

// Recursion-depth safety (added: see docs/superpowers/plans/2026-07-07-parser-depth-and-u16-truncation-fixes-plan.md):
// `parser::parse()` (src/parser.rs:1730) unconditionally pipes every parse
// through `resolve_ancestry` below, so a deeply-nested input expression can
// overflow the native stack here even though the raw Pratt parse itself
// completed successfully -- confirmed empirically: a 200,000-term
// left-nested arithmetic chain (`1+1+1+...`) SIGABRTs ("stack overflow")
// via the full `parser::parse()` entry point, in this file, not the parser.
//
// There are THREE recursive pieces in this file, but only TWO independent
// stack budgets: data flows one-way from resolve_ancestry into
// transform_node/transform_children/transform_path_steps/
// migrate_binding_markers ("cycle 1"), and separately into substitute_labels
// ("cycle 2", a second full-tree walk that only starts after cycle 1 has
// fully unwound -- see resolve_ancestry). walk_backward/seek_parent_step/
// seek_parent_wrapped ("cycle 3") is reached FROM cycle 1 (transform_path_
// steps's predicate/own-pending resolution, and transform_children's Sort
// arm) while cycle 1's frames are still LIVE on the native stack -- it nests
// ON TOP of cycle 1's depth rather than running after it -- so cycle 1 and
// cycle 3 share one stack budget and their depths ADD, not two independent
// caps. A depth guard that gives cycle 1 and cycle 3 each their own
// independent counter capped at the native-safe limit would still allow
// cap1 + cap3 frames live simultaneously and overflow; Task 2 needs ONE
// counter threaded through cycle 1 AND cycle 3 together, and a SEPARATE
// counter (reset to 0) for cycle 2 (substitute_labels), which only runs
// after cycle 1/3's frames are gone. Guard all of the functions listed
// below regardless of which cycle they're in -- checking depth in only one
// cycle's functions is the exact "Task 5 pattern" (a check added at only
// one of several recursive entry points) this task exists to avoid.
//
// (1) Main tree-transform mutual recursion -- depth scales with the general
//     AST's nesting depth (binary op chains, block/array/function-arg
//     nesting, parenthesized sub-paths used as a path step's node, etc.):
//   - transform_node (:558) -- recurses directly (Path -> transform_path_steps;
//     Block -> transform_node per element, a loop, but each iteration's call
//     itself recurses; Binary{FocusBind/IndexBind} -> transform_node(lhs));
//     for every other node kind, delegates to transform_children (still the
//     same cycle). This is the actual site hit by the confirmed arithmetic-
//     chain repro (`1+1+1+...` has no Path/`%` at all -- it's pure nested
//     Binary, handled by transform_node's `other => transform_children(...)`
//     fallback).
//   - transform_children (:653) -- recurses via transform_node on every
//     child of every composite node type (Binary lhs/rhs, Unary operand,
//     Array/Function/Call-args/Object/ObjectTransform/Sort/Transform/
//     ArrayGroup elements, Conditional branches, Lambda body, Predicate/
//     FunctionApplication inner). This is the other function actually hit
//     by the arithmetic-chain repro (Binary's lhs/rhs recursion).
//   - transform_path_steps (:933) -- does NOT recurse on the flat
//     `Vec<PathStep>` itself (that's a `for` loop over the steps -- bounded
//     iteration, not stack depth; confirmed empirically in Step 3 below: a
//     50,000-step flat dot-path parses fine). It DOES feed back into the
//     cycle per-step: calls `migrate_binding_markers(step, ...)` for every
//     step, and separately calls `transform_node` on each filter-stage
//     expression. Depth here scales with how deeply a single step's OWN
//     node is nested (e.g. a parenthesized sub-path `(Order.Product)` used
//     as one step, itself containing another Path), not with the number of
//     steps in the flat list.
//   - migrate_binding_markers (:1224) -- not itself self-recursive (one
//     match, each arm calls transform_node/splice_marker_steps once), but
//     it's the edge that closes the transform_path_steps -> transform_node
//     cycle, so it needs to participate in whatever depth-counter scheme
//     Task 2 uses (thread it through, even if it never increments/checks
//     independently of the transform_node call it makes).
//
// (2) substitute_labels (:273) -- self-recursive only (never calls
//     transform_node/transform_children/transform_path_steps), structurally
//     mirroring transform_children's per-node-type dispatch (every
//     composite node type recurses into every child). Runs as a SECOND,
//     separate full-tree walk after transform_node returns (see
//     resolve_ancestry), so it needs its own depth counter/reset -- reusing
//     a counter left over (at whatever depth) from pass (1) would be wrong.
//
// (3) Ancestor-seek recursion, reached from pass (1) (transform_path_steps's
//     predicate/own-pending resolution loop calling resolve_predicate_slot/
//     walk_backward, and transform_children's Sort arm calling
//     walk_backward directly) while pass (1)'s own frames are still live --
//     it never calls back into transform_node/transform_children/
//     transform_path_steps (a one-way bridge, not a mutual cycle with (1)),
//     but because it nests ON TOP of (1)'s live stack rather than running
//     after it unwinds, (1) and (3) share ONE stack budget (see the note
//     above the fold -- their depths add). Depth here scales with how many
//     levels of parenthesized sub-path nesting (`(...)` wrapping another
//     `(...)`) a `%` reference has to walk through, not with path step
//     count or general AST depth:
//   - walk_backward (:1056) -- its own "while level > 0" loop walking
//     backward through one `&mut [PathStep]` is bounded iteration (not a
//     stack risk regardless of the slice's length), but it calls
//     seek_parent_step per candidate step, which can call back into
//     walk_backward (via seek_parent_wrapped's Path case) -- indirect
//     recursion.
//   - seek_parent_step (:1121) -- recurses via seek_parent_wrapped for the
//     FunctionApplication and Block step-node cases (a parenthesized
//     sub-path used as a step).
//   - seek_parent_wrapped (:1191) -- recurses via walk_backward (Path case)
//     AND directly calls itself (Block case, recursing into the block's
//     last expression) -- e.g. doubly (or N-ly) nested parens.
//   - resolve_predicate_slot (:1028) -- NOT part of this cycle itself (no
//     self-loop; called once per predicate slot from transform_path_steps's
//     loop over a bounded number of stages), but forwards into it
//     (seek_parent_step / walk_backward), so its own frame sits at the
//     base of chain (3) each time -- no guard needed in this function
//     itself, but Task 2 should not assume the chain "starts" at
//     walk_backward/seek_parent_step without going through here first in
//     the predicate case.
//
// Functions confirmed NOT to need guarding (either non-recursive, or their
// only "recursion" is bounded iteration over a Vec/HashMap-chain, not stack
// depth):
//   - coded (:161), AncestryState::new (:207), AncestryState::fresh_label
//     (:214), Transformed::leaf (:243) -- trivial constructors/helpers, no
//     recursive or child-node-walking calls at all.
//   - AncestryState::canonical (:224) -- a `while let Some(...)` loop
//     following an alias chain in a HashMap; iteration, not recursion, and
//     the doc comment right above it already notes chains longer than one
//     hop shouldn't arise in practice regardless.
//   - apply_marker_to_step (:419), check_focus_bind_target (:454) -- single
//     match/if-chain over already-computed values, no calls back into any
//     tree-walking function.
//   - splice_marker_steps (:486) -- loops over a `Vec<PathStep>` produced by
//     an already-fully-transformed `Transformed` (its `steps`/`pending`
//     inputs were recursed into by the CALLER before this runs), and over a
//     small fixed-shape `while` popping trailing `Predicate` pseudo-steps;
//     calls only check_focus_bind_target/apply_marker_to_step, never
//     transform_node or itself.
//   - wrap_marker_as_path (:545) -- calls splice_marker_steps once; no
//     recursion, no self-loop.
//   - resolve_ancestry (:252) -- the pass's entry point: calls
//     transform_node exactly once, then substitute_labels exactly once.
//     Not itself part of either cycle (never re-entered from within the
//     tree walk it kicks off), so it doesn't need a depth CHECK, but Task 2
//     should initialize/reset each of the three counters above here (one
//     for cycle (1)+(shared edge into (3)), one for substitute_labels).
//
// Step 3 sanity check performed (throwaway test, not committed): a
// 200,000-step flat dot-path (`a.a.a...a`) -- same N as the crashing
// arithmetic chain, for a clean apples-to-apples Ok-vs-crash comparison --
// parsed via the FULL `parser::parse()` entry point returns `Ok`
// immediately (iteration in transform_path_steps's `for step in steps`
// loop, not recursion), while the 200,000-term arithmetic chain (`1+1+1+
// ...`) still SIGABRTs ("stack overflow") via the same full `parser::
// parse()` entry point in the same run -- confirming the root cause
// identified in the prior session is still live in current code, and that
// it's specifically recursion-on-nesting-depth (transform_children's
// Binary arm), not merely "large input," that triggers it.

use crate::ast::{AstNode, BinaryOp, PathStep, Stage};
use std::collections::HashMap;
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
pub fn resolve_ancestry(ast: AstNode) -> Result<AstNode, AstTransformError> {
    let mut state = AncestryState::new();
    let transformed = transform_node(ast, &mut state)?;
    // Mirrors jsonata-js's final check (parser.js ~L1404): a bare `%` as the
    // WHOLE expression, or any dangling (never-resolved) pending ancestor
    // reference that bubbled all the way to the top, means there was no
    // enclosing path to derive an ancestor from.
    if !transformed.pending.is_empty() || matches!(transformed.node, AstNode::Parent(_)) {
        return Err(coded(
            "S0217",
            "The parent operator % cannot be used at this point in the expression",
        ));
    }
    Ok(substitute_labels(transformed.node, &state))
}

/// Final pass: rewrite every `AstNode::Parent(label)` in the tree to its
/// canonical (alias-resolved) label. See `AncestryState` for why this is a
/// separate pass rather than done inline. Mirrors `transform_children`'s
/// traversal shape exactly (every composite node type), since by this point
/// there's no error case left to handle -- the tree is already fully valid.
fn substitute_labels(node: AstNode, state: &AncestryState) -> AstNode {
    match node {
        AstNode::Parent(label) => AstNode::Parent(state.canonical(&label)),
        AstNode::Path { steps } => AstNode::Path {
            steps: steps
                .into_iter()
                .map(|s| PathStep {
                    node: substitute_labels(s.node, state),
                    // Stages (predicates) can contain `%` references whose
                    // labels were aliased during resolution (e.g. a second
                    // predicate reusing a step an earlier one already tagged),
                    // so they must be substituted too -- otherwise the
                    // pre-alias label survives and evaluates against the wrong
                    // tuple key.
                    stages: s
                        .stages
                        .into_iter()
                        .map(|st| match st {
                            Stage::Filter(e) => {
                                Stage::Filter(Box::new(substitute_labels(*e, state)))
                            }
                            Stage::Index(v) => Stage::Index(v),
                        })
                        .collect(),
                    ..s
                })
                .collect(),
        },
        AstNode::Block(exprs) => AstNode::Block(
            exprs
                .into_iter()
                .map(|e| substitute_labels(e, state))
                .collect(),
        ),
        AstNode::Binary { op, lhs, rhs } => AstNode::Binary {
            op,
            lhs: Box::new(substitute_labels(*lhs, state)),
            rhs: Box::new(substitute_labels(*rhs, state)),
        },
        AstNode::Unary { op, operand } => AstNode::Unary {
            op,
            operand: Box::new(substitute_labels(*operand, state)),
        },
        AstNode::Array(elements) => AstNode::Array(
            elements
                .into_iter()
                .map(|e| substitute_labels(e, state))
                .collect(),
        ),
        AstNode::Function {
            name,
            args,
            is_builtin,
        } => AstNode::Function {
            name,
            args: args
                .into_iter()
                .map(|a| substitute_labels(a, state))
                .collect(),
            is_builtin,
        },
        AstNode::Call { procedure, args } => AstNode::Call {
            procedure: Box::new(substitute_labels(*procedure, state)),
            args: args
                .into_iter()
                .map(|a| substitute_labels(a, state))
                .collect(),
        },
        AstNode::Lambda {
            params,
            body,
            signature,
            thunk,
        } => AstNode::Lambda {
            params,
            body: Box::new(substitute_labels(*body, state)),
            signature,
            thunk,
        },
        AstNode::Object(pairs) => AstNode::Object(
            pairs
                .into_iter()
                .map(|(k, v)| (substitute_labels(k, state), substitute_labels(v, state)))
                .collect(),
        ),
        AstNode::ObjectTransform { input, pattern } => AstNode::ObjectTransform {
            input: Box::new(substitute_labels(*input, state)),
            pattern: pattern
                .into_iter()
                .map(|(k, v)| (substitute_labels(k, state), substitute_labels(v, state)))
                .collect(),
        },
        AstNode::Conditional {
            condition,
            then_branch,
            else_branch,
        } => AstNode::Conditional {
            condition: Box::new(substitute_labels(*condition, state)),
            then_branch: Box::new(substitute_labels(*then_branch, state)),
            else_branch: else_branch.map(|e| Box::new(substitute_labels(*e, state))),
        },
        AstNode::Sort { input, terms } => AstNode::Sort {
            input: Box::new(substitute_labels(*input, state)),
            terms: terms
                .into_iter()
                .map(|(e, asc)| (substitute_labels(e, state), asc))
                .collect(),
        },
        AstNode::Transform {
            location,
            update,
            delete,
        } => AstNode::Transform {
            location: Box::new(substitute_labels(*location, state)),
            update: Box::new(substitute_labels(*update, state)),
            delete: delete.map(|d| Box::new(substitute_labels(*d, state))),
        },
        AstNode::FunctionApplication(inner) => {
            AstNode::FunctionApplication(Box::new(substitute_labels(*inner, state)))
        }
        AstNode::ArrayGroup(elements) => AstNode::ArrayGroup(
            elements
                .into_iter()
                .map(|e| substitute_labels(e, state))
                .collect(),
        ),
        AstNode::Predicate(inner) => AstNode::Predicate(Box::new(substitute_labels(*inner, state))),
        // Leaf nodes and everything else pass through unchanged.
        other => other,
    }
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

/// Core shared logic for both call sites that need to migrate a `@$var`/
/// `#$var` marker: given the already-`transform_node`-recursed `lhs`/`input`
/// that the marker was parsed against, produce the flat sequence of
/// `PathStep`s the marker should resolve to, plus whatever pending ancestor
/// references bubbled up from transforming that `lhs`/`input`.
///
/// Mirrors jsonata-js's `processAST` `case '@'`/`case '#'`:
/// `result = processAST(expr.lhs); step = result; if (result.type ===
/// 'path') { step = result.steps[result.steps.length - 1]; }` -- `result`
/// (the possibly-multi-step path) is always what gets kept/spliced in, and
/// only `step` (the thing that gets the marker's flags stamped onto it) is
/// reassigned to the LAST step of that path when `result` is itself a path.
/// Note jsonata-js's `@`/`#` cases do NOT call `pushAncestry` on the lhs --
/// we deviate slightly (forwarding the lhs's pending through as this
/// marker's own pending) since dropping it silently seems more surprising
/// than propagating it, and no test data combines `%` with `@`/`#` closely
/// enough to distinguish the two choices.
///
/// - If `transformed` is a multi-step `Path`, the marker's flags land on its
///   LAST step, and ALL of its steps are returned to be spliced into the
///   caller's flat steps list (never wrapped in a new outer step).
/// - Otherwise (e.g. a bare `Name` with no `.` at all), wrap it into a new
///   single-step `Path` and stamp the marker onto that one step.
///
/// S0215/S0216 validation for `@` (focus binding only -- `#`/index binding
/// has no such restriction in jsonata-js): the target must not already have
/// predicates/stages attached, and must not itself be a `Sort` node.
fn check_focus_bind_target(
    marker: &BindingMarker,
    target_stages: &[crate::ast::Stage],
    target_node: &AstNode,
) -> Result<(), AstTransformError> {
    if !matches!(marker, BindingMarker::Focus(_)) {
        return Ok(());
    }
    if !target_stages.is_empty() {
        return Err(coded(
            "S0215",
            "A context variable binding must precede any predicates on a step",
        ));
    }
    if matches!(target_node, AstNode::Sort { .. }) {
        return Err(coded(
            "S0216",
            "A context variable binding must precede the 'order-by' clause on a step",
        ));
    }
    Ok(())
}
///
/// Fallible because `@` (focus binding) specifically -- not `#` -- rejects
/// being applied to a step that already has predicates/stages (S0215) or
/// that is itself a sort step (S0216), mirroring jsonata-js's `case '@'`
/// checks (parser.js ~L1183-1199): `step = result; if (result.type ===
/// 'path') { step = result.steps[...length-1]; }` -- note `step` can be the
/// bare (non-Path) `result` itself, e.g. `Account.Order^(...)@$o.Product`
/// parses `Account.Order^(...)` into a bare top-level `Sort` node (not
/// wrapped in a Path) *before* `@$o` wraps around it, so the S0216 check
/// must inspect the raw `other` node too, not just a `Path`'s last step.
fn splice_marker_steps(
    transformed: Transformed,
    marker: BindingMarker,
) -> Result<(Vec<PathStep>, Vec<PendingAncestor>), AstTransformError> {
    let Transformed { node, pending } = transformed;
    let steps = match node {
        AstNode::Path { mut steps } => {
            // Our parser encodes `$[[1..4]]` (and any `expr[pred]`) as a separate
            // trailing `Predicate` step rather than a step carrying the predicate
            // as a `stage` (as jsonata-js does). For an index marker, mirror
            // jsonata's `#` case (parser.js ~L1206-1223: when the target step
            // already has stages, PUSH an index stage) by folding those trailing
            // predicate pseudo-steps into the preceding real step's stages, then
            // stamping the index on that step. This makes `$[[1..4]]#$pos[$pos>=2]`
            // apply the `[[1..4]]` filter, then number the survivors, then filter
            // by `$pos` -- rather than crashing on a `Predicate` step node in
            // create_tuple_stream.
            if matches!(marker, BindingMarker::Index(_)) {
                while steps.len() >= 2
                    && matches!(steps.last().map(|s| &s.node), Some(AstNode::Predicate(_)))
                {
                    let pred = steps.pop().unwrap();
                    if let AstNode::Predicate(inner) = pred.node {
                        steps.last_mut().unwrap().stages.push(Stage::Filter(inner));
                    }
                }
            }
            if let Some(last) = steps.last_mut() {
                check_focus_bind_target(&marker, &last.stages, &last.node)?;
                // A SECOND index binding on the same step (e.g. `books#$ib[...]#$ib2`)
                // must not overwrite the first: append it as an ordered index
                // stage so it numbers the post-filter positions (jsonata's `#`
                // case pushing an index stage when the step already has one).
                if let (BindingMarker::Index(var), true) = (&marker, last.index_var.is_some()) {
                    last.stages.push(Stage::Index(var.clone()));
                    last.is_tuple = true;
                } else {
                    apply_marker_to_step(last, marker);
                }
            }
            steps
        }
        other => {
            check_focus_bind_target(&marker, &[], &other)?;
            let mut step = PathStep::new(other);
            apply_marker_to_step(&mut step, marker);
            vec![step]
        }
    };
    Ok((steps, pending))
}

/// Handle a `@$var`/`#$var` marker reaching `transform_node` as the raw node
/// itself (not already nested inside a `PathStep`) -- e.g. `Order@$o` or
/// `Account.Order@$o` where the parser's flat infix loop has already merged
/// any preceding `.` steps into a `Path` (or, for a single bare name, left a
/// non-Path leaf) *before* wrapping the whole thing in the marker node. At
/// this (top-level) call site there's no outer steps list to splice into, so
/// the spliced steps become the whole resulting `Path`.
fn wrap_marker_as_path(
    transformed: Transformed,
    marker: BindingMarker,
) -> Result<Transformed, AstTransformError> {
    let (steps, pending) = splice_marker_steps(transformed, marker)?;
    Ok(Transformed {
        node: AstNode::Path { steps },
        pending,
    })
}

/// Recursively rebuild `node`, resolving any `%`/`@`/`#` found within.
/// Mirrors jsonata-js's processAST's generic per-node-type dispatch.
fn transform_node(
    node: AstNode,
    state: &mut AncestryState,
) -> Result<Transformed, AstTransformError> {
    match node {
        AstNode::Path { steps } => {
            let (transformed_steps, pending) = transform_path_steps(steps, state)?;
            Ok(Transformed {
                node: AstNode::Path {
                    steps: transformed_steps,
                },
                pending,
            })
        }
        AstNode::Block(exprs) => {
            let mut pending = Vec::new();
            let mut transformed_exprs = Vec::with_capacity(exprs.len());
            for e in exprs {
                let t = transform_node(e, state)?;
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
            let transformed_lhs = transform_node(*lhs, state)?;
            wrap_marker_as_path(transformed_lhs, BindingMarker::Focus(var_name))
        }
        // Same story as FocusBind above, but for bare top-level `#$var`
        // (now represented the same generic way as FocusBind -- see
        // BinaryOp::IndexBind's doc comment in ast.rs).
        AstNode::Binary {
            op: BinaryOp::IndexBind,
            lhs,
            rhs,
        } => {
            let var_name = match *rhs {
                AstNode::Variable(name) => name,
                _ => unreachable!("parser guarantees IndexBind's rhs is always Variable"),
            };
            let transformed_lhs = transform_node(*lhs, state)?;
            wrap_marker_as_path(transformed_lhs, BindingMarker::Index(var_name))
        }
        // Recurse into every other node's children unchanged (no ancestor
        // resolution needed for nodes that aren't paths/blocks/parent refs).
        other => transform_children(other, state),
    }
}

/// Recurse into a node's child expressions without any path-specific
/// ancestor logic (used for node types that can't themselves be paths),
/// aggregating pending ancestor references from every child -- mirrors
/// jsonata-js's per-case `pushAncestry` calls in processAST.
///
/// Two deliberate asymmetries with the generic "bubble everything" rule,
/// both matching jsonata-js exactly:
/// - `Call`'s `procedure` does NOT bubble (only `args` do) -- jsonata-js's
///   function/partial case never calls `pushAncestry` on `result.procedure`.
/// - `Lambda`'s `body` does NOT bubble at all -- jsonata-js's lambda case
///   has no `pushAncestry` call for the body. A `%` inside a lambda body
///   refers to that lambda's OWN invocation-time ancestry chain (irrelevant
///   at definition/parse time), so it's correctly not resolved here; it
///   simply remains an inert `AstNode::Parent(label)` in the body until the
///   lambda is invoked (matching jsonata-js: `function(){%}` parses fine,
///   with the raw `%` untouched inside the body).
fn transform_children(
    node: AstNode,
    state: &mut AncestryState,
) -> Result<Transformed, AstTransformError> {
    match node {
        AstNode::Binary { op, lhs, rhs } => {
            let lhs_t = transform_node(*lhs, state)?;
            let rhs_t = transform_node(*rhs, state)?;
            let mut pending = lhs_t.pending;
            pending.extend(rhs_t.pending);
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
            let t = transform_node(*operand, state)?;
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
                let t = transform_node(e, state)?;
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
                let t = transform_node(a, state)?;
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
            let procedure_t = transform_node(*procedure, state)?;
            let mut pending = Vec::new();
            let mut transformed_args = Vec::with_capacity(args.len());
            for a in args {
                let t = transform_node(a, state)?;
                pending.extend(t.pending);
                transformed_args.push(t.node);
            }
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
            let body_t = transform_node(*body, state)?;
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
                let k_t = transform_node(k, state)?;
                pending.extend(k_t.pending);
                let v_t = transform_node(v, state)?;
                pending.extend(v_t.pending);
                transformed.push((k_t.node, v_t.node));
            }
            Ok(Transformed {
                node: AstNode::Object(transformed),
                pending,
            })
        }
        AstNode::ObjectTransform { input, pattern } => {
            let input_t = transform_node(*input, state)?;
            let mut pending = input_t.pending;
            let mut transformed_pattern = Vec::with_capacity(pattern.len());
            for (k, v) in pattern {
                let k_t = transform_node(k, state)?;
                pending.extend(k_t.pending);
                let v_t = transform_node(v, state)?;
                pending.extend(v_t.pending);
                transformed_pattern.push((k_t.node, v_t.node));
            }
            Ok(Transformed {
                node: AstNode::ObjectTransform {
                    input: Box::new(input_t.node),
                    pattern: transformed_pattern,
                },
                pending,
            })
        }
        AstNode::Conditional {
            condition,
            then_branch,
            else_branch,
        } => {
            let condition_t = transform_node(*condition, state)?;
            let then_t = transform_node(*then_branch, state)?;
            let mut pending = condition_t.pending;
            pending.extend(then_t.pending);
            let else_t = match else_branch {
                Some(e) => Some(transform_node(*e, state)?),
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
        AstNode::Sort { input, terms } => {
            // Mirrors jsonata-js's `case '^'` (parser.js ~L1151-1170): the
            // sort is modeled as a synthetic `sort` step APPENDED to the
            // input path, each term's own seeking `%` slots are bubbled onto
            // it, then resolveAncestry walks them backward. Because the sort
            // step sits after every input step, resolveAncestry starts at the
            // step BEFORE it -- i.e. the LAST real input step -- so a level-1
            // term slot resolves against the last input step (no predicate-
            // style "resolve against the step itself" special case is needed;
            // it's a plain uniform backward walk over the input steps).
            let input_t = transform_node(*input, state)?;
            let was_path = matches!(input_t.node, AstNode::Path { .. });
            // jsonata wraps a non-path input into a single-step path so the
            // sort step has something to walk back through. We do the same for
            // the walk, then unwrap again if nothing tagged the wrapped step.
            let mut steps = match input_t.node {
                AstNode::Path { steps } => steps,
                other => vec![PathStep::new(other)],
            };
            let mut pending = input_t.pending;
            let mut transformed_terms = Vec::with_capacity(terms.len());
            for (expr, asc) in terms {
                let t = transform_node(expr, state)?;
                for slot in t.pending {
                    let remaining = walk_backward(&mut steps, &slot.label, slot.level, state)?;
                    if remaining > 0 {
                        pending.push(PendingAncestor {
                            label: slot.label,
                            level: remaining,
                        });
                    }
                }
                transformed_terms.push((t.node, asc));
            }
            let input_node = if was_path {
                AstNode::Path { steps }
            } else {
                // Single-node input: keep it wrapped only if a sort term
                // actually tagged it (so the ancestor label survives on a
                // PathStep); otherwise restore the bare node unchanged.
                let s = steps.pop().expect("single wrapped step");
                if s.is_tuple || s.ancestor_label.is_some() {
                    AstNode::Path { steps: vec![s] }
                } else {
                    s.node
                }
            };
            Ok(Transformed {
                node: AstNode::Sort {
                    input: Box::new(input_node),
                    terms: transformed_terms,
                },
                pending,
            })
        }
        AstNode::Transform {
            location,
            update,
            delete,
        } => {
            let location_t = transform_node(*location, state)?;
            let update_t = transform_node(*update, state)?;
            let mut pending = location_t.pending;
            pending.extend(update_t.pending);
            let delete_t = match delete {
                Some(d) => Some(transform_node(*d, state)?),
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
            let t = transform_node(*inner, state)?;
            Ok(Transformed {
                node: AstNode::FunctionApplication(Box::new(t.node)),
                pending: t.pending,
            })
        }
        AstNode::ArrayGroup(elements) => {
            let mut pending = Vec::new();
            let mut transformed = Vec::with_capacity(elements.len());
            for e in elements {
                let t = transform_node(e, state)?;
                pending.extend(t.pending);
                transformed.push(t.node);
            }
            Ok(Transformed {
                node: AstNode::ArrayGroup(transformed),
                pending,
            })
        }
        AstNode::Predicate(inner) => {
            let t = transform_node(*inner, state)?;
            Ok(Transformed {
                node: AstNode::Predicate(Box::new(t.node)),
                pending: t.pending,
            })
        }
        // Leaf nodes and everything else pass through unchanged.
        other => Ok(Transformed::leaf(other)),
    }
}

/// Resolve a path's steps: migrate `#`/`@` markers into step-level flags,
/// then walk backward resolving any `%`/`%.%` references, left to right in
/// step-encounter order. Mirrors resolveAncestry (parser.js ~L1002-1030),
/// collapsed from jsonata-js's incremental per-'.' invocation into a single
/// pass: our parser already flattens an entire dotted chain into one flat
/// `steps` list up front (unlike jsonata-js's nested binary '.' AST nodes,
/// processed one dot at a time), so resolving every step's own pending
/// reference against the FULL flattened list-so-far in left-to-right order
/// produces the same result as jsonata-js's incremental resolution.
fn transform_path_steps(
    steps: Vec<PathStep>,
    state: &mut AncestryState,
) -> Result<(Vec<PathStep>, Vec<PendingAncestor>), AstTransformError> {
    // Pass 1: migrate #/@ into step flags, recursing into nested content
    // (which may itself bubble up pending `%` references, e.g. an object
    // constructor or array containing a `%`). `own_pending[i]` is whatever
    // pending arose from producing `resolved[i]` -- attached to the LAST
    // step of a marker's splice, since that's the step position pending
    // ancestor resolution should walk backward from.
    let mut resolved: Vec<PathStep> = Vec::with_capacity(steps.len());
    let mut own_pending: Vec<Vec<PendingAncestor>> = Vec::with_capacity(steps.len());
    // `pred_pending[i]` holds the seeking `%` slots bubbled up from step i's
    // own filter predicate(s) (`Stage::Filter`), transformed here so a `%`
    // inside `Product[%.OrderID=...]` is resolved (was previously left
    // untouched, since stages weren't recursed into). Transformed AFTER the
    // step's node so the step's OWN `%`-ness (if any) claims a label first,
    // matching jsonata-js's slot-creation order.
    let mut pred_pending: Vec<Vec<PendingAncestor>> = Vec::with_capacity(steps.len());
    for step in steps {
        let (spliced, pending) = migrate_binding_markers(step, state)?;
        let last_idx = spliced.len().saturating_sub(1);
        let mut pending_opt = Some(pending);
        for (i, mut s) in spliced.into_iter().enumerate() {
            let mut pp: Vec<PendingAncestor> = Vec::new();
            let stages = std::mem::take(&mut s.stages);
            let mut new_stages = Vec::with_capacity(stages.len());
            for stage in stages {
                match stage {
                    Stage::Filter(expr) => {
                        let t = transform_node(*expr, state)?;
                        pp.extend(t.pending);
                        new_stages.push(Stage::Filter(Box::new(t.node)));
                    }
                    // Index stages carry only a variable name -- nothing to
                    // resolve/transform.
                    Stage::Index(v) => new_stages.push(Stage::Index(v)),
                }
            }
            s.stages = new_stages;
            resolved.push(s);
            pred_pending.push(pp);
            if i == last_idx {
                own_pending.push(pending_opt.take().unwrap_or_default());
            } else {
                own_pending.push(Vec::new());
            }
        }
    }

    // Pass 2: for each step (in ascending/encounter order), resolve first its
    // predicate slots (mirroring jsonata-js pushing predicate slots onto the
    // step's seekingParent BEFORE the step's own slot), then its own pending.
    // Any reference that runs off the front of this path (never finding a
    // target) bubbles up as this whole Path's own pending.
    let mut path_pending: Vec<PendingAncestor> = Vec::new();
    for i in 0..resolved.len() {
        for pending in std::mem::take(&mut pred_pending[i]) {
            let remaining =
                resolve_predicate_slot(&mut resolved, i, &pending.label, pending.level, state)?;
            if remaining > 0 {
                path_pending.push(PendingAncestor {
                    label: pending.label,
                    level: remaining,
                });
            }
        }
        let pending_here = std::mem::take(&mut own_pending[i]);
        for pending in pending_here {
            let remaining =
                walk_backward(&mut resolved[..i], &pending.label, pending.level, state)?;
            if remaining > 0 {
                path_pending.push(PendingAncestor {
                    label: pending.label,
                    level: remaining,
                });
            }
        }
    }

    Ok((resolved, path_pending))
}

/// Resolve one seeking `%` slot that bubbled up out of a filter predicate
/// attached to step `i`. Mirrors jsonata-js's `case '['` slot handling
/// (parser.js ~L1119-1128):
/// - a `level == 1` slot resolves against the attached step ITSELF first
///   (`seekParent(step, slot)`): a `name`/`wildcard` step gets tagged; a `%`
///   (parent) step instead bumps the level and the walk continues backward;
/// - a `level > 1` slot is decremented (the attached step is skipped, never
///   tagged) and resolved by walking backward through the steps BEFORE it.
///
/// Either way, whatever level remains unresolved is walked backward through
/// `resolved[..i]`; the leftover (if the reference runs off the path front)
/// is returned to bubble up as the enclosing path's own pending.
fn resolve_predicate_slot(
    resolved: &mut [PathStep],
    i: usize,
    label: &str,
    level: usize,
    state: &mut AncestryState,
) -> Result<usize, AstTransformError> {
    // Split so the attached step (`rest[0]`) and the steps before it
    // (`prefix`) can be borrowed mutably at the same time.
    let (prefix, rest) = resolved.split_at_mut(i);
    let remaining = if level == 1 {
        seek_parent_step(&mut rest[0], label, 1, state)?
    } else {
        level - 1
    };
    if remaining == 0 {
        Ok(0)
    } else {
        walk_backward(prefix, label, remaining, state)
    }
}

/// Walk backward through `steps` (from its last element) trying to resolve
/// a single pending ancestor reference at `level`. Returns the remaining
/// level: 0 means fully resolved (some step in `steps` was tagged); >0 means
/// `steps` ran out before the reference resolved, so the caller must keep
/// walking further back through whatever contains `steps` (or, if there is
/// nothing further back, treat it as still-pending / bubble it up).
fn walk_backward(
    steps: &mut [PathStep],
    label: &str,
    mut level: usize,
    state: &mut AncestryState,
) -> Result<usize, AstTransformError> {
    let mut index = steps.len();
    while level > 0 {
        if index == 0 {
            return Ok(level);
        }
        index -= 1;
        // Skip filter-predicate pseudo-steps: our parser encodes `@$v[pred]`
        // and standalone `foo[pred]` chained after a marker as a separate
        // `Predicate` step, whereas jsonata-js carries the predicate as a
        // `stage` on the owning step (so it never appears as a distinct step in
        // resolveAncestry). A predicate is a filter, never an ancestor target,
        // so the backward ancestry walk steps over it -- without this, a `%`
        // after `books@$B[$L.isbn=$B.isbn]` hits the predicate step and wrongly
        // reports S0217.
        //
        // Then skip over a run of contiguous focus-bound (`@$var`) steps,
        // treating them as a SINGLE ancestor hop -- mirrors jsonata-js
        // resolveAncestry (parser.js ~L1023-1025): `while(index >= 0 &&
        // step.focus && path.steps[index].focus) { step = path.steps[index--] }`.
        // Because our extra `Predicate` steps sit between the focus steps (which
        // in jsonata are adjacent, the predicates being stages), the
        // focus-contiguity test must look through those predicate steps to the
        // previous REAL navigation step. So in
        // `library.loans@$L.books@$B[...].customers@$C[...].{ $keys(%.%) }` all
        // three focus steps collapse into one hop and `%.%` reaches the root.
        loop {
            while index > 0 && matches!(steps[index].node, AstNode::Predicate(_)) {
                index -= 1;
            }
            // Locate the previous non-predicate step (if any) to test contiguity.
            let mut prev = None;
            if index > 0 {
                let mut j = index - 1;
                loop {
                    if !matches!(steps[j].node, AstNode::Predicate(_)) {
                        prev = Some(j);
                        break;
                    }
                    if j == 0 {
                        break;
                    }
                    j -= 1;
                }
            }
            match prev {
                Some(p) if steps[index].focus.is_some() && steps[p].focus.is_some() => {
                    index = p;
                }
                _ => break,
            }
        }
        level = seek_parent_step(&mut steps[index], label, level, state)?;
    }
    Ok(0)
}

/// Try to resolve one level of a pending ancestor reference against a
/// single candidate step. Returns the remaining level (0 = tagged here).
/// Mirrors jsonata-js's seekParent (parser.js ~L941-986).
fn seek_parent_step(
    step: &mut PathStep,
    label: &str,
    level: usize,
    state: &mut AncestryState,
) -> Result<usize, AstTransformError> {
    match &mut step.node {
        AstNode::Name(_) | AstNode::Wildcard => {
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
            Ok(remaining)
        }
        // Chained %.%: this step is itself another (already independently
        // resolved-or-pending) `%` -- extend the level and keep walking
        // further back, exactly mirroring seekParent's `case 'parent':
        // slot.level++` (which notably does NOT set `.tuple` here).
        AstNode::Parent(_) => Ok(level + 1),
        // Parenthesized sub-path as a path step (e.g. `Account.(Order.Product).%`
        // parses `(Order.Product)` as `FunctionApplication(Path{...})`) --
        // mirrors seekParent's 'block'/'path' cases layered together: this
        // outer step becomes tuple-producing regardless of where inside the
        // parens the actual ancestor tag lands, and we recurse inward to
        // find it.
        AstNode::FunctionApplication(inner) => {
            step.is_tuple = true;
            seek_parent_wrapped(inner.as_mut(), label, level, state)
        }
        // A parenthesized block reached directly as a path step (e.g. a
        // leading `(Account.Order)` with no `.` before it, or a multi-
        // statement `(...)`) -- mirrors seekParent's 'block' case: recurse
        // into the LAST expression.
        AstNode::Block(exprs) => match exprs.last_mut() {
            Some(last) => {
                step.is_tuple = true;
                seek_parent_wrapped(last, label, level, state)
            }
            // An empty block `()` produces no ancestor and no tuple; the walk
            // simply steps over it with the level unchanged (mirrors jsonata-js
            // seekParent's `if(node.expressions.length > 0)` guard, which leaves
            // the slot untouched for an empty block). Lets `Account.Order.().%`
            // resolve `%` against `Order` rather than raising S0217.
            None => Ok(level),
        },
        _ => Err(coded(
            "S0217",
            "The parent operator % cannot derive an ancestor from this kind of path step",
        )),
    }
}

/// Recurse into a "wrapped" target (a `FunctionApplication`'s sole inner
/// expression, or a `Block`'s last expression) that must itself resolve to
/// a nested `Path` for us to walk backward through it -- mirrors how
/// jsonata-js's block/path seekParent cases can be layered on top of each
/// other for doubly-nested parens (e.g. `Account.(Order.(Product)).%`).
/// Anything else (a literal, a function call, ...) can't derive an
/// ancestor: S0217.
fn seek_parent_wrapped(
    node: &mut AstNode,
    label: &str,
    level: usize,
    state: &mut AncestryState,
) -> Result<usize, AstTransformError> {
    match node {
        AstNode::Path { steps } => walk_backward(steps, label, level, state),
        // A nested block (e.g. the inner `()` of `.()`, or `(a; b)`): recurse
        // into its last expression, or -- for an empty block -- step over it
        // leaving the level unchanged (jsonata-js seekParent's block guard).
        AstNode::Block(exprs) => match exprs.last_mut() {
            Some(last) => seek_parent_wrapped(last, label, level, state),
            None => Ok(level),
        },
        _ => Err(coded(
            "S0217",
            "The parent operator % cannot derive an ancestor from this kind of expression",
        )),
    }
}

/// Convert a step's raw-parse-time binding marker (if any) into the unified
/// PathStep flags, recursing into the step's own node first (a step's node
/// can itself be a Block/nested Path containing `%`/`@`/`#`).
///
/// Returns a `Vec` (not a single `PathStep`) because a marker's `lhs`/`input`
/// can itself turn out to be a multi-step `Path` -- see `splice_marker_steps`
/// -- in which case ALL of those steps must be spliced into the caller's
/// flat list in place of this one input step, with the marker's flags
/// stamped onto the LAST of them (not onto a step wrapping the whole thing).
/// Also returns whatever pending ancestor references bubbled up from
/// transforming this step's content.
fn migrate_binding_markers(
    mut step: PathStep,
    state: &mut AncestryState,
) -> Result<(Vec<PathStep>, Vec<PendingAncestor>), AstTransformError> {
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
            let transformed_lhs = transform_node(*lhs, state)?;
            splice_marker_steps(transformed_lhs, BindingMarker::Focus(var_name))
        }
        AstNode::Binary {
            op: BinaryOp::IndexBind,
            lhs,
            rhs,
        } => {
            let var_name = match *rhs {
                AstNode::Variable(name) => name,
                _ => unreachable!("parser guarantees IndexBind's rhs is always Variable"),
            };
            let transformed_lhs = transform_node(*lhs, state)?;
            splice_marker_steps(transformed_lhs, BindingMarker::Index(var_name))
        }
        other => {
            let t = transform_node(other, state)?;
            step.node = t.node;
            Ok((vec![step], t.pending))
        }
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
            .parse()
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
            Stage::Index(_) => unreachable!("no index stage in this test"),
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
            Stage::Index(_) => unreachable!("no index stage in this test"),
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
            Stage::Index(_) => unreachable!("no index stage in this test"),
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
            Stage::Index(_) => unreachable!("no index stage in this test"),
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
            .parse()
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
        .parse()
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
            steps: vec![PathStep::new(AstNode::Binary {
                op: BinaryOp::IndexBind,
                lhs: Box::new(AstNode::Name("arr".to_string())),
                rhs: Box::new(AstNode::Variable("i".to_string())),
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
        let err = resolve_ancestry(AstNode::Parent(String::new())).unwrap_err();
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
            .parse()
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
            .parse()
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
            .parse()
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
            .parse()
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
            .parse()
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
            .parse()
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
            .parse()
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
            .parse()
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
                .parse()
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
        .parse()
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

    #[test]
    fn test_real_parser_percent_through_parenthesized_step_function_application() {
        // parent001.jsonata's shape: "Account.(Order.Product).%" -- parens
        // around a multi-step sub-path parse as a FunctionApplication step
        // wrapping a nested Path. `%` must walk INTO that nested path to
        // find Product (its last step) as the target, exactly as if the
        // parens weren't there.
        let ast = crate::parser::Parser::new("Account.(Order.Product).%".to_string())
            .unwrap()
            .parse()
            .unwrap();
        let result = resolve_ancestry(ast).unwrap();
        match result {
            AstNode::Path { steps } => {
                assert_eq!(steps.len(), 3);
                assert!(steps[1].is_tuple, "the wrapping step must be flagged tuple");
                match &steps[1].node {
                    AstNode::FunctionApplication(inner) => match inner.as_ref() {
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
                .parse()
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
                    AstNode::FunctionApplication(inner) => match inner.as_ref() {
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
            .parse()
            .unwrap();
        let err = resolve_ancestry(ast).unwrap_err();
        assert!(err.to_string().starts_with("S0217"));
    }

    #[test]
    fn test_real_parser_lambda_body_percent_does_not_bubble_or_error() {
        // "function(){ % }" -- jsonata-js parses this successfully (the raw
        // `%` is left untouched inside the lambda body, only failing at
        // runtime when/if the lambda is invoked). Confirms Lambda bodies
        // don't bubble their pending to the enclosing scope.
        let ast = crate::parser::Parser::new("function(){ % }".to_string())
            .unwrap()
            .parse()
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
            .parse()
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
        .parse()
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
            .parse()
            .unwrap();
        assert!(resolve_ancestry(ast).is_ok());
    }
}

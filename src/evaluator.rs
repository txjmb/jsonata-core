// Expression evaluator
// Mirrors jsonata.js from the reference implementation

#![allow(clippy::cloned_ref_to_slice_refs)]
#![allow(clippy::explicit_counter_loop)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::manual_strip)]

use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::ast::{AstNode, BinaryOp, PathStep, Stage};
use crate::parser;
use crate::value::{flattens_in_step, with_stack, JArray, JValue};
use indexmap::IndexMap;
use std::rc::Rc;
use thiserror::Error;

/// Specialized sort comparator for `$l.field op $r.field` patterns.
/// Bypasses the full AST evaluator for simple field-based sort comparisons.
///
/// In JSONata `$sort`, the comparator returns true when `$l` should come AFTER `$r`.
/// `$l.field > $r.field` swaps when left > right, producing ascending order.
/// `$l.field < $r.field` swaps when left < right, producing descending order.
struct SpecializedSortComparator {
    field: String,
    descending: bool,
}

/// Pre-extracted sort key for the Schwartzian transform in specialized sorting.
enum SortKey {
    Num(f64),
    Str(Rc<str>),
    None,
}

fn compare_sort_keys(a: &SortKey, b: &SortKey, descending: bool) -> Ordering {
    let ord = match (a, b) {
        (SortKey::Num(x), SortKey::Num(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        (SortKey::Str(x), SortKey::Str(y)) => (**x).cmp(&**y),
        (SortKey::None, SortKey::None) => Ordering::Equal,
        (SortKey::None, _) => Ordering::Greater,
        (_, SortKey::None) => Ordering::Less,
        // Mixed types: maintain original order
        _ => Ordering::Equal,
    };
    if descending {
        ord.reverse()
    } else {
        ord
    }
}

/// Try to extract a specialized sort comparator from a lambda AST node.
/// Detects patterns like `function($l, $r) { $l.field > $r.field }`.
fn try_specialize_sort_comparator(
    body: &AstNode,
    left_param: &str,
    right_param: &str,
) -> Option<SpecializedSortComparator> {
    let AstNode::Binary { op, lhs, rhs } = body else {
        return None;
    };

    // Returns true if op means "swap when left > right" (ascending order).
    let is_ascending = |op: &BinaryOp| -> Option<bool> {
        match op {
            BinaryOp::GreaterThan | BinaryOp::GreaterThanOrEqual => Some(true),
            BinaryOp::LessThan | BinaryOp::LessThanOrEqual => Some(false),
            _ => None,
        }
    };

    // Extract field name from a `$param.field` path with no stages.
    let extract_var_field = |node: &AstNode, param: &str| -> Option<String> {
        let AstNode::Path { steps } = node else {
            return None;
        };
        if steps.len() != 2 {
            return None;
        }
        let AstNode::Variable(var) = &steps[0].node else {
            return None;
        };
        if var != param {
            return None;
        }
        let AstNode::Name(field) = &steps[1].node else {
            return None;
        };
        if !steps[0].stages.is_empty() || !steps[1].stages.is_empty() {
            return None;
        }
        Some(field.clone())
    };

    // Try both orientations: $l.field op $r.field and $r.field op $l.field (flipped).
    for flipped in [false, true] {
        let (lhs_param, rhs_param) = if flipped {
            (right_param, left_param)
        } else {
            (left_param, right_param)
        };
        if let (Some(lhs_field), Some(rhs_field)) = (
            extract_var_field(lhs, lhs_param),
            extract_var_field(rhs, rhs_param),
        ) {
            if lhs_field == rhs_field {
                let descending = match op {
                    // Subtraction: `$l.f - $r.f` → positive when l > r → ascending.
                    // Flipped `$r.f - $l.f` → positive when r > l → descending.
                    BinaryOp::Subtract => flipped,
                    // Comparison: `$l.f > $r.f` → ascending, flipped inverts.
                    _ => {
                        let ascending = is_ascending(op)?;
                        if flipped {
                            ascending
                        } else {
                            !ascending
                        }
                    }
                };
                return Some(SpecializedSortComparator {
                    field: lhs_field,
                    descending,
                });
            }
        }
    }
    None
}

// ──────────────────────────────────────────────────────────────────────────────
// CompiledExpr — unified compiled expression framework
// ──────────────────────────────────────────────────────────────────────────────
//
// Generalizes SpecializedPredicate and CompiledObjectMap into a single IR that
// can represent arbitrary simple expressions without AST walking.  Evaluated in
// a tight loop with no recursion tracking, no scope management, and no AstNode
// pattern matching.

/// Shape cache: maps field names to their positional index in an IndexMap.
/// When all objects in an array share the same key ordering (extremely common
/// in JSON data), field lookups become O(1) Vec index access via `get_index()`
/// instead of O(1)-amortized hash lookups.
type ShapeCache = HashMap<String, usize>;

/// `obj[field]`, read at the position the shape cache gives when the object
/// has the field there, else by hash lookup: an array's objects need not all
/// order their keys like the first.
#[inline]
fn shaped_get<'a>(
    obj: &'a IndexMap<String, JValue>,
    field: &str,
    shape: Option<&ShapeCache>,
) -> Option<&'a JValue> {
    if let Some(&idx) = shape.and_then(|shape| shape.get(field)) {
        if let Some((key, value)) = obj.get_index(idx) {
            if key == field {
                return Some(value);
            }
        }
    }
    obj.get(field)
}

/// Build a shape cache from the first object in an array.
/// Returns None if the data is not an object.
fn build_shape_cache(first_element: &JValue) -> Option<ShapeCache> {
    match first_element {
        JValue::Object(obj) => {
            let mut cache = HashMap::with_capacity(obj.len());
            for (idx, (key, _)) in obj.iter().enumerate() {
                cache.insert(key.clone(), idx);
            }
            Some(cache)
        }
        _ => None,
    }
}

/// Comparison operator for compiled expressions.
#[derive(Debug, Clone, Copy)]
pub(crate) enum CompiledCmp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// Arithmetic operator for compiled expressions.
#[derive(Debug, Clone, Copy)]
pub(crate) enum CompiledArithOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

/// Unified compiled expression — replaces SpecializedPredicate & CompiledObjectMap.
///
/// `try_compile_expr()` converts an AstNode subtree into a CompiledExpr at
/// expression-compile time (once), then `eval_compiled()` evaluates it per
/// element in O(expression-size) with no heap allocation in the hot path.
#[derive(Clone, Debug)]
pub(crate) enum CompiledExpr {
    // ── Leaves ──────────────────────────────────────────────────────────
    /// A literal value known at compile time.
    Literal(JValue),
    /// Single-level field lookup on the current object: `obj.get("field")`.
    /// Over an array it follows the general path rules (`FieldPath`).
    FieldLookup(String),
    /// Two-level nested field lookup: `obj.get("a")?.get("b")`. Over an
    /// array, at either level, it follows the general path rules.
    NestedFieldLookup(String, String),
    /// Variable lookup from enclosing scope (e.g. `$var`).
    /// Resolved at eval time via a provided variable map.
    VariableLookup(String),

    // ── Comparison ──────────────────────────────────────────────────────
    Compare {
        op: CompiledCmp,
        lhs: Box<CompiledExpr>,
        rhs: Box<CompiledExpr>,
    },

    // ── Arithmetic ──────────────────────────────────────────────────────
    Arithmetic {
        op: CompiledArithOp,
        lhs: Box<CompiledExpr>,
        rhs: Box<CompiledExpr>,
    },

    // ── String ──────────────────────────────────────────────────────────
    Concat(Box<CompiledExpr>, Box<CompiledExpr>),

    // ── Logical ─────────────────────────────────────────────────────────
    And(Box<CompiledExpr>, Box<CompiledExpr>),
    Or(Box<CompiledExpr>, Box<CompiledExpr>),
    Not(Box<CompiledExpr>),
    /// Negation of a numeric value.
    Negate(Box<CompiledExpr>),

    // ── Conditional ─────────────────────────────────────────────────────
    Conditional {
        condition: Box<CompiledExpr>,
        then_expr: Box<CompiledExpr>,
        else_expr: Option<Box<CompiledExpr>>,
    },

    // ── Compound ────────────────────────────────────────────────────────
    /// Object construction: `{"key1": expr1, "key2": expr2, ...}`
    ObjectConstruct(Vec<(String, CompiledExpr)>),
    /// Array construction: `[expr1, expr2, ...]`
    ///
    /// Each element carries a `bool` flag: `true` means the element originated
    /// from an explicit `AstNode::Array` constructor and must be kept nested even
    /// if it evaluates to an array. `false` means the element's array value is
    /// flattened one level into the outer result (JSONata `[a.b, ...]` semantics).
    /// Undefined values are always skipped.
    ArrayConstruct(Vec<(CompiledExpr, bool)>),

    // ── Phase 2 extensions ──────────────────────────────────────────────
    /// Multi-step field path with optional per-step filters: `a.b[pred].c`,
    /// evaluated as jsonata-js evaluates a path of field steps (see
    /// `compiled_eval_field_path`). `from_dollar` is set for `$.a...`: the
    /// leading `$` makes it map over the items of an array input, even the
    /// top-level one, which a bare `a...` treats as a single item.
    FieldPath {
        steps: Vec<CompiledStep>,
        from_dollar: bool,
    },

    /// Call a pure, side-effect-free builtin with compiled arguments.
    /// Only builtins in COMPILABLE_BUILTINS are allowed here.
    BuiltinCall {
        name: &'static str,
        args: Vec<CompiledExpr>,
    },

    /// Sequential block: evaluate all expressions, return last value.
    Block(Vec<CompiledExpr>),

    /// Coalesce (`??`): return lhs if it is defined and non-null, else rhs.
    Coalesce(Box<CompiledExpr>, Box<CompiledExpr>),

    // ── Higher-order functions with inline lambdas ───────────────────────
    /// `$map(array, function($v [, $i]) { body })` — compiled when the second
    /// argument is an inline lambda literal (not a stored variable).
    /// `params` holds the lambda parameter names (without `$`), 1 or 2 elements.
    MapCall {
        array: Box<CompiledExpr>,
        params: Vec<String>,
        body: Box<CompiledExpr>,
    },
    /// `$filter(array, function($v [, $i]) { body })` — compiled when the second
    /// argument is an inline lambda literal.
    FilterCall {
        array: Box<CompiledExpr>,
        params: Vec<String>,
        body: Box<CompiledExpr>,
    },
    /// `$reduce(array, function($acc, $v) { body } [, initial])` — compiled when the
    /// second argument is an inline lambda literal with exactly 2 parameters.
    ReduceCall {
        array: Box<CompiledExpr>,
        params: Vec<String>,
        body: Box<CompiledExpr>,
        initial: Option<Box<CompiledExpr>>,
    },
}

/// One step in a compiled `FieldPath`.
#[derive(Clone, Debug)]
pub(crate) struct CompiledStep {
    /// Field name to look up at this step.
    pub field: String,
    /// Optional predicate filter (a `Stage::Filter`), applied to the value
    /// looked up in each input item, as jsonata-js applies a step's stages.
    pub filter: Option<CompiledExpr>,
}

/// Try to compile an AstNode subtree into a CompiledExpr.
/// Returns None for anything that requires full AST evaluation (lambda calls,
/// function calls with side effects, complex paths, etc.).
///
/// The result calls builtins directly, so it is only right where no binding
/// shadows them: a whole expression evaluated with no bindings (the bytecode
/// VM). Inside the evaluator use `Evaluator::compile_in_scope` or
/// `CompiledBody`, which apply the shadowing rule (#191).
pub(crate) fn try_compile_expr(node: &AstNode) -> Option<CompiledExpr> {
    reject_if_too_large_to_pool(try_compile_expr_inner(node, None)?)
}

/// Like `try_compile_expr` but additionally allows the specified variable names
/// to be compiled as `VariableLookup`. Used by HOF integration where lambda
/// parameters are known and will be provided via the `vars` map at eval time.
/// Private for the same reason as `try_compile_expr`'s caveat.
fn try_compile_expr_with_allowed_vars(
    node: &AstNode,
    allowed_vars: &[&str],
) -> Option<CompiledExpr> {
    reject_if_too_large_to_pool(try_compile_expr_inner(node, Some(allowed_vars))?)
}

/// The builtins a compiled expression calls directly, by name: its
/// `BuiltinCall`s and the `$map`/`$filter`/`$reduce` it inlines. A compiled
/// expression gives the tree-walker's result only where each of these names
/// still means the builtin (`Evaluator::builtin_unshadowed`).
fn compiled_builtin_names(expr: &CompiledExpr, out: &mut Vec<&'static str>) {
    let add = |name: &'static str, out: &mut Vec<&'static str>| {
        if !out.contains(&name) {
            out.push(name);
        }
    };
    match expr {
        CompiledExpr::Literal(_)
        | CompiledExpr::FieldLookup(_)
        | CompiledExpr::NestedFieldLookup(..)
        | CompiledExpr::VariableLookup(_) => {}
        CompiledExpr::Compare { lhs, rhs, .. }
        | CompiledExpr::Arithmetic { lhs, rhs, .. }
        | CompiledExpr::Concat(lhs, rhs)
        | CompiledExpr::And(lhs, rhs)
        | CompiledExpr::Or(lhs, rhs)
        | CompiledExpr::Coalesce(lhs, rhs) => {
            compiled_builtin_names(lhs, out);
            compiled_builtin_names(rhs, out);
        }
        CompiledExpr::Not(e) | CompiledExpr::Negate(e) => compiled_builtin_names(e, out),
        CompiledExpr::Conditional {
            condition,
            then_expr,
            else_expr,
        } => {
            compiled_builtin_names(condition, out);
            compiled_builtin_names(then_expr, out);
            if let Some(e) = else_expr {
                compiled_builtin_names(e, out);
            }
        }
        CompiledExpr::ObjectConstruct(fields) => {
            for (_, e) in fields {
                compiled_builtin_names(e, out);
            }
        }
        CompiledExpr::ArrayConstruct(elems) => {
            for (e, _) in elems {
                compiled_builtin_names(e, out);
            }
        }
        CompiledExpr::FieldPath { steps, .. } => {
            for step in steps {
                if let Some(filter) = &step.filter {
                    compiled_builtin_names(filter, out);
                }
            }
        }
        CompiledExpr::BuiltinCall { name, args } => {
            add(name, out);
            for a in args {
                compiled_builtin_names(a, out);
            }
        }
        CompiledExpr::Block(exprs) => {
            for e in exprs {
                compiled_builtin_names(e, out);
            }
        }
        CompiledExpr::MapCall { array, body, .. }
        | CompiledExpr::FilterCall { array, body, .. } => {
            add(
                if matches!(expr, CompiledExpr::MapCall { .. }) {
                    "map"
                } else {
                    "filter"
                },
                out,
            );
            compiled_builtin_names(array, out);
            compiled_builtin_names(body, out);
        }
        CompiledExpr::ReduceCall {
            array,
            body,
            initial,
            ..
        } => {
            add("reduce", out);
            compiled_builtin_names(array, out);
            compiled_builtin_names(body, out);
            if let Some(i) = initial {
                compiled_builtin_names(i, out);
            }
        }
    }
}

/// A function body compiled when the function is defined, or a path step's
/// body compiled the first time an evaluator runs the step (kept under
/// `PathStep::id`), with the builtin names it calls directly. Whether those names still mean the
/// builtins is only known where it runs, so a function body is reached only
/// through `Evaluator::usable_compiled_body` and a step body only through
/// `Evaluator::cached_step_body`.
#[derive(Clone, Debug)]
pub(crate) struct CompiledBody {
    expr: CompiledExpr,
    builtins: Box<[&'static str]>,
}

impl CompiledBody {
    /// Compile a function body for the fast path. A signature (which coerces
    /// and checks arguments) or a tail call needs the full evaluator.
    fn compile(
        body: &AstNode,
        params: &[String],
        signature: &Option<String>,
        thunk: bool,
    ) -> Option<CompiledBody> {
        if signature.is_some() || thunk {
            return None;
        }
        let var_refs: Vec<&str> = params.iter().map(|s| s.as_str()).collect();
        let expr = try_compile_expr_with_allowed_vars(body, &var_refs)?;
        Some(CompiledBody::new(expr))
    }

    /// Compile a path step's body (an object constructor or the expression
    /// of a `.(expr)` step) to run on each of the step's items, with no
    /// variables of its own.
    fn compile_step(body: &AstNode) -> Option<CompiledBody> {
        #[cfg(test)]
        STEP_COMPILES.with(|n| n.set(n.get() + 1));
        try_compile_expr(body).map(CompiledBody::new)
    }

    fn new(expr: CompiledExpr) -> CompiledBody {
        let mut builtins = Vec::new();
        compiled_builtin_names(&expr, &mut builtins);
        CompiledBody {
            expr,
            builtins: builtins.into_boxed_slice(),
        }
    }
}

#[cfg(test)]
thread_local! {
    /// How many times a path step's body has been compiled on this thread,
    /// for tests that check the step cache (#197).
    static STEP_COMPILES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Reject (fall back to the tree-walker) a fully-built `CompiledExpr` tree
/// whose node count could overflow one of `BytecodeCompiler`'s `u16`-indexed
/// pools (`const_pool` / `string_pool` / `fallback_exprs` / `sub_programs`).
///
/// `BytecodeCompiler::compile` is otherwise infallible (`fn compile(&CompiledExpr)
/// -> BytecodeProgram`, called from `src/compiler.rs`'s own recursive filter-predicate
/// compilation, `src/vm.rs`, and twice from `src/lib.rs`) - changing its signature to
/// return `Option`/`Result` so its 4 pool-interning helpers could "abort gracefully"
/// mid-compilation would ripple to all of those call sites for a bug class that is
/// already astronomically impractical to trigger (it requires tens of thousands of
/// distinct string/field-name/literal constants, or that many nested fallback
/// sub-expressions, inside one compiled expression). Guarding here instead - before
/// `BytecodeCompiler::compile` is ever invoked - avoids that ripple entirely, matching
/// the same "compiler declines, tree-walker (which has no such limit) handles it"
/// architecture used by the `AstNode::Array`/`AstNode::Block` arity guards above.
fn reject_if_too_large_to_pool(compiled: CompiledExpr) -> Option<CompiledExpr> {
    if compiled_expr_node_count_exceeds(&compiled, u16::MAX as usize) {
        None
    } else {
        Some(compiled)
    }
}

/// Conservative upper bound check: does `expr`'s node count exceed `limit`?
///
/// Each of `BytecodeCompiler`'s 4 pools gains at most one entry per countable
/// unit processed here (fewer, after interning dedup), so the total count
/// this walk produces is a safe upper bound for every pool's occupancy -
/// including a pool populated by a *nested*, independently pooled
/// `BytecodeCompiler` instance (e.g. a `FieldPath` step's filter predicate,
/// recompiled into its own `BytecodeProgram` in `compiler.rs`'s `FieldPath`
/// arm), since that nested instance can only ever process a strict subset of
/// this tree's nodes. Over-counting (e.g. including a filter predicate's
/// nodes in the outer count even though it is compiled by a separate
/// `BytecodeCompiler` with its own pools) is safe: it only means falling back
/// to the tree-walker slightly earlier than strictly necessary.
///
/// "Countable unit" is *not* simply "one `CompiledExpr` node": a
/// `CompiledExpr::FieldPath`'s individual `CompiledStep`s are not
/// `CompiledExpr` nodes themselves, yet `compiler.rs`'s `FieldPath` arm
/// interns *every* step's field name into `string_pool` regardless of
/// whether that step carries a filter predicate. So this walk counts each
/// `PathStep` as its own unit (in addition to still recursing into any
/// filter expression the step may have) - a `FieldPath` with N no-filter
/// steps contributes N units here, matching the N `string_pool` entries it
/// costs in `compiler.rs`, not zero.
///
/// Uses a shared decrementing budget so pathologically large trees bail out
/// immediately instead of always walking to completion.
fn compiled_expr_node_count_exceeds(expr: &CompiledExpr, limit: usize) -> bool {
    fn walk(expr: &CompiledExpr, budget: &mut usize) -> bool {
        if *budget == 0 {
            return true;
        }
        *budget -= 1;
        match expr {
            // ── Leaves: no children ──────────────────────────────────
            CompiledExpr::Literal(_)
            | CompiledExpr::FieldLookup(_)
            | CompiledExpr::NestedFieldLookup(_, _)
            | CompiledExpr::VariableLookup(_) => false,

            // ── Binary ────────────────────────────────────────────────
            CompiledExpr::Compare { lhs, rhs, .. }
            | CompiledExpr::Arithmetic { lhs, rhs, .. }
            | CompiledExpr::Concat(lhs, rhs)
            | CompiledExpr::And(lhs, rhs)
            | CompiledExpr::Or(lhs, rhs)
            | CompiledExpr::Coalesce(lhs, rhs) => walk(lhs, budget) || walk(rhs, budget),

            // ── Unary ─────────────────────────────────────────────────
            CompiledExpr::Not(inner) | CompiledExpr::Negate(inner) => walk(inner, budget),

            // ── Conditional ───────────────────────────────────────────
            CompiledExpr::Conditional {
                condition,
                then_expr,
                else_expr,
            } => {
                walk(condition, budget)
                    || walk(then_expr, budget)
                    || else_expr.as_ref().is_some_and(|e| walk(e, budget))
            }

            // ── Compound ──────────────────────────────────────────────
            CompiledExpr::ObjectConstruct(pairs) => pairs.iter().any(|(_, v)| walk(v, budget)),
            CompiledExpr::ArrayConstruct(elems) => elems.iter().any(|(e, _)| walk(e, budget)),
            CompiledExpr::FieldPath { steps, .. } => {
                for step in steps.iter() {
                    // Each step interns its field name into `string_pool` in
                    // `compiler.rs` regardless of whether it has a filter -
                    // count the step itself, not just its optional filter.
                    if *budget == 0 {
                        return true;
                    }
                    *budget -= 1;
                    if let Some(filter) = step.filter.as_ref() {
                        if walk(filter, budget) {
                            return true;
                        }
                    }
                }
                false
            }
            CompiledExpr::BuiltinCall { args, .. } => args.iter().any(|a| walk(a, budget)),
            CompiledExpr::Block(exprs) => exprs.iter().any(|e| walk(e, budget)),

            // ── Higher-order functions ────────────────────────────────
            CompiledExpr::MapCall { array, body, .. }
            | CompiledExpr::FilterCall { array, body, .. } => {
                walk(array, budget) || walk(body, budget)
            }
            CompiledExpr::ReduceCall {
                array,
                body,
                initial,
                ..
            } => {
                walk(array, budget)
                    || walk(body, budget)
                    || initial.as_ref().is_some_and(|i| walk(i, budget))
            }
        }
    }
    let mut budget = limit;
    walk(expr, &mut budget)
}

/// Deepest AST nesting the compiled path accepts; anything deeper is left to
/// the tree-walker. Neither compilation nor `eval_compiled` checks the native
/// stack per node (that would tax the hot path), and both can run deep inside
/// a recursion with only `EVAL_RED_ZONE` of stack left, so their recursion
/// has to fit in it: at roughly 1KB of stack per level for compiling plus
/// evaluating in a release build (~9KB unoptimized), 128 levels stay inside
/// the red zone. Real expressions are far shallower; the parser alone allows
/// ~1000 levels.
const MAX_COMPILED_DEPTH: usize = 128;

thread_local! {
    static COMPILE_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// True when `node` is an array constructor in jsonata-js's terms: a `[...]`,
/// with or without predicates or `[]` after it (`[1,2][$>0]`, `[[1,2]][0]`,
/// `[1,2][]`). An array constructor member of another one is pushed whole,
/// not spliced in (jsonata-js `evaluateUnary`: `item.value === '['`).
///
/// jsonata-js puts the predicates and the `[]` on the constructor node
/// itself (parser.js `[`), so it stays a constructor; this parser carries
/// them on a one-step path around it instead. So does a `@$v`, which stays
/// on the constructor in jsonata-js (`[[1,2]@$v]` is `[[1,2]]`), but a
/// `#$i` makes a path there, whose value is spliced: `[[1,2]#$i]` is `[1,2]`.
fn is_array_constructor(node: &AstNode) -> bool {
    match node {
        AstNode::Array(_) => true,
        AstNode::Path { steps } => match steps.as_slice() {
            [step] => {
                matches!(step.node, AstNode::Array(_))
                    && step.index_var.is_none()
                    && !step
                        .stages
                        .iter()
                        .any(|stage| matches!(stage, Stage::Index(_)))
            }
            _ => false,
        },
        _ => false,
    }
}

/// Compile one AST node, declining (`None`, so the tree-walker evaluates the
/// whole expression) once nesting passes `MAX_COMPILED_DEPTH`. Every
/// recursive step of compilation passes through here.
fn try_compile_expr_inner(node: &AstNode, allowed_vars: Option<&[&str]>) -> Option<CompiledExpr> {
    struct DepthGuard;
    impl Drop for DepthGuard {
        fn drop(&mut self) {
            COMPILE_DEPTH.with(|d| d.set(d.get() - 1));
        }
    }
    let depth = COMPILE_DEPTH.with(|d| {
        let depth = d.get() + 1;
        d.set(depth);
        depth
    });
    let _guard = DepthGuard;
    if depth > MAX_COMPILED_DEPTH {
        return None;
    }
    try_compile_node(node, allowed_vars)
}

fn try_compile_node(node: &AstNode, allowed_vars: Option<&[&str]>) -> Option<CompiledExpr> {
    match node {
        // ── Literals ────────────────────────────────────────────────────
        AstNode::String(s) => Some(CompiledExpr::Literal(JValue::string(s.clone()))),
        AstNode::Number(n) => Some(CompiledExpr::Literal(JValue::Number(*n))),
        AstNode::Boolean(b) => Some(CompiledExpr::Literal(JValue::Bool(*b))),
        AstNode::Null => Some(CompiledExpr::Literal(JValue::Null)),

        // ── Field access ────────────────────────────────────────────────
        AstNode::Name(field) => Some(CompiledExpr::FieldLookup(field.clone())),

        // ── Variable lookup ─────────────────────────────────────────────
        // $ (empty name) always refers to the current element.
        // Named variables: in HOF mode (allowed_vars=Some), only compile if the
        // variable is in the allowed set (lambda params supplied via vars map).
        // Otherwise they fall back to the tree-walker (see below).
        AstNode::Variable(var) if var.is_empty() => Some(CompiledExpr::VariableLookup(var.clone())),
        AstNode::Variable(var) => {
            if let Some(allowed) = allowed_vars {
                // HOF mode: only compile if the variable is a known lambda param.
                if allowed.contains(&var.as_str()) {
                    return Some(CompiledExpr::VariableLookup(var.clone()));
                }
            }
            // Named variables require Context for correct lookup (scope stack, builtins
            // registry). The compiled fast path passes ctx=None, so fall back to the
            // tree-walker for all non-empty variable references.
            None
        }

        // ── Path expressions ────────────────────────────────────────────
        AstNode::Path { steps } => try_compile_path(steps, allowed_vars),

        // ── Binary operations ───────────────────────────────────────────
        AstNode::Binary { op, lhs, rhs } => {
            let compiled_lhs = try_compile_expr_inner(lhs, allowed_vars)?;
            let compiled_rhs = try_compile_expr_inner(rhs, allowed_vars)?;
            match op {
                // Comparison
                BinaryOp::Equal => Some(CompiledExpr::Compare {
                    op: CompiledCmp::Eq,
                    lhs: Box::new(compiled_lhs),
                    rhs: Box::new(compiled_rhs),
                }),
                BinaryOp::NotEqual => Some(CompiledExpr::Compare {
                    op: CompiledCmp::Ne,
                    lhs: Box::new(compiled_lhs),
                    rhs: Box::new(compiled_rhs),
                }),
                BinaryOp::LessThan => Some(CompiledExpr::Compare {
                    op: CompiledCmp::Lt,
                    lhs: Box::new(compiled_lhs),
                    rhs: Box::new(compiled_rhs),
                }),
                BinaryOp::LessThanOrEqual => Some(CompiledExpr::Compare {
                    op: CompiledCmp::Le,
                    lhs: Box::new(compiled_lhs),
                    rhs: Box::new(compiled_rhs),
                }),
                BinaryOp::GreaterThan => Some(CompiledExpr::Compare {
                    op: CompiledCmp::Gt,
                    lhs: Box::new(compiled_lhs),
                    rhs: Box::new(compiled_rhs),
                }),
                BinaryOp::GreaterThanOrEqual => Some(CompiledExpr::Compare {
                    op: CompiledCmp::Ge,
                    lhs: Box::new(compiled_lhs),
                    rhs: Box::new(compiled_rhs),
                }),
                // Arithmetic
                BinaryOp::Add => Some(CompiledExpr::Arithmetic {
                    op: CompiledArithOp::Add,
                    lhs: Box::new(compiled_lhs),
                    rhs: Box::new(compiled_rhs),
                }),
                BinaryOp::Subtract => Some(CompiledExpr::Arithmetic {
                    op: CompiledArithOp::Sub,
                    lhs: Box::new(compiled_lhs),
                    rhs: Box::new(compiled_rhs),
                }),
                BinaryOp::Multiply => Some(CompiledExpr::Arithmetic {
                    op: CompiledArithOp::Mul,
                    lhs: Box::new(compiled_lhs),
                    rhs: Box::new(compiled_rhs),
                }),
                BinaryOp::Divide => Some(CompiledExpr::Arithmetic {
                    op: CompiledArithOp::Div,
                    lhs: Box::new(compiled_lhs),
                    rhs: Box::new(compiled_rhs),
                }),
                BinaryOp::Modulo => Some(CompiledExpr::Arithmetic {
                    op: CompiledArithOp::Mod,
                    lhs: Box::new(compiled_lhs),
                    rhs: Box::new(compiled_rhs),
                }),
                // Logical
                BinaryOp::And => Some(CompiledExpr::And(
                    Box::new(compiled_lhs),
                    Box::new(compiled_rhs),
                )),
                BinaryOp::Or => Some(CompiledExpr::Or(
                    Box::new(compiled_lhs),
                    Box::new(compiled_rhs),
                )),
                // String concat
                BinaryOp::Concatenate => Some(CompiledExpr::Concat(
                    Box::new(compiled_lhs),
                    Box::new(compiled_rhs),
                )),
                // Coalesce: return lhs if defined/non-null, else rhs
                BinaryOp::Coalesce => Some(CompiledExpr::Coalesce(
                    Box::new(compiled_lhs),
                    Box::new(compiled_rhs),
                )),
                // Anything else (Range, In, ColonEqual, ChainPipe, etc.) — not compilable
                _ => None,
            }
        }

        // ── Unary operations ────────────────────────────────────────────
        AstNode::Unary { op, operand } => {
            let compiled = try_compile_expr_inner(operand, allowed_vars)?;
            match op {
                crate::ast::UnaryOp::Not => Some(CompiledExpr::Not(Box::new(compiled))),
                crate::ast::UnaryOp::Negate => Some(CompiledExpr::Negate(Box::new(compiled))),
            }
        }

        // ── Conditional ─────────────────────────────────────────────────
        AstNode::Conditional {
            condition,
            then_branch,
            else_branch,
        } => {
            let cond = try_compile_expr_inner(condition, allowed_vars)?;
            let then_e = try_compile_expr_inner(then_branch, allowed_vars)?;
            let else_e = match else_branch {
                Some(e) => Some(Box::new(try_compile_expr_inner(e, allowed_vars)?)),
                None => None,
            };
            Some(CompiledExpr::Conditional {
                condition: Box::new(cond),
                then_expr: Box::new(then_e),
                else_expr: else_e,
            })
        }

        // ── Object construction ─────────────────────────────────────────
        AstNode::Object(pairs) => {
            // Instr::MakeObject's operand is a u16 element count - bail out
            // to the (always-correct, no-limit) tree-walker rather than
            // silently truncating via CompiledExpr::ObjectConstruct here.
            // Duplicate literal keys raise D1009, which the tree-walker reports.
            if pairs.len() > u16::MAX as usize || duplicate_literal_key(pairs).is_some() {
                return None;
            }
            let mut fields = Vec::with_capacity(pairs.len());
            for (key_node, val_node) in pairs {
                // Key must be a string literal
                let key = match key_node {
                    AstNode::String(s) => s.clone(),
                    _ => return None,
                };
                let val = try_compile_expr_inner(val_node, allowed_vars)?;
                fields.push((key, val));
            }
            Some(CompiledExpr::ObjectConstruct(fields))
        }

        // ── Array construction ──────────────────────────────────────────
        AstNode::Array(elems) => {
            // Instr::MakeArray's operand is a u16 element count - bail out
            // to the (always-correct, no-limit) tree-walker rather than
            // silently truncating via CompiledExpr::ArrayConstruct here.
            if elems.len() > u16::MAX as usize {
                return None;
            }
            let mut compiled = Vec::with_capacity(elems.len());
            for elem in elems {
                // Tag whether the element itself is an array constructor: if so, its
                // array value must be kept nested rather than flattened (tree-walker parity).
                let is_nested = is_array_constructor(elem);
                compiled.push((try_compile_expr_inner(elem, allowed_vars)?, is_nested));
            }
            Some(CompiledExpr::ArrayConstruct(compiled))
        }

        // ── Block (sequential evaluation) ───────────────────────────────
        // A block's scope only matters for `:=`, which doesn't compile, so a
        // one-expression block (every `.(expr)` step) compiles to its
        // expression, without a one-element `CompiledExpr::Block` around it.
        AstNode::Block(exprs) if exprs.len() == 1 => {
            try_compile_expr_inner(&exprs[0], allowed_vars)
        }
        AstNode::Block(exprs) if !exprs.is_empty() => {
            // Instr::BlockEnd's operand is a u16 element count - bail out to
            // the (always-correct, no-limit) tree-walker rather than silently
            // truncating via CompiledExpr::Block here. Same pattern as the
            // AstNode::Array guard above.
            if exprs.len() > u16::MAX as usize {
                return None;
            }
            let compiled: Option<Vec<CompiledExpr>> = exprs
                .iter()
                .map(|e| try_compile_expr_inner(e, allowed_vars))
                .collect();
            compiled.map(CompiledExpr::Block)
        }

        // ── Pure builtin function calls ──────────────────────────────────
        AstNode::Function {
            name,
            args,
            is_builtin: true,
        } => {
            // A parameter named like the builtin shadows it:
            // `function($string) { $string(1) }` calls the argument.
            if allowed_vars.is_some_and(|vars| vars.contains(&name.as_str())) {
                return None;
            }
            if is_compilable_builtin(name) {
                // Arity guard: if the call site passes more args than the builtin accepts,
                // fall back to the tree-walker so it can raise the correct T0410 error.
                if let Some(max) = compilable_builtin_max_args(name) {
                    if args.len() > max {
                        return None;
                    }
                }
                // Instr::CallBuiltin's arg_count operand is a u8 - this is NOT
                // already covered by the max-args guard above for variadic
                // builtins like "merge" (compilable_builtin_max_args returns
                // None, i.e. unbounded), so a call site with > 255 arguments
                // would otherwise silently truncate `args.len() as u8`. Bail
                // out to the tree-walker rather than truncate.
                if args.len() > u8::MAX as usize {
                    return None;
                }
                let compiled_args: Option<Vec<CompiledExpr>> = args
                    .iter()
                    .map(|a| try_compile_expr_inner(a, allowed_vars))
                    .collect();
                compiled_args.map(|cargs| CompiledExpr::BuiltinCall {
                    name: static_builtin_name(name),
                    args: cargs,
                })
            } else {
                try_compile_hof_expr(name, args, allowed_vars)
            }
        }

        // Everything else: Lambda, non-pure builtins, Sort, Transform, etc.
        _ => None,
    }
}

/// Extract an inline lambda's params and body from an AST node, returning `None` if the
/// node is not a simple lambda (i.e. has a signature or is a TCO thunk).
fn extract_inline_lambda(node: &AstNode) -> Option<(&Vec<String>, &AstNode)> {
    match node {
        AstNode::Lambda {
            params,
            body,
            signature: None,
            thunk: false,
        } => Some((params, body)),
        _ => None,
    }
}

/// Compile the array argument + lambda body for a HOF call, returning `None` if either
/// fails to compile. The lambda params are added to the allowed-vars set so the body
/// can reference them.
fn compile_hof_array_and_body(
    array_node: &AstNode,
    params: &[String],
    body: &AstNode,
    allowed_vars: Option<&[&str]>,
) -> Option<(Box<CompiledExpr>, Box<CompiledExpr>)> {
    let array = try_compile_expr_inner(array_node, allowed_vars)?;
    // The enclosing lambda's parameters stay visible in the body (the
    // evaluator merges them into the inner call's variables), and shadow
    // builtins there as well.
    let mut param_refs: Vec<&str> = params.iter().map(|s| s.as_str()).collect();
    param_refs.extend(allowed_vars.unwrap_or_default().iter().copied());
    let compiled_body = try_compile_expr_inner(body, Some(&param_refs))?;
    Some((Box::new(array), Box::new(compiled_body)))
}

/// Try to compile a higher-order function call (`$map`, `$filter`, `$reduce`) when the
/// callback argument is an inline lambda literal with a compilable body.
///
/// Returns `None` when:
/// - The callback is not an inline lambda (e.g. a stored variable `$f`) — fall back so
///   the tree-walker can look up the lambda at runtime.
/// - The lambda has a signature or is a TCO thunk — semantics require the full evaluator.
/// - The lambda body is not fully compilable — fall back transparently.
/// - Param count is outside the supported range (see per-function constraints below).
fn try_compile_hof_expr(
    name: &str,
    args: &[AstNode],
    allowed_vars: Option<&[&str]>,
) -> Option<CompiledExpr> {
    match name {
        "map" | "filter" => {
            if args.len() != 2 {
                return None;
            }
            let (params, body) = extract_inline_lambda(&args[1])?;
            if params.is_empty() || params.len() > 2 {
                return None;
            }
            let (array, compiled_body) =
                compile_hof_array_and_body(&args[0], params, body, allowed_vars)?;
            if name == "map" {
                Some(CompiledExpr::MapCall {
                    array,
                    params: params.clone(),
                    body: compiled_body,
                })
            } else {
                Some(CompiledExpr::FilterCall {
                    array,
                    params: params.clone(),
                    body: compiled_body,
                })
            }
        }
        "reduce" => {
            if args.len() < 2 || args.len() > 3 {
                return None;
            }
            let (params, body) = extract_inline_lambda(&args[1])?;
            if params.len() != 2 {
                return None;
            }
            let (array, compiled_body) =
                compile_hof_array_and_body(&args[0], params, body, allowed_vars)?;
            let initial = if args.len() == 3 {
                Some(Box::new(try_compile_expr_inner(&args[2], allowed_vars)?))
            } else {
                None
            };
            Some(CompiledExpr::ReduceCall {
                array,
                params: params.clone(),
                body: compiled_body,
                initial,
            })
        }
        _ => None,
    }
}

/// Returns true if the named builtin is pure (no side effects, no context dependency)
/// and can be safely compiled into a BuiltinCall.
fn is_compilable_builtin(name: &str) -> bool {
    crate::builtins::is_compilable_builtin(name)
}

/// Maximum number of explicit arguments accepted by each compilable builtin.
/// Returns `None` for variadic functions with no fixed upper bound.
/// Used at compile time to fall back to the tree-walker for over-arity calls
/// (which the tree-walker turns into the correct T0410/T0411 type errors).
fn compilable_builtin_max_args(name: &str) -> Option<usize> {
    match name {
        "string" => Some(2),
        "length" | "uppercase" | "lowercase" | "trim" => Some(1),
        "substring" | "split" => Some(3),
        "substringBefore" | "substringAfter" | "contains" | "join" | "append" | "round" => Some(2),
        "number" | "floor" | "ceil" | "abs" | "sqrt" => Some(1),
        "sum" | "max" | "min" | "average" | "count" => Some(1),
        "boolean" | "not" | "keys" | "reverse" | "distinct" => Some(1),
        "merge" => None, // variadic: $merge(obj1, obj2, …) or $merge([…])
        _ => None,
    }
}

/// Return the `&'static str` for a known compilable builtin name.
/// SAFETY: only called after `is_compilable_builtin` returns true.
fn static_builtin_name(name: &str) -> &'static str {
    match name {
        "string" => "string",
        "length" => "length",
        "substring" => "substring",
        "substringBefore" => "substringBefore",
        "substringAfter" => "substringAfter",
        "uppercase" => "uppercase",
        "lowercase" => "lowercase",
        "trim" => "trim",
        "contains" => "contains",
        "split" => "split",
        "join" => "join",
        "number" => "number",
        "floor" => "floor",
        "ceil" => "ceil",
        "round" => "round",
        "abs" => "abs",
        "sqrt" => "sqrt",
        "sum" => "sum",
        "max" => "max",
        "min" => "min",
        "average" => "average",
        "count" => "count",
        "boolean" => "boolean",
        "not" => "not",
        "keys" => "keys",
        "append" => "append",
        "reverse" => "reverse",
        "distinct" => "distinct",
        "merge" => "merge",
        _ => unreachable!("Not a compilable builtin: {}", name),
    }
}

/// Evaluate a compiled expression against a single element.
///
/// `data` is the current element (typically an object from an array).
/// `vars` is an optional map of variable bindings (for HOF lambda parameters).
///
/// This is the tight inner loop — no recursion tracking, no scope push/pop,
/// no AstNode pattern matching.
#[inline(always)]
pub(crate) fn eval_compiled(
    expr: &CompiledExpr,
    data: &JValue,
    vars: Option<&HashMap<&str, &JValue>>,
    options: &EvaluatorOptions,
    start_time: Option<Instant>,
) -> Result<JValue, EvaluatorError> {
    eval_compiled_inner(expr, data, vars, None, None, options, start_time)
}

/// Like `eval_compiled` but with an optional shape cache for O(1) positional
/// field access. The shape cache maps field names to their index in the object's
/// internal Vec, enabling `get_index()` instead of hash lookups.
#[inline(always)]
fn eval_compiled_shaped(
    expr: &CompiledExpr,
    data: &JValue,
    vars: Option<&HashMap<&str, &JValue>>,
    shape: &ShapeCache,
    options: &EvaluatorOptions,
    start_time: Option<Instant>,
) -> Result<JValue, EvaluatorError> {
    eval_compiled_inner(expr, data, vars, None, Some(shape), options, start_time)
}

/// Clone the outer variable bindings into a new HashMap with the given capacity hint.
/// Used by HOF eval arms to create per-iteration variable scopes that merge outer vars
/// with lambda parameters.
#[inline]
fn clone_outer_vars<'a>(
    vars: Option<&HashMap<&'a str, &'a JValue>>,
    capacity: usize,
) -> HashMap<&'a str, &'a JValue> {
    vars.map(|v| v.iter().map(|(&k, v)| (k, *v)).collect())
        .unwrap_or_else(|| HashMap::with_capacity(capacity))
}

/// Evaluate one compiled node, applying jsonata-js's generic unwrap to its
/// result, as the tree-walker's `evaluate_internal` does for every node.
fn eval_compiled_inner(
    expr: &CompiledExpr,
    data: &JValue,
    vars: Option<&HashMap<&str, &JValue>>,
    ctx: Option<&Context>,
    shape: Option<&ShapeCache>,
    options: &EvaluatorOptions,
    start_time: Option<Instant>,
) -> Result<JValue, EvaluatorError> {
    eval_compiled_node(expr, data, vars, ctx, shape, options, start_time)
        .map(JValue::unwrap_sequence)
}

fn eval_compiled_node(
    expr: &CompiledExpr,
    data: &JValue,
    vars: Option<&HashMap<&str, &JValue>>,
    ctx: Option<&Context>,
    shape: Option<&ShapeCache>,
    options: &EvaluatorOptions,
    start_time: Option<Instant>,
) -> Result<JValue, EvaluatorError> {
    // Single, structurally-unbypassable D1012 checkpoint for the entire compiled fast
    // path. Every route into compiled evaluation -- the VM's EvalFallback, this task's
    // MapCall/FilterCall/ReduceCall loop bodies, invoke_stored_lambda's compiled fast
    // path, evaluate_function_call's inline $map/$filter fast-path loops, and any future
    // caller -- funnels through this one function (both eval_compiled and
    // eval_compiled_shaped delegate here), so checking once at entry covers all of them
    // without having to enumerate call sites. Deliberately timeout-only, no depth check:
    // self-recursive lambdas cannot compile to CompiledExpr, so genuine recursion always
    // routes through evaluate_internal's own (already guarded) recursion-depth counter.
    check_loop_timeout(options, start_time)?;
    match expr {
        // ── Leaves ──────────────────────────────────────────────────────
        CompiledExpr::Literal(v) => Ok(v.clone()),

        CompiledExpr::FieldLookup(field) => match data {
            JValue::Object(obj) => Ok(shaped_get(obj, field, shape)
                .cloned()
                .unwrap_or(JValue::Undefined)),
            #[cfg(feature = "python")]
            JValue::LazyPyDict(lazy) => Ok(lazy.get_field(field)?),
            JValue::Array(_) => {
                field_chain_by_path_rules(&[field], data, vars, ctx, options, start_time)
            }
            _ => Ok(JValue::Undefined),
        },

        CompiledExpr::NestedFieldLookup(outer, inner) => match data {
            JValue::Object(obj) => match shaped_get(obj, outer, shape) {
                Some(JValue::Object(nested)) => Ok(nested
                    .get(inner.as_str())
                    .cloned()
                    .unwrap_or(JValue::Undefined)),
                #[cfg(feature = "python")]
                Some(JValue::LazyPyDict(nested)) => Ok(nested.get_field(inner.as_str())?),
                Some(JValue::Array(_)) => {
                    field_chain_by_path_rules(&[outer, inner], data, vars, ctx, options, start_time)
                }
                _ => Ok(JValue::Undefined),
            },
            #[cfg(feature = "python")]
            JValue::LazyPyDict(lazy) => {
                let outer_val = lazy.get_field(outer.as_str())?;
                match outer_val {
                    JValue::Object(nested) => Ok(nested
                        .get(inner.as_str())
                        .cloned()
                        .unwrap_or(JValue::Undefined)),
                    JValue::LazyPyDict(nested) => Ok(nested.get_field(inner.as_str())?),
                    JValue::Array(_) => field_chain_by_path_rules(
                        &[outer, inner],
                        data,
                        vars,
                        ctx,
                        options,
                        start_time,
                    ),
                    _ => Ok(JValue::Undefined),
                }
            }
            JValue::Array(_) => {
                field_chain_by_path_rules(&[outer, inner], data, vars, ctx, options, start_time)
            }
            _ => Ok(JValue::Undefined),
        },

        CompiledExpr::VariableLookup(var) => {
            if let Some(vars) = vars {
                if let Some(val) = vars.get(var.as_str()) {
                    return Ok((*val).clone());
                }
            }
            // $ (empty var name) refers to the current data
            if var.is_empty() {
                return Ok(data.bare());
            }
            Ok(JValue::Undefined)
        }

        // ── Comparison ──────────────────────────────────────────────────
        CompiledExpr::Compare { op, lhs, rhs } => {
            let left = eval_compiled_inner(lhs, data, vars, ctx, shape, options, start_time)?;
            let right = eval_compiled_inner(rhs, data, vars, ctx, shape, options, start_time)?;
            match op {
                // compiled_equal normalizes lazy operands (guarded, zero-cost when
                // neither side is lazy) so conversion failures raise instead of
                // silently comparing unequal.
                CompiledCmp::Eq => compiled_equal(&left, &right),
                CompiledCmp::Ne => compiled_not_equal(&left, &right),
                CompiledCmp::Lt => {
                    compiled_ordered_cmp(&left, &right, "<", |a, b| a < b, |a, b| a < b)
                }
                CompiledCmp::Le => {
                    compiled_ordered_cmp(&left, &right, "<=", |a, b| a <= b, |a, b| a <= b)
                }
                CompiledCmp::Gt => {
                    compiled_ordered_cmp(&left, &right, ">", |a, b| a > b, |a, b| a > b)
                }
                CompiledCmp::Ge => {
                    compiled_ordered_cmp(&left, &right, ">=", |a, b| a >= b, |a, b| a >= b)
                }
            }
        }

        // ── Arithmetic ──────────────────────────────────────────────────
        CompiledExpr::Arithmetic { op, lhs, rhs } => {
            let left = eval_compiled_inner(lhs, data, vars, ctx, shape, options, start_time)?;
            let right = eval_compiled_inner(rhs, data, vars, ctx, shape, options, start_time)?;
            compiled_arithmetic(*op, &left, &right)
        }

        // ── String concat ───────────────────────────────────────────────
        CompiledExpr::Concat(lhs, rhs) => {
            let left = eval_compiled_inner(lhs, data, vars, ctx, shape, options, start_time)?;
            let right = eval_compiled_inner(rhs, data, vars, ctx, shape, options, start_time)?;
            let ls = compiled_to_concat_string(&left)?;
            let rs = compiled_to_concat_string(&right)?;
            Ok(JValue::string(format!("{}{}", ls, rs)))
        }

        // ── Logical ─────────────────────────────────────────────────────
        CompiledExpr::And(lhs, rhs) => {
            let left = eval_compiled_inner(lhs, data, vars, ctx, shape, options, start_time)?;
            if !compiled_is_truthy(&left) {
                return Ok(JValue::Bool(false));
            }
            let right = eval_compiled_inner(rhs, data, vars, ctx, shape, options, start_time)?;
            Ok(JValue::Bool(compiled_is_truthy(&right)))
        }
        CompiledExpr::Or(lhs, rhs) => {
            let left = eval_compiled_inner(lhs, data, vars, ctx, shape, options, start_time)?;
            if compiled_is_truthy(&left) {
                return Ok(JValue::Bool(true));
            }
            let right = eval_compiled_inner(rhs, data, vars, ctx, shape, options, start_time)?;
            Ok(JValue::Bool(compiled_is_truthy(&right)))
        }
        CompiledExpr::Not(inner) => {
            let val = eval_compiled_inner(inner, data, vars, ctx, shape, options, start_time)?;
            Ok(JValue::Bool(!compiled_is_truthy(&val)))
        }
        CompiledExpr::Negate(inner) => {
            let val = eval_compiled_inner(inner, data, vars, ctx, shape, options, start_time)?;
            match val {
                JValue::Number(n) => Ok(JValue::Number(-n)),
                // Only *undefined* propagates; null is D1002 like any other
                // non-number, matching the tree-walker and jsonata-js.
                v if v.is_undefined() => Ok(JValue::Undefined),
                _ => Err(EvaluatorError::TypeError(
                    "D1002: Cannot negate non-number value".to_string(),
                )),
            }
        }

        // ── Conditional ─────────────────────────────────────────────────
        CompiledExpr::Conditional {
            condition,
            then_expr,
            else_expr,
        } => {
            let cond = eval_compiled_inner(condition, data, vars, ctx, shape, options, start_time)?;
            if compiled_is_truthy(&cond) {
                eval_compiled_inner(then_expr, data, vars, ctx, shape, options, start_time)
            } else if let Some(else_e) = else_expr {
                eval_compiled_inner(else_e, data, vars, ctx, shape, options, start_time)
            } else {
                Ok(JValue::Undefined)
            }
        }

        // ── Object construction ─────────────────────────────────────────
        CompiledExpr::ObjectConstruct(fields) => {
            // Keys are literals, so every item of the input is in each
            // pair's group.
            let context = object_context(data);
            let shape = match context {
                Cow::Borrowed(_) => shape,
                Cow::Owned(_) => None,
            };
            let mut result = IndexMap::with_capacity(fields.len());
            for (key, expr) in fields {
                let value =
                    eval_compiled_inner(expr, &context, vars, ctx, shape, options, start_time)?;
                if !value.is_undefined() {
                    result.insert(key.clone(), value);
                }
            }
            Ok(JValue::object(result))
        }

        // ── Array construction ──────────────────────────────────────────
        CompiledExpr::ArrayConstruct(elems) => {
            let mut result = Vec::new();
            for (elem_expr, is_nested) in elems {
                let value =
                    eval_compiled_inner(elem_expr, data, vars, ctx, shape, options, start_time)?;
                // Undefined values are excluded from array constructors (tree-walker parity)
                if value.is_undefined() {
                    continue;
                }
                if *is_nested {
                    // Explicit array constructor [...] — keep nested even if it's an array
                    result.push(value);
                } else if let JValue::Array(arr) = value {
                    // Non-constructor that evaluated to an array — flatten one level
                    result.extend(arr.iter().cloned());
                } else {
                    result.push(value);
                }
            }
            Ok(JValue::array(result))
        }

        // ── Phase 2 new variants ─────────────────────────────────────────
        CompiledExpr::FieldPath { steps, from_dollar } => {
            compiled_eval_field_path(steps, *from_dollar, data, vars, ctx, options, start_time)
        }

        // BuiltinCall: evaluate all args, dispatch to pure builtin.
        CompiledExpr::BuiltinCall { name, args } => {
            let mut evaled_args = Vec::with_capacity(args.len());
            for arg in args.iter() {
                evaled_args.push(eval_compiled_inner(
                    arg, data, vars, ctx, shape, options, start_time,
                )?);
            }
            crate::builtins::dispatch_pure(name, &evaled_args, data, options)
        }

        // Block: evaluate each expression in sequence, return the last value.
        CompiledExpr::Block(exprs) => {
            let mut result = JValue::Undefined;
            for expr in exprs.iter() {
                result = eval_compiled_inner(expr, data, vars, ctx, shape, options, start_time)?;
            }
            Ok(result)
        }

        // Coalesce (`??`): return lhs unless it is Undefined; null IS a valid value.
        // JSONata spec: "returns the RHS operand if the LHS operand evaluates to undefined".
        CompiledExpr::Coalesce(lhs, rhs) => {
            let left = eval_compiled_inner(lhs, data, vars, ctx, shape, options, start_time)?;
            if left.is_undefined() {
                eval_compiled_inner(rhs, data, vars, ctx, shape, options, start_time)
            } else {
                Ok(left)
            }
        }

        // ── Higher-order functions ─────────────────────────────────────────────
        //
        // These variants are emitted by try_compile_hof_expr when the HOF argument
        // is an inline lambda literal with a compilable body. Outer vars are merged
        // with the lambda params so that nested HOF can access variables from
        // enclosing lambda scopes (e.g. `$map(a, function($x) { $map(b, function($y) { $x + $y }) })`).
        CompiledExpr::MapCall {
            array,
            params,
            body,
        } => {
            let arr_val = eval_compiled_inner(array, data, vars, ctx, shape, options, start_time)?;
            let single_holder;
            let items: &[JValue] = match &arr_val {
                JValue::Array(a) => a.as_slice(),
                JValue::Undefined => return Ok(JValue::Undefined),
                other => {
                    single_holder = [other.clone()];
                    &single_holder[..]
                }
            };
            let mut result = Vec::with_capacity(items.len());
            let p0 = params.first().map(|s| s.as_str());

            if let Some(p1) = params.get(1).map(|s| s.as_str()) {
                // 2-param lambda (element + index): build per-iteration because idx_val
                // is loop-local and cannot outlive the iteration.
                for (idx, item) in items.iter().enumerate() {
                    check_loop_timeout(options, start_time)?;
                    let idx_val = JValue::Number(idx as f64);
                    let mut call_vars = clone_outer_vars(vars, 2);
                    if let Some(p) = p0 {
                        call_vars.insert(p, item);
                    }
                    call_vars.insert(p1, &idx_val);
                    let mapped = eval_compiled_inner(
                        body,
                        data,
                        Some(&call_vars),
                        ctx,
                        shape,
                        options,
                        start_time,
                    )?;
                    if !mapped.is_undefined() {
                        result.push(mapped);
                    }
                }
            } else if let Some(p0) = p0 {
                // 1-param lambda (most common): build HashMap once, update element ref each iteration.
                let mut call_vars = clone_outer_vars(vars, 1);
                for item in items.iter() {
                    check_loop_timeout(options, start_time)?;
                    call_vars.insert(p0, item);
                    let mapped = eval_compiled_inner(
                        body,
                        data,
                        Some(&call_vars),
                        ctx,
                        shape,
                        options,
                        start_time,
                    )?;
                    if !mapped.is_undefined() {
                        result.push(mapped);
                    }
                }
            }
            // Sequence semantics, same as the tree-walker's `$map`: an empty
            // result is undefined and a single result unwraps.
            hof_result_sequence(result, options)
        }

        CompiledExpr::FilterCall {
            array,
            params,
            body,
        } => {
            let arr_val = eval_compiled_inner(array, data, vars, ctx, shape, options, start_time)?;
            if arr_val.is_undefined() || arr_val.is_null() {
                return Ok(JValue::Undefined);
            }
            let single_holder;
            let (items, was_single) = match &arr_val {
                JValue::Array(a) => (a.as_slice(), false),
                other => {
                    single_holder = [other.clone()];
                    (&single_holder[..], true)
                }
            };
            let mut result = Vec::with_capacity(items.len() / 2);
            let p0 = params.first().map(|s| s.as_str());

            if let Some(p1) = params.get(1).map(|s| s.as_str()) {
                for (idx, item) in items.iter().enumerate() {
                    check_loop_timeout(options, start_time)?;
                    let idx_val = JValue::Number(idx as f64);
                    let mut call_vars = clone_outer_vars(vars, 2);
                    if let Some(p) = p0 {
                        call_vars.insert(p, item);
                    }
                    call_vars.insert(p1, &idx_val);
                    let pred = eval_compiled_inner(
                        body,
                        data,
                        Some(&call_vars),
                        ctx,
                        shape,
                        options,
                        start_time,
                    )?;
                    if compiled_is_truthy(&pred) {
                        result.push(item.clone());
                    }
                }
            } else if let Some(p0) = p0 {
                let mut call_vars = clone_outer_vars(vars, 1);
                for item in items.iter() {
                    check_loop_timeout(options, start_time)?;
                    call_vars.insert(p0, item);
                    let pred = eval_compiled_inner(
                        body,
                        data,
                        Some(&call_vars),
                        ctx,
                        shape,
                        options,
                        start_time,
                    )?;
                    if compiled_is_truthy(&pred) {
                        result.push(item.clone());
                    }
                }
            }
            // `was_single` used to gate the unwrap; the sequence rule is the
            // same whether the input was an array or a lone value.
            let _ = was_single;
            hof_result_sequence(result, options)
        }

        CompiledExpr::ReduceCall {
            array,
            params,
            body,
            initial,
        } => {
            let arr_val = eval_compiled_inner(array, data, vars, ctx, shape, options, start_time)?;
            let single_holder;
            let items: &[JValue] = match &arr_val {
                JValue::Array(a) => a.as_slice(),
                JValue::Null => return Ok(JValue::Null),
                JValue::Undefined => return Ok(JValue::Undefined),
                other => {
                    single_holder = [other.clone()];
                    &single_holder[..]
                }
            };
            let (start_idx, mut accumulator) = if let Some(init_expr) = initial {
                let init_val =
                    eval_compiled_inner(init_expr, data, vars, ctx, shape, options, start_time)?;
                if items.is_empty() {
                    return Ok(init_val);
                }
                (0usize, init_val)
            } else {
                if items.is_empty() {
                    return Ok(JValue::Null);
                }
                (1, items[0].clone())
            };
            let acc_param = params[0].as_str();
            let item_param = params[1].as_str();
            for item in items[start_idx..].iter() {
                check_loop_timeout(options, start_time)?;
                // Per-iteration HashMap: &accumulator borrow must be released before we
                // can reassign `accumulator`. `drop(call_vars)` ends the borrow.
                let mut call_vars = clone_outer_vars(vars, 2);
                call_vars.insert(acc_param, &accumulator);
                call_vars.insert(item_param, item);
                let new_acc = eval_compiled_inner(
                    body,
                    data,
                    Some(&call_vars),
                    ctx,
                    shape,
                    options,
                    start_time,
                )?;
                drop(call_vars);
                accumulator = new_acc;
            }
            Ok(accumulator)
        }
    }
}

/// JavaScript truthiness, which jsonata-js's `$sort` applies to its
/// comparator's result (`if (await comp(left[0], right[0]))`) instead of
/// JSONata's `$boolean`: any array, object or function is true, even an
/// empty one, so a comparator returning `[]` or `{}` swaps.
fn js_truthy(value: &JValue) -> bool {
    match value {
        JValue::Null | JValue::Undefined => false,
        JValue::Bool(b) => *b,
        JValue::Number(n) => *n != 0.0 && !n.is_nan(),
        JValue::String(s) => !s.is_empty(),
        _ => true,
    }
}

/// Truthiness check (matches JSONata semantics). Standalone function for compiled path.
#[inline]
pub(crate) fn compiled_is_truthy(value: &JValue) -> bool {
    match value {
        JValue::Null | JValue::Undefined => false,
        JValue::Bool(b) => *b,
        JValue::Number(n) => *n != 0.0,
        JValue::String(s) => !s.is_empty(),
        // Recursive, matching `Evaluator::is_truthy` and `$boolean` (#111).
        JValue::Array(a) => with_stack(|| match a.len() {
            0 => false,
            1 => compiled_is_truthy(&a[0]),
            _ => a.iter().any(compiled_is_truthy),
        }),
        JValue::Object(o) => !o.is_empty(),
        // A Python dict arrives as a lazy view, not a materialised Object.
        // Without this arm it fell through to `_ => false` and every non-empty
        // dict was falsy on the compiled path -- mirrors `Evaluator::is_truthy`.
        #[cfg(feature = "python")]
        JValue::LazyPyDict(lazy) => !lazy.is_empty(),
        _ => false,
    }
}

/// Decide whether a filter predicate result selects the element at `index`.
///
/// In JSONata a predicate that evaluates to a number is an *index selector*,
/// not a truthiness test: the element is kept only when the number equals its
/// own position. Negative values count from the end and fractional values
/// floor, matching jsonata-js's `evaluateFilter`. An array whose elements are
/// all numbers is a set of such indices, and any match keeps the element --
/// which makes an empty array vacuously numeric and matching nothing.
///
/// Returns `None` when the result is not an index selector, leaving the caller
/// to apply its own truthiness rule.
///
/// This exists so the four filter implementations (the tree-walker's predicate
/// step and stage forms, the compiled path, and the VM) share one copy of the
/// decision while keeping their own evaluation strategies. Each previously
/// carried its own `is_truthy` call and so was wrong in the same way.
#[inline]
/// How many times a positional predicate selects the element at `index`.
///
/// `None` means the predicate is not a positional selector at all and the
/// caller should fall back to truthiness.
///
/// This counts rather than answering yes/no because jsonata-js pushes the
/// item once per *matching selector*, not once per matching element: it walks
/// the selector array with `forEach` and pushes on every hit. So `[0, 0]`
/// selects element 0 twice and `nums[[0,0]]` is `[10, 10]`, where an
/// `any()`-style membership test can only ever yield each element once.
pub(crate) fn predicate_index_match(pred: &JValue, index: usize, len: usize) -> Option<usize> {
    fn matches_index(n: f64, index: usize, len: usize) -> bool {
        let mut i = n.floor() as i64;
        if i < 0 {
            i += len as i64;
        }
        i >= 0 && i as usize == index
    }

    match pred {
        JValue::Number(n) => Some(usize::from(matches_index(*n, index, len))),
        JValue::Array(arr) if arr.iter().all(|v| matches!(v, JValue::Number(_))) => Some(
            arr.iter()
                .filter(|v| match v {
                    JValue::Number(n) => matches_index(*n, index, len),
                    _ => false,
                })
                .count(),
        ),
        _ => None,
    }
}

/// Ordered comparison shared by the tree-walker, the compiled path, and the
/// bytecode VM, so all three report identical errors.
///
/// jsonata-js's rule: only numbers, strings and *undefined* are comparable,
/// so anything else -- null, boolean, object, array -- raises T2010; an
/// undefined operand then makes the result undefined; and only after that
/// does a differing type raise T2009 (with `op_symbol` in the message).
#[inline]
pub(crate) fn compiled_ordered_cmp(
    left: &JValue,
    right: &JValue,
    op_symbol: &str,
    cmp_num: fn(f64, f64) -> bool,
    cmp_str: fn(&str, &str) -> bool,
) -> Result<JValue, EvaluatorError> {
    fn comparable(v: &JValue) -> bool {
        matches!(v, JValue::Number(_) | JValue::String(_) | JValue::Undefined)
    }

    if !comparable(left) || !comparable(right) {
        return Err(EvaluatorError::EvaluationError(format!(
            "T2010: Cannot compare {} and {}",
            Evaluator::type_name(left),
            Evaluator::type_name(right)
        )));
    }

    // An undefined operand makes the comparison undefined, not an error.
    if matches!(left, JValue::Undefined) || matches!(right, JValue::Undefined) {
        return Ok(JValue::Undefined);
    }

    match (left, right) {
        (JValue::Number(a), JValue::Number(b)) => Ok(JValue::Bool(cmp_num(*a, *b))),
        (JValue::String(a), JValue::String(b)) => Ok(JValue::Bool(cmp_str(a, b))),
        _ => Err(EvaluatorError::EvaluationError(format!(
            "T2009: The expressions on either side of operator \"{}\" must be of the same data type",
            op_symbol
        ))),
    }
}

/// Arithmetic for compiled expressions.
/// Mirrors the tree-walker's arithmetic functions including explicit-null semantics.
#[inline]
pub(crate) fn compiled_arithmetic(
    op: CompiledArithOp,
    left: &JValue,
    right: &JValue,
) -> Result<JValue, EvaluatorError> {
    let op_sym = match op {
        CompiledArithOp::Add => "+",
        CompiledArithOp::Sub => "-",
        CompiledArithOp::Mul => "*",
        CompiledArithOp::Div => "/",
        CompiledArithOp::Mod => "%",
    };
    // jsonata-js's `isNumeric` throws D1001 when an *operand* is Infinity. It
    // must run before the numeric arm below, or `1/(10e300 * 10e100)` quietly
    // computes 1/inf = 0 instead of raising. NaN operands are reported as
    // non-numeric instead and fall through to T2001/T2002.
    if matches!(left, JValue::Number(n) if n.is_infinite())
        || matches!(right, JValue::Number(n) if n.is_infinite())
    {
        return Err(EvaluatorError::EvaluationError(
            "D1001: Number out of range".to_string(),
        ));
    }

    match (left, right) {
        (JValue::Number(a), JValue::Number(b)) => {
            let result = match op {
                CompiledArithOp::Add => *a + *b,
                CompiledArithOp::Sub => *a - *b,
                // No check on the *result*: jsonata-js returns Infinity for
                // `1/0` and `10e300 * 10e100`, and NaN for `0/0` and `1%0`. The
                // error surfaces when such a value is later used as an operand
                // (see the D1001 guard above), matching `isNumeric`.
                CompiledArithOp::Mul => *a * *b,
                CompiledArithOp::Div => *a / *b,
                CompiledArithOp::Mod => *a % *b,
            };
            Ok(JValue::Number(result))
        }
        // Order matters, and matches jsonata-js: type-check each *defined*
        // operand first, then propagate undefined. `false + $x` raises T2001 on
        // the boolean even though the right side is undefined.
        //
        // Only undefined propagates. An explicit null -- like any other
        // non-number -- is a type error, whether written as a literal or
        // arriving at runtime from data or a lambda parameter (the
        // null/undefined split, #32, made runtime nulls unambiguous).
        _ if !matches!(left, JValue::Number(n) if !n.is_nan())
            && !matches!(left, JValue::Undefined) =>
        {
            Err(EvaluatorError::TypeError(format!(
                "T2001: The left side of the {} operator must evaluate to a number",
                op_sym
            )))
        }
        _ if !matches!(right, JValue::Number(n) if !n.is_nan())
            && !matches!(right, JValue::Undefined) =>
        {
            Err(EvaluatorError::TypeError(format!(
                "T2002: The right side of the {} operator must evaluate to a number",
                op_sym
            )))
        }
        // At least one operand is undefined and neither is a bad type.
        _ => Ok(JValue::Undefined),
    }
}

/// Convert a value to string for concatenation in compiled expressions.
#[inline]
pub(crate) fn compiled_to_concat_string(value: &JValue) -> Result<String, EvaluatorError> {
    // Normalize a lazy operand up front: `functions::string::string`'s lazy arm maps a
    // conversion failure to `JValue::Null` (silently stringifying to `""`), which would
    // swallow the TypeError this must raise instead. Guarded by `is_lazy` so the common
    // (non-lazy) path pays no clone.
    let normalized;
    let value = if value.is_lazy() {
        normalized = normalize_lazy(value)?;
        &normalized
    } else {
        value
    };
    match value {
        JValue::String(s) => Ok(s.to_string()),
        // An explicit null stringifies as "null" (same as `$string(null)`);
        // only an *undefined* operand contributes nothing.
        JValue::Undefined => Ok(String::new()),
        JValue::Null
        | JValue::Number(_)
        | JValue::Bool(_)
        | JValue::Array(_)
        | JValue::Object(_) => match crate::functions::string::string(value, None) {
            Ok(JValue::String(s)) => Ok(s.to_string()),
            Ok(JValue::Null) => Ok(String::new()),
            _ => Err(EvaluatorError::TypeError(
                "Cannot concatenate complex types".to_string(),
            )),
        },
        _ => Ok(String::new()),
    }
}

/// Equality comparison for the bytecode VM.
#[inline]
pub(crate) fn compiled_equal(lhs: &JValue, rhs: &JValue) -> Result<JValue, EvaluatorError> {
    // Normalize lazy operands so a conversion failure raises TypeError here rather than
    // being swallowed as `false` by `values_equal`'s lazy `to_object_ref` arms. Guarded
    // by `is_lazy` so the common (non-lazy) path pays no clone.
    if lhs.is_lazy() || rhs.is_lazy() {
        let lhs = normalize_lazy(lhs)?;
        let rhs = normalize_lazy(rhs)?;
        return Ok(JValue::Bool(crate::functions::array::values_equal(
            &lhs, &rhs,
        )));
    }
    Ok(JValue::Bool(crate::functions::array::values_equal(
        lhs, rhs,
    )))
}

/// Inequality. NOT simply the negation of `compiled_equal`: jsonata-js returns
/// false for both `=` and `!=` when either operand is undefined, so an element
/// whose field is missing does not survive `arr[p != null]`.
#[inline]
pub(crate) fn compiled_not_equal(lhs: &JValue, rhs: &JValue) -> Result<JValue, EvaluatorError> {
    if lhs.is_undefined() || rhs.is_undefined() {
        return Ok(JValue::Bool(false));
    }
    match compiled_equal(lhs, rhs)? {
        JValue::Bool(b) => Ok(JValue::Bool(!b)),
        other => Ok(other),
    }
}

/// jsonata-js's `hofFuncArgs` argument shaping, shared by the array HOFs
/// ($map/$filter/$single): the element is always passed regardless of the
/// callback's declared arity (a 0-arity callback still gets 1 argument, not
/// 0, matching JS); the index is added at arity 2, and the whole array at
/// arity >= 3 (the caller pre-builds `arr_value` exactly when needed).
fn hof_array_call_args(
    item: &JValue,
    index: usize,
    arr_value: Option<&JValue>,
    param_count: usize,
) -> Vec<JValue> {
    match param_count {
        0 | 1 => vec![item.clone()],
        2 => vec![item.clone(), JValue::Number(index as f64)],
        _ => vec![
            item.clone(),
            JValue::Number(index as f64),
            arr_value
                .expect("caller builds arr_value when param_count >= 3")
                .clone(),
        ],
    }
}

/// `hofFuncArgs` shaping for the object HOFs ($sift/$each): the value always,
/// the key at arity 2, and the whole object at arity >= 3.
fn hof_object_call_args(
    value: &JValue,
    key: &str,
    obj_value: Option<&JValue>,
    param_count: usize,
) -> Vec<JValue> {
    match param_count {
        0 | 1 => vec![value.clone()],
        2 => vec![value.clone(), JValue::string(key)],
        _ => vec![
            value.clone(),
            JValue::string(key),
            obj_value
                .expect("caller builds obj_value when param_count >= 3")
                .clone(),
        ],
    }
}

/// Parse a lambda's declared signature and validate/coerce `values` against
/// it, translating `SignatureError` into the reference's T0410/T0411/T0412
/// error codes. Shared by the direct (`invoke_lambda_with_env`) and TCO
/// (`invoke_lambda_body_for_tco`) invocation paths so the two cannot drift.
fn coerce_lambda_args(
    sig_str: &str,
    values: &[JValue],
    data: &JValue,
) -> Result<Vec<JValue>, EvaluatorError> {
    use crate::signature::SignatureError;
    let sig = crate::signature::Signature::parse(sig_str)
        .map_err(|e| EvaluatorError::EvaluationError(format!("Invalid signature: {}", e)))?;
    sig.validate_and_coerce(values, data).map_err(|e| match e {
        SignatureError::ArgumentTypeMismatch { index, expected } => EvaluatorError::TypeError(
            format!("T0410: Argument {} of function does not match function signature (expected {})", index, expected),
        ),
        SignatureError::ArrayTypeMismatch { index, expected } => EvaluatorError::TypeError(
            format!("T0412: Argument {} of function must be an array of {}", index, expected),
        ),
        SignatureError::ContextTypeMismatch { index, expected } => EvaluatorError::TypeError(
            format!("T0411: Context value at argument {} does not match function signature (expected {})", index, expected),
        ),
        other => EvaluatorError::TypeError(format!("Signature validation failed: {}", other)),
    })
}

/// String concatenation for the bytecode VM (its only caller is `vm.rs`).
#[inline]
pub(crate) fn compiled_concat(lhs: JValue, rhs: JValue) -> Result<JValue, EvaluatorError> {
    let l = compiled_to_concat_string(&lhs)?;
    let r = compiled_to_concat_string(&rhs)?;
    Ok(JValue::string(l + &r))
}

// ──────────────────────────────────────────────────────────────────────────────
// Phase 2: path compilation, builtin dispatch, and supporting helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Compile a `Path { steps }` AstNode into a `CompiledExpr`.
///
/// Handles paths like `a.b.c`, `a[pred].b`, `$var.field`.
/// Returns `None` if any step is not compilable (e.g. wildcards, function apps).
fn try_compile_path(
    steps: &[crate::ast::PathStep],
    allowed_vars: Option<&[&str]>,
) -> Option<CompiledExpr> {
    use crate::ast::{AstNode, Stage};

    if steps.is_empty() {
        return None;
    }

    // Determine the start of the path:
    //   `$.field...`  → starts from current data (drop the leading `$` step)
    //   `$var.field`  → variable-prefixed paths: not compiled yet, fall back to tree-walker
    //   `field...`    → starts from current data
    let (field_steps, from_dollar): (&[crate::ast::PathStep], bool) = match &steps[0].node {
        AstNode::Variable(var) if var.is_empty() && steps[0].stages.is_empty() => {
            (&steps[1..], true)
        }
        AstNode::Variable(_) => return None,
        AstNode::Name(_) => (steps, false),
        _ => return None,
    };

    // Compile a filter predicate. A numeric literal (`[0]`, `[-1]`) selects a
    // single item by position, which the tree-walker's `evaluate_filter`
    // handles, so it is left uncompiled.
    let compile_filter = |node: &AstNode| -> Option<CompiledExpr> {
        if numeric_literal(node).is_some() {
            return None;
        }
        try_compile_expr_inner(node, allowed_vars)
    };

    // Compile each field step: a Name with at most one filter stage.
    let mut compiled_steps = Vec::with_capacity(field_steps.len());
    for step in field_steps {
        // Tuple-stream steps (@ focus / # index / % parent binding) require the
        // tree-walker's tuple machinery (create_tuple_stream / evaluate_path's
        // tuple handling). Never compile them to the flat bytecode field path,
        // which is unaware of the binding flags and would silently drop them.
        if step.focus.is_some()
            || step.index_var.is_some()
            || step.ancestor_label.is_some()
            || step.is_tuple
        {
            return None;
        }
        match &step.node {
            AstNode::Name(name) => {
                let filter = match step.stages.as_slice() {
                    [] => None,
                    [Stage::Filter(filter_node)] => Some(compile_filter(filter_node)?),
                    _ => return None,
                };
                compiled_steps.push(CompiledStep {
                    field: name.clone(),
                    filter,
                });
            }
            _ => return None,
        }
    }

    if compiled_steps.is_empty() {
        // Bare `$` with no further field steps — current-data reference
        return Some(CompiledExpr::VariableLookup(String::new()));
    }

    // Shape-cache optimizations (FieldLookup / NestedFieldLookup) serve HOF
    // mode (allowed_vars=Some), where data is usually one object element of
    // an array; they fall back to the path rules on an array.
    if allowed_vars.is_some() {
        if compiled_steps.len() == 1 && compiled_steps[0].filter.is_none() {
            return Some(CompiledExpr::FieldLookup(compiled_steps.remove(0).field));
        }
        if compiled_steps.len() == 2
            && compiled_steps[0].filter.is_none()
            && compiled_steps[1].filter.is_none()
        {
            let outer = compiled_steps.remove(0).field;
            let inner = compiled_steps.remove(0).field;
            return Some(CompiledExpr::NestedFieldLookup(outer, inner));
        }
    }

    Some(CompiledExpr::FieldPath {
        steps: compiled_steps,
        from_dollar,
    })
}

/// Evaluate a compiled `FieldPath` against `data`, following jsonata-js
/// `evaluatePath` and `evaluateStep` for a path of field steps: each step
/// looks its field up in each item of the previous step's result and applies
/// its filter to that item's value, then the values are combined with the
/// last-step rule (`collect_step_results`), exactly as `evaluate_path` does.
/// A chain of plain field steps over a value the `FieldLookup` and
/// `NestedFieldLookup` fast paths don't cover, by the general path rules.
fn field_chain_by_path_rules(
    fields: &[&String],
    data: &JValue,
    vars: Option<&HashMap<&str, &JValue>>,
    ctx: Option<&Context>,
    options: &EvaluatorOptions,
    start_time: Option<Instant>,
) -> Result<JValue, EvaluatorError> {
    let steps: Vec<CompiledStep> = fields
        .iter()
        .map(|field| CompiledStep {
            field: (*field).clone(),
            filter: None,
        })
        .collect();
    compiled_eval_field_path(&steps, false, data, vars, ctx, options, start_time)
}

fn compiled_eval_field_path(
    steps: &[CompiledStep],
    from_dollar: bool,
    data: &JValue,
    vars: Option<&HashMap<&str, &JValue>>,
    ctx: Option<&Context>,
    options: &EvaluatorOptions,
    start_time: Option<Instant>,
) -> Result<JValue, EvaluatorError> {
    let per_item = matches!(data, JValue::Array(arr) if from_dollar || !arr.outer_wrapper);
    let (start, mut result) = if per_item {
        (0, JValue::Undefined)
    } else {
        match walk_single_values(steps.iter().map(|s| (&*s.field, s.filter.is_none())), data)? {
            Some(walked) => walked,
            None => return Ok(JValue::Undefined),
        }
    };
    if start == steps.len() {
        return Ok(result);
    }
    for (index, step) in steps.iter().enumerate().skip(start) {
        let previous;
        let items: &[JValue] = if index == 0 && per_item {
            data.as_array().map_or(&[][..], Vec::as_slice)
        } else if index == 0 {
            std::slice::from_ref(data)
        } else {
            match &result {
                JValue::Array(arr) if !arr.is_empty() => {
                    previous = arr.clone();
                    &previous
                }
                JValue::Undefined => break,
                single => std::slice::from_ref(single),
            }
        };
        let mut values = field_values(&step.field, items, options)?;
        if let Some(filter) = &step.filter {
            // One shape cache for the step, from its first object: the
            // filtered values usually hold objects of one shape.
            let shape = values
                .iter()
                .find_map(|value| match value {
                    JValue::Array(arr) => arr.first(),
                    JValue::Undefined => None,
                    other => Some(other),
                })
                .and_then(build_shape_cache);
            let mut filtered = Vec::with_capacity(values.len());
            for value in values {
                let value = compiled_apply_filter(
                    filter,
                    &value,
                    shape.as_ref(),
                    vars,
                    ctx,
                    options,
                    start_time,
                )?;
                if !value.is_undefined() {
                    filtered.push(value);
                }
            }
            values = filtered;
        }
        check_sequence_length(values.len(), options)?;
        result = collect_step_results(values, index + 1 == steps.len());
    }
    Ok(result)
}

/// Walk the leading `(field, plain)` steps of a path over a single input
/// value, looking each field up directly while the value is a single plain
/// object and the step is `plain` (has no filter or other stage). Returns how
/// many steps were taken and the value reached, which the general step loop
/// continues from, or `None` once a field is missing (the path is
/// undefined). This is exactly what jsonata-js gives: a one-item input
/// either unwraps to the field's value or, when that value is an array, maps
/// the next step over its items, as it does over the array itself; and the
/// last-step rule returns a final array value as is.
fn walk_single_values<'a>(
    steps: impl Iterator<Item = (&'a str, bool)>,
    data: &JValue,
) -> Result<Option<(usize, JValue)>, EvaluatorError> {
    let mut value = data.clone();
    let mut taken = 0;
    for (field, plain) in steps {
        if !plain {
            break;
        }
        value = match &value {
            JValue::Object(obj) if !is_tuple_object(obj) => {
                obj.get(field).cloned().unwrap_or(JValue::Undefined)
            }
            #[cfg(feature = "python")]
            JValue::LazyPyDict(lazy) => lazy.get_field(field)?,
            _ => break,
        };
        taken += 1;
        if value.is_undefined() {
            return Ok(None);
        }
    }
    Ok(Some((taken, value)))
}

/// The value of `field` in each of `items` that has one, in order: jsonata-js
/// `lookup` applied item by item. An item that is itself an array yields the
/// sequence of its own items' values. The field's position in the first
/// object is reused for the rest, so homogeneous arrays look it up in O(1)
/// without hashing.
fn field_values(
    field: &str,
    items: &[JValue],
    options: &EvaluatorOptions,
) -> Result<Vec<JValue>, EvaluatorError> {
    field_values_of::<false>(field, items, options)
}

/// `field_values`, keeping an undefined value for each item without the
/// field when `KEEP_UNDEFINED` is set: a step's filter stages still run on
/// those, as in jsonata-js (see `apply_filter_stages`).
fn field_values_of<const KEEP_UNDEFINED: bool>(
    field: &str,
    items: &[JValue],
    options: &EvaluatorOptions,
) -> Result<Vec<JValue>, EvaluatorError> {
    let mut position: Option<usize> = None;
    let mut values = Vec::with_capacity(items.len());
    for item in items {
        let value = match item {
            JValue::Object(obj) if !is_tuple_object(obj) => {
                match position.and_then(|p| obj.get_index(p)) {
                    Some((key, value)) if key == field => value.clone(),
                    _ => match obj.get_full(field) {
                        Some((index, _, value)) => {
                            position = Some(index);
                            value.clone()
                        }
                        None => JValue::Undefined,
                    },
                }
            }
            _ => compiled_field_step(field, item, options)?,
        };
        if KEEP_UNDEFINED || !value.is_undefined() {
            values.push(value);
        }
    }
    Ok(values)
}

/// Perform a single-field access with implicit array-mapping semantics.
///
/// - Object: look up `field`, return its value or Undefined
/// - Array: map field extraction over each element, flatten nested arrays, skip Undefined
///   (this is a query-result sequence, so D2015 applies — mirrors `evaluate_path`'s
///   array-mapping check and `vm.rs`'s `get_field_cached`)
/// - Tuple objects (`__tuple__: true`): look up in the `@` inner object
/// - Other: Undefined
pub(crate) fn compiled_field_step(
    field: &str,
    value: &JValue,
    options: &EvaluatorOptions,
) -> Result<JValue, EvaluatorError> {
    match value {
        JValue::Object(obj) => {
            // Check for tuple: extract from "@" inner object
            if is_tuple_object(obj) {
                match obj.get("@") {
                    Some(JValue::Object(inner)) => {
                        return Ok(inner.get(field).cloned().unwrap_or(JValue::Undefined));
                    }
                    #[cfg(feature = "python")]
                    Some(JValue::LazyPyDict(lazy)) => {
                        return Ok(lazy.get_field(field)?);
                    }
                    _ => {}
                }
                return Ok(JValue::Undefined);
            }
            Ok(obj.get(field).cloned().unwrap_or(JValue::Undefined))
        }
        #[cfg(feature = "python")]
        JValue::LazyPyDict(lazy) => Ok(lazy.get_field(field)?),
        JValue::Array(arr) => {
            let mut result = Vec::with_capacity(arr.len());
            // Flat (#196) when no array was flattened in: every item pushed
            // is then a non-array value.
            let mut flat = true;
            for value in field_values(field, arr, options)? {
                match value {
                    JValue::Array(inner) => {
                        flat = false;
                        result.extend(inner.iter().cloned())
                    }
                    other => result.push(other),
                }
            }
            check_sequence_length(result.len(), options)?;
            Ok(if result.is_empty() {
                JValue::Undefined
            } else {
                JValue::sequence_flat(result, flat)
            })
        }
        _ => Ok(JValue::Undefined),
    }
}

/// The parameter of a transform's function value. The parser names `$` as
/// the empty string, so no lambda written in an expression has it.
const TRANSFORM_INPUT: &str = "$";

/// Apply a compiled filter predicate to a value, following jsonata-js
/// `evaluateFilter` (as the tree-walker's `evaluate_filter` does): a
/// non-array is a one-item sequence, and each item is kept as many times as
/// the predicate selects it -- by position when the predicate yields a number
/// or an array of numbers, by truthiness otherwise. Numeric-literal
/// predicates (`[0]`) are not compiled; the tree-walker handles them.
/// `shape` speeds up the predicate's field reads.
fn compiled_apply_filter(
    filter: &CompiledExpr,
    value: &JValue,
    shape: Option<&ShapeCache>,
    vars: Option<&HashMap<&str, &JValue>>,
    ctx: Option<&Context>,
    options: &EvaluatorOptions,
    start_time: Option<Instant>,
) -> Result<JValue, EvaluatorError> {
    let single;
    let items: &[JValue] = match value {
        JValue::Undefined => return Ok(JValue::Undefined),
        JValue::Array(arr) => arr,
        other => {
            single = [other.clone()];
            &single
        }
    };
    let len = items.len();
    let mut result = Vec::new();
    for (index, item) in items.iter().enumerate() {
        check_loop_timeout(options, start_time)?;
        let selector = eval_compiled_inner(filter, item, vars, ctx, shape, options, start_time)?;
        let repeats = match predicate_index_match(&selector, index, len) {
            Some(n) => n,
            None => usize::from(compiled_is_truthy(&selector)),
        };
        for _ in 0..repeats {
            result.push(item.clone());
        }
    }
    check_sequence_length(result.len(), options)?;
    Ok(JValue::sequence(result))
}

/// Materialize a top-level lazy dict into a plain Object. Non-lazy values
/// pass through unchanged. Does NOT recurse into arrays/objects — element-
/// level laziness is handled by the specific consumers that need it.
#[cfg(feature = "python")]
pub(crate) fn normalize_lazy(value: &JValue) -> Result<JValue, EvaluatorError> {
    match value {
        JValue::LazyPyDict(lazy) => Ok(JValue::Object(lazy.to_object()?)),
        _ => Ok(value.clone()),
    }
}

#[cfg(not(feature = "python"))]
pub(crate) fn normalize_lazy(value: &JValue) -> Result<JValue, EvaluatorError> {
    Ok(value.clone())
}

/// Validate and coerce a builtin's arguments against its jsonata-js signature.
///
/// One copy shared by all three dispatch paths -- the compiled/VM path via
/// `crate::builtins::dispatch_pure`, the tree-walker's `evaluate_function_call`,
/// and `call_builtin_with_values` for builtins passed by reference. They used to
/// hand-roll their own argument checks, which is why they disagreed with each
/// other about `$round` and the `$substring*` family.
///
/// Returns `None` when validation declines because an argument was missing: the
/// answer differs per function ($count is 0, $exists is false, $abs is
/// undefined), so the caller's arm decides. A genuine type error is returned as
/// an error.
pub(crate) fn validate_builtin_args(
    name: &str,
    args: &[JValue],
    context: &JValue,
) -> Result<Option<Vec<JValue>>, EvaluatorError> {
    let Some(sig) = crate::signature::builtin_signature(name) else {
        return Ok(None);
    };
    // A trailing argument that evaluated to undefined and binds to an
    // *optional* parameter was not really supplied: `$round(2.5, missing.x)` is
    // 2 in jsonata-js, not an error. Trim those before validating.
    //
    // Only optional ones. An undefined argument bound to a required or
    // repeatable parameter is meaningful and must reach the function:
    // `$zip([1,2,3], [4,5,6], nothing)` is `[]` and `$append(1, notexist)` is
    // `1`, and both break if the argument is dropped.
    // Never trim to zero: the `-` marker makes the first parameter optional
    // too, and dropping the only argument turns an explicit call into a
    // context-substituted one.
    let mut end = args.len();
    while end > 1 {
        let binds_to_optional = sig.params.get(end - 1).is_some_and(|p| p.optional);
        if binds_to_optional && matches!(args.get(end - 1), Some(JValue::Undefined)) {
            end -= 1;
        } else {
            break;
        }
    }
    let args = &args[..end];
    match sig.validate_and_coerce_counted(args, context) {
        Ok((mut coerced, context_substitutions)) => {
            // Validation returns one entry per *parameter*, padding absent
            // optional ones with Undefined. The arms decide whether an optional
            // was supplied from `args.len()`, so a padded tail makes
            // `$split("a,b", ",")` read a third argument and reject it as a
            // non-numeric limit. Trim the padding back off.
            //
            // A '-' parameter filled from the context also lengthens the list,
            // and that entry is not padding -- it stands in for an argument the
            // function must receive. `$lookup(missing.x)` grows one argument
            // into `[context, Undefined]`, so trimming on length alone would
            // discard the supplied (undefined) key and leave the arm reporting
            // a one-argument call. Allow for the substitutions before deciding
            // what is padding.
            let supplied = args.len() + context_substitutions;
            while coerced.len() > supplied && matches!(coerced.last(), Some(JValue::Undefined)) {
                coerced.pop();
            }
            Ok(Some(coerced))
        }
        Err(e) => Err(EvaluatorError::TypeError(e.to_string())),
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// End of CompiledExpr framework
// ──────────────────────────────────────────────────────────────────────────────

/// Functions that propagate undefined (return undefined when given an undefined argument).
/// These functions should return null/undefined when their input path doesn't exist,
/// rather than throwing a type error.
const UNDEFINED_PROPAGATING_FUNCTIONS: &[&str] = &[
    "not",
    "boolean",
    "length",
    "number",
    "uppercase",
    "lowercase",
    "substring",
    "substringBefore",
    "substringAfter",
    "string",
    "abs",
    "ceil",
    "floor",
    "round",
    "sqrt",
    // Same `<s-:s>` shape as $uppercase/$lowercase above. These two only
    // reached the list once they had a signature to be validated against
    // (#126 group 2); before that their arms raised a hand-rolled type
    // error on an undefined argument instead of propagating it.
    "base64encode",
    "base64decode",
];

/// Check whether a function propagates undefined values
pub(crate) fn propagates_undefined(name: &str) -> bool {
    UNDEFINED_PROPAGATING_FUNCTIONS.contains(&name)
}

/// Iterator-based numeric aggregation helpers.
/// These avoid cloning values by iterating over references and extracting f64 values directly.
pub(crate) mod aggregation {
    use super::*;

    /// Iterate over all numeric values in a potentially nested array, yielding f64 values.
    /// Returns Err if any non-numeric value is encountered.
    fn for_each_numeric(
        arr: &[JValue],
        func_name: &str,
        mut f: impl FnMut(f64),
    ) -> Result<(), EvaluatorError> {
        fn recurse(
            arr: &[JValue],
            func_name: &str,
            f: &mut dyn FnMut(f64),
        ) -> Result<(), EvaluatorError> {
            for value in arr.iter() {
                match value {
                    JValue::Array(inner) => recurse(inner, func_name, f)?,
                    JValue::Number(n) => {
                        f(*n);
                    }
                    _ => {
                        return Err(EvaluatorError::TypeError(format!(
                            "{}() requires all array elements to be numbers",
                            func_name
                        )));
                    }
                }
            }
            Ok(())
        }
        recurse(arr, func_name, &mut f)
    }

    /// Count elements in a potentially nested array without cloning.
    fn count_numeric(arr: &[JValue], func_name: &str) -> Result<usize, EvaluatorError> {
        let mut count = 0usize;
        for_each_numeric(arr, func_name, |_| count += 1)?;
        Ok(count)
    }

    pub fn sum(arr: &[JValue]) -> Result<JValue, EvaluatorError> {
        if arr.is_empty() {
            return Ok(JValue::from(0i64));
        }
        let mut total = 0.0f64;
        for_each_numeric(arr, "sum", |n| total += n)?;
        Ok(JValue::Number(total))
    }

    pub fn max(arr: &[JValue]) -> Result<JValue, EvaluatorError> {
        if arr.is_empty() {
            // jsonata-js: $max([]) is undefined, not null (issue #109).
            return Ok(JValue::Undefined);
        }
        let mut max_val = f64::NEG_INFINITY;
        for_each_numeric(arr, "max", |n| {
            if n > max_val {
                max_val = n;
            }
        })?;
        Ok(JValue::Number(max_val))
    }

    pub fn min(arr: &[JValue]) -> Result<JValue, EvaluatorError> {
        if arr.is_empty() {
            // jsonata-js: $min([]) is undefined, not null (issue #109).
            return Ok(JValue::Undefined);
        }
        let mut min_val = f64::INFINITY;
        for_each_numeric(arr, "min", |n| {
            if n < min_val {
                min_val = n;
            }
        })?;
        Ok(JValue::Number(min_val))
    }

    pub fn average(arr: &[JValue]) -> Result<JValue, EvaluatorError> {
        if arr.is_empty() {
            // jsonata-js: $average([]) is undefined, not null (issue #109).
            return Ok(JValue::Undefined);
        }
        let mut total = 0.0f64;
        let count = count_numeric(arr, "average")?;
        for_each_numeric(arr, "average", |n| total += n)?;
        Ok(JValue::Number(total / count as f64))
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

    /// Python→JValue conversion failed during lazy field access.
    /// Surfaces as Python TypeError at the boundary (matching what eager
    /// conversion would have raised at call time).
    #[cfg(feature = "python")]
    #[error("Type error: {0}")]
    PyConversionError(String),
}

impl From<crate::functions::FunctionError> for EvaluatorError {
    fn from(e: crate::functions::FunctionError) -> Self {
        // `PyConversionError` must surface as a Python `TypeError` (matching what
        // eager conversion would have raised), not the generic `ValueError` every
        // other `FunctionError` variant maps to below -- see its doc comment.
        #[cfg(feature = "python")]
        if let crate::functions::FunctionError::PyConversionError(m) = &e {
            return EvaluatorError::PyConversionError(m.clone());
        }
        // Pass the inner message through rather than thiserror's Display,
        // which prepends "Runtime error: "/"Argument error: "/"Type error: "
        // — burying the JSONata spec code ("Runtime error: D3030: ...") so
        // no prefix-anchored classifier (the CLIs, `EvaluatorError::code`)
        // could see it, and double-stacking with EvaluatorError's own
        // Display prefix ("Evaluation error: Runtime error: ...").
        use crate::functions::FunctionError as FE;
        let msg = match e {
            FE::ArgumentError(m) | FE::TypeError(m) | FE::RuntimeError(m) => m,
            // Handled by the early return above; kept for exhaustiveness.
            #[cfg(feature = "python")]
            FE::PyConversionError(m) => m,
        };
        EvaluatorError::EvaluationError(msg)
    }
}

impl From<crate::datetime::DateTimeError> for EvaluatorError {
    fn from(e: crate::datetime::DateTimeError) -> Self {
        EvaluatorError::EvaluationError(e.to_string())
    }
}

impl EvaluatorError {
    /// The underlying message, without the outer "Type error: "/
    /// "Reference error: "/"Evaluation error: " prefix that `Display` (via
    /// thiserror's `#[error("Type error: {0}")]` etc.) would add. This is
    /// what JSONata-spec-coded messages like "T2002: ..." actually look
    /// like — the coded prefix is INSIDE this string, not added by
    /// `Display`. Used by both the Python bindings (`src/lib.rs`) and the
    /// `jsonata` CLI so the two never need to duplicate this unwrap.
    pub fn message(&self) -> &str {
        match self {
            EvaluatorError::TypeError(m) => m,
            EvaluatorError::ReferenceError(m) => m,
            EvaluatorError::EvaluationError(m) => m,
            #[cfg(feature = "python")]
            EvaluatorError::PyConversionError(m) => m,
        }
    }

    /// The JSONata spec code ("T2010", "D3030", ...) this error carries, if
    /// any: the message's leading `X####:` prefix. This is THE definition of
    /// "is this a coded error" — the C ABI's `jsonata_last_error_code` and
    /// the CLI's error formatting both classify with it (the Python CLI
    /// mirrors it with an equivalent anchored regex), so the same failure
    /// presents the same way in every binary.
    pub fn code(&self) -> Option<&str> {
        error_code_prefix(self.message())
    }
}

/// The leading JSONata spec code of `msg` (`X####:` at offset 0), if any.
pub fn error_code_prefix(msg: &str) -> Option<&str> {
    let b = msg.as_bytes();
    if b.len() >= 6
        && b[0].is_ascii_uppercase()
        && b[1..5].iter().all(u8::is_ascii_digit)
        && b[5] == b':'
    {
        Some(&msg[..5])
    } else {
        None
    }
}

#[cfg(feature = "python")]
impl From<crate::lazy::LazyConvertError> for EvaluatorError {
    fn from(e: crate::lazy::LazyConvertError) -> Self {
        EvaluatorError::PyConversionError(e.0)
    }
}

#[cfg(test)]
mod evaluator_error_message_tests {
    use super::EvaluatorError;

    #[test]
    fn message_strips_the_display_prefix() {
        let e = EvaluatorError::TypeError(
            "T2002: The left side of the + operator must evaluate to a number".to_string(),
        );
        assert_eq!(
            e.message(),
            "T2002: The left side of the + operator must evaluate to a number"
        );
        // Display, by contrast, adds the "Type error: " wrapper -- this is
        // exactly the distinction `message()` exists to avoid.
        assert_eq!(
            e.to_string(),
            "Type error: T2002: The left side of the + operator must evaluate to a number"
        );
    }

    #[test]
    fn message_works_for_all_variants() {
        assert_eq!(
            EvaluatorError::ReferenceError("$foo is not defined".to_string()).message(),
            "$foo is not defined"
        );
        assert_eq!(
            EvaluatorError::EvaluationError("something went wrong".to_string()).message(),
            "something went wrong"
        );
    }
}

/// The names a composition's parts are bound to
/// (`Evaluator::compose_functions`). They hold spaces, so no expression can
/// spell them: `$x` or `$__chain_x` would be visible to f and g.
mod composition_names {
    pub(super) const F: &str = "chain f";
    pub(super) const G: &str = "chain g";
    pub(super) const X: &str = "chain x";
}

/// How a function is being applied, for the input jsonata-js's `apply`
/// gives it (which only a builtin it calls in tail position reads).
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
enum ApplyMode {
    /// A direct call: the call site's input.
    #[default]
    Direct,
    /// Through the closure `evaluateFunction` wraps a function argument in,
    /// or by `~>` to a non-call: `apply(fn, args, null, env)`, a null input.
    Callback,
    /// The function itself, unwrapped, applied by a builtin that was a
    /// function's tail call (jsonata-js's trampoline evaluates such a call's
    /// arguments without wrapping them): `procedure.apply`, which applies it
    /// with the input it was defined with.
    Raw,
}

/// The function arguments of a builtin that is running as a function's
/// tail call (see `ApplyMode::Raw`): its argument nodes (as an address
/// range), and the function values it was given already evaluated.
struct RawHof {
    args: (usize, usize),
    fns: Vec<usize>,
}

/// The builtins that apply functions they are given.
fn is_hof_builtin(name: &str) -> bool {
    matches!(
        name,
        "map" | "filter" | "reduce" | "single" | "sift" | "each" | "sort" | "match" | "replace"
    )
}

/// The address of a function value's closure, to recognize it by.
fn lambda_ptr(value: &JValue) -> Option<usize> {
    match value {
        JValue::Lambda(rc) => Some(Rc::as_ptr(rc) as usize),
        _ => None,
    }
}

/// Result of evaluating a lambda body that may be a tail call
/// Used for trampoline-based tail call optimization
enum LambdaResult {
    /// Final value - evaluation is complete
    JValue(JValue),
    /// Tail call - need to continue with another lambda invocation
    TailCall {
        /// The lambda to call (shared handle; cloning is a refcount bump)
        lambda: Rc<StoredLambda>,
        /// Arguments for the call
        args: Vec<JValue>,
        /// Data context for the call
        data: JValue,
    },
}

thread_local! {
    static LIVE_LAMBDAS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Number of `StoredLambda` instances currently alive on this thread.
///
/// The snapshot-capture closure design cannot form `Rc` cycles (a closure can
/// only capture values that existed before it did, and self-reference is
/// late-bound at invocation rather than stored), so this count must return to
/// its prior level once evaluation results are dropped. Exposed so tests can
/// assert that repeated evaluation does not leak closures.
pub fn live_lambda_count() -> usize {
    LIVE_LAMBDAS.with(|c| c.get())
}

/// Marker owned by every `StoredLambda`, maintaining `live_lambda_count`.
#[derive(Debug)]
pub struct LiveLambdaToken(());

impl LiveLambdaToken {
    fn new() -> Self {
        LIVE_LAMBDAS.with(|c| c.set(c.get() + 1));
        LiveLambdaToken(())
    }
}

impl Default for LiveLambdaToken {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for LiveLambdaToken {
    fn clone(&self) -> Self {
        Self::new()
    }
}

impl Drop for LiveLambdaToken {
    fn drop(&mut self) {
        LIVE_LAMBDAS.with(|c| c.set(c.get() - 1));
    }
}

/// How a partial application (`$f(1, ?)`) reaches the function it applies.
#[derive(Clone, Debug)]
pub(crate) enum PartialTarget {
    /// A user-defined function, captured as a closure value at creation time —
    /// this is what lets the partial outlive the scope that defined its target.
    Lambda(Rc<StoredLambda>),
    /// A host function (`register_fn`/`register_fn_override`), taken from
    /// the registry when the partial is made, as jsonata-js takes a
    /// registered function's value.
    Host(HostTarget),
    /// A builtin, resolved when the partial is made (`$uppercase(?)`,
    /// `($uppercase)(?)`, `$f := $sum; $f(?)`): that builtin, whatever its
    /// name is bound to by the time the partial is called.
    Builtin(String),
}

/// A host function held by a partial application.
#[derive(Clone)]
pub(crate) struct HostTarget(Rc<dyn HostFn>);

impl std::fmt::Debug for HostTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HostTarget")
    }
}

/// A partial application, carried inside a `StoredLambda` in place of a body.
///
/// `$add(10, ?)` becomes params `["__p0"]` plus this struct: at invocation the
/// full argument list is rebuilt — bound values in their original positions,
/// call arguments filling the placeholder positions in order — and the target
/// is applied to it.
#[derive(Clone, Debug)]
pub(crate) struct PartialApplication {
    pub target: PartialTarget,
    /// `(position, value)` for arguments fixed at creation time (values, not
    /// expressions: `$add($x, ?)` snapshots `$x` when the partial is built).
    pub bound_args: Vec<(usize, JValue)>,
    /// Positions of the `?` placeholders, filled from call arguments in order.
    pub placeholder_positions: Vec<usize>,
    /// Argument count of the underlying call (bound + placeholders).
    pub total_args: usize,
}

/// Lambda storage
/// Stores the AST of a lambda function along with its parameters, optional signature,
/// and captured environment for closures.
///
/// Carried directly inside `JValue::Lambda(Rc<StoredLambda>)` (issue #157):
/// the value IS the closure, so its lifetime is the `Rc`'s and no side table,
/// escape analysis, or scope-pop migration is needed.
#[derive(Clone, Debug)]
pub struct StoredLambda {
    pub params: Vec<String>,
    pub body: AstNode,
    /// Pre-compiled body for use in tight inner loops (HOF fast path).
    /// `None` if the body is not compilable (transform, partial-app, thunk, etc.).
    /// Only usable through `Evaluator::usable_compiled_body`.
    pub(crate) compiled_body: Option<CompiledBody>,
    pub signature: Option<String>,
    /// Captured environment bindings for closures: a by-value snapshot of the
    /// free variables at definition time (this engine's long-standing capture
    /// semantics — NOT jsonata-js's live frames).
    pub captured_env: HashMap<String, JValue>,
    /// Captured data context for lexical scoping of bare field names
    pub captured_data: Option<JValue>,
    /// Whether this lambda's body contains tail calls that can be optimized
    pub thunk: bool,
    /// The variable name this lambda was bound to at definition time
    /// (`$f := function ...`). Bound to the closure itself at every
    /// invocation, so recursion works after the defining scope has popped
    /// (classic letrec, late-bound instead of stored — no `Rc` cycle).
    pub self_name: Option<String>,
    /// `Some` iff this lambda is a partial application: invocation applies the
    /// carried target instead of evaluating `body` (which is inert).
    /// `Rc` so invocation can detach a handle from the borrowed closure with a
    /// refcount bump.
    pub(crate) partial: Option<Rc<PartialApplication>>,
    /// Keeps `live_lambda_count` accurate; see that function.
    pub live_token: LiveLambdaToken,
    /// The scope the function was defined in: names its body neither binds
    /// nor captured resolve there, not where it is called (#191).
    pub(crate) def_scope: DefScope,
}

impl StoredLambda {
    /// Minimal instance for tests that need a lambda-shaped `JValue`.
    #[cfg(test)]
    pub(crate) fn test_stub() -> Self {
        StoredLambda {
            params: Vec::new(),
            body: AstNode::Null,
            compiled_body: None,
            signature: None,
            captured_env: HashMap::new(),
            captured_data: None,
            thunk: false,
            self_name: None,
            partial: None,
            live_token: LiveLambdaToken::new(),
            def_scope: DefScope::ROOT,
        }
    }
}

/// A single scope in the scope stack
struct Scope {
    bindings: HashMap<String, JValue>,
    /// Identifies this frame among every frame its context has pushed.
    /// Ids only grow, so a frame pushed after a function was defined has a
    /// larger id than any frame that was live when it was defined (see
    /// `Context::barrier_for`).
    id: u64,
    /// For a function's frame: the index of the frame the function was
    /// defined in. A lookup that misses here continues from that frame,
    /// skipping the caller's frames in between, so a function body sees the
    /// scope it was written in and not the one it is called from (#191).
    barrier: Option<usize>,
    /// A frame of the tail-call trampoline, which it may clear and reuse
    /// for the next call (`Context::enter_tail_call`).
    tco: bool,
    /// For a trampoline frame: a token each function defined to see this
    /// frame holds (`DefScope::keep`), created with the first. While some
    /// function outside the frame's own bindings holds one, the frame is
    /// part of its scope and must not be reused (#191).
    token: std::cell::OnceCell<Rc<FrameToken>>,
    /// An evaluator-internal frame (argument temporaries, a composition's
    /// parts), which `$eval` does not see.
    internal: bool,
    /// An internal frame standing for a jsonata-js call frame of its own (a
    /// composition's first function runs in one): `$eval` reached through it
    /// keeps its bindings to itself instead of passing them to the frame
    /// below.
    opaque: bool,
    /// The token's reference count when the frame was last judged seen
    /// from outside (`Scope::still_seen`); 0 if it has not been.
    seen_at: usize,
}

/// A value with a reference count, as `OwnershipWalk` sees it.
#[derive(Clone, Copy)]
enum Owned<'a> {
    Lambda(&'a Rc<StoredLambda>),
    Partial(&'a Rc<PartialApplication>),
    Items(&'a Rc<crate::value::JItems>),
    Map(&'a Rc<crate::value::JMap>),
    Token(&'a Rc<FrameToken>),
}

impl Owned<'_> {
    fn key(self) -> usize {
        match self {
            Owned::Lambda(rc) => Rc::as_ptr(rc) as *const u8 as usize,
            Owned::Partial(rc) => Rc::as_ptr(rc) as *const u8 as usize,
            Owned::Items(rc) => Rc::as_ptr(rc) as *const u8 as usize,
            Owned::Map(rc) => Rc::as_ptr(rc) as *const u8 as usize,
            Owned::Token(rc) => Rc::as_ptr(rc) as *const u8 as usize,
        }
    }

    fn strong_count(self) -> usize {
        match self {
            Owned::Lambda(rc) => Rc::strong_count(rc),
            Owned::Partial(rc) => Rc::strong_count(rc),
            Owned::Items(rc) => Rc::strong_count(rc),
            Owned::Map(rc) => Rc::strong_count(rc),
            Owned::Token(rc) => Rc::strong_count(rc),
        }
    }
}

/// Counts the references to a frame's token that come from values its
/// bindings own (see `Scope::seen_from_outside`). A walk step answers
/// `Break(false)` once those account for every holder.
struct OwnershipWalk<'a> {
    token: &'a Rc<FrameToken>,
    holders: usize,
    inside: usize,
    /// References found so far to each value, from owned values only.
    refs: HashMap<usize, usize>,
    /// Owned values not yet looked into: functions and tokens first, as
    /// they are where holders are.
    functions: Vec<Owned<'a>>,
    containers: Vec<Owned<'a>>,
}

impl<'a> OwnershipWalk<'a> {
    fn run(&mut self, bindings: impl Iterator<Item = &'a JValue>) -> std::ops::ControlFlow<bool> {
        for value in bindings {
            self.value(value)?;
        }
        loop {
            let next = match self.functions.pop() {
                Some(node) => node,
                None => match self.containers.pop() {
                    Some(node) => node,
                    None => return std::ops::ControlFlow::Break(true),
                },
            };
            self.expand(next)?;
        }
    }

    /// One reference to `node` from an owned value.
    fn reference(&mut self, node: Owned<'a>) -> std::ops::ControlFlow<bool> {
        if let Owned::Token(token) = node {
            if Rc::ptr_eq(token, self.token) {
                self.inside += 1;
                if self.inside == self.holders {
                    return std::ops::ControlFlow::Break(false);
                }
                return std::ops::ControlFlow::Continue(());
            }
        }
        let refs = self.refs.entry(node.key()).or_insert(0);
        *refs += 1;
        if *refs == node.strong_count() {
            // Every reference is from owned values: owned too.
            match node {
                Owned::Items(_) | Owned::Map(_) => self.containers.push(node),
                _ => self.functions.push(node),
            }
        }
        std::ops::ControlFlow::Continue(())
    }

    fn value(&mut self, value: &'a JValue) -> std::ops::ControlFlow<bool> {
        match value {
            JValue::Lambda(rc) => self.reference(Owned::Lambda(rc)),
            JValue::Array(items) => self.reference(Owned::Items(items.items_rc())),
            JValue::Object(map) => self.reference(Owned::Map(map)),
            _ => std::ops::ControlFlow::Continue(()),
        }
    }

    fn expand(&mut self, node: Owned<'a>) -> std::ops::ControlFlow<bool> {
        match node {
            Owned::Lambda(rc) => {
                for value in rc.captured_env.values() {
                    self.value(value)?;
                }
                if let Some(data) = &rc.captured_data {
                    self.value(data)?;
                }
                if let Some(partial) = &rc.partial {
                    self.reference(Owned::Partial(partial))?;
                }
                if let Some(token) = &rc.def_scope.keep {
                    self.reference(Owned::Token(token))?;
                }
            }
            Owned::Partial(partial) => {
                if let PartialTarget::Lambda(target) = &partial.target {
                    self.reference(Owned::Lambda(target))?;
                }
                for (_, value) in &partial.bound_args {
                    self.value(value)?;
                }
            }
            Owned::Items(items) => {
                for value in items.iter() {
                    self.value(value)?;
                }
            }
            Owned::Map(map) => {
                for value in map.values() {
                    self.value(value)?;
                }
            }
            Owned::Token(token) => {
                if let Some(outer) = &token.outer {
                    self.reference(Owned::Token(outer))?;
                }
            }
        }
        std::ops::ControlFlow::Continue(())
    }
}

/// A trampoline frame's token (`Scope::token`). It holds the token of the
/// next trampoline frame out in its lexical chain, so a function holds
/// only its innermost one and still keeps every frame it sees.
#[derive(Debug)]
pub(crate) struct FrameToken {
    outer: Option<Rc<FrameToken>>,
}

impl Scope {
    /// Whether a function that can see this frame may outlive the frame's
    /// bindings. Each such function holds the frame's token, directly or
    /// through the token of a trampoline frame further in; the frame's own
    /// bindings keep some of them. A value is owned by the bindings when
    /// every reference to it comes from the bindings or from values they
    /// own; owned values go when the frame is cleared, so if they account
    /// for every holder, nothing can see the frame afterwards. The walk
    /// looks into owned values only (functions first, as that is where
    /// holders are, and it stops once all are found), never into a value
    /// referenced from elsewhere. Owned values go when the frame is reused;
    /// a frame kept instead is walked again only once one of its token's
    /// holders has gone (`Scope::still_seen`). So the walks cost, in all,
    /// what making the owned values did, plus one walk of a kept frame per
    /// holder it loses.
    fn seen_from_outside(&self) -> bool {
        let Some(token) = self.token.get() else {
            return false;
        };
        let holders = Rc::strong_count(token) - 1;
        if holders == 0 {
            return false;
        }
        let mut walk = OwnershipWalk {
            token,
            holders,
            inside: 0,
            refs: HashMap::new(),
            functions: Vec::new(),
            containers: Vec::new(),
        };
        walk.run(self.bindings.values()) != std::ops::ControlFlow::Break(false)
    }

    /// `seen_from_outside`, remembered: a frame judged seen stays so, with
    /// no walk, until a holder of its token goes. A holder replaced by
    /// another meanwhile only keeps the frame a while longer.
    fn still_seen(&mut self) -> bool {
        let count = match self.token.get() {
            Some(token) => Rc::strong_count(token),
            None => return false,
        };
        if count == self.seen_at {
            return true;
        }
        let seen = self.seen_from_outside();
        self.seen_at = if seen { count } else { 0 };
        seen
    }

    fn new(id: u64, barrier: Option<usize>) -> Self {
        Scope {
            bindings: HashMap::new(),
            id,
            barrier,
            tco: false,
            token: std::cell::OnceCell::new(),
            internal: false,
            opaque: false,
            seen_at: 0,
        }
    }
}

/// Where a function was defined: the frames its body could see then, the
/// frame on top of the scope stack first, then each frame a lookup from it
/// would reach (its lexical ancestors, skipping callers' frames). Calling
/// the function resolves the names its body does not bind or capture
/// through the first of these still on the stack, as jsonata-js resolves
/// them through the function's `environment`.
#[derive(Clone, Debug)]
pub(crate) struct DefScope {
    /// The defining context (`Context::ctx_id`); 0 for a function defined
    /// outside any scope, which sees only the root frame.
    ctx: u64,
    /// `(index, id)` of each visible frame above the root, innermost first.
    /// The root frame (index 0) is always visible and never listed.
    chain: DefChain,
    /// The evaluator built this function (a composition, or the call of
    /// its first function); its frames are internal.
    internal: bool,
    /// Its frames continue at its caller's frame, not a defining one
    /// (`DefScope::AT_CALLER`).
    at_caller: bool,
    /// Its frames are opaque (see `Scope::opaque`).
    opaque: bool,
    /// The token of the innermost trampoline frame in `chain` (see
    /// `Scope::token`), which keeps the trampoline from reusing it, and
    /// through it the ones further out, while this function lives. None
    /// outside tail-call loops.
    keep: Option<Rc<FrameToken>>,
}

/// Lexical chains are short (a few blocks and calls), so they are kept
/// inline, without an allocation per function created.
const DEF_CHAIN_INLINE: usize = 8;

#[derive(Clone, Debug)]
enum DefChain {
    Inline {
        len: u8,
        frames: [(usize, u64); DEF_CHAIN_INLINE],
    },
    Heap(Rc<[(usize, u64)]>),
}

impl DefChain {
    fn frames(&self) -> &[(usize, u64)] {
        match self {
            DefChain::Inline { len, frames } => &frames[..*len as usize],
            DefChain::Heap(frames) => frames,
        }
    }
}

impl DefScope {
    /// Defined outside any scope: only the root frame (the caller's
    /// bindings) is visible.
    pub(crate) const ROOT: DefScope = DefScope {
        ctx: 0,
        chain: DefChain::Inline {
            len: 0,
            frames: [(0, 0); DEF_CHAIN_INLINE],
        },
        internal: false,
        at_caller: false,
        opaque: false,
        keep: None,
    };

    /// A composition `f ~> g` (`Evaluator::compose_functions`). Its frame
    /// continues at its caller's frame and is hidden from `$eval`: g is
    /// jsonata-js's tail call, run in the caller's environment, so `$eval`
    /// as g sees, and binds in, the scope the composition is called from.
    pub(crate) const AT_CALLER: DefScope = DefScope {
        ctx: 0,
        chain: DefChain::Inline {
            len: 0,
            frames: [(0, 0); DEF_CHAIN_INLINE],
        },
        internal: true,
        at_caller: true,
        opaque: false,
        keep: None,
    };
}

static NEXT_CONTEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Evaluation context
///
/// Holds variable bindings and other state needed during evaluation.
/// Uses a scope stack for efficient push/pop instead of clone/restore.
pub struct Context {
    scope_stack: Vec<Scope>,
    parent_data: Option<JValue>,
    /// Distinguishes this context's frames from another context's, for a
    /// function value that crosses evaluators.
    ctx_id: u64,
    /// The id the next pushed (or cleared) frame gets.
    next_scope_id: u64,
    /// The frame the innermost running tail-call trampoline was entered
    /// from: jsonata-js's environment of the `apply` that runs a chain of
    /// tail calls, where it applies each one (`Evaluator::invoke_lambda_with_tco`).
    tail_base: usize,
    /// Set while a builtin runs as a tail call: the frame its callbacks'
    /// own tail-call chains begin at, as jsonata-js applies a function
    /// argument of such a builtin in the trampoline's environment, and the
    /// stack height the builtin applies them at (so a function applied
    /// anywhere deeper, by a callback, starts where it is applied).
    callback_base: Option<(usize, usize)>,
    /// The most frames the stack has held, for tests that bound it.
    #[cfg(test)]
    peak_depth: usize,
}

impl Context {
    pub fn new() -> Self {
        Context {
            scope_stack: vec![Scope::new(0, None)],
            parent_data: None,
            ctx_id: NEXT_CONTEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            next_scope_id: 1,
            tail_base: 0,
            callback_base: None,
            #[cfg(test)]
            peak_depth: 1,
        }
    }

    fn fresh_scope_id(&mut self) -> u64 {
        let id = self.next_scope_id;
        self.next_scope_id += 1;
        id
    }

    fn push_frame(&mut self, scope: Scope) {
        self.scope_stack.push(scope);
        #[cfg(test)]
        {
            self.peak_depth = self.peak_depth.max(self.scope_stack.len());
        }
    }

    /// Push a new scope onto the stack
    fn push_scope(&mut self) {
        let id = self.fresh_scope_id();
        self.push_frame(Scope::new(id, None));
    }

    /// Push the frame of a call to a function defined at `def`: lookups that
    /// miss in it continue at the defining frame, not the caller's.
    fn push_scope_defined_at(&mut self, def: &DefScope) {
        let barrier = self.barrier_for(def);
        let id = self.fresh_scope_id();
        let mut scope = Scope::new(id, Some(barrier));
        scope.internal = def.internal;
        scope.opaque = def.opaque;
        self.push_frame(scope);
    }

    /// Push a frame for evaluator-internal bindings, hidden from `$eval`.
    fn push_internal_scope(&mut self) {
        let id = self.fresh_scope_id();
        let mut scope = Scope::new(id, None);
        scope.internal = true;
        self.push_frame(scope);
    }

    /// The innermost frame that is not internal, from the top or from frame
    /// `from`, where `$eval` runs, and whether an opaque frame is on the
    /// way there.
    fn frame_below_internal(&self, from: Option<usize>) -> (usize, bool) {
        let mut i = from.unwrap_or(self.scope_stack.len() - 1);
        let mut opaque = false;
        while i > 0 && self.scope_stack[i].internal {
            opaque |= self.scope_stack[i].opaque;
            i = self.scope_stack[i].barrier.unwrap_or(i - 1);
        }
        (i, opaque)
    }

    /// Push a frame whose lookups continue at frame `at`.
    fn push_scope_at(&mut self, at: usize) {
        let id = self.fresh_scope_id();
        self.push_frame(Scope::new(id, Some(at)));
    }

    /// Pop the top frame and move its bindings into frame `at`.
    fn pop_scope_into(&mut self, at: usize) {
        if self.scope_stack.len() > 1 {
            let top = self.scope_stack.pop().unwrap();
            self.scope_stack[at].bindings.extend(top.bindings);
        }
    }

    /// Push the trampoline's first frame, for a call to a function
    /// defined at `def`.
    fn push_tail_call_scope(&mut self, def: &DefScope) {
        if def.at_caller {
            let id = self.fresh_scope_id();
            let mut scope = Scope::new(id, Some(self.tail_base));
            scope.internal = def.internal;
            scope.opaque = def.opaque;
            self.push_frame(scope);
        } else {
            self.push_scope_defined_at(def);
        }
        self.scope_stack.last_mut().unwrap().tco = true;
    }

    /// Give a tail call to a function defined at `def` its frame, keeping
    /// `frames`, the count of frames the trampoline has pushed, up to date.
    /// The trampoline reuses the previous call's frame, cleared, when
    /// nothing else can see it: the callee was defined elsewhere, and no
    /// function defined to see the frame lives on outside the frame's own
    /// bindings. Otherwise the frame is someone's defining scope (the
    /// callee's, or a closure's that may still be called), so it stays and
    /// the callee gets a frame above it. A frame kept that way and no longer
    /// needed is dropped once it is under the top again, so tail loops run
    /// in one frame, even ones that make a function on each pass and drop
    /// it, or tail-call a function made in the pass.
    fn enter_tail_call(&mut self, def: &DefScope, frames: &mut usize) {
        // A composition continues at its caller's frame, which for a tail
        // call is the trampoline's caller.
        let barrier = if def.at_caller {
            self.tail_base
        } else {
            self.barrier_for(def)
        };
        loop {
            let top = self.scope_stack.len() - 1;
            if barrier >= top || self.scope_stack[top].still_seen() {
                self.push_tail_call_scope(def);
                *frames += 1;
                return;
            }
            if *frames > 1 {
                // The trampoline's frame below may be free now too.
                self.scope_stack.pop();
                *frames -= 1;
                continue;
            }
            self.clear_current_scope();
            let scope = &mut self.scope_stack[top];
            scope.barrier = Some(barrier);
            scope.internal = def.internal;
            scope.opaque = def.opaque;
            // Its lexical chain changes; a function defined from now on
            // takes a new token, holding the new chain's.
            scope.token = std::cell::OnceCell::new();
            scope.seen_at = 0;
            return;
        }
    }

    /// Where the defining scope at `def` is on the stack now: its frame if
    /// it is still live, else its innermost lexical ancestor that is (a
    /// function that outlived its block sees what is left of its scope, and
    /// never the frames of whoever called the function that made it).
    fn barrier_for(&self, def: &DefScope) -> usize {
        if def.at_caller {
            return self.scope_stack.len() - 1;
        }
        if def.ctx != self.ctx_id {
            return 0;
        }
        // Frame ids are never reused, so a matching id is the same frame.
        def.chain
            .frames()
            .iter()
            .find(|&&(index, id)| {
                self.scope_stack
                    .get(index)
                    .is_some_and(|scope| scope.id == id)
            })
            .map_or(0, |&(index, _)| index)
    }

    /// The token of trampoline frame `i`, holding that of the next
    /// trampoline frame out in its lexical chain.
    fn frame_token(&self, i: usize) -> Rc<FrameToken> {
        let scope = &self.scope_stack[i];
        if let Some(token) = scope.token.get() {
            return Rc::clone(token);
        }
        let mut j = i;
        let mut outer = None;
        while j > 0 {
            j = self.scope_stack[j].barrier.unwrap_or(j - 1);
            if self.scope_stack[j].tco {
                outer = Some(self.frame_token(j));
                break;
            }
        }
        Rc::clone(scope.token.get_or_init(|| Rc::new(FrameToken { outer })))
    }

    /// The defining scope for a function created now.
    fn def_scope(&self) -> DefScope {
        let mut keep = None;
        let mut frames = [(0, 0); DEF_CHAIN_INLINE];
        let mut len = 0;
        let mut heap: Option<Vec<(usize, u64)>> = None;
        let mut i = self.scope_stack.len() - 1;
        while i > 0 {
            let scope = &self.scope_stack[i];
            if scope.tco && keep.is_none() {
                keep = Some(self.frame_token(i));
            }
            match &mut heap {
                Some(v) => v.push((i, scope.id)),
                None if len < DEF_CHAIN_INLINE => {
                    frames[len] = (i, scope.id);
                    len += 1;
                }
                None => {
                    let mut v = frames.to_vec();
                    v.push((i, scope.id));
                    heap = Some(v);
                }
            }
            i = scope.barrier.unwrap_or(i - 1);
        }
        let chain = match heap {
            Some(v) => DefChain::Heap(Rc::from(v)),
            None => DefChain::Inline {
                len: len as u8,
                frames,
            },
        };
        DefScope {
            ctx: self.ctx_id,
            chain,
            internal: false,
            at_caller: false,
            opaque: false,
            keep,
        }
    }

    /// Pop the top scope from the stack
    fn pop_scope(&mut self) {
        if self.scope_stack.len() > 1 {
            self.scope_stack.pop();
        }
    }

    /// Clear all bindings in the top scope without deallocating. The frame
    /// gets a new id: a function defined before the clear no longer sees it.
    fn clear_current_scope(&mut self) {
        let id = self.fresh_scope_id();
        let top = self.scope_stack.last_mut().unwrap();
        top.bindings.clear();
        top.id = id;
    }

    pub fn bind(&mut self, name: String, value: JValue) {
        self.scope_stack
            .last_mut()
            .unwrap()
            .bindings
            .insert(name, value);
    }

    /// Bind `name` in the top scope, returning what the top scope held for
    /// it, for `restore_top`.
    fn bind_top(&mut self, name: &str, value: JValue) -> Option<JValue> {
        let top = self.scope_stack.last_mut().unwrap();
        top.bindings.insert(name.to_string(), value)
    }

    /// Put back what `bind_top` replaced in the top scope.
    fn restore_top(&mut self, name: &str, previous: Option<JValue>) {
        let top = self.scope_stack.last_mut().unwrap();
        match previous {
            Some(value) => top.bindings.insert(name.to_string(), value),
            None => top.bindings.remove(name),
        };
    }

    pub fn unbind(&mut self, name: &str) {
        // Remove from top scope only
        let top = self.scope_stack.last_mut().unwrap();
        top.bindings.remove(name);
    }

    pub fn lookup(&self, name: &str) -> Option<&JValue> {
        self.lookup_from(self.scope_stack.len() - 1, name)
    }

    /// Walk the scope stack down from frame `start`, jumping from a
    /// function's frame to the frame it was defined in.
    #[inline]
    fn lookup_from(&self, start: usize, name: &str) -> Option<&JValue> {
        let mut i = start;
        loop {
            let scope = &self.scope_stack[i];
            if let Some(value) = scope.bindings.get(name) {
                return Some(value);
            }
            // A barrier always points below its own frame.
            i = match scope.barrier {
                Some(barrier) => barrier,
                None if i == 0 => return None,
                None => i - 1,
            };
        }
    }

    /// Look `name` up as the body of a function defined at `def` would,
    /// before any of its own bindings.
    fn lookup_defined_at(&self, def: &DefScope, name: &str) -> Option<&JValue> {
        self.lookup_from(self.barrier_for(def), name)
    }

    /// Look up `name` and, if it is bound to a lambda value, hand out the
    /// shared closure handle (an Rc clone is a refcount bump).
    fn lookup_lambda_value(&self, name: &str) -> Option<&Rc<StoredLambda>> {
        match self.lookup(name) {
            Some(JValue::Lambda(rc)) => Some(rc),
            _ => None,
        }
    }

    pub fn set_parent(&mut self, data: JValue) {
        self.parent_data = Some(data);
    }

    pub fn get_parent(&self) -> Option<&JValue> {
        self.parent_data.as_ref()
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

/// Strip any lingering tuple-stream wrapper objects (`{"@":.., "__tuple__":true,
/// ...}`) from a value about to leave the evaluator.
///
/// `%`/`@`/`#` are implemented internally by wrapping each element of a path
/// step's result in a tuple object (see `create_tuple_stream`) so downstream
/// steps can resolve ancestor/focus/index bindings. Ordinarily an intermediate
/// path step consumes and re-wraps these as evaluation proceeds, but the
/// *final* result of an `evaluate()` call can still be tuple-wrapped — either
/// because the tuple-producing expression itself is the whole result (a bare
/// `#`/`@`/`%` path), or because it's nested inside object/array construction
/// (e.g. `{"skus": Product[%.OrderID=...].SKU}` or `[items#$i]`) where the
/// wrapper ends up embedded in a field value or array element rather than at
/// the top level. This recurses through both array elements and (non-tuple)
/// object field values so both shapes are cleaned up, not just a bare
/// top-level tuple array.
/// Merge a group of tuple wrappers into a single tuple, appending each key's
/// values across the group. Mirrors jsonata-js `reduceTupleStream`
/// (`Object.assign(result, tuple[0]); result[prop] = append(result[prop], ...)`):
/// a key present in one tuple stays a scalar; a key present in several becomes an
/// array of the collected values (used by group-by value evaluation so a group
/// of N tuples exposes `@` as the N collected `@` values and each `$focus` as the
/// N collected focus values).
fn reduce_tuple_stream(group: &[JValue]) -> IndexMap<String, JValue> {
    fn append(acc: Option<JValue>, v: JValue) -> JValue {
        match acc {
            None => v,
            Some(a) => {
                let mut out: Vec<JValue> = match a {
                    JValue::Array(arr) => arr.iter().cloned().collect(),
                    other => vec![other],
                };
                match v {
                    JValue::Array(arr) => out.extend(arr.iter().cloned()),
                    other => out.push(other),
                }
                JValue::array(out)
            }
        }
    }
    let mut result: IndexMap<String, JValue> = IndexMap::new();
    for tuple in group {
        if let JValue::Object(obj) = tuple {
            for (k, v) in obj.iter() {
                if k == "__tuple__" {
                    result.insert(k.clone(), v.clone());
                    continue;
                }
                let merged = append(result.shift_remove(k), v.clone());
                result.insert(k.clone(), merged);
            }
        }
    }
    result
}

fn unwrap_tuple_output(value: JValue) -> JValue {
    with_stack(|| unwrap_tuple_output_level(value))
}

fn unwrap_tuple_output_level(value: JValue) -> JValue {
    match value {
        JValue::Object(obj) if is_tuple_object(&obj) => obj
            .get("@")
            .cloned()
            .map(unwrap_tuple_output)
            .unwrap_or(JValue::Undefined),
        JValue::Object(obj) => {
            let mut new_map = IndexMap::with_capacity(obj.len());
            for (k, v) in obj.iter() {
                new_map.insert(k.clone(), unwrap_tuple_output(v.clone()));
            }
            JValue::object(new_map)
        }
        JValue::Array(arr) => {
            let mut values = arr.with_items(arr.iter().cloned().map(unwrap_tuple_output).collect());
            // The items are values now, not sorted tuples.
            values.sorted_tuples = false;
            JValue::Array(values)
        }
        other => other,
    }
}

/// Guard returned by [`Evaluator::bind_tuple_keys`]: remembers, for each
/// tuple-carried `$name`/`!label` key that was just bound into scope, what
/// (if anything) was bound under that name beforehand. `restore` puts the
/// prior value back -- or removes the binding entirely if there wasn't
/// one -- rather than unconditionally unbinding, so a tuple key that
/// happens to share a name with a live outer `:=` binding in the same
/// scope frame doesn't get permanently deleted once the tuple-row
/// evaluation finishes.
struct TupleKeyBindings {
    saved: Vec<(String, Option<JValue>)>,
}

impl TupleKeyBindings {
    fn restore(self, evaluator: &mut Evaluator) {
        for (name, prior) in self.saved {
            match prior {
                Some(value) => evaluator.context.bind(name, value),
                None => evaluator.context.unbind(&name),
            }
        }
    }
}

/// A handle that stops an evaluation from another thread.
///
/// Clones share one flag. Put a clone in [`EvaluatorOptions::cancel`] and call
/// [`CancelToken::cancel`] on another, and the evaluation raises U1003 at its
/// next checkpoint: each node it evaluates, each element a higher-order
/// builtin (`$map`, `$filter`, `$reduce`, `$single`, `$sift`, `$each`,
/// `$sort`) hands its callback, each match `$replace` hands its replacement
/// function, and each pass of a tail-call loop. These are the places
/// `timeout_ms` is checked.
///
/// A single builtin call over a large input, such as `$string` of a long
/// array, finishes before the flag is seen. If that call is the last step,
/// the evaluation returns its result even though the token was cancelled, so
/// check [`CancelToken::is_cancelled`] after it returns when that matters.
///
/// An [`Evaluator`] keeps its options, so one built with a cancelled token
/// fails every later evaluation. Pass a fresh token with each call, as below,
/// or build a new `Evaluator`.
///
/// ```
/// use jsonata_core::evaluator::{CancelToken, EvaluatorOptions};
/// use jsonata_core::value::JValue;
/// use jsonata_core::Expression;
///
/// let expr = Expression::compile("($inf := function(){ $inf() }; $inf())")?;
///
/// let token = CancelToken::new();
/// let canceller = token.clone();
/// std::thread::spawn(move || {
///     std::thread::sleep(std::time::Duration::from_millis(10));
///     canceller.cancel();
/// });
/// let options = EvaluatorOptions {
///     cancel: Some(token),
///     ..Default::default()
/// };
/// let err = expr.evaluate_with_options(&JValue::Null, options).unwrap_err();
/// assert!(err.to_string().contains("U1003"));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug, Default)]
pub struct CancelToken(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl CancelToken {
    /// A token that has not been cancelled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Stop every evaluation that holds a clone of this token. It cannot be
    /// reset.
    pub fn cancel(&self) {
        self.0.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Whether [`CancelToken::cancel`] has been called on any clone.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// Resource-limit guardrails, mirroring jsonata-js 2.2.1's `timeout`/`stack`/`sequence`
/// evaluator options. All fields default to `None` = unlimited (current behavior).
#[derive(Default, Clone, Debug)]
pub struct EvaluatorOptions {
    /// Maximum wall-clock evaluation time in milliseconds. Exceeding it raises D1012.
    pub timeout_ms: Option<u64>,
    /// Stops the evaluation with U1003 once the token is cancelled, from any
    /// thread. jsonata-js has no equivalent; see [`CancelToken`].
    pub cancel: Option<CancelToken>,
    /// Maximum evaluation stack depth, like jsonata-js's `stack` option. Exceeding
    /// it raises D1011. Without it, evaluation may nest up to the built-in
    /// `MAX_EVAL_DEPTH` (30,000 levels, about 10,000 non-tail recursive calls),
    /// beyond which U1001 is raised; a value at or above that limit changes
    /// nothing (GitHub issues #34, #200).
    pub max_stack_depth: Option<usize>,
    /// Maximum length of a query-result sequence (map/filter/wildcard/descendants/
    /// keys/lookup/append/spread/each/range/path-mapping). Exceeding it raises D2015.
    /// Does NOT currently apply to literal array construction (`MakeArray`/
    /// `ArrayConstruct`) — NOTE this is a deliberate, temporary divergence from
    /// upstream, not a match: jsonata-js DOES cap flat/non-nested array literals
    /// (via `fn.append`'s `createSequence` hook in `evaluateUnary`'s `[` case).
    /// Deferred until the separate `MakeArray(u16)` truncation bug is fixed (see
    /// the design spec's "Sequence length → D2015" section).
    pub max_sequence_length: Option<usize>,
}

/// The values under `*`, following jsonata-js `evaluateWildcard`: the
/// values of an object's fields, or an array's items, with each array value
/// flattened completely. An array value is spliced in with `fn.append`,
/// whose `concat` returns a plain array, so the result is a sequence only
/// when no value was an array.
fn wildcard_values<'a>(values: impl Iterator<Item = &'a JValue>) -> JValue {
    fn flatten_into(value: &JValue, out: &mut Vec<JValue>) {
        match value {
            JValue::Array(items) => {
                with_stack(|| items.iter().for_each(|item| flatten_into(item, out)))
            }
            other => out.push(other.clone()),
        }
    }
    let mut items = Vec::new();
    let mut appended_array = false;
    for value in values {
        if matches!(value, JValue::Array(_)) {
            appended_array = true;
        }
        flatten_into(value, &mut items);
    }
    if appended_array {
        JValue::array(items)
    } else {
        JValue::sequence(items)
    }
}

/// The array a higher-order function hands its callback as the last
/// argument: the input array itself, flags and all, as jsonata-js passes it,
/// or the items of a non-array input wrapped as a plain array.
fn callback_array_arg(input: &JValue, items: &[JValue]) -> JValue {
    match input {
        JValue::Array(arr) => JValue::Array(arr.clone()),
        _ => JValue::array(items.to_vec()),
    }
}

/// What an object constructor evaluates a value over when every item of its
/// input is in that pair's group, as with a literal key (jsonata-js
/// `evaluateGroupExpression`): the context of a group of all its items
/// (`group_items`). A value that is not an array is its own context.
pub(crate) fn object_context(data: &JValue) -> Cow<'_, JValue> {
    match data {
        JValue::Array(arr) if arr.outer_wrapper => Cow::Owned(data.bare()),
        JValue::Array(_) => Cow::Owned(group_context(&group_items(data))),
        _ => Cow::Borrowed(data),
    }
}

/// The items an object constructor groups (jsonata-js
/// `evaluateGroupExpression`): a value that is not an array is the one item,
/// the top-level input's wrapper holds the bare array, and an empty array
/// holds a single undefined item.
fn group_items(data: &JValue) -> Cow<'_, [JValue]> {
    match data {
        JValue::Array(arr) if arr.outer_wrapper => Cow::Owned(vec![data.bare()]),
        JValue::Array(arr) if arr.is_empty() => Cow::Owned(vec![JValue::Undefined]),
        JValue::Array(arr) => Cow::Borrowed(arr.as_slice()),
        other => Cow::Borrowed(std::slice::from_ref(other)),
    }
}

/// The first literal key that more than one pair of an object constructor
/// uses: jsonata-js raises D1009 for it, whatever the input.
fn duplicate_literal_key(pairs: &[(AstNode, AstNode)]) -> Option<&str> {
    let key = |index: usize| match &pairs[index].0 {
        AstNode::String(key) => Some(&**key),
        _ => None,
    };
    if pairs.len() <= 16 {
        return (1..pairs.len()).find_map(|i| key(i).filter(|k| (0..i).any(|j| key(j) == Some(k))));
    }
    let mut seen = HashSet::with_capacity(pairs.len());
    (0..pairs.len()).find_map(|i| key(i).filter(|k| !seen.insert(*k)))
}

/// jsonata-js's D1009, for two pairs of an object constructor giving `key`.
fn duplicate_key_error(key: &str) -> EvaluatorError {
    let key = serde_json::to_string(key).unwrap_or_else(|_| key.to_string());
    EvaluatorError::EvaluationError(format!(
        "D1009: Multiple key definitions evaluate to same key: {key}"
    ))
}

/// The variable a higher-order function's evaluated callback is bound to
/// (see `evaluate_builtin_call`). It holds a space, so no expression can
/// name it.
const HOF_CALLBACK: &str = "hof callback";

/// Which of a higher-order function's `arg_count` source arguments is its
/// callback, for the ones that apply it from its source argument.
fn hof_callback_index(name: &str, arg_count: usize) -> Option<usize> {
    match (name, arg_count) {
        ("map" | "filter" | "reduce" | "single" | "sort", 2..) => Some(1),
        ("sift" | "each", 1) => Some(0),
        ("sift" | "each", 2..) => Some(1),
        _ => None,
    }
}

/// The builtin name a regex match's `next` calls (see `Evaluator::call_regex`).
/// It is only ever bound inside `next`, and holds a space, so no expression
/// can name it.
const REGEX_NEXT: &str = "regex next";

/// The text a regex function value matches against, as JavaScript's
/// `RegExp.prototype.exec` converts its argument with `String()`. `None`
/// (the call gives undefined) for undefined, which jsonata-js would match as
/// the text "undefined", and for a function.
fn regex_subject(value: &JValue) -> Option<String> {
    fn js_string(value: &JValue) -> Option<String> {
        Some(match value {
            JValue::String(s) => s.to_string(),
            JValue::Number(n) => crate::value::js_number_to_string(*n),
            JValue::Bool(b) => b.to_string(),
            JValue::Null => "null".to_string(),
            // Array.prototype.toString: the items joined by commas, with
            // null and undefined as empty text.
            JValue::Array(items) => {
                let mut parts = Vec::with_capacity(items.len());
                for item in items.iter() {
                    parts.push(match item {
                        JValue::Null | JValue::Undefined => String::new(),
                        item => js_string(item)?,
                    });
                }
                parts.join(",")
            }
            JValue::Object(_) => "[object Object]".to_string(),
            #[cfg(feature = "python")]
            JValue::LazyPyDict(_) => "[object Object]".to_string(),
            JValue::Undefined
            | JValue::Lambda(_)
            | JValue::Builtin { .. }
            | JValue::Regex { .. } => return None,
        })
    }
    js_string(value)
}

/// Where a regex function value starts looking, from its second argument:
/// jsonata-js sets `lastIndex` to `fromIndex || 0`, which JavaScript reads
/// as a whole, non-negative number. A higher-order function's index or key
/// lands here (`$map(["ab", "ab"], /b/)` looks from 0, then 1).
fn regex_from_index(value: &JValue) -> usize {
    let n = match value {
        JValue::Number(n) => *n,
        JValue::Bool(b) => f64::from(u8::from(*b)),
        JValue::String(s) => {
            let s = s.trim();
            if s.is_empty() {
                0.0
            } else {
                s.parse::<f64>().unwrap_or(f64::NAN)
            }
        }
        _ => f64::NAN,
    };
    if n.is_nan() || n <= 0.0 {
        0
    } else if n >= usize::MAX as f64 {
        usize::MAX
    } else {
        n.floor() as usize
    }
}

/// jsonata-js's T1003, for an object key that is not a string.
fn key_type_error(key: &JValue) -> EvaluatorError {
    let key = match key {
        key if key.is_function() => format!("{key:?}"),
        _ => key.to_json_string().unwrap_or_else(|_| format!("{key:?}")),
    };
    EvaluatorError::TypeError(format!(
        "T1003: Key in object structure must evaluate to a string; got: {key}"
    ))
}

/// The value jsonata-js gives a group of items: the item itself when there
/// is one, else the items appended with `$append` into a plain array, so an
/// array item adds its own items.
fn group_context(items: &[JValue]) -> JValue {
    match items {
        [] => JValue::Undefined,
        [item] => item.clone(),
        _ => {
            let mut appended = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    JValue::Array(arr) => appended.extend(arr.iter().cloned()),
                    JValue::Undefined => {}
                    other => appended.push(other.clone()),
                }
            }
            JValue::array(appended)
        }
    }
}

/// The `limit` argument of `$match` and `$replace`: how many matches to
/// take. jsonata-js loops while `count < limit`, so a fractional limit
/// rounds up; a negative one is the function's error (`negative_limit`).
fn match_limit(
    value: Option<&JValue>,
    negative_limit: &str,
) -> Result<Option<usize>, EvaluatorError> {
    // The function's signature (`n?`) has already rejected any other value.
    match value {
        Some(JValue::Number(n)) if *n < 0.0 => {
            Err(EvaluatorError::EvaluationError(negative_limit.to_string()))
        }
        // `as` saturates, and a NaN limit matches nothing, as in jsonata-js.
        Some(JValue::Number(n)) => Ok(Some(n.ceil() as usize)),
        other => {
            debug_assert!(
                matches!(other, None | Some(JValue::Undefined)),
                "the signature admits only a number limit, got {other:?}"
            );
            Ok(None)
        }
    }
}

const MATCH_NEGATIVE_LIMIT: &str =
    "D3040: Third argument of match function must evaluate to a positive number";
const REPLACE_NEGATIVE_LIMIT: &str =
    "D3011: Fourth argument of replace function must evaluate to a positive number";

/// jsonata-js `evaluateMatcher`'s check on a custom matcher's result: a
/// value that isn't falsy must have a numeric `start`, an array `groups` or
/// a function `next`, else T1010. Undefined ends the matches, as in
/// jsonata-js. jsonata-js crashes reading a property of any other falsy
/// result, calling a missing `next`, or calling a builtin `next` (which it
/// can't apply); each of those ends the matches here instead.
fn check_matcher_result(result: &JValue, function: &str) -> Result<(), EvaluatorError> {
    let falsy = match result {
        JValue::Undefined | JValue::Null | JValue::Bool(false) => true,
        JValue::Number(n) => *n == 0.0 || n.is_nan(),
        JValue::String(s) => s.is_empty(),
        _ => false,
    };
    let well_formed = matches!(result, JValue::Object(obj) if
        matches!(obj.get("start"), Some(JValue::Number(_)))
            || matches!(obj.get("groups"), Some(JValue::Array(_)))
            || matches!(obj.get("next"), Some(JValue::Lambda(_) | JValue::Builtin { .. })));
    if falsy || well_formed {
        Ok(())
    } else {
        Err(EvaluatorError::TypeError(format!(
            "T1010: The matcher function argument passed to function \"{function}\" does not return the correct object structure"
        )))
    }
}

/// JavaScript `ToString` of a value, as `+` with a string applies it: an
/// array joins its items with `,` (null and undefined as nothing), and a
/// function is `[object Object]`, as jsonata-js's function values are
/// objects. A JSONata object has no `toString`, so jsonata-js fails on it;
/// that failure is an error here too.
fn js_to_string(value: &JValue) -> Result<String, EvaluatorError> {
    Ok(match value {
        JValue::Undefined => "undefined".to_string(),
        JValue::Null => "null".to_string(),
        JValue::Bool(b) => b.to_string(),
        JValue::Number(n) => crate::value::js_number_to_string(*n),
        JValue::String(s) => s.to_string(),
        JValue::Array(items) => {
            let mut joined = String::new();
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    joined.push(',');
                }
                if !matches!(item, JValue::Undefined | JValue::Null) {
                    joined.push_str(&js_to_string(item)?);
                }
            }
            joined
        }
        JValue::Lambda(_) | JValue::Builtin { .. } | JValue::Regex { .. } => {
            "[object Object]".to_string()
        }
        _ => {
            return Err(EvaluatorError::TypeError(
                "Cannot convert object to primitive value".to_string(),
            ))
        }
    })
}

/// JavaScript `ToNumber` of a value; a string or array goes through its
/// string form, and anything else that isn't a number, null or a boolean
/// is NaN.
fn js_to_number(value: &JValue) -> f64 {
    match value {
        JValue::Number(n) => *n,
        JValue::Null => 0.0,
        JValue::Bool(b) => f64::from(u8::from(*b)),
        JValue::String(s) => js_string_to_number(s),
        JValue::Array(_) => js_to_string(value).map_or(f64::NAN, |s| js_string_to_number(&s)),
        _ => f64::NAN,
    }
}

/// JavaScript `ToNumber` of a string: surrounding whitespace is ignored,
/// an empty string is 0, `0x`/`0o`/`0b` prefixes and `Infinity` are read,
/// and anything else that isn't a decimal number is NaN.
fn js_string_to_number(s: &str) -> f64 {
    let s = s.trim_matches(crate::functions::string::is_js_whitespace);
    if s.is_empty() {
        return 0.0;
    }
    for (prefix, radix) in [
        ("0x", 16),
        ("0X", 16),
        ("0o", 8),
        ("0O", 8),
        ("0b", 2),
        ("0B", 2),
    ] {
        if let Some(digits) = s.strip_prefix(prefix) {
            if digits.is_empty() {
                return f64::NAN;
            }
            return digits
                .chars()
                .try_fold(0.0, |n, c| {
                    c.to_digit(radix)
                        .map(|d| n * f64::from(radix) + f64::from(d))
                })
                .unwrap_or(f64::NAN);
        }
    }
    match s {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    if s.bytes()
        .all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'e' | b'E' | b'+' | b'-'))
    {
        s.parse().unwrap_or(f64::NAN)
    } else {
        f64::NAN
    }
}

/// JavaScript `String.prototype.substring` over UTF-16 code units: each
/// bound is clamped to the string (NaN is 0, a missing end is the end), and
/// the bounds are swapped if the end comes first.
fn js_substring(units: &[u16], start: f64, end: Option<f64>) -> &[u16] {
    let clamp = |bound: f64| {
        if bound.is_nan() {
            0
        } else {
            bound.clamp(0.0, units.len() as f64) as usize
        }
    };
    let (start, end) = (clamp(start), end.map_or(units.len(), clamp));
    &units[start.min(end)..start.max(end)]
}

/// True for a tuple-stream wrapper object (see `create_tuple_stream`). Every
/// wrapper is built with `@` as its first key, which rules out most plain
/// objects without hashing.
#[inline]
fn is_tuple_object(obj: &IndexMap<String, JValue>) -> bool {
    matches!(obj.get_index(0), Some((key, _)) if key == "@")
        && obj.get("__tuple__") == Some(&JValue::Bool(true))
}

/// True when an array's first item is a tuple wrapper, which is how a tuple
/// stream is recognised: every item of one is a wrapper.
#[inline]
fn starts_with_tuple(items: &[JValue]) -> bool {
    matches!(items.first(), Some(JValue::Object(obj)) if is_tuple_object(obj))
}

/// The items of an array of tuples a sort of a tuple stream returned, as
/// jsonata-js's filters, sorts and group-bys see them: each tuple is a plain
/// object, not its `@` value (see `JArray::sorted_tuples`). jsonata-js keys a
/// `#$i` or `@$v` binding by its bare name (`i`, `v`), while the wrappers here
/// key it as `$i`, so the leading `$` is dropped.
fn sorted_tuple_items(arr: &JArray) -> Vec<JValue> {
    arr.iter()
        .map(|item| match item {
            JValue::Object(obj) if is_tuple_object(obj) => {
                let plain: IndexMap<String, JValue> = obj
                    .iter()
                    .filter(|(key, _)| key.as_str() != "__tuple__")
                    .map(|(key, value)| {
                        let key = key.strip_prefix('$').unwrap_or(key);
                        (key.to_string(), value.clone())
                    })
                    .collect();
                JValue::object(plain)
            }
            other => other.clone(),
        })
        .collect()
}

/// The value a tuple-stream wrapper carries for its step (`@`).
fn tuple_focus_value(tuple: &JValue) -> JValue {
    match tuple {
        JValue::Object(obj) => obj.get("@").cloned().unwrap_or(JValue::Undefined),
        other => other.clone(),
    }
}

/// Combine a path step's per-item results the way jsonata-js
/// `evaluateStep` does. On the last step, a single result that is an array
/// but not a sequence is the step's result as is (the last-step rule).
/// Otherwise the results are flattened one level into a new sequence, except
/// constructed (`cons`) arrays, which stay whole.
fn collect_step_results(mut results: Vec<JValue>, last_step: bool) -> JValue {
    if last_step && results.len() == 1 && matches!(&results[0], JValue::Array(arr) if !arr.sequence)
    {
        return results.pop().expect("length checked");
    }
    // A single array flattens into a sequence of its own items, which can
    // share them.
    if let [JValue::Array(arr)] = results.as_slice() {
        if !arr.cons {
            return JValue::Array(arr.fresh_sequence());
        }
    }
    if !results.iter().any(flattens_in_step) {
        // Nothing to flatten: the results are the items, known flat (#196).
        return JValue::sequence_flat(results, true);
    }
    let mut items = Vec::with_capacity(results.len());
    for value in results {
        match value {
            JValue::Array(arr) if !arr.cons => items.extend(arr.iter().cloned()),
            other => items.push(other),
        }
    }
    JValue::sequence(items)
}

/// `collect_step_results` for a step with one result, without building the
/// one-item sequence: a value that is not an array stands for it (the path
/// loop takes it as one item, and the generic unwrap would return it).
fn collect_single_step_result(value: JValue, last_step: bool) -> JValue {
    match value {
        JValue::Array(arr) if last_step && !arr.sequence => JValue::Array(arr),
        JValue::Array(arr) if !arr.cons => JValue::Array(arr.fresh_sequence()),
        JValue::Array(arr) => JValue::sequence(vec![JValue::Array(arr)]),
        other => other,
    }
}

/// Apply a path's `[]` modifier to its result, following jsonata-js
/// `evaluatePath`: the result is marked keep-singleton, after a constructed
/// array that is not a sequence, or a single item, is first wrapped in a
/// sequence of its own.
fn keep_singleton(result: JValue) -> JValue {
    match result {
        JValue::Array(arr) if arr.cons && !arr.sequence => {
            let mut kept = JValue::sequence(vec![JValue::Array(arr)]);
            kept.mark_keep_singleton();
            kept
        }
        JValue::Array(mut arr) => {
            arr.keep_singleton = true;
            JValue::Array(arr)
        }
        JValue::Undefined => JValue::Undefined,
        // A single item stands for a one-item sequence.
        other => {
            let mut kept = JValue::sequence(vec![other]);
            kept.mark_keep_singleton();
            kept
        }
    }
}

/// Apply `[]` to a non-path expression's value, following jsonata-js
/// `evaluate`: only a sequence is marked keep-singleton.
fn keep_if_sequence(value: &mut JValue) {
    if matches!(value, JValue::Array(arr) if arr.sequence) {
        value.mark_keep_singleton();
    }
}

/// The value of a numeric-literal predicate (`[2]`, `[-1]`), which selects
/// by position; jsonata-js folds `-1` into a number literal.
fn numeric_literal(predicate: &AstNode) -> Option<f64> {
    match predicate {
        AstNode::Number(n) => Some(*n),
        AstNode::Unary {
            op: crate::ast::UnaryOp::Negate,
            operand,
        } => match **operand {
            AstNode::Number(n) => Some(-n),
            _ => None,
        },
        _ => None,
    }
}

/// The item at `position` (floored, counting from the end when negative).
fn item_at(items: &[JValue], position: f64) -> Option<&JValue> {
    let mut index = position.floor();
    if index < 0.0 {
        index += items.len() as f64;
    }
    (index >= 0.0).then(|| items.get(index as usize)).flatten()
}

/// Wrap a higher-order function's results as a JSONata *sequence*.
///
/// `$map`/`$filter` do not return arrays, they return sequences (jsonata-js
/// `createSequence()`): the generic unwrap at the expression boundary turns an
/// empty one into undefined and a single result into that result, unless a
/// `[]` keeps it, as in `$map(arr, $f)[]`.
pub(crate) fn hof_result_sequence(
    result: Vec<JValue>,
    options: &EvaluatorOptions,
) -> Result<JValue, EvaluatorError> {
    check_sequence_length(result.len(), options)?;
    Ok(JValue::sequence(result))
}

/// Checks a constructed query-result sequence's length against the configured
/// `max_sequence_length` guardrail. Call this at sites that build a query-result
/// sequence (map/filter/wildcard/descendants/keys/lookup/append/spread/each/range/
/// path-mapping). NOT currently called at literal array construction (`[1,2,3]`) —
/// unlike upstream jsonata-js, which caps flat/non-nested literals too via
/// `fn.append`'s `createSequence()` hook. See `EvaluatorOptions::max_sequence_length`
/// doc comment above for why this is a deliberate, temporary gap.
pub(crate) fn check_sequence_length(
    len: usize,
    options: &EvaluatorOptions,
) -> Result<(), EvaluatorError> {
    if let Some(max) = options.max_sequence_length {
        if len > max {
            return Err(EvaluatorError::EvaluationError(format!(
                "D2015: The maximum sequence length of {} was exceeded.",
                max
            )));
        }
    }
    Ok(())
}

/// U1003 once the caller has cancelled the evaluation. jsonata-js has no
/// cancellation and so no code for it; like U1001 and U1002 this one is ours.
#[inline(always)]
pub(crate) fn check_cancelled(options: &EvaluatorOptions) -> Result<(), EvaluatorError> {
    match &options.cancel {
        Some(token) if token.is_cancelled() => Err(EvaluatorError::EvaluationError(
            "U1003: Evaluation cancelled".to_string(),
        )),
        _ => Ok(()),
    }
}

/// Per-iteration D1012 and U1003 check for loops that don't pass through
/// `evaluate_internal`'s per-node checkpoint and would otherwise run untimed:
/// compiled map/filter/reduce element loops, FilterByBytecode, the tail-call
/// trampoline, and the higher-order builtins' loops over a builtin, host or
/// partial callback, whose call evaluates no node.
#[inline]
pub(crate) fn check_loop_timeout(
    options: &EvaluatorOptions,
    start_time: Option<Instant>,
) -> Result<(), EvaluatorError> {
    check_cancelled(options)?;
    if let Some(timeout_ms) = options.timeout_ms {
        if let Some(start) = start_time {
            if start.elapsed().as_millis() as u64 > timeout_ms {
                return Err(EvaluatorError::EvaluationError(format!(
                    "D1012: Evaluation timeout after {} milliseconds. Check for infinite loop",
                    timeout_ms
                )));
            }
        }
    }
    Ok(())
}

/// Result type returned by a host-registered function.
pub type HostFnResult = Result<JValue, EvaluatorError>;

/// Per-call context handed to a host function.
///
/// In v1 this is intentionally opaque: a host function takes data and returns
/// data. It exists so a later phase can add re-entrancy (invoking a JSONata
/// lambda passed as an argument) by widening this type, without changing the
/// [`HostFn`] signature or breaking existing callers.
#[non_exhaustive]
pub struct HostCtx {
    _private: (),
}

impl HostCtx {
    fn new() -> Self {
        HostCtx { _private: () }
    }
}

/// A function implemented by the host and callable from an expression as
/// `$name(...)`.
///
/// Most host functions are pure leaves: write them as a plain closure
/// `Fn(&[JValue]) -> Result<JValue, EvaluatorError>` (the blanket impl below
/// applies) and ignore the [`HostCtx`]. The trait form exists so a later phase
/// can hand the function a re-entrancy handle through [`HostCtx`] without an
/// API break.
pub trait HostFn {
    fn call(&self, args: &[JValue], ctx: &mut HostCtx) -> HostFnResult;
}

impl<F> HostFn for F
where
    F: Fn(&[JValue]) -> HostFnResult,
{
    fn call(&self, args: &[JValue], _ctx: &mut HostCtx) -> HostFnResult {
        self(args)
    }
}

/// Hard ceiling on nested `evaluate_internal` levels when the user has not set
/// a tighter `max_stack_depth` (GitHub issue #200). jsonata-js has no such
/// limit: its async evaluator recurses on the heap, so recursion is bounded
/// only by memory (`$f(1000000)` of a non-tail recursive `$f` evaluates under
/// node). Here deep levels run on heap-allocated stack segments (see
/// `EVAL_STACK_SEGMENT`), so this is a memory backstop, not a native-stack
/// one: it turns a runaway recursion into U1001 instead of exhausting memory.
///
/// A non-tail recursive call costs about three levels (the call, the
/// conditional, the operator around the call), so this allows about 10,000
/// nested calls, at roughly 3KB of stack segment per level in a release
/// build (~90MB at the limit).
///
/// A recursive function can build a value as deep as its recursion. The value
/// walkers (conversion to and from Python, serialization, `$string`,
/// equality, `**`) grow their own stack through `value::with_stack`, but
/// dropping a `JValue` is still plain recursive drop glue: a release build
/// tears down roughly 10,000 levels of nesting on a 1MB stack, fewer for
/// arrays. That limit predates this one (`$reduce` can nest a value
/// arbitrarily deep) and is not raised by it.
const MAX_EVAL_DEPTH: usize = 30_000;

/// Stack headroom below which `evaluate_internal` moves to a new segment. It
/// must cover the native frames of one evaluation level (`evaluate_internal`
/// down to the next `evaluate_internal`, through function calls, lambda
/// invocation and builtin dispatch) plus the work done at a level that does
/// not re-enter `evaluate_internal`: compiling and running a compiled
/// sub-expression (at most `MAX_COMPILED_DEPTH` levels), regex matching, a
/// host function. Value walkers grow their own stack (`value::with_stack`).
/// Unoptimized builds use about eight times more stack per frame (compiling
/// one AST level takes ~9KB there), hence the larger value.
const EVAL_RED_ZONE: usize = if cfg!(debug_assertions) {
    2 * 1024 * 1024
} else {
    256 * 1024
};

/// Size of each heap-allocated stack segment `stacker` switches to.
const EVAL_STACK_SEGMENT: usize = 8 * 1024 * 1024;

/// Evaluator for JSONata expressions
pub struct Evaluator {
    context: Context,
    recursion_depth: usize,
    /// Set whenever `create_tuple_stream` builds a `{"@":.., "__tuple__":true}`
    /// wrapper during this top-level `evaluate()` call. Reset at the start of
    /// `evaluate()` and checked at the end to decide whether the (recursive,
    /// O(result size)) tuple-unwrap pass is needed before returning to the
    /// caller — keeps the vast majority of evaluations, which never touch
    /// `%`/`@`/`#`, at zero added cost.
    tuple_stream_created: bool,
    /// When true, `evaluate_path` skips its end-of-path `@`-projection and returns
    /// the raw `{@, $var, !label, __tuple__}` tuple wrappers. Set (saved/restored)
    /// by the two consumers that read those carried bindings directly from the
    /// wrappers: a `Sort` node evaluating its tuple-carrying input path (sort
    /// terms reference `%`/`$focus`), and an `ObjectTransform` (group-by)
    /// evaluating its input path (key/value expressions read `$focus` off the
    /// wrapper). Mirrors jsonata-js keeping `path.tuple` for such a path instead
    /// of projecting each tuple's `@`.
    keep_tuple_stream: bool,
    /// Set by `apply_with` just before a function is applied other than
    /// directly (see `ApplyMode`). The function entry points take it, so it
    /// marks only that one application.
    callback_apply: ApplyMode,
    /// Set while a builtin runs as a function's tail call: the callbacks it
    /// applies itself are applied raw (`ApplyMode::Raw`), not as callbacks.
    raw_hof: Option<RawHof>,
    /// The input of the running trampoline's `apply` (jsonata-js
    /// `apply(proc, args, input, env)`): null for a callback, the call
    /// site's input for a direct call. A builtin called in tail position
    /// runs with it as its `this.input` (context), not with the body's
    /// input. `None` outside a trampoline.
    tail_apply_input: Option<JValue>,
    options: EvaluatorOptions,
    /// Set in `evaluate()` (only when `options.timeout_ms` is configured) and
    /// checked in `evaluate_internal`'s per-node checkpoint for D1012.
    start_time: Option<Instant>,
    /// Host-registered custom functions, dispatched by name from the call
    /// position (`$name(...)`). Empty for the overwhelming majority of
    /// evaluators, which pay nothing. See `register_fn`/`register_fn_override`.
    host_fns: HashMap<String, Rc<dyn HostFn>>,
    /// Compiled bodies of the object-constructor and `.(expr)` steps this
    /// evaluator has run, by `PathStep::id` (`None`: does not compile), so a
    /// step compiles once per evaluator rather than each time it runs (#197).
    step_bodies: StepBodies,
}

/// The compiled-step cache: the step looked up last, which is the one asked
/// for again in the common case of one such step run over and over, and a
/// map of the others, allocated only once a second step is cached. Step ids
/// are unique integers, so the map uses them as their own hash.
#[derive(Default)]
struct StepBodies {
    last: Option<(u64, Option<Rc<CompiledBody>>)>,
    others: HashMap<u64, Option<Rc<CompiledBody>>, std::hash::BuildHasherDefault<StepIdHasher>>,
}

/// The cached entries are dropped all at once when there are this many,
/// which only an evaluator running `$eval` on new expressions again and
/// again reaches (each parse makes new steps).
const MAX_STEP_BODIES: usize = 4096;

impl StepBodies {
    /// The entry for step `id`, made by `compile` if there is none.
    fn get_or_compile(
        &mut self,
        id: u64,
        compile: impl FnOnce() -> Option<CompiledBody>,
    ) -> Option<Rc<CompiledBody>> {
        if let Some((last_id, compiled)) = &self.last {
            if *last_id == id {
                return compiled.clone();
            }
        }
        let compiled = match self.others.remove(&id) {
            Some(compiled) => compiled,
            None => compile().map(Rc::new),
        };
        if let Some((last_id, last)) = self.last.replace((id, compiled.clone())) {
            if self.others.len() >= MAX_STEP_BODIES {
                self.others.clear();
            }
            self.others.insert(last_id, last);
        }
        compiled
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.others.len() + usize::from(self.last.is_some())
    }
}

#[derive(Default)]
struct StepIdHasher(u64);

impl std::hash::Hasher for StepIdHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 << 8) | u64::from(b);
        }
    }
    fn write_u64(&mut self, n: u64) {
        self.0 = n;
    }
}

impl Evaluator {
    pub fn new() -> Self {
        Evaluator {
            context: Context::new(),
            recursion_depth: 0,
            tuple_stream_created: false,
            keep_tuple_stream: false,
            callback_apply: ApplyMode::Direct,
            raw_hof: None,
            tail_apply_input: None,
            options: EvaluatorOptions::default(),
            start_time: None,
            host_fns: HashMap::new(),
            step_bodies: StepBodies::default(),
        }
    }

    pub fn with_context(context: Context) -> Self {
        Evaluator {
            context,
            recursion_depth: 0,
            tuple_stream_created: false,
            keep_tuple_stream: false,
            callback_apply: ApplyMode::Direct,
            raw_hof: None,
            tail_apply_input: None,
            options: EvaluatorOptions::default(),
            start_time: None,
            host_fns: HashMap::new(),
            step_bodies: StepBodies::default(),
        }
    }

    /// Construct an `Evaluator` with guardrail options. `Evaluator::new()`/
    /// `with_context()` remain unchanged (unlimited options) for existing callers.
    pub fn with_options(context: Context, options: EvaluatorOptions) -> Self {
        Evaluator {
            context,
            recursion_depth: 0,
            tuple_stream_created: false,
            keep_tuple_stream: false,
            callback_apply: ApplyMode::Direct,
            raw_hof: None,
            tail_apply_input: None,
            options,
            start_time: None,
            host_fns: HashMap::new(),
            step_bodies: StepBodies::default(),
        }
    }

    /// Register a host function callable from an expression as `$name(...)`.
    ///
    /// `f` may be any plain closure `Fn(&[JValue]) -> Result<JValue,
    /// EvaluatorError>` (via the blanket [`HostFn`] impl) or any [`HostFn`]
    /// implementor. Host functions resolve *after* the expression's own `:=`
    /// bindings and language-defined functions, and *before* built-ins.
    ///
    /// Returns an error if `name` collides with a built-in function; to replace
    /// a built-in deliberately (e.g. a frozen `$now`), use
    /// [`register_fn_override`](Self::register_fn_override).
    ///
    /// # Examples
    ///
    /// ```
    /// use jsonata_core::evaluator::Evaluator;
    /// use jsonata_core::parser::parse;
    /// use jsonata_core::value::JValue;
    ///
    /// let mut ev = Evaluator::new();
    /// ev.register_fn("shout", |args: &[JValue]| {
    ///     let s = args.first().and_then(|v| v.as_str()).unwrap_or("");
    ///     Ok(JValue::from(s.to_uppercase()))
    /// })
    /// .expect("`shout` does not collide with a built-in");
    ///
    /// let ast = parse("$shout(greeting)").unwrap();
    /// let data = JValue::from_json_str(r#"{"greeting": "hi"}"#).unwrap();
    /// assert_eq!(ev.evaluate(&ast, &data).unwrap(), JValue::from("HI"));
    /// ```
    pub fn register_fn(
        &mut self,
        name: impl Into<String>,
        f: impl HostFn + 'static,
    ) -> Result<(), EvaluatorError> {
        let name = name.into();
        if self.is_builtin_function(&name) {
            return Err(EvaluatorError::EvaluationError(format!(
                "cannot register host function '{name}': it shadows a built-in; \
                 use register_fn_override to replace a built-in deliberately"
            )));
        }
        self.host_fns.insert(name, Rc::new(f));
        Ok(())
    }

    /// Register a host function that deliberately replaces a built-in of the same
    /// name — the two legitimate cases being determinism injection for the impure
    /// built-ins (`$now`, `$millis`, `$random`) and sandboxing/hardening (e.g.
    /// disabling `$eval`).
    ///
    /// Overriding a *compilable* built-in is rejected: those names can be folded
    /// into the bytecode fast path, which has no visibility into the host
    /// registry, so the override could be silently bypassed. The impure built-ins
    /// that motivate overriding are all non-compilable, so this restriction never
    /// blocks a legitimate use.
    pub fn register_fn_override(
        &mut self,
        name: impl Into<String>,
        f: impl HostFn + 'static,
    ) -> Result<(), EvaluatorError> {
        let name = name.into();
        if is_compilable_builtin(&name) {
            return Err(EvaluatorError::EvaluationError(format!(
                "cannot override built-in '{name}': it participates in the compiled \
                 fast path and cannot be safely shadowed in this version"
            )));
        }
        self.host_fns.insert(name, Rc::new(f));
        Ok(())
    }

    /// Invoke a stored lambda with its captured environment and data.
    /// This is the standard way to call a StoredLambda, handling the
    /// captured_env and captured_data extraction boilerplate.
    ///
    /// Takes the shared handle so the closure's `self_name` (if any) can be
    /// bound to the closure itself for the duration of the call — that is
    /// what makes `$f := function(){ ...$f()... }` work after `$f` has
    /// escaped its defining scope (late-bound letrec, no stored self-`Rc`).
    fn invoke_stored_lambda(
        &mut self,
        stored: &Rc<StoredLambda>,
        args: &[JValue],
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        let mode = std::mem::take(&mut self.callback_apply);
        // A partial application carries its own invocation recipe; no scope,
        // signature, or body evaluation of its own.
        if let Some(partial) = &stored.partial {
            let partial = Rc::clone(partial);
            return self.apply_with(mode, |ev| ev.invoke_partial(&partial, args, data));
        }
        if let Some(result) = self.invoke_transform(stored, args) {
            return result;
        }

        // Compiled fast path: skip scope push/pop and tree-walking for simple lambdas.
        if let Some(ce) = self.usable_compiled_body(stored) {
            let call_data = stored.captured_data.as_ref().unwrap_or(data);
            // A parameter without an argument is undefined, as on the
            // tree-walking path. A compiled body reads only its parameters.
            let undefined = JValue::Undefined;
            let vars: HashMap<&str, &JValue> = stored
                .params
                .iter()
                .enumerate()
                .map(|(i, p)| (p.as_str(), args.get(i).unwrap_or(&undefined)))
                .collect();
            return eval_compiled(ce, call_data, Some(&vars), &self.options, self.start_time);
        }

        let captured_env = if stored.captured_env.is_empty() {
            None
        } else {
            Some(&stored.captured_env)
        };
        let captured_data = stored.captured_data.as_ref();
        self.callback_apply = mode;
        self.invoke_lambda_with_env(
            &stored.params,
            &stored.body,
            stored.signature.as_ref(),
            args,
            data,
            captured_env,
            captured_data,
            stored.thunk,
            Some(stored),
        )
    }

    /// Get the number of parameters a callback function expects by inspecting its AST.
    /// This is used to avoid passing unnecessary arguments to callbacks in HOF functions.
    /// Returns the parameter count, or usize::MAX if unable to determine (meaning pass all args).
    fn get_callback_param_count(&self, func_node: &AstNode) -> usize {
        match func_node {
            AstNode::Lambda { params, .. } => params.len(),
            // A transform's function takes the one input it transforms.
            AstNode::Transform { .. } => 1,
            AstNode::Variable(var_name) => {
                // Check if this variable holds a lambda value (user-defined
                // function, closure, or partial application)
                if let Some(value) = self.context.lookup(var_name) {
                    if let JValue::Lambda(stored_lambda) = value {
                        return stored_lambda.params.len();
                    }
                    // jsonata-js's regex closure is `function(str, fromIndex)`.
                    if let JValue::Regex { .. } = value {
                        return 2;
                    }
                    // `$f := $uppercase` stores a `JValue::Builtin`, so the
                    // arity belongs to the builtin it names, not to `$f`.
                    // Without this the lookup below asks for the arity of "f"
                    // and gets MAX, and the callback is handed every argument
                    // the HOF has (#126 group 5).
                    if let JValue::Builtin { name } = value {
                        if !self.host_fns.is_empty() && self.host_fns.contains_key(&**name) {
                            return usize::MAX;
                        }
                        if let Some(arity) = crate::builtins::builtin_arity(name) {
                            return arity;
                        }
                    }
                }
                // A host-registered function (`register_fn`/`register_fn_override`)
                // shadows any built-in of the same name in call position (see
                // `evaluate_function_call`'s `host_fns` check), so its arity
                // here must not fall back to the built-in's table entry --
                // jsonata-js truncates to the *override's own*
                // `implementation.length`, which this Rust API has no way to
                // introspect for an arbitrary host closure. Return MAX
                // instead: an override receives every argument uncut, rather
                // than being silently truncated to a same-named builtin's
                // arity it may not share.
                if !self.host_fns.is_empty() && self.host_fns.contains_key(var_name) {
                    return usize::MAX;
                }
                // A builtin passed by reference (e.g. `$map(arr, $uppercase)`)
                // parses as this same AstNode::Variable shape. jsonata-js
                // truncates the callback's arguments to the underlying
                // JavaScript function's parameter count -- see
                // `builtins::builtin_arity`. Names it doesn't cover (unknown
                // variables) fall through to the safe MAX default below.
                if let Some(arity) = crate::builtins::builtin_arity(var_name) {
                    return arity;
                }
                // Unknown, return max to be safe
                usize::MAX
            }
            AstNode::Function { .. } => {
                // For function references, we can't easily determine param count
                // Return max to be safe
                usize::MAX
            }
            _ => usize::MAX,
        }
    }

    /// Specialized sort using pre-extracted keys (Schwartzian transform).
    /// Extracts sort keys once (N lookups), then sorts by comparing keys directly,
    /// avoiding O(N log N) hash lookups during comparisons.
    ///
    /// Returns `false` when the keys are not uniformly comparable, leaving the
    /// array untouched so the caller falls back to the general comparator. A
    /// sort comparator is an ordered comparison, so jsonata-js rejects a null,
    /// boolean, object or array key with T2010 and mixed number/string keys
    /// with T2009. This path cannot raise (it sorts in place), and duplicating
    /// the type rule here is what let it silently sort inputs the general path
    /// rejects -- so it declines instead and lets `compiled_ordered_cmp`, which
    /// owns the rule, produce the error.
    fn merge_sort_specialized(arr: &mut [JValue], spec: &SpecializedSortComparator) -> bool {
        if arr.len() <= 1 {
            return true;
        }

        // Phase 0: A present key that is neither number nor string is
        // uncomparable. An *absent* key is undefined, which sorts last without
        // raising, so it stays on the fast path.
        let mut uncomparable = false;
        let mut saw_num = false;
        let mut saw_str = false;
        // Classify without cloning: the Object case is the hot one and only the
        // variant matters.
        enum KeyKind {
            Num,
            Str,
            Absent,
            Uncomparable,
        }
        fn kind(v: Option<&JValue>) -> KeyKind {
            match v {
                Some(JValue::Number(_)) => KeyKind::Num,
                Some(JValue::String(_)) => KeyKind::Str,
                None | Some(JValue::Undefined) => KeyKind::Absent,
                Some(_) => KeyKind::Uncomparable,
            }
        }
        for item in arr.iter() {
            let k = match item {
                JValue::Object(obj) => kind(obj.get(&spec.field)),
                #[cfg(feature = "python")]
                JValue::LazyPyDict(lazy) => match lazy.get_field(&spec.field) {
                    Ok(v) => kind(Some(&v)),
                    Err(_) => KeyKind::Absent,
                },
                _ => KeyKind::Absent,
            };
            match k {
                KeyKind::Num => saw_num = true,
                KeyKind::Str => saw_str = true,
                KeyKind::Absent => {}
                KeyKind::Uncomparable => {
                    uncomparable = true;
                    break;
                }
            }
        }
        if uncomparable || (saw_num && saw_str) {
            return false;
        }

        // Phase 1: Extract sort keys -- one IndexMap lookup per element
        let keys: Vec<SortKey> = arr
            .iter()
            .map(|item| match item {
                JValue::Object(obj) => match obj.get(&spec.field) {
                    Some(JValue::Number(n)) => SortKey::Num(*n),
                    Some(JValue::String(s)) => SortKey::Str(s.clone()),
                    _ => SortKey::None,
                },
                #[cfg(feature = "python")]
                JValue::LazyPyDict(lazy) => match lazy.get_field(&spec.field) {
                    Ok(JValue::Number(n)) => SortKey::Num(n),
                    Ok(JValue::String(s)) => SortKey::Str(s.clone()),
                    // Err(_) (conversion failure) and any other value fall through to
                    // SortKey::None (treated as "missing", sorts last). This arm is only
                    // reachable for elements that survived upstream evaluation of the sort
                    // array (T2008 gate / a prior get_field on the same data), so a
                    // conversion error swallowed here cannot silently mis-sort *today* --
                    // if this specialized path is ever reached before such validation,
                    // this arm must be revisited to propagate the error instead.
                    _ => SortKey::None,
                },
                _ => SortKey::None,
            })
            .collect();

        // Phase 2: Build index permutation sorted by pre-extracted keys
        let mut perm: Vec<usize> = (0..arr.len()).collect();
        perm.sort_by(|&a, &b| compare_sort_keys(&keys[a], &keys[b], spec.descending));

        // Phase 3: Apply permutation in-place via cycle-following
        let mut placed = vec![false; arr.len()];
        for i in 0..arr.len() {
            if placed[i] || perm[i] == i {
                continue;
            }
            let mut j = i;
            loop {
                let target = perm[j];
                placed[j] = true;
                if target == i {
                    break;
                }
                arr.swap(j, target);
                j = target;
            }
        }
        true
    }

    /// Merge sort implementation using a comparator function.
    /// This replaces the O(n²) bubble sort for better performance on large arrays.
    /// The comparator returns true if the first element should come AFTER the second.
    fn merge_sort_with_comparator(
        &mut self,
        arr: &mut [JValue],
        comparator: &AstNode,
        data: &JValue,
    ) -> Result<(), EvaluatorError> {
        if arr.len() <= 1 {
            return Ok(());
        }

        // Try specialized fast path for simple field comparisons like
        // function($l, $r) { $l.price > $r.price }
        if let AstNode::Lambda { params, body, .. } = comparator {
            if params.len() >= 2 {
                if let Some(spec) = try_specialize_sort_comparator(body, &params[0], &params[1]) {
                    // Falls through to the general comparator when the keys are
                    // not uniformly comparable, so the type error is raised.
                    if Self::merge_sort_specialized(arr, &spec) {
                        return Ok(());
                    }
                }
            }
        }

        let mid = arr.len() / 2;

        // Sort left half
        self.merge_sort_with_comparator(&mut arr[..mid], comparator, data)?;

        // Sort right half
        self.merge_sort_with_comparator(&mut arr[mid..], comparator, data)?;

        // Merge the sorted halves
        let mut temp = Vec::with_capacity(arr.len());
        let (left, right) = arr.split_at(mid);

        let mut i = 0;
        let mut j = 0;

        // For lambda comparators, use a reusable scope to avoid
        // push_scope/pop_scope per comparison (~n log n total comparisons)
        // Not for a body with tail calls: that runs as a function call, whose
        // tail builtin's raw result (`$keys({})`, an empty sequence) the
        // comparator then tests (#250).
        if let AstNode::Lambda {
            params,
            body,
            thunk: false,
            ..
        } = comparator
        {
            if params.len() >= 2 {
                // Pre-clone param names once outside the loop
                let param0 = params[0].clone();
                let param1 = params[1].clone();
                self.context.push_scope();
                while i < left.len() && j < right.len() {
                    // Reuse scope: clear and rebind instead of push/pop
                    self.context.clear_current_scope();
                    self.context.bind(param0.clone(), left[i].clone());
                    self.context.bind(param1.clone(), right[j].clone());

                    let cmp_result = self.evaluate_internal(body, data)?;

                    if js_truthy(&cmp_result) {
                        temp.push(right[j].clone());
                        j += 1;
                    } else {
                        temp.push(left[i].clone());
                        i += 1;
                    }
                }
                self.context.pop_scope();
            } else {
                // Unexpected param count - fall back to generic path
                while i < left.len() && j < right.len() {
                    check_loop_timeout(&self.options, self.start_time)?;
                    let cmp_result = self.apply_function(
                        comparator,
                        &[left[i].clone(), right[j].clone()],
                        data,
                    )?;
                    if js_truthy(&cmp_result) {
                        temp.push(right[j].clone());
                        j += 1;
                    } else {
                        temp.push(left[i].clone());
                        i += 1;
                    }
                }
            }
        } else {
            // Non-lambda comparator: use generic apply_function path
            while i < left.len() && j < right.len() {
                check_loop_timeout(&self.options, self.start_time)?;
                let cmp_result =
                    self.apply_function(comparator, &[left[i].clone(), right[j].clone()], data)?;
                if js_truthy(&cmp_result) {
                    temp.push(right[j].clone());
                    j += 1;
                } else {
                    temp.push(left[i].clone());
                    i += 1;
                }
            }
        }

        // Copy remaining elements
        temp.extend_from_slice(&left[i..]);
        temp.extend_from_slice(&right[j..]);

        // Copy back to original array (can't use copy_from_slice since JValue is not Copy)
        for (i, val) in temp.into_iter().enumerate() {
            arr[i] = val;
        }

        Ok(())
    }

    /// Evaluate an AST node against data
    ///
    /// This is the main entry point for evaluation. It sets up the parent context
    /// to be the root data if not already set.
    ///
    /// Also the single choke point for stripping any lingering tuple-stream
    /// wrapper objects (`{"@":.., "__tuple__":true, ...}`) from the result before
    /// it reaches the caller — `%`/`@`/`#` are implemented internally via a
    /// tuple-stream representation (see `create_tuple_stream`), and without this
    /// a bare (or object/array-nested) tuple-producing expression would leak
    /// that internal representation into user-visible output instead of the
    /// plain value.
    ///
    /// Each evaluation runs in a scope of its own, as jsonata-js runs each in
    /// a new frame of its environment, so nothing it binds outlives it:
    /// bindings made with `Context::bind` beforehand are visible, and those
    /// the evaluation makes are gone afterwards, whether it succeeded or
    /// failed. Bindings only ever go into the top scope, so dropping every
    /// scope above the entry depth undoes all of them, including any an error
    /// skipped popping. `$$` is the evaluation's input unless the caller set a
    /// parent on the context.
    pub fn evaluate(&mut self, node: &AstNode, data: &JValue) -> Result<JValue, EvaluatorError> {
        let entry_depth = self.context.scope_stack.len();
        let caller_parent = self.context.get_parent().is_some();
        self.context.push_scope();
        let result = self.evaluate_from_entry(node, data);
        debug_assert!(
            result.is_err() || self.context.scope_stack.len() == entry_depth + 1,
            "a successful evaluation pops every scope it pushes"
        );
        self.context.scope_stack.truncate(entry_depth);
        if !caller_parent {
            self.context.parent_data = None;
        }
        self.keep_tuple_stream = false;
        result
    }

    fn evaluate_from_entry(
        &mut self,
        node: &AstNode,
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        // Set parent context to root data if not already set
        if self.context.get_parent().is_none() {
            self.context.set_parent(data.clone());
        }

        if self.options.timeout_ms.is_some() {
            self.start_time = Some(Instant::now());
        }

        self.tuple_stream_created = false;
        let data = &data.clone().into_outer_wrapper();
        let result = self.evaluate_internal(node, data)?;
        Ok(if self.tuple_stream_created {
            unwrap_tuple_output(result)
        } else {
            result
        })
    }

    /// Fast evaluation for leaf nodes that don't need recursion tracking.
    /// Returns Some for literals, simple field access on objects, and simple variable lookups.
    /// Returns None for anything requiring the full evaluator.
    #[inline(always)]
    fn evaluate_leaf(
        &mut self,
        node: &AstNode,
        data: &JValue,
    ) -> Option<Result<JValue, EvaluatorError>> {
        match node {
            AstNode::String(s) => Some(Ok(JValue::string(s.clone()))),
            AstNode::Number(n) => {
                if n.fract() == 0.0 && n.is_finite() && n.abs() < (1i64 << 53) as f64 {
                    Some(Ok(JValue::from(*n as i64)))
                } else {
                    Some(Ok(JValue::Number(*n)))
                }
            }
            AstNode::Boolean(b) => Some(Ok(JValue::Bool(*b))),
            AstNode::Null => Some(Ok(JValue::Null)),
            AstNode::Undefined => Some(Ok(JValue::Undefined)),
            AstNode::Name(field_name) => match data {
                // Array mapping and other cases need full evaluator
                JValue::Object(obj) => Some(Ok(obj
                    .get(field_name)
                    .cloned()
                    .unwrap_or(JValue::Undefined))),
                #[cfg(feature = "python")]
                JValue::LazyPyDict(lazy) => {
                    Some(lazy.get_field(field_name).map_err(EvaluatorError::from))
                }
                _ => None,
            },
            AstNode::Variable(name) if !name.is_empty() => {
                // Simple variable lookup — only fast-path when no tuple data
                if let JValue::Object(obj) = data {
                    if is_tuple_object(obj) {
                        return None; // Tuple data needs full evaluator
                    }
                }
                // May be a lambda/builtin — needs full evaluator if None
                self.context.lookup(name).map(|value| Ok(value.clone()))
            }
            _ => None,
        }
    }

    /// Evaluate a node, applying jsonata-js's generic unwrap to its result
    /// (`JValue::unwrap_sequence`), as jsonata-js `evaluate` does after
    /// every expression.
    fn evaluate_internal(
        &mut self,
        node: &AstNode,
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        self.evaluate_unwrapped_later(node, data)
            .map(JValue::unwrap_sequence)
    }

    /// Evaluate a node without the final generic unwrap, for a caller that
    /// still has to apply the node's predicates or `[]` to the raw sequence
    /// first, as jsonata-js `evaluate` does before unwrapping.
    fn evaluate_unwrapped_later(
        &mut self,
        node: &AstNode,
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        // Fast path for leaf nodes — skip recursion tracking overhead
        if let Some(result) = self.evaluate_leaf(node, data) {
            return result;
        }

        // A request to keep tuple streams reaches only a tail path: jsonata-js
        // marks a path or a block `tuple`, never the expressions inside them.
        // A sort is a step of a path there.
        if self.keep_tuple_stream
            && !matches!(
                node,
                AstNode::Path { .. }
                    | AstNode::Sort { .. }
                    | AstNode::Block(_)
                    | AstNode::FunctionApplication(_)
            )
        {
            self.keep_tuple_stream = false;
            let result = self.evaluate_unwrapped_later(node, data);
            self.keep_tuple_stream = true;
            return result;
        }

        self.with_checkpoint(|ev| ev.evaluate_internal_impl(node, data))
    }

    /// Run one level of evaluation under the recursion-depth, cancellation,
    /// timeout and native-stack checks every node evaluation makes. Inlined so
    /// the hot node-evaluation path pays no extra call frame for it. The trampoline's
    /// own recursion paths that bypass `evaluate_internal` (a tail call of a
    /// higher-order builtin or of a composition) run under it too, so no
    /// recursion cycle grows the native stack unchecked (#191).
    #[inline(always)]
    fn with_checkpoint<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, EvaluatorError>,
    ) -> Result<T, EvaluatorError> {
        // Depth guard. A user `max_stack_depth` raises D1011, like jsonata-js's
        // `stack` option. Without one, the only limit is `MAX_EVAL_DEPTH`, a
        // memory backstop (U1001) far deeper than real recursion needs: the
        // native stack itself cannot overflow, since every level passes
        // through the `stacker::maybe_grow` below (GitHub issues #34, #200).
        self.recursion_depth += 1;
        let effective_limit = match self.options.max_stack_depth {
            Some(limit) => limit.min(MAX_EVAL_DEPTH),
            None => MAX_EVAL_DEPTH,
        };
        if self.recursion_depth > effective_limit {
            self.recursion_depth -= 1;
            return Err(EvaluatorError::EvaluationError(
                if effective_limit < MAX_EVAL_DEPTH {
                    "D1011: Stack overflow. Check for non-terminating recursive function. Consider rewriting as tail-recursive".to_string()
                } else {
                    // jsonata-js has no U1001 of its own; its test runner's
                    // depth guard (`timeboxExpression`) raises one worded
                    // "Stack overflow error: Check for non-terminating
                    // recursive function.  Consider rewriting as
                    // tail-recursive." This keeps that wording and names the
                    // limit that was hit (GitHub issue #251).
                    format!(
                        "U1001: Stack overflow error: evaluation nested deeper than {} levels. Check for non-terminating recursive function. Consider rewriting as tail-recursive.",
                        MAX_EVAL_DEPTH
                    )
                },
            ));
        }

        if let Err(e) = check_cancelled(&self.options) {
            self.recursion_depth -= 1;
            return Err(e);
        }

        // Check evaluation timeout (D1012). `start_time` is only set (in
        // `evaluate()`) when `options.timeout_ms` is configured, so this is a
        // single `is_none()` branch of overhead when no timeout is set.
        if let Some(timeout_ms) = self.options.timeout_ms {
            if let Some(start) = self.start_time {
                if start.elapsed().as_millis() as u64 > timeout_ms {
                    self.recursion_depth -= 1;
                    return Err(EvaluatorError::EvaluationError(format!(
                        "D1012: Evaluation timeout after {} milliseconds. Check for infinite loop",
                        timeout_ms
                    )));
                }
            }
        }

        // Run the node on a fresh heap-allocated stack segment whenever less
        // than `EVAL_RED_ZONE` of the current one is left, so deep recursion
        // never overruns the thread's real stack, however small (Windows'
        // 1MB main thread included). Every recursive evaluation cycle passes
        // through here; see `EVAL_RED_ZONE` for what the red zone covers. On
        // the common shallow path this is one stack-pointer compare.
        let result = stacker::maybe_grow(EVAL_RED_ZONE, EVAL_STACK_SEGMENT, || f(self));

        self.recursion_depth -= 1;
        result
    }

    /// Internal evaluation implementation (separated to allow depth tracking)
    fn evaluate_internal_impl(
        &mut self,
        node: &AstNode,
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        match node {
            AstNode::String(s) => Ok(JValue::string(s.clone())),

            // Bare Name outside a Path: field access on the current data.
            // `evaluate_leaf` already intercepts the Object/LazyPyDict cases,
            // so only array and scalar data reach here — and no expression in
            // the reference suite, the differential corpus, or the Python
            // suite does even that (verified by instrumentation). Delegate to
            // `compiled_field_step`, the semantics owner for a single field
            // step, instead of keeping a private copy that had drifted (it
            // kept nulls, never flattened nested arrays, and knew nothing of
            // tuples or lazy dicts).
            AstNode::Name(field_name) => compiled_field_step(field_name, data, &self.options),

            AstNode::Number(n) => {
                // Preserve integer-ness: if the number is a whole number, create an integer JValue
                if n.fract() == 0.0 && n.is_finite() && n.abs() < (1i64 << 53) as f64 {
                    // It's a whole number that can be represented as i64
                    Ok(JValue::from(*n as i64))
                } else {
                    Ok(JValue::Number(*n))
                }
            }
            AstNode::Boolean(b) => Ok(JValue::Bool(*b)),
            AstNode::Null => Ok(JValue::Null),
            AstNode::Undefined => Ok(JValue::Undefined),
            AstNode::Placeholder => {
                // Placeholders should only appear as function arguments
                // If we reach here, it's an error
                Err(EvaluatorError::EvaluationError(
                    "Placeholder '?' can only be used as a function argument".to_string(),
                ))
            }
            AstNode::Regex { pattern, flags } => {
                // Return a regex object as a special JSON value
                // This will be recognized by functions like $split, $match, $replace
                Ok(JValue::regex(pattern.as_str(), flags.as_str()))
            }

            AstNode::Variable(name) => {
                // Special case: $ alone (empty name) refers to current context
                // First check if $ is bound in the context (for closures that captured $)
                // Otherwise, use the data parameter
                if name.is_empty() {
                    if let Some(value) = self.context.lookup("$") {
                        return Ok(value.bare());
                    }
                    // If data is a tuple, return the @ value
                    if let JValue::Object(obj) = data {
                        if is_tuple_object(obj) {
                            if let Some(inner) = obj.get("@") {
                                return Ok(inner.clone());
                            }
                        }
                    }
                    return Ok(data.bare());
                }

                // Check variable bindings FIRST
                // This allows function parameters to shadow outer lambdas with the same name
                // Critical for Y-combinator pattern: function($g){$g($g)} where $g shadows outer $g
                if let Some(value) = self.context.lookup(name) {
                    return Ok(value.clone());
                }

                // Check tuple bindings in data (for index binding operator #$var)
                // When iterating over a tuple stream, $var can reference the bound index
                if let JValue::Object(obj) = data {
                    if is_tuple_object(obj) {
                        // Check for the variable in tuple bindings (stored as "$name")
                        let binding_key = format!("${}", name);
                        if let Some(binding_value) = obj.get(&binding_key) {
                            return Ok(binding_value.clone());
                        }
                    }
                }

                // A builtin or host function named by reference (only if not
                // shadowed) is a function value. Calling it dispatches host
                // functions first, as a call by name does.
                if self.is_function_name(name) {
                    // Return a marker for built-in functions
                    // This allows built-in functions to be passed to higher-order functions
                    let builtin_repr = JValue::builtin(name.as_str());
                    return Ok(builtin_repr);
                }

                // An unbound variable is undefined. This is what makes
                // `$not($x)` undefined, `{"a": $x}` drop the key, and `3 > $x`
                // undefined rather than a T2010 on an uncomparable null.
                // An unbound variable is undefined, not null: `3 > $x` is undefined
                // and `{"a": $x}` drops the key. (#98)
                Ok(JValue::Undefined)
            }

            AstNode::ParentVariable(name) => {
                // Special case: $$ alone (empty name) refers to parent/root context
                if name.is_empty() {
                    return self.context.get_parent().cloned().ok_or_else(|| {
                        EvaluatorError::ReferenceError("Parent context not available".to_string())
                    });
                }

                // `$$name` is the variable named `$name` in jsonata-js; the lexer
                // emits it as `Variable("$name")`, so only bare `$$` reaches here.
                self.evaluate_internal(&AstNode::Variable(format!("${}", name)), data)
            }

            AstNode::Path { steps } => self.evaluate_path(steps, data),

            AstNode::Binary { op, lhs, rhs } => self.evaluate_binary_op(*op, lhs, rhs, data),

            AstNode::Unary { op, operand } => self.evaluate_unary_op(*op, operand, data),

            // Array constructor - JSONata semantics:
            AstNode::Array(elements) => Ok(JValue::Array(self.construct_array(elements, data)?)),

            AstNode::Object(pairs) => self.construct_object(pairs, data),

            // Object transform: group items by key, then evaluate value once per group
            AstNode::ObjectTransform { input, pattern } => {
                // Evaluate the input expression. Keep tuple wrappers alive so the
                // group-by key/value expressions can read the carried `$focus`
                // bindings off each wrapper (e.g. `...@$e...{ $e.FirstName: ... }`).
                let saved_keep = self.keep_tuple_stream;
                self.keep_tuple_stream =
                    Self::is_tuple_path_node(input) || Self::sorts_tuples(input);
                // jsonata-js groups the input's result before the generic
                // unwrap, so a one-item sequence holding an array is one item.
                let input_value = self.evaluate_unwrapped_later(input, data);
                self.keep_tuple_stream = saved_keep;
                let input_value = input_value?;

                // Handle array input - process each item.
                //
                // jsonata-js #817 ("correctly handle empty joins"): an object
                // constructor (group-by) applied to an empty or undefined
                // sequence must yield an empty object `{}`, not undefined.
                // jsonata-js wraps the input via `createSequence`, so an
                // undefined input becomes an *empty* sequence that flows through
                // grouping and produces `{}`. We mirror that by mapping undefined
                // to an empty item list here (rather than short-circuiting), and
                // let the empty-input handling below generate the `{}`.
                //
                // An explicit null is a value and groups like any other scalar:
                // `nul{"a": $}` is `{"a": null}`. The early return that used to
                // sit here predates the Null/Undefined split, and made the
                // undotted form disagree with the dotted `nul.{"a": $}`, which
                // takes a different route and was already right.
                let items: Vec<JValue> = match input_value {
                    JValue::Array(ref arr) if arr.sorted_tuples => sorted_tuple_items(arr),
                    JValue::Array(ref arr) => arr.to_vec(),
                    JValue::Undefined => Vec::new(),
                    other => vec![other],
                };

                // If array is empty, add undefined to enable literal JSON object
                // generation. An empty tuple stream gets no such item: jsonata-js
                // reduces it tuple by tuple, so it makes no group and gives `{}`
                // (#215).
                if items.is_empty() && Self::groups_tuple_stream(input) {
                    return Ok(JValue::object(IndexMap::new()));
                }
                let items = if items.is_empty() {
                    vec![JValue::Undefined]
                } else {
                    items
                };

                // Grouping over a tuple stream ("reduce" mode, mirroring
                // jsonata-js evaluateGroupExpression): each item is a
                // `{@, $var, !label, __tuple__}` wrapper. The key/value
                // expressions are evaluated against the tuple's `@` value with the
                // carried focus/index/ancestor keys bound into scope (so
                // `...@$e...{ $e.FirstName: Phone[type='mobile'].number }` reads
                // `$e` AND resolves the relative `Phone` against the Contact `@`),
                // and grouped tuples are reduced (per-key values appended) before
                // the value expression sees them.
                let reduce = starts_with_tuple(&items);

                // Bind a tuple wrapper's carried `$var`/`!label` keys into scope;
                // returns the saved prior values so they can be restored.
                let bind_tuple = |ev: &mut Self,
                                  tuple: &IndexMap<String, JValue>|
                 -> Vec<(String, Option<JValue>)> {
                    let mut saved = Vec::new();
                    for (k, v) in tuple.iter() {
                        let name = if let Some(n) = k.strip_prefix('$') {
                            if n.is_empty() {
                                continue;
                            } else {
                                n.to_string()
                            }
                        } else if k.starts_with('!') {
                            k.clone()
                        } else {
                            continue;
                        };
                        saved.push((name.clone(), ev.context.lookup(&name).cloned()));
                        ev.context.bind(name, v.clone());
                    }
                    saved
                };
                let restore = |ev: &mut Self, saved: Vec<(String, Option<JValue>)>| {
                    for (name, old) in saved.into_iter().rev() {
                        match old {
                            Some(v) => ev.context.bind(name, v),
                            None => ev.context.unbind(&name),
                        }
                    }
                };

                // Save the current $ binding to restore later
                let saved_dollar = self.context.lookup("$").cloned();

                // The grouping, run so that `$` is restored on an error too.
                let grouped = (|ev: &mut Self| -> Result<JValue, EvaluatorError> {
                    // Phase 1: Group items by key expression
                    // groups maps key -> (grouped_data, expr_index), in first-seen
                    // order. When multiple items have same key, their data is
                    // appended together
                    let mut groups: IndexMap<String, (Vec<JValue>, usize)> = IndexMap::new();

                    for item in &items {
                        // In reduce mode evaluate the key against `@` with tuple keys
                        // bound; otherwise against the item itself.
                        let (key_data, tuple_saved) = match (reduce, item) {
                            (true, JValue::Object(o)) => {
                                let saved = bind_tuple(ev, o);
                                (
                                    o.get("@").cloned().unwrap_or(JValue::Undefined),
                                    Some(saved),
                                )
                            }
                            _ => (item.clone(), None),
                        };
                        ev.context.bind("$".to_string(), key_data.clone());

                        for (pair_index, (key_node, _value_node)) in pattern.iter().enumerate() {
                            // Evaluate key with current item as context
                            let key = match ev.evaluate_internal(key_node, &key_data)? {
                                JValue::String(s) => s,
                                other => {
                                    // Skip undefined keys
                                    if other.is_undefined() {
                                        continue;
                                    }
                                    if let Some(saved) = tuple_saved {
                                        restore(ev, saved);
                                    }
                                    return Err(key_type_error(&other));
                                }
                            };

                            // Group items by key
                            if let Some((existing_data, existing_idx)) = groups.get_mut(&*key) {
                                // Key already exists - check if from same expression index
                                if *existing_idx != pair_index {
                                    if let Some(saved) = tuple_saved {
                                        restore(ev, saved);
                                    }
                                    return Err(duplicate_key_error(&key));
                                }
                                // Append item to the group
                                existing_data.push(item.clone());
                            } else {
                                // New key - create new group
                                groups.insert(key.to_string(), (vec![item.clone()], pair_index));
                            }
                        }

                        if let Some(saved) = tuple_saved {
                            restore(ev, saved);
                        }
                    }

                    // Phase 2: Evaluate value expression for each group
                    let mut result = IndexMap::new();

                    for (key, (grouped_data, expr_index)) in groups {
                        // Get the value expression for this group
                        let (_key_node, value_node) = &pattern[expr_index];

                        if reduce {
                            // Reduce the grouped tuples into one (per-key values
                            // appended), mirroring jsonata-js reduceTupleStream, then
                            // evaluate the value against the merged `@` with the merged
                            // focus/index/ancestor keys bound.
                            let merged = reduce_tuple_stream(&grouped_data);
                            let context = merged.get("@").cloned().unwrap_or(JValue::Undefined);
                            let mut tuple_no_at = merged.clone();
                            tuple_no_at.shift_remove("@");
                            let saved = bind_tuple(ev, &tuple_no_at);
                            ev.context.bind("$".to_string(), context.clone());
                            let value = ev.evaluate_internal(value_node, &context);
                            restore(ev, saved);
                            let value = value?;
                            if !value.is_undefined() {
                                result.insert(key, value);
                            }
                            continue;
                        }

                        let context = group_context(&grouped_data);

                        // Bind $ to the context for value evaluation
                        ev.context.bind("$".to_string(), context.clone());

                        // Evaluate value expression with grouped context
                        let value = ev.evaluate_internal(value_node, &context)?;

                        // Skip undefined values
                        if !value.is_undefined() {
                            result.insert(key, value);
                        }
                    }
                    Ok(JValue::object(result))
                })(self);

                // Restore the previous $ binding
                if let Some(saved) = saved_dollar {
                    self.context.bind("$".to_string(), saved);
                } else {
                    self.context.unbind("$");
                }
                grouped
            }

            AstNode::Function {
                name,
                args,
                is_builtin,
            } => self.evaluate_function_call(name, args, *is_builtin, data),

            // Call: invoke an arbitrary expression as a function
            // Used for IIFE patterns like (function($x){...})(5) or chained calls
            AstNode::Call { procedure, args } => {
                // As jsonata-js `evaluateFunction`: evaluate the procedure, then
                // apply whatever function value it gives -- a lambda, a builtin,
                // a partial application or a composition. A `?` argument makes a
                // partial application; anything that is not a function is T1006.
                let callable = self.evaluate_internal(procedure, data)?;
                self.apply_call_value(callable, procedure, args, data)
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
                    Ok(JValue::Undefined)
                }
            }

            AstNode::Block(expressions) => self.evaluate_block(expressions, data),

            // Lambda: capture current environment for closure support
            AstNode::Lambda {
                params,
                body,
                signature,
                thunk,
            } => {
                let stored_lambda = StoredLambda {
                    params: params.clone(),
                    body: (**body).clone(),
                    compiled_body: CompiledBody::compile(body, params, signature, *thunk),
                    signature: signature.clone(),
                    captured_env: self.capture_environment_for(body, params),
                    captured_data: Some(data.clone()),
                    thunk: *thunk,
                    self_name: None,
                    partial: None,
                    live_token: LiveLambdaToken::new(),
                    def_scope: self.context.def_scope(),
                };
                Ok(JValue::Lambda(Rc::new(stored_lambda)))
            }

            // Wildcard: collect all values from current object
            AstNode::Wildcard => {
                let normalized = normalize_lazy(data)?;
                let result = match &normalized {
                    JValue::Object(obj) => wildcard_values(obj.values()),
                    // jsonata-js reads an array's items as its field values.
                    JValue::Array(arr) => wildcard_values(arr.iter()),
                    // Anything else has no children to collect. jsonata-js
                    // guards the wildcard with `typeof input === 'object' &&
                    // input !== null`, so every scalar -- and null, which only
                    // reaches that guard because of JS's `typeof null` quirk --
                    // maps over nothing and yields undefined, not null.
                    _ => return Ok(JValue::Undefined),
                };
                if let JValue::Array(arr) = &result {
                    check_sequence_length(arr.len(), &self.options)?;
                }
                Ok(result)
            }

            // `**`: every value at or below the input, following jsonata-js
            // `evaluateDescendants`, which returns a single descendant itself
            // rather than as a one-item sequence (so `**[]` does not keep it).
            AstNode::Descendant => {
                let mut descendants = self.collect_descendants(data)?;
                check_sequence_length(descendants.len(), &self.options)?;
                Ok(if descendants.len() == 1 {
                    descendants.pop().expect("length checked")
                } else {
                    JValue::sequence(descendants)
                })
            }

            // Array grouping: same as Array but prevents flattening in path contexts
            AstNode::ArrayGroup(elements) => {
                // Undefined elements are dropped like in every array constructor;
                // explicit null is a value and stays.
                let mut result = Vec::new();
                for element in elements {
                    let value = self.evaluate_internal(element, data)?;
                    if !value.is_undefined() {
                        result.push(value);
                    }
                }
                Ok(JValue::cons(result))
            }

            AstNode::FunctionApplication(_) => Err(EvaluatorError::EvaluationError(
                "Function application can only be used in path expressions".to_string(),
            )),

            AstNode::Sort { input, terms } => self
                .evaluate_sort_node(input, terms, data, false)
                .map(|(sorted, _)| sorted),

            // Transform: |location|update[,delete]|
            AstNode::Transform {
                location,
                update,
                delete,
            } => {
                // A function value, as in jsonata-js: the transform runs when
                // the function is invoked (`invoke_transform`). Being an
                // ordinary value, it can be bound to a variable, passed
                // around, and applied via `~>`.
                // Like a lambda, it closes over the variables it uses.
                let body = AstNode::Transform {
                    location: location.clone(),
                    update: update.clone(),
                    delete: delete.clone(),
                };
                let captured_env = self.capture_environment_for(&body, &[]);
                let transform_lambda = StoredLambda {
                    params: vec![TRANSFORM_INPUT.to_string()],
                    body,
                    compiled_body: None, // Transform is not a pure compilable expr
                    signature: None,
                    captured_env,
                    captured_data: None, // Transform takes $ as parameter
                    thunk: false,
                    self_name: None,
                    partial: None,
                    live_token: LiveLambdaToken::new(),
                    def_scope: self.context.def_scope(),
                };
                Ok(JValue::Lambda(Rc::new(transform_lambda)))
            }

            // Parent-reference operator (%): ast_transform has already resolved
            // this to a synthetic ancestor label ("!0", "!1", ...). The enclosing
            // tuple step binds that label into scope (create_tuple_stream +
            // bind_tuple_keys), so resolving it is an ordinary scope
            // lookup, mirroring jsonata-js's
            // `case 'parent': result = environment.lookup(expr.slot.label);`.
            AstNode::Parent(label) => {
                if let Some(v) = self.context.lookup(label) {
                    return Ok(v.clone());
                }
                // Fall back to the tuple wrapper carried as `data`: a `%` used
                // inside a predicate/stage over a tuple stream -- e.g.
                // `(Account.Order.Product)[%.OrderID='order104'].SKU`, where the
                // predicate is evaluated per tuple with the wrapper as data --
                // reads its ancestor from the tuple's `!label` key, which isn't
                // separately bound into scope here (mirrors AstNode::Variable's
                // tuple-binding fallback below).
                if let JValue::Object(obj) = data {
                    if is_tuple_object(obj) {
                        if let Some(v) = obj.get(label) {
                            return Ok(v.clone());
                        }
                    }
                }
                Ok(JValue::Undefined)
            }
        }
    }

    /// The items of an array constructor `[...]`, following jsonata-js: an
    /// element that is itself an array constructor stays nested, any other
    /// array value is spliced in one level (`fn.append`), and undefined
    /// elements are dropped. Explicit nulls are values and stay.
    ///
    /// The array is marked flat (#196) when that is known at no cost: every
    /// spliced array was flat and no pushed element is an array a step
    /// flattens (one O(1) check per element, not per spliced item).
    fn construct_array(
        &mut self,
        elements: &[AstNode],
        data: &JValue,
    ) -> Result<JArray, EvaluatorError> {
        let mut items = Vec::with_capacity(elements.len());
        let mut flat = true;
        for element in elements {
            let value = self.evaluate_internal(element, data)?;
            match value {
                JValue::Undefined => {}
                JValue::Array(arr) if !is_array_constructor(element) => {
                    flat &= arr.is_flat();
                    items.extend(arr.iter().cloned())
                }
                other => {
                    flat &= !flattens_in_step(&other);
                    items.push(other)
                }
            }
        }
        Ok(JArray::new_flat(items, flat))
    }

    /// `lhs ~> rhs`, following jsonata-js `evaluateApplyExpression`.
    ///
    /// A function call on the right is invoked with the left side as its
    /// first argument. Anything else must evaluate to a function: it is
    /// composed with the left side when that is a function too, and applied
    /// to the left side otherwise. An undefined left side is passed on like
    /// any other value (`nope ~> $count()` is 0).
    fn evaluate_apply_expression(
        &mut self,
        lhs: &AstNode,
        rhs: &AstNode,
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        // The call on the right, and the one-step path it heads when it has
        // a filter or `[]` (`x ~> $map($f)[0]`). jsonata-js parses a call
        // with a `?` argument as a partial application, not a call, so that
        // is applied like any function value below.
        let (call, path_steps) = match rhs {
            AstNode::Path { steps } if steps.len() == 1 => (&steps[0].node, Some(steps)),
            _ => (rhs, None),
        };
        if let AstNode::Function {
            name,
            args,
            is_builtin,
        } = call
        {
            if !args.iter().any(|a| matches!(a, AstNode::Placeholder)) {
                let mut all_args = Vec::with_capacity(args.len() + 1);
                all_args.push(lhs.clone());
                all_args.extend_from_slice(args);
                let result = self.evaluate_function_call(name, &all_args, *is_builtin, data)?;
                return match path_steps {
                    Some(steps) => self.finish_chained_path(steps, result),
                    None => Ok(result.unwrap_sequence()),
                };
            }
        }
        // A call of an expression's value, such as `$uppercase[0]()` or
        // `$o[1]()`: as jsonata-js `evaluateFunction` with the left side as
        // its context, which goes first among the arguments.
        if let AstNode::Call { procedure, args } = call {
            if !args.iter().any(|a| matches!(a, AstNode::Placeholder)) {
                let lhs_value = self.evaluate_internal(lhs, data)?;
                let callable = self.evaluate_internal(procedure, data)?;
                self.check_callable(&callable, procedure)?;
                let mut values = Vec::with_capacity(args.len() + 1);
                values.push(lhs_value);
                for arg in args {
                    values.push(self.evaluate_internal(arg, data)?);
                }
                let result = self.call_function_value(&callable, &values, data)?;
                return match path_steps {
                    Some(steps) => self.finish_chained_path(steps, result),
                    None => Ok(result.unwrap_sequence()),
                };
            }
        }

        let lhs_value = self.evaluate_internal(lhs, data)?;
        // An inline lambda applied to a value is invoked directly, without
        // building its closure first.
        if let AstNode::Lambda {
            params,
            body,
            signature,
            thunk,
        } = rhs
        {
            if !lhs_value.is_function() {
                return self
                    .as_callback(|ev| {
                        ev.invoke_lambda(
                            params,
                            body,
                            signature.as_ref(),
                            &[lhs_value],
                            data,
                            *thunk,
                        )
                    })
                    .map(JValue::unwrap_sequence);
            }
        }
        let func = self.evaluate_internal(rhs, data)?;
        if !func.is_function() {
            return Err(EvaluatorError::TypeError(
                "T2006: The right side of the function application operator ~> must be a function"
                    .to_string(),
            ));
        }
        if lhs_value.is_function() {
            let def = self.context.def_scope();
            return Ok(Self::compose_functions(lhs_value, func, def));
        }
        self.as_callback(|ev| ev.call_function_value(&func, &[lhs_value], data))
            .map(JValue::unwrap_sequence)
    }

    /// Whether an argument is a function with tail calls (written inline,
    /// or a variable holding one), whose chain of tail calls would begin
    /// where it is applied.
    fn has_thunk_function_arg(&self, args: &[AstNode]) -> bool {
        args.iter().any(|arg| match arg {
            AstNode::Lambda { thunk, .. } => *thunk,
            AstNode::Variable(name) => {
                matches!(self.context.lookup(name), Some(JValue::Lambda(rc)) if rc.thunk)
            }
            _ => false,
        })
    }

    /// The `(f, g)` a composition calls: the function that runs f (in its
    /// own frame) and g. `None` for any other function.
    fn composition_parts(lambda: &StoredLambda) -> Option<(&JValue, &JValue)> {
        if !lambda.def_scope.at_caller {
            return None;
        }
        let f = lambda.captured_env.get(composition_names::F)?;
        let g = lambda.captured_env.get(composition_names::G)?;
        Some((f, g))
    }

    /// A tail call of a composition: f's call runs here, as jsonata-js runs
    /// it in the composition's frame before the tail call, and g's is the
    /// tail call itself. So the composition never becomes the trampoline's
    /// function, which would keep the frame it was made in.
    fn tail_call_composition(
        &mut self,
        mut composition: Rc<StoredLambda>,
        args: Vec<JValue>,
        data: &JValue,
    ) -> Result<LambdaResult, EvaluatorError> {
        let mut arg = args.into_iter().next().unwrap_or(JValue::Undefined);
        loop {
            let (f, g) = match Self::composition_parts(&composition) {
                Some((f, g)) => (f.clone(), g.clone()),
                None => unreachable!("tail_call_composition of a non-composition"),
            };
            // jsonata-js's chain function has a null input, so `$f($x)` in
            // its body applies f with a null input.
            let value = self.as_callback(|ev| ev.call_function_value(&f, &[arg], data))?;
            match g {
                JValue::Lambda(next) if Self::composition_parts(&next).is_some() => {
                    composition = next;
                    arg = value;
                }
                JValue::Lambda(lambda) if lambda.thunk => {
                    return Ok(LambdaResult::TailCall {
                        lambda,
                        args: vec![value],
                        data: data.clone(),
                    });
                }
                JValue::Builtin { name }
                    if &*name == "eval" && self.builtin_unshadowed("eval", None) =>
                {
                    let at = self.context.tail_base;
                    let input = self.tail_input(data);
                    let result = self.eval_from_values_at(&[value], &input, Some(at))?;
                    return Ok(LambdaResult::JValue(result));
                }
                g => {
                    // g is the chain's tail call: a builtin runs with the
                    // trampoline's input.
                    let input = match g {
                        JValue::Builtin { .. } => self.tail_input(data),
                        _ => data.clone(),
                    };
                    let result = self.call_function_value(&g, &[value], &input)?;
                    return Ok(LambdaResult::JValue(result));
                }
            }
        }
    }

    /// `f ~> g` for two functions, made where `def` says: the function
    /// `λ($x) { g(f($x)) }`, as jsonata-js builds from its `chainAST` in the
    /// environment of the `~>`. There, `f($x)` runs in the composition's own
    /// call frame, below the defining environment, and `g(...)` is a tail
    /// call, run in the caller's. So f is called through a function defined
    /// at `def` with an opaque frame, and the composition itself continues
    /// at its caller's frame (`DefScope::AT_CALLER`); this matters only to
    /// `$eval`, as every other function resolves names where it was made.
    fn compose_functions(f: JValue, g: JValue, def: DefScope) -> JValue {
        use composition_names::{F, G, X};
        let call = |name: &str, arg: AstNode| AstNode::Function {
            name: name.to_string(),
            args: vec![arg],
            is_builtin: false,
        };
        let lambda = |body: AstNode, captured_env: HashMap<String, JValue>, def_scope, thunk| {
            JValue::Lambda(Rc::new(StoredLambda {
                params: vec![X.to_string()],
                body,
                compiled_body: None,
                signature: None,
                captured_env,
                captured_data: None,
                thunk,
                self_name: None,
                partial: None,
                live_token: LiveLambdaToken::new(),
                def_scope,
            }))
        };
        let first = lambda(
            call(F, AstNode::Variable(X.to_string())),
            HashMap::from([(F.to_string(), f)]),
            DefScope {
                internal: true,
                opaque: true,
                ..def
            },
            false,
        );
        // Its body names only what it captured.
        lambda(
            call(G, call(F, AstNode::Variable(X.to_string()))),
            HashMap::from([(F.to_string(), first), (G.to_string(), g)]),
            DefScope::AT_CALLER,
            // g's call is a tail call, as in jsonata-js.
            true,
        )
    }

    /// Finish a `~>` whose right side is a call with a filter or `[]`
    /// (`x ~> $f()[0]`), given that call's result: the call step's filter
    /// stages, then `[]`, as for any path. jsonata-js does not filter the
    /// call's result at all, so an undefined one is not filtered either.
    fn finish_chained_path(
        &mut self,
        steps: &[PathStep],
        call_result: JValue,
    ) -> Result<JValue, EvaluatorError> {
        let mut result = self.apply_filter_stages(call_result, &steps[0].stages)?;
        if Self::path_keeps_singleton_array(steps) {
            keep_if_sequence(&mut result);
        }
        Ok(result)
    }

    /// Evaluate a path expression, following jsonata-js `evaluatePath`.
    ///
    /// Each step maps over the previous step's result sequence
    /// (`evaluate_step`). From the first step that binds a focus, index or
    /// ancestor variable on, the path is a tuple stream instead, and every
    /// later step goes through `create_tuple_stream` (jsonata-js
    /// `evaluateTupleStep`). The result is a sequence; `evaluate_internal`
    /// applies the generic unwrap to it.
    fn evaluate_path(
        &mut self,
        steps: &[PathStep],
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        // A request to keep tuple streams is for this path only, not for the
        // expressions its steps evaluate.
        let keep_tuple_stream = std::mem::replace(&mut self.keep_tuple_stream, false);
        let result = self.evaluate_path_steps(steps, data, keep_tuple_stream, false);
        self.keep_tuple_stream = keep_tuple_stream;
        result.map(|(value, _)| value)
    }

    /// `evaluate_path`, with `keep_tuple_stream` saying whether a tuple
    /// stream result is returned as its tuples rather than their values.
    /// `dotted` is set for a one-step sort path that is itself the input of
    /// a sort starting a longer path: its first step then starts that path.
    /// The flag returned is set when a first-step sort of a consarray step
    /// gave a single value, for an enclosing sort to pass on.
    fn evaluate_path_steps(
        &mut self,
        steps: &[PathStep],
        data: &JValue,
        keep_tuple_stream: bool,
        dotted: bool,
    ) -> Result<(JValue, bool), EvaluatorError> {
        let Some(first) = steps.first() else {
            return Ok((data.clone(), false));
        };
        let mut cons_single_out = false;

        // A lone field step on an object is that field's value, whatever it
        // holds: the general loop would reach the same answer through the
        // last-step rule.
        if steps.len() == 1 && first.stages.is_empty() && !Self::step_creates_tuple(first) {
            if let AstNode::Name(field) = &first.node {
                match data {
                    JValue::Object(obj) if !is_tuple_object(obj) => {
                        return Ok((obj.get(field).cloned().unwrap_or(JValue::Undefined), false));
                    }
                    #[cfg(feature = "python")]
                    JValue::LazyPyDict(lazy) => return Ok((lazy.get_field(field)?, false)),
                    _ => {}
                }
            }
        }

        // `$var.field` over an object variable, common in lambda bodies: the
        // general loop's answer without its machinery.
        if let [var_step, field_step] = steps {
            if let (AstNode::Variable(name), AstNode::Name(field)) =
                (&var_step.node, &field_step.node)
            {
                let plain =
                    |step: &PathStep| step.stages.is_empty() && !Self::step_creates_tuple(step);
                if !name.is_empty()
                    && plain(var_step)
                    && plain(field_step)
                    && !matches!(data, JValue::Object(obj) if is_tuple_object(obj))
                {
                    if let Some(JValue::Object(obj)) = self.context.lookup(name) {
                        if !is_tuple_object(obj) {
                            let value = obj.get(field).cloned().unwrap_or(JValue::Undefined);
                            return Ok((collect_single_step_result(value, true), false));
                        }
                    }
                }
            }
        }

        // An expression jsonata-js does not treat as a path (`(expr)[0]`,
        // `$x[]`, `*[0]`) is evaluated once against the whole input, its
        // predicates filter that value, and `[]` keeps it only if it is a
        // sequence.
        if Self::is_non_path(steps) && !Self::step_creates_tuple(first) && !first.is_tuple {
            let value = self.evaluate_unwrapped_later(&first.node, data)?;
            // jsonata-js filters an undefined value too (`$x[$f()]`).
            let mut value = if value.is_undefined() && Self::filters_run_on_nothing(&first.stages) {
                self.filter_undefined(&first.stages)?
            } else {
                self.apply_filter_stages(value, &first.stages)?
            };
            if Self::path_keeps_singleton_array(steps) {
                keep_if_sequence(&mut value);
            }
            return Ok((value, false));
        }

        // jsonata-js maps the first step over an array input item by item,
        // except when the path starts with a variable or a sort, or the input
        // is the top-level array, which it wraps whole (`outerWrapper`).
        let takes_whole_input = matches!(
            first.node,
            AstNode::Variable(_) | AstNode::ParentVariable(_) | AstNode::Sort { .. }
        );
        // The wrapper's one item is the bare array; a sort evaluates its own
        // input path, so it keeps the wrapper.
        let bare;
        let first_items: &[JValue] = match data {
            JValue::Array(arr)
                if arr.outer_wrapper && !matches!(first.node, AstNode::Sort { .. }) =>
            {
                bare = data.bare();
                std::slice::from_ref(&bare)
            }
            JValue::Array(arr) if !takes_whole_input && !arr.outer_wrapper => arr,
            _ => std::slice::from_ref(data),
        };

        let mut tuples: Option<Vec<JValue>> = None;
        // Whether `tuples` came straight from a sort of tuples (#214).
        let mut sorted_tuples = false;
        let mut result = JValue::Undefined;
        for (index, step) in steps.iter().enumerate() {
            if tuples.is_some() || step.is_tuple || Self::step_creates_tuple(step) {
                // A final plain field step over a stream that is about to be
                // projected can't use the tuples' bindings: its projected
                // values are the field of each tuple's `@`, an array value
                // giving one tuple per item (jsonata-js `evaluateTupleStep`).
                if let (Some(stream), AstNode::Name(field)) = (&tuples, &step.node) {
                    if index + 1 == steps.len()
                        && !keep_tuple_stream
                        && step.stages.is_empty()
                        && !Self::step_creates_tuple(step)
                    {
                        let focus: Vec<JValue> = stream.iter().map(tuple_focus_value).collect();
                        let mut projected = Vec::with_capacity(focus.len());
                        for value in field_values(field, &focus, &self.options)? {
                            match value {
                                JValue::Array(items) => projected.extend(items.iter().cloned()),
                                other => projected.push(other),
                            }
                        }
                        check_sequence_length(projected.len(), &self.options)?;
                        result = JValue::sequence(projected);
                        tuples = None;
                        break;
                    }
                }
                // A leading array constructor is evaluated once against the
                // whole input, its focus or index binding ignored (jsonata-js
                // evaluatePath's `ii === 0 && step.consarray`). The stream
                // starts from its items, or with a focus, which keeps the
                // step's input as the next one's, from the path's input.
                // The parser flags it only on a dotted path: alone
                // (`[3,1]#$k`), it maps over the input like any first step.
                if index == 0
                    && (dotted || steps.len() > 1)
                    && matches!(step.node, AstNode::Array(_))
                {
                    let stream = self.consarray_tuple_start(step, data)?;
                    tuples = Some(stream);
                    sorted_tuples = false;
                    continue;
                }
                let source = match tuples.take() {
                    Some(stream) => JValue::array(stream),
                    None if index == 0 => data.clone(),
                    None => result.clone(),
                };
                let starts_dotted = index == 0 && (dotted || steps.len() > 1);
                let (stream, sorted) =
                    self.create_tuple_stream(step, &source, index == 0, starts_dotted)?;
                tuples = Some(stream);
                sorted_tuples = sorted;
                continue;
            }

            // The previous step's result, or the path's input for the first
            // step. A result that is not an array is a single item, standing
            // for the one-item sequence jsonata-js would build.
            let single = if index > 0 && !matches!(result, JValue::Array(_)) {
                std::mem::replace(&mut result, JValue::Undefined)
            } else {
                JValue::Undefined
            };
            let previous = match &result {
                JValue::Array(arr) if index > 0 => Some(arr.clone()),
                _ => None,
            };
            let items: &[JValue] = match &previous {
                Some(arr) => arr,
                None if index == 0 => first_items,
                None => std::slice::from_ref(&single),
            };

            result = if index == 0
                && (matches!(step.node, AstNode::Array(_))
                    || Self::is_cons_block(&step.node)
                    || Self::is_cons_group(&step.node))
            {
                self.evaluate_cons_first_step(step, steps, data)?
            } else if let (0, AstNode::Sort { input, terms }) = (index, &step.node) {
                let (value, cons_single) =
                    self.evaluate_first_step_sort(step, steps, input, terms, data, dotted)?;
                cons_single_out = cons_single;
                value
            } else {
                // A sort step gives its tuples, which the rest of the path reads.
                let saved = self.keep_tuple_stream;
                if matches!(step.node, AstNode::Sort { .. }) {
                    self.keep_tuple_stream = true;
                }
                let value =
                    self.evaluate_step(step, items, previous.as_ref(), index + 1 == steps.len());
                self.keep_tuple_stream = saved;
                value?
            };

            if result.is_undefined() || matches!(&result, JValue::Array(arr) if arr.is_empty()) {
                break;
            }
            // A step that returns a tuple stream (a sort lifted out of a tuple
            // path, or any step while an enclosing sort or group-by keeps
            // tuple streams) puts the rest of the path in tuple mode.
            let may_return_tuples = keep_tuple_stream || matches!(step.node, AstNode::Sort { .. });
            if let (JValue::Array(arr), true) = (&result, may_return_tuples) {
                if starts_with_tuple(arr) {
                    tuples = Some(arr.to_vec());
                    sorted_tuples = arr.sorted_tuples;
                    continue;
                }
            }
        }

        if let Some(stream) = tuples {
            result = if keep_tuple_stream {
                // An enclosing sort or group-by reads the carried bindings.
                let mut arr = JArray::new(stream);
                arr.sorted_tuples = sorted_tuples;
                JValue::Array(arr)
            } else {
                JValue::sequence(stream.iter().map(tuple_focus_value).collect())
            };
        }

        // A `[]` on a step marks the path in jsonata-js. One on a sort marks
        // the sort's path node instead, which keeps a sequence from being
        // unwrapped but leaves a constructed array as it is: `nn^($)[].[$]`
        // is the array `[$]` builds.
        if Self::path_keeps_singleton_array(steps) {
            let node_only = steps.len() > 1 && !Self::steps_keep_singleton(steps);
            if !(node_only && matches!(&result, JValue::Array(arr) if arr.cons && !arr.sequence)) {
                result = keep_singleton(result);
            }
        }

        if let JValue::Array(arr) = &result {
            check_sequence_length(arr.len(), &self.options)?;
        }
        Ok((result, cons_single_out))
    }

    /// The tuples a leading `consarray` step starts a tuple stream with: its
    /// items, or with a focus, the path's input (see `evaluate_path_steps`).
    /// Kept out of line, off the hot path.
    #[inline(never)]
    fn consarray_tuple_start(
        &mut self,
        step: &PathStep,
        data: &JValue,
    ) -> Result<Vec<JValue>, EvaluatorError> {
        self.tuple_stream_created = true;
        let value = self.evaluate_internal(&step.node, data)?;
        let value = self.apply_filter_stages(value, &step.stages)?;
        let items = match (step.focus.is_some(), value) {
            // The top-level array's wrapper holds the bare array.
            (true, _) => match data {
                JValue::Array(arr) if arr.outer_wrapper => vec![data.bare()],
                JValue::Array(arr) => arr.iter().cloned().collect(),
                JValue::Undefined => Vec::new(),
                other => vec![other.clone()],
            },
            (false, JValue::Undefined) => Vec::new(),
            (false, JValue::Array(arr)) => arr.iter().cloned().collect(),
            (false, other) => vec![other],
        };
        Ok(items
            .into_iter()
            .map(|item| {
                let mut wrapper = IndexMap::new();
                wrapper.insert("@".to_string(), item);
                wrapper.insert("__tuple__".to_string(), JValue::Bool(true));
                JValue::object(wrapper)
            })
            .collect())
    }

    /// A path's leading array constructor or consarray block, evaluated once
    /// against the whole input. Kept out of line, off the hot path.
    #[inline(never)]
    fn evaluate_cons_first_step(
        &mut self,
        step: &PathStep,
        steps: &[PathStep],
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        // A leading array constructor builds one array from the whole
        // input rather than one per item (jsonata-js `consarray`), as
        // does a block holding a path that starts with one.
        let mut value = self.evaluate_internal(&step.node, data)?;
        if let JValue::Array(arr) = &mut value {
            arr.cons = true;
        }
        let mut value = self.apply_filter_stages(value, &step.stages)?;
        // A predicate on the block leaves one value bare, as the
        // generic unwrap of jsonata-js's predicate result does.
        if !step.stages.is_empty()
            && !Self::path_keeps_singleton_array(std::slice::from_ref(step))
            && Self::is_cons_block(&step.node)
        {
            if let JValue::Array(arr) = &value {
                if arr.sequence && !arr.keep_singleton && arr.len() == 1 {
                    value = arr[0].clone();
                }
            }
        }
        // jsonata-js takes the value as the next step's input sequence
        // as is, so a block's single value, or the object a group-by on
        // the step makes, gives that step no items, and the path no
        // result; a sort still orders it.
        let next_reads_items = steps
            .get(1)
            .is_some_and(|next| !matches!(next.node, AstNode::Sort { .. }));
        Ok(
            if next_reads_items
                && !matches!(value, JValue::Array(_))
                && (Self::is_cons_block(&step.node) || Self::is_cons_group(&step.node))
            {
                JValue::Undefined
            } else {
                value
            },
        )
    }

    /// A path's first-step sort, and whether it gave a consarray step's
    /// single value back. Kept out of line, off the hot path.
    #[inline(never)]
    fn evaluate_first_step_sort(
        &mut self,
        step: &PathStep,
        steps: &[PathStep],
        input: &AstNode,
        terms: &[(AstNode, bool)],
        data: &JValue,
        dotted: bool,
    ) -> Result<(JValue, bool), EvaluatorError> {
        // A path's first-step sort evaluates once against the whole
        // input (`takes_whole_input`), with `$` the input, and gives
        // its tuples, which the rest of the path reads. Its input
        // starts a dotted path when more steps follow.
        let saved = self.keep_tuple_stream;
        self.keep_tuple_stream = true;
        let sorted = self.with_dollar_bound(data, |ev| {
            ev.with_checkpoint(|ev| {
                ev.evaluate_sort_node(input, terms, data, dotted || steps.len() > 1)
            })
        });
        self.keep_tuple_stream = saved;
        let (value, cons_single) = sorted?;
        // jsonata-js's evaluatePath stops at a step that gives nothing, so
        // the sort step's stages do not run then (`x^(a)[$error("e")]`).
        let value = self.apply_filter_stages(value, &step.stages)?;
        // A sort of a consarray first step's single value gives it
        // back as is, which the next step reads as no items, as for
        // the consarray step itself above. A predicate on the sort
        // collects its result into a sequence again.
        let next_reads_items = steps
            .get(1)
            .is_some_and(|next| !matches!(next.node, AstNode::Sort { .. }));
        // `[]` is only a mark there, not a predicate.
        let filters = step
            .stages
            .iter()
            .any(|stage| matches!(stage, Stage::Filter(_)));
        let cons_single = cons_single && !filters;
        let value = if cons_single && next_reads_items {
            JValue::Undefined
        } else {
            collect_single_step_result(value, steps.len() == 1)
        };
        Ok((value, cons_single))
    }

    /// One plain (non-tuple) path step over its input sequence, following
    /// jsonata-js `evaluateStep`: the step is evaluated against each input
    /// item and its filter stages applied to that item's result, and the
    /// results are combined by `collect_step_results`.
    fn evaluate_step(
        &mut self,
        step: &PathStep,
        items: &[JValue],
        source: Option<&JArray>,
        last_step: bool,
    ) -> Result<JValue, EvaluatorError> {
        // A `$` step without stages yields its input items again; unless one
        // of them is an array to flatten, that is the input as a sequence.
        // An array known to be flat skips the O(n) scan (#196).
        if let (AstNode::Variable(name), Some(source)) = (&step.node, source) {
            if name.is_empty()
                && step.stages.is_empty()
                && !(last_step && items.len() == 1)
                && (source.is_flat() || !items.iter().any(flattens_in_step))
            {
                return Ok(JValue::Array(source.fresh_sequence()));
            }
        }
        // One item and no stages (a step inside a lambda, or over a single
        // object) needs no per-item vectors.
        if let ([item], true) = (items, step.stages.is_empty()) {
            let value = match (&step.node, item) {
                (AstNode::Name(field), JValue::Object(obj)) if !is_tuple_object(obj) => {
                    obj.get(field).cloned().unwrap_or(JValue::Undefined)
                }
                (AstNode::Name(_), _) => return self.evaluate_step_items(step, items, last_step),
                (node, item) => self.evaluate_step_node(node, item)?,
            };
            return Ok(collect_single_step_result(value, last_step));
        }
        self.evaluate_step_items(step, items, last_step)
    }

    /// `evaluate_step` over any number of items.
    fn evaluate_step_items(
        &mut self,
        step: &PathStep,
        items: &[JValue],
        last_step: bool,
    ) -> Result<JValue, EvaluatorError> {
        let run_on_nothing = Self::filters_run_on_nothing(&step.stages);
        let values = match &step.node {
            // A field step looks the field up in each item, reusing its
            // position across a homogeneous array.
            // Filter stages run on an item without the field too, where
            // that can raise an error (see `filters_run_on_nothing`).
            AstNode::Name(field) if run_on_nothing => {
                field_values_of::<true>(field, items, &self.options)?
            }
            AstNode::Name(field) => field_values(field, items, &self.options)?,
            // `$` is each item itself.
            AstNode::Variable(name) if name.is_empty() => items.to_vec(),
            _ => self.evaluate_step_node_per_item(step, items)?,
        };
        if run_on_nothing {
            return self.filter_step_values(step, values, last_step);
        }
        let mut results = Vec::with_capacity(values.len());
        for value in values {
            let value = self.apply_filter_stages(value, &step.stages)?;
            if !value.is_undefined() {
                results.push(value);
            }
        }
        check_sequence_length(results.len(), &self.options)?;
        Ok(collect_step_results(results, last_step))
    }

    /// The end of `evaluate_step_items` for filter stages that run on an
    /// undefined value too (`filter_undefined`). Kept out of line, off the
    /// hot path.
    #[inline(never)]
    fn filter_step_values(
        &mut self,
        step: &PathStep,
        values: Vec<JValue>,
        last_step: bool,
    ) -> Result<JValue, EvaluatorError> {
        let mut results = Vec::with_capacity(values.len());
        for value in values {
            let value = if value.is_undefined() {
                self.filter_undefined(&step.stages)?
            } else {
                self.apply_filter_stages(value, &step.stages)?
            };
            if !value.is_undefined() {
                results.push(value);
            }
        }
        check_sequence_length(results.len(), &self.options)?;
        Ok(collect_step_results(results, last_step))
    }

    /// A step node's value for each item, in order, undefined ones included.
    /// An object constructor or a `.(expr)` block that compiles runs on the
    /// compiled path, which gives the same values with less per-item work.
    fn evaluate_step_node_per_item(
        &mut self,
        step: &PathStep,
        items: &[JValue],
    ) -> Result<Vec<JValue>, EvaluatorError> {
        let node = &step.node;
        // Compiling costs about one tree evaluation. It is done once per step
        // and kept by the evaluator (#197), but a step only ever seen with one
        // item would pay it for nothing, so it waits for one with several.
        let compiled = if items.len() < 2 {
            None
        } else {
            self.cached_step_body(step)
        };
        let mut values = Vec::with_capacity(items.len());
        if let Some(compiled) = compiled {
            for item in items {
                values.push(eval_compiled(
                    &compiled.expr,
                    item,
                    None,
                    &self.options,
                    self.start_time,
                )?);
            }
            return Ok(values);
        }
        for item in items {
            values.push(self.evaluate_step_node(node, item)?);
        }
        Ok(values)
    }

    /// An object constructor, following jsonata-js `evaluateGroupExpression`:
    /// each item of the input (see `object_context`) is grouped under the key
    /// each pair gives it, and a pair's value is evaluated over its group.
    fn construct_object(
        &mut self,
        pairs: &[(AstNode, AstNode)],
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        // A literal key puts every item in its pair's group.
        if pairs
            .iter()
            .all(|(key, _)| matches!(key, AstNode::String(_)))
        {
            if let Some(key) = duplicate_literal_key(pairs) {
                return Err(duplicate_key_error(key));
            }
            let context = object_context(data);
            let build = |ev: &mut Self| -> Result<JValue, EvaluatorError> {
                let mut result = IndexMap::with_capacity(pairs.len());
                for (key, value) in pairs {
                    let AstNode::String(key) = key else {
                        unreachable!("all keys are literals")
                    };
                    let value = ev.evaluate_internal(value, &context)?;
                    if !value.is_undefined() {
                        result.insert(key.to_string(), value);
                    }
                }
                Ok(JValue::object(result))
            };
            // `$` is the group's context, also under an outer `$` binding (a
            // step inside a block), bound once for the object and only when
            // a value can read it.
            return if pairs.iter().any(|(_, value)| Self::may_read_dollar(value)) {
                self.with_dollar_bound(&context, build)
            } else {
                build(self)
            };
        }
        let keys_read_dollar = pairs.iter().any(|(key, _)| Self::may_read_dollar(key));
        let values_read_dollar = pairs.iter().any(|(_, value)| Self::may_read_dollar(value));

        let items = group_items(data);
        // Each key's pair and the items grouped under it, in first-seen order.
        let mut groups: IndexMap<String, (usize, Vec<JValue>)> = IndexMap::new();
        for item in items.iter() {
            for (pair_index, (key, _)) in pairs.iter().enumerate() {
                let key = if keys_read_dollar {
                    self.with_dollar_bound(item, |ev| ev.evaluate_internal(key, item))?
                } else {
                    self.evaluate_internal(key, item)?
                };
                let key = match key {
                    JValue::String(key) => key,
                    JValue::Undefined => continue,
                    other => return Err(key_type_error(&other)),
                };
                match groups.get_mut(&*key) {
                    Some((index, _)) if *index != pair_index => {
                        return Err(duplicate_key_error(&key));
                    }
                    Some((_, group)) => group.push(item.clone()),
                    None => {
                        groups.insert(key.to_string(), (pair_index, vec![item.clone()]));
                    }
                }
            }
        }
        let mut result = IndexMap::with_capacity(groups.len());
        for (key, (pair_index, group)) in groups {
            let context = group_context(&group);
            let value = &pairs[pair_index].1;
            let value = if values_read_dollar {
                self.with_dollar_bound(&context, |ev| ev.evaluate_internal(value, &context))?
            } else {
                self.evaluate_internal(value, &context)?
            };
            if !value.is_undefined() {
                result.insert(key, value);
            }
        }
        Ok(JValue::object(result))
    }

    /// Whether a field step's filter `stages` are run on an item without
    /// the field (see `apply_filter_stages`): not when the first is a
    /// position, which selects nothing there and stops the rest (`x[0]`),
    /// or when none calls anything (`x[$ > 5]`). Those keep the faster
    /// lookup that drops such items, as the compiled path does.
    #[inline(never)]
    fn filters_run_on_nothing(stages: &[Stage]) -> bool {
        let first_selects = stages
            .iter()
            .find_map(|stage| match stage {
                Stage::Filter(predicate) => Some(numeric_literal(predicate).is_none()),
                _ => None,
            })
            .unwrap_or(false);
        first_selects
            && stages.iter().any(
                |stage| matches!(stage, Stage::Filter(predicate) if Self::may_raise_on_nothing(predicate)),
            )
    }

    /// False when `node` makes no call (of a function, lambda or `~>`), the
    /// only way a filter evaluated with nothing as its context raises an
    /// error that matters here: operators on undefined give undefined.
    /// Unknown kinds of node count as calling.
    fn may_raise_on_nothing(node: &AstNode) -> bool {
        match node {
            AstNode::String(_)
            | AstNode::Name(_)
            | AstNode::Number(_)
            | AstNode::Boolean(_)
            | AstNode::Null
            | AstNode::Undefined
            | AstNode::Regex { .. }
            | AstNode::ParentVariable(_)
            | AstNode::Parent(_)
            | AstNode::Wildcard
            | AstNode::Descendant
            | AstNode::Variable(_) => false,
            AstNode::Path { steps } => steps.iter().any(|step| {
                Self::may_raise_on_nothing(&step.node)
                    || step.stages.iter().any(|stage| match stage {
                        Stage::Filter(expr) => Self::may_raise_on_nothing(expr),
                        _ => false,
                    })
            }),
            AstNode::Binary { op, lhs, rhs } => {
                *op == BinaryOp::ChainPipe
                    || Self::may_raise_on_nothing(lhs)
                    || Self::may_raise_on_nothing(rhs)
            }
            AstNode::Unary { operand, .. } => Self::may_raise_on_nothing(operand),
            AstNode::Array(items) | AstNode::ArrayGroup(items) | AstNode::Block(items) => {
                items.iter().any(Self::may_raise_on_nothing)
            }
            AstNode::Object(pairs) => pairs.iter().any(|(key, value)| {
                Self::may_raise_on_nothing(key) || Self::may_raise_on_nothing(value)
            }),
            AstNode::Conditional {
                condition,
                then_branch,
                else_branch,
            } => {
                Self::may_raise_on_nothing(condition)
                    || Self::may_raise_on_nothing(then_branch)
                    || else_branch
                        .as_deref()
                        .is_some_and(Self::may_raise_on_nothing)
            }
            _ => true,
        }
    }

    /// False when evaluating `node` cannot read the `$` binding, so an
    /// object constructor need not rebind it: no `$` in it, and no lambda or
    /// call without arguments that could take the context. Unknown kinds of
    /// node count as reading it.
    fn may_read_dollar(node: &AstNode) -> bool {
        match node {
            AstNode::String(_)
            | AstNode::Name(_)
            | AstNode::Number(_)
            | AstNode::Boolean(_)
            | AstNode::Null
            | AstNode::Undefined
            | AstNode::Regex { .. }
            | AstNode::ParentVariable(_)
            | AstNode::Parent(_)
            | AstNode::Wildcard
            | AstNode::Descendant => false,
            AstNode::Variable(name) => name.is_empty(),
            AstNode::Path { steps } => steps.iter().any(|step| {
                Self::may_read_dollar(&step.node)
                    || step.stages.iter().any(|stage| match stage {
                        Stage::Filter(expr) => Self::may_read_dollar(expr),
                        _ => false,
                    })
            }),
            AstNode::Binary { lhs, rhs, .. } => {
                Self::may_read_dollar(lhs) || Self::may_read_dollar(rhs)
            }
            AstNode::Unary { operand, .. } => Self::may_read_dollar(operand),
            AstNode::Function { args, .. } => {
                args.is_empty() || args.iter().any(Self::may_read_dollar)
            }
            AstNode::Array(items) | AstNode::ArrayGroup(items) | AstNode::Block(items) => {
                items.iter().any(Self::may_read_dollar)
            }
            AstNode::Object(pairs) => pairs
                .iter()
                .any(|(key, value)| Self::may_read_dollar(key) || Self::may_read_dollar(value)),
            AstNode::Conditional {
                condition,
                then_branch,
                else_branch,
            } => {
                Self::may_read_dollar(condition)
                    || Self::may_read_dollar(then_branch)
                    || else_branch.as_deref().is_some_and(Self::may_read_dollar)
            }
            _ => true,
        }
    }

    /// Evaluate a path step's node against one input item. `$` is the item,
    /// a `.(expr)` block evaluates `expr` with `$` bound to the item, and a
    /// `.[...]` step builds a constructed (`cons`) array.
    fn evaluate_step_node(
        &mut self,
        node: &AstNode,
        item: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        match node {
            AstNode::Variable(name) if name.is_empty() => Ok(item.bare()),
            // `$` is the item there, bound even with no outer `$`: the
            // lambda-step desugaring (`x.function($v){...}()`) passes `$`
            // explicitly. A `.(...)` step's expression is a Block, which gives
            // its `:=` bindings a scope of their own and, as in jsonata-js,
            // counts as a level of evaluation depth.
            AstNode::FunctionApplication(expr) => {
                self.with_dollar(item, |ev| ev.evaluate_internal(expr, item))
            }
            AstNode::ArrayGroup(elements) => {
                let mut arr =
                    self.with_dollar_bound(item, |ev| ev.construct_array(elements, item))?;
                arr.cons = true;
                Ok(JValue::Array(arr))
            }
            // These don't read `$`, so need no binding.
            AstNode::Variable(_) | AstNode::ParentVariable(_) | AstNode::Name(_) => {
                self.evaluate_internal(node, item)
            }
            _ => self.with_dollar_bound(item, |ev| ev.evaluate_internal(node, item)),
        }
    }

    /// Run `f` with `$` bound to `item` when an outer `$` binding exists, so
    /// that `$` inside still means the current item, as in jsonata-js. With no
    /// outer binding, `$` already resolves to the data `f` is given.
    fn with_dollar_bound<T>(
        &mut self,
        item: &JValue,
        f: impl FnOnce(&mut Self) -> Result<T, EvaluatorError>,
    ) -> Result<T, EvaluatorError> {
        if self.context.lookup("$").is_none() {
            return f(self);
        }
        self.with_dollar(item, f)
    }

    /// Run `f` with `$` bound to `item` in the top scope, leaving the top
    /// scope as it was afterwards.
    fn with_dollar<T>(
        &mut self,
        item: &JValue,
        f: impl FnOnce(&mut Self) -> Result<T, EvaluatorError>,
    ) -> Result<T, EvaluatorError> {
        let previous = self.context.bind_top("$", item.clone());
        let result = f(self);
        self.context.restore_top("$", previous);
        result
    }

    /// A block: its expressions in order, in a scope of their own, giving the
    /// last one's value. The scope is popped however the block ends.
    fn evaluate_block(
        &mut self,
        expressions: &[AstNode],
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        self.context.push_scope();
        // An empty block (`()`) is undefined in jsonata-js, not null. A bare
        // top-level `()` never gets here (the parser makes it
        // `AstNode::Undefined`), but a `.()` step does.
        let mut result = Ok(JValue::Undefined);
        // Only the last expression is the block's value, so only it may keep
        // a tuple stream.
        let keep = std::mem::replace(&mut self.keep_tuple_stream, false);
        for (index, expr) in expressions.iter().enumerate() {
            if index + 1 == expressions.len() {
                self.keep_tuple_stream = keep;
            }
            result = self.evaluate_internal(expr, data);
            if result.is_err() {
                break;
            }
        }
        self.keep_tuple_stream = keep;
        // Closures escaping the block (IIFE pattern) carry their own storage
        // inside the value, so popping is unconditional.
        self.context.pop_scope();
        result
    }

    /// Apply a step's filter stages, in order, to one item's result. `[]`
    /// changes nothing here; it is handled once the path has a result. An
    /// undefined value stays undefined, its filters not run; see
    /// `filter_undefined` for where jsonata-js runs them.
    fn apply_filter_stages(
        &mut self,
        mut value: JValue,
        stages: &[Stage],
    ) -> Result<JValue, EvaluatorError> {
        for stage in stages {
            if let Stage::Filter(predicate) = stage {
                value = self.evaluate_filter(predicate, value)?;
            }
        }
        Ok(value)
    }

    /// A step's filter stages on an undefined value, as jsonata-js's
    /// `evaluateStep` runs them on an item the step gives nothing for: its
    /// `evaluateFilter` makes that value the one-item sequence
    /// `[undefined]`, so each predicate is evaluated once with an undefined
    /// context and its errors surface (`x[$string(1,2,3)]` is T0410 where
    /// `x` is missing), until one selects nothing. The value stays
    /// undefined. Only for stages `filters_run_on_nothing` accepts.
    #[cold]
    #[inline(never)]
    fn filter_undefined(&mut self, stages: &[Stage]) -> Result<JValue, EvaluatorError> {
        for stage in stages {
            if let Stage::Filter(predicate) = stage {
                if !self.filter_selects_undefined(predicate)? {
                    break;
                }
            }
        }
        Ok(JValue::Undefined)
    }

    /// Whether jsonata-js's `evaluateFilter` keeps the item of its input
    /// `[undefined]`: a numeric literal or a numeric selector picks the
    /// undefined item, which it drops; any other selector keeps it when
    /// truthy. The predicate is evaluated (not a literal) for its errors.
    fn filter_selects_undefined(&mut self, predicate: &AstNode) -> Result<bool, EvaluatorError> {
        if numeric_literal(predicate).is_some() {
            return Ok(false);
        }
        let undefined = JValue::Undefined;
        let selector =
            self.with_dollar_bound(&undefined, |ev| ev.evaluate_internal(predicate, &undefined))?;
        Ok(predicate_index_match(&selector, 0, 1).is_none() && self.is_truthy(&selector))
    }

    /// Apply a filter predicate to a value, following jsonata-js
    /// `evaluateFilter`. A non-array value is a one-item sequence. A numeric
    /// literal selects the item at that position (counting from the end when
    /// negative), and an array item selected that way is the result itself.
    /// Any other predicate is evaluated against each item: a number or array
    /// of numbers selects by position, anything else by truthiness.
    fn evaluate_filter(
        &mut self,
        predicate: &AstNode,
        input: JValue,
    ) -> Result<JValue, EvaluatorError> {
        if input.is_undefined() {
            return Ok(JValue::Undefined);
        }
        let single;
        let items: &[JValue] = match &input {
            JValue::Array(arr) => arr,
            other => {
                single = [other.clone()];
                &single
            }
        };
        // Sorted tuples are filtered as plain objects, and stay sorted tuples
        // (jsonata-js marks a filter's result only when its input is marked).
        let sorted_tuples = matches!(&input, JValue::Array(arr) if arr.sorted_tuples);
        let sequence = |items: Vec<JValue>| {
            let mut arr = JArray::new(items);
            arr.sequence = true;
            arr.sorted_tuples = sorted_tuples;
            JValue::Array(arr)
        };

        if let Some(position) = numeric_literal(predicate) {
            return Ok(match item_at(items, position) {
                Some(JValue::Array(arr)) => JValue::Array(arr.clone()),
                Some(item) => sequence(vec![item.clone()]),
                None => sequence(Vec::new()),
            });
        }

        let plain = match &input {
            JValue::Array(arr) if sorted_tuples => Some(sorted_tuple_items(arr)),
            _ => None,
        };
        let len = items.len();
        let mut results = Vec::new();
        for (index, item) in items.iter().enumerate() {
            let context = plain.as_ref().map_or(item, |plain| &plain[index]);
            let selector =
                self.with_dollar_bound(context, |ev| ev.evaluate_internal(predicate, context))?;
            let repeats = match predicate_index_match(&selector, index, len) {
                Some(n) => n,
                None => usize::from(self.is_truthy(&selector)),
            };
            for _ in 0..repeats {
                results.push(item.clone());
            }
        }
        Ok(sequence(results))
    }

    /// True when a path step carries a tuple-binding flag (`@$var` focus,
    /// `#$var` index, or a resolved `%` ancestor label) and must therefore
    /// produce/extend a tuple stream rather than be evaluated as a plain step.
    ///
    fn step_creates_tuple(step: &PathStep) -> bool {
        step.focus.is_some() || step.index_var.is_some() || step.ancestor_label.is_some()
    }

    /// True when any step makes the path a tuple stream. jsonata-js collects
    /// such a path's result into a fresh sequence of each tuple's `@` value.
    fn is_tuple_path(steps: &[PathStep]) -> bool {
        steps.iter().any(Self::step_creates_tuple)
    }

    /// True for a path node that is a tuple stream (one of its steps is
    /// `tuple`). A sort or group-by over one sees its tuples, as jsonata-js
    /// passes them its path's tuple bindings; over anything else, even a
    /// block holding such a path, it sees values.
    fn is_tuple_path_node(node: &AstNode) -> bool {
        matches!(node, AstNode::Path { steps } if steps.iter().any(|step| step.is_tuple))
    }

    /// True when a group-by over `node` reduces a tuple stream, as jsonata-js
    /// does over a tuple path, or a sort of one. A bind on an expression
    /// jsonata-js doesn't treat as a path (`$x@$y`) leaves it a plain value.
    fn groups_tuple_stream(node: &AstNode) -> bool {
        match node {
            AstNode::Path { steps } if Self::is_non_path(steps) => {
                let step = &steps[0];
                step.index_var.is_some() || step.ancestor_label.is_some() || step.inner_tuple
            }
            AstNode::Sort { input, .. } => Self::groups_tuple_stream(input),
            _ => Self::is_tuple_path_node(node),
        }
    }

    /// True for a sort of a tuple stream, alone or with stages after it, whose
    /// tuples a sort or group-by around it sees as plain objects (#214).
    fn sorts_tuples(node: &AstNode) -> bool {
        match node {
            AstNode::Sort { input, .. } => {
                Self::is_tuple_path_node(input) || Self::sorts_tuples(input)
            }
            AstNode::Path { steps } if steps.len() == 1 => Self::sorts_tuples(&steps[0].node),
            _ => false,
        }
    }

    /// True for a one-step path whose step is an expression jsonata-js does
    /// not treat as a path -- `[...][0]`, `$x[]`, `*[0]`, `(expr)[]` -- which
    /// this parser represents as a one-step path only to carry the postfix
    /// predicate or `[]`. jsonata-js paths are names, dotted chains and sorts.
    fn is_non_path(steps: &[PathStep]) -> bool {
        steps.len() == 1 && !matches!(steps[0].node, AstNode::Name(_) | AstNode::Sort { .. })
    }

    /// True when a path has an explicit `[]` (keep-array) stage, on any
    /// step or inside a sort step's input path: the `[]` of
    /// `$#$pos[][$pos<3]^($)[-1]` must survive the sort and the trailing
    /// index, so the singleton result stays `[4]`.
    fn path_keeps_singleton_array(steps: &[PathStep]) -> bool {
        steps.iter().any(|step| {
            if step.stages.iter().any(|s| matches!(s, Stage::KeepArray)) {
                return true;
            }
            if let AstNode::Sort { input, .. } = &step.node {
                // A `.` after the sort makes one path of all the steps before
                // it, which jsonata-js marks for any `[]` among them.
                return if steps.len() > 1 {
                    let (on_step, on_node) = Self::sort_chain_keeps(input);
                    on_step || on_node
                } else {
                    Self::sort_input_keeps_singleton(input)
                };
            }
            false
        })
    }

    /// The `[]`s in a sort's input chain, as the steps jsonata-js makes of
    /// it: whether one is on a step other than a sort, which marks a path a
    /// later `.` makes of them, and whether one is on a sort, which marks
    /// the sort's path node.
    fn sort_chain_keeps(node: &AstNode) -> (bool, bool) {
        match node {
            AstNode::Sort { input, .. } => Self::sort_chain_keeps(input),
            AstNode::Path { steps } => {
                let (mut on_step, mut on_node) = (false, false);
                for step in steps {
                    let keeps = step.stages.iter().any(|s| matches!(s, Stage::KeepArray));
                    if matches!(&step.node, AstNode::Sort { .. }) {
                        let (inner_step, inner_node) = Self::sort_chain_keeps(&step.node);
                        on_step |= inner_step;
                        on_node |= inner_node || keeps;
                    } else {
                        on_step |= keeps;
                    }
                }
                (on_step, on_node)
            }
            _ => (false, false),
        }
    }

    /// True when a dotted path has a `[]` jsonata-js puts on a step, which
    /// marks the whole path: one on any step but a sort, or in the chain a
    /// first-step sort sorts (see `sort_chain_keeps`).
    fn steps_keep_singleton(steps: &[PathStep]) -> bool {
        steps.iter().any(|step| match &step.node {
            AstNode::Sort { input, .. } => Self::sort_chain_keeps(input).0,
            _ => step.stages.iter().any(|s| matches!(s, Stage::KeepArray)),
        })
    }

    /// True for a block jsonata-js marks `consarray` (parser.js `case
    /// 'block'`): one of its expressions is such a block, or a path whose
    /// first step is an array constructor or such a block. As a path's first
    /// step it is evaluated once against the whole input, not per item.
    fn is_cons_block(node: &AstNode) -> bool {
        let AstNode::Block(exprs) = node else {
            return false;
        };
        exprs.iter().any(|expr| Self::starts_cons(expr, false))
    }

    /// True for a group-by on a `consarray` step: an array constructor
    /// (with any predicates or `[]`) or a consarray block. jsonata-js
    /// groups on that node itself (parser.js `{`), so a path's first step
    /// evaluates once to the grouped object, as `evaluate_cons_first_step`
    /// does, rather than once per input item.
    fn is_cons_group(node: &AstNode) -> bool {
        let AstNode::ObjectTransform { input, .. } = node else {
            return false;
        };
        let input = match input.as_ref() {
            AstNode::Path { steps } if steps.len() == 1 => &steps[0].node,
            other => other,
        };
        matches!(input, AstNode::Array(_)) || Self::is_cons_block(input)
    }

    /// True when `node` starts with a `consarray` first step: an array
    /// constructor that begins a dotted path (`dotted`), or a consarray
    /// block. A sort is a step of the path it ends in jsonata-js, so
    /// `[3,1].$^($)` and `[3,1]^($).$` start with one too.
    fn starts_cons(node: &AstNode, dotted: bool) -> bool {
        match node {
            AstNode::Array(_) => dotted,
            AstNode::Block(_) => Self::is_cons_block(node),
            AstNode::Sort { input, .. } => Self::starts_cons(input, dotted),
            AstNode::Path { steps } if !Self::is_non_path(steps) => {
                Self::starts_cons(&steps[0].node, dotted || steps.len() > 1)
            }
            _ => false,
        }
    }

    /// Evaluate a sort node: its input, then the ordering. `dotted` is set
    /// when the sort is the first step of a longer path, where jsonata-js
    /// makes an array constructor that starts its input `consarray`. The
    /// flag returned is set when such a consarray first step gave a single
    /// value rather than an array, which the path's next step reads as no
    /// items.
    fn evaluate_sort_node(
        &mut self,
        input: &AstNode,
        terms: &[(AstNode, bool)],
        data: &JValue,
        dotted: bool,
    ) -> Result<(JValue, bool), EvaluatorError> {
        // Keep the input path's tuple wrappers so the sort terms can read
        // the carried `%`/`$focus`/`$index` bindings per element.
        let saved = self.keep_tuple_stream;
        let tuple_input = Self::is_tuple_path_node(input) || Self::sorts_tuples(input);
        self.keep_tuple_stream = tuple_input;
        let value = if tuple_input {
            // A stream of one tuple stays a stream, not the bare tuple.
            self.evaluate_unwrapped_later(input, data)
                .map(|value| (value, false))
        } else {
            // jsonata-js sorts as a step after the input's last step,
            // which is then not the last step: its result is
            // flattened into a sequence, and `fn.sort` returns a
            // sequence of zero or one item as is, so the generic
            // unwrap applies to it (#221). The input's array, whole
            // (the last-step rule, a constructor, a block) or a
            // sequence, becomes a fresh sequence here, without any
            // `[]` mark of its own.
            self.evaluate_sort_input(input, data, dotted)
        };
        let (value, cons_single) = match value {
            Ok((value, cons_single)) => (Ok(value), cons_single),
            Err(e) => (Err(e), false),
        };
        // The sort terms are expressions of their own, which keep no
        // tuple stream.
        self.keep_tuple_stream = false;
        let sorted = value.and_then(|value| self.evaluate_sort(&value, terms, tuple_input));
        self.keep_tuple_stream = saved;
        let mut sorted = sorted?;
        if let JValue::Array(arr) = &mut sorted {
            // Only a tuple input gives tuples; input data shaped like
            // a wrapper is left alone.
            if tuple_input && starts_with_tuple(arr) {
                // A `[]` inside the input path doesn't carry over;
                // the path's own `[]` is applied below.
                arr.keep_singleton = false;
                if saved {
                    // Sorting tuples gives a plain array of them in
                    // jsonata-js, which is no longer marked as a tuple
                    // stream (#214). Its sort returns zero or one
                    // item as it was, still marked.
                    arr.sorted_tuples |= arr.len() > 1;
                } else {
                    // Nothing around the sort reads the tuples, so
                    // the path ends here and gives each one's value.
                    let mut values = arr.with_items(arr.iter().map(tuple_focus_value).collect());
                    values.sequence = true;
                    values.sorted_tuples = false;
                    sorted = JValue::Array(values);
                }
            }
        }
        // jsonata-js sorts as a step of the path, so the path's own
        // result rules apply after the sort: a tuple-stream path
        // re-collects into a fresh sequence, and `[]` keeps it.
        if Self::is_tuple_path_node(input) {
            sorted.mark_sequence();
        }
        if Self::sort_input_keeps_singleton(input) {
            sorted.mark_keep_singleton();
        }
        Ok((sorted, cons_single))
    }

    /// Evaluate a non-tuple sort input as the first step of the path
    /// jsonata-js makes of it, collected as a step that is not the last.
    /// A name path, a variable or a sort is evaluated once against the whole
    /// input. Any other expression (a block, literal, function call or array
    /// constructor) is a first step, mapped over the items of an array input
    /// one by one; the top-level array is one item, the bare array. Its
    /// results are flattened into a fresh sequence: jsonata-js marks an array
    /// constructor `consarray` only on a `.` path, so one is flattened here.
    fn evaluate_sort_input(
        &mut self,
        input: &AstNode,
        data: &JValue,
        dotted: bool,
    ) -> Result<(JValue, bool), EvaluatorError> {
        let first = match input {
            AstNode::Path { steps } if !Self::is_non_path(steps) => None,
            AstNode::Path { steps } => Some(&steps[0].node),
            other => Some(other),
        };
        let whole = match first {
            None => true,
            Some(node) => {
                matches!(
                    node,
                    AstNode::Variable(_) | AstNode::ParentVariable(_) | AstNode::Sort { .. }
                ) || Self::starts_cons(node, dotted)
            }
        };
        // A sort of a sort is one path: the inner sort's input is its first
        // step too.
        let pass_dotted = dotted && matches!(input, AstNode::Sort { .. });
        let pass_dotted_path = dotted
            && matches!(input, AstNode::Path { steps } if steps.len() == 1
                && !steps[0].is_tuple && !Self::step_creates_tuple(&steps[0]));
        let uncons = |value: JValue| match value {
            JValue::Array(mut arr) if first.is_some() && arr.cons => {
                arr.cons = false;
                JValue::Array(arr)
            }
            other => other,
        };
        let per_item = match data {
            JValue::Array(arr) if !whole && !arr.outer_wrapper => Some(arr.clone()),
            _ => None,
        };
        let Some(items) = per_item else {
            let item = match data {
                JValue::Array(arr) if !whole && arr.outer_wrapper => data.bare(),
                _ => data.clone(),
            };
            let mut cons_single = false;
            let value = if whole {
                let value = match input {
                    AstNode::Sort {
                        input: inner,
                        terms,
                    } => {
                        let (value, inner_single) = self.with_checkpoint(|ev| {
                            ev.evaluate_sort_node(inner, terms, &item, pass_dotted)
                        })?;
                        cons_single = inner_single;
                        value
                    }
                    // A one-step path holding a sort (with its predicates)
                    // is that sort's step: under `dotted` it starts the
                    // path too.
                    AstNode::Path { steps }
                        if pass_dotted_path && matches!(steps[0].node, AstNode::Sort { .. }) =>
                    {
                        let (value, inner_single) = self.with_checkpoint(|ev| {
                            let keep = std::mem::replace(&mut ev.keep_tuple_stream, false);
                            let result = ev.evaluate_path_steps(steps, &item, keep, true);
                            ev.keep_tuple_stream = keep;
                            result
                        })?;
                        cons_single = inner_single;
                        value
                    }
                    _ => self.evaluate_unwrapped_later(input, &item)?,
                };
                if dotted && first.is_some_and(|node| Self::starts_cons(node, true)) {
                    let stages = match input {
                        AstNode::Path { steps } => !steps[0].stages.is_empty(),
                        _ => false,
                    };
                    let single = match &value {
                        JValue::Array(arr) => {
                            stages && arr.sequence && !arr.keep_singleton && arr.len() == 1
                        }
                        JValue::Undefined => false,
                        _ => true,
                    };
                    cons_single |= single;
                }
                value
            } else {
                self.with_dollar_bound(&item, |ev| ev.evaluate_unwrapped_later(input, &item))?
            };
            let value = uncons(value);
            // A sort step returns its sorted, filtered value as is to the
            // next step in jsonata-js; only another step's results are
            // flattened into a sequence.
            let ends_in_sort = match input {
                AstNode::Path { steps } => {
                    matches!(
                        steps.last().map(|step| &step.node),
                        Some(AstNode::Sort { .. })
                    )
                }
                AstNode::Sort { .. } => true,
                _ => false,
            };
            let value = match value {
                JValue::Array(arr) if !arr.outer_wrapper && !ends_in_sort => {
                    collect_single_step_result(JValue::Array(arr), false)
                }
                other => other,
            };
            return Ok((value, cons_single));
        };
        let mut results = Vec::with_capacity(items.len());
        for item in items.iter() {
            // `$` is the item, as in any path step.
            let value = uncons(
                self.with_dollar_bound(item, |ev| ev.evaluate_unwrapped_later(input, item))?,
            );
            if !value.is_undefined() {
                results.push(value);
            }
        }
        let value = match results.len() {
            0 => JValue::Undefined,
            _ => collect_step_results(results, false),
        };
        Ok((value, false))
    }

    /// True when a sort keeps a single result as an array because its input
    /// path has a `[]`. jsonata-js appends the sort to the input path, which
    /// carries `keepSingletonArray` only when it is a name or a dotted chain:
    /// a `[]` on a block, variable or constructor (`$x[]^(p)`) applies to
    /// that value alone, which the sort step then collects anew (#221). A
    /// `#` makes such a step a path, so a `[]` written after it
    /// (`$#$pos[]^($)`, not `$x[]#$i^(p)`) applies to the whole path.
    /// A sort of a sort is one path, so the inner sort's input counts.
    fn sort_input_keeps_singleton(input: &AstNode) -> bool {
        match input {
            AstNode::Path { steps } if Self::is_non_path(steps) => {
                let step = &steps[0];
                let mut after_index = step.index_var.is_some();
                step.stages.iter().any(|stage| {
                    after_index |= matches!(stage, Stage::Index(_));
                    after_index && matches!(stage, Stage::KeepArray)
                })
            }
            AstNode::Path { steps } => Self::path_keeps_singleton_array(steps),
            AstNode::Sort { input, .. } => Self::sort_input_keeps_singleton(input),
            _ => false,
        }
    }

    /// Bind a tuple wrapper's carried `$name`/`!label` keys into the current
    /// scope, saving whatever was previously bound under each of those names
    /// so [`TupleKeyBindings::restore`] can put it back afterward.
    ///
    /// This is the single shared implementation of the
    /// "iterate a tuple wrapper's carried keys, bind, evaluate, then undo"
    /// pattern that recurs across `create_tuple_stream`,
    /// `apply_tuple_stages`, and `evaluate_sort` -- it exists specifically so
    /// none of those call sites can regress to a blind `unbind` (which
    /// deletes rather than restores a same-named outer `:=` binding that was
    /// live in the same scope frame; see issue: chained `@`/`#`/sort-term
    /// binding silently clobbering an outer variable of the same name).
    fn bind_tuple_keys(&mut self, tuple_obj: &IndexMap<String, JValue>) -> TupleKeyBindings {
        let mut saved = Vec::new();
        for (key, value) in tuple_obj.iter() {
            let name = if let Some(n) = key.strip_prefix('$') {
                if n.is_empty() {
                    continue;
                }
                n.to_string()
            } else if key.starts_with('!') {
                key.clone()
            } else {
                continue;
            };
            saved.push((name.clone(), self.context.lookup(&name).cloned()));
            self.context.bind(name, value.clone());
        }
        TupleKeyBindings { saved }
    }

    /// Create or extend a tuple stream for a tuple-binding path step, mirroring
    /// jsonata-js's `evaluateTupleStep` (jsonata.js ~L315-380). The returned
    /// vector holds `JValue::Object` tuple wrappers of the shape
    /// `{ "@": value, "$focus"/"$index": ..., "!label": ..., "__tuple__": true }`
    /// which downstream steps consume via the existing tuple-aware handling in
    /// `evaluate_path`.
    ///
    /// `input` is the previous step's result: either an already-built tuple
    /// stream (each wrapper carried forward, per JS's `tupleBindings`) or a
    /// plain value/array entering tuple mode for the first time (each item
    /// wrapped as `{'@': item}`, per JS's `input.map(item => {'@': item})`).
    ///
    /// This is the sole *origin* of fresh `__tuple__` wrapper objects: the other
    /// `"__tuple__".to_string()` insert sites in `evaluate_path`'s single-field
    /// fast paths only *rebuild* a wrapper around a value pulled from an input
    /// element that is already `__tuple__`-tagged, which can only be true if a
    /// `create_tuple_stream` call already ran earlier in this evaluation and set
    /// `tuple_stream_created`. If a future edit adds a wrapping site that can
    /// fire on a value that did NOT come from an existing tuple stream, it must
    /// also set `self.tuple_stream_created = true`, or `Evaluator::evaluate`'s
    /// output-unwrap pass will be skipped and the wrapper will leak to callers.
    fn create_tuple_stream(
        &mut self,
        step: &PathStep,
        input: &JValue,
        is_first_path_step: bool,
        dotted: bool,
    ) -> Result<(Vec<JValue>, bool), EvaluatorError> {
        use std::rc::Rc;

        // Mark that this evaluate() call produced tuple wrappers, so the
        // top-level `evaluate()` knows to run the output-unwrap pass.
        self.tuple_stream_created = true;

        // Gather the incoming tuple bindings.
        let is_tuple_input = matches!(
            input,
            JValue::Array(arr) if starts_with_tuple(arr)
        );
        let incoming: Vec<Rc<crate::value::JMap>> = if is_tuple_input {
            match input {
                JValue::Array(arr) => arr
                    .iter()
                    .filter_map(|t| match t {
                        JValue::Object(o) => Some(o.clone()),
                        _ => None,
                    })
                    .collect(),
                _ => unreachable!(),
            }
        } else {
            let items: Vec<JValue> = match input {
                // The top-level array's wrapper holds one item, the bare
                // array, which a first step that is not a variable maps over
                // as a whole (jsonata-js `outerWrapper`). An empty one keeps
                // giving no items: a literal step, which jsonata-js reads as
                // a field name, must not yield itself.
                JValue::Array(arr)
                    if arr.outer_wrapper
                        && !arr.is_empty()
                        && is_first_path_step
                        && !matches!(&step.node, AstNode::Variable(_)) =>
                {
                    vec![input.bare()]
                }
                // Mirrors jsonata-js evaluatePath's inputSequence rule
                // (`if (Array.isArray(input) && expr.steps[0].type !== 'variable')`):
                // when the path's FIRST step is a variable reference (`$`/`$$`) the
                // input array is taken as a SINGLE sequence value
                // (`createSequence(input)`) rather than iterated per-element. We
                // only need this for a leading INDEX bind (`$#$pos`): the whole
                // array becomes one incoming tuple whose `@` is the array, then
                // the inner position counter walks its elements so `$pos` runs
                // 0..n-1 (not 0 for every singleton). A leading FOCUS bind
                // (`$@$i`) must instead iterate per-element -- focus keeps `@` as
                // the step input, so a single binding would yield one copy of the
                // whole array per element (`$@$i` on [1,2,3] must give [1,2,3],
                // not [[1,2,3],[1,2,3],[1,2,3]]). The rule is scoped to step 0 so
                // `$.$#$pos` (a later step) still iterates per-element.
                JValue::Array(arr)
                    if !(is_first_path_step
                        && matches!(&step.node, AstNode::Variable(_))
                        && step.index_var.is_some()) =>
                {
                    arr.iter().cloned().collect()
                }
                single => vec![single.clone()],
            };
            items
                .into_iter()
                .map(|item| {
                    let mut wrapper = IndexMap::new();
                    wrapper.insert("@".to_string(), item);
                    wrapper.insert("__tuple__".to_string(), JValue::Bool(true));
                    Rc::new(crate::value::JMap::from(wrapper))
                })
                .collect()
        };

        // A sort step in a tuple stream orders the WHOLE stream (not per element)
        // and re-tuples with the index = sorted position, mirroring jsonata-js
        // evaluateTupleStep's `sort` case. `$^($)#$pos[$pos<3]` must sort the
        // array, then number the sorted values, then filter by `$pos`.
        if let AstNode::Sort {
            input: sort_input,
            terms,
        } = &step.node
        {
            // A sort of a tuple stream keeps its tuples' bindings and adds no
            // index; jsonata-js numbers only a sort that starts a stream.
            let keeps_tuples = is_tuple_input
                || matches!(&**sort_input, AstNode::Path { steps } if Self::is_tuple_path(steps))
                || Self::sorts_tuples(sort_input);
            let sorted = if is_tuple_input {
                let stream = JValue::array(
                    incoming
                        .iter()
                        .map(|t| JValue::Object(Rc::new((**t).clone())))
                        .collect(),
                );
                // evaluate_sort is tuple-aware (orders by each wrapper's `@`,
                // with the carried keys bound), returning the wrappers in
                // sorted order.
                self.evaluate_sort(&stream, terms, true)?
            } else {
                // The sort's own input path (`arr` in `arr^($)#$i`) gives what
                // it orders. Over a tuple path the sort returns the tuples,
                // whose bindings carry into the new stream.
                // Its sequence of one item is not unwrapped: the item is the
                // one value sorted, even when it is an array.
                // A sort starting a longer path is its input's first step
                // there (`dotted`).
                let saved = self.keep_tuple_stream;
                self.keep_tuple_stream = true;
                let value = if dotted {
                    self.with_checkpoint(|ev| ev.evaluate_sort_node(sort_input, terms, input, true))
                        .map(|(value, _)| value)
                } else {
                    self.evaluate_unwrapped_later(&step.node, input)
                };
                self.keep_tuple_stream = saved;
                value?
            };
            // A sort of more than one tuple leaves them unmarked (#214).
            let sorted_tuples = match &sorted {
                JValue::Array(a) => a.sorted_tuples || (is_tuple_input && a.len() > 1),
                _ => false,
            };
            let sorted_arr: Vec<JValue> = match sorted {
                JValue::Array(a) => a.iter().cloned().collect(),
                JValue::Null | JValue::Undefined => Vec::new(),
                other => vec![other],
            };
            let mut result = Vec::new();
            for (ss, elem) in sorted_arr.into_iter().enumerate() {
                let mut new_tuple = match elem {
                    JValue::Object(o) if keeps_tuples && is_tuple_object(&o) => (**o).clone(),
                    other => {
                        let value = match &other {
                            JValue::Object(o) if is_tuple_object(o) => tuple_focus_value(&other),
                            _ => other,
                        };
                        let mut m = IndexMap::new();
                        m.insert("@".to_string(), value);
                        if let Some(index_var) = &step.index_var {
                            m.insert(format!("${}", index_var), JValue::from(ss as i64));
                        }
                        m
                    }
                };
                new_tuple.insert("__tuple__".to_string(), JValue::Bool(true));
                result.push(JValue::object(new_tuple));
            }
            let stream = self.apply_tuple_stages(result, &step.stages, sorted_tuples)?;
            return Ok((stream, sorted_tuples));
        }

        let mut result = Vec::new();
        for tuple_obj in incoming {
            // Bind every carried tuple key into a real scope frame so the step
            // expression can see prior focus/index/ancestor bindings, mirroring
            // createFrameFromTuple's "for every key in tuple, frame.bind(...)".
            // Saves/restores rather than blindly unbinding, so a tuple key
            // whose name collides with a live outer `:=` binding doesn't get
            // deleted once this tuple row's evaluation is done.
            // A field step can't see the bindings, so it needs no frame; its
            // stages, which can, run on the built stream below.
            let tuple_bindings =
                (!matches!(step.node, AstNode::Name(_))).then(|| self.bind_tuple_keys(&tuple_obj));

            let actual_data = tuple_obj.get("@").cloned().unwrap_or(JValue::Undefined);
            // A block step whose inner path a `%` resolves through keeps that
            // path's tuples, so their bindings merge into this stream, as
            // jsonata-js does for a `tuple` path.
            let saved_keep = self.keep_tuple_stream;
            if step.inner_tuple {
                self.keep_tuple_stream = true;
            }
            let step_value = self.evaluate_step_node(&step.node, &actual_data);
            self.keep_tuple_stream = saved_keep;

            if let Some(bindings) = tuple_bindings {
                bindings.restore(self);
            }
            let step_value = step_value?;

            let row: Vec<JValue> = match step_value {
                JValue::Undefined => continue,
                JValue::Array(arr) => arr.iter().cloned().collect(),
                other => vec![other],
            };

            for (position, value) in row.into_iter().enumerate() {
                if value.is_undefined() {
                    continue;
                }
                let mut new_tuple = (*tuple_obj).clone();
                if let JValue::Object(inner) = &value {
                    if is_tuple_object(inner) {
                        // A tuple from an inner tuple stream: its bindings,
                        // `@` included, extend this tuple (`Object.assign`).
                        for (key, binding) in inner.iter() {
                            new_tuple.insert(key.clone(), binding.clone());
                        }
                        result.push(JValue::Object(Rc::new(new_tuple)));
                        continue;
                    }
                }
                if let Some(focus_var) = &step.focus {
                    // Focus binding keeps `@` as this step's INPUT (already carried
                    // in the cloned tuple) and binds the result to `$focus`,
                    // matching jsonata-js: `tuple[expr.focus] = res[bb];
                    // tuple['@'] = tupleBindings[ee]['@'];`.
                    new_tuple.insert(format!("${}", focus_var), value);
                } else {
                    new_tuple.insert("@".to_string(), value);
                }
                if let Some(index_var) = &step.index_var {
                    // Index binding records the position of this value WITHIN the
                    // per-binding result row (jsonata-js evaluateTupleStep: the
                    // inner `bb` counter, `tuple[expr.index] = bb`), which resets
                    // for each incoming tuple.
                    new_tuple.insert(format!("${}", index_var), JValue::from(position as i64));
                }
                if let Some(ancestor_label) = &step.ancestor_label {
                    // `%` ancestor: preserve this step's INPUT under the label.
                    new_tuple.insert(ancestor_label.clone(), actual_data.clone());
                }
                new_tuple.insert("__tuple__".to_string(), JValue::Bool(true));
                result.push(JValue::Object(Rc::new(new_tuple)));
            }
        }

        // jsonata-js applies a tuple step's stages to the stream it built
        // (`evaluateStages`), each filter seeing every tuple's bindings.
        Ok((self.apply_tuple_stages(result, &step.stages, false)?, false))
    }

    /// Apply a step's stages, in order, to an already-built tuple stream --
    /// mirrors jsonata-js `evaluateStages` (jsonata.js ~L288-305): a `filter`
    /// keeps the tuples whose predicate is truthy (evaluated against each tuple's
    /// `@` with its carried `$var`/`!label` bindings in scope), and an `index`
    /// stage sets its variable on every surviving tuple to that tuple's position
    /// in the CURRENT stream. Used for steps carrying a second `#$var` index
    /// binding (e.g. `books@$b#$ib[$l.isbn=$b.isbn]#$ib2`), where `$ib` is the
    /// pre-filter position and `$ib2` the post-filter position.
    ///
    /// For `sorted_tuples` (#214), a filter sees each tuple as a plain object,
    /// with no bindings, as jsonata-js's filter does over an unmarked array.
    fn apply_tuple_stages(
        &mut self,
        mut tuples: Vec<JValue>,
        stages: &[Stage],
        sorted_tuples: bool,
    ) -> Result<Vec<JValue>, EvaluatorError> {
        for stage in stages {
            match stage {
                Stage::Filter(pred) => {
                    if let Some(position) = numeric_literal(pred) {
                        tuples = item_at(&tuples, position).cloned().into_iter().collect();
                        continue;
                    }
                    if sorted_tuples {
                        let mut stream = JArray::new(tuples);
                        stream.sorted_tuples = true;
                        tuples = match self.evaluate_filter(pred, JValue::Array(stream))? {
                            JValue::Array(kept) => kept.into_vec(),
                            _ => Vec::new(),
                        };
                        continue;
                    }
                    let len = tuples.len();
                    let mut kept = Vec::with_capacity(len);
                    for (index, tup) in tuples.into_iter().enumerate() {
                        let JValue::Object(obj) = &tup else {
                            continue;
                        };
                        // Bind this tuple's carried focus/index/ancestor keys so
                        // the predicate can reference them (save/restore rather
                        // than blind unbind -- see bind_tuple_keys).
                        let tuple_bindings = self.bind_tuple_keys(obj);
                        let at = obj.get("@").cloned().unwrap_or(JValue::Undefined);
                        let selector =
                            self.with_dollar_bound(&at, |ev| ev.evaluate_internal(pred, &at));
                        tuple_bindings.restore(self);
                        let selector = selector?;
                        let repeats = match predicate_index_match(&selector, index, len) {
                            Some(n) => n,
                            None => usize::from(self.is_truthy(&selector)),
                        };
                        for _ in 0..repeats {
                            kept.push(tup.clone());
                        }
                    }
                    tuples = kept;
                }
                Stage::Index(var) => {
                    for (pos, tup) in tuples.iter_mut().enumerate() {
                        if let JValue::Object(obj) = tup {
                            let mut m = (**obj).clone();
                            m.insert(format!("${}", var), JValue::from(pos as i64));
                            *tup = JValue::Object(Rc::new(m));
                        }
                    }
                }
                // `[]` keeps the stream as-is; the keep-singleton decision is
                // made when the tuple stream is turned back into values.
                Stage::KeepArray => {}
            }
        }
        Ok(tuples)
    }

    /// Evaluate a binary operation
    fn evaluate_binary_op(
        &mut self,
        op: crate::ast::BinaryOp,
        lhs: &AstNode,
        rhs: &AstNode,
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        use crate::ast::BinaryOp;

        // Coalescing (`??`): jsonata-js parses `a ?? b` as
        // `$exists(a) ? a : b`, so an undefined left side gives the right
        // one, null included as a value, and an error on the left propagates.
        // jsonata-js evaluates `a` twice that way; once gives the same value
        // unless `a` isn't deterministic (`$random()`), and does half the work.
        if op == BinaryOp::Coalesce {
            let value = self.evaluate_internal(lhs, data)?;
            return if value.is_undefined() {
                self.evaluate_internal(rhs, data)
            } else {
                Ok(value)
            };
        }

        // Special handling for default operator (?:)
        // Returns right side if left is falsy or a non-value (like a function)
        if op == BinaryOp::Default {
            let left = self.evaluate_internal(lhs, data)?;
            // `?:` used to have its own truthiness rule, half-recursive where
            // `is_truthy` was flat. Now that `is_truthy` matches `$boolean`
            // the two agree exactly, so there is one rule again (#111).
            if self.is_truthy(&left) {
                return Ok(left);
            }
            return self.evaluate_internal(rhs, data);
        }

        // Special handling for chain/pipe operator (~>)
        // Pipes the LHS result to the RHS function as the first argument
        // e.g., expr ~> func(arg2) becomes func(expr, arg2)
        if op == BinaryOp::ChainPipe {
            return self.evaluate_apply_expression(lhs, rhs, data);
        }

        // Special handling for variable binding (:=)
        if op == BinaryOp::ColonEqual {
            // Extract variable name from LHS
            let var_name = match lhs {
                AstNode::Variable(name) => name.clone(),
                _ => {
                    return Err(EvaluatorError::TypeError(
                        "S0212: The left side of := must be a variable name (start with $)"
                            .to_string(),
                    ))
                }
            };

            // Check if RHS is a lambda - store it specially
            if let AstNode::Lambda {
                params,
                body,
                signature,
                thunk,
            } = rhs
            {
                // Store the lambda AST for later invocation
                // Capture only the free variables referenced by the lambda body
                let captured_env = self.capture_environment_for(body, params);
                let stored_lambda = StoredLambda {
                    params: params.clone(),
                    body: (**body).clone(),
                    compiled_body: CompiledBody::compile(body, params, signature, *thunk),
                    signature: signature.clone(),
                    captured_env,
                    captured_data: Some(data.clone()),
                    thunk: *thunk,
                    // Named definition: the closure knows its own name so the
                    // body's recursive `$f(...)` resolves to THIS closure at
                    // every invocation, wherever the value has traveled.
                    self_name: Some(var_name.clone()),
                    partial: None,
                    live_token: LiveLambdaToken::new(),
                    def_scope: self.context.def_scope(),
                };
                let lambda_value = JValue::Lambda(Rc::new(stored_lambda));
                self.context.bind(var_name, lambda_value.clone());
                return Ok(lambda_value);
            }

            // Evaluate the RHS. A lambda value carries its own closure, so
            // aliasing (`$g := $f`) is just binding the value.
            let value = self.evaluate_internal(rhs, data)?;

            // Bind even if undefined (null) so inner scopes can shadow outer variables
            self.context.bind(var_name, value.clone());
            return Ok(value);
        }

        // Special handling for logical operators (short-circuit evaluation)
        if op == BinaryOp::And {
            let left = self.evaluate_internal(lhs, data)?;
            if !self.is_truthy(&left) {
                // Short-circuit: if left is falsy, return false without evaluating right
                return Ok(JValue::Bool(false));
            }
            let right = self.evaluate_internal(rhs, data)?;
            return Ok(JValue::Bool(self.is_truthy(&right)));
        }

        if op == BinaryOp::Or {
            let left = self.evaluate_internal(lhs, data)?;
            if self.is_truthy(&left) {
                // Short-circuit: if left is truthy, return true without evaluating right
                return Ok(JValue::Bool(true));
            }
            let right = self.evaluate_internal(rhs, data)?;
            return Ok(JValue::Bool(self.is_truthy(&right)));
        }

        // Standard evaluation: evaluate both operands, then dispatch to the
        // shared arithmetic/comparison implementations (one definition for the
        // tree-walker, the compiled path, and the VM).
        let left = self.evaluate_internal(lhs, data)?;
        let right = self.evaluate_internal(rhs, data)?;

        match op {
            BinaryOp::Add => compiled_arithmetic(CompiledArithOp::Add, &left, &right),
            BinaryOp::Subtract => compiled_arithmetic(CompiledArithOp::Sub, &left, &right),
            BinaryOp::Multiply => compiled_arithmetic(CompiledArithOp::Mul, &left, &right),
            BinaryOp::Divide => compiled_arithmetic(CompiledArithOp::Div, &left, &right),
            BinaryOp::Modulo => compiled_arithmetic(CompiledArithOp::Mod, &left, &right),

            // compiled_equal normalizes lazy operands (guarded, zero-cost when neither
            // side is lazy) so conversion failures raise instead of silently comparing
            // unequal.
            BinaryOp::Equal => compiled_equal(&left, &right),
            BinaryOp::NotEqual => compiled_not_equal(&left, &right),
            BinaryOp::LessThan => {
                compiled_ordered_cmp(&left, &right, "<", |a, b| a < b, |a, b| a < b)
            }
            BinaryOp::LessThanOrEqual => {
                compiled_ordered_cmp(&left, &right, "<=", |a, b| a <= b, |a, b| a <= b)
            }
            BinaryOp::GreaterThan => {
                compiled_ordered_cmp(&left, &right, ">", |a, b| a > b, |a, b| a > b)
            }
            BinaryOp::GreaterThanOrEqual => {
                compiled_ordered_cmp(&left, &right, ">=", |a, b| a >= b, |a, b| a >= b)
            }

            // And/Or handled above with short-circuit evaluation
            BinaryOp::And | BinaryOp::Or => unreachable!(),

            BinaryOp::Concatenate => self.concatenate(&left, &right),
            BinaryOp::Range => self.range(&left, &right),
            BinaryOp::In => self.in_operator(&left, &right),

            // Focus binding: should be resolved by ast_transform pass (Task 2)
            BinaryOp::FocusBind => Err(EvaluatorError::EvaluationError(
                "Focus binding operator (@) must be resolved by ast_transform pass".to_string(),
            )),

            // Index binding: should be resolved by ast_transform pass (Task 4,
            // which retired the dedicated AstNode::IndexBind variant in favor
            // of this generic Binary marker, mirroring FocusBind above)
            BinaryOp::IndexBind => Err(EvaluatorError::EvaluationError(
                "Index binding operator (#) must be resolved by ast_transform pass".to_string(),
            )),

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
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        use crate::ast::UnaryOp;

        let value = self.evaluate_internal(operand, data)?;

        match op {
            UnaryOp::Negate => match value {
                // Only *undefined* propagates; null is not a number, so it is
                // D1002 like any other non-number (jsonata-js `evaluateUnary`).
                JValue::Undefined => Ok(JValue::Undefined),
                JValue::Number(n) => Ok(JValue::Number(-n)),
                _ => Err(EvaluatorError::TypeError(
                    "D1002: Cannot negate non-number value".to_string(),
                )),
            },
            UnaryOp::Not => Ok(JValue::Bool(!self.is_truthy(&value))),
        }
    }

    /// Try to fuse an aggregate function call with its Path argument.
    /// Handles patterns like:
    /// - $sum(arr.field) → iterate arr, extract field, accumulate
    /// - $sum(arr[pred].field) → iterate arr, filter, extract, accumulate
    ///
    /// Returns None if the pattern doesn't match (falls back to normal evaluation).
    fn try_fused_aggregate(
        &mut self,
        name: &str,
        arg: &AstNode,
        data: &JValue,
    ) -> Result<Option<JValue>, EvaluatorError> {
        // Only applies to numeric aggregates
        if !matches!(name, "sum" | "max" | "min" | "average") {
            return Ok(None);
        }

        // Argument must be a Path
        let AstNode::Path { steps } = arg else {
            return Ok(None);
        };

        // Pattern: Name(arr).Name(field) — extract field from array, aggregate
        // Pattern: Name(arr)[filter].Name(field) — filter, extract, aggregate
        if steps.len() != 2 {
            return Ok(None);
        }

        // Last step must be a simple Name (the field to extract)
        let field_step = &steps[1];
        if !field_step.stages.is_empty() {
            return Ok(None);
        }
        let AstNode::Name(extract_field) = &field_step.node else {
            return Ok(None);
        };

        // First step: Name with optional filter stage
        let arr_step = &steps[0];
        let AstNode::Name(arr_name) = &arr_step.node else {
            return Ok(None);
        };

        // Get the source array from data
        let arr = match data {
            JValue::Object(obj) => match obj.get(arr_name) {
                Some(JValue::Array(arr)) => arr,
                _ => return Ok(None),
            },
            _ => return Ok(None),
        };

        // Check for filter stage — try CompiledExpr for the predicate
        let filter_compiled = match arr_step.stages.as_slice() {
            [] => None,
            [Stage::Filter(pred)] => self.compile_in_scope(pred, None),
            _ => return Ok(None),
        };
        // If filter stage exists but wasn't compilable, bail out
        if !arr_step.stages.is_empty() && filter_compiled.is_none() {
            return Ok(None);
        }

        // Build shape cache for the array
        let shape = arr.first().and_then(build_shape_cache);

        // Fused iteration: filter (optional) + extract + aggregate
        let mut total = 0.0f64;
        let mut count = 0usize;
        let mut max_val = f64::NEG_INFINITY;
        let mut min_val = f64::INFINITY;
        let mut has_any = false;

        let len = arr.len();
        for (index, item) in arr.iter().enumerate() {
            // Apply the compiled filter if present. As in every filter, a
            // number (or array of numbers) selects by position, so an item
            // can be selected more than once or not at all.
            let repeats = match filter_compiled {
                Some(ref compiled) => {
                    let selector = if let Some(ref s) = shape {
                        eval_compiled_shaped(
                            compiled,
                            item,
                            None,
                            s,
                            &self.options,
                            self.start_time,
                        )?
                    } else {
                        eval_compiled(compiled, item, None, &self.options, self.start_time)?
                    };
                    match predicate_index_match(&selector, index, len) {
                        Some(n) => n,
                        None => usize::from(compiled_is_truthy(&selector)),
                    }
                }
                None => 1,
            };
            if repeats == 0 {
                continue;
            }

            // Extract field value
            let val = match item {
                JValue::Object(obj) => match obj.get(extract_field) {
                    Some(JValue::Number(n)) => *n,
                    // A present non-numeric value is a type error (T0412), not
                    // something to skip. Bail out so the canonical aggregate
                    // raises it, rather than duplicating the check here.
                    Some(_) => return Ok(None),
                    // A missing field is undefined and drops out of the sequence.
                    None => continue,
                },
                // A non-object element has no fields; it drops out too.
                _ => continue,
            };

            has_any = true;
            match name {
                "sum" => total += val * repeats as f64,
                "max" => max_val = max_val.max(val),
                "min" => min_val = min_val.min(val),
                "average" => {
                    total += val * repeats as f64;
                    count += repeats;
                }
                _ => unreachable!(),
            }
        }

        // An empty sequence is undefined, and each aggregate spells that out
        // differently. Defer to the canonical implementation instead of
        // reproducing its empty-input semantics.
        if !has_any {
            return Ok(None);
        }

        Ok(Some(match name {
            "sum" => JValue::Number(total),
            "max" => JValue::Number(max_val),
            "min" => JValue::Number(min_val),
            "average" => JValue::Number(total / count as f64),
            _ => unreachable!(),
        }))
    }

    /// Evaluate a function call
    fn evaluate_function_call(
        &mut self,
        name: &str,
        args: &[AstNode],
        is_builtin: bool,
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        // Check for partial application (any argument is a Placeholder)
        let has_placeholder = args.iter().any(|arg| matches!(arg, AstNode::Placeholder));
        if has_placeholder {
            return self.create_partial_application(name, args, is_builtin, data);
        }

        // FIRST check if this variable holds a function value (lambda or builtin
        // reference). User-defined functions, closures, partial applications and
        // parameters holding function values are all ordinary bindings now, so
        // one lookup covers them — and parameter shadowing (Y-combinator's
        // function($g){$g($g)}) falls out of plain scope order.
        // A builtin reference (`$f := $sum`) and a regex (`$f := /a/`) are
        // function values too.
        if let Some(value) = self.context.lookup(name).cloned() {
            if value.is_function() {
                let mut evaluated_args = Vec::with_capacity(args.len());
                for arg in args {
                    evaluated_args.push(self.evaluate_internal(arg, data)?);
                }
                // A composition's `$f($x)`: jsonata-js's chain function, and
                // so that call, has a null input.
                if name == composition_names::F {
                    return self
                        .as_callback(|ev| ev.call_function_value(&value, &evaluated_args, data));
                }
                return self.call_function_value(&value, &evaluated_args, data);
            }
            // Any other value, undefined included, hides a builtin or host
            // function of the same name, as a jsonata-js frame binding does:
            // `$string := 5; $string(1)` calls 5.
            if is_builtin {
                for arg in args {
                    self.evaluate_internal(arg, data)?;
                }
                return Err(EvaluatorError::ReferenceError(
                    "T1006: Attempted to invoke a non-function".to_string(),
                ));
            }
        }

        // THEN a host-registered custom function (register_fn / register_fn_override).
        // Resolves after the expression's own bindings and lambdas (checked above)
        // and before built-ins, so an explicit override replaces the built-in in
        // call position. Non-override names never collide with a built-in
        // (register_fn rejects that), so the ordering is only observable for
        // overrides. The `is_empty` guard keeps the common (no host fns) path free.
        if !self.host_fns.is_empty() {
            if let Some(f) = self.host_fns.get(name).cloned() {
                let mut evaluated_args = Vec::with_capacity(args.len());
                for arg in args {
                    evaluated_args.push(self.evaluate_internal(arg, data)?);
                }
                let mut ctx = HostCtx::new();
                return f.call(&evaluated_args, &mut ctx);
            }
        }

        // If the function was called without $ prefix and it's not a stored lambda,
        // it's an error (unknown function without $ prefix)
        if !is_builtin && name != "__lambda__" {
            // jsonata-js evaluates the name as a path on the input, so a
            // field holding a function is called: a regex match's `next()`.
            if let JValue::Object(obj) = data {
                if let Some(function) = obj.get(name).filter(|f| f.is_function()).cloned() {
                    let mut evaluated_args = Vec::with_capacity(args.len());
                    for arg in args {
                        evaluated_args.push(self.evaluate_internal(arg, data)?);
                    }
                    return self.call_function_value(&function, &evaluated_args, data);
                }
            }
            // Reached only for a call written WITHOUT the `$`. jsonata-js
            // separates the two cases: a name that is a builtin was almost
            // certainly missing its sigil, and gets T1005 with the suggestion;
            // anything else is T1006.
            return Err(if self.is_builtin_function(name) {
                EvaluatorError::ReferenceError(format!(
                    "T1005: Attempted to invoke a non-function. Did you mean ${}?",
                    name
                ))
            } else {
                EvaluatorError::ReferenceError(
                    "T1006: Attempted to invoke a non-function".to_string(),
                )
            });
        }

        self.evaluate_builtin_call(name, args, data)
    }

    /// Call the builtin `name` with AST arguments, skipping the variable
    /// and host-function lookups `evaluate_function_call` makes first. A
    /// builtin called by value (`$s := $sort; $s(x)`) comes here, so it is
    /// the builtin it names whatever `$sort` is bound to by then.
    fn evaluate_builtin_call(
        &mut self,
        name: &str,
        args: &[AstNode],
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        // Special handling for $exists function
        // It needs to know if the argument is explicit null vs undefined
        if name == "exists" && args.len() == 1 {
            let arg = &args[0];

            // Check if it's an explicit null literal
            if matches!(arg, AstNode::Null) {
                return Ok(JValue::Bool(true)); // Explicit null exists
            }

            // Check if it's a function reference
            if let AstNode::Variable(var_name) = arg {
                // A binding (lambda values are ordinary bindings, so this
                // covers user-defined functions too) shadows a builtin or
                // host function of the same name.
                if let Some(val) = self.context.lookup(var_name) {
                    return Ok(JValue::Bool(!val.is_undefined()));
                }
                return Ok(JValue::Bool(self.is_function_name(var_name)));
            }

            // For other expressions, evaluate and check whether anything is
            // there. An explicit null exists -- only a missing value does not,
            // which is what the AstNode::Null branch above already assumed for
            // the literal form.
            let value = self.evaluate_internal(arg, data)?;
            return Ok(JValue::Bool(!value.is_undefined()));
        }

        // Fused aggregate pipeline: for $sum/$max/$min/$average with a single Path argument,
        // try to fuse filter+extract+aggregate into a single pass.
        if args.len() == 1 {
            if let Some(result) = self.try_fused_aggregate(name, &args[0], data)? {
                return Ok(result);
            }
        }

        let mut evaluated_args = Vec::with_capacity(args.len());
        for arg in args {
            evaluated_args.push(self.evaluate_internal(arg, data)?);
        }

        // Everything that needs nothing but its arguments goes to the shared
        // dispatcher, which owns the prologue (context insertion, lazy
        // materialization, validation, undefined propagation) as well as the
        // per-function logic. Only the builtins that call back into evaluation
        // are still handled below.
        if crate::builtins::is_pure_builtin(name) {
            return crate::builtins::dispatch_pure(name, &evaluated_args, data, &self.options);
        }

        // JSONata feature: when a function is called with one fewer argument than
        // it expects, the context value (data) becomes the implicit first argument.
        // Of the builtins that used to need this, only `replace` still reaches
        // here — everything else routes through `dispatch_pure` above, which
        // owns the same insertion for its own names.
        if evaluated_args.len() == 1 && name == "replace" {
            // `replace` expects 3+ arguments, but received 1. Only insert
            // context if it's a compatible type (string); otherwise let the
            // function throw T0411 for wrong argument count.
            if matches!(data, JValue::String(_)) {
                evaluated_args.insert(0, data.clone());
            }
        }

        #[cfg(feature = "python")]
        for arg in evaluated_args.iter_mut() {
            if matches!(arg, JValue::LazyPyDict(_)) {
                *arg = normalize_lazy(arg)?;
            }
        }

        // Same signature validation the compiled path performs, so the two
        // engines cannot disagree about argument handling.
        if let Some(coerced) = validate_builtin_args(name, &evaluated_args, data)? {
            evaluated_args = coerced;
        }

        // A callback that is not an inline lambda or a variable is evaluated
        // once, here, and its value called for each item (#204).
        if let Some(index) = hof_callback_index(name, args.len()) {
            if !matches!(args[index], AstNode::Lambda { .. } | AstNode::Variable(_)) {
                // Validation may have put the context in front of the
                // arguments (`$sift(fn)`), or dropped an undefined last one.
                let shift = evaluated_args.len().saturating_sub(args.len());
                let callback = evaluated_args
                    .get(index + shift)
                    .cloned()
                    .unwrap_or(JValue::Undefined);
                let mut bound_args = args.to_vec();
                bound_args[index] = AstNode::Variable(HOF_CALLBACK.to_string());
                // Hidden from `$eval`, which runs in the caller's scope.
                self.context.push_internal_scope();
                self.context.bind(HOF_CALLBACK.to_string(), callback);
                let result = self.call_evaluator_builtin(name, &bound_args, evaluated_args, data);
                self.context.pop_scope();
                return result;
            }
        }
        self.call_evaluator_builtin(name, args, evaluated_args, data)
    }

    /// The builtins that call back into evaluation, given their source
    /// arguments and their validated values.
    fn call_evaluator_builtin(
        &mut self,
        name: &str,
        args: &[AstNode],
        evaluated_args: Vec<JValue>,
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        use crate::functions;

        match name {
            "replace" => {
                if evaluated_args.len() < 3 || evaluated_args.len() > 4 {
                    return Err(EvaluatorError::EvaluationError(
                        "replace() requires 3 or 4 arguments".to_string(),
                    ));
                }
                if evaluated_args[0].is_null() {
                    return Ok(JValue::Null);
                }
                if evaluated_args[0].is_undefined() {
                    return Ok(JValue::Undefined);
                }

                let JValue::String(s) = &evaluated_args[0] else {
                    return Err(EvaluatorError::TypeError(
                        "replace() requires string arguments".to_string(),
                    ));
                };
                // jsonata-js checks the pattern before the limit.
                if matches!(&evaluated_args[1], JValue::String(p) if p.is_empty()) {
                    return Err(crate::functions::FunctionError::RuntimeError(
                        "D3010: Pattern cannot be empty".to_string(),
                    )
                    .into());
                }
                let limit = match_limit(evaluated_args.get(3), REPLACE_NEGATIVE_LIMIT)?;
                self.replace_matches(s, &evaluated_args[1], &evaluated_args[2], limit, data)
            }
            "match" => {
                // $match(str, pattern [, limit])
                // Returns array of match objects for regex matches or custom matcher function
                if evaluated_args.is_empty() || evaluated_args.len() > 3 {
                    return Err(EvaluatorError::EvaluationError(
                        "match() requires 1 to 3 arguments".to_string(),
                    ));
                }
                if evaluated_args[0].is_undefined() {
                    return Ok(JValue::Undefined);
                }

                let s = match &evaluated_args[0] {
                    JValue::String(s) => s.clone(),
                    _ => {
                        return Err(EvaluatorError::TypeError(
                            "match() first argument must be a string".to_string(),
                        ))
                    }
                };

                let limit = match_limit(evaluated_args.get(2), MATCH_NEGATIVE_LIMIT)?;

                // Check if second argument is a custom matcher function (lambda)
                let pattern_value = evaluated_args.get(1);
                let is_custom_matcher = pattern_value.is_some_and(|val| {
                    matches!(val, JValue::Lambda { .. } | JValue::Builtin { .. })
                });

                if is_custom_matcher {
                    // Custom matcher function support
                    // Call the matcher with the string, get match objects with {match, start, end, groups, next}
                    return self.match_with_custom_matcher(&s, &evaluated_args[1], limit, data);
                }

                // Get regex pattern from second argument
                let (pattern, flags) = match pattern_value {
                    Some(val) => crate::functions::string::extract_regex(val).ok_or_else(|| {
                        EvaluatorError::TypeError(
                            "match() second argument must be a regex pattern or matcher function"
                                .to_string(),
                        )
                    })?,
                    None => (".*".to_string(), "".to_string()),
                };

                // Build regex via the shared flag translation ($split/$replace
                // use the same helper, so i/m/s behave identically everywhere)
                let re = crate::functions::string::build_regex(&pattern, &flags)
                    .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))?;
                Ok(crate::functions::string::match_regex(&re, &s, limit)?)
            }
            "sift" => {
                // $sift(object, function) or $sift(function) - filter object by predicate
                if args.is_empty() || args.len() > 2 {
                    return Err(EvaluatorError::EvaluationError(
                        "sift() requires 1 or 2 arguments".to_string(),
                    ));
                }

                // Decide from `args` (the source arguments), not from the
                // evaluated values: signature coercion can substitute the
                // context for a `-` parameter, which grows the value list
                // without adding a source argument. Indexing `args` by the
                // value count then runs off the end.
                let func_arg = if args.len() == 1 { &args[0] } else { &args[1] };

                // Detect how many parameters the callback expects
                let param_count = self.get_callback_param_count(func_arg);

                // Helper function to sift a single object
                let sift_object = |evaluator: &mut Self,
                                   obj: &IndexMap<String, JValue>,
                                   func_node: &AstNode,
                                   context_data: &JValue,
                                   param_count: usize|
                 -> Result<JValue, EvaluatorError> {
                    // Only create the object value if callback uses 3 parameters
                    let obj_value = if param_count >= 3 {
                        Some(JValue::object(obj.clone()))
                    } else {
                        None
                    };

                    let mut result = IndexMap::new();
                    for (key, value) in obj.iter() {
                        // hofFuncArgs shaping (see hof_object_call_args)
                        let call_args =
                            hof_object_call_args(value, key, obj_value.as_ref(), param_count);

                        check_loop_timeout(&evaluator.options, evaluator.start_time)?;
                        let pred_result =
                            evaluator.apply_function(func_node, &call_args, context_data)?;
                        if evaluator.is_truthy(&pred_result) {
                            result.insert(key.clone(), value.clone());
                        }
                    }
                    // Return undefined for empty results (will be filtered by function application)
                    if result.is_empty() {
                        Ok(JValue::Undefined)
                    } else {
                        Ok(JValue::object(result))
                    }
                };

                // Handle partial application - if only 1 arg, use current context as object
                if args.len() == 1 {
                    // The one-argument form is `$sift(function)`, with the
                    // object coming from the context via the signature's '-'
                    // marker. `$sift(obj)` looks identical here but is not that
                    // form: the object binds to parameter 1 and jsonata-js
                    // reaches the body with no callback at all, where applying
                    // `undefined` throws. Discriminate on the value in the
                    // callback slot, which validation leaves last -- and only
                    // when something is actually there. An undefined argument
                    // must still fall through: `$sift(missing.x)` iterates no
                    // keys and is undefined, not an error.
                    match evaluated_args.last() {
                        Some(JValue::Undefined) | None => {}
                        Some(function) if function.is_function() => {}
                        Some(_) => return Err(EvaluatorError::TypeError(
                            "T0410: Argument 2 of function sift does not match function signature"
                                .to_string(),
                        )),
                    }
                    // $sift(function) - use current context data as object
                    let data = &normalize_lazy(data)?;
                    match data {
                        JValue::Object(o) => sift_object(self, o, &args[0], data, param_count),
                        JValue::Array(arr) => {
                            // Map sift over each object in the array
                            let mut results = Vec::new();
                            for item in arr.iter() {
                                let item = &normalize_lazy(item)?;
                                if let JValue::Object(o) = item {
                                    let sifted = sift_object(self, o, &args[0], item, param_count)?;
                                    // sift_object returns undefined for empty results
                                    if !sifted.is_undefined() {
                                        results.push(sifted);
                                    }
                                }
                            }
                            Ok(JValue::array(results))
                        }
                        JValue::Null => Ok(JValue::Null),
                        _ => Ok(JValue::Undefined),
                    }
                } else {
                    // $sift(object, function)
                    match &evaluated_args[0] {
                        JValue::Object(o) => sift_object(self, o, &args[1], data, param_count),
                        // jsonata-js iterates the keys of nothing: undefined.
                        JValue::Undefined => Ok(JValue::Undefined),
                        JValue::Null => Ok(JValue::Null),
                        _ => Err(EvaluatorError::TypeError(
                            "sift() first argument must be an object".to_string(),
                        )),
                    }
                }
            }

            "sort" => {
                if evaluated_args.is_empty() || evaluated_args.len() > 2 {
                    return Err(EvaluatorError::EvaluationError(
                        "sort() requires 1 or 2 arguments".to_string(),
                    ));
                }

                // Use pre-evaluated first argument (avoid double evaluation)
                let array_value = &evaluated_args[0];

                // Handle undefined input
                if array_value.is_null() {
                    return Ok(JValue::Null);
                }
                if array_value.is_undefined() {
                    return Ok(JValue::Undefined);
                }

                let mut arr = match array_value {
                    JValue::Array(arr) => arr.to_vec(),
                    other => vec![other.clone()],
                };

                if args.len() == 2 {
                    // Sort using the comparator from raw args (need unevaluated lambda AST)
                    // Use merge sort for O(n log n) performance instead of O(n²) bubble sort
                    self.merge_sort_with_comparator(&mut arr, &args[1], data)?;
                    Ok(JValue::array(arr))
                } else {
                    // Default sort (no comparator)
                    Ok(functions::array::sort(&arr)?)
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

                // A non-array argument is the singleton sequence containing it,
                // so `$map({"p": 1}, fn)` maps over the one object rather than
                // raising. `$filter` already did this; undefined stays undefined.
                let array = match array {
                    JValue::Array(_) | JValue::Undefined => array,
                    other => JValue::array(vec![other]),
                };

                match array {
                    JValue::Array(arr) => {
                        // Detect how many parameters the callback expects
                        let param_count = self.get_callback_param_count(&args[1]);

                        // CompiledExpr fast path: direct lambda with 1 param, compilable body
                        if param_count == 1 {
                            if let AstNode::Lambda {
                                params,
                                body,
                                signature: None,
                                thunk: false,
                            } = &args[1]
                            {
                                let var_refs: Vec<&str> =
                                    params.iter().map(|s| s.as_str()).collect();
                                if let Some(compiled) = self.compile_in_scope(body, Some(&var_refs))
                                {
                                    let param_name = params[0].as_str();
                                    let mut result = Vec::with_capacity(arr.len());
                                    let mut vars = HashMap::new();
                                    for item in arr.iter() {
                                        vars.insert(param_name, item);
                                        let mapped = eval_compiled(
                                            &compiled,
                                            data,
                                            Some(&vars),
                                            &self.options,
                                            self.start_time,
                                        )?;
                                        if !mapped.is_undefined() {
                                            result.push(mapped);
                                        }
                                    }
                                    return hof_result_sequence(result, &self.options);
                                }
                            }
                            // Stored lambda variable fast path: $var with pre-compiled body
                            if let AstNode::Variable(var_name) = &args[1] {
                                if let Some(stored) =
                                    self.context.lookup_lambda_value(var_name).cloned()
                                {
                                    if let Some(ce) = self.usable_compiled_body(&stored) {
                                        let param_name = stored.params[0].as_str();
                                        let call_data =
                                            stored.captured_data.as_ref().unwrap_or(data);
                                        let mut result = Vec::with_capacity(arr.len());
                                        let mut vars: HashMap<&str, &JValue> = HashMap::new();
                                        for item in arr.iter() {
                                            vars.insert(param_name, item);
                                            let mapped = eval_compiled(
                                                ce,
                                                call_data,
                                                Some(&vars),
                                                &self.options,
                                                self.start_time,
                                            )?;
                                            if !mapped.is_undefined() {
                                                result.push(mapped);
                                            }
                                        }
                                        return hof_result_sequence(result, &self.options);
                                    }
                                }
                            }
                        }

                        // Only create the array value if callback uses 3 parameters
                        let arr_value = if param_count >= 3 {
                            Some(JValue::Array(arr.clone()))
                        } else {
                            None
                        };

                        let mut result = Vec::with_capacity(arr.len());
                        for (index, item) in arr.iter().enumerate() {
                            // hofFuncArgs shaping (see hof_array_call_args)
                            let call_args =
                                hof_array_call_args(item, index, arr_value.as_ref(), param_count);

                            check_loop_timeout(&self.options, self.start_time)?;
                            let mapped = self.apply_function(&args[1], &call_args, data)?;
                            // Filter out undefined results but keep explicit null (JSONata map semantics)
                            // undefined comes from missing else clause, null is explicit
                            if !mapped.is_undefined() {
                                result.push(mapped);
                            }
                        }
                        hof_result_sequence(result, &self.options)
                    }
                    JValue::Null => Ok(JValue::Null),
                    JValue::Undefined => Ok(JValue::Undefined),
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
                if array.is_undefined() {
                    return Ok(JValue::Undefined);
                }

                // Handle null input
                if array.is_null() {
                    return Ok(JValue::Undefined);
                }

                // Coerce non-array values to single-element arrays
                // Track if input was a single value to unwrap result appropriately
                // Use references to avoid upfront cloning of all elements
                let single_holder;
                let (items, was_single_value): (&[JValue], bool) = match &array {
                    JValue::Array(arr) => (arr.as_slice(), false),
                    _ => {
                        single_holder = [array.clone()];
                        (&single_holder[..], true)
                    }
                };

                // Detect how many parameters the callback expects
                let param_count = self.get_callback_param_count(&args[1]);

                // CompiledExpr fast path: direct lambda with 1 param, compilable body
                if param_count == 1 {
                    if let AstNode::Lambda {
                        params,
                        body,
                        signature: None,
                        thunk: false,
                    } = &args[1]
                    {
                        let var_refs: Vec<&str> = params.iter().map(|s| s.as_str()).collect();
                        if let Some(compiled) = self.compile_in_scope(body, Some(&var_refs)) {
                            let param_name = params[0].as_str();
                            let mut result = Vec::with_capacity(items.len() / 2);
                            let mut vars = HashMap::new();
                            for item in items.iter() {
                                vars.insert(param_name, item);
                                let pred_result = eval_compiled(
                                    &compiled,
                                    data,
                                    Some(&vars),
                                    &self.options,
                                    self.start_time,
                                )?;
                                if compiled_is_truthy(&pred_result) {
                                    result.push(item.clone());
                                }
                            }
                            if was_single_value {
                                if result.len() == 1 {
                                    return Ok(result.remove(0));
                                } else if result.is_empty() {
                                    return Ok(JValue::Undefined);
                                }
                            }
                            return hof_result_sequence(result, &self.options);
                        }
                    }
                    // Stored lambda variable fast path: $var with pre-compiled body
                    if let AstNode::Variable(var_name) = &args[1] {
                        if let Some(stored) = self.context.lookup_lambda_value(var_name).cloned() {
                            if let Some(ce) = self.usable_compiled_body(&stored) {
                                let param_name = stored.params[0].as_str();
                                let call_data = stored.captured_data.as_ref().unwrap_or(data);
                                let mut result = Vec::with_capacity(items.len() / 2);
                                let mut vars: HashMap<&str, &JValue> = HashMap::new();
                                for item in items.iter() {
                                    vars.insert(param_name, item);
                                    let pred_result = eval_compiled(
                                        ce,
                                        call_data,
                                        Some(&vars),
                                        &self.options,
                                        self.start_time,
                                    )?;
                                    if compiled_is_truthy(&pred_result) {
                                        result.push(item.clone());
                                    }
                                }
                                if was_single_value {
                                    if result.len() == 1 {
                                        return Ok(result.remove(0));
                                    } else if result.is_empty() {
                                        return Ok(JValue::Undefined);
                                    }
                                }
                                return hof_result_sequence(result, &self.options);
                            }
                        }
                    }
                }

                // Only create the array value if callback uses 3 parameters
                let arr_value = if param_count >= 3 {
                    Some(callback_array_arg(&array, items))
                } else {
                    None
                };

                let mut result = Vec::with_capacity(items.len() / 2);

                for (index, item) in items.iter().enumerate() {
                    // hofFuncArgs shaping (see hof_array_call_args)
                    let call_args =
                        hof_array_call_args(item, index, arr_value.as_ref(), param_count);

                    check_loop_timeout(&self.options, self.start_time)?;
                    let predicate_result = self.apply_function(&args[1], &call_args, data)?;
                    if self.is_truthy(&predicate_result) {
                        result.push(item.clone());
                    }
                }

                // If input was a single value, return the single matching item
                // (or undefined if no match)
                if was_single_value {
                    if result.len() == 1 {
                        return Ok(result.remove(0));
                    } else if result.is_empty() {
                        return Ok(JValue::Undefined);
                    }
                }

                hof_result_sequence(result, &self.options)
            }

            "reduce" => {
                if args.len() < 2 || args.len() > 3 {
                    return Err(EvaluatorError::EvaluationError(
                        "reduce() requires 2 or 3 arguments".to_string(),
                    ));
                }

                // Evaluate the array argument
                let array = self.evaluate_internal(&args[0], data)?;
                if array.is_undefined() {
                    return Ok(JValue::Undefined);
                }

                // As jsonata-js `foldLeft`: the callback must take at least
                // two arguments, whatever kind of function it is. A host
                // function's arity is unknown (MAX) and passes.
                let param_count = self.get_callback_param_count(&args[1]);
                if param_count < 2 {
                    return Err(EvaluatorError::EvaluationError(
                        "D3050: The second argument of reduce function must be a function with at least two arguments".to_string(),
                    ));
                }

                // Convert single value to array (JSONata reduce accepts single values)
                // Use references to avoid upfront cloning of all elements
                let single_holder;
                let items: &[JValue] = match &array {
                    JValue::Array(arr) => arr.as_slice(),
                    JValue::Null => return Ok(JValue::Null),
                    _ => {
                        single_holder = [array.clone()];
                        &single_holder[..]
                    }
                };

                if items.is_empty() {
                    // Return initial value if provided, otherwise null
                    return if args.len() == 3 {
                        self.evaluate_internal(&args[2], data)
                    } else {
                        Ok(JValue::Null)
                    };
                }

                // Get initial accumulator
                let mut accumulator = if args.len() == 3 {
                    self.evaluate_internal(&args[2], data)?
                } else {
                    items[0].clone()
                };

                let start_idx = if args.len() == 3 { 0 } else { 1 };

                // CompiledExpr fast path: direct lambda with 2 params, compilable body
                if param_count == 2 {
                    if let AstNode::Lambda {
                        params,
                        body,
                        signature: None,
                        thunk: false,
                    } = &args[1]
                    {
                        let var_refs: Vec<&str> = params.iter().map(|s| s.as_str()).collect();
                        if let Some(compiled) = self.compile_in_scope(body, Some(&var_refs)) {
                            let acc_name = params[0].as_str();
                            let item_name = params[1].as_str();
                            for item in items[start_idx..].iter() {
                                let vars: HashMap<&str, &JValue> =
                                    HashMap::from([(acc_name, &accumulator), (item_name, item)]);
                                accumulator = eval_compiled(
                                    &compiled,
                                    data,
                                    Some(&vars),
                                    &self.options,
                                    self.start_time,
                                )?;
                            }
                            return Ok(accumulator);
                        }
                    }
                    // Stored lambda variable fast path: $var with pre-compiled body
                    if let AstNode::Variable(var_name) = &args[1] {
                        if let Some(stored) = self.context.lookup_lambda_value(var_name).cloned() {
                            if stored.params.len() == 2 {
                                if let Some(ce) = self.usable_compiled_body(&stored) {
                                    let acc_param = stored.params[0].as_str();
                                    let item_param = stored.params[1].as_str();
                                    let call_data = stored.captured_data.as_ref().unwrap_or(data);
                                    for item in items[start_idx..].iter() {
                                        let vars: HashMap<&str, &JValue> = HashMap::from([
                                            (acc_param, &accumulator),
                                            (item_param, item),
                                        ]);
                                        accumulator = eval_compiled(
                                            ce,
                                            call_data,
                                            Some(&vars),
                                            &self.options,
                                            self.start_time,
                                        )?;
                                    }
                                    return Ok(accumulator);
                                }
                            }
                        }
                    }
                }

                // Only create the array value if callback uses 4 parameters
                let arr_value = if param_count >= 4 {
                    Some(callback_array_arg(&array, items))
                } else {
                    None
                };

                // Apply function to each element
                for (idx, item) in items[start_idx..].iter().enumerate() {
                    // For reduce, the function receives (accumulator, value, index, array)
                    // Callbacks may use any subset of these parameters
                    let actual_idx = start_idx + idx;

                    // Build argument list based on what callback expects. The
                    // D3050 check above only inspects a literal inline
                    // lambda; a by-reference callback (stored lambda or
                    // builtin) with arity below 2 slips past it, so guard
                    // here too rather than let `arr_value.unwrap()` panic --
                    // jsonata-js would have raised D3050 before ever reaching
                    // this loop for such a callback.
                    let call_args = match param_count {
                        0..=2 => vec![accumulator.clone(), item.clone()],
                        3 => vec![
                            accumulator.clone(),
                            item.clone(),
                            JValue::Number(actual_idx as f64),
                        ],
                        _ => vec![
                            accumulator.clone(),
                            item.clone(),
                            JValue::Number(actual_idx as f64),
                            arr_value.as_ref().unwrap().clone(),
                        ],
                    };

                    check_loop_timeout(&self.options, self.start_time)?;
                    accumulator = self.apply_function(&args[1], &call_args, data)?;
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

                // As in jsonata-js, an undefined input gives undefined, and
                // any other single value, null included, is a one-item
                // sequence.
                let single_holder;
                let arr: &[JValue] = match &array {
                    JValue::Array(arr) => arr.as_slice(),
                    JValue::Undefined => return Ok(JValue::Undefined),
                    _ => {
                        single_holder = [array.clone()];
                        &single_holder[..]
                    }
                };

                if args.len() == 1 {
                    // No predicate - array must have exactly 1 element
                    match arr.len() {
                        0 => Err(EvaluatorError::EvaluationError(
                            "D3139: The $single() function expected exactly 1 matching result.  Instead it matched 0.".to_string(),
                        )),
                        1 => Ok(arr[0].clone()),
                        count => Err(EvaluatorError::EvaluationError(format!(
                            "D3138: The $single() function expected exactly 1 matching \
                             result.  Instead it matched more. ({} values)",
                            count
                        ))),
                    }
                } else {
                    // With predicate - find exactly 1 matching element.
                    // Detect how many parameters the callback expects so a
                    // by-reference builtin (e.g. `$single(arr, $exists)`)
                    // isn't handed arguments it never declared -- mirrors
                    // the `$filter`/`$map`/`$each` call sites above.
                    let param_count = self.get_callback_param_count(&args[1]);
                    // Only create the array value if callback uses 3 parameters
                    let arr_value = if param_count >= 3 {
                        Some(callback_array_arg(&array, arr))
                    } else {
                        None
                    };
                    let mut matches = Vec::new();
                    for (index, item) in arr.iter().enumerate() {
                        // hofFuncArgs shaping (see hof_array_call_args)
                        let call_args =
                            hof_array_call_args(item, index, arr_value.as_ref(), param_count);
                        check_loop_timeout(&self.options, self.start_time)?;
                        let predicate_result = self.apply_function(&args[1], &call_args, data)?;
                        if self.is_truthy(&predicate_result) {
                            matches.push(item.clone());
                        }
                    }

                    match matches.len() {
                        0 => Err(EvaluatorError::EvaluationError(
                            "D3139: The $single() function expected exactly 1 matching result.  Instead it matched 0.".to_string(),
                        )),
                        1 => Ok(matches.into_iter().next().unwrap()),
                        count => Err(EvaluatorError::EvaluationError(format!(
                            "D3138: The $single() function expected exactly 1 matching result.  \
                             Instead it matched more. ({} values)",
                            count
                        ))),
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

                // Detect how many parameters the callback expects
                let param_count = self.get_callback_param_count(func_arg);

                let obj_value = normalize_lazy(&obj_value)?;

                match obj_value {
                    JValue::Object(obj) => {
                        let mut result = Vec::new();
                        for (key, value) in obj.iter() {
                            // hofFuncArgs shaping (see hof_object_call_args)
                            let obj_whole = (param_count >= 3).then(|| JValue::Object(obj.clone()));
                            let call_args =
                                hof_object_call_args(value, key, obj_whole.as_ref(), param_count);

                            check_loop_timeout(&self.options, self.start_time)?;
                            let fn_result = self.apply_function(func_arg, &call_args, data)?;
                            // Skip undefined results only. jsonata-js guards
                            // this push with `typeof val !== 'undefined'`, so an
                            // explicit null is a result like any other and
                            // `$each({"a": null, "b": 1}, fn)` is `[null, 1]`.
                            if !fn_result.is_undefined() {
                                result.push(fn_result);
                            }
                        }
                        check_sequence_length(result.len(), &self.options)?;
                        // jsonata-js pushes each result into a sequence, so the
                        // sequence rules apply: a single-key object yields the
                        // result itself and an empty one yields undefined.
                        hof_result_sequence(result, &self.options)
                    }
                    JValue::Null => Ok(JValue::Null),
                    // jsonata-js: $each returns undefined when its first argument
                    // is undefined (e.g. a path that resolves to nothing).
                    JValue::Undefined => Ok(JValue::Undefined),
                    _ => Err(EvaluatorError::TypeError(
                        "each() first argument must be an object".to_string(),
                    )),
                }
            }
            "eval" => self.eval_from_values(&evaluated_args, data),

            // `$notreal(1)` -- written with the sigil, so no "did you mean"
            // suggestion applies; jsonata-js calls this T1006.
            _ => Err(EvaluatorError::ReferenceError(format!(
                "T1006: Attempted to invoke a non-function: ${}",
                name
            ))),
        }
    }

    /// Apply a function the way jsonata-js applies a function argument of a
    /// builtin (the closure `evaluateFunction` wraps it in) or the right side
    /// of `~>`: `apply(fn, args, null, environment)`, with a null input. Only
    /// a builtin the function calls in tail position sees that input (see
    /// `tail_apply_input`).
    fn as_callback<T>(
        &mut self,
        call: impl FnOnce(&mut Self) -> Result<T, EvaluatorError>,
    ) -> Result<T, EvaluatorError> {
        self.apply_with(ApplyMode::Callback, call)
    }

    /// Run `call`, a function application, applied as `mode` says.
    fn apply_with<T>(
        &mut self,
        mode: ApplyMode,
        call: impl FnOnce(&mut Self) -> Result<T, EvaluatorError>,
    ) -> Result<T, EvaluatorError> {
        self.callback_apply = mode;
        let result = call(self);
        self.callback_apply = ApplyMode::Direct;
        result
    }

    /// Apply a callback a builtin was given, at `node` (with `value`, its
    /// function, unless it is an inline lambda): as a callback, or raw when
    /// the builtin is a function's tail call. Nothing the callback runs is
    /// the builtin's own
    /// application, so `raw_hof` is cleared while it runs.
    fn apply_hof_callback<T>(
        &mut self,
        node: &AstNode,
        value: Option<&JValue>,
        call: impl FnOnce(&mut Self) -> Result<T, EvaluatorError>,
    ) -> Result<T, EvaluatorError> {
        let mode = match &self.raw_hof {
            None => ApplyMode::Callback,
            Some(raw) => {
                let at = node as *const AstNode as usize;
                let in_args = (raw.args.0..raw.args.1).contains(&at)
                    || matches!(node, AstNode::Variable(name) if name == HOF_CALLBACK);
                let ptr = value.and_then(lambda_ptr);
                let listed = in_args || ptr.is_some_and(|p| raw.fns.contains(&p));
                // A function that came in wrapped (jsonata-js wraps one
                // passed as an argument of a direct call) would be applied
                // with a null input; it is not told apart yet (#254).
                match (value, ptr) {
                    _ if !listed => ApplyMode::Callback,
                    (None, _) | (Some(_), Some(_)) => ApplyMode::Raw,
                    (Some(_), None) => ApplyMode::Callback,
                }
            }
        };
        let raw = self.raw_hof.take();
        let result = self.apply_with(mode, call);
        self.raw_hof = raw;
        result
    }

    /// Run `call`, a builtin applied as a function's tail call, whose
    /// argument nodes are `args` and evaluated function arguments `values`:
    /// it applies those functions raw (see `ApplyMode::Raw`).
    fn as_raw_hof<T>(
        &mut self,
        args: &[AstNode],
        values: &[JValue],
        call: impl FnOnce(&mut Self) -> Result<T, EvaluatorError>,
    ) -> Result<T, EvaluatorError> {
        let start = args.as_ptr() as usize;
        let raw = RawHof {
            args: (start, start + std::mem::size_of_val(args)),
            fns: values.iter().filter_map(lambda_ptr).collect(),
        };
        let outer = self.raw_hof.replace(raw);
        let result = call(self);
        self.raw_hof = outer;
        result
    }

    /// Apply a higher-order function's callback to `values`.
    ///
    /// `evaluate_builtin_call` has already replaced a callback that is not an
    /// inline lambda or a variable with a variable bound to its value
    /// (`HOF_CALLBACK`), so the callback is evaluated once, as in
    /// jsonata-js, however many items it is applied to.
    fn apply_function(
        &mut self,
        func_node: &AstNode,
        values: &[JValue],
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        match func_node {
            AstNode::Lambda {
                params,
                body,
                signature,
                thunk,
            } => {
                // Direct lambda - invoke it
                self.apply_hof_callback(func_node, None, |ev| {
                    ev.invoke_lambda(params, body, signature.as_ref(), values, data, *thunk)
                })
            }
            AstNode::Variable(var_name) => {
                if let Some(value) = self.context.lookup(var_name).cloned() {
                    // A lambda (closure, partial application, composition,
                    // transform), a builtin reference or a regex.
                    if value.is_function() {
                        return self.apply_hof_callback(func_node, Some(&value), |ev| {
                            ev.call_function_value(&value, values, data)
                        });
                    }
                    // Regular variable value - evaluate with first value as context
                    if values.is_empty() {
                        self.evaluate_internal(func_node, data)
                    } else {
                        self.evaluate_internal(func_node, &values[0])
                    }
                } else if self.is_function_name(var_name) {
                    // A builtin or host function reference (e.g., $string)
                    // passed as a callback to another function.
                    self.as_callback(|ev| ev.call_builtin_with_values(var_name, values, data))
                } else {
                    // Unknown variable - evaluate with first value as context
                    if values.is_empty() {
                        self.evaluate_internal(func_node, data)
                    } else {
                        self.evaluate_internal(func_node, &values[0])
                    }
                }
            }
            // Not reached from the higher-order functions (see above); any
            // other expression is evaluated to the function it gives.
            _ => {
                let value = self.evaluate_internal(func_node, data)?;
                if value.is_function() {
                    return self.as_callback(|ev| ev.call_function_value(&value, values, data));
                }
                Ok(value)
            }
        }
    }

    /// Run a transform's function value (built by the `Transform` arm) on
    /// its argument, in a scope holding the variables it closed over and
    /// with `$` bound to the argument. `None` for any other lambda.
    fn invoke_transform(
        &mut self,
        stored: &StoredLambda,
        args: &[JValue],
    ) -> Option<Result<JValue, EvaluatorError>> {
        let AstNode::Transform {
            location,
            update,
            delete,
        } = &stored.body
        else {
            return None;
        };
        if stored.params.len() != 1 || stored.params[0] != TRANSFORM_INPUT {
            return None;
        }
        // The transform's signature, `<(oa):o>`, takes one argument.
        if args.len() > 1 {
            return Some(Err(EvaluatorError::TypeError(
                "T0410: Argument 2 of function undefined does not match function signature"
                    .to_string(),
            )));
        }
        let input = args.first().cloned().unwrap_or(JValue::Undefined);
        self.context.push_scope_defined_at(&stored.def_scope);
        for (name, value) in &stored.captured_env {
            self.context.bind(name.clone(), value.clone());
        }
        let result = self.with_dollar(&input, |ev| {
            ev.execute_transform(location, update, delete.as_deref(), &input)
        });
        self.context.pop_scope();
        Some(result)
    }

    /// The field names a transform's delete clause gives for one matched
    /// object, evaluated with `$` bound to it, as jsonata-js does: a string
    /// or an array of strings, or undefined for none.
    fn transform_deletions(
        &mut self,
        delete: &AstNode,
        matched: &JValue,
    ) -> Result<Vec<String>, EvaluatorError> {
        let deletions = self.with_dollar(matched, |ev| ev.evaluate_internal(delete, matched))?;
        let not_strings = || {
            EvaluatorError::EvaluationError(
                "T2012: The third argument of the transform operator must be an array of strings"
                    .to_string(),
            )
        };
        match deletions {
            JValue::Undefined => Ok(Vec::new()),
            JValue::String(s) => Ok(vec![s.to_string()]),
            JValue::Array(arr) => arr
                .iter()
                .map(|v| match v {
                    JValue::String(s) => Ok(s.to_string()),
                    _ => Err(not_strings()),
                })
                .collect(),
            _ => Err(not_strings()),
        }
    }

    /// Execute a transform operator on `input`, following jsonata-js
    /// `evaluateTransformExpression`.
    fn execute_transform(
        &mut self,
        location: &AstNode,
        update: &AstNode,
        delete: Option<&AstNode>,
        input: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        // jsonata-js compiles the transform into a function with signature
        // `<(oa):o>` that returns undefined for an undefined input.
        if input.is_undefined() {
            return Ok(JValue::Undefined);
        }
        #[cfg(feature = "python")]
        let is_lazy = matches!(input, JValue::LazyPyDict(_));
        #[cfg(not(feature = "python"))]
        let is_lazy = false;
        if !is_lazy && !matches!(input, JValue::Object(_) | JValue::Array(_)) {
            return Err(EvaluatorError::TypeError(
                "T0410: Argument 1 of function undefined does not match function signature"
                    .to_string(),
            ));
        }

        // Evaluate location expression on the input to get objects to transform
        let located_objects = self.evaluate_internal(location, input)?;

        // Collect target objects into a vector for comparison
        let targets: Vec<JValue> = match located_objects {
            JValue::Array(arr) => arr.to_vec(),
            JValue::Object(_) => vec![located_objects],
            #[cfg(feature = "python")]
            JValue::LazyPyDict(_) => vec![located_objects],
            JValue::Null => Vec::new(),
            other => vec![other],
        };

        // Recursive helper to apply transformation throughout the structure
        fn apply_transform_deep(
            evaluator: &mut Evaluator,
            value: &JValue,
            targets: &[JValue],
            update: &AstNode,
            delete: Option<&AstNode>,
        ) -> Result<JValue, EvaluatorError> {
            // Check if this value is one of the targets to transform
            // Use JValue's PartialEq for semantic equality comparison
            if targets.iter().any(|t| t == value) {
                // Transform this object
                let value = &normalize_lazy(value)?;
                if let JValue::Object(map_rc) = value.clone() {
                    let mut map = (*map_rc).clone();
                    // As in jsonata-js, `$` in the update is the matched
                    // object, not the transform's input.
                    let update_val =
                        evaluator.with_dollar(value, |ev| ev.evaluate_internal(update, value))?;
                    // Validate that update evaluates to an object or null (undefined)
                    match update_val {
                        JValue::Object(update_map) => {
                            for (key, val) in update_map.iter() {
                                map.insert(key.clone(), val.clone());
                            }
                        }
                        JValue::Null | JValue::Undefined => {
                            // Null/undefined means no updates, just continue to deletions
                        }
                        _ => {
                            return Err(EvaluatorError::EvaluationError(
                                "T2011: The second argument of the transform operator must evaluate to an object".to_string()
                            ));
                        }
                    }
                    if let Some(delete) = delete {
                        for field in evaluator.transform_deletions(delete, value)? {
                            map.shift_remove(&field);
                        }
                    }
                    return Ok(JValue::Object(Rc::new(map)));
                }
                return Ok(value.clone());
            }

            // Otherwise, recursively process children to find and transform targets
            match value {
                JValue::Object(map) => {
                    let mut new_map = IndexMap::new();
                    for (k, v) in map.iter() {
                        new_map.insert(
                            k.clone(),
                            apply_transform_deep(evaluator, v, targets, update, delete)?,
                        );
                    }
                    Ok(JValue::object(new_map))
                }
                #[cfg(feature = "python")]
                JValue::LazyPyDict(lazy) => {
                    let obj = JValue::Object(lazy.to_object().map_err(EvaluatorError::from)?);
                    apply_transform_deep(evaluator, &obj, targets, update, delete)
                }
                JValue::Array(arr) => {
                    let mut new_arr = Vec::new();
                    for item in arr.iter() {
                        new_arr.push(apply_transform_deep(
                            evaluator, item, targets, update, delete,
                        )?);
                    }
                    Ok(JValue::array(new_arr))
                }
                _ => Ok(value.clone()),
            }
        }

        // Apply transformation recursively starting from input
        apply_transform_deep(self, input, &targets, update, delete)
    }

    /// Helper to invoke a lambda with given parameters
    fn invoke_lambda(
        &mut self,
        params: &[String],
        body: &AstNode,
        signature: Option<&String>,
        values: &[JValue],
        data: &JValue,
        thunk: bool,
    ) -> Result<JValue, EvaluatorError> {
        self.invoke_lambda_with_env(
            params, body, signature, values, data, None, None, thunk, None,
        )
    }

    /// Invoke a lambda with optional captured environment (for closures).
    ///
    /// `self_rc` is the shared closure handle when the caller is invoking a
    /// `StoredLambda` value; if that closure has a `self_name`, the name is
    /// bound to the closure itself for the call (late-bound letrec).
    #[allow(clippy::too_many_arguments)]
    fn invoke_lambda_with_env(
        &mut self,
        params: &[String],
        body: &AstNode,
        signature: Option<&String>,
        values: &[JValue],
        data: &JValue,
        captured_env: Option<&HashMap<String, JValue>>,
        captured_data: Option<&JValue>,
        thunk: bool,
        self_rc: Option<&Rc<StoredLambda>>,
    ) -> Result<JValue, EvaluatorError> {
        let mode = std::mem::take(&mut self.callback_apply);
        // If this is a thunk (has tail calls), use TCO trampoline
        if thunk {
            let stored = match self_rc {
                Some(rc) => Rc::clone(rc),
                None => Rc::new(StoredLambda {
                    params: params.to_vec(),
                    body: body.clone(),
                    compiled_body: None, // Thunks use TCO, not the compiled fast path
                    signature: signature.cloned(),
                    captured_env: captured_env.cloned().unwrap_or_default(),
                    captured_data: captured_data.cloned(),
                    thunk,
                    self_name: None,
                    partial: None,
                    live_token: LiveLambdaToken::new(),
                    // An inline function, defined where it is called.
                    def_scope: self.context.def_scope(),
                }),
            };
            // jsonata-js's `apply` input: null for a callback, the input
            // the function was defined with when applied raw, else the
            // caller's input.
            let apply_input = match mode {
                ApplyMode::Callback => JValue::Null,
                ApplyMode::Raw => captured_data.cloned().unwrap_or_else(|| data.clone()),
                ApplyMode::Direct => data.clone(),
            };
            return self.invoke_lambda_with_tco(&stored, values, data, apply_input);
        }

        // Validate signature if present, and get coerced arguments
        // Push a new scope for this lambda invocation. A stored function's
        // body resolves free names where it was defined, not in the caller's
        // scopes (#191); an inline one is defined right here.
        match self_rc {
            Some(rc) => self.context.push_scope_defined_at(&rc.def_scope),
            None => self.context.push_scope(),
        }

        // First apply captured environment (for closures)
        if let Some(env) = captured_env {
            for (name, value) in env {
                self.context.bind(name.clone(), value.clone());
            }
        }

        // Late-bound self-reference: shadows any captured binding of the same
        // name (a rebinding `$f := function(){ ...$f()... }` must recurse into
        // itself, not the captured previous $f); parameters bind after this
        // and shadow it in turn.
        if let Some(rc) = self_rc {
            if let Some(self_name) = &rc.self_name {
                self.context
                    .bind(self_name.clone(), JValue::Lambda(Rc::clone(rc)));
            }
        }

        if let Some(sig_str) = signature {
            // Validate and coerce arguments with signature
            let coerced_values = match coerce_lambda_args(sig_str, values, data) {
                Ok(v) => v,
                Err(e) => {
                    self.context.pop_scope();
                    return Err(e);
                }
            };
            // Bind coerced values to params
            for (i, param) in params.iter().enumerate() {
                let value = coerced_values.get(i).cloned().unwrap_or(JValue::Undefined);
                self.context.bind(param.clone(), value);
            }
        } else {
            // No signature - bind directly from values slice (no allocation)
            for (i, param) in params.iter().enumerate() {
                let value = values.get(i).cloned().unwrap_or(JValue::Undefined);
                self.context.bind(param.clone(), value);
            }
        }

        // Evaluate lambda body (normal case)
        // Use captured_data for lexical scoping if available, otherwise use call-site data
        let body_data = captured_data.unwrap_or(data);
        let result = self.evaluate_internal(body, body_data);
        // A returned closure carries its own storage; popping is unconditional.
        self.context.pop_scope();
        result
    }

    /// Invoke a lambda with tail call optimization using a trampoline
    /// This method uses an iterative loop to handle tail-recursive calls without
    /// growing the stack, enabling deep recursion for tail-recursive functions.
    fn invoke_lambda_with_tco(
        &mut self,
        stored_lambda: &Rc<StoredLambda>,
        initial_args: &[JValue],
        data: &JValue,
        apply_input: JValue,
    ) -> Result<JValue, EvaluatorError> {
        // A callback of a builtin run as a tail call starts its chain where
        // that builtin's did; nothing it calls in turn does.
        let height = self.context.scope_stack.len();
        let base = match self.context.callback_base {
            Some((base, at)) if at == height => base,
            _ => height - 1,
        };
        let outer_base = std::mem::replace(&mut self.context.tail_base, base);
        // Every builtin the chain calls in tail position runs with the input
        // this application was given, as jsonata-js's trampoline applies
        // each with `apply`'s own input.
        let outer_input = self.tail_apply_input.replace(apply_input);
        let result = self.run_tail_calls(stored_lambda, initial_args, data);
        self.tail_apply_input = outer_input;
        self.context.tail_base = outer_base;
        result
    }

    /// The trampoline of `invoke_lambda_with_tco`.
    fn run_tail_calls(
        &mut self,
        stored_lambda: &Rc<StoredLambda>,
        initial_args: &[JValue],
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        let mut current_lambda = Rc::clone(stored_lambda);
        let mut current_args = initial_args.to_vec();
        let mut current_data = data.clone();

        // Push a persistent scope for the TCO trampoline loop.
        // This scope persists across all iterations so that lambdas defined
        // in one iteration (like recursive $iter) remain available in subsequent ones.
        // Like any call's frame, it looks past the caller's scopes to the
        // function's defining one (re-pointed per iteration below).
        self.context.push_tail_call_scope(&stored_lambda.def_scope);
        // Frames this trampoline has pushed (see `Context::enter_tail_call`).
        let mut frames = 1;
        let mut first = true;

        // Trampoline loop - keeps evaluating until we get a final value
        let result = loop {
            // No iteration cap: a tail call reuses this loop, not the stack,
            // so like jsonata-js's trampoline (`apply` in jsonata.js) a tail
            // loop runs as long as it keeps calling (GitHub issue #251). It
            // used to stop at 100,000 iterations with a U1001 naming a depth
            // of 500 that nothing enforced. A runaway tail loop is stopped by
            // the configured timeout (D1012), checked on every pass.
            if let Err(e) = check_loop_timeout(&self.options, self.start_time) {
                for _ in 0..frames {
                    self.context.pop_scope();
                }
                return Err(e);
            }
            if !first {
                self.context
                    .enter_tail_call(&current_lambda.def_scope, &mut frames);
            }
            first = false;

            // Evaluate the lambda body within the trampoline's frame
            let result =
                self.invoke_lambda_body_for_tco(&current_lambda, &current_args, &current_data)?;

            match result {
                LambdaResult::JValue(v) => break v,
                LambdaResult::TailCall { lambda, args, data } => {
                    // Continue with the tail call - no stack growth
                    current_lambda = lambda;
                    current_args = args;
                    current_data = data;
                }
            }
        };

        // Pop the trampoline's frames; escaping closures carry their own storage.
        for _ in 0..frames {
            self.context.pop_scope();
        }

        Ok(result)
    }

    /// Evaluate a lambda body, detecting tail calls for TCO
    /// Returns either a final value or a tail call continuation.
    /// NOTE: Does not push/pop its own scope - the caller (invoke_lambda_with_tco)
    /// manages the persistent scope for the trampoline loop.
    fn invoke_lambda_body_for_tco(
        &mut self,
        lambda: &Rc<StoredLambda>,
        values: &[JValue],
        data: &JValue,
    ) -> Result<LambdaResult, EvaluatorError> {
        if let Some(result) = self.invoke_transform(lambda, values) {
            return result.map(LambdaResult::JValue);
        }
        // Validate signature if present
        let coerced_values = if let Some(sig_str) = &lambda.signature {
            coerce_lambda_args(sig_str, values, data)?
        } else {
            values.to_vec()
        };

        // Bind directly into the trampoline's frame (managed by
        // invoke_lambda_with_tco), defined at the function's scope.
        // Apply captured environment
        for (name, value) in &lambda.captured_env {
            self.context.bind(name.clone(), value.clone());
        }

        // Late-bound self-reference (shadows a captured binding of the same
        // name; parameters shadow it in turn)
        if let Some(self_name) = &lambda.self_name {
            self.context
                .bind(self_name.clone(), JValue::Lambda(Rc::clone(lambda)));
        }

        // Bind parameters
        for (i, param) in lambda.params.iter().enumerate() {
            let value = coerced_values.get(i).cloned().unwrap_or(JValue::Undefined);
            self.context.bind(param.clone(), value);
        }

        // Evaluate the body with tail call detection
        let body_data = lambda.captured_data.as_ref().unwrap_or(data);
        self.evaluate_for_tco(&lambda.body, body_data)
    }

    /// Evaluate an expression for TCO, detecting tail calls
    /// Returns LambdaResult::TailCall if the expression is a function call to a user lambda
    /// The input a builtin called in tail position runs with: the running
    /// trampoline's `apply` input (see `tail_apply_input`), or `data`.
    fn tail_input(&self, data: &JValue) -> JValue {
        self.tail_apply_input
            .clone()
            .unwrap_or_else(|| data.clone())
    }

    fn evaluate_for_tco(
        &mut self,
        node: &AstNode,
        data: &JValue,
    ) -> Result<LambdaResult, EvaluatorError> {
        match node {
            // Conditional: evaluate condition, then evaluate the chosen branch for TCO
            AstNode::Conditional {
                condition,
                then_branch,
                else_branch,
            } => {
                let cond_value = self.evaluate_internal(condition, data)?;
                let is_truthy = self.is_truthy(&cond_value);

                if is_truthy {
                    self.evaluate_for_tco(then_branch, data)
                } else if let Some(else_expr) = else_branch {
                    self.evaluate_for_tco(else_expr, data)
                } else {
                    // No else branch: undefined, as everywhere else, so a
                    // `$map` callback adds nothing for it (not a null).
                    Ok(LambdaResult::JValue(JValue::Undefined))
                }
            }

            // Block: evaluate all but last normally, last for TCO
            AstNode::Block(exprs) => {
                if exprs.is_empty() {
                    return Ok(LambdaResult::JValue(JValue::Undefined));
                }

                // Evaluate all expressions except the last
                let mut result = JValue::Null;
                for (i, expr) in exprs.iter().enumerate() {
                    if i == exprs.len() - 1 {
                        // Last expression - evaluate for TCO
                        return self.evaluate_for_tco(expr, data);
                    } else {
                        result = self.evaluate_internal(expr, data)?;
                    }
                }
                Ok(LambdaResult::JValue(result))
            }

            // Variable binding: evaluate value, bind, then evaluate result for TCO if present
            AstNode::Binary {
                op: BinaryOp::ColonEqual,
                lhs,
                rhs,
            } => {
                // This is var := value; get the variable name
                let var_name = match lhs.as_ref() {
                    AstNode::Variable(name) => name.clone(),
                    _ => {
                        // Not a simple variable binding, evaluate normally
                        let result = self.evaluate_internal(node, data)?;
                        return Ok(LambdaResult::JValue(result));
                    }
                };

                // Check if RHS is a lambda - store it specially
                if let AstNode::Lambda {
                    params,
                    body,
                    signature,
                    thunk,
                } = rhs.as_ref()
                {
                    let captured_env = self.capture_environment_for(body, params);
                    let stored_lambda = StoredLambda {
                        params: params.clone(),
                        body: (**body).clone(),
                        compiled_body: CompiledBody::compile(body, params, signature, *thunk),
                        signature: signature.clone(),
                        captured_env,
                        captured_data: Some(data.clone()),
                        thunk: *thunk,
                        self_name: Some(var_name.clone()),
                        partial: None,
                        live_token: LiveLambdaToken::new(),
                        def_scope: self.context.def_scope(),
                    };
                    let lambda_value = JValue::Lambda(Rc::new(stored_lambda));
                    self.context.bind(var_name, lambda_value.clone());
                    return Ok(LambdaResult::JValue(lambda_value));
                }

                // Evaluate the RHS
                let value = self.evaluate_internal(rhs, data)?;
                self.context.bind(var_name, value.clone());
                Ok(LambdaResult::JValue(value))
            }

            // Function call - this is where TCO happens
            AstNode::Function { name, args, .. } => {
                // A `?` argument makes a partial application, not a call: the
                // non-TCO path builds it.
                let has_placeholder = args.iter().any(|a| matches!(a, AstNode::Placeholder));
                // A call to a lambda value (user function), or `$eval` as a
                // tail call: jsonata-js makes that a thunk too, which its
                // trampoline applies in the environment it was entered from,
                // so it reads and binds there, not in this body's scope.
                let (tail_lambda, tail_eval) = if has_placeholder {
                    (None, false)
                } else {
                    match self.context.lookup(name) {
                        Some(JValue::Lambda(rc)) if rc.thunk => (Some(Rc::clone(rc)), false),
                        Some(JValue::Builtin { name: b }) => (None, &**b == "eval"),
                        None => (None, name == "eval"),
                        Some(_) => (None, false),
                    }
                };
                if let Some(stored_lambda) = tail_lambda {
                    let mut evaluated_args = Vec::with_capacity(args.len());
                    for arg in args {
                        evaluated_args.push(self.evaluate_internal(arg, data)?);
                    }
                    if Self::composition_parts(&stored_lambda).is_some() {
                        return self.with_checkpoint(|ev| {
                            ev.tail_call_composition(stored_lambda, evaluated_args, data)
                        });
                    }
                    return Ok(LambdaResult::TailCall {
                        lambda: stored_lambda,
                        args: evaluated_args,
                        data: data.clone(),
                    });
                }
                // A builtin given a function with tail calls, as a tail call:
                // jsonata-js passes it the function unwrapped, and applies it
                // in the trampoline's environment, so that function's own
                // chain of tail calls begins there. Not `$sort`: jsonata-js
                // 2.2.2 fails such a call with "comp is not a function", so
                // there is no chain to match, and the usual path keeps the
                // fast path for an inline comparator.
                if matches!(
                    name.as_str(),
                    "map" | "filter" | "reduce" | "single" | "sift" | "each" | "match" | "replace"
                ) && !has_placeholder
                    && self.context.lookup(name).is_none()
                    && self.builtin_unshadowed(name, None)
                    && self.has_thunk_function_arg(args)
                {
                    let mut evaluated_args = Vec::with_capacity(args.len());
                    for arg in args {
                        evaluated_args.push(self.evaluate_internal(arg, data)?);
                    }
                    // It applies them over the frame of its value arguments.
                    let height = self.context.scope_stack.len() + 1;
                    let outer = self
                        .context
                        .callback_base
                        .replace((self.context.tail_base, height));
                    let input = self.tail_input(data);
                    let result = self.with_checkpoint(|ev| {
                        ev.as_raw_hof(args, &evaluated_args, |ev| {
                            ev.call_builtin_with_values(name, &evaluated_args, &input)
                        })
                    });
                    self.context.callback_base = outer;
                    return Ok(LambdaResult::JValue(result?));
                }
                if tail_eval && self.builtin_unshadowed("eval", None) {
                    let mut evaluated_args = Vec::with_capacity(args.len());
                    for arg in args {
                        evaluated_args.push(self.evaluate_internal(arg, data)?);
                    }
                    let at = self.context.tail_base;
                    let input = self.tail_input(data);
                    let result = self.eval_from_values_at(&evaluated_args, &input, Some(at))?;
                    return Ok(LambdaResult::JValue(result));
                }
                // A builtin that would read the context (its signature's `-`
                // parameter left without an argument) reads the trampoline's
                // input, not this body's, as jsonata-js's `applyInner` passes
                // `apply`'s input to `validateArguments` and as `this.input`.
                if !has_placeholder
                    && self.tail_apply_input.is_some()
                    && self.context.lookup(name).is_none()
                    && self.builtin_unshadowed(name, None)
                    && crate::builtins::builtin_may_read_context(name, args.len())
                {
                    let mut evaluated_args = Vec::with_capacity(args.len());
                    for arg in args {
                        evaluated_args.push(self.evaluate_internal(arg, data)?);
                    }
                    let input = self.tail_input(data);
                    let result = self.with_checkpoint(|ev| {
                        ev.as_raw_hof(args, &evaluated_args, |ev| {
                            ev.call_builtin_with_values(name, &evaluated_args, &input)
                        })
                    })?;
                    return Ok(LambdaResult::JValue(result));
                }
                // Not a thunk lambda: a builtin (or a function without tail
                // calls). jsonata-js runs it from its trampoline with
                // `applyInner`, whose result is never passed through
                // `evaluate`'s generic unwrap, so the function returns the
                // builtin's raw result: `$keys({})` is an empty sequence, not
                // undefined, and `$map([1,2], function(){ $keys({}) })` is
                // `[[], []]` (#250). A direct call `$f()` still unwraps it,
                // as its own call node does.
                // A builtin applies the functions it is given raw here (see
                // `ApplyMode::Raw`).
                let result = if !has_placeholder
                    && is_hof_builtin(name)
                    && self.context.lookup(name).is_none()
                    && self.builtin_unshadowed(name, None)
                {
                    self.as_raw_hof(args, &[], |ev| ev.evaluate_unwrapped_later(node, data))?
                } else {
                    self.evaluate_unwrapped_later(node, data)?
                };
                Ok(LambdaResult::JValue(result))
            }

            // Call node (calling a lambda value)
            AstNode::Call { procedure, args } => {
                // A `?` argument makes a partial application, not a call: the
                // non-TCO path builds it.
                if args.iter().any(|a| matches!(a, AstNode::Placeholder)) {
                    let result = self.evaluate_internal(node, data)?;
                    return Ok(LambdaResult::JValue(result));
                }
                // Evaluate the procedure to get the callable
                let callable = self.evaluate_internal(procedure, data)?;

                // Check if it's a lambda with TCO
                if let JValue::Lambda(stored_lambda) = &callable {
                    if stored_lambda.thunk {
                        let stored_lambda = Rc::clone(stored_lambda);
                        let mut evaluated_args = Vec::with_capacity(args.len());
                        for arg in args {
                            evaluated_args.push(self.evaluate_internal(arg, data)?);
                        }
                        if Self::composition_parts(&stored_lambda).is_some() {
                            return self.with_checkpoint(|ev| {
                                ev.tail_call_composition(stored_lambda, evaluated_args, data)
                            });
                        }
                        return Ok(LambdaResult::TailCall {
                            lambda: stored_lambda,
                            args: evaluated_args,
                            data: data.clone(),
                        });
                    }
                }
                // `($eval)(...)` as a tail call, as `$eval(...)` above.
                if matches!(&callable, JValue::Builtin { name } if &**name == "eval")
                    && self.builtin_unshadowed("eval", None)
                {
                    let mut evaluated_args = Vec::with_capacity(args.len());
                    for arg in args {
                        evaluated_args.push(self.evaluate_internal(arg, data)?);
                    }
                    let at = self.context.tail_base;
                    let input = self.tail_input(data);
                    let result = self.eval_from_values_at(&evaluated_args, &input, Some(at))?;
                    return Ok(LambdaResult::JValue(result));
                }
                // Not a thunk - apply the value already evaluated, so the
                // callee is not evaluated a second time. A builtin runs with
                // the trampoline's input, as above. This is the one way out of
                // the trampoline body into a call that does not pass through
                // `evaluate_internal`: a partial application (`$f($n-1, ?)()`)
                // or a builtin value (`($map)(xs, $f)`) applied here can call
                // straight back into a thunk lambda's trampoline. Without the
                // checkpoint that recursion grew the native stack with no depth
                // or stack guard and crashed the process at about 2,000 levels
                // (#255).
                let result = self.with_checkpoint(|ev| {
                    if matches!(callable, JValue::Builtin { .. }) {
                        let input = ev.tail_input(data);
                        if args.iter().any(|a| matches!(a, AstNode::Placeholder)) {
                            ev.apply_call_value(callable, procedure, args, data)
                        } else {
                            let mut evaluated_args = Vec::with_capacity(args.len());
                            for arg in args {
                                evaluated_args.push(ev.evaluate_internal(arg, data)?);
                            }
                            ev.call_function_value(&callable, &evaluated_args, &input)
                        }
                    } else {
                        ev.apply_call_value(callable, procedure, args, data)
                    }
                })?;
                Ok(LambdaResult::JValue(result))
            }

            // Variable reference that might be a function call
            // This handles cases like $f($x) where $f is referenced by name
            AstNode::Variable(_) => {
                let result = self.evaluate_internal(node, data)?;
                Ok(LambdaResult::JValue(result))
            }

            // Any other expression - evaluate normally
            _ => {
                let result = self.evaluate_internal(node, data)?;
                Ok(LambdaResult::JValue(result))
            }
        }
    }

    /// Apply the value of a called expression to `args`, as jsonata-js
    /// `evaluateFunction` does once it has the procedure's value.
    fn apply_call_value(
        &mut self,
        callable: JValue,
        procedure: &AstNode,
        args: &[AstNode],
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        if args.iter().any(|a| matches!(a, AstNode::Placeholder)) {
            let target = match &callable {
                JValue::Lambda(stored) => PartialTarget::Lambda(Rc::clone(stored)),
                JValue::Builtin { name } => PartialTarget::Builtin(name.to_string()),
                _ => {
                    return Err(EvaluatorError::EvaluationError(
                        "T1008: Attempted to partially apply a non-function".to_string(),
                    ))
                }
            };
            return self.build_partial_application(target, args, data);
        }
        self.check_callable(&callable, procedure)?;
        let mut evaluated_args = Vec::with_capacity(args.len());
        for arg in args.iter() {
            evaluated_args.push(self.evaluate_internal(arg, data)?);
        }
        self.call_function_value(&callable, &evaluated_args, data)
    }

    /// The error for calling `callable`, the value of `procedure`, when it
    /// is not a function: T1006, or T1005 as jsonata-js `evaluateFunction`
    /// raises it.
    fn check_callable(&self, callable: &JValue, procedure: &AstNode) -> Result<(), EvaluatorError> {
        if callable.is_function() {
            return Ok(());
        }
        // A procedure path that gives nothing, and whose first step names
        // something bound in the environment, was probably missing its `$`:
        // T1005 with the suggestion, as for `uppercase#$i()` or
        // `uppercase[0]()`.
        if callable.is_undefined() {
            if let Some(name) = Self::procedure_first_name(procedure) {
                if self.name_is_bound_js_truthy(name) {
                    return Err(EvaluatorError::ReferenceError(format!(
                        "T1005: Attempted to invoke a non-function. Did you mean ${}?",
                        name
                    )));
                }
            }
        }
        Err(EvaluatorError::TypeError(
            "T1006: Attempted to invoke a non-function".to_string(),
        ))
    }

    /// The name of a called path's first step, which jsonata-js looks up
    /// for its T1005 hint: `uppercase` in `uppercase#$i()` and in
    /// `a.uppercase[0]()`, whose call is the path step `uppercase[0]()`.
    fn procedure_first_name(procedure: &AstNode) -> Option<&str> {
        match procedure {
            AstNode::Path { steps } => match &steps.first()?.node {
                AstNode::Name(name) => Some(name),
                _ => None,
            },
            AstNode::Binary {
                op: BinaryOp::IndexBind | BinaryOp::FocusBind,
                lhs,
                ..
            } => Self::procedure_first_name(lhs),
            _ => None,
        }
    }

    /// Whether jsonata-js's `environment.lookup(name)` gives a JavaScript
    /// truthy value: a builtin or host function not shadowed by a binding,
    /// or a binding holding anything but undefined, null, false, 0, NaN or "".
    fn name_is_bound_js_truthy(&self, name: &str) -> bool {
        match self.context.lookup(name) {
            Some(JValue::Undefined | JValue::Null) => false,
            Some(JValue::Bool(b)) => *b,
            Some(JValue::Number(n)) => *n != 0.0 && !n.is_nan(),
            Some(JValue::String(s)) => !s.is_empty(),
            Some(_) => true,
            None => self.is_builtin_function(name) || self.host_fns.contains_key(name),
        }
    }

    /// Call a function value, a lambda, a builtin or a regex, with `values`.
    /// `function` must be one of the three.
    fn call_function_value(
        &mut self,
        function: &JValue,
        values: &[JValue],
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        match function {
            JValue::Lambda(stored) => self.invoke_stored_lambda(stored, values, data),
            JValue::Builtin { name } => self.call_builtin_with_values(name, values, data),
            JValue::Regex { pattern, flags } => {
                let text = values.first().and_then(regex_subject);
                let from = values.get(1).map_or(0, regex_from_index);
                Self::call_regex(function, pattern, flags, text.as_deref(), from, true)
            }
            other => Err(EvaluatorError::TypeError(format!(
                "T1006: Attempted to invoke a non-function: {other:?}"
            ))),
        }
    }

    /// Call the function a regex literal evaluates to, following jsonata-js
    /// `evaluateRegex`: the first match in `text` at or after `from` (in
    /// UTF-16 units) as `{match, start, end, groups, next}`, or undefined.
    /// `next()` gives the match after this one, undefined once this one
    /// ends the string, and D1004 for a zero-length one, which would never
    /// move on. `regex` is the regex value itself, which `next` carries.
    ///
    /// Deliberate divergence: jsonata-js matches an undefined subject as the
    /// text "undefined" (`nope ~> /n/` matches); here it gives undefined.
    fn call_regex(
        regex: &JValue,
        pattern: &str,
        flags: &str,
        text: Option<&str>,
        from: usize,
        first: bool,
    ) -> Result<JValue, EvaluatorError> {
        let Some(text) = text else {
            return Ok(JValue::Undefined);
        };
        let re = crate::functions::string::build_regex(pattern, flags)?;
        let Some(mut matches) = crate::functions::string::RegexMatches::new(&re, text)
            .counting_utf16()
            .starting_at(from, first)
        else {
            return Ok(JValue::Undefined);
        };
        let Some(found) = matches.next_match()? else {
            return Ok(JValue::Undefined);
        };
        let (_, end) = found.utf16_span.expect("counting UTF-16 offsets");
        let mut result = match found.to_object() {
            JValue::Object(result) => result,
            other => return Ok(other),
        };
        // `next` is `λ() { REGEX_NEXT(regex, text, end) }`, its arguments
        // captured like a composition's functions (see `compose_functions`).
        // The names hold spaces, so expressions can't see them.
        const NEXT: &str = REGEX_NEXT;
        const RE: &str = "regex re";
        const TEXT: &str = "regex text";
        const END: &str = "regex end";
        let next = JValue::Lambda(Rc::new(StoredLambda {
            params: Vec::new(),
            body: AstNode::Function {
                name: NEXT.to_string(),
                args: [RE, TEXT, END]
                    .iter()
                    .map(|name| AstNode::Variable(name.to_string()))
                    .collect(),
                is_builtin: false,
            },
            compiled_body: None,
            signature: None,
            captured_env: HashMap::from([
                (NEXT.to_string(), JValue::builtin(REGEX_NEXT)),
                (RE.to_string(), regex.clone()),
                (TEXT.to_string(), JValue::string(text)),
                (END.to_string(), JValue::from(end as i64)),
            ]),
            captured_data: None,
            thunk: false,
            self_name: None,
            partial: None,
            live_token: LiveLambdaToken::new(),
            // Its body names only what it captured.
            def_scope: DefScope::ROOT,
        }));
        Rc::make_mut(&mut result).insert("next".to_string(), next);
        Ok(JValue::Object(result))
    }

    /// The body of a regex match's `next()`: `values` are the regex, the
    /// text and where the match ended (see `call_regex`). Undefined once
    /// the match ended the text; D1004 for a zero-length next match.
    fn regex_next(values: &[JValue]) -> Result<JValue, EvaluatorError> {
        let (
            Some(regex @ JValue::Regex { pattern, flags }),
            Some(JValue::String(text)),
            Some(&JValue::Number(end)),
        ) = (values.first(), values.get(1), values.get(2))
        else {
            return Err(EvaluatorError::EvaluationError(
                "a regex match's next() takes its regex, text and end".to_string(),
            ));
        };
        Self::call_regex(regex, pattern, flags, Some(text), end as usize, false)
    }

    /// $match(str, matcher, limit) with a custom matcher function, following
    /// jsonata-js `match`. The matcher is called with the string and returns
    /// `{match, start, end, groups, next}`. Each `next()` gives the following
    /// match, until one is undefined. A limit of 0 doesn't call the matcher,
    /// and after each match the next one is asked for, and checked, before
    /// the limit is.
    fn match_with_custom_matcher(
        &mut self,
        str_value: &str,
        matcher: &JValue,
        limit: Option<usize>,
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        let mut results = Vec::new();
        if limit == Some(0) {
            return Ok(JValue::sequence(results));
        }
        let str_val = JValue::string(str_value.to_string());
        let mut current = normalize_lazy(
            &self.as_callback(|ev| ev.call_function_value(matcher, &[str_val], data))?,
        )?;
        loop {
            check_matcher_result(&current, "match")?;
            let JValue::Object(match_obj) = &current else {
                break;
            };
            let mut result_obj = IndexMap::with_capacity(3);
            for (from, to) in [("match", "match"), ("start", "index"), ("groups", "groups")] {
                if let Some(value) = match_obj.get(from) {
                    result_obj.insert(to.to_string(), value.clone());
                }
            }
            results.push(JValue::object(result_obj));

            // Only a lambda `next` is called (see `check_matcher_result`).
            let Some(JValue::Lambda(next)) = match_obj.get("next") else {
                break;
            };
            let next = Rc::clone(next);
            current = normalize_lazy(&self.invoke_stored_lambda(&next, &[], data)?)?;
            if limit.is_some_and(|limit| results.len() >= limit) {
                check_matcher_result(&current, "match")?;
                break;
            }
        }
        Ok(JValue::sequence(results))
    }

    /// $replace(str, pattern, replacement, limit), following jsonata-js
    /// `replace`. A substring pattern is replaced where it occurs; a regex or
    /// a custom matcher function is driven as jsonata-js's `evaluateMatcher`
    /// drives it, one match and then each `next()`. The replacement is either
    /// a string, in which `$0` and `$1`... stand for the match and its groups,
    /// or a function called with the match object, which must return a
    /// string (D3012).
    fn replace_matches(
        &mut self,
        s: &str,
        pattern: &JValue,
        replacement: &JValue,
        limit: Option<usize>,
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        match pattern {
            JValue::String(_) => match replacement {
                JValue::String(text) => Ok(crate::functions::string::replace(s, pattern, text, limit)?),
                // jsonata-js inserts the function's JavaScript source text
                // here, which has no equivalent to reproduce.
                _ => Err(EvaluatorError::TypeError(
                    "replace() pattern must be a regex or a function when the replacement is a function"
                        .to_string(),
                )),
            },
            JValue::Lambda(_) | JValue::Builtin { .. } => {
                self.replace_with_custom_matcher(s, pattern, replacement, limit, data)
            }
            _ => {
                use crate::functions::string::{replace_regex, RegexMatches};
                let (pat, flags) =
                    crate::functions::string::extract_regex(pattern).ok_or_else(|| {
                        EvaluatorError::TypeError(
                            "replace() pattern must be a string, a regex or a function".to_string(),
                        )
                    })?;
                let re = crate::functions::string::build_regex(&pat, &flags)?;
                let mut matches = RegexMatches::new(&re, s);
                // Only a replacement function sees the UTF-16 offsets.
                if !matches!(replacement, JValue::String(_)) {
                    matches = matches.counting_utf16();
                }
                let result = replace_regex(matches, limit, |found, out| {
                    self.replace_one(
                        out,
                        replacement,
                        found.text,
                        found.groups.len(),
                        |i| Ok(found.groups[i].map(Cow::Borrowed)),
                        || found.to_object(),
                        data,
                    )
                })?;
                Ok(JValue::string(result))
            }
        }
    }

    /// jsonata-js `replace`'s replacer applied to one match, appended to
    /// `out`: a replacement string with `$0`/`$n` substituted (`group(i)`
    /// gives group `i + 1` of `group_count`, see `substitute_capture_groups`),
    /// or a replacement function's result for `match_object`.
    fn replace_one<'g>(
        &mut self,
        out: &mut String,
        replacement: &JValue,
        text: &str,
        group_count: usize,
        group: impl FnMut(usize) -> Result<Option<Cow<'g, str>>, EvaluatorError>,
        match_object: impl FnOnce() -> JValue,
        data: &JValue,
    ) -> Result<(), EvaluatorError> {
        if let JValue::String(template) = replacement {
            return crate::functions::string::substitute_capture_groups(
                out,
                template,
                text,
                group_count,
                group,
            );
        }
        check_loop_timeout(&self.options, self.start_time)?;
        match self.as_callback(|ev| ev.call_function_value(replacement, &[match_object()], data))? {
            JValue::String(replaced) => {
                out.push_str(&replaced);
                Ok(())
            }
            _ => Err(EvaluatorError::TypeError(
                "D3012: Attempted to replace a matched string with a non-string value".to_string(),
            )),
        }
    }

    /// $replace(str, matcher, replacement, limit) with a custom matcher
    /// function, following jsonata-js `replace`. The matcher returns
    /// `{match, start, groups, next}` objects (checked as `evaluateMatcher`
    /// checks them), and a replacement function receives each one as it is.
    /// `start` counts UTF-16 code units, so the string is cut in those, and
    /// it and the groups a replacement string inserts are converted as
    /// JavaScript converts them. A result without a `next` function ends the
    /// matches, as does a missing `match` (taken as empty); jsonata-js
    /// crashes on both.
    fn replace_with_custom_matcher(
        &mut self,
        s: &str,
        matcher: &JValue,
        replacement: &JValue,
        limit: Option<usize>,
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        if limit == Some(0) {
            return Ok(JValue::string(s));
        }
        let units: Vec<u16> = s.encode_utf16().collect();
        let str_val = JValue::string(s);
        let mut current = normalize_lazy(
            &self.as_callback(|ev| ev.call_function_value(matcher, &[str_val], data))?,
        )?;
        check_matcher_result(&current, "replace")?;
        let mut result: Vec<u16> = Vec::with_capacity(units.len());
        let mut replaced = String::new();
        let mut position = 0.0;
        let mut count = 0;
        while let JValue::Object(found) = current.clone() {
            if limit.is_some_and(|limit| count >= limit) {
                break;
            }
            let start = found.get("start").unwrap_or(&JValue::Undefined);
            // `str.substring(position, matches.start)`: an undefined end is
            // the end of the string.
            let end = (!start.is_undefined()).then(|| js_to_number(start));
            result.extend_from_slice(js_substring(&units, position, end));
            let text = match found.get("match") {
                Some(JValue::String(text)) => text.clone(),
                _ => Rc::from(""),
            };
            // `substitute += submatch` converts a group the replacement
            // refers to into a string; an undefined one is skipped.
            let groups: &[JValue] = match found.get("groups") {
                Some(JValue::Array(groups)) => groups,
                _ => &[],
            };
            replaced.clear();
            self.replace_one(
                &mut replaced,
                replacement,
                &text,
                groups.len(),
                |i| match &groups[i] {
                    JValue::Undefined => Ok(None),
                    group => js_to_string(group).map(|text| Some(Cow::Owned(text))),
                },
                || current.clone(),
                data,
            )?;
            result.extend(replaced.encode_utf16());
            // `matches.start + matches.match.length`, which concatenates
            // when `start` converts to a string first.
            let match_len = text.encode_utf16().count() as f64;
            position = match start {
                JValue::String(_) | JValue::Array(_) => {
                    let primitive = js_to_string(start)?;
                    js_string_to_number(&format!("{primitive}{match_len}"))
                }
                start => js_to_number(start) + match_len,
            };
            count += 1;

            // Only a lambda `next` is called (see `check_matcher_result`).
            current = match found.get("next") {
                Some(JValue::Lambda(next)) => {
                    let next = Rc::clone(next);
                    normalize_lazy(&self.invoke_stored_lambda(&next, &[], data)?)?
                }
                _ => JValue::Undefined,
            };
            check_matcher_result(&current, "replace")?;
        }
        result.extend_from_slice(js_substring(&units, position, None));
        Ok(JValue::string(String::from_utf16_lossy(&result)))
    }

    /// THE builtin-shadowing rule for compiled code (#191). A compiled
    /// expression calls each builtin it names directly, which is what the
    /// tree-walker does only when `name`, resolved where the expression is
    /// evaluated, is unbound (`bound` is that resolution) and no host
    /// override replaces it -- or is bound to that same builtin. Any other
    /// binding, a function or not, shadows the builtin.
    fn builtin_unshadowed(&self, name: &str, bound: Option<&JValue>) -> bool {
        match bound {
            None => self.host_fns.is_empty() || !self.host_fns.contains_key(name),
            Some(JValue::Builtin { name: b }) => {
                &**b == name && (self.host_fns.is_empty() || !self.host_fns.contains_key(name))
            }
            Some(_) => false,
        }
    }

    /// Whether each of `names` still means the builtin right here, in the
    /// current scope.
    fn builtins_unshadowed_here(&self, names: &[&str]) -> bool {
        names
            .iter()
            .all(|name| self.builtin_unshadowed(name, self.context.lookup(name)))
    }

    /// Compile `node` to run right here, in the current scope: `None` if it
    /// does not compile or calls a builtin a binding here shadows. Every
    /// evaluator site that compiles part of an expression goes through this
    /// (or `CompiledBody` for function bodies, or `cached_step_body` for path
    /// steps), so none can skip the rule.
    fn compile_in_scope(
        &self,
        node: &AstNode,
        allowed_vars: Option<&[&str]>,
    ) -> Option<CompiledExpr> {
        let compiled = match allowed_vars {
            Some(vars) => try_compile_expr_with_allowed_vars(node, vars)?,
            None => try_compile_expr(node)?,
        };
        let mut names = Vec::new();
        compiled_builtin_names(&compiled, &mut names);
        self.builtins_unshadowed_here(&names).then_some(compiled)
    }

    /// The compiled body of an object constructor or `.(expr)` step, if it
    /// may run right here: compiled the first time this evaluator runs the
    /// step and kept by its id (#197), which is safe because compiling reads
    /// nothing but the node. A function's body is a clone of the steps in the
    /// expression, with the same ids, so all its calls share the entry.
    /// Whether the builtins it calls are shadowed depends on where the step
    /// runs (one call of a function may bind `$string`, the next not), so
    /// that is checked on every use, as `compile_in_scope` does.
    fn cached_step_body(&mut self, step: &PathStep) -> Option<Rc<CompiledBody>> {
        let body = match &step.node {
            AstNode::Object(_) => &step.node,
            AstNode::FunctionApplication(expr) => &**expr,
            _ => return None,
        };
        let compiled = self
            .step_bodies
            .get_or_compile(step.id.get(), || CompiledBody::compile_step(body))?;
        self.builtins_unshadowed_here(&compiled.builtins)
            .then_some(compiled)
    }

    /// A function's compiled body, if calling it now may take the fast path:
    /// each builtin it calls must be unshadowed where the function was
    /// defined -- by what it captured, its own name, or a binding made since
    /// in its defining scope -- as its body would resolve it.
    fn usable_compiled_body<'a>(&self, stored: &'a StoredLambda) -> Option<&'a CompiledExpr> {
        let body = stored.compiled_body.as_ref()?;
        let unshadowed = body.builtins.iter().all(|&name| {
            if stored.self_name.as_deref() == Some(name) {
                return false;
            }
            let bound = match stored.captured_env.get(name) {
                Some(v) => Some(v),
                None => self.context.lookup_defined_at(&stored.def_scope, name),
            };
            self.builtin_unshadowed(name, bound)
        });
        unshadowed.then_some(&body.expr)
    }

    /// Capture only the variables referenced by a lambda body (selective capture).
    /// This avoids cloning the entire environment when only a few variables are needed.
    fn capture_environment_for(
        &self,
        body: &AstNode,
        params: &[String],
    ) -> HashMap<String, JValue> {
        let free_vars = Self::collect_free_variables(body, params);
        if free_vars.is_empty() {
            return HashMap::new();
        }
        let mut result = HashMap::new();
        for var_name in &free_vars {
            if let Some(value) = self.context.lookup(var_name) {
                result.insert(var_name.clone(), value.clone());
            }
        }
        result
    }

    /// Collect all free variables in an AST node that are not bound by the given params.
    /// A "free variable" is one that is referenced but not defined within the expression.
    fn collect_free_variables(body: &AstNode, params: &[String]) -> HashSet<String> {
        let mut free_vars = HashSet::new();
        let bound: HashSet<&str> = params.iter().map(|s| s.as_str()).collect();
        Self::collect_free_vars_walk(body, &bound, &mut free_vars);
        free_vars
    }

    /// Runs on every lambda creation, possibly deep in a recursion with only
    /// `EVAL_RED_ZONE` of stack left, over a body that can be about 1000
    /// levels deep: each level moves to a new stack segment when the current
    /// one runs low, as `AstNode::clone` does.
    fn collect_free_vars_walk(node: &AstNode, bound: &HashSet<&str>, free: &mut HashSet<String>) {
        crate::value::with_stack(|| Self::collect_free_vars_node(node, bound, free))
    }

    fn collect_free_vars_node(node: &AstNode, bound: &HashSet<&str>, free: &mut HashSet<String>) {
        match node {
            AstNode::Variable(name) => {
                if !bound.contains(name.as_str()) {
                    free.insert(name.clone());
                }
            }
            AstNode::Function { name, args, .. } => {
                // Function name references a variable (e.g., $f(...))
                if !bound.contains(name.as_str()) {
                    free.insert(name.clone());
                }
                for arg in args {
                    Self::collect_free_vars_walk(arg, bound, free);
                }
            }
            AstNode::Lambda { params, body, .. } => {
                // Inner lambda introduces new bindings
                let mut inner_bound = bound.clone();
                for p in params {
                    inner_bound.insert(p.as_str());
                }
                Self::collect_free_vars_walk(body, &inner_bound, free);
            }
            AstNode::Binary { op, lhs, rhs } => {
                Self::collect_free_vars_walk(lhs, bound, free);
                Self::collect_free_vars_walk(rhs, bound, free);
                // For ColonEqual, note: the binding is visible after this expr in blocks,
                // but block handling takes care of that separately
                let _ = op;
            }
            AstNode::Unary { operand, .. } => {
                Self::collect_free_vars_walk(operand, bound, free);
            }
            AstNode::Path { steps } => {
                for step in steps {
                    Self::collect_free_vars_walk(&step.node, bound, free);
                    for stage in &step.stages {
                        match stage {
                            Stage::Filter(expr) => Self::collect_free_vars_walk(expr, bound, free),
                            // An index stage binds a variable; it introduces no
                            // free variable references, and `[]` carries no
                            // expression at all.
                            Stage::Index(_) | Stage::KeepArray => {}
                        }
                    }
                }
            }
            AstNode::Call { procedure, args } => {
                Self::collect_free_vars_walk(procedure, bound, free);
                for arg in args {
                    Self::collect_free_vars_walk(arg, bound, free);
                }
            }
            AstNode::Conditional {
                condition,
                then_branch,
                else_branch,
            } => {
                Self::collect_free_vars_walk(condition, bound, free);
                Self::collect_free_vars_walk(then_branch, bound, free);
                if let Some(else_expr) = else_branch {
                    Self::collect_free_vars_walk(else_expr, bound, free);
                }
            }
            AstNode::Block(exprs) => {
                let mut block_bound = bound.clone();
                for expr in exprs {
                    Self::collect_free_vars_walk(expr, &block_bound, free);
                    // Bindings introduced via := become bound for subsequent expressions
                    if let AstNode::Binary {
                        op: BinaryOp::ColonEqual,
                        lhs,
                        ..
                    } = expr
                    {
                        if let AstNode::Variable(var_name) = lhs.as_ref() {
                            block_bound.insert(var_name.as_str());
                        }
                    }
                }
            }
            AstNode::Array(exprs) | AstNode::ArrayGroup(exprs) => {
                for expr in exprs {
                    Self::collect_free_vars_walk(expr, bound, free);
                }
            }
            AstNode::Object(pairs) => {
                for (key, value) in pairs {
                    Self::collect_free_vars_walk(key, bound, free);
                    Self::collect_free_vars_walk(value, bound, free);
                }
            }
            AstNode::ObjectTransform { input, pattern } => {
                Self::collect_free_vars_walk(input, bound, free);
                for (key, value) in pattern {
                    Self::collect_free_vars_walk(key, bound, free);
                    Self::collect_free_vars_walk(value, bound, free);
                }
            }
            AstNode::FunctionApplication(expr) => {
                Self::collect_free_vars_walk(expr, bound, free);
            }
            AstNode::Sort { input, terms } => {
                Self::collect_free_vars_walk(input, bound, free);
                for (expr, _) in terms {
                    Self::collect_free_vars_walk(expr, bound, free);
                }
            }
            AstNode::Transform {
                location,
                update,
                delete,
            } => {
                Self::collect_free_vars_walk(location, bound, free);
                Self::collect_free_vars_walk(update, bound, free);
                if let Some(del) = delete {
                    Self::collect_free_vars_walk(del, bound, free);
                }
            }
            // Leaf nodes with no variable references
            AstNode::String(_)
            | AstNode::Name(_)
            | AstNode::Number(_)
            | AstNode::Boolean(_)
            | AstNode::Null
            | AstNode::Undefined
            | AstNode::Placeholder
            | AstNode::Regex { .. }
            | AstNode::Wildcard
            | AstNode::Descendant
            | AstNode::Parent(_)
            | AstNode::ParentVariable(_) => {}
        }
    }

    /// Whether `name` names a builtin or a host function, which a variable
    /// of that name gives as a function value unless it is bound.
    fn is_function_name(&self, name: &str) -> bool {
        self.is_builtin_function(name) || self.host_fns.contains_key(name)
    }

    /// Check if a name refers to a built-in function
    fn is_builtin_function(&self, name: &str) -> bool {
        matches!(
            name,
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
            "map" | "filter" | "reduce" | "single" |

            // Date/time functions
            "now" | "millis" | "fromMillis" | "toMillis"
        )
    }

    /// Call a built-in function directly with pre-evaluated Values: a
    /// builtin-valued variable called directly (`$f := $keys; $f(x)`), or a
    /// builtin passed to a higher-order function (`$map(arr, $keys)`). A
    /// sequence result is unwrapped once, by whichever expression boundary
    /// receives it, as in jsonata-js.
    fn call_builtin_with_values(
        &mut self,
        name: &str,
        values: &[JValue],
        context: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        // A builtin applied as a callback runs with a null input, as
        // jsonata-js's closure wrapper applies it.
        let context = if std::mem::take(&mut self.callback_apply) == ApplyMode::Callback {
            &JValue::Null
        } else {
            context
        };
        // Only a regex match's `next` holds this builtin (see `call_regex`).
        if name == REGEX_NEXT {
            return Self::regex_next(values);
        }
        // A host override applied in value position (e.g. `$f := $now; $f()`) or
        // passed to a higher-order function reaches dispatch here rather than
        // through `evaluate_function_call`. Check the registry first so the
        // override is honoured consistently in both positions, and before
        // `dispatch_pure` so a host override shadows a same-named builtin here
        // exactly as it does at the other two dispatch sites.
        if !self.host_fns.is_empty() {
            if let Some(f) = self.host_fns.get(name).cloned() {
                let mut ctx = HostCtx::new();
                return f.call(values, &mut ctx);
            }
        }

        // Every builtin that needs nothing but its arguments, the context,
        // and the evaluation options -- the full set this function used to
        // hand-implement 22 arms of -- is handled by the one shared
        // dispatcher the compiled path and the tree-walker also use. This
        // runs before any arity check: `dispatch_pure` does its own
        // signature-driven validation (including its own implicit-context
        // insertion for a zero-arg call like bare `$string`), and gating on
        // "at least 1 argument" first would wrongly reject `$zip`/`$millis`
        // (arity 0) reached by reference.
        if crate::builtins::is_pure_builtin(name) {
            return crate::builtins::dispatch_pure(name, values, context, &self.options);
        }

        // `$eval` is evaluator-dependent but its arguments are ordinary
        // values -- an expression string and an optional focus -- so it runs
        // from evaluated values like any other builtin. jsonata-js evaluates
        // it as a callback for the same reason: `$map(["1+1"], $eval)` is 2.
        if name == "eval" {
            return self.eval_from_values(values, context);
        }

        // The other nine ($map, $filter, $reduce, $single, $sift, $each,
        // $sort, $match, $replace) take AST arguments and call back into
        // evaluation. Called by value (`[3,1,2] ~> $sort`, `$f := $single`),
        // they run the builtin a direct `$sort(...)` runs, so they validate
        // their arguments the same way.
        self.with_value_args(values, |ev, args| {
            ev.evaluate_builtin_call(name, args, context)
        })
    }

    /// Run `call` with AST arguments that evaluate to `values`: temporary
    /// bindings in a scope of their own. This lets already-evaluated values
    /// reach the calls that take AST arguments.
    fn with_value_args(
        &mut self,
        values: &[JValue],
        call: impl FnOnce(&mut Self, &[AstNode]) -> Result<JValue, EvaluatorError>,
    ) -> Result<JValue, EvaluatorError> {
        self.context.push_internal_scope();
        let temp_args: Vec<AstNode> = values
            .iter()
            .enumerate()
            .map(|(i, value)| {
                let temp_name = format!("__temp_arg_{i}");
                self.context.bind(temp_name.clone(), value.clone());
                AstNode::Variable(temp_name)
            })
            .collect();
        let result = call(self, &temp_args);
        self.context.pop_scope();
        result
    }

    /// `$eval(expression [, focus])` -- parse and evaluate a JSONata
    /// expression at runtime.
    ///
    /// Split out of `evaluate_function_call` so the by-reference path can
    /// reach it too. `$eval` is the one evaluator-dependent builtin that
    /// genuinely works as a callback -- its second parameter is an
    /// arbitrary focus value rather than a function, so `$map(["1+1"],
    /// $eval)` is 2 (#140).
    fn eval_from_values(
        &mut self,
        args: &[JValue],
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        self.eval_from_values_at(args, data, None)
    }

    /// `$eval`, run in the caller's scope, or in frame `from` (as a tail
    /// call: see `evaluate_for_tco`).
    fn eval_from_values_at(
        &mut self,
        args: &[JValue],
        data: &JValue,
        from: Option<usize>,
    ) -> Result<JValue, EvaluatorError> {
        // $eval(expression [, context]) - parse and evaluate a JSONata expression at runtime
        if args.is_empty() || args.len() > 2 {
            return Err(EvaluatorError::EvaluationError(
                "T0410: Argument 1 of function $eval must be a string".to_string(),
            ));
        }

        // Undefined propagates -- `$eval(nothing)` is undefined. An explicit
        // null does NOT: the reference's signature is `<sx?:x>`, and `s`
        // admits missing but not null, so `$eval(null)` is T0410. The null
        // passthrough here predates that distinction; it fell through to the
        // non-string branch below on every other type already.

        if args[0].is_undefined() {
            return Ok(JValue::Undefined);
        }

        // First argument must be a string expression
        let expr_str = match &args[0] {
            JValue::String(s) => &**s,
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
        // A focus array is wrapped the way evaluate wraps its input. An
        // undefined focus is no focus at all: jsonata-js's `functionEval`
        // keeps the current input unless `typeof focus !== 'undefined'`, so
        // `$eval("a", $unbound)` reads `a` from the input (#247).
        let focus;
        let eval_context = if args.len() == 2 && !args[1].is_undefined() {
            focus = args[1].clone().into_outer_wrapper();
            &focus
        } else {
            data
        };

        // Evaluate the parsed expression
        // In the caller's scope, as jsonata-js evaluates in the calling
        // environment, so a `:=` there binds in it. When the evaluator's own
        // frames (argument temporaries, a composition) are on top, it runs
        // in a frame that looks past them, whose bindings then go to the
        // caller's frame, or nowhere when one of those frames stands for a
        // jsonata-js call frame of its own (a composition's first function).
        let (at, opaque) = self.context.frame_below_internal(from);
        let evaluated = if at == self.context.scope_stack.len() - 1 {
            self.evaluate_internal(&parsed_ast, eval_context)
        } else {
            self.context.push_scope_at(at);
            let evaluated = self.evaluate_internal(&parsed_ast, eval_context);
            if opaque {
                self.context.pop_scope();
            } else {
                self.context.pop_scope_into(at);
            }
            evaluated
        };
        match evaluated {
            Ok(result) => Ok(result),
            Err(e) => {
                // D3121 is the error code for evaluation errors in $eval
                let err_msg = e.to_string();
                // An unknown function inside $eval is D3121, not the T100x the
                // inner evaluation produced. Matched on the codes rather than
                // on the prose it used to say ("Unknown function: x"), which
                // is exactly the coupling that broke when those messages
                // gained their JSONata codes.
                if err_msg.starts_with("D3121")
                    || err_msg.contains("T1005")
                    || err_msg.contains("T1006")
                {
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

    /// Collect all descendant values recursively
    fn collect_descendants(&self, value: &JValue) -> Result<Vec<JValue>, EvaluatorError> {
        let mut descendants = Vec::new();

        match value {
            // A missing value has no descendants: Undefined used to fall
            // through to the catch-all and collect *itself*, so `**` with no
            // input produced `[null]` instead of undefined.
            //
            // An explicit null is NOT grouped with Undefined here (issue
            // #114): jsonata-js treats null as an ordinary primitive for
            // `**` just like a number or string -- it collects itself, both
            // as the whole result of `nul.**` and for a null value found
            // while descending into a structure (`{"a": {"b": null}}.**`
            // includes that `null` as one of the descendants). It falls
            // through to the primitive catch-all below rather than getting
            // its own arm.
            JValue::Undefined => {
                return Ok(descendants);
            }
            JValue::Object(obj) => {
                // Include the current object
                descendants.push(value.clone());

                for val in obj.values() {
                    // Recursively collect descendants
                    descendants.extend(with_stack(|| self.collect_descendants(val))?);
                }
            }
            #[cfg(feature = "python")]
            JValue::LazyPyDict(lazy) => {
                // Include the current (lazy) object, mirroring the Object arm
                descendants.push(value.clone());

                let obj = lazy.to_object().map_err(EvaluatorError::from)?;
                for val in obj.values() {
                    descendants.extend(with_stack(|| self.collect_descendants(val))?);
                }
            }
            JValue::Array(arr) => {
                // DO NOT include the array itself - only recurse into elements
                // This matches JavaScript behavior: arrays are traversed but not collected
                for val in arr.iter() {
                    // Recursively collect descendants
                    descendants.extend(with_stack(|| self.collect_descendants(val))?);
                }
            }
            _ => {
                // For primitives (string, number, boolean), just include the value itself
                descendants.push(value.clone());
            }
        }

        Ok(descendants)
    }

    /// Evaluate a sort term expression, distinguishing missing fields from explicit null
    /// Returns JValue::Undefined for missing fields, JValue::Null for explicit null
    fn evaluate_sort_term(
        &mut self,
        term_expr: &AstNode,
        element: &JValue,
        tuples: bool,
    ) -> Result<JValue, EvaluatorError> {
        // For tuples (from index binding), extract the actual value from @ field
        let actual_element = if let JValue::Object(obj) = element {
            if tuples && is_tuple_object(obj) {
                obj.get("@").cloned().unwrap_or(JValue::Null)
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
                    match &actual_element {
                        JValue::Object(obj) => {
                            return match obj.get(field_name) {
                                Some(val) => Ok(val.clone()),  // Field exists (may be null)
                                None => Ok(JValue::Undefined), // Field is missing
                            };
                        }
                        #[cfg(feature = "python")]
                        JValue::LazyPyDict(lazy) => {
                            return Ok(lazy.get_field(field_name)?);
                        }
                        // A field of an array is its items' fields, gathered
                        // as for any path; an explicit null among them stays
                        // null, which raises T2008 as in jsonata-js.
                        JValue::Array(_) => {
                            return self.evaluate_internal(term_expr, &actual_element);
                        }
                        _ => return Ok(JValue::Undefined),
                    }
                }
            }
        }

        // For complex expressions, evaluate against the tuple's `@` value (the
        // real element), not the wrapper. The tuple's carried focus/index/ancestor
        // bindings are reachable via context (bound by evaluate_sort), so a term
        // like `$`, `%.Price`, or `$pos` still resolves correctly.
        // A null result is a value, as for a simple field: jsonata-js's
        // comparator raises T2008 when it compares it with another defined
        // term, and only an undefined term sorts last.
        self.evaluate_internal(term_expr, &actual_element)
    }

    /// Evaluate sort operator. `tuples` is set when `data` is a tuple stream,
    /// whose wrappers' `@` values are ordered with their bindings in scope;
    /// otherwise input objects shaped like wrappers are ordinary data.
    fn evaluate_sort(
        &mut self,
        data: &JValue,
        terms: &[(AstNode, bool)],
        tuples: bool,
    ) -> Result<JValue, EvaluatorError> {
        // If data is null, return null
        if data.is_null() {
            return Ok(JValue::Null);
        }

        // If data is not an array, return it as-is (can't sort a single value)
        let array = match data {
            JValue::Array(arr) => arr.clone(),
            other => return Ok(other.clone()),
        };
        // jsonata-js `fn.sort` returns an input of length 0 or 1 as is (still a
        // sequence, if it was one), and builds a new plain array otherwise.
        if array.len() <= 1 {
            return Ok(JValue::Array(array));
        }

        // A sort of sorted tuples orders them as plain objects, as
        // jsonata-js does; the result keeps the tuples themselves.
        let plain = array.sorted_tuples.then(|| sorted_tuple_items(&array));

        // Evaluate sort keys for each element
        let mut indexed_array: Vec<(usize, Vec<JValue>)> = Vec::new();

        for (idx, element) in array.iter().enumerate() {
            let mut sort_keys = Vec::new();
            let element = match &plain {
                Some(plain) => &plain[idx],
                None => element,
            };

            // When sorting a tuple stream (the input path had a `%`/`@`/`#`
            // step, so each element is a `{@, !label, $var, __tuple__}`
            // wrapper), bind its carried ancestor/focus/index keys into scope
            // so a `%` (or `$focus`) inside a sort term resolves -- mirroring
            // create_tuple_stream's per-tuple frame binding. Sort terms attach
            // to a synthetic step after the last input step, so `%` refers to
            // the last input step's ancestry, carried under `!label` here.
            // Saves/restores rather than blindly unbinding, so a tuple key
            // that collides with a live outer `:=` binding doesn't get
            // deleted once this row's sort terms are evaluated.
            let tuple_bindings = match element {
                JValue::Object(obj) if tuples && is_tuple_object(obj) => {
                    Some(self.bind_tuple_keys(obj))
                }
                _ => None,
            };

            // When sorting a tuple stream, `$` and the term's data context are the
            // tuple's `@` value, not the `{@, $var, !label, __tuple__}` wrapper --
            // otherwise a term like `^($)` would try to order by the wrapper
            // object and raise T2008. The carried focus/index/ancestor keys stay
            // reachable via the context bindings established just above.
            let term_data = match element {
                JValue::Object(obj) if tuples && is_tuple_object(obj) => {
                    obj.get("@").cloned().unwrap_or(JValue::Null)
                }
                other => other.clone(),
            };

            // Evaluate each sort term with $ bound to the element
            for (term_expr, _ascending) in terms {
                // Save current $ binding
                let saved_dollar = self.context.lookup("$").cloned();

                // Bind $ to current element
                self.context.bind("$".to_string(), term_data.clone());

                // Evaluate the sort expression, distinguishing missing fields from explicit null
                let sort_value = self.evaluate_sort_term(term_expr, element, tuples)?;

                // Restore $ binding
                if let Some(val) = saved_dollar {
                    self.context.bind("$".to_string(), val);
                } else {
                    self.context.unbind("$");
                }

                sort_keys.push(sort_value);
            }

            if let Some(tuple_bindings) = tuple_bindings {
                tuple_bindings.restore(self);
            }

            indexed_array.push((idx, sort_keys));
        }

        // When each term's defined values are all numbers or all strings, no
        // comparison can fail and any stable sort gives jsonata-js's order.
        // Otherwise the keyed items go through jsonata-js's merge sort and
        // comparator, so the type checks (T2008, T2007) apply only to the
        // pairs it compares, after its check for undefined terms.
        let homogeneous = (0..terms.len()).all(|term| {
            let mut kind = None;
            indexed_array.iter().all(|(_, keys)| {
                let this = match &keys[term] {
                    JValue::Undefined => return true,
                    JValue::Number(_) => 0,
                    JValue::String(_) => 1,
                    _ => return false,
                };
                *kind.get_or_insert(this) == this
            })
        });
        let sorted: Vec<JValue> = if homogeneous {
            indexed_array.sort_by(|(_, a), (_, b)| Self::compare_sort_keys(a, b, terms));
            indexed_array
                .iter()
                .map(|(idx, _)| array[*idx].clone())
                .collect()
        } else {
            let mut order: Vec<usize> = (0..indexed_array.len()).collect();
            let mut buffer = Vec::with_capacity(order.len());
            Self::merge_sort_keyed(&mut order, &mut buffer, &indexed_array, terms)?;
            order.iter().map(|&idx| array[idx].clone()).collect()
        };

        Ok(JValue::array(sorted))
    }

    /// jsonata-js's order-by comparison of two items' keys whose terms are
    /// known to compare without error: an undefined term sorts last in
    /// either direction.
    fn compare_sort_keys(a: &[JValue], b: &[JValue], terms: &[(AstNode, bool)]) -> Ordering {
        for (index, (_, ascending)) in terms.iter().enumerate() {
            let comp = match (&a[index], &b[index]) {
                (JValue::Undefined, JValue::Undefined) => continue,
                (JValue::Undefined, _) => return Ordering::Greater,
                (_, JValue::Undefined) => return Ordering::Less,
                (JValue::Number(x), JValue::Number(y)) => {
                    x.partial_cmp(y).unwrap_or(Ordering::Equal)
                }
                (JValue::String(x), JValue::String(y)) => x.cmp(y),
                _ => Ordering::Equal,
            };
            if comp != Ordering::Equal {
                return if *ascending { comp } else { comp.reverse() };
            }
        }
        Ordering::Equal
    }

    /// jsonata-js `fn.sort`'s merge sort over item indices: split at the
    /// middle, sort each half, and merge taking the right item only when the
    /// comparator says the left one goes after it.
    fn merge_sort_keyed(
        order: &mut [usize],
        buffer: &mut Vec<usize>,
        keys: &[(usize, Vec<JValue>)],
        terms: &[(AstNode, bool)],
    ) -> Result<(), EvaluatorError> {
        if order.len() <= 1 {
            return Ok(());
        }
        let middle = order.len() / 2;
        Self::merge_sort_keyed(&mut order[..middle], buffer, keys, terms)?;
        Self::merge_sort_keyed(&mut order[middle..], buffer, keys, terms)?;
        buffer.clear();
        let (mut i, mut j) = (0, middle);
        while i < middle && j < order.len() {
            if Self::sort_keys_after(&keys[order[i]].1, &keys[order[j]].1, terms)? {
                buffer.push(order[j]);
                j += 1;
            } else {
                buffer.push(order[i]);
                i += 1;
            }
        }
        buffer.extend_from_slice(&order[i..middle]);
        buffer.extend_from_slice(&order[j..]);
        order.copy_from_slice(buffer);
        Ok(())
    }

    /// jsonata-js's order-by comparator: true when the item with keys `a`
    /// goes after the one with keys `b`. An undefined term sorts last in
    /// either direction and is checked before the types; other terms must
    /// both be numbers or both be strings.
    fn sort_keys_after(
        a: &[JValue],
        b: &[JValue],
        terms: &[(AstNode, bool)],
    ) -> Result<bool, EvaluatorError> {
        fn type_name(value: &JValue) -> &'static str {
            match value {
                JValue::Number(_) => "number",
                JValue::String(_) => "string",
                _ => "object",
            }
        }
        let mut comp = Ordering::Equal;
        for (index, (_, ascending)) in terms.iter().enumerate() {
            if comp != Ordering::Equal {
                break;
            }
            let (aa, bb) = (&a[index], &b[index]);
            match (aa.is_undefined(), bb.is_undefined()) {
                (true, true) => continue,
                (true, false) => {
                    comp = Ordering::Greater;
                    continue;
                }
                (false, true) => {
                    comp = Ordering::Less;
                    continue;
                }
                (false, false) => {}
            }
            comp = match (aa, bb) {
                (JValue::Number(x), JValue::Number(y)) => {
                    x.partial_cmp(y).unwrap_or(Ordering::Equal)
                }
                (JValue::String(x), JValue::String(y)) => x.cmp(y),
                (JValue::Number(_) | JValue::String(_), JValue::Number(_) | JValue::String(_)) => {
                    return Err(EvaluatorError::TypeError(format!(
                        "T2007: Type mismatch when comparing values in order-by clause: {} and {}",
                        type_name(aa),
                        type_name(bb)
                    )));
                }
                _ => {
                    return Err(EvaluatorError::TypeError(
                        "T2008: The expressions within an order-by clause must evaluate to numeric or string values"
                            .to_string(),
                    ));
                }
            };
            if !*ascending {
                comp = comp.reverse();
            }
        }
        Ok(comp == Ordering::Greater)
    }

    /// Check if a value is truthy (JSONata semantics).
    fn is_truthy(&self, value: &JValue) -> bool {
        // One definition of truthiness for both engines (see
        // `compiled_is_truthy` for the recursive-container rule, #111).
        compiled_is_truthy(value)
    }

    /// Addition
    /// Get human-readable type name for error messages
    fn type_name(value: &JValue) -> &'static str {
        match value {
            JValue::Null => "null",
            JValue::Bool(_) => "boolean",
            JValue::Number(_) => "number",
            JValue::String(_) => "string",
            JValue::Array(_) => "array",
            JValue::Object(_) => "object",
            #[cfg(feature = "python")]
            JValue::LazyPyDict(_) => "object",
            _ => "unknown",
        }
    }

    /// Convert a value to a string for concatenation
    fn value_to_concat_string(value: &JValue) -> Result<String, EvaluatorError> {
        // Normalize a lazy operand up front: `functions::string::string`'s lazy arm maps a
        // conversion failure to `JValue::Null` (silently stringifying to `""`), which would
        // swallow the TypeError this must raise instead. Guarded by `is_lazy` so the
        // common (non-lazy) path pays no clone.
        let normalized;
        let value = if value.is_lazy() {
            normalized = normalize_lazy(value)?;
            &normalized
        } else {
            value
        };
        match value {
            JValue::String(s) => Ok(s.to_string()),
            // An explicit null stringifies as "null" (same as `$string(null)`);
            // only an *undefined* operand contributes nothing.
            JValue::Undefined => Ok(String::new()),
            JValue::Null
            | JValue::Number(_)
            | JValue::Bool(_)
            | JValue::Array(_)
            | JValue::Object(_) => match crate::functions::string::string(value, None) {
                Ok(JValue::String(s)) => Ok(s.to_string()),
                Ok(JValue::Null) => Ok(String::new()),
                _ => Err(EvaluatorError::TypeError(
                    "Cannot concatenate complex types".to_string(),
                )),
            },
            _ => Ok(String::new()),
        }
    }

    /// String concatenation
    fn concatenate(&self, left: &JValue, right: &JValue) -> Result<JValue, EvaluatorError> {
        let left_str = Self::value_to_concat_string(left)?;
        let right_str = Self::value_to_concat_string(right)?;
        Ok(JValue::string(format!("{}{}", left_str, right_str)))
    }

    /// Range operator (e.g., 1..5 produces [1,2,3,4,5])
    fn range(&self, left: &JValue, right: &JValue) -> Result<JValue, EvaluatorError> {
        // Check left operand is a number or null
        let start_f64 = match left {
            JValue::Number(n) => Some(*n),
            JValue::Null | JValue::Undefined => None,
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
            JValue::Number(n) => Some(*n),
            JValue::Null | JValue::Undefined => None,
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
            return Ok(JValue::sequence(vec![]));
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
        check_sequence_length(size, &self.options)?;

        let mut result = Vec::with_capacity(size);
        if start <= end {
            for i in start..=end {
                result.push(JValue::Number(i as f64));
            }
        }
        // Note: if start > end, return empty array (not reversed)
        // Numbers only: flat for a `$` step (#196).
        Ok(JValue::sequence_flat(result, true))
    }

    /// The `in` membership operator.
    ///
    /// Mirrors jsonata-js's `evaluateIncludesExpression`: an *undefined* operand
    /// on either side makes the result false; a non-array right side is wrapped
    /// in a one-element array; and membership is decided with `===`, so
    /// primitives match by value while composites match only by identity.
    /// `obj in [obj]` is true and `obj in [{"k": 1}]` is false.
    ///
    /// An explicit null is a value here and matches itself. An object on the
    /// right is NOT key-containment -- `"k" in obj` is false, because the object
    /// is wrapped and compared against the string.
    fn in_operator(&self, left: &JValue, right: &JValue) -> Result<JValue, EvaluatorError> {
        if left.is_undefined() || right.is_undefined() {
            return Ok(JValue::Bool(false));
        }

        // Deliberately NOT normalizing a lazy operand here. Membership is decided
        // by identity, and materializing one side turns a lazy dict into a fresh
        // `Object` that can no longer be identical to the lazy dict on the other
        // side -- which made `obj in obj` false on the Python-dict route only.
        // `LazyPyDict::same_object` compares the underlying Python object.

        /// `===` in Rust terms: primitives by value, composites by identity.
        fn identical(a: &JValue, b: &JValue) -> bool {
            match (a, b) {
                (JValue::Null, JValue::Null) => true,
                (JValue::Bool(x), JValue::Bool(y)) => x == y,
                (JValue::Number(x), JValue::Number(y)) => x == y,
                (JValue::String(x), JValue::String(y)) => x == y,
                (JValue::Array(x), JValue::Array(y)) => JArray::ptr_eq(x, y),
                (JValue::Object(x), JValue::Object(y)) => Rc::ptr_eq(x, y),
                #[cfg(feature = "python")]
                (JValue::LazyPyDict(x), JValue::LazyPyDict(y)) => x.same_object(y),
                _ => false,
            }
        }

        match right {
            JValue::Array(arr) => Ok(JValue::Bool(arr.iter().any(|v| identical(left, v)))),
            other => Ok(JValue::Bool(identical(left, other))),
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
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        // First, look up the function to ensure it exists
        let bound = self.context.lookup(name);
        let is_lambda = matches!(bound, Some(JValue::Lambda { .. }));
        // A binding that is not a function hides any builtin of that name.
        if bound.is_some_and(|v| !v.is_function()) {
            return Err(EvaluatorError::EvaluationError(
                "T1008: Attempted to partially apply a non-function".to_string(),
            ));
        }

        // Built-in functions must be called with $ prefix for partial application
        // Without $, it's an error (T1007) suggesting the user forgot the $
        if !is_lambda && !is_builtin {
            // Check if it's a built-in function called without $
            if self.is_builtin_function(name) {
                return Err(EvaluatorError::EvaluationError(format!(
                    "T1007: Attempted to partially apply a non-function. Did you mean ${}?",
                    name
                )));
            }
            return Err(EvaluatorError::EvaluationError(
                "T1008: Attempted to partially apply a non-function".to_string(),
            ));
        }

        // The target is resolved now, as jsonata-js evaluates the procedure
        // when it builds the partial: a closure, builtin or host function, so
        // rebinding the name afterwards does not change it.
        let target = match bound {
            Some(JValue::Lambda(rc)) => PartialTarget::Lambda(Rc::clone(rc)),
            Some(JValue::Builtin { name: b }) => PartialTarget::Builtin(b.to_string()),
            _ if self.host_fns.contains_key(name) => {
                PartialTarget::Host(HostTarget(Rc::clone(&self.host_fns[name])))
            }
            _ if self.is_builtin_function(name) => PartialTarget::Builtin(name.to_string()),
            _ => {
                return Err(EvaluatorError::EvaluationError(
                    "T1008: Attempted to partially apply a non-function".to_string(),
                ))
            }
        };
        self.build_partial_application(target, args, data)
    }

    /// Build the partial application of `target` to `args`, whose `?`
    /// placeholders become the new function's parameters. The other
    /// arguments are evaluated now.
    fn build_partial_application(
        &mut self,
        target: PartialTarget,
        args: &[AstNode],
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        // Evaluate non-placeholder arguments and track placeholder positions
        let mut bound_args: Vec<(usize, JValue)> = Vec::new();
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

        let stored_lambda = StoredLambda {
            params: param_names,
            body: AstNode::Null, // Inert: invocation dispatches on `partial`
            compiled_body: None,
            signature: None,
            captured_env: HashMap::new(),
            captured_data: None, // Targets evaluate against the call-site data
            thunk: false,
            self_name: None,
            partial: Some(Rc::new(PartialApplication {
                target,
                bound_args,
                placeholder_positions,
                total_args: args.len(),
            })),
            live_token: LiveLambdaToken::new(),
            // Its target is resolved already; the scope is never consulted.
            def_scope: DefScope::ROOT,
        };

        Ok(JValue::Lambda(Rc::new(stored_lambda)))
    }

    /// Invoke a partial application: rebuild the full argument list (bound
    /// values in their creation-time positions, call arguments filling the
    /// placeholder positions in order, anything else Null) and apply the
    /// target.
    fn invoke_partial(
        &mut self,
        partial: &PartialApplication,
        values: &[JValue],
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        // As in jsonata-js, a placeholder left without an argument is
        // undefined.
        let mut full_args: Vec<JValue> = vec![JValue::Undefined; partial.total_args];
        for (pos, value) in &partial.bound_args {
            if *pos < partial.total_args {
                full_args[*pos] = value.clone();
            }
        }
        for (i, &pos) in partial.placeholder_positions.iter().enumerate() {
            if pos < partial.total_args {
                full_args[pos] = values.get(i).cloned().unwrap_or(JValue::Undefined);
            }
        }

        match &partial.target {
            PartialTarget::Lambda(stored) => {
                let stored = Rc::clone(stored);
                self.invoke_stored_lambda(&stored, &full_args, data)
            }
            PartialTarget::Builtin(name) => self.call_builtin_with_values(name, &full_args, data),
            PartialTarget::Host(HostTarget(f)) => {
                let f = Rc::clone(f);
                let mut ctx = HostCtx::new();
                f.call(&full_args, &mut ctx)
            }
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

    // --- Task 7: tuple-wrapper output leak -----------------------------------
    //
    // `%`/`@`/`#` are implemented internally via a tuple-stream representation
    // (`create_tuple_stream`): each element gets wrapped as
    // `{"@": value, "__tuple__": true, ...bindings}`. Intermediate path steps
    // consume/re-wrap these, but the *final* evaluate() result can still carry
    // a lingering wrapper -- confirmed for real by dumping actual output before
    // this fix (see task-7-report.md for the raw before/after). These tests
    // pin both the bare top-level case (Task 5's brief `#` example) and the
    // object/array-construction-nested case (found while verifying the brief's
    // illustrative fix against real output -- a plain per-element Array-only
    // recursion does not reach into a constructed object's field values).

    fn dataset5_for_tuple_tests() -> JValue {
        let s = include_str!("../tests/jsonata-js/test/test-suite/datasets/dataset5.json");
        serde_json::from_str::<serde_json::Value>(s).unwrap().into()
    }

    fn assert_no_tuple_wrapper(value: &JValue) {
        match value {
            JValue::Object(obj) => {
                assert!(
                    obj.get("__tuple__").is_none(),
                    "tuple wrapper leaked into output: {:?}",
                    value
                );
                for v in obj.values() {
                    assert_no_tuple_wrapper(v);
                }
            }
            JValue::Array(arr) => {
                for item in arr.iter() {
                    assert_no_tuple_wrapper(item);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn test_bare_index_bind_result_does_not_leak_tuple_wrapper() {
        let data: JValue = serde_json::json!({"items": [1, 2, 3]}).into();
        let ast = crate::parser::parse("items#$i").unwrap();
        let mut evaluator = Evaluator::new();
        let result = evaluator.evaluate(&ast, &data).unwrap();
        assert_no_tuple_wrapper(&result);
        assert_eq!(
            result,
            JValue::array(vec![
                JValue::from(1i64),
                JValue::from(2i64),
                JValue::from(3i64)
            ])
        );
    }

    #[test]
    fn test_percent_predicate_result_does_not_leak_tuple_wrapper() {
        // Confirmed by Task 6 to evaluate to the correct @-values but stay
        // wrapped: Account.Order.Product[%.OrderID='order104'].SKU
        let data = dataset5_for_tuple_tests();
        let ast = crate::parser::parse("Account.Order.Product[%.OrderID='order104'].SKU").unwrap();
        let mut evaluator = Evaluator::new();
        let result = evaluator.evaluate(&ast, &data).unwrap();
        assert_no_tuple_wrapper(&result);
        assert_eq!(
            result,
            JValue::array(vec![
                JValue::string("040657863"),
                JValue::string("0406654603"),
            ])
        );
    }

    #[test]
    fn test_percent_step_over_tuple_stream_does_not_leak_tuple_wrapper() {
        // Confirmed by Task 6: Account.Order.Product.Price.%[%.OrderID='order103'].SKU
        let data = dataset5_for_tuple_tests();
        let ast = crate::parser::parse("Account.Order.Product.Price.%[%.OrderID='order103'].SKU")
            .unwrap();
        let mut evaluator = Evaluator::new();
        let result = evaluator.evaluate(&ast, &data).unwrap();
        assert_no_tuple_wrapper(&result);
        assert_eq!(
            result,
            JValue::array(vec![
                JValue::string("0406654608"),
                JValue::string("0406634348"),
            ])
        );
    }

    #[test]
    fn test_tuple_wrapper_does_not_leak_when_nested_in_object_construction() {
        // A tuple-producing expression nested inside a constructed object's field
        // value: the top-level result is a plain (non-tuple) Object, so a naive
        // "unwrap only if the whole value is a tuple wrapper" check would miss
        // this -- must recurse into field values too.
        let data = dataset5_for_tuple_tests();
        let ast =
            crate::parser::parse(r#"{ "skus": Account.Order.Product[%.OrderID='order104'].SKU }"#)
                .unwrap();
        let mut evaluator = Evaluator::new();
        let result = evaluator.evaluate(&ast, &data).unwrap();
        assert_no_tuple_wrapper(&result);
        assert_eq!(
            result,
            JValue::from(serde_json::json!({
                "skus": ["040657863", "0406654603"]
            }))
        );
    }

    #[test]
    fn test_tuple_wrapper_does_not_leak_when_nested_in_array_construction() {
        let data: JValue = serde_json::json!({"items": [1, 2, 3]}).into();
        let ast = crate::parser::parse("[items#$i]").unwrap();
        let mut evaluator = Evaluator::new();
        let result = evaluator.evaluate(&ast, &data).unwrap();
        assert_no_tuple_wrapper(&result);
    }

    #[test]
    fn test_evaluate_literals() {
        let mut evaluator = Evaluator::new();
        let data = JValue::Null;

        // String literal
        let result = evaluator
            .evaluate(&AstNode::string("hello"), &data)
            .unwrap();
        assert_eq!(result, JValue::string("hello"));

        // Number literal
        let result = evaluator.evaluate(&AstNode::number(42.0), &data).unwrap();
        assert_eq!(result, JValue::from(42i64));

        // Boolean literal
        let result = evaluator.evaluate(&AstNode::boolean(true), &data).unwrap();
        assert_eq!(result, JValue::Bool(true));

        // Null literal
        let result = evaluator.evaluate(&AstNode::null(), &data).unwrap();
        assert_eq!(result, JValue::Null);
    }

    #[test]
    fn test_evaluate_variables() {
        let mut evaluator = Evaluator::new();
        let data = JValue::Null;

        // Bind a variable
        evaluator
            .context
            .bind("x".to_string(), JValue::from(100i64));

        // Look up the variable
        let result = evaluator.evaluate(&AstNode::variable("x"), &data).unwrap();
        assert_eq!(result, JValue::from(100i64));

        // An unbound variable is undefined, not null (see #98).
        let result = evaluator
            .evaluate(&AstNode::variable("undefined"), &data)
            .unwrap();
        assert_eq!(result, JValue::Undefined);
    }

    #[test]
    fn test_evaluate_path() {
        let mut evaluator = Evaluator::new();
        let data = JValue::from(serde_json::json!({
            "foo": {
                "bar": {
                    "baz": 42
                }
            }
        }));
        // Simple path
        let path = AstNode::Path {
            steps: vec![PathStep::new(AstNode::Name("foo".to_string()))],
        };
        let result = evaluator.evaluate(&path, &data).unwrap();
        assert_eq!(
            result,
            JValue::from(serde_json::json!({"bar": {"baz": 42}}))
        );

        // Nested path
        let path = AstNode::Path {
            steps: vec![
                PathStep::new(AstNode::Name("foo".to_string())),
                PathStep::new(AstNode::Name("bar".to_string())),
                PathStep::new(AstNode::Name("baz".to_string())),
            ],
        };
        let result = evaluator.evaluate(&path, &data).unwrap();
        assert_eq!(result, JValue::from(42i64));

        // Missing path returns undefined (not null - see issue #32)
        let path = AstNode::Path {
            steps: vec![PathStep::new(AstNode::Name("missing".to_string()))],
        };
        let result = evaluator.evaluate(&path, &data).unwrap();
        assert_eq!(result, JValue::Undefined);
    }

    #[test]
    fn test_arithmetic_operations() {
        let mut evaluator = Evaluator::new();
        let data = JValue::Null;

        // Addition
        let expr = AstNode::Binary {
            op: BinaryOp::Add,
            lhs: Box::new(AstNode::number(10.0)),
            rhs: Box::new(AstNode::number(5.0)),
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, JValue::Number(15.0));

        // Subtraction
        let expr = AstNode::Binary {
            op: BinaryOp::Subtract,
            lhs: Box::new(AstNode::number(10.0)),
            rhs: Box::new(AstNode::number(5.0)),
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, JValue::Number(5.0));

        // Multiplication
        let expr = AstNode::Binary {
            op: BinaryOp::Multiply,
            lhs: Box::new(AstNode::number(10.0)),
            rhs: Box::new(AstNode::number(5.0)),
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, JValue::Number(50.0));

        // Division
        let expr = AstNode::Binary {
            op: BinaryOp::Divide,
            lhs: Box::new(AstNode::number(10.0)),
            rhs: Box::new(AstNode::number(5.0)),
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, JValue::Number(2.0));

        // Modulo
        let expr = AstNode::Binary {
            op: BinaryOp::Modulo,
            lhs: Box::new(AstNode::number(10.0)),
            rhs: Box::new(AstNode::number(3.0)),
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, JValue::Number(1.0));
    }

    #[test]
    fn test_division_by_zero() {
        let mut evaluator = Evaluator::new();
        let data = JValue::Null;

        // jsonata-js has no division-by-zero error: `10/0` is Infinity, and the
        // D1001 appears only when that value is used as an operand. This
        // asserted an error that the reference does not raise (#102).
        let expr = AstNode::Binary {
            op: BinaryOp::Divide,
            lhs: Box::new(AstNode::number(10.0)),
            rhs: Box::new(AstNode::number(0.0)),
        };
        match evaluator.evaluate(&expr, &data) {
            Ok(JValue::Number(n)) => assert!(n.is_infinite() && n > 0.0),
            other => panic!("expected +inf, got {other:?}"),
        }
    }

    #[test]
    fn test_comparison_operations() {
        let mut evaluator = Evaluator::new();
        let data = JValue::Null;

        // Equal
        let expr = AstNode::Binary {
            op: BinaryOp::Equal,
            lhs: Box::new(AstNode::number(5.0)),
            rhs: Box::new(AstNode::number(5.0)),
        };
        assert_eq!(
            evaluator.evaluate(&expr, &data).unwrap(),
            JValue::Bool(true)
        );

        // Not equal
        let expr = AstNode::Binary {
            op: BinaryOp::NotEqual,
            lhs: Box::new(AstNode::number(5.0)),
            rhs: Box::new(AstNode::number(3.0)),
        };
        assert_eq!(
            evaluator.evaluate(&expr, &data).unwrap(),
            JValue::Bool(true)
        );

        // Less than
        let expr = AstNode::Binary {
            op: BinaryOp::LessThan,
            lhs: Box::new(AstNode::number(3.0)),
            rhs: Box::new(AstNode::number(5.0)),
        };
        assert_eq!(
            evaluator.evaluate(&expr, &data).unwrap(),
            JValue::Bool(true)
        );

        // Greater than
        let expr = AstNode::Binary {
            op: BinaryOp::GreaterThan,
            lhs: Box::new(AstNode::number(5.0)),
            rhs: Box::new(AstNode::number(3.0)),
        };
        assert_eq!(
            evaluator.evaluate(&expr, &data).unwrap(),
            JValue::Bool(true)
        );
    }

    #[test]
    fn test_logical_operations() {
        let mut evaluator = Evaluator::new();
        let data = JValue::Null;

        // And - both true
        let expr = AstNode::Binary {
            op: BinaryOp::And,
            lhs: Box::new(AstNode::boolean(true)),
            rhs: Box::new(AstNode::boolean(true)),
        };
        assert_eq!(
            evaluator.evaluate(&expr, &data).unwrap(),
            JValue::Bool(true)
        );

        // And - first false
        let expr = AstNode::Binary {
            op: BinaryOp::And,
            lhs: Box::new(AstNode::boolean(false)),
            rhs: Box::new(AstNode::boolean(true)),
        };
        assert_eq!(
            evaluator.evaluate(&expr, &data).unwrap(),
            JValue::Bool(false)
        );

        // Or - first true
        let expr = AstNode::Binary {
            op: BinaryOp::Or,
            lhs: Box::new(AstNode::boolean(true)),
            rhs: Box::new(AstNode::boolean(false)),
        };
        assert_eq!(
            evaluator.evaluate(&expr, &data).unwrap(),
            JValue::Bool(true)
        );

        // Or - both false
        let expr = AstNode::Binary {
            op: BinaryOp::Or,
            lhs: Box::new(AstNode::boolean(false)),
            rhs: Box::new(AstNode::boolean(false)),
        };
        assert_eq!(
            evaluator.evaluate(&expr, &data).unwrap(),
            JValue::Bool(false)
        );
    }

    #[test]
    fn test_string_concatenation() {
        let mut evaluator = Evaluator::new();
        let data = JValue::Null;

        let expr = AstNode::Binary {
            op: BinaryOp::Concatenate,
            lhs: Box::new(AstNode::string("Hello")),
            rhs: Box::new(AstNode::string(" World")),
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, JValue::string("Hello World"));
    }

    #[test]
    fn test_range_operator() {
        let mut evaluator = Evaluator::new();
        let data = JValue::Null;

        // Forward range
        let expr = AstNode::Binary {
            op: BinaryOp::Range,
            lhs: Box::new(AstNode::number(1.0)),
            rhs: Box::new(AstNode::number(5.0)),
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(
            result,
            JValue::array(vec![
                JValue::Number(1.0),
                JValue::Number(2.0),
                JValue::Number(3.0),
                JValue::Number(4.0),
                JValue::Number(5.0)
            ])
        );

        // A backward range (start > end) is an empty sequence, which is
        // undefined on its own; inside `[...]` it contributes nothing.
        let expr = AstNode::Binary {
            op: BinaryOp::Range,
            lhs: Box::new(AstNode::number(5.0)),
            rhs: Box::new(AstNode::number(1.0)),
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, JValue::Undefined);
    }

    #[test]
    fn test_in_operator() {
        let mut evaluator = Evaluator::new();
        let data = JValue::Null;

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
        assert_eq!(result, JValue::Bool(true));

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
        assert_eq!(result, JValue::Bool(false));
    }

    #[test]
    fn test_unary_operations() {
        let mut evaluator = Evaluator::new();
        let data = JValue::Null;

        // Negation
        let expr = AstNode::Unary {
            op: UnaryOp::Negate,
            operand: Box::new(AstNode::number(5.0)),
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, JValue::Number(-5.0));

        // Not
        let expr = AstNode::Unary {
            op: UnaryOp::Not,
            operand: Box::new(AstNode::boolean(true)),
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, JValue::Bool(false));
    }

    #[test]
    fn test_array_construction() {
        let mut evaluator = Evaluator::new();
        let data = JValue::Null;

        let expr = AstNode::Array(vec![
            AstNode::number(1.0),
            AstNode::number(2.0),
            AstNode::number(3.0),
        ]);
        let result = evaluator.evaluate(&expr, &data).unwrap();
        // Whole number literals are preserved as integers
        assert_eq!(result, JValue::from(serde_json::json!([1, 2, 3])));
    }

    #[test]
    fn test_object_construction() {
        let mut evaluator = Evaluator::new();
        let data = JValue::Null;

        let expr = AstNode::Object(vec![
            (AstNode::string("name"), AstNode::string("Alice")),
            (AstNode::string("age"), AstNode::number(30.0)),
        ]);
        let result = evaluator.evaluate(&expr, &data).unwrap();
        // Whole number literals are preserved as integers
        let mut expected = IndexMap::new();
        expected.insert("name".to_string(), JValue::string("Alice"));
        expected.insert("age".to_string(), JValue::Number(30.0));
        assert_eq!(result, JValue::object(expected));
    }

    #[test]
    fn test_conditional() {
        let mut evaluator = Evaluator::new();
        let data = JValue::Null;

        // True condition
        let expr = AstNode::Conditional {
            condition: Box::new(AstNode::boolean(true)),
            then_branch: Box::new(AstNode::string("yes")),
            else_branch: Some(Box::new(AstNode::string("no"))),
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, JValue::string("yes"));

        // False condition
        let expr = AstNode::Conditional {
            condition: Box::new(AstNode::boolean(false)),
            then_branch: Box::new(AstNode::string("yes")),
            else_branch: Some(Box::new(AstNode::string("no"))),
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, JValue::string("no"));

        // No else branch returns undefined (not null)
        let expr = AstNode::Conditional {
            condition: Box::new(AstNode::boolean(false)),
            then_branch: Box::new(AstNode::string("yes")),
            else_branch: None,
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, JValue::Undefined);
    }

    #[test]
    fn test_block_expression() {
        let mut evaluator = Evaluator::new();
        let data = JValue::Null;

        let expr = AstNode::Block(vec![
            AstNode::number(1.0),
            AstNode::number(2.0),
            AstNode::number(3.0),
        ]);
        let result = evaluator.evaluate(&expr, &data).unwrap();
        // Block returns the last expression; whole numbers are preserved as integers
        assert_eq!(result, JValue::from(3i64));
    }

    #[test]
    fn test_function_calls() {
        let mut evaluator = Evaluator::new();
        let data = JValue::Null;

        // uppercase function
        let expr = AstNode::Function {
            name: "uppercase".to_string(),
            args: vec![AstNode::string("hello")],
            is_builtin: true,
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, JValue::string("HELLO"));

        // lowercase function
        let expr = AstNode::Function {
            name: "lowercase".to_string(),
            args: vec![AstNode::string("HELLO")],
            is_builtin: true,
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, JValue::string("hello"));

        // length function
        let expr = AstNode::Function {
            name: "length".to_string(),
            args: vec![AstNode::string("hello")],
            is_builtin: true,
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, JValue::from(5i64));

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
        assert_eq!(result, JValue::Number(6.0));

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
        assert_eq!(result, JValue::from(3i64));
    }

    #[test]
    fn test_complex_nested_data() {
        let mut evaluator = Evaluator::new();
        let data = JValue::from(serde_json::json!({
            "users": [
                {"name": "Alice", "age": 30},
                {"name": "Bob", "age": 25},
                {"name": "Charlie", "age": 35}
            ],
            "metadata": {
                "total": 3,
                "version": "1.0"
            }
        }));
        // Access nested field
        let path = AstNode::Path {
            steps: vec![
                PathStep::new(AstNode::Name("metadata".to_string())),
                PathStep::new(AstNode::Name("version".to_string())),
            ],
        };
        let result = evaluator.evaluate(&path, &data).unwrap();
        assert_eq!(result, JValue::string("1.0"));
    }

    #[test]
    fn test_error_handling() {
        let mut evaluator = Evaluator::new();
        let data = JValue::Null;

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

        assert!(!evaluator.is_truthy(&JValue::Null));
        assert!(!evaluator.is_truthy(&JValue::Bool(false)));
        assert!(evaluator.is_truthy(&JValue::Bool(true)));
        assert!(!evaluator.is_truthy(&JValue::from(0i64)));
        assert!(evaluator.is_truthy(&JValue::from(1i64)));
        assert!(!evaluator.is_truthy(&JValue::string("")));
        assert!(evaluator.is_truthy(&JValue::string("hello")));
        assert!(!evaluator.is_truthy(&JValue::array(vec![])));
        assert!(evaluator.is_truthy(&JValue::from(serde_json::json!([1, 2, 3]))));
    }

    #[test]
    fn test_integration_with_parser() {
        use crate::parser::parse;

        let mut evaluator = Evaluator::new();
        let data = JValue::from(serde_json::json!({
            "price": 10,
            "quantity": 5
        }));
        // Test simple path
        let ast = parse("price").unwrap();
        let result = evaluator.evaluate(&ast, &data).unwrap();
        assert_eq!(result, JValue::from(10i64));

        // Test arithmetic
        let ast = parse("price * quantity").unwrap();
        let result = evaluator.evaluate(&ast, &data).unwrap();
        // Note: Arithmetic operations produce f64 results in JSON
        assert_eq!(result, JValue::Number(50.0));

        // Test comparison
        let ast = parse("price > 5").unwrap();
        let result = evaluator.evaluate(&ast, &data).unwrap();
        assert_eq!(result, JValue::Bool(true));
    }

    #[test]
    fn test_evaluate_dollar_function_uppercase() {
        use crate::parser::parse;

        let mut evaluator = Evaluator::new();
        let ast = parse(r#"$uppercase("hello")"#).unwrap();
        let empty = JValue::object(IndexMap::new());
        let result = evaluator.evaluate(&ast, &empty).unwrap();
        assert_eq!(result, JValue::string("HELLO"));
    }

    #[test]
    fn test_evaluate_dollar_function_sum() {
        use crate::parser::parse;

        let mut evaluator = Evaluator::new();
        let ast = parse("$sum([1, 2, 3, 4, 5])").unwrap();
        let empty = JValue::object(IndexMap::new());
        let result = evaluator.evaluate(&ast, &empty).unwrap();
        assert_eq!(result, JValue::Number(15.0));
    }

    #[test]
    fn test_evaluate_nested_dollar_functions() {
        use crate::parser::parse;

        let mut evaluator = Evaluator::new();
        let ast = parse(r#"$length($lowercase("HELLO"))"#).unwrap();
        let empty = JValue::object(IndexMap::new());
        let result = evaluator.evaluate(&ast, &empty).unwrap();
        // length() returns an integer, not a float
        assert_eq!(result, JValue::Number(5.0));
    }

    #[test]
    fn test_array_mapping() {
        use crate::parser::parse;

        let mut evaluator = Evaluator::new();
        let data: JValue = serde_json::from_str(
            r#"{
            "products": [
                {"id": 1, "name": "Laptop", "price": 999.99},
                {"id": 2, "name": "Mouse", "price": 29.99},
                {"id": 3, "name": "Keyboard", "price": 79.99}
            ]
        }"#,
        )
        .map(|v: serde_json::Value| JValue::from(v))
        .unwrap();

        // Test mapping over array to extract field
        let ast = parse("products.name").unwrap();
        let result = evaluator.evaluate(&ast, &data).unwrap();
        assert_eq!(
            result,
            JValue::array(vec![
                JValue::string("Laptop"),
                JValue::string("Mouse"),
                JValue::string("Keyboard")
            ])
        );

        // Test mapping over array to extract prices
        let ast = parse("products.price").unwrap();
        let result = evaluator.evaluate(&ast, &data).unwrap();
        assert_eq!(
            result,
            JValue::array(vec![
                JValue::Number(999.99),
                JValue::Number(29.99),
                JValue::Number(79.99)
            ])
        );

        // Test with $sum function on mapped array
        let ast = parse("$sum(products.price)").unwrap();
        let result = evaluator.evaluate(&ast, &data).unwrap();
        assert_eq!(result, JValue::Number(1109.97));
    }

    #[test]
    fn test_empty_brackets() {
        use crate::parser::parse;

        let mut evaluator = Evaluator::new();

        // Test empty brackets on simple value - should wrap in array
        let data: JValue = JValue::from(serde_json::json!({"foo": "bar"}));
        let ast = parse("foo[]").unwrap();
        let result = evaluator.evaluate(&ast, &data).unwrap();
        assert_eq!(
            result,
            JValue::array(vec![JValue::string("bar")]),
            "Empty brackets should wrap value in array"
        );

        // Test empty brackets on array - should return array as-is
        let data2: JValue = JValue::from(serde_json::json!({"arr": [1, 2, 3]}));
        let ast2 = parse("arr[]").unwrap();
        let result2 = evaluator.evaluate(&ast2, &data2).unwrap();
        assert_eq!(
            result2,
            JValue::array(vec![
                JValue::Number(1.0),
                JValue::Number(2.0),
                JValue::Number(3.0)
            ]),
            "Empty brackets should preserve array"
        );
    }

    // ---- Tuple-stream runtime: %/@/# binding operators (Task 5) ----
    // Expected values below are ground-truthed against jsonata-js 2.x.

    #[test]
    fn test_index_bind_makes_variable_available_in_next_step() {
        // `#$o` binds each Order's position; `$o` must resolve in the later step.
        let data: JValue = serde_json::json!({
            "Account": {
                "Order": [
                    {"OrderID": "o1", "Product": [{"Name": "Hat"}]},
                    {"OrderID": "o2", "Product": [{"Name": "Cap"}, {"Name": "Sock"}]}
                ]
            }
        })
        .into();
        let ast =
            crate::parser::parse("Account.Order#$o.Product.{ 'name': Name, 'idx': $o }").unwrap();
        let mut evaluator = Evaluator::new();
        let result = evaluator.evaluate(&ast, &data).unwrap();
        assert_eq!(
            result,
            serde_json::json!([
                {"name": "Hat", "idx": 0},
                {"name": "Cap", "idx": 1},
                {"name": "Sock", "idx": 1}
            ])
            .into()
        );
    }

    #[test]
    fn test_index_bind_with_predicate_stage() {
        // Mirrors reference joins/index[13]: index binding, then a predicate on
        // the next step, carrying the index binding through.
        let data: JValue = serde_json::json!({
            "Account": {
                "Order": [
                    {"Product": [{"ProductID": 1, "Name": "A"}, {"ProductID": 9, "Name": "B"}]},
                    {"Product": [{"ProductID": 9, "Name": "C"}]}
                ]
            }
        })
        .into();
        let ast =
            crate::parser::parse("Account.Order#$o.Product[ProductID=9].{ 'n': Name, 'idx': $o }")
                .unwrap();
        let mut evaluator = Evaluator::new();
        let result = evaluator.evaluate(&ast, &data).unwrap();
        assert_eq!(
            result,
            serde_json::json!([
                {"n": "B", "idx": 0},
                {"n": "C", "idx": 1}
            ])
            .into()
        );
    }

    #[test]
    fn test_focus_bind_makes_variable_available_in_next_step() {
        // NOTE: `Account.Order@$o.Product` is `undefined` in jsonata-js (focus
        // does NOT advance the context `@`); the variable itself is what carries
        // forward. This asserts the real jsonata-js behaviour.
        let data: JValue = serde_json::json!({
            "Account": {
                "Order": [
                    {"OrderID": "o1"},
                    {"OrderID": "o2"}
                ]
            }
        })
        .into();
        let ast = crate::parser::parse("Account.Order@$o.$o.OrderID").unwrap();
        let mut evaluator = Evaluator::new();
        let result = evaluator.evaluate(&ast, &data).unwrap();
        assert_eq!(result, serde_json::json!(["o1", "o2"]).into());
    }

    #[test]
    fn test_parent_reference_resolves_to_enclosing_step_value() {
        let data: JValue = serde_json::json!({
            "Account": {
                "Order": [
                    {"OrderID": "o1", "Product": [{"Name": "Hat"}]}
                ]
            }
        })
        .into();
        let ast =
            crate::parser::parse("Account.Order.Product.{ 'name': Name, 'order': %.OrderID }")
                .unwrap();
        let mut evaluator = Evaluator::new();
        let result = evaluator.evaluate(&ast, &data).unwrap();
        // A one-item result sequence unwraps to its item, as in jsonata-js.
        assert_eq!(
            result,
            serde_json::json!({"name": "Hat", "order": "o1"}).into()
        );
    }

    // Regression tests for a bug where create_tuple_stream/evaluate_sort bound
    // a tuple-carried `$name`/`!label` key straight into the top scope and then
    // UNCONDITIONALLY unbound it afterward, deleting (rather than restoring) a
    // same-named outer `:=` binding that happened to be live in that scope
    // frame. Expected values below are verified against jsonata-js (2.2.1
    // reference, `tests/jsonata-js`).

    #[test]
    fn test_chained_focus_bind_does_not_clobber_outer_variable() {
        let data: JValue = serde_json::json!({"a": {"b": {"c": 1}}}).into();
        let ast = crate::parser::parse(r#"($x := "OUT"; a@$x.b@$y.c; $x)"#).unwrap();
        let mut evaluator = Evaluator::new();
        let result = evaluator.evaluate(&ast, &data).unwrap();
        assert_eq!(result, serde_json::json!("OUT").into());
    }

    #[test]
    fn test_chained_index_bind_does_not_clobber_outer_variable() {
        let data: JValue = serde_json::json!({"a": {"b": {"c": 1}}}).into();
        let ast = crate::parser::parse(r#"($x := "OUT"; a#$x.b#$y.c; $x)"#).unwrap();
        let mut evaluator = Evaluator::new();
        let result = evaluator.evaluate(&ast, &data).unwrap();
        assert_eq!(result, serde_json::json!("OUT").into());
    }

    #[test]
    fn test_mixed_focus_and_index_bind_does_not_clobber_outer_variable() {
        let data: JValue = serde_json::json!({"a": {"b": {"c": 1}}}).into();
        let ast = crate::parser::parse(r#"($x := "OUT"; a@$x.b#$y.c; $x)"#).unwrap();
        let mut evaluator = Evaluator::new();
        let result = evaluator.evaluate(&ast, &data).unwrap();
        assert_eq!(result, serde_json::json!("OUT").into());
    }

    #[test]
    fn test_sort_term_tuple_binding_does_not_clobber_outer_variable() {
        let data: JValue = serde_json::json!({"items": [{"v": 3}, {"v": 1}, {"v": 2}]}).into();
        let ast = crate::parser::parse(r#"($x := "OUT"; items@$x.v^(%.v); $x)"#).unwrap();
        let mut evaluator = Evaluator::new();
        let result = evaluator.evaluate(&ast, &data).unwrap();
        assert_eq!(result, serde_json::json!("OUT").into());
    }

    // --- #196: arrays known to be flat ---------------------------------------

    fn eval_flat(expr: &str, data: &str) -> JValue {
        let ast = crate::parser::parse(expr).unwrap();
        let data = JValue::from_json_str(data).unwrap();
        Evaluator::new().evaluate(&ast, &data).unwrap()
    }

    fn flat_of(v: &JValue) -> bool {
        v.as_jarray().expect("an array").is_flat()
    }

    #[test]
    fn test_flat_set_where_known() {
        // A range holds numbers only.
        assert!(flat_of(&eval_flat("1..5", "{}")));
        // A step that flattened nothing.
        assert!(flat_of(&eval_flat("a.b", r#"{"a": [{"b": 1}, {"b": 2}]}"#)));
        // A `$` step over a flat input shares it, flag included.
        assert!(flat_of(&eval_flat("v.$", r#"{"v": [1, 2, 3]}"#)));
        // A constructor over flat parts.
        assert!(flat_of(&eval_flat("[1..3, 4, v]", r#"{"v": [5, 6]}"#)));
    }

    #[test]
    fn test_flat_not_set_over_nested_arrays() {
        // A step that flattened arrays in may hold arrays from deeper down.
        assert!(!flat_of(&eval_flat(
            "a.b",
            r#"{"a": [{"b": [[1]]}, {"b": 2}]}"#
        )));
        // A constructor holding an array, or splicing a non-flat one.
        assert!(!flat_of(&eval_flat("[1, v]", r#"{"v": [[5], 6]}"#)));
        assert!(!flat_of(&eval_flat("[[1, 2], 3]", "{}")));
        // A function building a new array from its arguments.
        assert!(!flat_of(&eval_flat(
            "$append(v, [[9]])",
            r#"{"v": [1, 2]}"#
        )));
    }

    #[test]
    fn test_dollar_step_over_nested_arrays_still_flattens() {
        let data = r#"{"v": [1, 2], "n": [[1, 2], [3, [4]]]}"#;
        let json = |e: &str| eval_flat(e, data).to_json_string().unwrap();
        assert_eq!(json("n.$"), "[1,2,3,[4]]");
        assert_eq!(json("n.$.$"), "[1,2,3,4]");
        assert_eq!(json("$append(v, [[9]]).$"), "[1,2,9]");
        assert_eq!(json("[v, [5]].$"), "[1,2,5]");
        assert_eq!(json("[1..2, n].$"), "[1,2,1,2,3,[4]]");
        assert_eq!(json("$sum([1..1000].$)"), "500500");
    }

    // --- #191: tail loops that make a function on each pass ----------------
    //
    // The tail-call trampoline reuses its frame unless a function that can
    // see it lives on. A function made and dropped on each pass must not
    // keep the frames, or the stack (and memory) grows with the loop.

    fn peak_depth_of(src: &str) -> (JValue, usize) {
        let ast = crate::parser::parse(src).unwrap();
        let mut evaluator = Evaluator::new();
        let result = evaluator.evaluate(&ast, &JValue::Null).unwrap();
        (result, evaluator.context.peak_depth)
    }

    #[test]
    fn tail_loop_making_a_closure_each_pass_keeps_stack_bounded() {
        let (result, depth) = peak_depth_of(
            "($f := function($n, $acc){ ($g := function(){ $n }; \
             $n = 0 ? $acc : $f($n - 1, $acc + $g())) }; $f(50000, 0))",
        );
        assert_eq!(result, JValue::from(1_250_025_000.0));
        assert!(depth < 16, "scope stack reached {depth} frames");
    }

    #[test]
    fn tail_loop_passing_a_lambda_each_pass_keeps_stack_bounded() {
        let (result, depth) = peak_depth_of(
            "($f := function($n, $acc){ ($m := $map([$n], function($v){ $v }); \
             $n = 0 ? $acc : $f($n - 1, $acc + $m)) }; $f(50000, 0))",
        );
        assert_eq!(result, JValue::from(1_250_025_000.0));
        assert!(depth < 16, "scope stack reached {depth} frames");
    }

    #[test]
    fn tail_loop_keeps_frames_of_closures_that_escape() {
        // Each pass's closure goes into the accumulator, so each pass's
        // frame stays; the closures see their own pass's `$n`.
        let (result, depth) = peak_depth_of(
            "($f := function($n, $acc){ ($g := function(){ $n }; \
             $n = 0 ? $map($acc, function($h){ $h() }) : $f($n - 1, $append($acc, [$g]))) }; $f(50, []))",
        );
        let expected: Vec<JValue> = (1..=50).rev().map(|n| JValue::from(n as f64)).collect();
        assert_eq!(result, JValue::array(expected));
        assert!(depth > 50, "scope stack reached only {depth} frames");
    }

    #[test]
    fn tail_loop_dropping_functions_held_in_values_keeps_stack_bounded() {
        // Functions held by arrays, objects, other functions' results and a
        // local tail-recursive maker all go with the pass.
        for body in [
            "$a := [function(){ $n }]",
            "$o := {\"k\": function(){ $n }}",
            "$q := $map([1, 2], function($v){ function(){ $v } })",
            "$g := function(){ $n }; $l := [$g]",
            "$mk := function($k){ $k = 0 ? function(){ $n } : $mk($k - 1) }; $q := $mk(1)",
        ] {
            let src = format!(
                "($f := function($n, $acc){{ ({body}; $n = 0 ? $acc : $f($n - 1, $acc + 1)) }}; \
                 $f(5000, 0))"
            );
            let (result, depth) = peak_depth_of(&src);
            assert_eq!(result, JValue::from(5000.0), "{body}");
            assert!(depth < 16, "{body}: scope stack reached {depth} frames");
        }
    }

    #[test]
    fn tail_loop_dropping_many_functions_keeps_stack_bounded() {
        // Far more functions per pass than any fixed walk budget.
        for body in [
            "$fs := $map([1..1000], function($i){ function(){ $i } })",
            "$u := [{'x': [function(){ 1 }]}, [1..2000].{'a': $}]",
        ] {
            let src = format!(
                "($f := function($n, $acc){{ ({body}; $n = 0 ? $acc : $f($n - 1, $acc + 1)) }}; \
                 $f(100, 0))"
            );
            let (result, depth) = peak_depth_of(&src);
            assert_eq!(result, JValue::from(100.0), "{body}");
            assert!(depth < 16, "{body}: scope stack reached {depth} frames");
        }
    }

    #[test]
    fn tail_called_composition_or_local_function_keeps_stack_bounded() {
        for src in [
            "($inc := function($x){ $x + 1 }; \
             $loop := function($n){ $n >= 20000 ? $n : ($inc ~> $loop)($n) }; $loop(0))",
            "($loop := function($n){ $n >= 20000 ? $n : \
             ($string ~> function($s){ $loop($number($s) + 1) })($n) }; $loop(0))",
            "($loop := function($n){ $n >= 20000 ? $n : (function($s){ $loop($s + 1) })($n) }; \
             $loop(0))",
        ] {
            let (result, depth) = peak_depth_of(src);
            assert_eq!(result, JValue::from(20000.0), "{src}");
            assert!(depth < 16, "{src}: scope stack reached {depth} frames");
        }
    }

    #[test]
    fn kept_frame_is_not_walked_again_each_pass() {
        // The first pass's frame owns a large array and stays for a closure
        // passed along every pass; deciding again on each pass that it
        // stays must not walk the array again.
        let start = std::time::Instant::now();
        let (result, _) = peak_depth_of(
            "($f := function($k, $keep){ ($big := $k = 5000 ? [1..20000] : []; \
             $k = 0 ? $keep() : $f($k - 1, $k = 5000 ? function(){ $k } : $keep)) }; \
             $f(5000, function(){ 0 }))",
        );
        assert_eq!(result, JValue::from(5000.0));
        let elapsed = start.elapsed();
        assert!(elapsed.as_secs() < 3, "took {elapsed:?}");
    }

    #[test]
    fn tail_loop_passing_a_chain_of_closures_stays_linear() {
        // Each pass's closure captures the previous one, so the frames stay
        // (each closure may see its own); deciding that must not walk the
        // chain, or the loop takes quadratic time.
        // Dropping the chain of 20000 closures at the end recurses through
        // it, which a test thread's default stack does not hold.
        let elapsed = std::thread::Builder::new()
            .stack_size(512 << 20)
            .spawn(|| {
                let start = std::time::Instant::now();
                let (result, _) = peak_depth_of(
                    "($f := function($n, $prev){ $n = 0 ? 'done' : $f($n - 1, function(){ $prev() + 1 }) }; \
                     $f(20000, function(){ 0 }))",
                );
                assert_eq!(result, JValue::from("done"));
                start.elapsed()
            })
            .unwrap()
            .join()
            .unwrap();
        assert!(elapsed.as_secs() < 5, "took {elapsed:?}");
    }

    // --- #197: a step's compiled body is cached on the step -----------------

    mod step_cache {
        use super::*;

        fn step_compiles() -> usize {
            STEP_COMPILES.with(|n| n.get())
        }

        fn jv(s: &str) -> JValue {
            JValue::from_json_str(s).unwrap()
        }

        fn big(n: usize) -> JValue {
            let items: Vec<String> = (0..n)
                .map(|i| {
                    format!(
                        r#"{{"items": [{{"x": {i}, "y": "a"}}, {{"x": {}, "y": "b"}}, {{"x": 0, "y": "c"}}]}}"#,
                        i + 1
                    )
                })
                .collect();
            jv(&format!(r#"{{"big": [{}]}}"#, items.join(",")))
        }

        fn assert_map_rows(result: &JValue) {
            let JValue::Array(rows) = result else {
                panic!("expected an array, got {result:?}")
            };
            assert_eq!(rows.len(), 200);
            assert_eq!(
                rows[1],
                jv(r#"[{"a": 1, "b": "a"}, {"a": 2, "b": "b"}, {"a": 0, "b": "c"}]"#)
            );
        }

        #[test]
        fn constructor_in_a_function_body_compiles_once_per_evaluator() {
            // Before #197 the step compiled once per call: 200 times per
            // evaluation. Now once per evaluator, for an inline function and
            // for a stored one, whose body is a clone with the same step ids.
            let data = big(200);
            for src in [
                r#"$map(big, function($v){ $v.items.{"a": x, "b": y} })"#,
                r#"( $f := function($v){ $v.items.{"a": x, "b": y} }; $map(big, $f) )"#,
                r#"( $f := function($v){ $v.items.{"a": x, "b": y} }; $map(big, function($v){ $f($v) }) )"#,
            ] {
                let ast = parser::parse(src).unwrap();
                for _ in 0..2 {
                    let before = step_compiles();
                    assert_map_rows(&Evaluator::new().evaluate(&ast, &data).unwrap());
                    assert_eq!(step_compiles() - before, 1, "{src}");
                }
                // One evaluator over several evaluations keeps its cache.
                let mut evaluator = Evaluator::new();
                let before = step_compiles();
                for _ in 0..3 {
                    assert_map_rows(&evaluator.evaluate(&ast, &data).unwrap());
                }
                assert_eq!(step_compiles() - before, 1, "{src}");
            }
        }

        #[test]
        fn alternating_steps_each_compile_once() {
            // Two cached steps run in turn, 200 times each: the second one
            // moves the first out of the last-used slot into the map.
            let data = big(200);
            let ast = parser::parse(
                r#"$map(big, function($v){ [$v.items.{"a": x}, $v.items.("b" & y)] })"#,
            )
            .unwrap();
            let before = step_compiles();
            let result = Evaluator::new().evaluate(&ast, &data).unwrap();
            assert_eq!(step_compiles() - before, 2);
            let JValue::Array(rows) = &result else {
                panic!("expected an array, got {result:?}")
            };
            assert_eq!(
                rows[1],
                jv(r#"[{"a": 1}, {"a": 2}, {"a": 0}, "ba", "bb", "bc"]"#)
            );
        }

        #[test]
        fn cache_is_bounded() {
            // `$eval` parses new steps each time; the cache drops its
            // entries rather than growing without limit.
            let data = jv(r#"{"p": [{"a": 1}, {"a": 2}]}"#);
            let mut evaluator = Evaluator::new();
            let ast = parser::parse(r#"[1..5000].$eval('p.{"a": a}', $$)"#).unwrap();
            let before = step_compiles();
            let result = evaluator.evaluate(&ast, &data).unwrap();
            // Each parsed `p.{...}` once, and the outer `.$eval(...)` step.
            assert_eq!(step_compiles() - before, 5001);
            let JValue::Array(rows) = &result else {
                panic!("expected an array, got {result:?}")
            };
            assert_eq!(rows.len(), 10000);
            let cached = evaluator.step_bodies.len();
            assert!(cached > 0 && cached <= MAX_STEP_BODIES + 1, "{cached}");
        }

        #[test]
        fn each_parsed_step_has_its_own_id() {
            // Two steps sharing an id would share a cache entry.
            let ast = parser::parse("a.b.c").unwrap();
            let AstNode::Path { steps } = &ast else {
                panic!("expected a path, got {ast:?}")
            };
            let ids: std::collections::HashSet<u64> =
                steps.iter().map(|step| step.id.get()).collect();
            assert_eq!(ids.len(), steps.len());
        }

        #[test]
        fn top_level_step_compiles_once_per_evaluator() {
            let data =
                jv(r#"{"products": [{"name": "a", "price": 1}, {"name": "b", "price": 2}]}"#);
            let ast = parser::parse(r#"products.{"n": name, "p": price}"#).unwrap();
            let mut evaluator = Evaluator::new();
            let before = step_compiles();
            for _ in 0..3 {
                assert_eq!(
                    evaluator.evaluate(&ast, &data).unwrap(),
                    jv(r#"[{"n": "a", "p": 1}, {"n": "b", "p": 2}]"#)
                );
            }
            assert_eq!(step_compiles() - before, 1);
        }

        #[test]
        fn step_that_does_not_compile_is_tried_once() {
            // `$f` is neither a parameter nor compilable, so the step runs on
            // the tree-walker; the failed compile is remembered too.
            let data = big(50);
            let ast = parser::parse(
                r#"( $f := function($x){ $x & "!" }; $map(big, function($v){ $v.items.{"a": $f(y)} }) )"#,
            )
            .unwrap();
            let before = step_compiles();
            let result = Evaluator::new().evaluate(&ast, &data).unwrap();
            assert_eq!(step_compiles() - before, 1);
            let JValue::Array(rows) = &result else {
                panic!("expected an array, got {result:?}")
            };
            assert_eq!(rows[0], jv(r#"[{"a": "a!"}, {"a": "b!"}, {"a": "c!"}]"#));
        }

        #[test]
        fn shadowing_is_checked_on_every_use_of_the_cached_step() {
            // The same cached step runs with `$string` the builtin, then bound
            // to a function in the call's own block, then the builtin again
            // (values from jsonata-js 2.x). Compiled once all the same.
            let data = jv(r#"{"d": {"items": [{"x": 1}, {"x": 2}, {"x": 3}]}}"#);
            for (body, constructs) in [
                (r#"$v.items.{"s": $string(x)}"#, true),
                (r#"$v.items.($string(x))"#, false),
            ] {
                for order in [[false, true, false], [true, false, true]] {
                    let calls: Vec<String> = order.iter().map(|s| format!("$f(d, {s})")).collect();
                    let src = format!(
                        r#"( $f := function($v, $s){{ ( $s ? $string := function($x){{ "z" }} : 0; {body} ) }}; [{}] )"#,
                        calls.join(", ")
                    );
                    let ast = parser::parse(&src).unwrap();
                    let before = step_compiles();
                    let result = Evaluator::new().evaluate(&ast, &data).unwrap();
                    assert_eq!(step_compiles() - before, 1, "{src}");
                    let expected: Vec<JValue> = order
                        .iter()
                        .flat_map(|&shadowed| {
                            ["1", "2", "3"].map(|x| if shadowed { "z" } else { x })
                        })
                        .map(|s| {
                            if constructs {
                                jv(&format!(r#"{{"s": "{s}"}}"#))
                            } else {
                                JValue::from(s)
                            }
                        })
                        .collect();
                    assert_eq!(result, JValue::array(expected), "{src}");
                }
            }
        }
    }
}

// Expression evaluator
// Mirrors jsonata.js from the reference implementation

#![allow(clippy::cloned_ref_to_slice_refs)]
#![allow(clippy::explicit_counter_loop)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::manual_strip)]

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::ast::{AstNode, BinaryOp, PathStep, Stage};
use crate::parser;
use crate::value::JValue;
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
    FieldLookup(String),
    /// Two-level nested field lookup: `obj.get("a")?.get("b")`.
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
    /// Named variable lookup from context scope (any `$name` not in lambda params).
    /// Compiled when a named variable is encountered and no allowed_vars list is
    /// provided (top-level compilation). At runtime, returns the value from the vars
    /// map (lambda params or captured env), or Undefined if not present.
    #[allow(dead_code)]
    ContextVar(String),

    /// Multi-step field path with optional per-step filters: `a.b[pred].c`
    /// Applies implicit array-mapping semantics at each step.
    FieldPath(Vec<CompiledStep>),

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
    /// Optional predicate filter, from either a `Stage::Filter` stage or a
    /// folded-in standalone `Predicate` step.
    pub filter: Option<CompiledExpr>,
    /// True when `filter` came from a standalone `Predicate` step (`arr[p]`)
    /// rather than a `Stage::Filter` (`a.b[-1]`). The two are NOT
    /// interchangeable for numeric predicates: a standalone predicate matches
    /// each element against its own index, while a stage filter maps the index
    /// over each extracted sub-array, so `foo.blah.baz.fud[-1]` takes the last
    /// of every group. Boolean predicates behave identically either way.
    pub filter_selects_by_index: bool,
}

/// Try to compile an AstNode subtree into a CompiledExpr.
/// Returns None for anything that requires full AST evaluation (lambda calls,
/// function calls with side effects, complex paths, etc.).
pub(crate) fn try_compile_expr(node: &AstNode) -> Option<CompiledExpr> {
    reject_if_too_large_to_pool(try_compile_expr_inner(node, None)?)
}

/// Like `try_compile_expr` but additionally allows the specified variable names
/// to be compiled as `VariableLookup`. Used by HOF integration where lambda
/// parameters are known and will be provided via the `vars` map at eval time.
pub(crate) fn try_compile_expr_with_allowed_vars(
    node: &AstNode,
    allowed_vars: &[&str],
) -> Option<CompiledExpr> {
    reject_if_too_large_to_pool(try_compile_expr_inner(node, Some(allowed_vars))?)
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
            | CompiledExpr::VariableLookup(_)
            | CompiledExpr::ContextVar(_) => false,

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
            CompiledExpr::FieldPath(steps) => {
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

fn try_compile_expr_inner(node: &AstNode, allowed_vars: Option<&[&str]>) -> Option<CompiledExpr> {
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
        // In top-level mode (allowed_vars=None), compile unknown variables as
        // ContextVar — they return Undefined at runtime when no bindings are passed.
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
            if pairs.len() > u16::MAX as usize {
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
                let is_nested = matches!(elem, AstNode::Array(_));
                compiled.push((try_compile_expr_inner(elem, allowed_vars)?, is_nested));
            }
            Some(CompiledExpr::ArrayConstruct(compiled))
        }

        // ── Block (sequential evaluation) ───────────────────────────────
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
    let param_refs: Vec<&str> = params.iter().map(|s| s.as_str()).collect();
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

fn eval_compiled_inner(
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
            JValue::Object(obj) => {
                // Shape-accelerated: use positional index if available
                if let Some(shape) = shape {
                    if let Some(&idx) = shape.get(field.as_str()) {
                        return Ok(obj
                            .get_index(idx)
                            .map(|(_, v)| v.clone())
                            .unwrap_or(JValue::Undefined));
                    }
                }
                Ok(obj
                    .get(field.as_str())
                    .cloned()
                    .unwrap_or(JValue::Undefined))
            }
            #[cfg(feature = "python")]
            JValue::LazyPyDict(lazy) => Ok(lazy.get_field(field)?),
            _ => Ok(JValue::Undefined),
        },

        CompiledExpr::NestedFieldLookup(outer, inner) => match data {
            JValue::Object(obj) => {
                // Shape-accelerated outer lookup
                let outer_val = if let Some(shape) = shape {
                    if let Some(&idx) = shape.get(outer.as_str()) {
                        obj.get_index(idx).map(|(_, v)| v)
                    } else {
                        obj.get(outer.as_str())
                    }
                } else {
                    obj.get(outer.as_str())
                };
                match outer_val {
                    Some(JValue::Object(nested)) => Ok(nested
                        .get(inner.as_str())
                        .cloned()
                        .unwrap_or(JValue::Undefined)),
                    #[cfg(feature = "python")]
                    Some(JValue::LazyPyDict(nested)) => Ok(nested.get_field(inner.as_str())?),
                    _ => Ok(JValue::Undefined),
                }
            }
            #[cfg(feature = "python")]
            JValue::LazyPyDict(lazy) => {
                let outer_val = lazy.get_field(outer.as_str())?;
                match outer_val {
                    JValue::Object(nested) => Ok(nested
                        .get(inner.as_str())
                        .cloned()
                        .unwrap_or(JValue::Undefined)),
                    JValue::LazyPyDict(nested) => Ok(nested.get_field(inner.as_str())?),
                    _ => Ok(JValue::Undefined),
                }
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
                return Ok(data.clone());
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
            let mut result = IndexMap::with_capacity(fields.len());
            for (key, expr) in fields {
                let value = eval_compiled_inner(expr, data, vars, ctx, shape, options, start_time)?;
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

        // ContextVar: named variable lookup from context scope.
        // In top-level mode (ctx=None, no bindings), returns Undefined.
        // In HOF mode, ctx is None too (HOF call sites pass no ctx), so this
        // is only ever populated for top-level calls — always Undefined there.
        CompiledExpr::ContextVar(name) => {
            // Check vars map first (for lambda params that might shadow context)
            if let Some(vars) = vars {
                if let Some(val) = vars.get(name.as_str()) {
                    return Ok((*val).clone());
                }
            }
            // Then check context scope
            if let Some(ctx) = ctx {
                if let Some(val) = ctx.lookup(name) {
                    return Ok(val.clone());
                }
            }
            Ok(JValue::Undefined)
        }

        // FieldPath: multi-step field access with implicit array mapping.
        CompiledExpr::FieldPath(steps) => {
            compiled_eval_field_path(steps, data, vars, ctx, shape, options, start_time)
        }

        // BuiltinCall: evaluate all args, dispatch to pure builtin.
        CompiledExpr::BuiltinCall { name, args } => {
            let mut evaled_args = Vec::with_capacity(args.len());
            for arg in args.iter() {
                evaled_args.push(eval_compiled_inner(
                    arg, data, vars, ctx, shape, options, start_time,
                )?);
            }
            crate::builtins::dispatch_pure(name, &evaled_args, data, options, false)
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

/// Truthiness check (matches JSONata semantics). Standalone function for compiled path.
#[inline]
pub(crate) fn compiled_is_truthy(value: &JValue) -> bool {
    match value {
        JValue::Null | JValue::Undefined => false,
        JValue::Bool(b) => *b,
        JValue::Number(n) => *n != 0.0,
        JValue::String(s) => !s.is_empty(),
        // Recursive, matching `Evaluator::is_truthy` and `$boolean` (#111).
        JValue::Array(a) => match a.len() {
            0 => false,
            1 => compiled_is_truthy(&a[0]),
            _ => a.iter().any(compiled_is_truthy),
        },
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
    let field_steps: &[crate::ast::PathStep] = match &steps[0].node {
        AstNode::Variable(var) if var.is_empty() && steps[0].stages.is_empty() => &steps[1..],
        AstNode::Variable(_) => return None,
        AstNode::Name(_) => steps,
        _ => return None,
    };

    // Compile a boolean filter predicate, rejecting numeric predicates (`[0]`, `[1]`)
    // which represent index access in JSONata, not boolean filtering, and the
    // explicit `[]` keep-array marker (`Boolean(true)`), which forces the result
    // to stay an array rather than filtering — the tree-walker's
    // `evaluate_predicate` special-cases it and the compiled path has no
    // equivalent, so bail out rather than silently treating it as `filter(true)`.
    let compile_filter = |node: &AstNode| -> Option<CompiledExpr> {
        let is_numeric_literal = match node {
            AstNode::Number(_) => true,
            // `[-1]` parses as a negation of a literal, not a negative literal.
            // Missing this let it compile as a plain truthy constant, so
            // `arr.p[-1]` kept every element instead of taking the last of each
            // extracted group.
            AstNode::Unary { op, operand } => {
                matches!(op, crate::ast::UnaryOp::Negate) && matches!(**operand, AstNode::Number(_))
            }
            _ => false,
        };
        if is_numeric_literal || matches!(node, AstNode::Boolean(true)) {
            return None;
        }
        try_compile_expr_inner(node, allowed_vars)
    };

    // Compile each field step.
    // Handles:
    //   - Name nodes with at most one Stage::Filter attached (from `a.b[pred]` dot-path parsing)
    //   - Predicate nodes (from `products[pred]` standalone predicate parsing) — folded into the
    //     previous step's filter slot, flagged with `filter_selects_by_index`. The two encodings
    //     are NOT interchangeable: for a numeric predicate a standalone step matches each element
    //     against its own index, while a stage filter maps the index over each extracted
    //     sub-array. They coincide only for boolean predicates.
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
                    filter_selects_by_index: false,
                });
            }
            AstNode::Predicate(filter_node) => {
                // Standalone predicate step — fold into the previous Name step's filter slot.
                if !step.stages.is_empty() {
                    return None;
                }
                let last = compiled_steps.last_mut()?;
                if last.filter.is_some() {
                    return None;
                }
                last.filter = Some(compile_filter(filter_node)?);
                last.filter_selects_by_index = true;
            }
            _ => return None,
        }
    }

    if compiled_steps.is_empty() {
        // Bare `$` with no further field steps — current-data reference
        return Some(CompiledExpr::VariableLookup(String::new()));
    }

    // Shape-cache optimizations (FieldLookup / NestedFieldLookup) are only safe
    // in HOF mode (allowed_vars=Some), where data is always a single Object element
    // from an array. In top-level mode (allowed_vars=None), data can itself be an
    // Array, so we must use FieldPath which applies implicit array-mapping semantics.
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

    Some(CompiledExpr::FieldPath(compiled_steps))
}

/// Evaluate a compiled `FieldPath` against `data`.
///
/// Applies implicit array-mapping semantics at each step (matching the tree-walker).
/// Filters are applied as predicates: truthy elements are kept.
///
/// Singleton unwrapping mirrors the tree-walker's `did_array_mapping` rule:
/// - Extracting a field from an *array* sets the mapping flag (unwrap singletons at end).
/// - Extracting a field from a *single object* resets the flag (preserve the raw value).
fn compiled_eval_field_path(
    steps: &[CompiledStep],
    data: &JValue,
    vars: Option<&HashMap<&str, &JValue>>,
    ctx: Option<&Context>,
    shape: Option<&ShapeCache>,
    options: &EvaluatorOptions,
    start_time: Option<Instant>,
) -> Result<JValue, EvaluatorError> {
    let mut current = data.clone();
    // Track whether the most recent field step mapped over an array (like the tree-walker's
    // `did_array_mapping` flag). Filters also count as array operations.
    let mut did_array_mapping = false;
    for step in steps {
        // Determine if this step will do array mapping before we overwrite `current`
        let is_array = matches!(current, JValue::Array(_));
        // Field access with implicit array mapping
        current = compiled_field_step(&step.field, &current, options)?;
        if is_array {
            did_array_mapping = true;
        } else {
            // Extracting from a single object resets the flag (tree-walker parity)
            did_array_mapping = false;
        }
        // Apply filter if present (filter is an array operation — keep the flag set)
        if let Some(filter) = &step.filter {
            current = compiled_apply_filter(
                filter,
                step.filter_selects_by_index,
                &current,
                vars,
                ctx,
                shape,
                options,
                start_time,
            )?;
            // Filter always implies we operated on an array
            did_array_mapping = true;
        }
    }
    // Singleton unwrapping: only when array-mapping occurred, matching tree-walker.
    if did_array_mapping {
        Ok(match current {
            JValue::Array(ref arr) if arr.len() == 1 => arr[0].clone(),
            other => other,
        })
    } else {
        Ok(current)
    }
}

/// Perform a single-field access with implicit array-mapping semantics.
///
/// - Object: look up `field`, return its value or Undefined
/// - Array: map field extraction over each element, flatten nested arrays, skip Undefined
///   (this is a query-result sequence, so D2015 applies — mirrors `evaluate_path`'s
///   array-mapping check and `vm.rs`'s `get_field_cached`)
/// - Tuple objects (`__tuple__: true`): look up in the `@` inner object
/// - Other: Undefined
fn compiled_field_step(
    field: &str,
    value: &JValue,
    options: &EvaluatorOptions,
) -> Result<JValue, EvaluatorError> {
    match value {
        JValue::Object(obj) => {
            // Check for tuple: extract from "@" inner object
            if obj.get("__tuple__") == Some(&JValue::Bool(true)) {
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
            // Build shape cache from first plain (non-tuple) object for O(1) positional access.
            let shape: Option<ShapeCache> = arr.iter().find_map(|v| {
                if let JValue::Object(obj) = v {
                    if obj.get("__tuple__") != Some(&JValue::Bool(true)) {
                        return build_shape_cache(v);
                    }
                }
                None
            });
            let mut result = Vec::new();
            for item in arr.iter() {
                let extracted = if let (Some(ref sh), JValue::Object(obj)) = (&shape, item) {
                    // Tuple objects need the recursive path for "@" inner lookup.
                    if obj.get("__tuple__") == Some(&JValue::Bool(true)) {
                        compiled_field_step(field, item, options)?
                    } else if let Some(&pos) = sh.get(field) {
                        // Positional access with key verification: guards against heterogeneous
                        // schemas (objects where the same field is at a different index).
                        // On a mismatch, fall back to a regular hash lookup.
                        match obj.get_index(pos) {
                            Some((k, v)) if k.as_str() == field => v.clone(),
                            _ => obj.get(field).cloned().unwrap_or(JValue::Undefined),
                        }
                    } else {
                        // Field not in the first object's schema — fall back to hash lookup
                        // so that heterogeneous arrays (e.g. [{a:1},{b:2}]) are handled correctly.
                        obj.get(field).cloned().unwrap_or(JValue::Undefined)
                    }
                } else {
                    compiled_field_step(field, item, options)?
                };
                match extracted {
                    JValue::Undefined => {}
                    JValue::Array(inner) => result.extend(inner.iter().cloned()),
                    other => result.push(other),
                }
            }
            check_sequence_length(result.len(), options)?;
            Ok(if result.is_empty() {
                JValue::Undefined
            } else {
                JValue::array(result)
            })
        }
        _ => Ok(JValue::Undefined),
    }
}

/// Apply a compiled filter predicate to a value.
///
/// - Array: return elements for which the predicate is truthy
/// - Single value: return it if predicate is truthy, else Undefined
/// - Numeric predicates (index access) are NOT supported here — fall back via None compilation
#[allow(clippy::too_many_arguments)]
fn compiled_apply_filter(
    filter: &CompiledExpr,
    selects_by_index: bool,
    value: &JValue,
    vars: Option<&HashMap<&str, &JValue>>,
    ctx: Option<&Context>,
    shape: Option<&ShapeCache>,
    options: &EvaluatorOptions,
    start_time: Option<Instant>,
) -> Result<JValue, EvaluatorError> {
    match value {
        JValue::Array(arr) => {
            let mut result = Vec::new();
            // Auto-build shape cache from first element when not provided.
            // Avoids per-element hash lookups in the filter predicate for homogeneous arrays.
            let local_shape: Option<ShapeCache> = if shape.is_none() {
                arr.first().and_then(build_shape_cache)
            } else {
                None
            };
            let effective_shape = shape.or(local_shape.as_ref());
            // A standalone predicate matches each element against its own index;
            // a stage filter maps a numeric predicate over each extracted
            // sub-array instead, so `foo.blah.baz.fud[-1]` takes the last of
            // every group rather than one element of the flattened sequence.
            let len = arr.len();
            for (index, item) in arr.iter().enumerate() {
                check_loop_timeout(options, start_time)?;
                let pred = eval_compiled_inner(
                    filter,
                    item,
                    vars,
                    ctx,
                    effective_shape,
                    options,
                    start_time,
                )?;
                let repeats = match predicate_index_match(&pred, index, len) {
                    Some(n) if selects_by_index => n,
                    _ => usize::from(compiled_is_truthy(&pred)),
                };
                for _ in 0..repeats {
                    result.push(item.clone());
                }
            }
            if result.is_empty() {
                Ok(JValue::Undefined)
            } else if result.len() == 1 {
                check_sequence_length(1, options)?;
                Ok(result.remove(0))
            } else {
                check_sequence_length(result.len(), options)?;
                Ok(JValue::array(result))
            }
        }
        JValue::Undefined => Ok(JValue::Undefined),
        _ => {
            // A non-array is a singleton sequence: index 0, length 1.
            let pred = eval_compiled_inner(filter, value, vars, ctx, shape, options, start_time)?;
            let repeats = match predicate_index_match(&pred, 0, 1) {
                Some(n) if selects_by_index => n,
                _ => usize::from(compiled_is_truthy(&pred)),
            };
            // A selector can name position 0 more than once, and a singleton
            // sequence repeats like any other: `num[[0,0]]` is `[5, 5]`.
            if repeats > 1 {
                Ok(JValue::array(vec![value.clone(); repeats]))
            } else if repeats == 1 {
                Ok(value.clone())
            } else {
                Ok(JValue::Undefined)
            }
        }
    }
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

/// Result of evaluating a lambda body that may be a tail call
/// Used for trampoline-based tail call optimization
enum LambdaResult {
    /// Final value - evaluation is complete
    JValue(JValue),
    /// Tail call - need to continue with another lambda invocation
    TailCall {
        /// The lambda to call (boxed to reduce enum size)
        lambda: Box<StoredLambda>,
        /// Arguments for the call
        args: Vec<JValue>,
        /// Data context for the call
        data: JValue,
    },
}

/// Lambda storage
/// Stores the AST of a lambda function along with its parameters, optional signature,
/// and captured environment for closures
#[derive(Clone, Debug)]
pub struct StoredLambda {
    pub params: Vec<String>,
    pub body: AstNode,
    /// Pre-compiled body for use in tight inner loops (HOF fast path).
    /// `None` if the body is not compilable (transform, partial-app, thunk, etc.).
    pub(crate) compiled_body: Option<CompiledExpr>,
    pub signature: Option<String>,
    /// Captured environment bindings for closures
    pub captured_env: HashMap<String, JValue>,
    /// Captured data context for lexical scoping of bare field names
    pub captured_data: Option<JValue>,
    /// Whether this lambda's body contains tail calls that can be optimized
    pub thunk: bool,
}

/// A single scope in the scope stack
struct Scope {
    bindings: HashMap<String, JValue>,
    lambdas: HashMap<String, Rc<StoredLambda>>,
}

impl Scope {
    fn new() -> Self {
        Scope {
            bindings: HashMap::new(),
            lambdas: HashMap::new(),
        }
    }
}

/// Evaluation context
///
/// Holds variable bindings and other state needed during evaluation.
/// Uses a scope stack for efficient push/pop instead of clone/restore.
pub struct Context {
    scope_stack: Vec<Scope>,
    parent_data: Option<JValue>,
}

impl Context {
    pub fn new() -> Self {
        Context {
            scope_stack: vec![Scope::new()],
            parent_data: None,
        }
    }

    /// Push a new scope onto the stack
    fn push_scope(&mut self) {
        self.scope_stack.push(Scope::new());
    }

    /// Pop the top scope from the stack
    fn pop_scope(&mut self) {
        if self.scope_stack.len() > 1 {
            self.scope_stack.pop();
        }
    }

    /// Pop scope but preserve specified lambdas by moving them to the current
    /// top scope. The side table holds `Rc<StoredLambda>`, so migration is a
    /// refcount bump, not a deep clone of the lambda's body AST.
    fn pop_scope_preserving_lambdas(&mut self, lambda_ids: &[String]) {
        if self.scope_stack.len() > 1 {
            let popped = self.scope_stack.pop().unwrap();
            if !lambda_ids.is_empty() {
                let top = self.scope_stack.last_mut().unwrap();
                for id in lambda_ids {
                    if let Some(stored) = popped.lambdas.get(id) {
                        top.lambdas.insert(id.clone(), Rc::clone(stored));
                    }
                }
            }
        }
    }

    /// Clear all bindings and lambdas in the top scope without deallocating
    fn clear_current_scope(&mut self) {
        let top = self.scope_stack.last_mut().unwrap();
        top.bindings.clear();
        top.lambdas.clear();
    }

    pub fn bind(&mut self, name: String, value: JValue) {
        self.scope_stack
            .last_mut()
            .unwrap()
            .bindings
            .insert(name, value);
    }

    pub fn bind_lambda(&mut self, name: String, lambda: StoredLambda) {
        self.bind_lambda_rc(name, Rc::new(lambda));
    }

    /// Bind an already-shared lambda (e.g. one migrating up a popped scope).
    fn bind_lambda_rc(&mut self, name: String, lambda: Rc<StoredLambda>) {
        self.scope_stack
            .last_mut()
            .unwrap()
            .lambdas
            .insert(name, lambda);
    }

    pub fn unbind(&mut self, name: &str) {
        // Remove from top scope only
        let top = self.scope_stack.last_mut().unwrap();
        top.bindings.remove(name);
        top.lambdas.remove(name);
    }

    pub fn lookup(&self, name: &str) -> Option<&JValue> {
        // Walk scope stack from top to bottom
        for scope in self.scope_stack.iter().rev() {
            if let Some(value) = scope.bindings.get(name) {
                return Some(value);
            }
        }
        None
    }

    pub fn lookup_lambda(&self, name: &str) -> Option<&StoredLambda> {
        self.lookup_lambda_rc(name).map(|rc| rc.as_ref())
    }

    /// Like `lookup_lambda`, but hands out the shared handle (an Rc clone is
    /// a refcount bump — this is what makes by-reference invocation and
    /// scope-pop preservation cheap).
    fn lookup_lambda_rc(&self, name: &str) -> Option<&Rc<StoredLambda>> {
        // Walk scope stack from top to bottom
        for scope in self.scope_stack.iter().rev() {
            if let Some(lambda) = scope.lambdas.get(name) {
                return Some(lambda);
            }
        }
        None
    }

    pub fn set_parent(&mut self, data: JValue) {
        self.parent_data = Some(data);
    }

    pub fn get_parent(&self) -> Option<&JValue> {
        self.parent_data.as_ref()
    }

    /// Collect all bindings across all scopes (for environment capture).
    /// Higher scopes shadow lower scopes.
    fn all_bindings(&self) -> HashMap<String, JValue> {
        let mut result = HashMap::new();
        for scope in &self.scope_stack {
            for (k, v) in &scope.bindings {
                result.insert(k.clone(), v.clone());
            }
        }
        result
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
    match value {
        JValue::Object(obj) if obj.get("__tuple__") == Some(&JValue::Bool(true)) => obj
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
        JValue::Array(arr) => JValue::array(arr.iter().cloned().map(unwrap_tuple_output).collect()),
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
    /// True if `name` was one of the keys this guard bound (used by callers
    /// that need to know whether a given tuple key is already in scope
    /// before binding it a second time under a different role, e.g.
    /// `create_tuple_stream`'s ancestor-label handling).
    fn contains(&self, name: &str) -> bool {
        self.saved.iter().any(|(n, _)| n == name)
    }

    fn restore(self, evaluator: &mut Evaluator) {
        for (name, prior) in self.saved {
            match prior {
                Some(value) => evaluator.context.bind(name, value),
                None => evaluator.context.unbind(&name),
            }
        }
    }
}

/// Resource-limit guardrails, mirroring jsonata-js 2.2.1's `timeout`/`stack`/`sequence`
/// evaluator options. All fields default to `None` = unlimited (current behavior).
#[derive(Default, Clone, Debug)]
pub struct EvaluatorOptions {
    /// Maximum wall-clock evaluation time in milliseconds. Exceeding it raises D1012.
    pub timeout_ms: Option<u64>,
    /// Maximum AST-recursion stack depth. Exceeding it raises D1011 if this is the
    /// tighter of this value and the hardcoded native-stack safety ceiling (302);
    /// otherwise the hardcoded ceiling still raises U1001 (see GitHub issue #34).
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

/// Checks a constructed query-result sequence's length against the configured
/// `max_sequence_length` guardrail. Call this at sites that build a query-result
/// sequence (map/filter/wildcard/descendants/keys/lookup/append/spread/each/range/
/// path-mapping). NOT currently called at literal array construction (`[1,2,3]`) —
/// unlike upstream jsonata-js, which caps flat/non-nested literals too via
/// `fn.append`'s `createSequence()` hook. See `EvaluatorOptions::max_sequence_length`
/// doc comment above for why this is a deliberate, temporary gap.
/// Wrap a higher-order function's results as a JSONata *sequence*.
///
/// `$map`/`$filter` do not return arrays, they return sequences: an empty
/// result is undefined rather than `[]`, and a single result unwraps to that
/// result. Returning a bare `Vec` from each of the six exit points is what let
/// these drift from `$map(arr, ...)` producing `["free"]` where jsonata-js
/// produces `"free"`.
pub(crate) fn hof_result_sequence(
    mut result: Vec<JValue>,
    options: &EvaluatorOptions,
) -> Result<JValue, EvaluatorError> {
    match result.len() {
        0 => Ok(JValue::Undefined),
        1 => Ok(result.pop().expect("len checked")),
        n => {
            check_sequence_length(n, options)?;
            Ok(JValue::array(result))
        }
    }
}

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

/// Per-iteration D1012 check for loop-based compiled/VM constructs (map/filter/
/// reduce element loops, FilterByBytecode) that don't pass through
/// `evaluate_internal`'s per-node checkpoint and would otherwise run untimed.
#[inline]
pub(crate) fn check_loop_timeout(
    options: &EvaluatorOptions,
    start_time: Option<Instant>,
) -> Result<(), EvaluatorError> {
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

/// Evaluator for JSONata expressions
pub struct Evaluator {
    context: Context,
    recursion_depth: usize,
    max_recursion_depth: usize,
    /// Monotonic counter for generating unique lambda IDs. Each evaluation of a
    /// Lambda AST node creates a new closure *instance* and must get a fresh ID -
    /// using the AST node's pointer address (as before) collided whenever the same
    /// lambda expression was evaluated more than once (e.g. each level of Y-combinator
    /// or other repeated recursion), aliasing unrelated closures that shared an id.
    next_lambda_id: u64,
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
    options: EvaluatorOptions,
    /// Set in `evaluate()` (only when `options.timeout_ms` is configured) and
    /// checked in `evaluate_internal`'s per-node checkpoint for D1012.
    start_time: Option<Instant>,
    /// Host-registered custom functions, dispatched by name from the call
    /// position (`$name(...)`). Empty for the overwhelming majority of
    /// evaluators, which pay nothing. See `register_fn`/`register_fn_override`.
    host_fns: HashMap<String, Rc<dyn HostFn>>,
}

impl Evaluator {
    pub fn new() -> Self {
        Evaluator {
            context: Context::new(),
            recursion_depth: 0,
            // Limit recursion depth to prevent stack overflow
            // True TCO would allow deeper recursion but requires parser-level thunk marking
            max_recursion_depth: 302,
            next_lambda_id: 0,
            tuple_stream_created: false,
            keep_tuple_stream: false,
            options: EvaluatorOptions::default(),
            start_time: None,
            host_fns: HashMap::new(),
        }
    }

    pub fn with_context(context: Context) -> Self {
        Evaluator {
            context,
            recursion_depth: 0,
            max_recursion_depth: 302,
            next_lambda_id: 0,
            tuple_stream_created: false,
            keep_tuple_stream: false,
            options: EvaluatorOptions::default(),
            start_time: None,
            host_fns: HashMap::new(),
        }
    }

    /// Construct an `Evaluator` with guardrail options. `Evaluator::new()`/
    /// `with_context()` remain unchanged (unlimited options) for existing callers.
    pub fn with_options(context: Context, options: EvaluatorOptions) -> Self {
        Evaluator {
            context,
            recursion_depth: 0,
            max_recursion_depth: 302,
            next_lambda_id: 0,
            tuple_stream_created: false,
            keep_tuple_stream: false,
            options,
            start_time: None,
            host_fns: HashMap::new(),
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

    /// Allocate a fresh, process-unique-per-Evaluator id for a new lambda instance.
    fn fresh_lambda_id(&mut self) -> u64 {
        let id = self.next_lambda_id;
        self.next_lambda_id += 1;
        id
    }

    /// Invoke a stored lambda with its captured environment and data.
    /// This is the standard way to call a StoredLambda, handling the
    /// captured_env and captured_data extraction boilerplate.
    fn invoke_stored_lambda(
        &mut self,
        stored: &StoredLambda,
        args: &[JValue],
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        // Compiled fast path: skip scope push/pop and tree-walking for simple lambdas.
        // Conditions: has compiled body, no signature (can't skip validation), no thunk,
        // and no captured lambda/builtin values (those require Context for runtime lookup).
        if let Some(ref ce) = stored.compiled_body {
            if stored.signature.is_none()
                && !stored.thunk
                && !stored
                    .captured_env
                    .values()
                    .any(|v| matches!(v, JValue::Lambda { .. } | JValue::Builtin { .. }))
            {
                let call_data = stored.captured_data.as_ref().unwrap_or(data);
                let vars: HashMap<&str, &JValue> = stored
                    .params
                    .iter()
                    .zip(args.iter())
                    .map(|(p, v)| (p.as_str(), v))
                    .chain(stored.captured_env.iter().map(|(k, v)| (k.as_str(), v)))
                    .collect();
                return eval_compiled(ce, call_data, Some(&vars), &self.options, self.start_time);
            }
        }

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

    /// Look up a StoredLambda from a JValue that may be a lambda marker.
    /// Returns the cloned StoredLambda if the value is a JValue::Lambda variant
    /// with a valid lambda_id that references a stored lambda.
    fn lookup_lambda_from_value(&self, value: &JValue) -> Option<Rc<StoredLambda>> {
        if let JValue::Lambda { lambda_id, .. } = value {
            return self.context.lookup_lambda_rc(lambda_id).cloned();
        }
        None
    }

    /// Get the number of parameters a callback function expects by inspecting its AST.
    /// This is used to avoid passing unnecessary arguments to callbacks in HOF functions.
    /// Returns the parameter count, or usize::MAX if unable to determine (meaning pass all args).
    fn get_callback_param_count(&self, func_node: &AstNode) -> usize {
        match func_node {
            AstNode::Lambda { params, .. } => params.len(),
            AstNode::Variable(var_name) => {
                // Check if this variable holds a stored lambda
                if let Some(stored_lambda) = self.context.lookup_lambda(var_name) {
                    return stored_lambda.params.len();
                }
                // Also check if it's a lambda value in bindings (e.g., from partial application)
                if let Some(value) = self.context.lookup(var_name) {
                    if let Some(stored_lambda) = self.lookup_lambda_from_value(value) {
                        return stored_lambda.params.len();
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
        if let AstNode::Lambda { params, body, .. } = comparator {
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

                    if self.is_truthy(&cmp_result) {
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
                    let cmp_result = self.apply_function(
                        comparator,
                        &[left[i].clone(), right[j].clone()],
                        data,
                    )?;
                    if self.is_truthy(&cmp_result) {
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
                let cmp_result =
                    self.apply_function(comparator, &[left[i].clone(), right[j].clone()], data)?;
                if self.is_truthy(&cmp_result) {
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
    pub fn evaluate(&mut self, node: &AstNode, data: &JValue) -> Result<JValue, EvaluatorError> {
        // Set parent context to root data if not already set
        if self.context.get_parent().is_none() {
            self.context.set_parent(data.clone());
        }

        if self.options.timeout_ms.is_some() {
            self.start_time = Some(Instant::now());
        }

        self.tuple_stream_created = false;
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
                    if obj.get("__tuple__") == Some(&JValue::Bool(true)) {
                        return None; // Tuple data needs full evaluator
                    }
                }
                // May be a lambda/builtin — needs full evaluator if None
                self.context.lookup(name).map(|value| Ok(value.clone()))
            }
            _ => None,
        }
    }

    /// Internal evaluation method
    fn evaluate_internal(
        &mut self,
        node: &AstNode,
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        // Fast path for leaf nodes — skip recursion tracking overhead
        if let Some(result) = self.evaluate_leaf(node, data) {
            return result;
        }

        // Check recursion depth to prevent stack overflow. `effective_limit` is
        // whichever is tighter: the user's `max_stack_depth` guardrail or the
        // hardcoded native-stack-safety ceiling (`max_recursion_depth`, always
        // 302, GitHub issue #34). The hardcoded ceiling is an always-on backstop
        // regardless of user options — only a user limit BELOW it can produce
        // D1011; hitting the hardcoded ceiling itself (no option set, or an
        // option set at/above 302) still produces U1001.
        self.recursion_depth += 1;
        let effective_limit = match self.options.max_stack_depth {
            Some(limit) => limit.min(self.max_recursion_depth),
            None => self.max_recursion_depth,
        };
        if self.recursion_depth > effective_limit {
            self.recursion_depth -= 1;
            return Err(EvaluatorError::EvaluationError(
                if effective_limit < self.max_recursion_depth {
                    "D1011: Stack overflow. Check for non-terminating recursive function. Consider rewriting as tail-recursive".to_string()
                } else {
                    format!(
                        "U1001: Stack overflow - maximum recursion depth ({}) exceeded",
                        effective_limit
                    )
                },
            ));
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

        // The soft depth counter above is calibrated against a comfortably
        // large native stack. Hosts with a much smaller default thread stack
        // (notably Windows, ~1MB vs Linux's ~8MB) can exhaust the *real*
        // stack well before this counter trips, crashing the process instead
        // of returning U1001 (see GitHub issue #34). stacker::maybe_grow
        // transparently swaps in a bigger stack segment when headroom is
        // low, so this stays a no-op cost on the common shallow path.
        const RED_ZONE: usize = 128 * 1024;
        const GROW_STACK_SIZE: usize = 8 * 1024 * 1024;
        let result = stacker::maybe_grow(RED_ZONE, GROW_STACK_SIZE, || {
            self.evaluate_internal_impl(node, data)
        });

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
            AstNode::KeepArray => Ok(Self::keep_array(data)),

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
                        return Ok(value.clone());
                    }
                    // If data is a tuple, return the @ value
                    if let JValue::Object(obj) = data {
                        if obj.get("__tuple__") == Some(&JValue::Bool(true)) {
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
                if let JValue::Object(obj) = data {
                    if obj.get("__tuple__") == Some(&JValue::Bool(true)) {
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
                    let lambda_repr = JValue::lambda(
                        name.as_str(),
                        stored_lambda.params.clone(),
                        Some(name.to_string()),
                        stored_lambda.signature.clone(),
                    );
                    return Ok(lambda_repr);
                }

                // Check if this is a built-in function reference (only if not shadowed)
                if self.is_builtin_function(name) {
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

                // For $$name, we need to evaluate name against parent context
                // This is similar to $.name but using parent data
                let parent_data = self.context.get_parent().ok_or_else(|| {
                    EvaluatorError::ReferenceError("Parent context not available".to_string())
                })?;

                // Access field on parent context
                match parent_data {
                    JValue::Object(obj) => Ok(obj.get(name).cloned().unwrap_or(JValue::Null)),
                    _ => Ok(JValue::Null),
                }
            }

            AstNode::Path { steps } => self.evaluate_path(steps, data),

            AstNode::Binary { op, lhs, rhs } => self.evaluate_binary_op(*op, lhs, rhs, data),

            AstNode::Unary { op, operand } => self.evaluate_unary_op(*op, operand, data),

            // Array constructor - JSONata semantics:
            AstNode::Array(elements) => {
                // - If element is itself an array constructor [...], keep it nested
                // - Otherwise, if element evaluates to an array, flatten it
                // - Undefined values are excluded
                let mut result = Vec::with_capacity(elements.len());
                for element in elements {
                    // Check if this element is itself an explicit array constructor
                    let is_array_constructor = matches!(element, AstNode::Array(_));

                    let value = self.evaluate_internal(element, data)?;

                    // Skip undefined values in array constructors
                    // Note: explicit null is preserved, only undefined (no value) is filtered
                    if value.is_undefined() {
                        continue;
                    }

                    if is_array_constructor {
                        // Explicit array constructor - keep nested
                        result.push(value);
                    } else if let JValue::Array(arr) = value {
                        // Non-array-constructor that evaluated to array - flatten it
                        result.extend(arr.iter().cloned());
                    } else {
                        // Non-array value - add as-is
                        result.push(value);
                    }
                }
                Ok(JValue::array(result))
            }

            AstNode::Object(pairs) => {
                let mut result = IndexMap::with_capacity(pairs.len());

                // Check if all keys are string literals — can skip D1009 HashMap
                let all_literal_keys = pairs.iter().all(|(k, _)| matches!(k, AstNode::String(_)));

                if all_literal_keys {
                    // Fast path: literal keys, no need for D1009 tracking
                    for (key_node, value_node) in pairs.iter() {
                        let key = match key_node {
                            AstNode::String(s) => s,
                            _ => unreachable!(),
                        };
                        let value = self.evaluate_internal(value_node, data)?;
                        if value.is_undefined() {
                            continue;
                        }
                        result.insert(key.clone(), value);
                    }
                } else {
                    let mut key_sources: HashMap<String, usize> = HashMap::new();
                    for (pair_index, (key_node, value_node)) in pairs.iter().enumerate() {
                        let key = match self.evaluate_internal(key_node, data)? {
                            JValue::String(s) => s,
                            JValue::Null => continue,
                            other => {
                                if other.is_undefined() {
                                    continue;
                                }
                                return Err(EvaluatorError::TypeError(format!(
                                    "T1003: Key in object structure must evaluate to a string; got: {:?}",
                                    other
                                )));
                            }
                        };

                        if let Some(&existing_idx) = key_sources.get(&*key) {
                            if existing_idx != pair_index {
                                return Err(EvaluatorError::EvaluationError(format!(
                                    "D1009: Multiple key expressions evaluate to same key: {}",
                                    key
                                )));
                            }
                        }
                        key_sources.insert(key.to_string(), pair_index);

                        let value = self.evaluate_internal(value_node, data)?;
                        if value.is_undefined() {
                            continue;
                        }
                        result.insert(key.to_string(), value);
                    }
                }
                Ok(JValue::object(result))
            }

            // Object transform: group items by key, then evaluate value once per group
            AstNode::ObjectTransform { input, pattern } => {
                // Evaluate the input expression. Keep tuple wrappers alive so the
                // group-by key/value expressions can read the carried `$focus`
                // bindings off each wrapper (e.g. `...@$e...{ $e.FirstName: ... }`).
                let saved_keep = self.keep_tuple_stream;
                self.keep_tuple_stream = true;
                let input_value = self.evaluate_internal(input, data);
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
                    JValue::Array(ref arr) => (**arr).clone(),
                    JValue::Undefined => Vec::new(),
                    other => vec![other],
                };

                // If array is empty, add undefined to enable literal JSON object generation
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
                let reduce = items.first().is_some_and(|it| {
                    matches!(it, JValue::Object(o) if o.get("__tuple__") == Some(&JValue::Bool(true)))
                });

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

                // Phase 1: Group items by key expression
                // groups maps key -> (grouped_data, expr_index)
                // When multiple items have same key, their data is appended together
                let mut groups: HashMap<String, (Vec<JValue>, usize)> = HashMap::new();

                // Save the current $ binding to restore later
                let saved_dollar = self.context.lookup("$").cloned();

                for item in &items {
                    // In reduce mode evaluate the key against `@` with tuple keys
                    // bound; otherwise against the item itself.
                    let (key_data, tuple_saved) = match (reduce, item) {
                        (true, JValue::Object(o)) => {
                            let saved = bind_tuple(self, o);
                            (
                                o.get("@").cloned().unwrap_or(JValue::Undefined),
                                Some(saved),
                            )
                        }
                        _ => (item.clone(), None),
                    };
                    self.context.bind("$".to_string(), key_data.clone());

                    for (pair_index, (key_node, _value_node)) in pattern.iter().enumerate() {
                        // Evaluate key with current item as context
                        let key = match self.evaluate_internal(key_node, &key_data)? {
                            JValue::String(s) => s,
                            JValue::Null => continue, // Skip null keys
                            other => {
                                // Skip undefined keys
                                if other.is_undefined() {
                                    continue;
                                }
                                if let Some(saved) = tuple_saved {
                                    restore(self, saved);
                                }
                                return Err(EvaluatorError::TypeError(format!(
                                    "T1003: Object key must be a string, got: {:?}",
                                    other
                                )));
                            }
                        };

                        // Group items by key
                        if let Some((existing_data, existing_idx)) = groups.get_mut(&*key) {
                            // Key already exists - check if from same expression index
                            if *existing_idx != pair_index {
                                if let Some(saved) = tuple_saved {
                                    restore(self, saved);
                                }
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
                            groups.insert(key.to_string(), (vec![item.clone()], pair_index));
                        }
                    }

                    if let Some(saved) = tuple_saved {
                        restore(self, saved);
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
                        let saved = bind_tuple(self, &tuple_no_at);
                        self.context.bind("$".to_string(), context.clone());
                        let value = self.evaluate_internal(value_node, &context);
                        restore(self, saved);
                        let value = value?;
                        if !value.is_undefined() {
                            result.insert(key, value);
                        }
                        continue;
                    }

                    // Determine the context for value evaluation:
                    // - If single item, use that item directly
                    // - If multiple items, use the array of items
                    let context = if grouped_data.len() == 1 {
                        grouped_data.into_iter().next().unwrap()
                    } else {
                        JValue::array(grouped_data)
                    };

                    // Bind $ to the context for value evaluation
                    self.context.bind("$".to_string(), context.clone());

                    // Evaluate value expression with grouped context
                    let value = self.evaluate_internal(value_node, &context)?;

                    // Skip undefined values
                    if !value.is_undefined() {
                        result.insert(key, value);
                    }
                }

                // Restore the previous $ binding
                if let Some(saved) = saved_dollar {
                    self.context.bind("$".to_string(), saved);
                } else {
                    self.context.unbind("$");
                }

                Ok(JValue::object(result))
            }

            AstNode::Function {
                name,
                args,
                is_builtin,
            } => self.evaluate_function_call(name, args, *is_builtin, data),

            // Call: invoke an arbitrary expression as a function
            // Used for IIFE patterns like (function($x){...})(5) or chained calls
            AstNode::Call { procedure, args } => {
                // Evaluate the procedure to get the callable value
                let callable = self.evaluate_internal(procedure, data)?;

                // Check if it's a lambda value
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
                    Ok(JValue::Undefined)
                }
            }

            AstNode::Block(expressions) => {
                // Blocks create a new scope - push scope instead of clone/restore
                self.context.push_scope();

                // An empty block (`()`) is undefined in jsonata-js, not null.
                // A non-empty block always overwrites this before it's read,
                // so this default only matters for the empty case -- reachable
                // via `.()` as a path step (parser.rs routes a bare top-level
                // `()` around this arm entirely, straight to
                // `AstNode::Undefined`, but a `.()` step keeps it as an empty
                // `Block` so the ancestry pass can walk past it; see #114).
                let mut result = JValue::Undefined;
                for expr in expressions {
                    result = self.evaluate_internal(expr, data)?;
                }

                // Before popping, preserve any lambdas referenced by the result
                // This is essential for closures returned from blocks (IIFE pattern)
                let lambdas_to_keep = self.extract_lambda_ids(&result);
                self.context.pop_scope_preserving_lambdas(&lambdas_to_keep);

                Ok(result)
            }

            // Lambda: capture current environment for closure support
            AstNode::Lambda {
                params,
                body,
                signature,
                thunk,
            } => {
                let lambda_id = format!("__lambda_{}_{}", params.len(), self.fresh_lambda_id());

                let compiled_body = if !thunk {
                    let var_refs: Vec<&str> = params.iter().map(|s| s.as_str()).collect();
                    try_compile_expr_with_allowed_vars(body, &var_refs)
                } else {
                    None
                };
                let stored_lambda = StoredLambda {
                    params: params.clone(),
                    body: (**body).clone(),
                    compiled_body,
                    signature: signature.clone(),
                    captured_env: self.capture_environment_for(body, params),
                    captured_data: Some(data.clone()),
                    thunk: *thunk,
                };
                self.context.bind_lambda(lambda_id.clone(), stored_lambda);

                let lambda_obj = JValue::lambda(
                    lambda_id.as_str(),
                    params.clone(),
                    None::<String>,
                    signature.clone(),
                );

                Ok(lambda_obj)
            }

            // Wildcard: collect all values from current object
            AstNode::Wildcard => {
                let normalized = normalize_lazy(data)?;
                match &normalized {
                    JValue::Object(obj) => {
                        let mut result = Vec::new();
                        for value in obj.values() {
                            // Flatten arrays into the result
                            match value {
                                JValue::Array(arr) => result.extend(arr.iter().cloned()),
                                _ => result.push(value.clone()),
                            }
                        }
                        check_sequence_length(result.len(), &self.options)?;
                        Ok(JValue::array(result))
                    }
                    JValue::Array(arr) => {
                        // For arrays, wildcard returns all elements
                        Ok(JValue::Array(arr.clone()))
                    }
                    // Anything else has no children to collect. jsonata-js
                    // guards the wildcard with `typeof input === 'object' &&
                    // input !== null`, so every scalar -- and null, which only
                    // reaches that guard because of JS's `typeof null` quirk --
                    // maps over nothing and yields undefined, not null.
                    _ => Ok(JValue::Undefined),
                }
            }

            // Descendant: recursively traverse all nested values
            AstNode::Descendant => {
                let descendants = self.collect_descendants(data)?;
                if descendants.is_empty() {
                    Ok(JValue::Null) // No descendants means undefined
                } else {
                    check_sequence_length(descendants.len(), &self.options)?;
                    Ok(JValue::array(descendants))
                }
            }

            AstNode::Predicate(_) => Err(EvaluatorError::EvaluationError(
                "Predicate can only be used in path expressions".to_string(),
            )),

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
                Ok(JValue::array(result))
            }

            AstNode::FunctionApplication(_) => Err(EvaluatorError::EvaluationError(
                "Function application can only be used in path expressions".to_string(),
            )),

            AstNode::Sort { input, terms } => {
                // Keep the input path's tuple wrappers so the sort terms can read
                // the carried `%`/`$focus`/`$index` bindings per element.
                let saved = self.keep_tuple_stream;
                self.keep_tuple_stream = true;
                let value = self.evaluate_internal(input, data);
                self.keep_tuple_stream = saved;
                self.evaluate_sort(&value?, terms)
            }

            // Transform: |location|update[,delete]|
            AstNode::Transform {
                location,
                update,
                delete,
            } => {
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
                        compiled_body: None, // Transform is not a pure compilable expr
                        signature: None,
                        captured_env: HashMap::new(),
                        captured_data: None, // Transform takes $ as parameter
                        thunk: false,
                    };

                    // Store with a generated unique name
                    let lambda_name = format!("__transform_{}", self.fresh_lambda_id());
                    self.context.bind_lambda(lambda_name, transform_lambda);

                    // Return lambda marker
                    Ok(JValue::string("<lambda>"))
                }
            }

            // Parent-reference operator (%): ast_transform has already resolved
            // this to a synthetic ancestor label ("!0", "!1", ...). The enclosing
            // tuple step binds that label into scope (create_tuple_stream +
            // needs_tuple_context_binding), so resolving it is an ordinary scope
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
                    if obj.get("__tuple__") == Some(&JValue::Bool(true)) {
                        if let Some(v) = obj.get(label) {
                            return Ok(v.clone());
                        }
                    }
                }
                Ok(JValue::Undefined)
            }
        }
    }

    /// Apply stages (filters/predicates) to a value during field extraction
    /// Non-array values are wrapped in an array before filtering (JSONata semantics)
    /// This matches the JavaScript reference where stages apply to sequences
    fn apply_stages(&mut self, value: JValue, stages: &[Stage]) -> Result<JValue, EvaluatorError> {
        // Wrap non-arrays in an array for filtering (JSONata semantics)
        // An explicit null is a *value*, so it wraps into a one-element
        // sequence like any other scalar: `nul[]` is `[null]` and `nul[0]` is
        // `null`. This arm used to return early, a leftover from before the
        // Null/Undefined split, which made every stage over a null a no-op.
        // Undefined never reaches here -- the step loop returns before stages.
        let mut result = match value {
            JValue::Array(_) => value,
            other => JValue::array(vec![other]),
        };

        for stage in stages {
            match stage {
                Stage::Filter(predicate_expr) => {
                    // When applying stages, use stage-specific predicate logic
                    result = self.evaluate_predicate_as_stage(&result, predicate_expr)?;
                }
                // `[]` keeps whatever it is handed; `apply_stages` has already
                // wrapped a non-array into a one-element sequence above.
                Stage::KeepArray => {}
                // Positional index stages are meaningful only over a tuple stream
                // (they set a variable to each tuple's position); they are applied
                // in `create_tuple_stream`, not on a plain value sequence here.
                Stage::Index(_) => {}
            }
        }
        Ok(result)
    }

    /// Check if an AST node is definitely a filter expression (comparison/logical)
    /// rather than a potential numeric index. When true, we skip speculative numeric evaluation.
    fn is_filter_predicate(predicate: &AstNode) -> bool {
        match predicate {
            AstNode::Binary { op, .. } => matches!(
                op,
                BinaryOp::GreaterThan
                    | BinaryOp::GreaterThanOrEqual
                    | BinaryOp::LessThan
                    | BinaryOp::LessThanOrEqual
                    | BinaryOp::Equal
                    | BinaryOp::NotEqual
                    | BinaryOp::And
                    | BinaryOp::Or
                    | BinaryOp::In
            ),
            AstNode::Unary {
                op: crate::ast::UnaryOp::Not,
                ..
            } => true,
            _ => false,
        }
    }

    /// Evaluate a predicate as a stage during field extraction
    /// This has different semantics than standalone predicates:
    /// - Maps index operations over arrays of extracted values
    fn evaluate_predicate_as_stage(
        &mut self,
        current: &JValue,
        predicate: &AstNode,
    ) -> Result<JValue, EvaluatorError> {
        match current {
            JValue::Array(arr) => {
                // For stages: if we have an array of values (from field extraction),
                // apply the predicate to each value if appropriate

                // Check if predicate is a numeric index
                if let AstNode::Number(n) = predicate {
                    // Check if this is an array of arrays (extracted array fields)
                    let is_array_of_arrays =
                        arr.iter().any(|item| matches!(item, JValue::Array(_)));

                    if !is_array_of_arrays {
                        // Simple values: just index normally
                        return self.array_index(current, &JValue::Number(*n));
                    }

                    // Array of arrays: map index access over each extracted array
                    let mut result = Vec::new();
                    for item in arr.iter() {
                        match item {
                            JValue::Array(_) => {
                                let indexed = self.array_index(item, &JValue::Number(*n))?;
                                if !indexed.is_null() && !indexed.is_undefined() {
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
                    return Ok(JValue::array(result));
                }

                // Short-circuit: if predicate is definitely a comparison/logical expression,
                // skip speculative numeric evaluation and go directly to filter logic
                if Self::is_filter_predicate(predicate) {
                    // Try CompiledExpr fast path (handles compound predicates, arithmetic, etc.)
                    if let Some(compiled) = try_compile_expr(predicate) {
                        let shape = arr.first().and_then(build_shape_cache);
                        let mut filtered = Vec::with_capacity(arr.len());
                        for item in arr.iter() {
                            let result = if let Some(ref s) = shape {
                                eval_compiled_shaped(
                                    &compiled,
                                    item,
                                    None,
                                    s,
                                    &self.options,
                                    self.start_time,
                                )?
                            } else {
                                eval_compiled(
                                    &compiled,
                                    item,
                                    None,
                                    &self.options,
                                    self.start_time,
                                )?
                            };
                            if compiled_is_truthy(&result) {
                                filtered.push(item.clone());
                            }
                        }
                        return Ok(JValue::array(filtered));
                    }
                    // Fallback: full AST evaluation
                    let mut filtered = Vec::new();
                    for item in arr.iter() {
                        let item_result = self.evaluate_internal(predicate, item)?;
                        if self.is_truthy(&item_result) {
                            filtered.push(item.clone());
                        }
                    }
                    return Ok(JValue::array(filtered));
                }

                // Try to evaluate the predicate to see if it's a numeric index or array of indices
                // If evaluation succeeds and yields a number, use it as an index
                // If it yields an array of numbers, use them as multiple indices
                // If evaluation fails (e.g., comparison error), treat as filter
                match self.evaluate_internal(predicate, current) {
                    Ok(JValue::Number(n)) => {
                        let n_val = n;
                        let is_array_of_arrays =
                            arr.iter().any(|item| matches!(item, JValue::Array(_)));

                        if !is_array_of_arrays {
                            let pred_result = JValue::Number(n_val);
                            return self.array_index(current, &pred_result);
                        }

                        // Array of arrays: map index access
                        let mut result = Vec::new();
                        let pred_result = JValue::Number(n_val);
                        for item in arr.iter() {
                            match item {
                                JValue::Array(_) => {
                                    let indexed = self.array_index(item, &pred_result)?;
                                    if !indexed.is_null() && !indexed.is_undefined() {
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
                        return Ok(JValue::array(result));
                    }
                    Ok(JValue::Array(indices)) => {
                        // Array of values - could be indices or filter results
                        // Check if all values are numeric
                        let has_non_numeric =
                            indices.iter().any(|v| !matches!(v, JValue::Number(_)));

                        if has_non_numeric {
                            // Non-numeric values - treat as filter, fall through
                        } else {
                            // All numeric - use as indices
                            let arr_len = arr.len() as i64;
                            let mut resolved_indices: Vec<i64> = indices
                                .iter()
                                .filter_map(|v| {
                                    if let JValue::Number(n) = v {
                                        let idx = *n as i64;
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
                            let result: Vec<JValue> = resolved_indices
                                .iter()
                                .map(|&idx| arr[idx as usize].clone())
                                .collect();

                            return Ok(JValue::array(result));
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
                let len = arr.len();
                for (index, item) in arr.iter().enumerate() {
                    let item_result = self.evaluate_internal(predicate, item)?;
                    // A numeric predicate selects by position, not truthiness.
                    let repeats = match predicate_index_match(&item_result, index, len) {
                        Some(n) => n,
                        None => usize::from(self.is_truthy(&item_result)),
                    };
                    for _ in 0..repeats {
                        filtered.push(item.clone());
                    }
                }
                Ok(JValue::array(filtered))
            }
            JValue::Null => {
                // Null: return null
                Ok(JValue::Null)
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
                        return Ok(JValue::Null);
                    }
                }

                // Try to evaluate the predicate to see if it's a numeric index
                match self.evaluate_internal(predicate, other) {
                    Ok(JValue::Number(n)) => {
                        // Index 0 returns the value, other indices return null
                        if n == 0.0 {
                            Ok(other.clone())
                        } else {
                            Ok(JValue::Null)
                        }
                    }
                    Ok(pred_result) => {
                        // Boolean filter: return value if truthy, null if falsy
                        if self.is_truthy(&pred_result) {
                            Ok(other.clone())
                        } else {
                            Ok(JValue::Null)
                        }
                    }
                    Err(e) => Err(e),
                }
            }
        }
    }

    /// Evaluate a path expression (e.g., foo.bar.baz)
    fn evaluate_path(
        &mut self,
        steps: &[PathStep],
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        // Avoid cloning by using references and only cloning when necessary
        if steps.is_empty() {
            return Ok(data.clone());
        }

        // Fast path: single field access on object
        // This is a very common pattern, so optimize it.
        // Skipped for tuple-binding steps (@/#/%), which need full tuple-stream
        // creation handled below.
        if steps.len() == 1 && !Self::step_creates_tuple(&steps[0]) {
            if let AstNode::Name(field_name) = &steps[0].node {
                // A tuple stream falls through to the general step loop below —
                // its tuple arm is the single implementation of tuple-aware
                // field extraction (the fast path used to carry a drifted copy
                // that skipped nulls and returned Null on empty).
                let is_tuple_stream = matches!(data, JValue::Array(arr) if arr.first().is_some_and(
                    |item| matches!(item, JValue::Object(obj) if obj.get("__tuple__") == Some(&JValue::Bool(true)))
                ));
                if !is_tuple_stream {
                    return match data {
                        JValue::Object(obj) => {
                            // Check if this is a tuple - extract '@' value
                            if obj.get("__tuple__") == Some(&JValue::Bool(true)) {
                                match obj.get("@") {
                                    Some(JValue::Object(inner)) => Ok(inner
                                        .get(field_name)
                                        .cloned()
                                        .unwrap_or(JValue::Undefined)),
                                    #[cfg(feature = "python")]
                                    Some(JValue::LazyPyDict(lazy)) => {
                                        Ok(lazy.get_field(field_name)?)
                                    }
                                    _ => Ok(JValue::Undefined),
                                }
                            } else {
                                Ok(obj.get(field_name).cloned().unwrap_or(JValue::Undefined))
                            }
                        }
                        #[cfg(feature = "python")]
                        JValue::LazyPyDict(lazy) => Ok(lazy.get_field(field_name)?),
                        JValue::Array(_) => {
                            // Delegate to the shared field step (skip undefined,
                            // KEEP nulls, flatten one level, empty -> undefined)
                            // plus the end-of-path singleton unwrap — the same
                            // policy as the VM's get_field_cached and jsonata-js.
                            let extracted = compiled_field_step(field_name, data, &self.options)?;
                            match extracted {
                                JValue::Array(arr) if arr.len() == 1 => Ok(arr[0].clone()),
                                other => Ok(other),
                            }
                        }
                        _ => Ok(JValue::Undefined),
                    };
                }
            }
        }

        // Fast path: 2-step $variable.field with no stages
        // Handles common patterns like $l.rating, $item.price in sort/HOF bodies
        if steps.len() == 2 && steps[0].stages.is_empty() && steps[1].stages.is_empty() {
            if let (AstNode::Variable(var_name), AstNode::Name(field_name)) =
                (&steps[0].node, &steps[1].node)
            {
                if !var_name.is_empty() {
                    if let Some(value) = self.context.lookup(var_name) {
                        match value {
                            // An absent field is undefined, not null: `$v.p`
                            // over `{"q": 9}` must drop out of a sequence
                            // rather than become an explicit null.
                            JValue::Object(obj) => {
                                return Ok(obj
                                    .get(field_name)
                                    .cloned()
                                    .unwrap_or(JValue::Undefined));
                            }
                            #[cfg(feature = "python")]
                            JValue::LazyPyDict(lazy) => {
                                return lazy.get_field(field_name).map_err(Into::into);
                            }
                            JValue::Array(_) => {
                                // Delegate to the shared field step. Its loop
                                // recurses into nested-array elements the way
                                // jsonata-js's `lookup` does — the hand-rolled
                                // loop this replaces skipped them, so
                                // `($v := [[{"p":1}],{"p":2}]; $v.p)` returned
                                // 2 where the reference returns [1,2] — then
                                // apply the end-of-path singleton unwrap the
                                // general walker performs after array mapping.
                                let extracted =
                                    compiled_field_step(field_name, value, &self.options)?;
                                return Ok(match extracted {
                                    JValue::Array(arr) if arr.len() == 1 => arr[0].clone(),
                                    other => other,
                                });
                            }
                            _ => {} // Fall through to general path evaluation
                        }
                    }
                }
            }
        }

        // Track whether we did array mapping (for singleton unwrapping)
        let mut did_array_mapping = false;

        // For the first step, work with a reference.
        // Tuple-binding first steps (e.g. `items#$i`, `foo@$v`) create a tuple
        // stream up front, mirroring jsonata-js's evaluateTupleStep for the
        // first path step where tupleBindings is undefined.
        let mut current: JValue = if Self::step_creates_tuple(&steps[0]) {
            JValue::array(self.create_tuple_stream(&steps[0], data, true)?)
        } else {
            match &steps[0].node {
                AstNode::Wildcard => {
                    // Wildcard as first step
                    let normalized = normalize_lazy(data)?;
                    match &normalized {
                        JValue::Object(obj) => {
                            let mut result = Vec::new();
                            for value in obj.values() {
                                // Flatten arrays into the result
                                match value {
                                    JValue::Array(arr) => result.extend(arr.iter().cloned()),
                                    _ => result.push(value.clone()),
                                }
                            }
                            JValue::array(result)
                        }
                        JValue::Array(arr) => JValue::Array(arr.clone()),
                        // jsonata-js's wildcard guards with `typeof input ===
                        // 'object' && input !== null`, so null maps over
                        // nothing (issue #114) -- and so does every scalar,
                        // which is the same answer, hence one arm rather than
                        // the two this used to carry.
                        _ => JValue::Undefined,
                    }
                }
                AstNode::Descendant => {
                    // Descendant as first step
                    let descendants = self.collect_descendants(data)?;
                    JValue::array(descendants)
                }
                AstNode::ParentVariable(name) => {
                    // Parent variable as first step
                    let parent_data = self.context.get_parent().ok_or_else(|| {
                        EvaluatorError::ReferenceError("Parent context not available".to_string())
                    })?;

                    if name.is_empty() {
                        // $$ alone returns parent context
                        parent_data.clone()
                    } else {
                        // $$field accesses field on parent
                        match parent_data {
                            JValue::Object(obj) => obj.get(name).cloned().unwrap_or(JValue::Null),
                            _ => JValue::Null,
                        }
                    }
                }
                AstNode::Name(field_name) => {
                    // Field/property access - get the stages for this step
                    let stages = &steps[0].stages;

                    match data {
                        JValue::Object(obj) => {
                            let val = obj.get(field_name).cloned().unwrap_or(JValue::Undefined);
                            // Apply any stages to the extracted value
                            if !stages.is_empty() {
                                self.apply_stages(val, stages)?
                            } else {
                                val
                            }
                        }
                        #[cfg(feature = "python")]
                        JValue::LazyPyDict(lazy) => {
                            let val = lazy.get_field(field_name)?;
                            if !stages.is_empty() {
                                self.apply_stages(val, stages)?
                            } else {
                                val
                            }
                        }
                        JValue::Array(arr) => {
                            // Array mapping: extract field from each element and apply stages
                            let mut result = Vec::new();
                            for item in arr.iter() {
                                match item {
                                    JValue::Object(obj) => {
                                        let val = obj
                                            .get(field_name)
                                            .cloned()
                                            .unwrap_or(JValue::Undefined);
                                        if !val.is_null() && !val.is_undefined() {
                                            if !stages.is_empty() {
                                                // Apply stages to the extracted value
                                                let processed_val =
                                                    self.apply_stages(val, stages)?;
                                                // Stages always return an array (or null); extend results
                                                match processed_val {
                                                    JValue::Array(arr) => {
                                                        result.extend(arr.iter().cloned())
                                                    }
                                                    JValue::Null => {}
                                                    other => result.push(other), // Shouldn't happen, but handle it
                                                }
                                            } else {
                                                // No stages: flatten arrays, push scalars
                                                match val {
                                                    JValue::Array(arr) => {
                                                        result.extend(arr.iter().cloned())
                                                    }
                                                    other => result.push(other),
                                                }
                                            }
                                        }
                                    }
                                    JValue::Array(inner_arr) => {
                                        // Recursively map over nested array
                                        let nested_result = self.evaluate_path(
                                            &[steps[0].clone()],
                                            &JValue::Array(inner_arr.clone()),
                                        )?;
                                        match nested_result {
                                            JValue::Array(nested) => {
                                                result.extend(nested.iter().cloned())
                                            }
                                            JValue::Null => {}
                                            other => result.push(other),
                                        }
                                    }
                                    #[cfg(feature = "python")]
                                    JValue::LazyPyDict(lazy) => {
                                        let val = lazy.get_field(field_name)?;
                                        if !val.is_null() && !val.is_undefined() {
                                            if !stages.is_empty() {
                                                let processed_val =
                                                    self.apply_stages(val, stages)?;
                                                match processed_val {
                                                    JValue::Array(arr) => {
                                                        result.extend(arr.iter().cloned())
                                                    }
                                                    JValue::Null => {}
                                                    other => result.push(other),
                                                }
                                            } else {
                                                match val {
                                                    JValue::Array(arr) => {
                                                        result.extend(arr.iter().cloned())
                                                    }
                                                    other => result.push(other),
                                                }
                                            }
                                        }
                                    }
                                    _ => {} // Skip non-object items
                                }
                            }
                            JValue::array(result)
                        }
                        // Accessing field on non-object returns undefined (not an
                        // error) -- including when that non-object is an explicit
                        // null (issue #114): a pre-migration leftover here used to
                        // special-case null back to itself instead of falling
                        // through to this arm, e.g. `nul.(foo.bar)`'s inner path
                        // hits this via the FunctionApplication step below binding
                        // `data = null`.
                        _ => JValue::Undefined,
                    }
                }
                AstNode::String(string_literal) => {
                    // String literal in path context - evaluate as literal and apply stages
                    // This handles cases like "Red"[true] where "Red" is a literal, not a field access
                    let stages = &steps[0].stages;
                    let val = JValue::string(string_literal.clone());

                    if !stages.is_empty() {
                        // Apply stages (predicates) to the string literal
                        let result = self.apply_stages(val, stages)?;
                        // Unwrap single-element arrays back to scalar
                        // (string literals with predicates should return scalar or null, not arrays)
                        match result {
                            JValue::Array(arr) if arr.len() == 1 => arr[0].clone(),
                            JValue::Array(arr) if arr.is_empty() => JValue::Null,
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
                AstNode::KeepArray => Self::keep_array(data),
                _ => {
                    // Complex first step - evaluate it. When the step is
                    // tuple-carrying (e.g. a parenthesized `(Account.Order.Product)`
                    // whose `Product` is `%`-tagged, as in
                    // `(Account.Order.Product)[%.OrderID='order104'].SKU`), keep the
                    // inner path's tuple wrappers so the following predicate/step
                    // can read the `!label` bindings.
                    let saved_keep = self.keep_tuple_stream;
                    if steps[0].is_tuple {
                        self.keep_tuple_stream = true;
                    }
                    let v = self.evaluate_path_step(&steps[0].node, data, data, true);
                    self.keep_tuple_stream = saved_keep;
                    v?
                }
            }
        };

        // Process remaining steps
        for (step_idx, step) in steps[1..].iter().enumerate() {
            let is_last_step = step_idx == steps.len() - 2;
            // Undefined means "no value" and short-circuits unconditionally --
            // there is nothing for any step kind to index into, call, or
            // build from. This handles cases like `blah.{}` where `blah`
            // doesn't exist.
            //
            // An explicit null is NOT short-circuited (issue #114): null is
            // an ordinary value in jsonata-js, and every step kind below
            // already has its own correct null handling once it actually
            // runs -- an object-constructor step ignores its context
            // entirely (`nul.{}` is `{}`), a `FunctionApplication` step
            // (`.$fn()` calls and parenthesised block steps `.(expr)`,
            // including the empty `.()`) hands null to the function/block
            // like any other value (issue #110 -- `nul.$string()` is
            // "null"), and a `Name`/`Wildcard` step correctly falls out of
            // the sequence to undefined the same way it would for a number
            // or string current value.
            if current.is_undefined() {
                return Ok(JValue::Undefined);
            }

            // A lone tuple wrapper (e.g. from a numeric index predicate `[1]` over
            // a tuple stream, which selects a single tuple and unwraps it out of
            // the array) must stay a tuple stream so the following step keeps
            // reading its carried `$focus`/`!label` bindings. Re-wrap it as a
            // one-element array (e.g. `library.loans@$l.books@$b[...][1].{...}`).
            if let JValue::Object(o) = &current {
                if o.get("__tuple__") == Some(&JValue::Bool(true)) {
                    current = JValue::array(vec![current.clone()]);
                    // The lone wrapper came from a singleton index selection, so
                    // the final result should unwrap back to a scalar (a following
                    // object step must not leave a spurious 1-element array).
                    did_array_mapping = true;
                }
            }

            // Check if current is a tuple array - if so, we need to bind tuple variables
            // to context so they're available in nested expressions (like predicates)
            let is_tuple_array = if let JValue::Array(arr) = &current {
                arr.first().is_some_and(|first| {
                    if let JValue::Object(obj) = first {
                        obj.get("__tuple__") == Some(&JValue::Bool(true))
                    } else {
                        false
                    }
                })
            } else {
                false
            };

            // Tuple-binding step (@ focus / # index / % parent): create/extend the
            // tuple stream, mirroring jsonata-js's evaluateTupleStep. Downstream
            // (non-binding) steps then consume the {@, $var, !label, __tuple__}
            // wrappers via the existing tuple-aware handling below.
            //
            // A `%` reference used AS a path step (`AstNode::Parent`, e.g. the
            // `.%` in `Account.Order.Product.Price.%[...]`) must also extend the
            // stream, but ONLY when it is consuming an existing tuple stream:
            // its ancestor label lives in those incoming tuples, so
            // create_tuple_stream's per-tuple frame binding is what lets
            // `evaluate_internal(Parent, ..)` resolve it (and any predicate
            // stage on the `%` step then resolves in the same frame). A `%`
            // that instead LEADS a fresh path (e.g. the `%.OrderID` inside a
            // predicate, whose input is plain data, not a tuple stream) must
            // NOT be routed here -- it's an ordinary scope lookup.
            let is_parent_step_over_tuple =
                matches!(step.node, AstNode::Parent(_)) && is_tuple_array;
            if Self::step_creates_tuple(step) || is_parent_step_over_tuple {
                current = JValue::array(self.create_tuple_stream(step, &current, false)?);
                continue;
            }

            // For tuple arrays with certain step types, we need special handling to bind
            // tuple variables to context so they're available in nested expressions.
            // This is needed for:
            // - Object constructors: {"label": $$.items[$i]} needs $i in context
            // - Function applications: .($$.items[$i]) needs $i in context
            // - Variable lookups: .$i needs to find the tuple binding
            //
            // Steps like Name (field access) already have proper tuple handling in their
            // specific cases, so we don't intercept those here.
            let needs_tuple_context_binding = is_tuple_array
                && matches!(
                    &step.node,
                    AstNode::Object(_)
                        | AstNode::FunctionApplication(_)
                        | AstNode::Variable(_)
                        | AstNode::ArrayGroup(_)
                );

            if needs_tuple_context_binding {
                if let JValue::Array(arr) = &current {
                    let mut results = Vec::new();

                    for tuple in arr.iter() {
                        if let JValue::Object(tuple_obj) = tuple {
                            // Extract tuple bindings so nested expressions can see
                            // them: `$var` focus/index bindings (stored `$name`,
                            // bound as `name`) AND `!label` ancestor bindings for
                            // `%` (stored and bound under the full `!label` key).
                            // Saves/restores rather than blindly unbinding, so a
                            // tuple key that collides with a live outer `:=`
                            // binding doesn't get deleted afterward.
                            let tuple_bindings = self.bind_tuple_keys(tuple_obj);

                            // Get the actual value from the tuple (@ field)
                            let actual_data = tuple_obj.get("@").cloned().unwrap_or(JValue::Null);

                            // Evaluate the step
                            let step_result = match &step.node {
                                AstNode::Variable(_) => {
                                    // Variable lookup - check context (which now has bindings)
                                    self.evaluate_internal(&step.node, tuple)?
                                }
                                AstNode::Object(_) | AstNode::ArrayGroup(_) => {
                                    // Object / array constructor step (e.g.
                                    // `Product.[`Product Name`, %.OrderID]`) -
                                    // evaluate on the tuple's `@` value with the
                                    // carried `!label`/`$focus` bindings in scope
                                    // so an embedded `%` resolves.
                                    self.evaluate_internal(&step.node, &actual_data)?
                                }
                                AstNode::FunctionApplication(inner) => {
                                    // A parenthesized step `(expr)` consuming a tuple stream
                                    // (e.g. `Account.Order.Product.( %.OrderID )` or
                                    // `Employee@$e.(Contact)[...]`): evaluate the INNER
                                    // expression on the tuple's `@` value with `$` bound to
                                    // it, mirroring the non-tuple FunctionApplication step
                                    // handling. Routing the wrapper node itself through
                                    // evaluate_internal raises "Function application can only
                                    // be used in path expressions".
                                    let saved_dollar = self.context.lookup("$").cloned();
                                    self.context.bind("$".to_string(), actual_data.clone());
                                    // Keep tuple wrappers from the inner path alive:
                                    // when `inner` is itself a tuple-carrying path
                                    // (e.g. `(Order.Product)` whose `Product` is
                                    // `%`-tagged), its `!label` wrappers must survive
                                    // to be merged into this tuple by the rewrap below
                                    // (they feed a later `%`/`%.%`). Without this the
                                    // inner path projects to `@` and drops the labels.
                                    let saved_keep = self.keep_tuple_stream;
                                    self.keep_tuple_stream = true;
                                    let v = self.evaluate_internal(inner, &actual_data);
                                    self.keep_tuple_stream = saved_keep;
                                    match saved_dollar {
                                        Some(s) => self.context.bind("$".to_string(), s),
                                        None => self.context.unbind("$"),
                                    }
                                    v?
                                }
                                _ => unreachable!(), // We only match specific types above
                            };

                            // Apply this step's own filter stages (e.g. the
                            // `[$substring(title,0,3)='The']` on `.$[...]` in
                            // `library.books#$pos.$[...].$pos`) while the tuple
                            // bindings are still in scope, so the predicate can
                            // reference them and non-matching tuples are dropped.
                            let step_result = if step.stages.is_empty() {
                                step_result
                            } else {
                                self.apply_stages(step_result, &step.stages)?
                            };

                            // Restore previous bindings
                            tuple_bindings.restore(self);

                            // Rewrap results as tuples carrying this incoming
                            // tuple's focus/index/ancestor bindings, so that
                            // DOWNSTREAM steps keep seeing them: a predicate like
                            // `[ssn = $e.SSN]` after `Employee@$e.(Contact)`, a
                            // later `%`/`%.%` in `Account.Order.(Product).{...}`,
                            // or a further path step all read those bindings from
                            // the tuple wrapper (see AstNode::Variable's tuple
                            // fallback). Without rewrapping, the tuple chain is
                            // severed after a parenthesized/object/variable step
                            // and those references resolve to nothing. The
                            // wrappers are projected back to their `@` values by
                            // the top-level `unwrap_tuple_output` pass.
                            let carried: Vec<(String, JValue)> = tuple_obj
                                .iter()
                                .filter(|(k, _)| {
                                    (k.starts_with('$') && k.len() > 1) || k.starts_with('!')
                                })
                                .map(|(k, v)| (k.clone(), v.clone()))
                                .collect();
                            let wrap = |v: JValue| -> JValue {
                                match v {
                                    // If the step produced a nested tuple stream
                                    // (e.g. `(Product)` whose inner `Product` is
                                    // itself `%`-tagged), MERGE the inner tuple's
                                    // keys over the carried outer bindings, mirroring
                                    // jsonata-js's `res.tupleStream` branch
                                    // (`Object.assign(tuple, res[bb])`) -- do NOT
                                    // double-wrap, which would bury `@`/`!label`
                                    // one level down and break a following `%`/`%.%`.
                                    JValue::Object(inner)
                                        if inner.get("__tuple__") == Some(&JValue::Bool(true)) =>
                                    {
                                        let mut w = IndexMap::new();
                                        for (k, val) in &carried {
                                            w.insert(k.clone(), val.clone());
                                        }
                                        for (k, val) in inner.iter() {
                                            w.insert(k.clone(), val.clone());
                                        }
                                        w.insert("__tuple__".to_string(), JValue::Bool(true));
                                        JValue::object(w)
                                    }
                                    other => {
                                        let mut w = IndexMap::new();
                                        w.insert("@".to_string(), other);
                                        for (k, val) in &carried {
                                            w.insert(k.clone(), val.clone());
                                        }
                                        w.insert("__tuple__".to_string(), JValue::Bool(true));
                                        JValue::object(w)
                                    }
                                }
                            };
                            if !step_result.is_null() && !step_result.is_undefined() {
                                // Object constructors yield one value per tuple;
                                // other steps may yield an array to splice in.
                                if matches!(&step.node, AstNode::Object(_)) {
                                    results.push(wrap(step_result));
                                } else if let JValue::Array(arr) = step_result {
                                    for it in arr.iter() {
                                        results.push(wrap(it.clone()));
                                    }
                                } else {
                                    results.push(wrap(step_result));
                                }
                            }
                        }
                    }

                    current = JValue::array(results);
                    continue; // Skip the regular step processing
                }
            }

            current = match &step.node {
                AstNode::Wildcard => {
                    // Wildcard in path
                    let stages = &step.stages;
                    let normalized_current = normalize_lazy(&current)?;
                    let wildcard_result = match &normalized_current {
                        JValue::Object(obj) => {
                            let mut result = Vec::new();
                            for value in obj.values() {
                                // Flatten arrays into the result
                                match value {
                                    JValue::Array(arr) => result.extend(arr.iter().cloned()),
                                    _ => result.push(value.clone()),
                                }
                            }
                            JValue::array(result)
                        }
                        JValue::Array(arr) => {
                            // Map wildcard over array
                            let mut all_values = Vec::new();
                            for item in arr.iter() {
                                let normalized_item = normalize_lazy(item)?;
                                match &normalized_item {
                                    JValue::Object(obj) => {
                                        for value in obj.values() {
                                            // Flatten arrays
                                            match value {
                                                JValue::Array(arr) => {
                                                    all_values.extend(arr.iter().cloned())
                                                }
                                                _ => all_values.push(value.clone()),
                                            }
                                        }
                                    }
                                    JValue::Array(inner) => {
                                        all_values.extend(inner.iter().cloned());
                                    }
                                    _ => {}
                                }
                            }
                            JValue::array(all_values)
                        }
                        // See the matching first-step Wildcard arm above:
                        // null and every scalar alike map over nothing.
                        _ => JValue::Undefined,
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
                        JValue::Array(arr) => {
                            // Collect descendants from all array elements
                            let mut all_descendants = Vec::new();
                            for item in arr.iter() {
                                all_descendants.extend(self.collect_descendants(item)?);
                            }
                            JValue::array(all_descendants)
                        }
                        _ => {
                            // Collect descendants from current value
                            let descendants = self.collect_descendants(&current)?;
                            JValue::array(descendants)
                        }
                    }
                }
                AstNode::Name(field_name) => {
                    // Navigate into object field or map over array, applying stages
                    let stages = &step.stages;

                    match &current {
                        JValue::Object(obj) => {
                            // Single object field extraction - NOT array mapping
                            // This resets did_array_mapping because we're extracting from
                            // a single value, not mapping over an array. The field's value
                            // (even if it's an array) should be preserved as-is.
                            did_array_mapping = false;
                            let val = obj.get(field_name).cloned().unwrap_or(JValue::Undefined);
                            // Apply stages if present
                            if !stages.is_empty() {
                                self.apply_stages(val, stages)?
                            } else {
                                val
                            }
                        }
                        #[cfg(feature = "python")]
                        JValue::LazyPyDict(lazy) => {
                            did_array_mapping = false;
                            let val = lazy.get_field(field_name)?;
                            if !stages.is_empty() {
                                self.apply_stages(val, stages)?
                            } else {
                                val
                            }
                        }
                        JValue::Array(arr) => {
                            // Array mapping: extract field from each element and apply stages
                            did_array_mapping = true; // Track that we did array mapping

                            // Fast path: if no elements are tuples and no stages,
                            // skip all tuple checking overhead (common case for products.price etc.)
                            // Tuples are all-or-nothing (created by index binding #$i),
                            // so checking only the first element is sufficient.
                            let has_tuples = arr.first().is_some_and(|item| {
                                matches!(item, JValue::Object(obj) if obj.get("__tuple__") == Some(&JValue::Bool(true)))
                            });

                            if !has_tuples && stages.is_empty() {
                                // Delegate to the shared field step (skip undefined,
                                // keep nulls, flatten one level, recurse into nested
                                // arrays). Mid-path wants the raw sequence: no
                                // singleton unwrap, and an empty result stays an
                                // empty array so the remaining steps map over
                                // nothing exactly as before.
                                match compiled_field_step(field_name, &current, &self.options)? {
                                    JValue::Undefined => JValue::array(Vec::new()),
                                    other => other,
                                }
                            } else {
                                // Full path with tuple support and stages
                                let mut result = Vec::new();

                                for item in arr.iter() {
                                    match item {
                                        JValue::Object(obj) => {
                                            // Check if this is a tuple stream element
                                            let (val, tuple_bindings) = if obj.get("__tuple__")
                                                == Some(&JValue::Bool(true))
                                            {
                                                // This is a tuple - extract '@' value and preserve bindings
                                                // Collect index bindings (variables starting with $)
                                                let bindings: Vec<(String, JValue)> = obj
                                                    .iter()
                                                    .filter(|(k, _)| k.starts_with('$'))
                                                    .map(|(k, v)| (k.clone(), v.clone()))
                                                    .collect();
                                                match obj.get("@") {
                                                    // Absent field -> Undefined, matching the
                                                    // non-tuple arm below, so the guard drops it
                                                    // while keeping a present null.
                                                    Some(JValue::Object(inner)) => (
                                                        inner
                                                            .get(field_name)
                                                            .cloned()
                                                            .unwrap_or(JValue::Undefined),
                                                        Some(bindings),
                                                    ),
                                                    #[cfg(feature = "python")]
                                                    Some(JValue::LazyPyDict(lazy)) => (
                                                        lazy.get_field(field_name)?,
                                                        Some(bindings),
                                                    ),
                                                    _ => continue, // Invalid tuple
                                                }
                                            } else {
                                                (
                                                    obj.get(field_name)
                                                        .cloned()
                                                        .unwrap_or(JValue::Undefined),
                                                    None,
                                                )
                                            };

                                            if !val.is_undefined() {
                                                // Helper to wrap value in tuple if we have bindings
                                                let wrap_in_tuple = |v: JValue, bindings: &Option<Vec<(String, JValue)>>| -> JValue {
                                                    if let Some(b) = bindings {
                                                        let mut wrapper = IndexMap::new();
                                                        wrapper.insert("@".to_string(), v);
                                                        wrapper.insert("__tuple__".to_string(), JValue::Bool(true));
                                                        for (k, val) in b {
                                                            wrapper.insert(k.clone(), val.clone());
                                                        }
                                                        JValue::object(wrapper)
                                                    } else {
                                                        v
                                                    }
                                                };

                                                if !stages.is_empty() {
                                                    // Bind this tuple's carried focus/index/ancestor
                                                    // bindings so a filter predicate that references
                                                    // them resolves -- e.g. `library.loans@$l.books[$l.isbn=isbn]`,
                                                    // where the `[$l.isbn=isbn]` stage on the (non-focus)
                                                    // `books` step must see `$l` from the enclosing
                                                    // `@$l` focus stream. Without this the predicate
                                                    // evaluates `$l` as unbound and filters everything out.
                                                    let saved_tuple: Vec<(String, Option<JValue>)> =
                                                        obj.iter()
                                                            .filter_map(|(k, _)| {
                                                                if let Some(n) = k.strip_prefix('$')
                                                                {
                                                                    (!n.is_empty())
                                                                        .then(|| n.to_string())
                                                                } else if k.starts_with('!') {
                                                                    Some(k.clone())
                                                                } else {
                                                                    None
                                                                }
                                                            })
                                                            .map(|n| {
                                                                (
                                                                    n.clone(),
                                                                    self.context
                                                                        .lookup(&n)
                                                                        .cloned(),
                                                                )
                                                            })
                                                            .collect();
                                                    for (k, v) in obj.iter() {
                                                        if let Some(n) = k.strip_prefix('$') {
                                                            if !n.is_empty() {
                                                                self.context
                                                                    .bind(n.to_string(), v.clone());
                                                            }
                                                        } else if k.starts_with('!') {
                                                            self.context.bind(k.clone(), v.clone());
                                                        }
                                                    }
                                                    // Apply stages to the extracted value
                                                    let processed_val =
                                                        self.apply_stages(val, stages);
                                                    for (n, old) in saved_tuple.into_iter().rev() {
                                                        match old {
                                                            Some(v) => self.context.bind(n, v),
                                                            None => self.context.unbind(&n),
                                                        }
                                                    }
                                                    let processed_val = processed_val?;
                                                    // Stages always return an array (or null); extend results
                                                    match processed_val {
                                                        JValue::Array(arr) => {
                                                            for item in arr.iter() {
                                                                result.push(wrap_in_tuple(
                                                                    item.clone(),
                                                                    &tuple_bindings,
                                                                ));
                                                            }
                                                        }
                                                        // A stage yielding nothing is Undefined; a genuine null result is a
                                                        // value and stays in the sequence.
                                                        JValue::Undefined => {}
                                                        other => result.push(wrap_in_tuple(
                                                            other,
                                                            &tuple_bindings,
                                                        )),
                                                    }
                                                } else {
                                                    // No stages: flatten arrays, push scalars
                                                    // But preserve tuple bindings!
                                                    match val {
                                                        JValue::Array(arr) => {
                                                            for item in arr.iter() {
                                                                result.push(wrap_in_tuple(
                                                                    item.clone(),
                                                                    &tuple_bindings,
                                                                ));
                                                            }
                                                        }
                                                        other => result.push(wrap_in_tuple(
                                                            other,
                                                            &tuple_bindings,
                                                        )),
                                                    }
                                                }
                                            }
                                        }
                                        JValue::Array(_) => {
                                            // Recursively map over nested array
                                            let nested_result =
                                                self.evaluate_path(&[step.clone()], item)?;
                                            match nested_result {
                                                JValue::Array(nested) => {
                                                    result.extend(nested.iter().cloned())
                                                }
                                                JValue::Null => {}
                                                other => result.push(other),
                                            }
                                        }
                                        #[cfg(feature = "python")]
                                        JValue::LazyPyDict(lazy) => {
                                            // Lazy dicts are never tuples; read directly. Mirrors
                                            // the Object arm's non-tuple branch (tuple_bindings =
                                            // None throughout, so wrap_in_tuple would be a no-op).
                                            let val = lazy.get_field(field_name)?;

                                            // Only an absent field (Undefined) drops out; a
                                            // present null is a value.
                                            if !val.is_undefined() {
                                                if !stages.is_empty() {
                                                    let processed_val =
                                                        self.apply_stages(val, stages)?;
                                                    match processed_val {
                                                        JValue::Array(arr) => {
                                                            result.extend(arr.iter().cloned())
                                                        }
                                                        JValue::Undefined => {}
                                                        other => result.push(other),
                                                    }
                                                } else {
                                                    match val {
                                                        JValue::Array(arr) => {
                                                            result.extend(arr.iter().cloned())
                                                        }
                                                        other => result.push(other),
                                                    }
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }

                                JValue::array(result)
                            }
                        }
                        // Accessing field on non-object returns undefined (not an
                        // error) -- including when that non-object is an explicit
                        // null (issue #114): `nul.foo`, `nul.a.b`.
                        _ => JValue::Undefined,
                    }
                }
                AstNode::String(string_literal) => {
                    // String literal as a path step - evaluate as literal and apply stages
                    let stages = &step.stages;
                    let val = JValue::string(string_literal.clone());

                    if !stages.is_empty() {
                        // Apply stages (predicates) to the string literal
                        let result = self.apply_stages(val, stages)?;
                        // Unwrap single-element arrays back to scalar
                        match result {
                            JValue::Array(arr) if arr.len() == 1 => arr[0].clone(),
                            JValue::Array(arr) if arr.is_empty() => JValue::Null,
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
                AstNode::KeepArray => Self::keep_array(&current),
                AstNode::ArrayGroup(elements) => {
                    // Array grouping: map expression over array but keep results grouped
                    // .[expr] means evaluate expr for each array element
                    match &current {
                        JValue::Array(arr) => {
                            let mut result = Vec::new();
                            for item in arr.iter() {
                                // For each array item, evaluate all elements and collect results
                                let mut group_values = Vec::new();
                                for element in elements {
                                    let value = self.evaluate_internal(element, item)?;
                                    // If the element is an Array/ArrayGroup, preserve its structure (don't flatten)
                                    // This ensures [[expr]] produces properly nested arrays
                                    let should_preserve_array = matches!(
                                        element,
                                        AstNode::Array(_) | AstNode::ArrayGroup(_)
                                    );

                                    if should_preserve_array {
                                        // Keep the array as a single element to preserve nesting
                                        group_values.push(value);
                                    } else {
                                        // Flatten the value into group_values. Undefined is
                                        // dropped like in every array constructor (jsonata-js:
                                        // `foo.blah.[baz]` over an element without `baz` is
                                        // `[]`, not `[null]`); explicit null is a value and
                                        // stays.
                                        match value {
                                            JValue::Array(arr) => {
                                                group_values.extend(arr.iter().cloned())
                                            }
                                            JValue::Undefined => {}
                                            other => group_values.push(other),
                                        }
                                    }
                                }
                                // Each array element gets its own sub-array with all values
                                result.push(JValue::array(group_values));
                            }
                            // jsonata-js's evaluateStep: when this is the path's last
                            // step and mapping produced exactly one constructed
                            // sub-array, that sub-array IS the path result directly
                            // (not wrapped in an outer singleton array) — e.g.
                            // `$.[value,epochSeconds]` over a 1-element array yields
                            // `[3, 1578381600]`, not `[[3, 1578381600]]`.
                            if result.is_empty() {
                                // Mapping over an EMPTY array produced nothing at
                                // all — undefined, like any step mapped over an
                                // empty sequence (`emptyarr.[b]`), NOT a kept empty
                                // array (that is the single-value construction
                                // case, handled in finalize_path_result).
                                JValue::Undefined
                            } else if is_last_step && result.len() == 1 {
                                result.into_iter().next().unwrap()
                            } else {
                                JValue::array(result)
                            }
                        }
                        _ => {
                            // For non-arrays, just evaluate the array constructor
                            // normally; undefined elements are dropped like in every
                            // array constructor (`{"a":1}.[b]` is `[]`).
                            let mut result = Vec::new();
                            for element in elements {
                                let value = self.evaluate_internal(element, &current)?;
                                if !value.is_undefined() {
                                    result.push(value);
                                }
                            }
                            JValue::array(result)
                        }
                    }
                }
                AstNode::FunctionApplication(expr) => {
                    // Function application: map expr over the current value
                    // .(expr) means evaluate expr for each element, with $ bound to that element
                    // Null/undefined results are filtered out
                    //
                    // When this parenthesized step is itself tuple-carrying (its
                    // inner path has a `%`-tagged step, e.g. `Account.(Order.Product).{...}`),
                    // keep the inner path's tuple wrappers so their `!label`
                    // bindings survive to the following object/`%` step; the
                    // end-of-path projection (or a later consumer) unwraps them.
                    let saved_keep = self.keep_tuple_stream;
                    if step.is_tuple {
                        self.keep_tuple_stream = true;
                    }
                    let fa_result = match &current {
                        JValue::Array(arr) => {
                            // Produce the mapped result (compiled fast path or tree-walker fallback).
                            // Do NOT return early — singleton unwrapping is applied by the outer
                            // path evaluation code after all steps are processed.
                            let mapped: Vec<JValue> = if let Some(compiled) = try_compile_expr(expr)
                            {
                                let shape = arr.first().and_then(build_shape_cache);
                                let mut result = Vec::with_capacity(arr.len());
                                for item in arr.iter() {
                                    let value = if let Some(ref s) = shape {
                                        eval_compiled_shaped(
                                            &compiled,
                                            item,
                                            None,
                                            s,
                                            &self.options,
                                            self.start_time,
                                        )?
                                    } else {
                                        eval_compiled(
                                            &compiled,
                                            item,
                                            None,
                                            &self.options,
                                            self.start_time,
                                        )?
                                    };
                                    if !value.is_null() && !value.is_undefined() {
                                        result.push(value);
                                    }
                                }
                                result
                            } else {
                                let mut result = Vec::new();
                                for item in arr.iter() {
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
                                    if !value.is_null() && !value.is_undefined() {
                                        result.push(value);
                                    }
                                }
                                result
                            };
                            // Don't do singleton unwrapping here - let the path result
                            // handling deal with it, which respects has_explicit_array_keep
                            JValue::array(mapped)
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
                    };
                    self.keep_tuple_stream = saved_keep;
                    fa_result
                }
                AstNode::Sort { terms, .. } => {
                    // Sort as a path step - sort 'current' by the terms
                    self.evaluate_sort(&current, terms)?
                }
                // Handle complex path steps (e.g., computed properties, object construction)
                _ => {
                    // These steps map over an array just as a Name step does, so
                    // the result is a sequence and a singleton unwraps:
                    // `arr.{"k": p}` over one element is the object, not `[object]`.
                    if matches!(current, JValue::Array(_)) {
                        did_array_mapping = true;
                    }
                    let saved_keep = self.keep_tuple_stream;
                    if step.is_tuple {
                        self.keep_tuple_stream = true;
                    }
                    let v = self.evaluate_path_step(&step.node, &current, data, false);
                    self.keep_tuple_stream = saved_keep;
                    v?
                }
            };
        }

        self.finalize_path_result(steps, current, did_array_mapping)
    }

    /// End-of-path result policy, applied once after the step loop: project a
    /// tuple stream down to its visible `@` values (unless a consumer asked to
    /// keep the wrappers), decide singleton unwrapping vs `[]` keep-array,
    /// collapse an empty result sequence to undefined, and enforce D2015.
    ///
    /// Extracted from `evaluate_path` verbatim so the policy has a name; the
    /// inline comments below are the rule.
    fn finalize_path_result(
        &mut self,
        steps: &[PathStep],
        mut current: JValue,
        did_array_mapping: bool,
    ) -> Result<JValue, EvaluatorError> {
        // End-of-path tuple projection, mirroring jsonata-js evaluatePath
        // (jsonata.js ~L202-212): once the path is a tuple stream, its VISIBLE
        // result is each tuple's `@` value; the `{@, $var, !label, __tuple__}`
        // wrappers are internal bookkeeping and must not escape into an enclosing
        // operator (e.g. `$#$pos[$pos<3] = $[[0..2]]`, where leaked wrappers make
        // `=` compare wrapper objects and always yield false). Suppressed only for
        // the two consumers that read the carried bindings directly off the
        // wrappers (Sort input, ObjectTransform/group-by input), which set
        // `keep_tuple_stream`. The top-level `evaluate()` still runs
        // `unwrap_tuple_output` as a backstop for wrappers nested inside
        // constructed output.
        if !self.keep_tuple_stream {
            if let JValue::Array(arr) = &current {
                let is_tuple_stream = arr.first().is_some_and(|f| {
                    matches!(f, JValue::Object(o) if o.get("__tuple__") == Some(&JValue::Bool(true)))
                });
                if is_tuple_stream {
                    let projected: Vec<JValue> = arr
                        .iter()
                        .map(|t| match t {
                            JValue::Object(o) => o.get("@").cloned().unwrap_or(JValue::Undefined),
                            other => other.clone(),
                        })
                        .collect();
                    current = JValue::array(projected);
                }
            }
        }

        // JSONata singleton unwrapping: singleton results are unwrapped when we did array operations
        // BUT NOT when there's an explicit array-keeping operation like [] (empty predicate)

        // Check for explicit array-keeping operations. Empty predicate `[]` can
        // be a `Predicate(Boolean(true))` step node or a `Filter(Boolean(true))`
        // stage; it also counts when it sits inside a `Sort` step's input path
        // (e.g. `$#$pos[][$pos<3]^($)[-1]`), whose keep-array-ness must survive
        // the sort and the trailing index so the singleton stays `[4]`.
        let has_explicit_array_keep = Self::path_keeps_singleton_array(steps);

        // Unwrap when:
        // 1. Any step is an array operation -- a stage (sort, filter) or a
        //    `Predicate` step node. Both spellings must count: `arr[p = 1]`
        //    parses the predicate as a step *node* with empty stages, so
        //    checking `stages` alone missed every filter written that way.
        // 2. We did array mapping during step evaluation (tracked via did_array_mapping flag)
        //    Note: did_array_mapping is reset to false when extracting from a single object,
        //    so a[0].b where a[0] returns a single object and .b extracts a field will NOT unwrap.
        // BUT NOT when there's an explicit array-keeping operation
        //
        // Important: We DON'T unwrap just because original data was an array - what matters is
        // whether the final extraction was from an array mapping context or a single object.
        // A numeric-literal predicate is index access: `evaluate_predicate`
        // returns the selected element itself, already final. A filter
        // predicate returns a sequence, which is what the singleton rule
        // unwraps. Counting index access here would unwrap it a second time
        // and turn `a[0]` over `[[5]]` into `5` instead of `[5]`.
        //
        // `**` (Descendant) always produces a query-result sequence too --
        // `collect_descendants` returns exactly one item for any leaf value
        // (`a.**` over `{"a": 5}` is `5`, not `[5]`; `nul.**` is `null`, not
        // `[null]`, issue #114) -- so it belongs in this list the same way a
        // filter predicate does, independent of whether `did_array_mapping`
        // got set for this particular current value.
        //
        // `*` (Wildcard) is a sequence for exactly the same reason and was
        // simply missing from the list: `deep.*` over `{"a": {"b": 1}}` is the
        // inner object, not a one-element array wrapping it (#126 group 1).
        //
        // `#$i` (positional binding) is a third: it turns its step into a
        // tuple stream, and a stream of one unwraps like any other sequence
        // (`num#$i` is `5`, not `[5]`). It is a *modifier* on the step rather
        // than a node, which is why it was missed here -- everything else in
        // this list is matched on `step.node`. The rule holds for every input
        // kind; `arr#$i` and `arrobj#$i` only ever looked right because a
        // multi-element result has no singleton to unwrap.
        let has_array_op = steps.iter().any(|step| {
            !step.stages.is_empty()
                || step.index_var.is_some()
                || matches!(&step.node, AstNode::Predicate(p) if !matches!(**p, AstNode::Number(_)))
                || matches!(&step.node, AstNode::Descendant | AstNode::Wildcard)
        });
        let should_unwrap = !has_explicit_array_keep && (has_array_op || did_array_mapping);

        let result = match &current {
            // An empty result sequence is "no value" -> undefined (jsonata-js
            // treats an empty sequence, e.g. from a filter that matched nothing,
            // as undefined so a following `.field` and object/array construction
            // drop it rather than keeping an explicit null). `[]` array-keep is
            // handled separately above via has_explicit_array_keep.
            // ... unless `[]` asked for the array to be kept *and* the empty
            // array is a value rather than an empty result sequence.
            // `emptyarr[]` is `[]` -- the field holds a real empty array --
            // while `arr.p[]` over `{"arr": []}` mapped over nothing and stays
            // undefined. `did_array_mapping` is what tells the two apart; a
            // path that found nothing never even reaches the predicate,
            // because the step loop returns undefined first, which is why
            // `nope[]` needs no special case here.
            // ...and a trailing `.[...]` group over a SINGLE value is a
            // constructed array, not a result sequence: `{"a":1}.[b]` is `[]`
            // in jsonata-js, while `emptyarr.[b]` (mapped over nothing) is
            // undefined — `did_array_mapping` is again what tells them apart.
            JValue::Array(arr)
                if arr.is_empty()
                    && (did_array_mapping || !has_explicit_array_keep)
                    && (did_array_mapping
                        || !matches!(
                            steps.last().map(|s| &s.node),
                            Some(AstNode::ArrayGroup(_))
                        )) =>
            {
                JValue::Undefined
            }
            // Unwrap singleton arrays when appropriate
            JValue::Array(arr) if arr.len() == 1 && should_unwrap => arr[0].clone(),
            // Keep arrays otherwise
            _ => current,
        };

        // An explicit `[]` keep-array forces the result to remain an array even
        // after a later singleton index collapses it to a scalar (jsonata's
        // keepSingleton), e.g. `$#$pos[][$pos<3]^($)[-1]` must yield `[4]`.
        // An explicit null is re-wrapped like any other value -- `nul[][0]`
        // is `[null]`. Only undefined stays out: there is nothing to keep.
        let result =
            if has_explicit_array_keep && !matches!(result, JValue::Array(_) | JValue::Undefined) {
                JValue::array(vec![result])
            } else {
                result
            };

        if let JValue::Array(arr) = &result {
            check_sequence_length(arr.len(), &self.options)?;
        }

        Ok(result)
    }

    /// True when a path step carries a tuple-binding flag (`@$var` focus,
    /// `#$var` index, or a resolved `%` ancestor label) and must therefore
    /// produce/extend a tuple stream rather than be evaluated as a plain step.
    ///
    fn step_creates_tuple(step: &PathStep) -> bool {
        step.focus.is_some() || step.index_var.is_some() || step.ancestor_label.is_some()
    }

    /// True when a path contains an explicit empty predicate `[]` (keep-array),
    /// either directly as a step/stage or nested inside a `Sort` step's input
    /// path. The keep-array-ness of an inner `[]` must survive an enclosing sort
    /// and trailing index so a singleton result stays wrapped (`$#$pos[]...^()[-1]`
    /// -> `[4]`).
    /// `[]` -- jsonata's keepSingleton. Forces the value to be an array,
    /// leaving an existing one alone. An explicit null is a value like any
    /// other, so `nul[]` is `[null]`.
    fn keep_array(value: &JValue) -> JValue {
        match value {
            JValue::Array(arr) => JValue::Array(arr.clone()),
            other => JValue::array(vec![other.clone()]),
        }
    }

    fn path_keeps_singleton_array(steps: &[PathStep]) -> bool {
        steps.iter().any(|step| {
            if matches!(&step.node, AstNode::KeepArray) {
                return true;
            }
            if step.stages.iter().any(|s| matches!(s, Stage::KeepArray)) {
                return true;
            }
            if let AstNode::Sort { input, .. } = &step.node {
                if let AstNode::Path { steps: inner } = input.as_ref() {
                    return Self::path_keeps_singleton_array(inner);
                }
            }
            false
        })
    }

    /// Bind a tuple wrapper's carried `$name`/`!label` keys into the current
    /// scope, saving whatever was previously bound under each of those names
    /// so [`TupleKeyBindings::restore`] can put it back afterward.
    ///
    /// This is the single shared implementation of the
    /// "iterate a tuple wrapper's carried keys, bind, evaluate, then undo"
    /// pattern that recurs across `create_tuple_stream`,
    /// `needs_tuple_context_binding`'s handling in `evaluate_path`,
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
    ) -> Result<Vec<JValue>, EvaluatorError> {
        use std::rc::Rc;

        // Mark that this evaluate() call produced tuple wrappers, so the
        // top-level `evaluate()` knows to run the output-unwrap pass.
        self.tuple_stream_created = true;

        // Gather the incoming tuple bindings.
        let is_tuple_input = matches!(
            input,
            JValue::Array(arr) if arr.first().is_some_and(|f| {
                matches!(f, JValue::Object(o) if o.get("__tuple__") == Some(&JValue::Bool(true)))
            })
        );
        let incoming: Vec<Rc<IndexMap<String, JValue>>> = if is_tuple_input {
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
                    Rc::new(wrapper)
                })
                .collect()
        };

        // A sort step in a tuple stream orders the WHOLE stream (not per element)
        // and re-tuples with the index = sorted position, mirroring jsonata-js
        // evaluateTupleStep's `sort` case. `$^($)#$pos[$pos<3]` must sort the
        // array, then number the sorted values, then filter by `$pos`.
        if let AstNode::Sort { terms, .. } = &step.node {
            let stream = JValue::array(
                incoming
                    .iter()
                    .map(|t| JValue::object((**t).clone()))
                    .collect(),
            );
            // evaluate_sort is tuple-aware (orders by each wrapper's `@`, with the
            // carried keys bound), returning the wrappers in sorted order.
            let sorted = self.evaluate_sort(&stream, terms)?;
            let sorted_arr: Vec<JValue> = match sorted {
                JValue::Array(a) => a.iter().cloned().collect(),
                JValue::Null | JValue::Undefined => Vec::new(),
                other => vec![other],
            };
            let mut result = Vec::new();
            for (ss, elem) in sorted_arr.into_iter().enumerate() {
                let mut new_tuple = match elem {
                    JValue::Object(o) => (*o).clone(),
                    other => {
                        let mut m = IndexMap::new();
                        m.insert("@".to_string(), other);
                        m
                    }
                };
                if let Some(index_var) = &step.index_var {
                    new_tuple.insert(format!("${}", index_var), JValue::from(ss as i64));
                }
                new_tuple.insert("__tuple__".to_string(), JValue::Bool(true));
                result.push(JValue::object(new_tuple));
            }
            return Ok(result);
        }

        let mut result = Vec::new();
        for tuple_obj in incoming {
            // Bind every carried tuple key into a real scope frame so the step
            // expression can see prior focus/index/ancestor bindings, mirroring
            // createFrameFromTuple's "for every key in tuple, frame.bind(...)".
            // Saves/restores rather than blindly unbinding, so a tuple key
            // whose name collides with a live outer `:=` binding doesn't get
            // deleted once this tuple row's evaluation is done.
            let tuple_bindings = self.bind_tuple_keys(&tuple_obj);

            let actual_data = tuple_obj.get("@").cloned().unwrap_or(JValue::Undefined);
            let step_value = self.evaluate_internal(&step.node, &actual_data);

            let mut step_value = step_value?;
            // When the step carries an ORDERED index stage (a second `#$var`,
            // e.g. `books@$b#$ib[...]#$ib2`), its stages must be applied to the
            // BUILT tuple stream in order (filter then re-number) so the filter
            // sees the per-tuple focus/index bindings and each index reflects the
            // position at its point in the sequence. Those steps defer all stage
            // application to `apply_tuple_stages` after the stream is built.
            let has_index_stage = step.stages.iter().any(|s| matches!(s, Stage::Index(_)));
            if !step.stages.is_empty() && !has_index_stage {
                // A `%` inside a filter predicate refers to the ancestry of
                // THIS step (its own input for a level-1 `%`, or an earlier
                // step's input for a `%.%` chain). ast_transform tags this step
                // with `ancestor_label`; bind it to the step's input so the
                // level-1 `%` resolves. The `%.%` chain's deeper references use
                // labels carried in the INCOMING tuple, so those bindings
                // (`tuple_bindings`) must stay live through `apply_stages` --
                // their restore is deferred until after it (previously they
                // were unbound first, which silently broke `%.%` inside
                // predicates).
                let own_label = match &step.ancestor_label {
                    Some(label) if !tuple_bindings.contains(label) => {
                        self.context.bind(label.clone(), actual_data.clone());
                        Some(label.clone())
                    }
                    _ => None,
                };
                step_value = self.apply_stages(step_value, &step.stages)?;
                if let Some(label) = own_label {
                    self.context.unbind(&label);
                }
            }

            tuple_bindings.restore(self);

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
                result.push(JValue::object(new_tuple));
            }
        }

        // Apply ordered filter/index stages to the built tuple stream when a
        // second index binding deferred them (see the has_index_stage comment
        // in the build loop above).
        if step.stages.iter().any(|s| matches!(s, Stage::Index(_))) {
            result = self.apply_tuple_stages(result, &step.stages)?;
        }

        Ok(result)
    }

    /// Apply a step's stages, in order, to an already-built tuple stream --
    /// mirrors jsonata-js `evaluateStages` (jsonata.js ~L288-305): a `filter`
    /// keeps the tuples whose predicate is truthy (evaluated against each tuple's
    /// `@` with its carried `$var`/`!label` bindings in scope), and an `index`
    /// stage sets its variable on every surviving tuple to that tuple's position
    /// in the CURRENT stream. Used for steps carrying a second `#$var` index
    /// binding (e.g. `books@$b#$ib[$l.isbn=$b.isbn]#$ib2`), where `$ib` is the
    /// pre-filter position and `$ib2` the post-filter position.
    fn apply_tuple_stages(
        &mut self,
        mut tuples: Vec<JValue>,
        stages: &[Stage],
    ) -> Result<Vec<JValue>, EvaluatorError> {
        for stage in stages {
            match stage {
                Stage::Filter(pred) => {
                    let mut kept = Vec::with_capacity(tuples.len());
                    for tup in tuples.into_iter() {
                        let JValue::Object(obj) = &tup else {
                            continue;
                        };
                        // Bind this tuple's carried focus/index/ancestor keys so
                        // the predicate can reference them (save/restore rather
                        // than blind unbind -- see bind_tuple_keys).
                        let tuple_bindings = self.bind_tuple_keys(obj);
                        let at = obj.get("@").cloned().unwrap_or(JValue::Undefined);
                        let pred_res = self.evaluate_internal(pred, &at);
                        tuple_bindings.restore(self);
                        if self.is_truthy(&pred_res?) {
                            kept.push(tup);
                        }
                    }
                    tuples = kept;
                }
                Stage::Index(var) => {
                    for (pos, tup) in tuples.iter_mut().enumerate() {
                        if let JValue::Object(obj) = tup {
                            let mut m = (**obj).clone();
                            m.insert(format!("${}", var), JValue::from(pos as i64));
                            *tup = JValue::object(m);
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

    /// Helper to evaluate a complex path step
    /// `is_first_step` distinguishes the head of a path from a later step. The
    /// head is evaluated against the input itself, so an expression that does
    /// not reference the input -- an object constructor, say -- still evaluates
    /// when the input is undefined: `{"a": 1}.a` is 1 with no input at all. A
    /// *later* step over an undefined value has nothing to map and stays
    /// undefined.
    fn evaluate_path_step(
        &mut self,
        step: &AstNode,
        current: &JValue,
        original_data: &JValue,
        is_first_step: bool,
    ) -> Result<JValue, EvaluatorError> {
        // Special case: array mapping with object construction
        // e.g., items.{"name": name, "price": price}
        if matches!(current, JValue::Array(_)) && matches!(step, AstNode::Object(_)) {
            match (current, step) {
                (JValue::Array(arr), AstNode::Object(pairs)) => {
                    // Try CompiledExpr for object construction (handles arithmetic, conditionals, etc.)
                    if let Some(compiled) = try_compile_expr(&AstNode::Object(pairs.clone())) {
                        let shape = arr.first().and_then(build_shape_cache);
                        let mut mapped = Vec::with_capacity(arr.len());
                        for item in arr.iter() {
                            let result = if let Some(ref s) = shape {
                                eval_compiled_shaped(
                                    &compiled,
                                    item,
                                    None,
                                    s,
                                    &self.options,
                                    self.start_time,
                                )?
                            } else {
                                eval_compiled(
                                    &compiled,
                                    item,
                                    None,
                                    &self.options,
                                    self.start_time,
                                )?
                            };
                            if !result.is_undefined() {
                                mapped.push(result);
                            }
                        }
                        return Ok(JValue::array(mapped));
                    }
                    // Fallback: full AST evaluation per element
                    let mapped: Result<Vec<JValue>, EvaluatorError> = arr
                        .iter()
                        .map(|item| self.evaluate_internal(step, item))
                        .collect();
                    Ok(JValue::array(mapped?))
                }
                _ => unreachable!(),
            }
        } else {
            // Special case: array.$ should map $ over the array, returning each element
            // e.g., [1, 2, 3].$ returns [1, 2, 3]
            if let AstNode::Variable(name) = step {
                if name.is_empty() {
                    // Bare $ - map over array if current is an array
                    if let JValue::Array(arr) = current {
                        // Map $ over each element - $ refers to each element in turn
                        return Ok(JValue::Array(arr.clone()));
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
                if let JValue::Array(arr) = current {
                    // Check if this is a tuple array
                    let is_tuple_array = arr.first().is_some_and(|first| {
                        if let JValue::Object(obj) = first {
                            obj.get("__tuple__") == Some(&JValue::Bool(true))
                        } else {
                            false
                        }
                    });

                    if is_tuple_array {
                        // Map the variable lookup over each tuple
                        let mut results = Vec::new();
                        for tuple in arr.iter() {
                            // Evaluate the variable in the context of this tuple
                            // This allows tuple bindings ($i, etc.) to be found
                            let val = self.evaluate_internal(step, tuple)?;
                            if !val.is_null() && !val.is_undefined() {
                                results.push(val);
                            }
                        }
                        return Ok(JValue::array(results));
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
            // An object constructor as a path step builds from the value at that
            // step, not from the root: `arr.{"k": p}` over `{"p": 1}` reads `p`
            // from that object. The array case is handled above; this is the
            // singleton form. An undefined step value stays undefined.
            if matches!(step, AstNode::Object(_)) {
                if matches!(current, JValue::Undefined) && !is_first_step {
                    return Ok(JValue::Undefined);
                }
                return self.evaluate_internal(step, current);
            }

            if matches!(
                step,
                AstNode::Binary { .. }
                    | AstNode::Function { .. }
                    | AstNode::Variable(_)
                    | AstNode::ParentVariable(_)
                    | AstNode::Parent(_)
                    | AstNode::Array(_)
                    | AstNode::Sort { .. }
                    | AstNode::Block(_)
            ) {
                // Evaluate the step in the context of original_data and return the result directly
                return self.evaluate_internal(step, original_data);
            }

            // Standard path step evaluation for indexing/accessing current value
            let step_value = self.evaluate_internal(step, original_data)?;
            Ok(match (current, &step_value) {
                (JValue::Object(obj), JValue::String(key)) => {
                    obj.get(&**key).cloned().unwrap_or(JValue::Undefined)
                }
                #[cfg(feature = "python")]
                (JValue::LazyPyDict(lazy), JValue::String(key)) => lazy.get_field(key)?,
                (JValue::Array(arr), JValue::Number(n)) => {
                    let index = *n as i64;
                    let len = arr.len() as i64;

                    // Handle negative indexing (offset from end)
                    let actual_idx = if index < 0 { len + index } else { index };

                    if actual_idx < 0 || actual_idx >= len {
                        JValue::Undefined
                    } else {
                        arr[actual_idx as usize].clone()
                    }
                }
                _ => JValue::Undefined,
            })
        }
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
                    // For paths and variables, undefined (no match/unbound) - use RHS
                    else if value.is_undefined()
                        && (matches!(lhs, AstNode::Path { .. })
                            || matches!(lhs, AstNode::String(_))
                            || matches!(lhs, AstNode::Variable(_)))
                    {
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
            // Handle regex on RHS - treat as $match(lhs, regex)
            if let AstNode::Regex { pattern, flags } = rhs {
                // Evaluate LHS
                let lhs_value = self.evaluate_internal(lhs, data)?;
                // Do regex match inline
                return match lhs_value {
                    JValue::String(s) => {
                        // Build the regex via the shared flag translation
                        // ($split/$replace use the same helper, so i/m/s
                        // behave identically on every entry point).
                        match crate::functions::string::build_regex(pattern, flags)
                            .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))
                        {
                            Ok(re) => {
                                if let Some(m) = re.find(&s) {
                                    // Return match object
                                    let mut result = IndexMap::new();
                                    result.insert(
                                        "match".to_string(),
                                        JValue::string(m.as_str().to_string()),
                                    );
                                    result.insert(
                                        "start".to_string(),
                                        JValue::Number(m.start() as f64),
                                    );
                                    result
                                        .insert("end".to_string(), JValue::Number(m.end() as f64));

                                    // Capture groups
                                    let mut groups = Vec::new();
                                    for cap in re.captures_iter(&s).take(1) {
                                        for i in 1..cap.len() {
                                            if let Some(c) = cap.get(i) {
                                                groups.push(JValue::string(c.as_str().to_string()));
                                            }
                                        }
                                    }
                                    if !groups.is_empty() {
                                        result.insert("groups".to_string(), JValue::array(groups));
                                    }

                                    Ok(JValue::object(result))
                                } else {
                                    Ok(JValue::Null)
                                }
                            }
                            Err(e) => Err(EvaluatorError::EvaluationError(format!(
                                "Invalid regex: {}",
                                e
                            ))),
                        }
                    }
                    JValue::Null => Ok(JValue::Null),
                    _ => Err(EvaluatorError::TypeError(
                        "Left side of ~> /regex/ must be a string".to_string(),
                    )),
                };
            }

            // Early check: if LHS evaluates to undefined, return undefined.
            // This matches JSONata behavior where undefined ~> anyFunc returns
            // undefined. An explicit null LHS is NOT short-circuited here
            // (issue #116): null is an ordinary value in jsonata-js, and each
            // RHS kind below evaluates `lhs` again on its own terms -- as a
            // function argument (`nul ~> $string()` == `$string(nul)` ==
            // "null"), as a transform's `$` binding, etc. -- so it gets the
            // same null-vs-undefined handling a direct call would.
            let lhs_value_for_check = self.evaluate_internal(lhs, data)?;
            if lhs_value_for_check.is_undefined() {
                return Ok(JValue::Undefined);
            }

            // Handle different RHS types
            match rhs {
                AstNode::Function {
                    name,
                    args,
                    is_builtin,
                } => {
                    // RHS is a function call
                    // Check if the function call has placeholder arguments (partial application)
                    let has_placeholder =
                        args.iter().any(|arg| matches!(arg, AstNode::Placeholder));

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
                        let result =
                            self.evaluate_function_call(name, &filled_args, *is_builtin, data);

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
                        return self
                            .evaluate_function_call(name, &all_args, *is_builtin, data)
                            .map(|v| self.unwrap_singleton(v));
                    }
                }
                AstNode::Variable(var_name) => {
                    // RHS is a function reference (no parens)
                    // e.g., $average($tempReadings) ~> $round
                    let all_args = vec![lhs.clone()];
                    // Unwrap singleton results from chain operator
                    return self
                        .evaluate_function_call(var_name, &all_args, true, data)
                        .map(|v| self.unwrap_singleton(v));
                }
                AstNode::Binary {
                    op: BinaryOp::ChainPipe,
                    ..
                } => {
                    // RHS is another chain pipe - evaluate LHS first, then pipe through RHS
                    // e.g., x ~> (f1 ~> f2) => (x ~> f1) ~> f2
                    let lhs_value = self.evaluate_internal(lhs, data)?;
                    return self.evaluate_internal(rhs, &lhs_value);
                }
                AstNode::Transform { .. } => {
                    // RHS is a transform - invoke it with LHS as input
                    // Evaluate LHS first
                    let lhs_value = self.evaluate_internal(lhs, data)?;

                    // jsonata-js compiles the transform operator into a
                    // function with signature `<(oa):o>` (first argument:
                    // object or array), so piping any other type into it --
                    // including an explicit null (issue #116) -- is a type
                    // error, not a silent passthrough.
                    if !matches!(lhs_value, JValue::Object(_) | JValue::Array(_)) {
                        #[cfg(feature = "python")]
                        let is_lazy = matches!(lhs_value, JValue::LazyPyDict(_));
                        #[cfg(not(feature = "python"))]
                        let is_lazy = false;
                        if !is_lazy {
                            return Err(EvaluatorError::TypeError(
                                "T0410: Argument 1 of function undefined does not match function signature".to_string(),
                            ));
                        }
                    }

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
                AstNode::Lambda {
                    params,
                    body,
                    signature,
                    thunk,
                } => {
                    // RHS is a lambda - invoke it with LHS as argument
                    let lhs_value = self.evaluate_internal(lhs, data)?;
                    // Unwrap singleton results from chain operator
                    return self
                        .invoke_lambda(params, body, signature.as_ref(), &[lhs_value], data, *thunk)
                        .map(|v| self.unwrap_singleton(v));
                }
                AstNode::Path { steps } => {
                    // RHS is a path expression (e.g., function call with predicate: $map($f)[])
                    // If the first step is a function call, we need to add LHS as first argument
                    if let Some(first_step) = steps.first() {
                        match &first_step.node {
                            AstNode::Function {
                                name,
                                args,
                                is_builtin,
                            } => {
                                // Prepend LHS to the function arguments
                                let mut all_args = vec![lhs.clone()];
                                all_args.extend_from_slice(args);

                                // Call the function
                                let mut result = self.evaluate_function_call(
                                    name,
                                    &all_args,
                                    *is_builtin,
                                    data,
                                )?;

                                // Apply stages from the first step (e.g., predicates)
                                for stage in &first_step.stages {
                                    match stage {
                                        Stage::Filter(filter_expr) => {
                                            result = self.evaluate_predicate_as_stage(
                                                &result,
                                                filter_expr,
                                            )?;
                                        }
                                        Stage::Index(_) => {}
                                        // `[]` only forces the result to stay
                                        // an array, which the caller's
                                        // keep-singleton handling already
                                        // does; there is nothing to apply.
                                        Stage::KeepArray => {}
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
                                let mut result =
                                    self.evaluate_function_call(var_name, &all_args, true, data)?;

                                // Apply stages from the first step
                                for stage in &first_step.stages {
                                    match stage {
                                        Stage::Filter(filter_expr) => {
                                            result = self.evaluate_predicate_as_stage(
                                                &result,
                                                filter_expr,
                                            )?;
                                        }
                                        Stage::Index(_) => {}
                                        // `[]` only forces the result to stay
                                        // an array, which the caller's
                                        // keep-singleton handling already
                                        // does; there is nothing to apply.
                                        Stage::KeepArray => {}
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
                                return self
                                    .evaluate_internal(rhs, &lhs_value)
                                    .map(|v| self.unwrap_singleton(v));
                            }
                        }
                    }

                    // Empty path? Shouldn't happen, but handle it
                    let lhs_value = self.evaluate_internal(lhs, data)?;
                    return self
                        .evaluate_internal(rhs, &lhs_value)
                        .map(|v| self.unwrap_singleton(v));
                }
                _ => {
                    return Err(EvaluatorError::TypeError(
                        "T2006: The right side of the function application operator ~> must be a function"
                            .to_string(),
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
                let compiled_body = if !thunk {
                    let var_refs: Vec<&str> = params.iter().map(|s| s.as_str()).collect();
                    try_compile_expr_with_allowed_vars(body, &var_refs)
                } else {
                    None
                };
                let stored_lambda = StoredLambda {
                    params: params.clone(),
                    body: (**body).clone(),
                    compiled_body,
                    signature: signature.clone(),
                    captured_env,
                    captured_data: Some(data.clone()),
                    thunk: *thunk,
                };
                let lambda_params = stored_lambda.params.clone();
                let lambda_sig = stored_lambda.signature.clone();
                self.context.bind_lambda(var_name.clone(), stored_lambda);

                // Return a lambda marker value (include _lambda_id so it can be found later)
                let lambda_repr = JValue::lambda(
                    var_name.as_str(),
                    lambda_params,
                    Some(var_name.clone()),
                    lambda_sig,
                );
                return Ok(lambda_repr);
            }

            // Check if RHS is a pure function composition (ChainPipe between function references)
            // e.g., $uppertrim := $trim ~> $uppercase
            // This creates a lambda that composes the functions.
            // But NOT for data ~> function, which should be evaluated immediately.
            // e.g., $result := data ~> $map($fn) should evaluate the pipe
            if let AstNode::Binary {
                op: BinaryOp::ChainPipe,
                lhs: chain_lhs,
                rhs: chain_rhs,
            } = rhs
            {
                // Only wrap in lambda if LHS is a function reference (Variable pointing to a function)
                // If LHS is data (array, object, function call result, etc.), evaluate the pipe
                let is_function_composition = match chain_lhs.as_ref() {
                    // LHS is a function reference like $trim or $sum
                    AstNode::Variable(name)
                        if self.is_builtin_function(name)
                            || self.context.lookup_lambda(name).is_some() =>
                    {
                        true
                    }
                    // LHS is another ChainPipe (nested composition like $f ~> $g ~> $h)
                    AstNode::Binary {
                        op: BinaryOp::ChainPipe,
                        ..
                    } => true,
                    // A function call with placeholder creates a partial application
                    // e.g., $substringAfter(?, "@") ~> $substringBefore(?, ".")
                    AstNode::Function { args, .. }
                        if args.iter().any(|a| matches!(a, AstNode::Placeholder)) =>
                    {
                        true
                    }
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
                        compiled_body: None, // ChainPipe body is not compilable
                        signature: None,
                        captured_env: self.capture_current_environment(),
                        captured_data: Some(data.clone()),
                        thunk: false,
                    };
                    self.context.bind_lambda(var_name.clone(), stored_lambda);

                    // Return a lambda marker value (include _lambda_id for later lookup)
                    let lambda_repr = JValue::lambda(
                        var_name.as_str(),
                        vec!["$".to_string()],
                        Some(var_name.clone()),
                        None::<String>,
                    );
                    return Ok(lambda_repr);
                }
                // If not function composition, fall through to normal evaluation below
            }

            // Evaluate the RHS
            let value = self.evaluate_internal(rhs, data)?;

            // If the value is a lambda, alias the shared stored lambda under
            // the new variable name (refcount bump, not a copy)
            if let Some(stored) = self.lookup_lambda_from_value(&value) {
                self.context.bind_lambda_rc(var_name.clone(), stored);
            }

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
            [Stage::Filter(pred)] => try_compile_expr(pred),
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

        for item in arr.iter() {
            // Apply compiled filter if present
            if let Some(ref compiled) = filter_compiled {
                let result = if let Some(ref s) = shape {
                    eval_compiled_shaped(compiled, item, None, s, &self.options, self.start_time)?
                } else {
                    eval_compiled(compiled, item, None, &self.options, self.start_time)?
                };
                if !compiled_is_truthy(&result) {
                    continue;
                }
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
                "sum" => total += val,
                "max" => max_val = max_val.max(val),
                "min" => min_val = min_val.min(val),
                "average" => {
                    total += val;
                    count += 1;
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
            if let JValue::Builtin { name: builtin_name } = &value {
                // This is a built-in function reference (e.g., $f bound to $sum),
                // called directly with an explicit argument list -- not passed as
                // an argument to another function, so jsonata-js does not wrap it
                // in a closure. `by_reference: false` (see call_builtin_with_values).
                let mut evaluated_args = Vec::with_capacity(args.len());
                for arg in args {
                    evaluated_args.push(self.evaluate_internal(arg, data)?);
                }
                return self.call_builtin_with_values(builtin_name, &evaluated_args, data, false);
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
                if self.is_builtin_function(var_name) {
                    return Ok(JValue::Bool(true)); // Built-in function exists
                }

                // Check if it's a stored lambda
                if self.context.lookup_lambda(var_name).is_some() {
                    return Ok(JValue::Bool(true)); // Lambda exists
                }

                // Check if the variable is defined
                if let Some(val) = self.context.lookup(var_name) {
                    // A variable bound to the undefined marker doesn't "exist"
                    if val.is_undefined() {
                        return Ok(JValue::Bool(false));
                    }
                    return Ok(JValue::Bool(true)); // Variable is defined (even if null)
                } else {
                    return Ok(JValue::Bool(false)); // Variable is undefined
                }
            }

            // For other expressions, evaluate and check whether anything is
            // there. An explicit null exists -- only a missing value does not,
            // which is what the AstNode::Null branch above already assumed for
            // the literal form.
            let value = self.evaluate_internal(arg, data)?;
            return Ok(JValue::Bool(!value.is_undefined()));
        }

        // Check if any arguments are undefined variables or undefined paths
        // Functions like $not() should return undefined when given undefined values
        for arg in args {
            // Check for undefined variable (e.g., $undefined_var)
            if let AstNode::Variable(var_name) = arg {
                // Skip built-in function names - they're function references, not undefined variables
                if !var_name.is_empty()
                    && !self.is_builtin_function(var_name)
                    && self.context.lookup(var_name).is_none()
                {
                    // Undefined variable - for functions that should propagate undefined
                    if propagates_undefined(name) {
                        return Ok(JValue::Null); // Return undefined
                    }
                }
            }
            // Check for simple field name (e.g., blah) that evaluates to undefined
            if let AstNode::Name(field_name) = arg {
                let field_exists = matches!(data, JValue::Object(obj) if obj.contains_key(field_name))
                    || {
                        #[cfg(feature = "python")]
                        {
                            matches!(data, JValue::LazyPyDict(l) if l.contains_field(field_name))
                        }
                        #[cfg(not(feature = "python"))]
                        {
                            false
                        }
                    };
                if !field_exists && propagates_undefined(name) {
                    return Ok(JValue::Null);
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
                if let Ok(JValue::Null) = self.evaluate_internal(arg, data) {
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
                                JValue::Object(obj) => {
                                    if !obj.contains_key(field) {
                                        // Field doesn't exist - return undefined
                                        if propagates_undefined(name) {
                                            return Ok(JValue::Null);
                                        }
                                    }
                                    // Field exists with value null - continue to throw T0410
                                }
                                // Trying to access field on null data - return undefined
                                JValue::Null if propagates_undefined(name) => {
                                    return Ok(JValue::Null);
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
                                    JValue::Object(obj) => {
                                        if let Some(val) = obj.get(field_name) {
                                            current = val;
                                        } else {
                                            // Field doesn't exist
                                            failed_due_to_missing_field = true;
                                            break;
                                        }
                                    }
                                    JValue::Array(_) => {
                                        // Array access - evaluate normally
                                        break;
                                    }
                                    JValue::Null => {
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

                        if failed_due_to_missing_field && propagates_undefined(name) {
                            return Ok(JValue::Null);
                        }
                    }
                }
            }
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
            return crate::builtins::dispatch_pure(
                name,
                &evaluated_args,
                data,
                &self.options,
                false,
            );
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

                // Check if replacement (3rd arg) is a function/lambda
                let replacement_is_lambda = matches!(
                    evaluated_args[2],
                    JValue::Lambda { .. } | JValue::Builtin { .. }
                );

                if replacement_is_lambda {
                    // Lambda replacement mode
                    return self.replace_with_lambda(
                        &evaluated_args[0],
                        &evaluated_args[1],
                        &evaluated_args[2],
                        if evaluated_args.len() == 4 {
                            Some(&evaluated_args[3])
                        } else {
                            None
                        },
                        data,
                    );
                }

                // String replacement mode
                match (&evaluated_args[0], &evaluated_args[2]) {
                    (JValue::String(s), JValue::String(replacement)) => {
                        let limit = if evaluated_args.len() == 4 {
                            match &evaluated_args[3] {
                                JValue::Number(n) => {
                                    let lim_f64 = *n;
                                    if lim_f64 < 0.0 {
                                        return Err(EvaluatorError::EvaluationError(format!(
                                            "D3011: Limit must be non-negative, got {}",
                                            lim_f64
                                        )));
                                    }
                                    Some(lim_f64 as usize)
                                }
                                _ => {
                                    return Err(EvaluatorError::TypeError(
                                        "replace() limit must be a number".to_string(),
                                    ))
                                }
                            }
                        } else {
                            None
                        };
                        Ok(functions::string::replace(
                            s,
                            &evaluated_args[1],
                            replacement,
                            limit,
                        )?)
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
                if evaluated_args[0].is_null() {
                    return Ok(JValue::Null);
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

                // Get optional limit
                let limit = if evaluated_args.len() == 3 {
                    match &evaluated_args[2] {
                        JValue::Number(n) => Some(*n as usize),
                        JValue::Null => None,
                        _ => {
                            return Err(EvaluatorError::TypeError(
                                "match() limit must be a number".to_string(),
                            ))
                        }
                    }
                } else {
                    None
                };

                // Check if second argument is a custom matcher function (lambda)
                let pattern_value = evaluated_args.get(1);
                let is_custom_matcher = pattern_value.is_some_and(|val| {
                    matches!(val, JValue::Lambda { .. } | JValue::Builtin { .. })
                });

                if is_custom_matcher {
                    // Custom matcher function support
                    // Call the matcher with the string, get match objects with {match, start, end, groups, next}
                    return self.match_with_custom_matcher(&s, &args[1], limit, data);
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
                let is_global = flags.contains('g');
                let re = crate::functions::string::build_regex(&pattern, &flags)
                    .map_err(|e| EvaluatorError::EvaluationError(e.to_string()))?;

                let mut results = Vec::new();
                let mut count = 0;

                for caps in re.captures_iter(&s) {
                    if let Some(lim) = limit {
                        if count >= lim {
                            break;
                        }
                    }

                    let full_match = caps.get(0).unwrap();
                    let mut match_obj = IndexMap::new();
                    match_obj.insert(
                        "match".to_string(),
                        JValue::string(full_match.as_str().to_string()),
                    );
                    match_obj.insert(
                        "index".to_string(),
                        JValue::Number(full_match.start() as f64),
                    );

                    // Collect capture groups
                    let mut groups: Vec<JValue> = Vec::new();
                    for i in 1..caps.len() {
                        if let Some(group) = caps.get(i) {
                            groups.push(JValue::string(group.as_str().to_string()));
                        } else {
                            groups.push(JValue::Null);
                        }
                    }
                    if !groups.is_empty() {
                        match_obj.insert("groups".to_string(), JValue::array(groups));
                    }

                    results.push(JValue::object(match_obj));
                    count += 1;

                    // If not global, only return first match
                    if !is_global {
                        break;
                    }
                }

                if results.is_empty() {
                    Ok(JValue::Null)
                } else if results.len() == 1 && !is_global {
                    // Single match (non-global) returns the match object directly
                    Ok(results.into_iter().next().unwrap())
                } else {
                    Ok(JValue::array(results))
                }
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
                        Some(JValue::Lambda { .. } | JValue::Builtin { .. })
                        | Some(JValue::Undefined)
                        | None => {}
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
                                if let Some(compiled) =
                                    try_compile_expr_with_allowed_vars(body, &var_refs)
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
                                if let Some(stored) = self.context.lookup_lambda(var_name) {
                                    if let Some(ref ce) = stored.compiled_body.clone() {
                                        let param_name = stored.params[0].clone();
                                        let captured_data = stored.captured_data.clone();
                                        let captured_env_clone = stored.captured_env.clone();
                                        let ce_clone = ce.clone();
                                        if !captured_env_clone.values().any(|v| {
                                            matches!(
                                                v,
                                                JValue::Lambda { .. } | JValue::Builtin { .. }
                                            )
                                        }) {
                                            let call_data = captured_data.as_ref().unwrap_or(data);
                                            let mut result = Vec::with_capacity(arr.len());
                                            let mut vars: HashMap<&str, &JValue> =
                                                captured_env_clone
                                                    .iter()
                                                    .map(|(k, v)| (k.as_str(), v))
                                                    .collect();
                                            for item in arr.iter() {
                                                vars.insert(param_name.as_str(), item);
                                                let mapped = eval_compiled(
                                                    &ce_clone,
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
                        single_holder = [array];
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
                        if let Some(compiled) = try_compile_expr_with_allowed_vars(body, &var_refs)
                        {
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
                        if let Some(stored) = self.context.lookup_lambda(var_name) {
                            if let Some(ref ce) = stored.compiled_body.clone() {
                                let param_name = stored.params[0].clone();
                                let captured_data = stored.captured_data.clone();
                                let captured_env_clone = stored.captured_env.clone();
                                let ce_clone = ce.clone();
                                if !captured_env_clone.values().any(|v| {
                                    matches!(v, JValue::Lambda { .. } | JValue::Builtin { .. })
                                }) {
                                    let call_data = captured_data.as_ref().unwrap_or(data);
                                    let mut result = Vec::with_capacity(items.len() / 2);
                                    let mut vars: HashMap<&str, &JValue> = captured_env_clone
                                        .iter()
                                        .map(|(k, v)| (k.as_str(), v))
                                        .collect();
                                    for item in items.iter() {
                                        vars.insert(param_name.as_str(), item);
                                        let pred_result = eval_compiled(
                                            &ce_clone,
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
                }

                // Only create the array value if callback uses 3 parameters
                let arr_value = if param_count >= 3 {
                    Some(JValue::array(items.to_vec()))
                } else {
                    None
                };

                let mut result = Vec::with_capacity(items.len() / 2);

                for (index, item) in items.iter().enumerate() {
                    // hofFuncArgs shaping (see hof_array_call_args)
                    let call_args =
                        hof_array_call_args(item, index, arr_value.as_ref(), param_count);

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
                // Use references to avoid upfront cloning of all elements
                let single_holder;
                let items: &[JValue] = match &array {
                    JValue::Array(arr) => arr.as_slice(),
                    JValue::Null => return Ok(JValue::Null),
                    _ => {
                        single_holder = [array];
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

                // Detect how many parameters the callback expects
                let param_count = self.get_callback_param_count(&args[1]);

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
                        if let Some(compiled) = try_compile_expr_with_allowed_vars(body, &var_refs)
                        {
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
                        if let Some(stored) = self.context.lookup_lambda(var_name) {
                            if stored.params.len() == 2 {
                                if let Some(ref ce) = stored.compiled_body.clone() {
                                    let acc_param = stored.params[0].clone();
                                    let item_param = stored.params[1].clone();
                                    let captured_data = stored.captured_data.clone();
                                    let captured_env_clone = stored.captured_env.clone();
                                    let ce_clone = ce.clone();
                                    if !captured_env_clone.values().any(|v| {
                                        matches!(v, JValue::Lambda { .. } | JValue::Builtin { .. })
                                    }) {
                                        let call_data = captured_data.as_ref().unwrap_or(data);
                                        for item in items[start_idx..].iter() {
                                            let mut vars: HashMap<&str, &JValue> =
                                                captured_env_clone
                                                    .iter()
                                                    .map(|(k, v)| (k.as_str(), v))
                                                    .collect();
                                            vars.insert(acc_param.as_str(), &accumulator);
                                            vars.insert(item_param.as_str(), item);
                                            // Evaluate and drop vars before assigning accumulator
                                            // to satisfy borrow checker (vars borrows accumulator)
                                            let new_acc = eval_compiled(
                                                &ce_clone,
                                                call_data,
                                                Some(&vars),
                                                &self.options,
                                                self.start_time,
                                            )?;
                                            drop(vars);
                                            accumulator = new_acc;
                                        }
                                        return Ok(accumulator);
                                    }
                                }
                            }
                        }
                    }
                }

                // Only create the array value if callback uses 4 parameters
                let arr_value = if param_count >= 4 {
                    Some(JValue::array(items.to_vec()))
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

                // Convert to array (wrap single values)
                let arr = match array {
                    JValue::Array(arr) => arr.to_vec(),
                    JValue::Null => return Ok(JValue::Null),
                    other => vec![other],
                };

                if args.len() == 1 {
                    // No predicate - array must have exactly 1 element
                    match arr.len() {
                        0 => Err(EvaluatorError::EvaluationError(
                            "D3139: The $single() function expected exactly 1 matching result.  Instead it matched 0.".to_string(),
                        )),
                        1 => Ok(arr.into_iter().next().unwrap()),
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
                        Some(JValue::array(arr.clone()))
                    } else {
                        None
                    };
                    let mut matches = Vec::new();
                    for (index, item) in arr.into_iter().enumerate() {
                        // hofFuncArgs shaping (see hof_array_call_args)
                        let call_args =
                            hof_array_call_args(&item, index, arr_value.as_ref(), param_count);
                        let predicate_result = self.apply_function(&args[1], &call_args, data)?;
                        if self.is_truthy(&predicate_result) {
                            matches.push(item);
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

    /// Apply a function (lambda or expression) to values
    ///
    /// This handles both:
    /// 1. Lambda nodes: function($x) { $x * 2 } - binds parameters and evaluates body
    /// 2. Simple expressions: price * 2 - evaluates with values as context
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
                self.invoke_lambda(params, body, signature.as_ref(), values, data, *thunk)
            }
            AstNode::Function {
                name,
                args,
                is_builtin,
            } => {
                // Function call - check if it has placeholders (partial application)
                let has_placeholder = args.iter().any(|arg| matches!(arg, AstNode::Placeholder));

                if has_placeholder {
                    // This is a partial application - evaluate it to get the lambda value
                    let partial_lambda =
                        self.create_partial_application(name, args, *is_builtin, data)?;

                    // Now invoke the partial lambda with the provided values
                    if let Some(stored) = self.lookup_lambda_from_value(&partial_lambda) {
                        return self.invoke_stored_lambda(&stored, values, data);
                    }
                    Err(EvaluatorError::EvaluationError(
                        "Failed to apply partial application".to_string(),
                    ))
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
                    // Check if this variable holds a lambda value
                    // This handles lambdas passed as bound arguments in partial applications
                    if let Some(stored) = self.lookup_lambda_from_value(&value) {
                        return self.invoke_stored_lambda(&stored, values, data);
                    }
                    // `$f := $uppercase` binds a `JValue::Builtin`, which is
                    // not a lambda and so fell through to "evaluate as a plain
                    // variable" below -- the callback yielded the builtin value
                    // itself rather than calling it. It takes the same
                    // by-reference route a literal `$uppercase` callback takes
                    // (#126 group 5).
                    if let JValue::Builtin { name } = &value {
                        let name = name.to_string();
                        return self.call_builtin_with_values(&name, values, data, true);
                    }
                    // Regular variable value - evaluate with first value as context
                    if values.is_empty() {
                        self.evaluate_internal(func_node, data)
                    } else {
                        self.evaluate_internal(func_node, &values[0])
                    }
                } else if self.is_builtin_function(var_name) {
                    // This is a built-in function reference (e.g., $string, $number).
                    // Every caller of `apply_function` -- $map/$filter/$reduce/$sift/
                    // $each's per-element loop, $single's predicate, $sort's
                    // comparator, $match's matcher -- passes the callback as an
                    // ARGUMENT to another function, the shape jsonata-js wraps in a
                    // closure. `by_reference: true` (see call_builtin_with_values).
                    self.call_builtin_with_values(var_name, values, data, true)
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
        _original_data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        // Get the input value from $ binding
        let input = self
            .context
            .lookup("$")
            .ok_or_else(|| {
                EvaluatorError::EvaluationError("Transform requires $ binding".to_string())
            })?
            .clone();

        // Evaluate location expression on the input to get objects to transform
        let located_objects = self.evaluate_internal(location, &input)?;

        // Collect target objects into a vector for comparison
        let targets: Vec<JValue> = match located_objects {
            JValue::Array(arr) => arr.to_vec(),
            JValue::Object(_) => vec![located_objects],
            #[cfg(feature = "python")]
            JValue::LazyPyDict(_) => vec![located_objects],
            JValue::Null => Vec::new(),
            other => vec![other],
        };

        // Validate update parameter - must be an object constructor
        // We need to check this before evaluation in case of errors
        // For now, we'll validate after evaluation in the transform helper

        // Parse delete field names if provided
        let delete_fields: Vec<String> = if let Some(delete_node) = delete {
            let delete_val = self.evaluate_internal(delete_node, &input)?;
            match delete_val {
                JValue::Array(arr) => arr
                    .iter()
                    .filter_map(|v| match v {
                        JValue::String(s) => Some(s.to_string()),
                        _ => None,
                    })
                    .collect(),
                JValue::String(s) => vec![s.to_string()],
                JValue::Null | JValue::Undefined => Vec::new(), // Undefined variable is treated as no deletion
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
            value: &JValue,
            targets: &[JValue],
            update: &AstNode,
            delete_fields: &[String],
        ) -> Result<JValue, EvaluatorError> {
            // Check if this value is one of the targets to transform
            // Use JValue's PartialEq for semantic equality comparison
            if targets.iter().any(|t| t == value) {
                // Transform this object
                let value = &normalize_lazy(value)?;
                if let JValue::Object(map_rc) = value.clone() {
                    let mut map = (*map_rc).clone();
                    let update_val = evaluator.evaluate_internal(update, value)?;
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
                    for field in delete_fields {
                        map.shift_remove(field);
                    }
                    return Ok(JValue::object(map));
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
                            apply_transform_deep(evaluator, v, targets, update, delete_fields)?,
                        );
                    }
                    Ok(JValue::object(new_map))
                }
                #[cfg(feature = "python")]
                JValue::LazyPyDict(lazy) => {
                    let obj = JValue::Object(lazy.to_object().map_err(EvaluatorError::from)?);
                    apply_transform_deep(evaluator, &obj, targets, update, delete_fields)
                }
                JValue::Array(arr) => {
                    let mut new_arr = Vec::new();
                    for item in arr.iter() {
                        new_arr.push(apply_transform_deep(
                            evaluator,
                            item,
                            targets,
                            update,
                            delete_fields,
                        )?);
                    }
                    Ok(JValue::array(new_arr))
                }
                _ => Ok(value.clone()),
            }
        }

        // Apply transformation recursively starting from input
        apply_transform_deep(self, &input, &targets, update, &delete_fields)
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
        self.invoke_lambda_with_env(params, body, signature, values, data, None, None, thunk)
    }

    /// Invoke a lambda with optional captured environment (for closures)
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
    ) -> Result<JValue, EvaluatorError> {
        // If this is a thunk (has tail calls), use TCO trampoline
        if thunk {
            let stored = StoredLambda {
                params: params.to_vec(),
                body: body.clone(),
                compiled_body: None, // Thunks use TCO, not the compiled fast path
                signature: signature.cloned(),
                captured_env: captured_env.cloned().unwrap_or_default(),
                captured_data: captured_data.cloned(),
                thunk,
            };
            return self.invoke_lambda_with_tco(&stored, values, data);
        }

        // Validate signature if present, and get coerced arguments
        // Push a new scope for this lambda invocation
        self.context.push_scope();

        // First apply captured environment (for closures)
        if let Some(env) = captured_env {
            for (name, value) in env {
                self.context.bind(name.clone(), value.clone());
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
                        if let Some(JValue::Array(positions)) = env.get("__placeholder_positions") {
                            positions
                                .iter()
                                .filter_map(|v| v.as_f64().map(|n| n as usize))
                                .collect()
                        } else {
                            vec![]
                        }
                    } else {
                        vec![]
                    };

                    // Reconstruct the full argument list
                    let mut full_args: Vec<JValue> = vec![JValue::Null; total_args];

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
                            let value = values.get(i).cloned().unwrap_or(JValue::Null);
                            full_args[pos] = value;
                        }
                    }

                    // Pop lambda scope, then push a new scope for temp args
                    self.context.pop_scope();
                    self.context.push_scope();

                    // Build AST nodes for the function call arguments
                    let mut temp_args: Vec<AstNode> = Vec::new();
                    for (i, value) in full_args.iter().enumerate() {
                        let temp_name = format!("__temp_arg_{}", i);
                        self.context.bind(temp_name.clone(), value.clone());
                        temp_args.push(AstNode::Variable(temp_name));
                    }

                    // Call the original function
                    let result =
                        self.evaluate_function_call(func_name, &temp_args, is_builtin, data);

                    // Pop temp scope
                    self.context.pop_scope();

                    return result;
                }
            }
        }

        // Evaluate lambda body (normal case)
        // Use captured_data for lexical scoping if available, otherwise use call-site data
        let body_data = captured_data.unwrap_or(data);
        let result = self.evaluate_internal(body, body_data)?;

        // Pop lambda scope, preserving any lambdas referenced by the return value
        // Fast path: scalar results can never contain lambda references
        let is_scalar = matches!(
            &result,
            JValue::Number(_)
                | JValue::Bool(_)
                | JValue::String(_)
                | JValue::Null
                | JValue::Undefined
        );
        if is_scalar {
            self.context.pop_scope();
        } else {
            let lambdas_to_keep = self.extract_lambda_ids(&result);
            self.context.pop_scope_preserving_lambdas(&lambdas_to_keep);
        }

        Ok(result)
    }

    /// Invoke a lambda with tail call optimization using a trampoline
    /// This method uses an iterative loop to handle tail-recursive calls without
    /// growing the stack, enabling deep recursion for tail-recursive functions.
    fn invoke_lambda_with_tco(
        &mut self,
        stored_lambda: &StoredLambda,
        initial_args: &[JValue],
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        let mut current_lambda = stored_lambda.clone();
        let mut current_args = initial_args.to_vec();
        let mut current_data = data.clone();

        // Maximum number of tail call iterations to prevent infinite loops
        // This is much higher than non-TCO depth limit since TCO doesn't grow the stack
        const MAX_TCO_ITERATIONS: usize = 100_000;
        let mut iterations = 0;

        // Push a persistent scope for the TCO trampoline loop.
        // This scope persists across all iterations so that lambdas defined
        // in one iteration (like recursive $iter) remain available in subsequent ones.
        self.context.push_scope();

        // Trampoline loop - keeps evaluating until we get a final value
        let result = loop {
            iterations += 1;
            // The hardcoded iteration cap is a backstop for when no timeout is
            // configured; it must not preempt a configured timeout (which is the
            // more specific, user-controlled guardrail). Without this gate, an
            // infinite tail-recursive loop with a cheap per-iteration body hits
            // this cap in single-digit-to-tens of milliseconds and reports the
            // misleading "U1001: Stack overflow" (TCO does not grow the stack;
            // there is no depth-500 stack here) instead of D1012, for *any*
            // realistic `timeout_ms` (100ms, 1s, the docs' own 5000ms default) -
            // defeating the purpose of the timeout guardrail for exactly the
            // scenario it exists to catch (see jsonata-js's own `$inf := function
            // (){$inf()}; $inf()` guardrails-documentation example).
            if self.options.timeout_ms.is_none() && iterations > MAX_TCO_ITERATIONS {
                self.context.pop_scope();
                return Err(EvaluatorError::EvaluationError(
                    "U1001: Stack overflow - maximum recursion depth (500) exceeded".to_string(),
                ));
            }
            if let Err(e) = check_loop_timeout(&self.options, self.start_time) {
                self.context.pop_scope();
                return Err(e);
            }

            // Evaluate the lambda body within the persistent scope
            let result =
                self.invoke_lambda_body_for_tco(&current_lambda, &current_args, &current_data)?;

            match result {
                LambdaResult::JValue(v) => break v,
                LambdaResult::TailCall { lambda, args, data } => {
                    // Continue with the tail call - no stack growth
                    current_lambda = *lambda;
                    current_args = args;
                    current_data = data;
                }
            }
        };

        // Pop the persistent TCO scope, preserving lambdas referenced by the result
        let lambdas_to_keep = self.extract_lambda_ids(&result);
        self.context.pop_scope_preserving_lambdas(&lambdas_to_keep);

        Ok(result)
    }

    /// Evaluate a lambda body, detecting tail calls for TCO
    /// Returns either a final value or a tail call continuation.
    /// NOTE: Does not push/pop its own scope - the caller (invoke_lambda_with_tco)
    /// manages the persistent scope for the trampoline loop.
    fn invoke_lambda_body_for_tco(
        &mut self,
        lambda: &StoredLambda,
        values: &[JValue],
        data: &JValue,
    ) -> Result<LambdaResult, EvaluatorError> {
        // Validate signature if present
        let coerced_values = if let Some(sig_str) = &lambda.signature {
            coerce_lambda_args(sig_str, values, data)?
        } else {
            values.to_vec()
        };

        // Bind directly into the persistent scope (managed by invoke_lambda_with_tco)
        // Apply captured environment
        for (name, value) in &lambda.captured_env {
            self.context.bind(name.clone(), value.clone());
        }

        // Bind parameters
        for (i, param) in lambda.params.iter().enumerate() {
            let value = coerced_values.get(i).cloned().unwrap_or(JValue::Null);
            self.context.bind(param.clone(), value);
        }

        // Evaluate the body with tail call detection
        let body_data = lambda.captured_data.as_ref().unwrap_or(data);
        self.evaluate_for_tco(&lambda.body, body_data)
    }

    /// Evaluate an expression for TCO, detecting tail calls
    /// Returns LambdaResult::TailCall if the expression is a function call to a user lambda
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
                    Ok(LambdaResult::JValue(JValue::Null))
                }
            }

            // Block: evaluate all but last normally, last for TCO
            AstNode::Block(exprs) => {
                if exprs.is_empty() {
                    return Ok(LambdaResult::JValue(JValue::Null));
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
                    let compiled_body = if !thunk {
                        let var_refs: Vec<&str> = params.iter().map(|s| s.as_str()).collect();
                        try_compile_expr_with_allowed_vars(body, &var_refs)
                    } else {
                        None
                    };
                    let stored_lambda = StoredLambda {
                        params: params.clone(),
                        body: (**body).clone(),
                        compiled_body,
                        signature: signature.clone(),
                        captured_env,
                        captured_data: Some(data.clone()),
                        thunk: *thunk,
                    };
                    self.context.bind_lambda(var_name, stored_lambda);
                    let lambda_repr =
                        JValue::lambda("anon", params.clone(), None::<String>, None::<String>);
                    return Ok(LambdaResult::JValue(lambda_repr));
                }

                // Evaluate the RHS
                let value = self.evaluate_internal(rhs, data)?;
                self.context.bind(var_name, value.clone());
                Ok(LambdaResult::JValue(value))
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
                            lambda: Box::new(stored_lambda),
                            args: evaluated_args,
                            data: data.clone(),
                        });
                    }
                }
                // Not a thunk lambda - evaluate normally
                let result = self.evaluate_internal(node, data)?;
                Ok(LambdaResult::JValue(result))
            }

            // Call node (calling a lambda value)
            AstNode::Call { procedure, args } => {
                // Evaluate the procedure to get the callable
                let callable = self.evaluate_internal(procedure, data)?;

                // Check if it's a lambda with TCO
                if let JValue::Lambda { lambda_id, .. } = &callable {
                    if let Some(stored_lambda) = self.context.lookup_lambda(lambda_id).cloned() {
                        if stored_lambda.thunk {
                            let mut evaluated_args = Vec::with_capacity(args.len());
                            for arg in args {
                                evaluated_args.push(self.evaluate_internal(arg, data)?);
                            }
                            return Ok(LambdaResult::TailCall {
                                lambda: Box::new(stored_lambda),
                                args: evaluated_args,
                                data: data.clone(),
                            });
                        }
                    }
                }
                // Not a thunk - evaluate normally
                let result = self.evaluate_internal(node, data)?;
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
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        let mut results = Vec::new();
        let mut count = 0;

        // Call the matcher function with the string
        let str_val = JValue::string(str_value.to_string());
        let mut current_match = self.apply_function(matcher_node, &[str_val], data)?;

        // Iterate through matches following the 'next' chain
        while !current_match.is_undefined() && !current_match.is_null() {
            // Check limit
            if let Some(lim) = limit {
                if count >= lim {
                    break;
                }
            }

            // Extract match information from the result object
            if let JValue::Object(ref match_obj) = current_match {
                // Validate that this is a proper match object
                let has_match = match_obj.contains_key("match");
                let has_start = match_obj.contains_key("start");
                let has_end = match_obj.contains_key("end");
                let has_groups = match_obj.contains_key("groups");
                let has_next = match_obj.contains_key("next");

                if !has_match && !has_start && !has_end && !has_groups && !has_next {
                    // Invalid matcher result - T1010 error
                    return Err(EvaluatorError::EvaluationError(
                        "T1010: The matcher function did not return the correct object structure"
                            .to_string(),
                    ));
                }

                // Build the result match object (match, index, groups)
                let mut result_obj = IndexMap::new();

                if let Some(match_val) = match_obj.get("match") {
                    result_obj.insert("match".to_string(), match_val.clone());
                }

                if let Some(start_val) = match_obj.get("start") {
                    result_obj.insert("index".to_string(), start_val.clone());
                }

                if let Some(groups_val) = match_obj.get("groups") {
                    result_obj.insert("groups".to_string(), groups_val.clone());
                }

                results.push(JValue::object(result_obj));
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
            Ok(JValue::Undefined)
        } else {
            Ok(JValue::array(results))
        }
    }

    /// Replace with lambda/function callback
    ///
    /// Implements lambda replacement for $replace(str, pattern, function, limit?)
    /// The function receives a match object with: match, start, end, groups
    fn replace_with_lambda(
        &mut self,
        str_value: &JValue,
        pattern_value: &JValue,
        lambda_value: &JValue,
        limit_value: Option<&JValue>,
        data: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        // Extract string
        let s = match str_value {
            JValue::String(s) => &**s,
            _ => {
                return Err(EvaluatorError::TypeError(
                    "replace() requires string arguments".to_string(),
                ))
            }
        };

        // Extract regex pattern
        let (pattern, flags) =
            crate::functions::string::extract_regex(pattern_value).ok_or_else(|| {
                EvaluatorError::TypeError(
                    "replace() pattern must be a regex when using lambda replacement".to_string(),
                )
            })?;

        // Build regex
        let re = crate::functions::string::build_regex(&pattern, &flags)?;

        // Parse limit
        let limit = if let Some(lim_val) = limit_value {
            match lim_val {
                JValue::Number(n) => {
                    let lim_f64 = *n;
                    if lim_f64 < 0.0 {
                        return Err(EvaluatorError::EvaluationError(format!(
                            "D3011: Limit must be non-negative, got {}",
                            lim_f64
                        )));
                    }
                    Some(lim_f64 as usize)
                }
                _ => {
                    return Err(EvaluatorError::TypeError(
                        "replace() limit must be a number".to_string(),
                    ))
                }
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
            let groups: Vec<JValue> = (1..cap.len())
                .map(|i| {
                    cap.get(i)
                        .map(|m| JValue::string(m.as_str().to_string()))
                        .unwrap_or(JValue::Null)
                })
                .collect();

            let mut match_map = IndexMap::new();
            match_map.insert("match".to_string(), JValue::string(match_str));
            match_map.insert("start".to_string(), JValue::Number(match_start as f64));
            match_map.insert("end".to_string(), JValue::Number(match_end as f64));
            match_map.insert("groups".to_string(), JValue::array(groups));
            let match_obj = JValue::object(match_map);

            // Invoke lambda with match object
            let stored_lambda = self.lookup_lambda_from_value(lambda_value).ok_or_else(|| {
                EvaluatorError::TypeError("Replacement must be a lambda function".to_string())
            })?;
            let lambda_result = self.invoke_stored_lambda(&stored_lambda, &[match_obj], data)?;
            let replacement_str = match lambda_result {
                JValue::String(s) => s,
                _ => {
                    return Err(EvaluatorError::TypeError(format!(
                        "D3012: Replacement function must return a string, got {:?}",
                        lambda_result
                    )))
                }
            };

            // Add replacement
            result.push_str(&replacement_str);

            last_end = match_end;
            count += 1;
        }

        // Add remaining text after last match
        result.push_str(&s[last_end..]);

        Ok(JValue::string(result))
    }

    /// Capture the current environment bindings for closure support
    fn capture_current_environment(&self) -> HashMap<String, JValue> {
        self.context.all_bindings()
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

    fn collect_free_vars_walk(node: &AstNode, bound: &HashSet<&str>, free: &mut HashSet<String>) {
        match node {
            AstNode::KeepArray => {}
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
            AstNode::Predicate(expr) | AstNode::FunctionApplication(expr) => {
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

    /// Call a built-in function directly with pre-evaluated Values.
    ///
    /// Reached from two genuinely different call shapes, and `by_reference`
    /// tells them apart:
    ///
    /// - A builtin-valued variable called directly, `$f := $uppercase;
    ///   $f("x")`. jsonata-js evaluates `$f` to the builtin's own `proc`
    ///   object and applies it exactly as it would `$uppercase("x")` --
    ///   no wrapping happens. `by_reference: false`.
    /// - A builtin passed as an ARGUMENT to another function -- a HOF
    ///   callback (`$map(arr, $uppercase)`), a `$sort` comparator, a
    ///   `$match` matcher. jsonata-js's `evaluateFunction` wraps any
    ///   function-valued *argument* in a closure (`jsonata.js:1461-1471`)
    ///   before the callee ever sees it, and that closure calls `apply(arg,
    ///   params, null, environment)` -- note the literal `null` for
    ///   `input`. `by_reference: true`.
    ///
    /// Getting this wrong previously showed up as `$f := $keys; $f({"a":
    /// 1})` wrongly returning `["a"]` instead of `"a"` -- the by-reference
    /// (HOF-argument) collapse-skipping described on `dispatch_pure`'s
    /// `by_reference` parameter was leaking into the direct-call shape,
    /// which must collapse exactly like a literal `$keys({"a":1})` does.
    fn call_builtin_with_values(
        &mut self,
        name: &str,
        values: &[JValue],
        context: &JValue,
        by_reference: bool,
    ) -> Result<JValue, EvaluatorError> {
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
            return crate::builtins::dispatch_pure(
                name,
                values,
                context,
                &self.options,
                by_reference,
            );
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
        // evaluation, so they cannot run from already-evaluated values. That
        // is not a gap: each needs a *function* argument, and a higher-order
        // function hands its callback a value and an index, so jsonata-js
        // rejects these on the signature too. It raises T0410 -- which is
        // what this used to get wrong, reporting an uncoded error instead.
        Err(EvaluatorError::TypeError(format!(
            "T0410: Argument 2 of function {} does not match function signature",
            name
        )))
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
        let eval_context = if args.len() == 2 { &args[1] } else { data };

        // Evaluate the parsed expression
        match self.evaluate_internal(&parsed_ast, eval_context) {
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
                    descendants.extend(self.collect_descendants(val)?);
                }
            }
            #[cfg(feature = "python")]
            JValue::LazyPyDict(lazy) => {
                // Include the current (lazy) object, mirroring the Object arm
                descendants.push(value.clone());

                let obj = lazy.to_object().map_err(EvaluatorError::from)?;
                for val in obj.values() {
                    descendants.extend(self.collect_descendants(val)?);
                }
            }
            JValue::Array(arr) => {
                // DO NOT include the array itself - only recurse into elements
                // This matches JavaScript behavior: arrays are traversed but not collected
                for val in arr.iter() {
                    // Recursively collect descendants
                    descendants.extend(self.collect_descendants(val)?);
                }
            }
            _ => {
                // For primitives (string, number, boolean), just include the value itself
                descendants.push(value.clone());
            }
        }

        Ok(descendants)
    }

    /// Evaluate a predicate (array filter or index)
    fn evaluate_predicate(
        &mut self,
        current: &JValue,
        predicate: &AstNode,
    ) -> Result<JValue, EvaluatorError> {
        match current {
            JValue::Array(_arr) => {
                // Standalone predicates: jsonata-js evaluates the predicate once
                // per element and decides from the result. A numeric result is an
                // index selector compared against that element's own position; any
                // other result is a truthiness test. This one loop subsumes the
                // multi-index selector form too -- a constant `[0, 1]` predicate
                // evaluates to the same array for every element and matches
                // positions 0 and 1 -- so there is no separate whole-array path.

                // Literal numeric index keeps its direct route: array_index returns
                // the selected element itself rather than a one-element sequence,
                // which the caller's singleton-unwrap rule depends on.
                if let AstNode::Number(n) = predicate {
                    return self.array_index(current, &JValue::Number(*n));
                }

                let len = _arr.len();
                let compiled = try_compile_expr(predicate);
                let shape = compiled
                    .as_ref()
                    .and_then(|_| _arr.first().and_then(build_shape_cache));

                let mut filtered = Vec::new();
                // Whether every element's predicate result was an index selector.
                // Index selection yields the element itself, not a one-element
                // sequence: `[1, 2, [3, 4]][-1][-1]` needs the first `[-1]` to
                // hand `[3, 4]` to the second, and this path unwraps singletons
                // only once, at the end of the whole path.
                let mut all_index_selectors = true;
                for (index, item) in _arr.iter().enumerate() {
                    let result = match (&compiled, &shape) {
                        (Some(c), Some(s)) => {
                            eval_compiled_shaped(c, item, None, s, &self.options, self.start_time)?
                        }
                        (Some(c), None) => {
                            eval_compiled(c, item, None, &self.options, self.start_time)?
                        }
                        (None, _) => self.evaluate_internal(predicate, item)?,
                    };
                    let repeats = match predicate_index_match(&result, index, len) {
                        Some(n) => n,
                        None => {
                            all_index_selectors = false;
                            usize::from(self.is_truthy(&result))
                        }
                    };
                    for _ in 0..repeats {
                        filtered.push(item.clone());
                    }
                }

                if all_index_selectors && filtered.len() == 1 {
                    return Ok(filtered.remove(0));
                }
                Ok(JValue::array(filtered))
            }
            JValue::Object(obj) => {
                // A non-array is a singleton sequence: index 0, length 1. The
                // same rule as for arrays applies, so `o[p]` keeps the object
                // only when `p` is 0 (or a non-numeric truthy value), and
                // `o[-1]` wraps to index 0.
                //
                // A string predicate is NOT computed property access -- `o["a"]`
                // keeps the object because a non-empty string is truthy, which
                // is what jsonata-js does. `_ = obj` keeps the binding readable
                // for the debugger without implying a lookup happens here.
                let _ = obj;
                let pred_result = self.evaluate_internal(predicate, current)?;

                let repeats = match predicate_index_match(&pred_result, 0, 1) {
                    Some(n) => n,
                    None => usize::from(self.is_truthy(&pred_result)),
                };
                if repeats > 1 {
                    Ok(JValue::array(vec![current.clone(); repeats]))
                } else if repeats == 1 {
                    Ok(current.clone())
                } else {
                    Ok(JValue::Undefined)
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
                        return Ok(JValue::Undefined);
                    }
                }

                // Try to evaluate the predicate to see if it's a positional
                // selector. This used to test only for a single `JValue::Number`,
                // so an *array* of indices fell through to the truthiness branch
                // below: `num[[0]]` was undefined (an all-falsy container is
                // falsy) and `num[[1]]` was the value (a non-empty container is
                // truthy) -- both inverted. The compiled path was already
                // correct, so the two engines disagreed.
                //
                // Otherwise a filter: `value[true]` is the value,
                // `value[false]` is undefined, which is what makes
                // `$k[$v>2]` work.
                let pred_result = self.evaluate_internal(predicate, current)?;

                let repeats = match predicate_index_match(&pred_result, 0, 1) {
                    Some(n) => n,
                    None => usize::from(self.is_truthy(&pred_result)),
                };
                if repeats > 1 {
                    Ok(JValue::array(vec![current.clone(); repeats]))
                } else if repeats == 1 {
                    Ok(current.clone())
                } else {
                    // Undefined (not null) so $map can filter it out.
                    Ok(JValue::Undefined)
                }
            }
        }
    }

    /// Evaluate a sort term expression, distinguishing missing fields from explicit null
    /// Returns JValue::Undefined for missing fields, JValue::Null for explicit null
    fn evaluate_sort_term(
        &mut self,
        term_expr: &AstNode,
        element: &JValue,
    ) -> Result<JValue, EvaluatorError> {
        // For tuples (from index binding), extract the actual value from @ field
        let actual_element = if let JValue::Object(obj) = element {
            if obj.get("__tuple__") == Some(&JValue::Bool(true)) {
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
                        _ => return Ok(JValue::Undefined),
                    }
                }
            }
        }

        // For complex expressions, evaluate against the tuple's `@` value (the
        // real element), not the wrapper. The tuple's carried focus/index/ancestor
        // bindings are reachable via context (bound by evaluate_sort), so a term
        // like `$`, `%.Price`, or `$pos` still resolves correctly.
        let result = self.evaluate_internal(term_expr, &actual_element)?;

        // If the result is null from a complex expression, we can't easily tell if it's
        // "missing field" or "explicit null". For now, treat null results as undefined
        // to maintain compatibility with existing tests.
        // TODO: For full JS compatibility, would need deeper analysis of the expression
        if result.is_null() {
            return Ok(JValue::Undefined);
        }

        Ok(result)
    }

    /// Evaluate sort operator
    fn evaluate_sort(
        &mut self,
        data: &JValue,
        terms: &[(AstNode, bool)],
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

        // If empty array, return as-is
        if array.is_empty() {
            return Ok(JValue::Array(array));
        }

        // Evaluate sort keys for each element
        let mut indexed_array: Vec<(usize, Vec<JValue>)> = Vec::new();

        for (idx, element) in array.iter().enumerate() {
            let mut sort_keys = Vec::new();

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
                JValue::Object(obj) if obj.get("__tuple__") == Some(&JValue::Bool(true)) => {
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
                JValue::Object(obj) if obj.get("__tuple__") == Some(&JValue::Bool(true)) => {
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
                let sort_value = self.evaluate_sort_term(term_expr, element)?;

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

        // Validate that all sort keys are comparable (same type, or undefined)
        // Undefined values (missing fields) are allowed and sort to the end
        // Null values (explicit null in data) are NOT allowed (typeof null === 'object' in JS, triggers T2008)
        for term_idx in 0..terms.len() {
            let mut first_valid_type: Option<&str> = None;

            for (_idx, sort_keys) in &indexed_array {
                let sort_value = &sort_keys[term_idx];

                // Skip undefined markers (missing fields) - these are allowed and sort to end
                if sort_value.is_undefined() {
                    continue;
                }

                // Get the type name for this value
                // Note: explicit null is NOT allowed - typeof null === 'object' in JS
                let value_type = match sort_value {
                    JValue::Number(_) => "number",
                    JValue::String(_) => "string",
                    JValue::Bool(_) => "boolean",
                    JValue::Array(_) => "array",
                    JValue::Object(_) => "object", // This catches non-undefined objects
                    JValue::Null => "null",        // Explicit null from data
                    #[cfg(feature = "python")]
                    JValue::LazyPyDict(_) => "object",
                    _ => "unknown",
                };

                // Check that sort keys are only numbers or strings
                // Null, boolean, array, and object types are not valid for sorting
                if value_type != "number" && value_type != "string" {
                    return Err(EvaluatorError::TypeError("T2008: The expressions within an order-by clause must evaluate to numeric or string values".to_string()));
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
                    return if *ascending { cmp } else { cmp.reverse() };
                }
            }

            // If all keys are equal, maintain original order (stable sort)
            a.0.cmp(&b.0)
        });

        // Extract sorted elements
        let sorted: Vec<JValue> = indexed_array
            .iter()
            .map(|(idx, _)| array[*idx].clone())
            .collect();

        Ok(JValue::array(sorted))
    }

    /// Compare two values for sorting (JSONata semantics)
    fn compare_values(&self, left: &JValue, right: &JValue) -> Ordering {
        // Handle undefined markers first - they sort to the end
        let left_undef = left.is_undefined();
        let right_undef = right.is_undefined();

        if left_undef && right_undef {
            return Ordering::Equal;
        }
        if left_undef {
            return Ordering::Greater; // Undefined sorts last
        }
        if right_undef {
            return Ordering::Less;
        }

        match (left, right) {
            // Nulls also sort last (explicit null in data)
            (JValue::Null, JValue::Null) => Ordering::Equal,
            (JValue::Null, _) => Ordering::Greater,
            (_, JValue::Null) => Ordering::Less,

            // Numbers
            (JValue::Number(a), JValue::Number(b)) => {
                let a_f64 = *a;
                let b_f64 = *b;
                a_f64.partial_cmp(&b_f64).unwrap_or(Ordering::Equal)
            }

            // Strings
            (JValue::String(a), JValue::String(b)) => a.cmp(b),

            // Booleans
            (JValue::Bool(a), JValue::Bool(b)) => a.cmp(b),

            // Arrays (lexicographic comparison)
            (JValue::Array(a), JValue::Array(b)) => {
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
            (JValue::Bool(_), JValue::Number(_)) => Ordering::Less,
            (JValue::Bool(_), JValue::String(_)) => Ordering::Less,
            (JValue::Bool(_), JValue::Array(_)) => Ordering::Less,
            (JValue::Bool(_), JValue::Object(_)) => Ordering::Less,
            #[cfg(feature = "python")]
            (JValue::Bool(_), JValue::LazyPyDict(_)) => Ordering::Less,

            (JValue::Number(_), JValue::Bool(_)) => Ordering::Greater,
            (JValue::Number(_), JValue::String(_)) => Ordering::Less,
            (JValue::Number(_), JValue::Array(_)) => Ordering::Less,
            (JValue::Number(_), JValue::Object(_)) => Ordering::Less,
            #[cfg(feature = "python")]
            (JValue::Number(_), JValue::LazyPyDict(_)) => Ordering::Less,

            (JValue::String(_), JValue::Bool(_)) => Ordering::Greater,
            (JValue::String(_), JValue::Number(_)) => Ordering::Greater,
            (JValue::String(_), JValue::Array(_)) => Ordering::Less,
            (JValue::String(_), JValue::Object(_)) => Ordering::Less,
            #[cfg(feature = "python")]
            (JValue::String(_), JValue::LazyPyDict(_)) => Ordering::Less,

            (JValue::Array(_), JValue::Bool(_)) => Ordering::Greater,
            (JValue::Array(_), JValue::Number(_)) => Ordering::Greater,
            (JValue::Array(_), JValue::String(_)) => Ordering::Greater,
            (JValue::Array(_), JValue::Object(_)) => Ordering::Less,
            #[cfg(feature = "python")]
            (JValue::Array(_), JValue::LazyPyDict(_)) => Ordering::Less,

            (JValue::Object(_), _) => Ordering::Greater,
            #[cfg(feature = "python")]
            (JValue::LazyPyDict(_), _) => Ordering::Greater,
            _ => Ordering::Equal,
        }
    }

    /// Check if a value is truthy (JSONata semantics).
    fn is_truthy(&self, value: &JValue) -> bool {
        // One definition of truthiness for both engines (see
        // `compiled_is_truthy` for the recursive-container rule, #111).
        compiled_is_truthy(value)
    }

    /// Unwrap singleton arrays to scalar values
    /// This is used when no explicit array-keeping operation (like []) was used
    fn unwrap_singleton(&self, value: JValue) -> JValue {
        match value {
            JValue::Array(ref arr) if arr.len() == 1 => arr[0].clone(),
            _ => value,
        }
    }

    /// Extract lambda IDs from a value (used for closure preservation)
    /// Finds any lambda_id references in the value so they can be preserved
    /// when exiting a block scope
    fn extract_lambda_ids(&self, value: &JValue) -> Vec<String> {
        // Fast path: scalars can never contain lambda references
        match value {
            JValue::Number(_)
            | JValue::Bool(_)
            | JValue::String(_)
            | JValue::Null
            | JValue::Undefined
            | JValue::Regex { .. }
            | JValue::Builtin { .. } => return Vec::new(),
            _ => {}
        }
        let mut ids = Vec::new();
        self.collect_lambda_ids(value, &mut ids);
        ids
    }

    fn collect_lambda_ids(&self, value: &JValue, ids: &mut Vec<String>) {
        match value {
            JValue::Lambda { lambda_id, .. } => {
                let id_str = lambda_id.to_string();
                if !ids.contains(&id_str) {
                    ids.push(id_str);
                    // Transitively follow the stored lambda's captured_env
                    // to find all referenced lambdas. This is critical for
                    // closures like the Y-combinator where returned lambdas
                    // capture other lambdas in their environment.
                    if let Some(stored) = self.context.lookup_lambda(lambda_id) {
                        let env_values: Vec<JValue> =
                            stored.captured_env.values().cloned().collect();
                        for env_value in &env_values {
                            self.collect_lambda_ids(env_value, ids);
                        }
                    }
                }
            }
            JValue::Object(map) => {
                // Recurse into object values
                for v in map.values() {
                    self.collect_lambda_ids(v, ids);
                }
            }
            JValue::Array(arr) => {
                // Recurse into array elements
                for v in arr.iter() {
                    self.collect_lambda_ids(v, ids);
                }
            }
            _ => {}
        }
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
            return Ok(JValue::array(vec![]));
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
        Ok(JValue::array(result))
    }

    /// In operator (checks if left is in right array/object)
    /// Array indexing: array[index]
    fn array_index(&self, array: &JValue, index: &JValue) -> Result<JValue, EvaluatorError> {
        match (array, index) {
            (JValue::Array(arr), JValue::Number(n)) => {
                let idx = *n as i64;
                let len = arr.len() as i64;

                // Handle negative indexing (offset from end)
                let actual_idx = if idx < 0 { len + idx } else { idx };

                if actual_idx < 0 || actual_idx >= len {
                    Ok(JValue::Undefined)
                } else {
                    Ok(arr[actual_idx as usize].clone())
                }
            }
            _ => Err(EvaluatorError::TypeError(
                "Array indexing requires array and number".to_string(),
            )),
        }
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
                (JValue::Array(x), JValue::Array(y)) => Rc::ptr_eq(x, y),
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
        let is_lambda = self.context.lookup_lambda(name).is_some()
            || (self
                .context
                .lookup(name)
                .map(|v| matches!(v, JValue::Lambda { .. }))
                .unwrap_or(false));

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
            compiled_body: None, // Partial application uses a special body marker
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
                    JValue::array(
                        placeholder_positions
                            .iter()
                            .map(|p| JValue::Number(*p as f64))
                            .collect::<Vec<_>>(),
                    ),
                );
                // Store total argument count
                env.insert(
                    "__total_args".to_string(),
                    JValue::Number(args.len() as f64),
                );
                env
            },
            captured_data: Some(data.clone()),
            thunk: false,
        };

        self.context.bind_lambda(partial_id.clone(), stored_lambda);

        // Return a lambda object that can be invoked
        let lambda_obj = JValue::lambda(
            partial_id.as_str(),
            param_names,
            Some(name.to_string()),
            None::<String>,
        );

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

        // Backward range (start > end) returns empty array
        let expr = AstNode::Binary {
            op: BinaryOp::Range,
            lhs: Box::new(AstNode::number(5.0)),
            rhs: Box::new(AstNode::number(1.0)),
        };
        let result = evaluator.evaluate(&expr, &data).unwrap();
        assert_eq!(result, JValue::array(vec![]));
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
        assert_eq!(
            result,
            serde_json::json!([{"name": "Hat", "order": "o1"}]).into()
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
}

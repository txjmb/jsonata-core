//! Compiled-expression API: parse once, evaluate many times, on the same
//! bytecode-VM dispatch the Python wheel and the C ABI use.
//!
//! [`Evaluator`](crate::evaluator::Evaluator) always tree-walks; this type
//! lowers the parsed expression to bytecode on first evaluation (when the
//! expression is compilable) and runs the VM, which is what the shipped
//! Python wheel has always done. This is the recommended entry point for
//! Rust callers evaluating one expression against many payloads.
//!
//! ```
//! use jsonata_core::{Expression, value::JValue};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let expr = Expression::compile("$sum(orders[price > 100].price)")?;
//! let data = JValue::from_json_str(r#"{"orders": [{"price": 150}, {"price": 50}]}"#)?;
//! assert_eq!(expr.evaluate(&data)?, JValue::Number(150.0));
//! # Ok(())
//! # }
//! ```

use std::cell::OnceCell;

use crate::ast::AstNode;
use crate::evaluator::{self, EvaluatorError, EvaluatorOptions};
use crate::parser::{self, ParserError};
use crate::value::JValue;
use crate::vm::BytecodeProgram;

/// Test-support toggle: bypass the bytecode VM and exercise the tree-walking
/// evaluator on every dispatch in this process. Seeded by the bindings at
/// module import from the JSONATAPY_FORCE_TREE_WALKER env var, and flipped
/// at runtime by the private Python test hooks. A relaxed atomic load is
/// ~1ns, cheap enough for the hot path (a per-call env read was not: #74).
static FORCE_TREE_WALKER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(crate) fn force_tree_walker() -> bool {
    FORCE_TREE_WALKER.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(feature = "python")] // only the Python test hooks flip it today
pub(crate) fn set_force_tree_walker(on: bool) {
    FORCE_TREE_WALKER.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// The one bytecode-or-tree-walker dispatch, shared by [`Expression`], the
/// Python bindings' `run_eval`, and the C ABI. Lowers the AST to bytecode on
/// first call (memoized in `bytecode`; `None` = not compilable) and runs the
/// VM; falls back to a fresh tree-walking evaluator otherwise.
///
/// Callers with bindings or host functions must NOT come through here — the
/// VM has no view of a host-function registry, and the binding policy
/// (tree-walker) is enforced at each binding surface.
pub(crate) fn run_compiled(
    ast: &AstNode,
    bytecode: &OnceCell<Option<BytecodeProgram>>,
    data: &JValue,
    options: EvaluatorOptions,
) -> Result<JValue, EvaluatorError> {
    if !force_tree_walker() {
        let bytecode = bytecode.get_or_init(|| {
            evaluator::try_compile_expr(ast)
                .map(|ce| crate::compiler::BytecodeCompiler::compile(&ce))
        });
        if let Some(bc) = bytecode {
            return crate::vm::Vm::with_options(bc, options).run(data, None);
        }
    }
    let mut ev = evaluator::Evaluator::with_options(evaluator::Context::new(), options);
    ev.evaluate(ast, data)
}

/// A parsed JSONata expression, reusable across payloads.
///
/// Compilable expressions run on the bytecode VM (lowered lazily on first
/// evaluation); the rest run on the tree-walking evaluator — the same
/// dispatch, with the same semantics, as the Python wheel. For bindings or
/// host functions use [`Evaluator`](crate::evaluator::Evaluator) directly;
/// both of those require the tree-walker.
pub struct Expression {
    ast: AstNode,
    bytecode: OnceCell<Option<BytecodeProgram>>,
    options: EvaluatorOptions,
}

impl Expression {
    /// Parse `source` into a reusable expression with default options.
    pub fn compile(source: &str) -> Result<Self, ParserError> {
        Self::compile_with_options(source, EvaluatorOptions::default())
    }

    /// Parse `source` with evaluation guardrails (timeout, stack depth,
    /// sequence length) applied to every subsequent `evaluate` call.
    pub fn compile_with_options(
        source: &str,
        options: EvaluatorOptions,
    ) -> Result<Self, ParserError> {
        Ok(Expression {
            ast: parser::parse(source)?,
            bytecode: OnceCell::new(),
            options,
        })
    }

    /// Wrap an already-parsed AST (default options).
    pub fn from_ast(ast: AstNode) -> Self {
        Expression {
            ast,
            bytecode: OnceCell::new(),
            options: EvaluatorOptions::default(),
        }
    }

    /// The parsed AST (e.g. for reuse with a configured `Evaluator`).
    pub fn ast(&self) -> &AstNode {
        &self.ast
    }

    /// Evaluate against `data` with this expression's options.
    pub fn evaluate(&self, data: &JValue) -> Result<JValue, EvaluatorError> {
        run_compiled(&self.ast, &self.bytecode, data, self.options.clone())
    }

    /// Evaluate against `data` with per-call options overriding the
    /// expression's own.
    pub fn evaluate_with_options(
        &self,
        data: &JValue,
        options: EvaluatorOptions,
    ) -> Result<JValue, EvaluatorError> {
        run_compiled(&self.ast, &self.bytecode, data, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jv(s: &str) -> JValue {
        JValue::from_json_str(s).unwrap()
    }

    #[test]
    fn compilable_expression_matches_tree_walker() {
        let data = jv(r#"{"orders": [{"price": 150}, {"price": 50}, {"price": 200}]}"#);
        for src in [
            "$sum(orders.price)",
            "orders[price > 100].price",
            "orders[0].price + orders[1].price",
        ] {
            let expr = Expression::compile(src).unwrap();
            let via_expr = expr.evaluate(&data).unwrap();
            let via_tree = evaluator::Evaluator::new()
                .evaluate(expr.ast(), &data)
                .unwrap();
            assert_eq!(via_expr, via_tree, "{src}");
            // Second call exercises the memoized program.
            assert_eq!(expr.evaluate(&data).unwrap(), via_tree, "{src} (memoized)");
        }
    }

    #[test]
    fn non_compilable_expression_falls_back() {
        // $eval never compiles to bytecode; the fallback tree-walker must run.
        let expr = Expression::compile("$eval(\"1 + 2\")").unwrap();
        assert_eq!(expr.evaluate(&jv("{}")).unwrap(), JValue::Number(3.0));
    }

    #[test]
    fn options_are_enforced() {
        let expr = Expression::compile_with_options(
            "$map([1..100000], function($x) { $x * 2 })",
            EvaluatorOptions {
                max_sequence_length: Some(10),
                ..EvaluatorOptions::default()
            },
        )
        .unwrap();
        assert!(expr.evaluate(&jv("{}")).is_err());
    }
}

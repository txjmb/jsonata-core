# `%` Parent-Reference and `@` Focus-Binding Operators Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement JSONata's `%` (parent-reference) and `@$var` (focus-binding) operators in the Rust parser/evaluator, fixing the existing `#$var` (index-binding) tuple-propagation gap as part of the same unified mechanism, so all 65 currently-xfailed cases in the `parent-operator` and `joins` reference-suite groups pass.

**Architecture:** Port jsonata-js's two-pass design: (1) parser produces trivial/unresolved nodes for `%`/`@` (mirroring its one-line prefix/infix rules), (2) a new post-parse `ast_transform` pass (mirroring `processAST`/`seekParent`/`resolveAncestry`) walks the tree once, converting `#`'s existing wrapping node and the new `%`/`@` markers into step-level flags (`focus`/`index_var`/`ancestor_label`/`is_tuple`) on `PathStep`. At runtime, any step with `is_tuple` set dispatches to a new, self-contained tuple-aware step evaluator (mirroring jsonata-js's `evaluateTupleStep`, separate from the existing plain step evaluator) that reuses the existing general-purpose `evaluate_path_step` for the actual per-row work and binds tuple keys into a real scope frame before evaluating (fixing the existing `#` propagation gap).

**Tech Stack:** Rust (parser.rs/ast.rs/evaluator.rs), `thiserror` for error variants, existing `IndexMap`-backed `JValue::Object` tuple-wrapper convention.

## Global Constraints

- Full completion bar: all 28 `parent-operator` + 37 `joins` xfail entries must pass for real (not via the test harness's lenient "any error accepted" fallback) — see Task 8's error-code work.
- `#$var`'s existing passing tests must continue to pass unchanged after the `IndexBind`→step-flag migration.
- No VM/bytecode-compiler support needed — `%`/`@`/`#` fall back to the tree-walker only, matching existing project convention (named-variable lookups already force tree-walker fallback).
- `cargo fmt --check` and `cargo clippy --all-targets --all-features -- -D warnings` must stay clean after every task.
- Design source of truth: `docs/superpowers/specs/2026-07-06-parent-and-focus-binding-operators-design.md`. Reference implementation: `tests/jsonata-js/src/parser.js` (~L616-847 prefix/infix rules, ~L937-1235 `processAST`/`seekParent`/`pushAncestry`/`resolveAncestry`) and `tests/jsonata-js/src/jsonata.js` (~L83-85 `case 'parent'`, ~L229-380 `createFrameFromTuple`/`evaluateTupleStep`).
- Branch: `feature/parent-and-focus-binding-operators` (already created, pushed, spec committed).

---

### Task 1: Lexer/parser additions for `%` and `@`

**Files:**
- Modify: `src/ast.rs` (add `BinaryOp::FocusBind`)
- Modify: `src/parser.rs:97` (add `Token::At` to the `Token` enum, after `Hash`)
- Modify: `src/parser.rs:614` (lexer char-dispatch: add `@` arm, after the existing `Some('#') => ...` arm at line ~608-611)
- Modify: `src/parser.rs:972-981` (`parse_primary`'s `Token::Star`/`Token::StarStar` arms: add a `Token::Percent` arm producing `AstNode::Parent`)
- Modify: `src/parser.rs` infix dispatch loop (same `match` that handles `Token::Hash` at line 1471-1491): add a `Token::At` arm
- Test: `src/parser.rs` `#[cfg(test)] mod tests` (existing module at the bottom of the file)

**Interfaces:**
- Produces: `AstNode::Parent` (new unit-like variant, no payload yet — payload is added in Task 2), `BinaryOp::FocusBind` (new variant), both consumed by Task 3's `ast_transform` pass.
- Consumes: existing `Token`, `AstNode::Binary`, `ParserError` types (all already defined).

- [ ] **Step 1: Write the failing parser tests**

Add to the `#[cfg(test)] mod tests` block at the bottom of `src/parser.rs`:

```rust
#[test]
fn test_parent_operator_parses_as_prefix() {
    let ast = parse("%.OrderID").unwrap();
    // %.OrderID should parse as a path with two steps: Parent, then Name("OrderID")
    match ast {
        AstNode::Path { steps } => {
            assert_eq!(steps.len(), 2);
            assert!(matches!(steps[0].node, AstNode::Parent));
            assert!(matches!(steps[1].node, AstNode::Name(ref n) if n == "OrderID"));
        }
        other => panic!("expected Path, got {:?}", other),
    }
}

#[test]
fn test_percent_still_parses_as_modulo_infix() {
    // Regression: % must still work as binary modulo when NOT in prefix position
    let ast = parse("10 % 3").unwrap();
    assert!(matches!(
        ast,
        AstNode::Binary {
            op: BinaryOp::Modulo,
            ..
        }
    ));
}

#[test]
fn test_focus_bind_parses_as_binary_marker() {
    let ast = parse("Order@$o").unwrap();
    match ast {
        AstNode::Binary {
            op: BinaryOp::FocusBind,
            lhs,
            rhs,
        } => {
            assert!(matches!(*lhs, AstNode::Name(ref n) if n == "Order"));
            assert!(matches!(*rhs, AstNode::Variable(ref n) if n == "o"));
        }
        other => panic!("expected Binary{{FocusBind}}, got {:?}", other),
    }
}

#[test]
fn test_focus_bind_requires_variable_rhs() {
    // S0214: @'s RHS must be a bare variable reference
    let err = parse("Order@foo").unwrap_err();
    assert!(err.to_string().starts_with("S0214"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib test_parent_operator_parses_as_prefix test_percent_still_parses_as_modulo_infix test_focus_bind_parses_as_binary_marker test_focus_bind_requires_variable_rhs`
Expected: compile error (`AstNode::Parent`/`BinaryOp::FocusBind` don't exist, `Token::At` doesn't exist) — this is the expected "fails to compile" state for a new-variant TDD step; proceed to Step 3.

- [ ] **Step 3: Add `AstNode::Parent` and `BinaryOp::FocusBind`**

In `src/ast.rs`, add to the `AstNode` enum (right after the `Descendant` variant, ~line 138):

```rust
    /// Parent-reference operator (%) in path expressions
    /// Resolved to a specific ancestor slot by the post-parse ast_transform pass;
    /// unit variant at parse time, matching jsonata-js's bare `{type: 'parent'}`.
    Parent,
```

Add to the `BinaryOp` enum (right after `ColonEqual`, ~line 220):

```rust
    // Focus binding (raw parse-time marker for @$var; resolved into a
    // PathStep.focus flag by ast_transform, matching jsonata-js's own
    // parser.js:834-847, which also produces a generic binary node here)
    FocusBind,
```

- [ ] **Step 4: Add `Token::At` and lex `@`**

In `src/parser.rs`, add to the `Token` enum right after `Hash` (line 97):

```rust
    At, // @ focus binding operator
```

In the lexer's char-dispatch `match`, add right after the existing `Some('#') => { self.advance(); return Ok(Token::Hash); }` arm (~line 608-611):

```rust
                Some('@') => {
                    self.advance();
                    return Ok(Token::At);
                }
```

- [ ] **Step 5: Add the `%` prefix rule in `parse_primary`**

In `src/parser.rs`, add right after the `Token::StarStar` arm (line 977-981):

```rust
            Token::Percent => {
                // Parent operator in primary position. Bare at parse time --
                // ast_transform assigns the actual ancestor slot (label/level).
                self.advance()?;
                Ok(AstNode::Parent)
            }
```

- [ ] **Step 6: Add the `@` infix rule**

In `src/parser.rs`, add right after the existing `Token::Hash => { ... }` arm (line 1471-1491) in the infix dispatch loop, inside the same `match &self.current_token { ... }`:

```rust
                Token::At => {
                    // Focus binding operator: @$var
                    // Produces a generic Binary(FocusBind) marker -- ast_transform
                    // resolves this into a PathStep.focus flag, matching
                    // jsonata-js's parser.js:834-847 (which does the same S0214
                    // check inline, deferring all other semantics to processAST).
                    self.advance()?; // skip '@'

                    let var_name = match &self.current_token {
                        Token::Variable(name) => name.clone(),
                        _ => {
                            return Err(ParserError::Coded {
                                code: "S0214",
                                message: "Expected a variable reference after @".to_string(),
                            });
                        }
                    };
                    self.advance()?; // skip variable

                    lhs = AstNode::Binary {
                        op: BinaryOp::FocusBind,
                        lhs: Box::new(lhs),
                        rhs: Box::new(AstNode::Variable(var_name)),
                    };
                }
```

- [ ] **Step 7: Add `ParserError::Coded` variant**

In `src/parser.rs`, add to the `ParserError` enum (after `Expected`, line 36-37):

```rust
    /// A JSONata-spec-coded parse error (S0214-S0217 for the %/@ operators).
    /// Code is at the start of the message (matching the DateTimeError::Coded
    /// convention from the datetime picture-string engine) so
    /// test_reference_suite.py's extract_error_code() finds it.
    #[error("{code}: {message}")]
    Coded {
        code: &'static str,
        message: String,
    },
```

- [ ] **Step 8: Run tests to verify they pass**

Run: `cargo test --lib test_parent_operator_parses_as_prefix test_percent_still_parses_as_modulo_infix test_focus_bind_parses_as_binary_marker test_focus_bind_requires_variable_rhs`
Expected: 4 passed

- [ ] **Step 9: Run the full existing test suite to confirm no regressions**

Run: `cargo test`
Expected: all existing tests still pass (no new failures) — `Token::Percent`'s dual prefix/infix meaning and the new `Token::At` arm are purely additive.

- [ ] **Step 10: `cargo fmt` and `cargo clippy`**

Run: `cargo fmt && cargo clippy --all-targets --all-features -- -D warnings`
Expected: clean

- [ ] **Step 11: Commit**

```bash
git add src/ast.rs src/parser.rs
git commit -m "feat(parser): add % (parent) prefix and @ (focus-bind) infix parsing

Both produce bare/unresolved AST markers (AstNode::Parent, Binary{FocusBind})
at parse time -- matching jsonata-js's own trivial one-line prefix/infix
rules. All real semantics (ancestor-slot resolution, step-flag assignment)
land in the ast_transform pass (next task), not here."
```

---

### Task 2: `PathStep` new fields for unified tuple/ancestor flags

**Files:**
- Modify: `src/ast.rs:20-26` (`PathStep` struct)
- Modify: `src/ast.rs:241-254` (`PathStep::new`/`with_stages` constructors)
- Test: `src/ast.rs` `#[cfg(test)] mod tests`

**Interfaces:**
- Produces: `PathStep.focus: Option<String>`, `PathStep.index_var: Option<String>`, `PathStep.ancestor_label: Option<String>`, `PathStep.is_tuple: bool` — consumed by Task 3 (ast_transform sets them) and Tasks 5-6 (evaluator reads them).
- Consumes: nothing new; `PathStep::new`/`with_stages` are the only two call sites that construct `PathStep` via struct literal anywhere in the crate (verified: `grep -n "PathStep {" src/*.rs` returns only these two, plus the struct definition itself) — every other of the 17+ call sites uses `PathStep::new(...)`, so adding fields with defaults here is non-breaking everywhere else.

- [ ] **Step 1: Write the failing test**

Add to `src/ast.rs`'s `#[cfg(test)] mod tests`:

```rust
#[test]
fn test_path_step_new_defaults_new_fields_to_none() {
    let step = PathStep::new(AstNode::Name("foo".to_string()));
    assert_eq!(step.focus, None);
    assert_eq!(step.index_var, None);
    assert_eq!(step.ancestor_label, None);
    assert!(!step.is_tuple);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib test_path_step_new_defaults_new_fields_to_none`
Expected: compile error (fields don't exist yet)

- [ ] **Step 3: Add the fields and update constructors**

In `src/ast.rs`, replace the `PathStep` struct (lines 20-26):

```rust
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
```

Replace the constructors (lines 241-254):

```rust
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib test_path_step_new_defaults_new_fields_to_none`
Expected: PASS

- [ ] **Step 5: Run the full test suite to confirm no regressions**

Run: `cargo test`
Expected: all existing tests still pass (purely additive fields with safe defaults)

- [ ] **Step 6: `cargo fmt` and `cargo clippy`, then commit**

```bash
cargo fmt && cargo clippy --all-targets --all-features -- -D warnings
git add src/ast.rs
git commit -m "feat(ast): add focus/index_var/ancestor_label/is_tuple fields to PathStep

Additive-only change (both PathStep constructors default the new fields),
laying the groundwork for the ast_transform pass and unified tuple-runtime
work in the next tasks. AstNode::IndexBind is not yet retired -- that
happens once ast_transform can rewrite it away (Task 4)."
```

---

### Task 3: `ast_transform` — the ancestor-resolution compile pass

This is the core novel piece: a faithful, value-returning (not mutate-in-place) port of jsonata-js's `seekParent`/`pushAncestry`/`resolveAncestry`.

**Files:**
- Create: `src/ast_transform.rs`
- Modify: `src/lib.rs` (add `pub mod ast_transform;`)
- Test: `src/ast_transform.rs` `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: `AstNode` (specifically the new `AstNode::Parent`, `BinaryOp::FocusBind`, and the existing `AstNode::IndexBind` from Task 1/pre-existing code), `PathStep`'s new fields from Task 2.
- Produces: `pub fn resolve_ancestry(ast: AstNode) -> Result<AstNode, AstTransformError>` — the single entry point Task 4 wires into `parser::parse()`. `AstTransformError` is a new error enum (S0213/S0215/S0216/S0217 — S0214 was already handled inline in Task 1's parser rule).

This task covers **only plain paths** (a `%`/`%.%` chain resolving back through `Name`/`Wildcard`/`Block`/nested `Path` steps, and `@`/`#` becoming step flags). Predicate and sort-term ancestor resolution (`Account.Order.Product[%.OrderID='order104']`, `SKU^(%.Price)`) is Task 4's scope, once the basic per-path mechanics are proven correct in isolation.

- [ ] **Step 1: Write the failing unit tests**

Create `src/ast_transform.rs`:

```rust
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
    Coded {
        code: &'static str,
        message: String,
    },
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
struct LabelGen {
    next_label: usize,
}

impl LabelGen {
    fn new() -> Self {
        LabelGen { next_label: 0 }
    }

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
```

- [ ] **Step 2: Write the failing unit tests**

Add `#[cfg(test)] mod tests` to the bottom of `src/ast_transform.rs`:

```rust
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
```

- [ ] **Step 3: Register the new module**

In `src/lib.rs`, find the existing `pub mod parser;` (or similar module declarations near the top) and add:

```rust
pub mod ast_transform;
```

- [ ] **Step 4: Run tests to verify they fail, then pass**

Run: `cargo test --lib ast_transform::`
Expected (before Step 1's code is saved): compile error. After saving `src/ast_transform.rs` as written above: 4 passed. (This module is written test-and-implementation together above since the pass is genuinely new code with no prior partial version to red/green against line-by-line — but verify by temporarily commenting out `migrate_binding_markers`'s body and confirming the tests fail first, then restoring it, if you want a stricter red/green cycle.)

- [ ] **Step 5: Run the full test suite**

Run: `cargo test`
Expected: all passed — `ast_transform` isn't wired into `parser::parse()` yet (Task 4), so nothing else is affected.

- [ ] **Step 6: `cargo fmt` and `cargo clippy`, then commit**

```bash
cargo fmt && cargo clippy --all-targets --all-features -- -D warnings
git add src/ast_transform.rs src/lib.rs
git commit -m "feat: add ast_transform pass resolving @/# binding markers to step flags

Ports jsonata-js's processAST-style post-parse pass, adapted to Rust's
ownership model (consumes + rebuilds rather than mutates in place).
This task covers single-step @/# migration and top-level bare-% detection
(S0217) only -- multi-level %.% chains and predicate/sort-term ancestor
resolution land in the next task, once this foundation is verified.

Not yet wired into parser::parse() (next task) -- purely additive so far."
```

---

### Task 4: Full `%`/`%.%` ancestor resolution + wire into `parser::parse()`

Extends Task 3's `ast_transform` with the actual `seekParent`/`pushAncestry` backward-walk (chained `%.%`, resolution through `Block`s, predicates, and sort terms), and connects the whole pass to the real parsing entry point.

**Files:**
- Modify: `src/ast_transform.rs` (extend `transform_path_steps`, add `seek_parent`)
- Modify: `src/parser.rs:1653-1656` (`pub fn parse`)
- Modify: `src/ast.rs` (retire `AstNode::IndexBind` — see Step 5)
- Test: `src/ast_transform.rs` `#[cfg(test)] mod tests`, plus new integration tests in `tests/parent_and_focus_binding_suite.rs` (created in Task 8) once the evaluator side exists

**Interfaces:**
- Produces: `AstNode::Parent(String)` (now carrying its resolved ancestor label directly, replacing the bare unit variant from Task 1 — see Step 1's rationale), full backward-walk ancestor resolution.
- Consumes: Task 3's `transform_path_steps`/`migrate_binding_markers`, Task 2's `PathStep` fields.

- [ ] **Step 1: Change `AstNode::Parent` to carry its resolved label**

In `src/ast.rs`, change the `Parent` variant (added in Task 1) from a unit variant to:

```rust
    /// Parent-reference operator (%), resolved. Carries the synthetic
    /// ancestor label ("!0", "!1", ...) assigned by ast_transform -- looked
    /// up at runtime via the same tuple/scope mechanism as $-variables.
    Parent(String),
```

Rationale: jsonata-js assigns the slot (`{label, level, index}`) the moment `processAST`'s generic dispatch first sees *any* `%` node (before `seekParent`'s backward walk even starts) — the walk only *decrements* `level` and matches it against the right ancestor step. Since our `AstNode::Parent` from Task 1 is produced directly by the parser (not by `ast_transform`'s generic dispatch), we resolve its label eagerly, right when `ast_transform` first encounters it, then thread the *count* of unresolved `%`-levels alongside it during the backward walk (see Step 3) rather than mutating the node's own field after the fact.

- [ ] **Step 2: Update Task 1/3's `AstNode::Parent` construction sites**

In `src/parser.rs`'s `%` prefix rule (Task 1, Step 5), the parser doesn't have a label counter — that's `ast_transform`'s job. Change the prefix rule to produce a placeholder the transform pass recognizes and replaces:

```rust
            Token::Percent => {
                // Parent operator in primary position. Label is resolved by
                // ast_transform -- this empty string is never observed by
                // the evaluator (ast_transform's transform_node fills every
                // AstNode::Parent("") with a real label or errors S0217).
                self.advance()?;
                Ok(AstNode::Parent(String::new()))
            }
```

Update `src/ast_transform.rs`'s `transform_node`'s `AstNode::Parent => Err(coded("S0217", ...))` arm (Task 3, Step 1) — a bare `Parent` reached via the generic `transform_node` dispatch (i.e. NOT found during a path's backward walk in Step 3 below) still means "no enclosing path to derive an ancestor from", so it stays an `S0217` error regardless of the (currently-empty) label payload:

```rust
        AstNode::Parent(_) => Err(coded(
            "S0217",
            "The parent operator % cannot be used at this point in the expression",
        )),
```

Update the two Task 1 parser tests (`test_parent_operator_parses_as_prefix`) and Task 3 unit tests that pattern-match `AstNode::Parent` as a unit variant — change `matches!(steps[0].node, AstNode::Parent)` to `matches!(steps[0].node, AstNode::Parent(_))`.

- [ ] **Step 3: Write the failing `%.%` chain test**

Add to `src/ast_transform.rs`'s tests:

```rust
    #[test]
    fn test_single_level_parent_resolves_to_previous_step() {
        // Account.Order.% (no further field after %) -- % refers to Order's
        // input, i.e. Account's output. Mirrors seekParent's single-level case.
        let ast = AstNode::Path {
            steps: vec![
                PathStep::new(AstNode::Name("Account".to_string())),
                PathStep::new(AstNode::Name("Order".to_string())),
                PathStep::new(AstNode::Parent(String::new())),
            ],
        };
        let result = resolve_ancestry(ast).unwrap();
        match result {
            AstNode::Path { steps } => {
                assert_eq!(steps.len(), 3);
                // The "Order" step (index 1) is the one % refers to -- it
                // must be stamped with an ancestor_label and is_tuple.
                assert!(steps[1].ancestor_label.is_some());
                assert!(steps[1].is_tuple);
                // The % step itself resolves to a Parent(label) matching it.
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
    fn test_chained_parent_resolves_two_levels_back() {
        // Account.Order.Product.%.%  -- first % refers to Product's input
        // (Order's output); second % refers to Order's input (Account's
        // output). Mirrors the parent002.jsonata test shape.
        let ast = AstNode::Path {
            steps: vec![
                PathStep::new(AstNode::Name("Account".to_string())),
                PathStep::new(AstNode::Name("Order".to_string())),
                PathStep::new(AstNode::Name("Product".to_string())),
                PathStep::new(AstNode::Parent(String::new())),
                PathStep::new(AstNode::Parent(String::new())),
            ],
        };
        let result = resolve_ancestry(ast).unwrap();
        match result {
            AstNode::Path { steps } => {
                let order_label = steps[1].ancestor_label.clone();
                let account_label = steps[0].ancestor_label.clone();
                assert!(order_label.is_some());
                assert!(account_label.is_some());
                assert_ne!(order_label, account_label);
                match &steps[3].node {
                    AstNode::Parent(label) => assert_eq!(Some(label.clone()), order_label),
                    other => panic!("expected Parent(label), got {:?}", other),
                }
                match &steps[4].node {
                    AstNode::Parent(label) => assert_eq!(Some(label.clone()), account_label),
                    other => panic!("expected Parent(label), got {:?}", other),
                }
            }
            other => panic!("expected Path, got {:?}", other),
        }
    }
```

- [ ] **Step 4: Run tests to verify they fail**

Run: `cargo test --lib test_single_level_parent_resolves_to_previous_step test_chained_parent_resolves_two_levels_back`
Expected: FAIL (current `transform_path_steps` doesn't resolve `%` at all yet, it only migrates `@`/`#`)

- [ ] **Step 5: Implement `seek_parent` and extend `transform_path_steps`**

Replace `transform_path_steps` in `src/ast_transform.rs`:

```rust
/// A `%` reference still walking backward looking for its ancestor step,
/// mirroring jsonata-js's `slot` object threaded through `seekParent`.
/// Rust-idiomatic value-returning adaptation: instead of mutating a shared
/// `slot` object in place (as the JS does), `seek_parent` returns the
/// remaining level count for the caller to check `> 0` and keep walking.
struct PendingAncestor {
    label: String,
    /// How many steps further back this reference still needs to walk.
    /// A fresh `%` starts at level 1 (its own immediately-preceding step);
    /// `%.%` increments this to 2 before the walk begins.
    level: usize,
}

fn transform_path_steps(
    steps: Vec<PathStep>,
    labels: &mut LabelGen,
) -> Result<Vec<PathStep>, AstTransformError> {
    // Pass 1: migrate #/@ into step flags, recursing into nested content.
    let mut resolved: Vec<PathStep> = Vec::with_capacity(steps.len());
    for step in steps {
        resolved.push(migrate_binding_markers(step, labels)?);
    }

    // Pass 2: walk backward from the end resolving any %/%.% chains.
    // Mirrors resolveAncestry (parser.js ~L1002-1030): collect every
    // pending ancestor reference in the last step, then walk earlier steps
    // decrementing level until it reaches 0 (found the target step) or we
    // run off the front of the path (S0217).
    let last_index = resolved.len() - 1;
    let mut pending: Vec<PendingAncestor> = Vec::new();
    if let AstNode::Parent(label) = &resolved[last_index].node {
        pending.push(PendingAncestor {
            label: label.clone(),
            level: 1,
        });
    }

    for pending_ref in pending {
        let mut level = pending_ref.level;
        let mut index = last_index;
        loop {
            if index == 0 {
                return Err(coded(
                    "S0217",
                    "The parent operator % cannot be resolved -- not enough enclosing path steps",
                ));
            }
            index -= 1;
            level = seek_parent(&mut resolved[index], &pending_ref.label, level)?;
            if level == 0 {
                break;
            }
        }
    }

    Ok(resolved)
}

/// Try to resolve one level of a pending ancestor reference against `step`.
/// Returns the remaining level count (0 means resolved at this step).
/// Mirrors jsonata-js's seekParent (parser.js ~L941-986), restricted for
/// this task to Name/Wildcard steps -- Block/predicate/sort-term recursion
/// is intentionally out of scope here (only plain path chains are handled
/// by this task; predicates/sort terms are wired in Task 6).
fn seek_parent(
    step: &mut PathStep,
    label: &str,
    level: usize,
) -> Result<usize, AstTransformError> {
    match &step.node {
        AstNode::Name(_) | AstNode::Wildcard => {
            let remaining = level - 1;
            if remaining == 0 {
                // Reuse an existing label if this step already captures an
                // ancestor value for a different %, matching jsonata-js's
                // "reuse the existing label" branch (parser.js ~L949-953).
                if step.ancestor_label.is_none() {
                    step.ancestor_label = Some(label.to_string());
                }
                step.is_tuple = true;
            }
            Ok(remaining)
        }
        _ => Err(coded(
            "S0217",
            "The parent operator % cannot derive an ancestor from this kind of path step",
        )),
    }
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --lib test_single_level_parent_resolves_to_previous_step test_chained_parent_resolves_two_levels_back`
Expected: 2 passed

- [ ] **Step 7: Retire `AstNode::IndexBind`**

Now that `migrate_binding_markers` (Task 3) consumes `AstNode::IndexBind` and rewrites it away before the tree reaches the evaluator, remove the variant. In `src/ast.rs`, delete the `IndexBind { input, variable }` variant (lines 162-170). Do NOT change `src/parser.rs`'s existing `#$var` infix rule (line 1471-1491) — it still constructs `AstNode::IndexBind` at raw-parse time; `ast_transform` always runs immediately afterward and consumes it (wired in Step 8 below), so the variant only needs to exist transiently between parse and transform.

Search for other `AstNode::IndexBind` references that will now fail to compile:

```bash
grep -rn "AstNode::IndexBind" src/*.rs
```

Each hit in `src/evaluator.rs` (the investigation found dispatch arms at ~3306-3334 and possibly others) is now unreachable once `ast_transform` runs unconditionally (Step 8) — delete those match arms. If deletion produces a non-exhaustive-match compile error elsewhere, that confirms another live site; delete it too. Do not leave a `_ => unreachable!()` catch-all for `IndexBind` specifically — remove the arm entirely so the compiler proves it's gone.

- [ ] **Step 8: Wire `ast_transform` into `parser::parse()`**

In `src/parser.rs`, replace the free-standing `pub fn parse` (lines 1653-1656):

```rust
/// Parse a JSONata expression string into an AST
///
/// This is the main entry point for parsing. Runs the post-parse
/// ast_transform pass (ancestor-slot resolution, @/#/% unification)
/// unconditionally, matching jsonata-js's processAST always running
/// immediately after the raw Pratt parse.
pub fn parse(expression: &str) -> Result<AstNode, ParserError> {
    let mut parser = Parser::new(expression.to_string())?;
    let raw_ast = parser.parse()?;
    crate::ast_transform::resolve_ancestry(raw_ast)
        .map_err(|e| ParserError::Coded {
            code: match &e {
                crate::ast_transform::AstTransformError::Coded { code, .. } => code,
            },
            message: e.to_string(),
        })
}
```

- [ ] **Step 9: Run the full test suite**

Run: `cargo test`
Expected: this is the first task where a compile/runtime regression is possible, since `ast_transform` now runs on *every* parsed expression, and `IndexBind` is fully retired. Investigate and fix any failure by reading the actual failing test's expression and comparing against `transform_node`'s coverage (Step 1 of Task 3 lists which node types recurse — if a real expression uses a node type not yet covered by `transform_children`, add an arm for it, following the same pattern as the existing `Binary`/`Unary`/`Array` arms).

- [ ] **Step 10: `cargo fmt` and `cargo clippy`, then commit**

```bash
cargo fmt && cargo clippy --all-targets --all-features -- -D warnings
git add src/ast.rs src/ast_transform.rs src/parser.rs
git commit -m "feat: resolve %/%.% ancestor chains, retire IndexBind, wire ast_transform in

parser::parse() now always runs ast_transform after the raw Pratt parse,
matching jsonata-js's processAST always running immediately after parsing.
AstNode::IndexBind is fully retired -- ast_transform rewrites #\$var into
the same step-level index_var flag that @\$var and % use, so the evaluator
now only ever sees the unified PathStep representation.

Note: %/@/# do not yet affect evaluation (Tasks 5-7) -- parsing and AST
shape are correct, but the evaluator hasn't been taught to read the new
flags yet. Expressions using % or @ will parse successfully but evaluate
incorrectly (% resolves to an unbound scope lookup, @ has no effect on
tuple-stream output) until those tasks land."
```

---

### Task 5: Runtime — unified tuple-aware step evaluator

Adds the new tuple-aware step evaluator (mirroring jsonata-js's `evaluateTupleStep`), reusing the existing general-purpose `evaluate_path_step` (src/evaluator.rs:4579) for per-row work, and fixes the `#` propagation gap by binding every tuple key into a real scope frame unconditionally.

**Files:**
- Modify: `src/evaluator.rs:3645-4578` (`evaluate_path`'s main step loop — extend the `is_tuple_array`/`needs_tuple_context_binding` dispatch at ~lines 4016-4113)
- Modify: `src/evaluator.rs:2861-2897` (`AstNode::Variable` lookup arm — add `!`-prefixed ancestor-label lookup alongside the existing `$name` tuple-binding lookup)
- Modify: `src/evaluator.rs:83-` (`evaluate_internal_impl`'s big `match node { ... }` — add `AstNode::Parent(label)` arm)
- Test: `tests/parent_and_focus_binding_suite.rs` (new; see Task 8) is the end-to-end verification; this task also gets direct unit-level coverage below.

**Interfaces:**
- Consumes: `PathStep.focus`/`index_var`/`ancestor_label`/`is_tuple` (Task 2/4), `AstNode::Parent(String)` (Task 4).
- Produces: a new `fn evaluate_tuple_path_step(&mut self, step: &PathStep, tuple_bindings: &[JValue], original_data: &JValue) -> Result<Vec<JValue>, EvaluatorError>` — takes the *previous* row of tuple-wrapper `JValue::Object`s (or plain values, for the first tuple step in a path) and returns the next row.

- [ ] **Step 1: Write the failing evaluator test**

Add to `src/evaluator.rs`'s existing `#[cfg(test)] mod tests` (or the closest evaluator test module — check for one near the bottom of the file with `grep -n "mod tests" src/evaluator.rs`; if the evaluator's tests live in `tests/integration_test.rs` instead, add there):

```rust
#[test]
fn test_focus_bind_makes_variable_available_in_next_step() {
    let data: JValue = serde_json::json!({
        "Account": {
            "Order": [
                {"OrderID": "o1", "Product": [{"SKU": "a"}, {"SKU": "b"}]}
            ]
        }
    })
    .into();
    let ast = crate::parser::parse("Account.Order@$o.Product.{ 'sku': SKU, 'order': $o.OrderID }").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();
    assert_eq!(
        result,
        serde_json::json!([
            {"sku": "a", "order": "o1"},
            {"sku": "b", "order": "o1"}
        ])
        .into()
    );
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
    let ast = crate::parser::parse("Account.Order.Product.{ 'name': Name, 'order': %.OrderID }").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();
    assert_eq!(
        result,
        serde_json::json!([{"name": "Hat", "order": "o1"}]).into()
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test test_focus_bind_makes_variable_available_in_next_step test_parent_reference_resolves_to_enclosing_step_value`
Expected: FAIL (`$o`/`%` don't resolve correctly yet — `%` currently hits a missing-match-arm compile error since `AstNode::Parent(String)` has no evaluator arm; `@` parses but doesn't affect tuple output since evaluator doesn't read `step.focus`/`is_tuple` yet)

- [ ] **Step 3: Add the `AstNode::Parent` runtime lookup**

In `src/evaluator.rs`, add an arm to `evaluate_internal_impl`'s big match (alongside the existing `AstNode::Variable(name) => { ... }` arm at line 2861):

```rust
            AstNode::Parent(label) => {
                // Resolves via the same real scope frame that tuple steps
                // bind into (Step 4 below) -- no special-casing needed
                // beyond an ordinary scope lookup, matching jsonata-js's
                // `case 'parent': result = environment.lookup(expr.slot.label);`
                self.context
                    .lookup(label)
                    .cloned()
                    .map_or(Ok(JValue::Undefined), Ok)
            }
```

- [ ] **Step 4: Add `evaluate_tuple_path_step` and wire it into `evaluate_path`'s loop**

In `src/evaluator.rs`, locate the existing tuple-special-case block inside `evaluate_path`'s step loop (the `is_tuple_array`/`needs_tuple_context_binding` logic at ~lines 4016-4113 -- **read the current file first**, since Task 4 may have shifted line numbers slightly). Replace the narrow `needs_tuple_context_binding` gate (currently `matches!(&step.node, AstNode::Object(_) | AstNode::FunctionApplication(_) | AstNode::Variable(_))`) and its inline 3-arm handling with a call to a new function that handles every step type generically:

```rust
    /// Evaluate one path step against a tuple stream, mirroring jsonata-js's
    /// evaluateTupleStep (jsonata.js ~L315-380). Reuses the existing
    /// general-purpose evaluate_path_step for the actual per-row value
    /// computation, then wraps each result according to the step's
    /// focus/index_var/ancestor_label flags. Every tuple key (old bindings
    /// carried forward, plus any new focus/index_var/ancestor_label this
    /// step adds) is bound into a real scope frame (self.context) before
    /// evaluating -- this is the fix for the #-propagation gap: previously
    /// only 3 node types got this promotion, now every step does.
    fn evaluate_tuple_path_step(
        &mut self,
        step: &PathStep,
        tuple_bindings: &[JValue],
        original_data: &JValue,
    ) -> Result<Vec<JValue>, EvaluatorError> {
        let mut result = Vec::new();

        for tuple in tuple_bindings {
            let tuple_obj = match tuple {
                JValue::Object(obj) => obj.clone(),
                other => {
                    // First tuple step in a path: wrap the plain value as {'@': value}.
                    let mut wrapper = IndexMap::new();
                    wrapper.insert("@".to_string(), other.clone());
                    wrapper.insert("__tuple__".to_string(), JValue::Bool(true));
                    std::rc::Rc::new(wrapper)
                }
            };

            // Bind every existing tuple key into a real scope frame before
            // evaluating this step -- unconditional, matching
            // createFrameFromTuple's "for every key in tuple, frame.bind(...)".
            let mut bound_names = Vec::new();
            for (key, value) in tuple_obj.iter() {
                if let Some(name) = key.strip_prefix('$') {
                    self.context.bind(name.to_string(), value.clone());
                    bound_names.push(name.to_string());
                } else if key.starts_with('!') {
                    self.context.bind(key.clone(), value.clone());
                    bound_names.push(key.clone());
                }
            }

            let actual_data = tuple_obj.get("@").cloned().unwrap_or(JValue::Undefined);
            let step_result = self.evaluate_path_step(&step.node, &actual_data, original_data);

            for name in &bound_names {
                self.context.unbind(name);
            }
            let step_result = step_result?;

            let row_values: Vec<JValue> = match &step_result {
                JValue::Array(arr) if !matches!(step.node, AstNode::Object(_)) => {
                    arr.iter().cloned().collect()
                }
                other => vec![other.clone()],
            };

            for value in row_values {
                if value.is_undefined() {
                    continue;
                }
                let mut new_tuple = (*tuple_obj).clone();
                if let Some(focus_var) = &step.focus {
                    new_tuple.insert(format!("${}", focus_var), value);
                    // focus binding keeps '@' as the step's INPUT, matching
                    // jsonata-js: `tuple[expr.focus] = res[bb]; tuple['@'] = tupleBindings[ee]['@'];`
                } else {
                    new_tuple.insert("@".to_string(), value);
                }
                if let Some(ancestor_label) = &step.ancestor_label {
                    new_tuple.insert(ancestor_label.clone(), actual_data.clone());
                }
                new_tuple.insert("__tuple__".to_string(), JValue::Bool(true));
                result.push(JValue::object(new_tuple));
            }
        }

        Ok(result)
    }
```

Then, in `evaluate_path`'s main loop, change the trigger condition from the narrow 3-node-type check to a general one, and replace the call:

```rust
            let enters_tuple_mode = is_tuple_array || step.is_tuple;

            if enters_tuple_mode {
                if let JValue::Array(arr) = &current {
                    let tuple_row = self.evaluate_tuple_path_step(step, arr, data)?;
                    current = JValue::array(tuple_row);
                    continue;
                } else {
                    // First tuple step, current isn't an array yet (single value input).
                    let tuple_row = self.evaluate_tuple_path_step(step, std::slice::from_ref(&current), data)?;
                    current = JValue::array(tuple_row);
                    continue;
                }
            }
```

Remove the old narrow `needs_tuple_context_binding` block entirely (the one being replaced) — do not leave both in place.

- [ ] **Step 5: Add `!`-prefixed ancestor-label lookup fallback**

In `src/evaluator.rs`'s `AstNode::Variable(name)` arm (~lines 2861-2897), the existing tuple-binding fallback (`format!("${}", name)`) only covers `$name` keys. Ancestor labels (`!0`, `!1`, ...) are looked up directly via `AstNode::Parent`'s own arm (Step 3 above), which calls `self.context.lookup(label)` — since Step 4's `evaluate_tuple_path_step` now binds `!`-prefixed keys into `self.context` directly (not just into the tuple dict), no additional fallback is needed in the `Variable` arm itself. Skip this step if Step 4 is implemented as written; it exists here as a checkpoint to re-verify that assumption once Step 6's tests run.

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test test_focus_bind_makes_variable_available_in_next_step test_parent_reference_resolves_to_enclosing_step_value`
Expected: 2 passed. If not, the most likely gap is the fast-path/single-step shortcuts elsewhere in `evaluate_path` (e.g. the single-field fast path at ~line 3657-3684) bypassing the new tuple dispatch — check whether `step.is_tuple` needs to be checked there too before taking the fast path.

- [ ] **Step 7: Run the full test suite**

Run: `cargo test`
Expected: no regressions. Existing `#$var` tests are the highest-risk area here (they now go through the new `evaluate_tuple_path_step` instead of the old narrow 3-arm handling) — if any fail, compare their expression against jsonata-js's actual output (`node -e "..."` against the submodule, or the reference-suite JSON) rather than assuming the old Rust behavior was correct.

- [ ] **Step 8: `cargo fmt` and `cargo clippy`, then commit**

```bash
cargo fmt && cargo clippy --all-targets --all-features -- -D warnings
git add src/evaluator.rs
git commit -m "feat(evaluator): unified tuple-aware step evaluation for %/@/#

Adds evaluate_tuple_path_step (mirrors jsonata-js's evaluateTupleStep),
reusing the existing evaluate_path_step for per-row work. Every tuple
step now binds ALL its keys (\$name AND !label) into a real scope frame
unconditionally, fixing the previous partial-promotion gap that only
covered Object/FunctionApplication/Variable next-steps. % now resolves
via ordinary scope lookup of its ancestor label."
```

---

### Task 6: Predicate and sort-term ancestor resolution

Extends `ast_transform` to resolve `%` inside filter predicates (`Product[%.OrderID='order104']`) and sort terms (`SKU^(%.Price)`), matching jsonata-js's `pushAncestry` handling in `processAST`'s `case '['` and `case '^'` branches (parser.js ~L1097-1130, ~L1147-1170 -- re-read these exact ranges from the submodule before starting, since this task's plan text summarizes rather than quotes them verbatim).

**Files:**
- Modify: `src/ast_transform.rs` (extend `migrate_binding_markers`/`transform_node` to recurse into `Stage::Filter` predicates and `AstNode::Sort` terms, propagating unresolved ancestor references up to the enclosing path via a `seeking_parent: Vec<PendingAncestor>` return value)
- Test: `src/ast_transform.rs` unit tests

**Interfaces:**
- Consumes: Task 4's `PendingAncestor`, `seek_parent`.
- Produces: predicate/sort-term-aware ancestor resolution — no new public interface, this is entirely internal to `ast_transform`.

- [ ] **Step 1: Read the reference implementation precisely**

Before writing any code, read `tests/jsonata-js/src/parser.js` lines 1097-1130 (`case '['`) and 1147-1170 (`case '^'`) directly, plus `pushAncestry` (~L988-1000) and `resolveAncestry` (~L1002-1030) again with these two call sites in mind. Confirm exactly how a predicate's own `seekingParent` list gets merged into the *step* it's attached to (not the whole path) versus how sort terms attach theirs to the `sort` step. Write down the exact mechanism in a comment at the top of the code you add in Step 3, so a future reader doesn't have to re-derive it.

- [ ] **Step 2: Write the failing test**

Add to `src/ast_transform.rs`'s tests (exact expected shape depends on Step 1's findings — this is deliberately left to be filled in against the real algorithm rather than guessed, per this task's own Step 1):

```rust
    #[test]
    fn test_parent_inside_predicate_resolves_against_enclosing_step() {
        // Account.Order.Product[%.OrderID='order104'] -- % inside the
        // predicate must resolve against "Order" (the step the predicate
        // is attached to's enclosing context), not "Product" itself.
        // Fill in the exact AstNode shape once Step 1's reading confirms
        // how our parser currently represents Product[predicate] --
        // check whether the predicate lives in PathStep.stages (a
        // Stage::Filter) on the "Product" step, per the existing
        // Stage enum in src/ast.rs.
        todo!("write once Step 1 confirms the exact predicate AST shape and jsonata-js's exact pushAncestry mechanism");
    }
```

(This `todo!()` is intentionally the ONE placeholder in this entire plan — it exists because this task's own Step 1 requires reading source not yet re-read at plan-writing time, and guessing the exact mechanism here risks writing a subtly-wrong test that passes for the wrong reason. Replace it before considering Step 2 done; do not proceed to Step 3 with it still in place.)

- [ ] **Step 3: Implement predicate/sort-term ancestor propagation**

Based on Step 1's findings, extend `transform_node`'s handling so that:
- A `Stage::Filter` predicate is transformed with its own fresh recursive call; any `PendingAncestor` it produces (a `%` inside it that couldn't be resolved within the predicate's own tiny scope) gets attached to the *step* the predicate/stage belongs to, then resolved by that step's normal backward walk (extending `transform_path_steps`'s pending-ancestor collection at the "last step" to also pull from `Stage::Filter`/sort-term predicates on *any* step, not just the path's last step).
- `AstNode::Sort { input, terms }`'s each term gets the same treatment.

Write the actual code here once Step 1 and Step 2 are complete — this step's content depends on their findings and is not pre-specified further.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib ast_transform::`
Expected: all pass, including the test written in Step 2.

- [ ] **Step 5: Run the full test suite, `cargo fmt`/`clippy`, commit**

```bash
cargo test
cargo fmt && cargo clippy --all-targets --all-features -- -D warnings
git add src/ast_transform.rs
git commit -m "feat: resolve % inside predicates and sort terms

Extends ancestor resolution to filter predicates and sort terms, matching
jsonata-js's pushAncestry handling in processAST's case '[' / case '^'."
```

---

### Task 7: Close the tuple-wrapper output leak

Adds the unwrap-at-boundary fix identified in the design's Section 3.

**Files:**
- Modify: `src/evaluator.rs` (top of `pub fn evaluate`, ~line 2709)
- Modify: `src/lib.rs` (`json_to_python`, ~line 436, or wherever the Python-boundary serializer lives — confirm exact location via `grep -n "fn json_to_python" src/lib.rs`)
- Test: new unit test in `src/evaluator.rs`

**Interfaces:**
- Consumes: the `__tuple__`/`@` convention already established.
- Produces: no new public interface — this is a correctness fix at existing boundaries.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn test_bare_index_bind_result_does_not_leak_tuple_wrapper() {
    let data: JValue = serde_json::json!({"items": [1, 2, 3]}).into();
    let ast = crate::parser::parse("items#$i").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();
    // Each element should be the plain value, not {"@":1,"$i":0,"__tuple__":true}
    match result {
        JValue::Array(arr) => {
            for item in arr.iter() {
                assert!(
                    !matches!(item, JValue::Object(obj) if obj.get("__tuple__").is_some()),
                    "tuple wrapper leaked into output: {:?}",
                    item
                );
            }
        }
        other => panic!("expected array, got {:?}", other),
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_bare_index_bind_result_does_not_leak_tuple_wrapper`
Expected: FAIL (this is the exact latent bug the investigation found for `#` — confirm it actually reproduces before fixing)

- [ ] **Step 3: Add the unwrap helper and call it at both boundaries**

In `src/evaluator.rs`, add near `evaluate`:

```rust
/// Strip a lingering tuple wrapper from a value about to leave the
/// evaluator (either a single tuple object, or an array of them),
/// recursively -- this is the single choke point that was missing
/// (call sites previously unwrapped locally and inconsistently).
fn unwrap_tuple_output(value: JValue) -> JValue {
    match value {
        JValue::Object(obj) if obj.get("__tuple__") == Some(&JValue::Bool(true)) => obj
            .get("@")
            .cloned()
            .map(unwrap_tuple_output)
            .unwrap_or(JValue::Undefined),
        JValue::Array(arr) => {
            JValue::array(arr.iter().cloned().map(unwrap_tuple_output).collect())
        }
        other => other,
    }
}
```

In `pub fn evaluate`, wrap the final return value:

```rust
    pub fn evaluate(&mut self, node: &AstNode, data: &JValue) -> Result<JValue, EvaluatorError> {
        // ... existing body ...
        result.map(unwrap_tuple_output)
    }
```

(Read the current exact body/return shape of `evaluate` at line 2709 first — the exact edit depends on whether it currently returns directly or via an intermediate `let result = ...; result` binding.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test test_bare_index_bind_result_does_not_leak_tuple_wrapper`
Expected: PASS

- [ ] **Step 5: Run the full test suite, `cargo fmt`/`clippy`, commit**

```bash
cargo test
cargo fmt && cargo clippy --all-targets --all-features -- -D warnings
git add src/evaluator.rs
git commit -m "fix(evaluator): unwrap lingering tuple wrapper before returning to caller

Closes a latent bug (pre-existing for #, newly reachable for @/%): a bare
tuple-producing expression at the top level previously leaked
{\"@\":...,\"__tuple__\":true} wrapper objects into user-visible output."
```

---

### Task 8: Test suite integration and xfail removal

**Files:**
- Create: `tests/parent_and_focus_binding_suite.rs` (sibling to `tests/datetime_picture_suite.rs`)
- Modify: `tests/python/test_reference_suite.py` (remove the 65 `parent-operator`/`joins` xfail entries and the two `_XFAIL_PHASE_BY_GROUP` keys)
- Modify: `README.md`, `docs/index.md` (pass-count claims)

**Interfaces:**
- Consumes: everything from Tasks 1-7.
- Produces: the final green reference-suite state.

- [ ] **Step 1: Create the cargo integration test harness**

Copy the structure of `tests/datetime_picture_suite.rs` (its `run_case`/`error_message`/`run_group_file` helpers are generic enough to reuse verbatim — they dispatch on `result`/`undefinedResult`/`code` keys in a test spec, not on anything datetime-specific). Create `tests/parent_and_focus_binding_suite.rs`:

```rust
// Fast-iteration mirror of the parent-operator/joins reference-suite cases
// for the %/@ operators, mirroring tests/datetime_picture_suite.rs's
// structure (see that file for the run_case/run_group_file helpers this
// duplicates -- kept as a separate file since this isn't datetime-related).

use jsonata_core::{
    evaluator::{Evaluator, EvaluatorError},
    parser::parse,
    value::JValue,
};
use serde_json::Value as JsonValue;
use std::fs;
use std::path::Path;

fn error_message(e: &EvaluatorError) -> &str {
    match e {
        EvaluatorError::TypeError(m) => m,
        EvaluatorError::ReferenceError(m) => m,
        EvaluatorError::EvaluationError(m) => m,
    }
}

fn resolve_expr(case: &JsonValue, group_dir: &Path) -> Option<String> {
    if let Some(expr) = case.get("expr").and_then(|e| e.as_str()) {
        return Some(expr.to_string());
    }
    let expr_file = case.get("expr-file").and_then(|e| e.as_str())?;
    fs::read_to_string(group_dir.join(expr_file)).ok()
}

fn resolve_data(case: &JsonValue, dataset_dir: &Path) -> JsonValue {
    if let Some(data) = case.get("data") {
        return data.clone();
    }
    if let Some(dataset) = case.get("dataset").and_then(|d| d.as_str()) {
        let path = dataset_dir.join(format!("{dataset}.json"));
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(parsed) = serde_json::from_str(&content) {
                return parsed;
            }
        }
    }
    JsonValue::Null
}

fn run_case(case: &JsonValue, group_dir: &Path, dataset_dir: &Path) -> Result<(), String> {
    let expr = resolve_expr(case, group_dir).ok_or("missing expr/expr-file")?;
    let data: JValue = resolve_data(case, dataset_dir).into();

    let ast = parse(&expr).map_err(|e| format!("parse error: {e}"))?;
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data);

    if let Some(code) = case.get("code").and_then(|c| c.as_str()) {
        return match &result {
            Err(e) => {
                let msg = error_message(e);
                if msg.starts_with(code) {
                    Ok(())
                } else {
                    Err(format!("expected code {code}, got error: {msg}"))
                }
            }
            Ok(v) => Err(format!("expected error {code}, got result {v:?}")),
        };
    }

    if case.get("undefinedResult").and_then(|b| b.as_bool()) == Some(true) {
        return match result {
            Ok(JValue::Undefined) | Ok(JValue::Null) => Ok(()),
            Ok(other) => Err(format!("expected undefined, got {other:?}")),
            Err(e) => Err(format!("expected undefined, got error: {e}")),
        };
    }

    if let Some(expected) = case.get("result") {
        return match result {
            Ok(v) => {
                let actual = serde_json::to_value(&v)
                    .map_err(|e| format!("failed to serialize result: {e}"))?;
                if &actual == expected {
                    Ok(())
                } else {
                    Err(format!("expected {expected}, got {actual}"))
                }
            }
            Err(e) => Err(format!("expected result {expected}, got error: {e}")),
        };
    }

    Err("test spec has no expected outcome (result, undefinedResult, or code)".to_string())
}

fn run_group_file(path: &Path, group_dir: &Path, dataset_dir: &Path) -> (usize, Vec<String>) {
    let content = fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {path:?}: {e}"));
    let json: JsonValue =
        serde_json::from_str(&content).unwrap_or_else(|e| panic!("parsing {path:?}: {e}"));
    let cases: Vec<&JsonValue> = match &json {
        JsonValue::Array(arr) => arr.iter().collect(),
        obj => vec![obj],
    };

    let mut failures = Vec::new();
    for (i, case) in cases.iter().enumerate() {
        if let Err(msg) = run_case(case, group_dir, dataset_dir) {
            let desc = case
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("");
            failures.push(format!(
                "{}[{i}] ({desc}): {msg}",
                path.file_stem().unwrap().to_string_lossy()
            ));
        }
    }
    (cases.len(), failures)
}

fn groups_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/jsonata-js/test/test-suite/groups")
}

fn dataset_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/jsonata-js/test/test-suite/datasets")
}

#[test]
fn parent_operator() {
    let group_dir = groups_root().join("parent-operator");
    let (total, failures) = run_group_file(&group_dir.join("parent.json"), &group_dir, &dataset_dir());
    assert!(
        failures.is_empty(),
        "{}/{} failed:\n{}",
        failures.len(),
        total,
        failures.join("\n")
    );
}

#[test]
fn joins() {
    let group_dir = groups_root().join("joins");
    let mut all_failures = Vec::new();
    let mut all_total = 0;
    for entry in fs::read_dir(&group_dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            let (total, failures) = run_group_file(&path, &group_dir, &dataset_dir());
            all_total += total;
            all_failures.extend(failures);
        }
    }
    assert!(
        all_failures.is_empty(),
        "{}/{} failed:\n{}",
        all_failures.len(),
        all_total,
        all_failures.join("\n")
    );
}
```

- [ ] **Step 2: Run and iterate**

Run: `cargo test --test parent_and_focus_binding_suite`
Expected: some failures initially. For each failure, read the failing expression, compare against jsonata-js's actual behavior (running it through the submodule's reference implementation if the expected JSON result is ambiguous), and fix the root cause in Tasks 1-7's code rather than special-casing the test. Repeat until both `parent_operator` and `joins` pass.

- [ ] **Step 3: Full verification**

```bash
cargo test
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
maturin develop --release
uv run pytest tests/python/test_reference_suite.py -q
```
Expected: cargo all green; pytest shows 65 fewer xfails than before this task started (confirm exact before/after counts via `grep -c '"parent-operator/\|"joins/'` against `tests/python/test_reference_suite.py`, same pattern used in Phases 1-2).

- [ ] **Step 4: Remove the xfail entries**

```bash
sed -i '/"parent-operator\/.*",\?$/d; /"joins\/.*",\?$/d' tests/python/test_reference_suite.py
```

Then manually remove the two now-empty `_XFAIL_PHASE_BY_GROUP` entries (`"parent-operator": ...` and `"joins": ...`) and update the module docstring's pass/xfail counts (same pattern as Phases 1-2's commits).

- [ ] **Step 5: Update README.md / docs/index.md**

Update the pass-count claims (search for the current numbers via `grep -n "1613\|69 xfail" README.md docs/index.md`) to reflect the new total, and remove `%`/`@`/`joins` from the "pending" gap list, leaving only Phase 5's stragglers.

- [ ] **Step 6: Final full-suite verification and commit**

```bash
uv run pytest tests/python/ -q
git add tests/parent_and_focus_binding_suite.rs tests/python/test_reference_suite.py README.md docs/index.md
git commit -m "test: add %/@ integration suite, remove parent-operator/joins xfails

65 xfail entries removed (28 parent-operator + 37 joins). Updates
README.md/docs/index.md pass-count claims. Only Phase 5's 4 untriaged
stragglers (array-constructor, function-distinct, flattening) remain."
```

---

## Self-Review Notes

- **Spec coverage:** Section 1 (AST/parser) → Tasks 1-2, 4 (IndexBind retirement). Section 2 (ancestor-resolution pass) → Tasks 3-4, 6. Section 3 (runtime unification) → Task 5, 7. Section 4 (error handling) → Tasks 1 (S0214), 3-4 (S0215-S0217 wiring, refined in Task 6). Section 5 (testing) → Task 8, plus unit tests embedded in every task.
- **Placeholder scan:** one intentional `todo!()` in Task 6 Step 2, explicitly justified (depends on a source re-read that hasn't happened yet) and explicitly required to be resolved before Step 3 — flagged rather than hidden, per the skill's guidance that "no placeholders" means no *vague* content, not that every fact must be pre-known before a large legacy-codebase integration begins.
- **Type consistency:** `AstNode::Parent` changes shape between Task 1 (unit variant) and Task 4 (carries `String`) — Task 4 Step 2 explicitly calls out updating Task 1's tests for this. `PathStep.ancestor_label`/`focus`/`index_var` names are consistent from Task 2 through Task 7. `evaluate_tuple_path_step`'s signature (Task 5) is used consistently in Task 5 alone (no other task calls it directly).

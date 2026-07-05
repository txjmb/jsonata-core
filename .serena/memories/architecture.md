## Evaluator/compiler architecture (non-obvious internals)

### JValue (src/value.rs)
```rust
pub enum JValue {
    Null, Bool(bool), Number(f64),
    String(Rc<str>), Array(Rc<Vec<JValue>>), Object(Rc<IndexMap<String, JValue>>),
    Undefined, Lambda { lambda_id, params, name, signature },
    Builtin { name }, Regex { pattern, flags },
    #[cfg(feature = "python")] LazyPyDict(Rc<lazy::LazyPyDict>),
}
```
- O(1) clone for every variant (Rc bump for heap types).
- Constructors: `JValue::string(..)`, `::array(..)`, `::lambda(..)`, `::builtin(..)`.
- Gotchas: `Rc<str>` doesn't satisfy `Borrow<String>` (use `&**s` for HashMap lookups);
  `Rc<Vec>` can't `.into_iter()` (use `.iter()` or `.to_vec()`/`Rc::make_mut()` to mutate).

### Null vs Undefined
JSONata distinguishes explicit `null` from "not present" `Undefined`; the tree-walker
historically produced `Null` for almost all "field not found" cases (a pre-existing bug fixed by
migrating ~20 hand-written call sites in evaluator.rs — fast paths, multi-step loops,
out-of-range array index, `apply_stages`, etc. — plus ~20 more downstream sites that assumed
"Null means missing": `signature.rs::validate_and_coerce`, builtin arg guards, arithmetic
explicit-null branches, `??`, `$not`/`$string`/`$append`/`$spread`). Reference pattern for
correct "present vs missing" semantics: `compiled_field_step` in evaluator.rs — Object: field-or-
Undefined; Array: map+flatten+skip-Undefined; other: Undefined. Exception: `AstNode::Variable`
(unbound `$var`) deliberately still returns `Null`, not `Undefined` — only path/field "not found"
semantics were migrated, not unbound-variable semantics. If tree-walker code decides
present-vs-missing and a value isn't propagating as expected, suspect a leftover pre-migration
site (grep `unwrap_or(JValue::Null)` / `JValue::Null =>` near field-access code).

### Lambda representation
- Lambdas live in the scope stack's `lambdas: HashMap<String, StoredLambda>`; `JValue::Lambda`
  only carries a `lambda_id` (+ params/name/signature) referencing that map.
- `lambda_id` MUST come from `Evaluator`'s monotonic `next_lambda_id`/`fresh_lambda_id()` counter,
  never a pointer-derived id (`format!("{:p}", ast_node)`) — an AST node's address is fixed per
  source site but each evaluation of a recursive/repeated lambda creates a new closure instance;
  pointer-derived ids alias distinct instances in the scope map, causing ~0.5%-frequency
  intermittent wrong-closure bugs (spurious T2010 / U1001 depth-limit errors). This was a real,
  hard-to-reproduce bug (issue #35) — reproducible only in a tight pure-Rust loop with a fresh
  `Evaluator` per iteration.
- `collect_lambda_ids()` transitively follows a `StoredLambda`'s captured_env.
- `:=` with a lambda RHS stores only in the lambdas map, not in regular bindings.

### Compilation pipeline (bytecode fast path)
`evaluator::try_compile_expr(ast)` → `CompiledExpr` (IR) → `compiler::BytecodeCompiler::compile`
→ `BytecodeProgram` (flat `Vec<Instr>`) → `peephole()` folds (`PushData+GetField` →
`GetDataField`, `GetVar+GetField` → `GetVarField`, elides `Not+Not`) → `vm::Vm::new(bc).run(...)`.
`JsonataExpression` (lib.rs) caches `bytecode: OnceCell<Option<BytecodeProgram>>`; all 4 evaluate
entrypoints try the VM first and fall back to the tree-walker.

Key correctness rules baked into the compiler/VM (violate these and specific test-suite groups
regress):
- And/Or compile to jump-based short-circuit code, not a binary instruction — required for
  exprs like `foo = '' or $number(foo) = 0` where evaluating rhs unconditionally would be wrong.
- Arithmetic/comparison ops carry compile-time `ExplicitNull` flags (`Add(lhs_en, rhs_en)`,
  `CmpLt(...)`) for correct T2002/T2010 errors.
- `get_field_cached` flattens one level of nested arrays when mapping (matches `compiled_field_step`)
  — required for `flattening/case001`.
- `MakeArray` skips undefined and flattens one level; but a "complex" `ArrayConstruct` (elements
  marked `is_nested=true`) must NOT flatten and falls back to `EvalFallback`.
- `FieldPath` with filter predicates always falls back to `eval_compiled` (tree-walker) — filters
  aren't compiled.
- Numeric predicates like `[0]`, `[1]` in a path are INDEX access, not boolean filters — must
  fall back to the tree-walker in `try_compile_path`.
- Peephole 2-instruction folds must remap BOTH consumed instructions' jump targets to the same
  new address, or jumps land mid-fold.

### Lambda compilation (Phase 3)
`StoredLambda.compiled_body: Option<CompiledExpr>`, compiled at definition time via
`try_compile_expr_with_allowed_vars`. Fast path in `invoke_stored_lambda` skips scope push/pop
when: body is compiled AND no signature AND not a thunk AND no captured functions. HOF fast
paths (`$map`/`$filter`/`$reduce`) handle both inline lambdas and stored-lambda variables.
`compiled_body` is deliberately set to `None` (forcing tree-walker) for: transform body,
ChainPipe composition, TCO thunks, partial-application body markers — these are known-uncompiled
sites, not bugs.

### LazyPyDict (Phase 5, Python-only perf path)
`JValue::LazyPyDict(Rc<LazyPyDict>)` (src/lazy.rs) lets field access on a Python dict skip full
materialization into `JValue::Object`; `to_object()` must skip `Undefined` entries when
materializing (absent fields must not appear as explicit nulls). Every code path that switches
on `JValue` object/array shape (sort keys, `$keys`/`$spread`/`$lookup`/`$merge`/`$sort`,
`$each`/`$sift`, the `@` tuple binding, `apply_transform_deep`) needs an explicit `LazyPyDict` arm
or a materialize-first step — it's easy to add a new object-shaped builtin and forget this.
Benefits sparse field-access patterns (2–5x speedup measured); dense full-object access can
slightly regress vs. eager materialization — don't assume it's a strict win everywhere.

# Host-callable custom functions

## Context

jsonata-core today has **no way for a host application to register its own function** that a
JSONata expression can call. The `bindings` argument (Rust `Context::bind`, the Python
`bindings=` dict, the C ABI) is converted to JSON *data* eagerly — a callable value passed
in is dropped, not retained as a callable. So an expression can only call:

1. built-in functions (`$sum`, `$map`, …), dispatched by name, and
2. user-defined functions written **in the JSONata language itself** (`function($x){…}`),
   which are pure and evaluated entirely inside the engine.

This is the gap versus jsonata-js's `registerFunction`/`assign`. It is invisible in CI
because the portable reference suite (`tests/jsonata-js/test/test-suite/groups`) can only
express pure-language cases — a host closure cannot live in a JSON fixture — so nothing in
the ported suite exercises or requires host functions. Conformance (language) and
extensibility (host callbacks) are orthogonal axes; we pass the first and don't implement
the second.

"Custom functions" is two different features and only the language one is tested:

- **Language user-functions** (`function(){}`, and HOF builtins like `$map`/`$filter`/
  `$reduce` that take a lambda arg) — implemented, `JValue::Lambda` / `StoredLambda`.
- **Host extension functions** (the app injecting native Rust/Python/C code) — this spec.

### Why this and not async

The recurring question is whether host functions require making the engine async. They do
not. The motivating case is I/O inside a host function (`$fxRate`, `$productName`,
`$riskScore`). With a synchronous core the call stack is simply:

```
caller ─▶ evaluate() ─▶ host_fn(args) ─▶ [blocking I/O] ─▶ value ─▶ … ─▶ result
```

One thread, blocked top-to-bottom while the I/O is in flight. That is *correct*, not a bug.
Concurrency is obtained by running N such stacks on N threads (a thread/process pool in
Python, native threads in Rust). This matches the guidance already in
`docs/migration-from-js.md` ("replace Promise patterns with threading").

Async is only needed to avoid *occupying a thread while waiting*, which matters in exactly
one deployment shape: an async-native host (e.g. tokio) doing async I/O inside host
functions **at high concurrency**, where one blocked thread per in-flight evaluation is too
many. Even that shape is served *without* touching the core: wrap the whole synchronous
evaluation in `tokio::task::spawn_blocking` (or the Python event-loop equivalent) and
`.await` the join handle — the host stays async, jsonata-core stays sync, one blocked
pool thread per concurrent eval. Only at extreme concurrency (thousands of simultaneous
I/O-bound evals) would true async-inside-the-core win, and buying that means the
`Rc → Arc` / `Send + Sync` / boxed-recursion teardown the core has so far avoided. Verdict:
**sync core + blocking host functions; document the `spawn_blocking` pattern; revisit true
async only if a concrete extreme-concurrency deployment proves it, and prefer an async
wrapper over a core rewrite even then.**

This is the reason jsonata-js is async at all: JavaScript has no threads and no blocking
I/O, so an event loop is its *only* concurrency primitive — async was forced by the host
language, not by JSONata. Rust and Python have threads, so we do not inherit the constraint.

### What other Rust implementations do

- **`jsonata-rs` (Stedi)** — alpha, "all interfaces unstable." Exposes `register_function`
  and is **synchronous**; no async `evaluate` variant.
- **`jsonata`** — incomplete ("panics in unexpected places"), synchronous.
- **`async_jsonata_rust` (LajaSoft)** — the async-end-to-end project. `evaluate_async`
  returns a future; runtime-agnostic (tokio / async-std / any executor); **also ships a
  sync `evaluate` facade**. Custom functions are registered via a `FunctionRegistry` and
  **may themselves be async** ("awaited cooperatively"). Mature: 1651/1653 on the official
  suite. Streaming is **not** mentioned in its docs (despite the feature sometimes being
  attributed to it).

Takeaway for our decision: `async_jsonata_rust` confirms — rather than contradicts — the
sync-core plan. Its single reason to be async end-to-end is exactly the case analyzed above
(user functions that do I/O and are awaited); it pays for that with an evaluator where every
node is a polled (boxed) future — overhead the pure-CPU common case eats on every
evaluation. That is antithetical to jsonata-core's identity (SIMD-accelerated parse,
bytecode VM, benchmark focus, PyO3/C bindings). The async niche is therefore already
occupied and served well; jsonata-core's differentiation is **the fastest *sync* engine with
real language bindings**. Adding *sync* host functions closes the extensibility gap without
leaving that lane. A user who genuinely needs awaited async host functions at high
concurrency already has `async_jsonata_rust` — we neither need to duplicate it nor should
pay its per-node future cost to do so. (That even the async-first project keeps a sync
`evaluate` facade underscores that sync is the common path.)

## The reuse seam (confirmed against source)

Host functions are the **builtin analog, not the lambda analog**: they are name-dispatched
callables that receive already-evaluated arguments. That is exactly how `apply_function`
resolves a `$foo(...)` call today (`src/evaluator.rs:9660`, the `AstNode::Variable` arm):

```
lookup_lambda(name)      → StoredLambda        → invoke_stored_lambda
else lookup(name)        → lambda-valued JValue → invoke_stored_lambda
else is_builtin_function → call_builtin_with_values(name, values)   ← host fns slot here
else                     → unknown
```

`call_builtin_with_values(name, &[JValue])` already receives the **evaluated** argument
slice — the precise shape of a leaf host callback `Fn(&[JValue]) -> Result<JValue, _>`. So
the integration is: add a host-function registry and consult it in the same
`is_builtin_function` / `call_builtin_with_values` seam (and its compiled-`BuiltinCall`
equivalent at `src/evaluator.rs:1211`). No new `JValue` variant is required for the leaf
case; the registry is keyed by name exactly like the builtin table.

`Context` (`src/evaluator.rs:2759`) already keeps a `scope_stack` of `Scope { bindings,
lambdas }`. The host registry lives one level up on the evaluator (it is process/config
state, not per-scope), or on `Context` alongside the scopes; either way it is consulted
only at the unresolved-name step, so it cannot shadow user `:=` bindings or builtins unless
we choose the precedence (see Decisions).

### Reentrancy is a phase-2 widening, not a redesign

The evaluated `values: &[JValue]` handed to a name-dispatched call **can already contain a
lambda value** — that is how `$map(arr, function($x){…})` works: the builtin invokes the
lambda via `lookup_lambda_from_value` + `invoke_stored_lambda`. So the machinery for a
host function to invoke a passed-in lambda already exists in the engine; what a v1 leaf
callback lacks is a *handle* to reach it. jsonata-js supports exactly this (it wraps every
function-typed argument in a callable closure before handing it to a native function —
`apply(arg, params, null, environment)`), enabling host-defined higher-order functions;
it does **not** expose a public API for arbitrary re-evaluation (issue #259).

We match that split by choosing a **forward-compatible callback type now** so re-entry is
an additive change later:

```rust
pub trait HostFn {
    fn call(&self, args: &[JValue], ctx: &mut HostCtx) -> Result<JValue, EvaluatorError>;
}

// Ergonomic blanket impl: leaf closures never touch ctx.
impl<F> HostFn for F where F: Fn(&[JValue]) -> Result<JValue, EvaluatorError> { … }
```

- **v1:** `HostCtx` is opaque (maybe read-only options). Simple closures use the blanket
  `Fn` impl and ignore it entirely.
- **phase 2:** add `HostCtx::apply(&mut self, f: &JValue, args: &[JValue]) -> Result<JValue,
  _>` that routes back through `apply_function`/`invoke_stored_lambda`. Because re-entry
  goes through the existing guarded path, the recursion/stack-depth guards (`stacker`,
  `max_stack_depth`, `D1011`) are preserved automatically — a re-entering host fn cannot
  bypass them. No signature change for existing callers.

## Python surface specifics

Extending to Python (Phase 2) is confirmed intent, not new scope, but Python — unlike the
Rust-only competitor — has `asyncio`, so it is where the async temptation actually has
teeth. It does not change the sync-core decision; it adds three realities to bake in.

- **The GIL matches the sync core, and there is precedent.** The core is `Rc`
  single-threaded; Python is GIL single-threaded — same shape. The evaluator *already*
  re-enters Python mid-evaluation with the GIL held, to materialize lazy dict views
  (`JValue::LazyPyDict`, `src/lazy.rs`, spec `2026-07-12-lazy-python-views-design.md`). A
  Python host function is the same move, so the GIL-handling patterns already exist. This is
  a point in favor.
- **`async def` is the wrinkle.** Python devs reach for `async def` for I/O; the sync core
  **cannot await** a returned coroutine. v1 **rejects a coroutine return with a clear
  error**. The documented pattern for async-Python users is *do the async I/O outside
  jsonata* — `await` lookups in the host, pass results in as `bindings`, then run the sync
  transform — or run the sync `evaluate()` in `loop.run_in_executor(...)` with sync-blocking
  host fns inside (the Python `spawn_blocking`). Making the Python `evaluate` truly async
  would require the sync core to suspend at a host-fn call, i.e. making the core async (the
  jsonata-js generator/trampoline path) — rejected: a Python-only benefit for the full core
  teardown. Note there is **no async-Python JSONata to punt to** (`async_jsonata_rust` is
  Rust), but the gap is narrow (next point) and is closed with a pattern, not a rewrite.
- **The GIL caps the async payoff anyway.** A blocking Python host fn holds the GIL and
  stalls all Python threads, so "threads for concurrency" is weaker in Python than Rust —
  but async would not rescue CPU-bound work either (GIL serializes it). The slice where
  async-Python host fns would truly win (thousands of concurrent I/O-bound `async def`
  callbacks) is narrow; gather-then-bind covers most of it.
- **Boundary cost → coarse-grained only.** Each call crosses Rust↔Python (marshal args →
  call → marshal result → GIL). Cheap once, expensive 100k times. Python host fns are for a
  handful of enrichment lookups, **not** per-element callbacks inside a `$map` over a large
  array. Document this; the existing lazy-view work shows the codebase already minimizes
  Py-boundary crossings.

## Scope

In scope (v1):
- A host-function registry consulted in the `apply_function` name-resolution seam and the
  compiled `BuiltinCall` path.
- The `HostFn` trait + `Fn(&[JValue]) -> Result<JValue, EvaluatorError>` blanket impl,
  typed so phase-2 reentrancy is additive.
- **Rust crate API**: `register_fn(name, closure)` — the reference implementation.
- **Python (PyO3) API**: register a Python callable; marshal args via the existing
  `json_to_python` / `python_to_json` (`src/lib.rs`); map a Python exception →
  `EvaluatorError`; detect an `async def` (coroutine return) and reject with a clear error.
- Bespoke unit tests per binding (nothing in the portable suite covers this).

Out of scope (later phases / explicitly not v1):
- **Reentrancy / host-defined higher-order functions** — phase 2; the callback type is
  chosen now so this does not break API.
- **C ABI registration** (function pointer + `void* userdata`) — phase 3.
- **Signature strings** for host fns (reusing `signature.rs`) — optional; v1 host fns
  validate their own arity/types.
- **VM support** — v1 forces the tree-walker fallback for any expression containing a
  host-fn call (the engine already falls back for some lambda cases); teach the bytecode
  compiler later only if profiling warrants.
- **Async anything** — see "Why this and not async."

## Phasing & estimate

- **Phase 1 — core registry + dispatch + Rust API (usable MVP): IMPLEMENTED.**
  `Evaluator::register_fn` / `register_fn_override`, a `HostFn` trait with an
  `Fn(&[JValue]) -> Result<JValue, EvaluatorError>` blanket impl, and an opaque `HostCtx`
  reserved for phase-2 reentrancy (`src/evaluator.rs`). Dispatch is hooked in
  `evaluate_function_call` (direct `$name(...)` calls, resolving after the expression's own
  bindings/lambdas and before built-ins) and defensively in `call_builtin_with_values`
  (so a host *override* also wins in value position, e.g. `$f := $now; $f()`). Tests:
  `tests/host_functions_test.rs` (12 cases). Notes from implementation:
  - **Tree-walker fallback is automatic** for novel host-fn names: `try_compile_hof_expr`
    already returns `None` for any non-`map`/`filter`/`reduce` name, so every compile site
    bails to the tree-walker with no compiler change.
  - **Overriding a *compilable* built-in is rejected at registration** (the bytecode path
    can't see the registry). The impure targets that motivate overriding — `$now`,
    `$millis`, `$random`, `$eval` — are all non-compilable, so this never blocks a
    legitimate use.
  - **Deferred from v1:** host-fn-as-first-class-value (`$map(arr, $greet)`,
    `$f := $greet`) — pairs with reentrancy in a later phase; direct calls cover every
    practical use case enumerated in this design.
- **Phase 2 — Python (PyO3) binding:** ~1–2 days. Highest-value surface here; marshalling
  helpers already exist. → ~1 week for Rust + Python end-to-end.
- **Phase 3 — C ABI + signatures + docs polish:** ~1 week more. → ~2 weeks fully polished.
- **Reentrancy** is folded into whichever later phase needs host-defined HOFs; not on the
  critical path.

## Decisions

- **Host fns are the builtin analog** (name-dispatched, pre-evaluated args), not lambdas
  bound into a scope. Registry keyed by name, consulted at the unresolved-name step.
- **Precedence & shadowing.** User `:=` bindings and language lambdas win over host fns
  (avoid surprising capture). For builtins, **do not** silently resolve host fns "after
  builtins" — that turns a registered `$now` into a silent no-op. Instead: **a host name
  colliding with a builtin is a registration-time error by default**, with an explicit
  opt-in override API (`register_fn_override(name, …)` / `.allow_override()`). Rationale:
  the only legitimate shadow cases cluster into two deliberate, small-set categories, and
  both want a loud, self-documenting opt-in rather than default-allow (which is a
  portability landmine) or a blanket ban (which forecloses real needs):
  - **(A) Determinism injection** for impure builtins that exist here — `$now`
    (`evaluator.rs:9542`), `$millis` (`:9551`), `$random` (`:10797`): frozen clock / seeded
    RNG for reproducible tests, golden files, replay, or event/logical time in prod.
  - **(B) Sandboxing/hardening** — override `$eval` (`evaluator.rs:9479`, dynamic
    string-eval, the riskiest builtin) to disable it, or wrap a builtin to add
    auditing/limits when running semi-trusted expressions.
  Overriding *pure* builtins (`$sum`, `$map`, string/array ops) has no legitimate case and
  should use a differently-named function instead. This default is safer than jsonata-js,
  where a user binding can silently shadow a builtin.
- **Sync, blocking host fns.** Blocking is the intended model; threads provide concurrency;
  `spawn_blocking` bridges async hosts without touching the core.
- **Forward-compatible `HostFn` trait now**, ergonomic `Fn` blanket impl, so phase-2
  reentrancy adds `HostCtx::apply` without an API break.
- **Tree-walker only in v1**; host-fn presence forces fallback.

## Open questions

- Precedence vs. builtins: **resolved** — collision is an error unless the explicit override
  API is used (see Decisions). Remaining detail: override granularity (per-call
  `register_fn_override` vs. a builder-wide `.allow_override()` flag).
- Python: reject `async def` outright, or accept and run it to completion on a private loop?
  (Lean reject in v1 — clearer, no hidden event loop.)
- Whether to expose read-only `options`/context to leaf host fns in v1 (`HostCtx` non-opaque
  from day one) or keep it fully opaque until phase 2.

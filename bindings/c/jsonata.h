/*
 * jsonata.h — C API for jsonata-core, a high-performance Rust implementation
 * of the JSONata query and transformation language (https://jsonata.org).
 *
 * Implemented by src/capi.rs (build with `cargo build --release --features
 * capi`, link against the produced libjsonata_core.{so,dylib} /
 * jsonata_core.dll). This header is hand-maintained; CI compiles
 * bindings/c/examples/smoke.c against it to keep the two in sync.
 *
 * ── Contract ────────────────────────────────────────────────────────────
 *
 * Strings.  All strings crossing the boundary are UTF-8, NUL-terminated.
 * JSON documents and results cross as JSON text; there is no C-visible
 * structured value model. Every char* RETURNED by this library is owned by
 * the caller and must be released with jsonata_free_string() — except
 * jsonata_version(), which returns a static string that must never be
 * freed.
 *
 * Errors.  Failures are reported through a thread-local "last error" slot:
 *   - jsonata_compile() returns NULL and sets the slot on failure.
 *   - jsonata_evaluate() returns NULL in TWO cases, disambiguated by the
 *     slot: if jsonata_last_error_message() returns non-NULL it was an
 *     error; if it returns NULL the JSONata result was *undefined* (a
 *     legitimate result — e.g. a path that matched nothing).
 *   - jsonata_bind_var() returns 0 on success, -1 on failure (slot set).
 * Successful calls clear the slot. jsonata_last_error_code() additionally
 * extracts the JSONata specification error code ("T2002", "S0201",
 * "D3030", ...) when the failure has one; errors without a spec code
 * (e.g. invalid input JSON) yield NULL. Error source positions are not
 * available — the engine does not currently track them.
 *
 * Threading.  A JsonataExpr* must be created, used, and freed on a single
 * thread (the engine's value model is reference-counted without atomics).
 * Different threads may each use their own handles concurrently; the error
 * slot is thread-local, so per-thread error reporting never races.
 *
 * Panics.  Internal engine panics are caught at this boundary and reported
 * as errors (message prefixed "internal error:"); they neither unwind into
 * the caller nor abort the host process.
 */

#ifndef JSONATA_CORE_H
#define JSONATA_CORE_H

#ifdef __cplusplus
extern "C" {
#endif

/* An opaque compiled-expression handle. Not thread-safe; see header notes. */
typedef struct JsonataExpr JsonataExpr;

/*
 * Compile a JSONata expression (UTF-8).
 * Returns a handle, or NULL on failure (check jsonata_last_error_message).
 * Free with jsonata_free_expr().
 */
JsonataExpr *jsonata_compile(const char *expr_utf8);

/*
 * Evaluate the compiled expression against a JSON document (UTF-8 JSON
 * text). Returns the result as JSON text (caller frees with
 * jsonata_free_string), or NULL — meaning *undefined* if
 * jsonata_last_error_message() is NULL, otherwise an error.
 * The handle may be reused for any number of evaluations; the expression
 * is compiled to bytecode lazily on first use.
 */
char *jsonata_evaluate(JsonataExpr *expr, const char *json_utf8);

/*
 * Bind a variable ($name) on the expression from a JSON value (UTF-8 JSON
 * text). Applies to every subsequent jsonata_evaluate() on this handle.
 * A leading '$' in name is accepted and stripped; re-binding an existing
 * name replaces its value. Returns 0 on success, -1 on error (slot set).
 */
int jsonata_bind_var(JsonataExpr *expr, const char *name,
                     const char *json_value_utf8);

/* Free a handle returned by jsonata_compile(). NULL is a no-op. */
void jsonata_free_expr(JsonataExpr *expr);

/* Free a string returned by this library. NULL is a no-op. */
void jsonata_free_string(char *s);

/*
 * The last error message on this thread, as a fresh copy (caller frees
 * with jsonata_free_string), or NULL if the last call succeeded.
 */
char *jsonata_last_error_message(void);

/*
 * The JSONata spec code of the last error on this thread ("T2002", ...),
 * as a fresh copy (caller frees with jsonata_free_string), or NULL when
 * there is no error or the error carries no spec code.
 */
char *jsonata_last_error_code(void);

/* The jsonata-core version, e.g. "2.2.4". Static string — do NOT free. */
const char *jsonata_version(void);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* JSONATA_CORE_H */

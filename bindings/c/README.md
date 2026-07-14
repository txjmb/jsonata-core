# jsonata-core C API

Use [JSONata](https://jsonata.org) — the JSON query and transformation
language — from C, C++, or any language with C interop, backed by
[jsonata-core](https://github.com/txjmb/jsonata-core), a high-performance
Rust implementation with 100% reference-suite compatibility against
jsonata-js.

The API is deliberately small: JSON text in, JSON text out, eight
functions. See [`jsonata.h`](jsonata.h) for the authoritative contract.

```c
#include "jsonata.h"
#include <stdio.h>

int main(void) {
    JsonataExpr *expr = jsonata_compile("$sum(order.items.(price * qty))");
    if (!expr) {
        char *err = jsonata_last_error_message();
        fprintf(stderr, "compile failed: %s\n", err);
        jsonata_free_string(err);
        return 1;
    }

    char *result = jsonata_evaluate(expr,
        "{\"order\":{\"items\":[{\"price\":9.99,\"qty\":2},{\"price\":5,\"qty\":1}]}}");
    if (result) {
        printf("total: %s\n", result);   /* total: 24.98 */
        jsonata_free_string(result);
    } else {
        char *err = jsonata_last_error_message();
        if (err) {                        /* error */
            fprintf(stderr, "evaluate failed: %s\n", err);
            jsonata_free_string(err);
        } else {                          /* JSONata "undefined" — no match */
            printf("no result\n");
        }
    }

    jsonata_free_expr(expr);
    return 0;
}
```

## Building the shared library

Requires a Rust toolchain (rustc 1.70+, [rustup.rs](https://rustup.rs)):

```sh
git clone https://github.com/txjmb/jsonata-core
cd jsonata-core
cargo build --release --features capi
```

This produces the shared library in `target/release/`:

| Platform | Library |
|---|---|
| Linux | `libjsonata_core.so` |
| macOS | `libjsonata_core.dylib` |
| Windows | `jsonata_core.dll` (+ `jsonata_core.dll.lib` import lib) |

The only header you need is `bindings/c/jsonata.h`.

## Compiling and linking

**C** (gcc/clang):

```sh
gcc -Wall -o myapp myapp.c -I/path/to/jsonata-core/bindings/c \
    -L/path/to/jsonata-core/target/release -ljsonata_core
LD_LIBRARY_PATH=/path/to/jsonata-core/target/release ./myapp
```

**C++**: the header is C++-safe (`extern "C"` guarded); compile and link
exactly as above with `g++`/`clang++`.

For deployment, install the library somewhere on the loader path (or set
`rpath`): `-Wl,-rpath,/opt/myapp/lib` at link time avoids
`LD_LIBRARY_PATH` at run time.

**Makefile snippet:**

```make
JSONATA_DIR := /path/to/jsonata-core
CFLAGS  += -I$(JSONATA_DIR)/bindings/c
LDFLAGS += -L$(JSONATA_DIR)/target/release -Wl,-rpath,$(JSONATA_DIR)/target/release
LDLIBS  += -ljsonata_core

myapp: myapp.o
	$(CC) $(LDFLAGS) -o $@ $^ $(LDLIBS)
```

**CMake snippet:**

```cmake
set(JSONATA_DIR /path/to/jsonata-core)

add_library(jsonata_core SHARED IMPORTED)
set_target_properties(jsonata_core PROPERTIES
    IMPORTED_LOCATION ${JSONATA_DIR}/target/release/libjsonata_core.so
    INTERFACE_INCLUDE_DIRECTORIES ${JSONATA_DIR}/bindings/c)

target_link_libraries(myapp PRIVATE jsonata_core)
```

(Optionally wrap the `cargo build` in a CMake `add_custom_command` so the
Rust library rebuilds with your project.)

## API rules (the short version)

1. **Ownership:** every `char*` the library returns is yours — free it with
   `jsonata_free_string()`. The single exception is `jsonata_version()`
   (static; never free). Never free library strings with your own
   `free()` — allocators may differ.
2. **NULL from `jsonata_evaluate` is ambiguous by design:** check
   `jsonata_last_error_message()`. Non-NULL → error. NULL → the JSONata
   result was *undefined* (a normal outcome — e.g. a path that matched
   nothing). This mirrors JSONata semantics, where `undefined` and `null`
   are distinct.
3. **Error codes:** `jsonata_last_error_code()` returns the JSONata spec
   code (`"T2002"`, `"S0201"`, `"D3030"`, …) when the failure has one,
   NULL otherwise. Error source positions are not exposed (the engine
   does not currently track them).
4. **Threading:** one `JsonataExpr*` per thread — create, use, and free a
   handle on the same thread. Handles are cheap; compile per thread rather
   than sharing. Different threads with their own handles are fine, and
   the error slot is thread-local.
5. **Reuse handles:** `jsonata_compile` once, `jsonata_evaluate` many
   times — the expression is JIT-compiled to bytecode on first evaluation
   and reused. Compilation is the expensive step.
6. **Variables:** `jsonata_bind_var(expr, "x", "[1,2,3]")` binds `$x` for
   all subsequent evaluations on that handle (values are JSON text;
   re-binding replaces).
7. **Panics:** internal engine panics are caught at the boundary and
   surface as errors prefixed `internal error:` — they will not abort
   your process.

## Smoke test

[`examples/smoke.c`](examples/smoke.c) exercises the full API surface and
doubles as usage documentation. Build and run it from the repo root:

```sh
cargo build --release --features capi
gcc -Wall -Wextra -Werror -o smoke bindings/c/examples/smoke.c \
    -Ltarget/release -ljsonata_core
LD_LIBRARY_PATH=target/release ./smoke
```

CI compiles and runs it (as both C and C++) on every change to the C ABI.

## Related

- [jsonatapy](https://pypi.org/project/jsonatapy/) — the Python binding of
  this engine (PyPI, zero runtime dependencies).
- [`jsonata` CLI](../../docs/) — a jq-style command-line interface to the
  same engine.
- [jsonata-js](https://github.com/jsonata-js/jsonata) — the reference
  implementation this engine tracks (1682/1682 reference tests passing).

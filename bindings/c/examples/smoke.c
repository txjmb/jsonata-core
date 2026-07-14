/*
 * smoke.c — compile-and-run smoke test for the jsonata-core C ABI.
 *
 * Exercises every function in jsonata.h against the real library and exits
 * non-zero on the first failure. CI builds and runs this on every PR that
 * touches the C ABI; see bindings/c/README.md for the manual build/run
 * commands.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "../jsonata.h"

static int failures = 0;

#define CHECK(cond, name)                                                    \
    do {                                                                     \
        if (cond) {                                                          \
            printf("PASS  %s\n", name);                                      \
        } else {                                                             \
            printf("FAIL  %s (line %d)\n", name, __LINE__);                  \
            failures++;                                                      \
        }                                                                    \
    } while (0)

static char *take_error(void) { return jsonata_last_error_message(); }

int main(void) {
    /* version */
    const char *v = jsonata_version();
    CHECK(v != NULL && strlen(v) > 0, "version non-empty");

    /* simple path */
    {
        JsonataExpr *e = jsonata_compile("user.name");
        CHECK(e != NULL, "compile simple path");
        char *r = jsonata_evaluate(e, "{\"user\":{\"name\":\"Alice\"}}");
        CHECK(r != NULL && strcmp(r, "\"Alice\"") == 0, "evaluate simple path");
        jsonata_free_string(r);
        jsonata_free_expr(e);
    }

    /* object construction + arithmetic */
    {
        JsonataExpr *e = jsonata_compile("{\"n\": a + b}");
        char *r = jsonata_evaluate(e, "{\"a\":1,\"b\":2}");
        CHECK(r != NULL && strcmp(r, "{\"n\":3}") == 0, "object result");
        jsonata_free_string(r);
        jsonata_free_expr(e);
    }

    /* undefined result: NULL with EMPTY error slot */
    {
        JsonataExpr *e = jsonata_compile("missing.path");
        char *r = jsonata_evaluate(e, "{\"a\":1}");
        char *err = take_error();
        CHECK(r == NULL && err == NULL, "undefined -> NULL + empty error");
        jsonata_free_string(err);
        jsonata_free_expr(e);
    }

    /* parse error: NULL handle + message */
    {
        JsonataExpr *e = jsonata_compile("a.b[");
        char *err = take_error();
        CHECK(e == NULL && err != NULL && strlen(err) > 0,
              "parse error -> NULL + message");
        jsonata_free_string(err);
    }

    /* coded evaluation error: message + spec code */
    {
        JsonataExpr *e = jsonata_compile("$number(b)");
        char *r = jsonata_evaluate(e, "{\"b\":[1]}");
        char *err = take_error();
        char *code = jsonata_last_error_code();
        CHECK(r == NULL && err != NULL, "eval error -> NULL + message");
        CHECK(code != NULL && strcmp(code, "D3030") == 0,
              "eval error -> spec code D3030");
        jsonata_free_string(err);
        jsonata_free_string(code);
        jsonata_free_expr(e);
    }

    /* variable binding */
    {
        JsonataExpr *e = jsonata_compile("$sum($xs) + n");
        int rc = jsonata_bind_var(e, "$xs", "[1,2,3]");
        CHECK(rc == 0, "bind_var succeeds");
        char *r = jsonata_evaluate(e, "{\"n\":10}");
        CHECK(r != NULL && strcmp(r, "16") == 0, "evaluate with bound var");
        jsonata_free_string(r);
        jsonata_free_expr(e);
    }

    /* invalid input JSON: NULL + uncoded message */
    {
        JsonataExpr *e = jsonata_compile("a");
        char *r = jsonata_evaluate(e, "{not json");
        char *err = take_error();
        char *code = jsonata_last_error_code();
        CHECK(r == NULL && err != NULL && strstr(err, "invalid input JSON") != NULL,
              "invalid input JSON -> message");
        CHECK(code == NULL, "invalid input JSON -> no spec code");
        jsonata_free_string(err);
        jsonata_free_string(code);
        jsonata_free_expr(e);
    }

    /* multibyte UTF-8 round trip */
    {
        JsonataExpr *e = jsonata_compile("$uppercase(name)");
        char *r = jsonata_evaluate(e, "{\"name\":\"h\xC3\xA9llo \xE2\x9C\x93\"}");
        CHECK(r != NULL && strcmp(r, "\"H\xC3\x89LLO \xE2\x9C\x93\"") == 0,
              "multibyte UTF-8 round trip");
        jsonata_free_string(r);
        jsonata_free_expr(e);
    }

    /* NULL tolerance of free functions */
    jsonata_free_expr(NULL);
    jsonata_free_string(NULL);
    CHECK(1, "free(NULL) is a no-op");

    if (failures == 0) {
        printf("SMOKE OK (jsonata-core %s)\n", v);
        return 0;
    }
    printf("SMOKE FAILED: %d failure(s)\n", failures);
    return 1;
}

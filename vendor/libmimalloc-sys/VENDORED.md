# Vendored libmimalloc-sys

This is `libmimalloc-sys` 0.1.49 from crates.io (MIT, purpleprotocol/mimalloc_rust),
used through `[patch.crates-io]` in the root `Cargo.toml`. It is unchanged except:

- `c_src/mimalloc/v3` holds mimalloc **v3.4.5** (microsoft/mimalloc tag `v3.4.5`,
  commit `acf2fdd329f9dc2a7ffe3f12a133fe7175e39378`): only `include/`, `src/` and
  `LICENSE`, the files the build script compiles.
- `c_src/mimalloc/v2` is removed; this project never enables the `v2` feature,
  and `build.rs` panics with a pointer here if something does.

Why (GitHub #298): the crate's bundled mimalloc v3.3.2 keeps its thread heap in
fixed macOS TLS slots shared by every mimalloc v3 copy in the process, so it
crashed next to pyarrow 24.0.0, and on every platform it segfaults once the
first thread that allocated has exited (microsoft/mimalloc#1287). mimalloc
3.4.5 uses pthread keys on macOS by default and fixes the thread-exit crash.

Remove this directory and the `[patch.crates-io]` entry once a `libmimalloc-sys`
release bundles mimalloc >= 3.4.5 (purpleprotocol/mimalloc_rust#169), then
check that `tests/python/test_allocator_coexistence.py` still passes.

To check the C sources still match upstream:

```sh
git clone --depth 1 --branch v3.4.5 https://github.com/microsoft/mimalloc /tmp/mimalloc
for d in include src; do diff -r /tmp/mimalloc/$d vendor/libmimalloc-sys/c_src/mimalloc/v3/$d; done
diff /tmp/mimalloc/LICENSE vendor/libmimalloc-sys/c_src/mimalloc/v3/LICENSE
```

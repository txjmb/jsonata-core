//! Criterion benchmarks for the jsonatapy evaluator — pure Rust, no PyO3.
//!
//! Measures raw evaluation cost: no Python interpreter, no PyO3 boundary,
//! no JSON serialization. This is the performance ceiling for the library.
//!
//! Mirrors the cases in benchmarks/python/benchmark.py so timings are
//! directly comparable (multiply Criterion ns/iter by Python iteration
//! count to get equivalent total-ms figures).
//!
//! Run:
//!   cargo bench
//!   cargo bench -- simple_path        # one group
//!   cargo bench -- realistic_workload # one group

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use indexmap::IndexMap;
use jsonata_core::evaluator::Evaluator;
use jsonata_core::parser;
use jsonata_core::value::JValue;

#[cfg(feature = "bench")]
use jsonata_core::_bench;

// ── Data builders ─────────────────────────────────────────────────────────────

/// Tiny single-field object used by simple-path benchmarks.
fn tiny_obj(key: &str, val: JValue) -> JValue {
    let mut m = IndexMap::new();
    m.insert(key.to_string(), val);
    JValue::object(m)
}

/// Flat array of f64 values: [0.0, 1.0, ..., (n-1).0].
fn numeric_array(n: usize) -> JValue {
    let values: Vec<JValue> = (0..n).map(|i| JValue::from(i as f64)).collect();
    let mut root = IndexMap::new();
    root.insert("values".to_string(), JValue::array(values));
    JValue::object(root)
}

/// 100 simple product objects: {id, name, price, inStock}.
fn products_simple_100() -> JValue {
    let products: Vec<JValue> = (0..100_usize)
        .map(|i| {
            let mut m = IndexMap::new();
            m.insert("id".to_string(), JValue::from(i as f64));
            m.insert("name".to_string(), JValue::string(format!("Product {i}")));
            m.insert("price".to_string(), JValue::from(10.0 + i as f64 * 2.5));
            m.insert("inStock".to_string(), JValue::Bool(i % 2 == 0));
            JValue::object(m)
        })
        .collect();
    let mut root = IndexMap::new();
    root.insert("products".to_string(), JValue::array(products));
    JValue::object(root)
}

/// Full e-commerce dataset: 100 products matching benchmark.py exactly.
///
/// Each product: {id, name, category, price, inStock, rating, reviews, tags, vendor}.
fn ecommerce_100() -> JValue {
    let categories = ["Electronics", "Clothing", "Books", "Home"];
    let products: Vec<JValue> = (0..100_usize)
        .map(|i| {
            let tags: Vec<JValue> = (0..i % 5)
                .map(|j| JValue::string(format!("tag{j}")))
                .collect();

            let mut vendor = IndexMap::new();
            vendor.insert(
                "name".to_string(),
                JValue::string(format!("Vendor {}", i % 10)),
            );
            vendor.insert(
                "rating".to_string(),
                JValue::from(4.0 + (i % 5) as f64 * 0.2),
            );

            let mut p = IndexMap::new();
            p.insert("id".to_string(), JValue::from(i as f64));
            p.insert("name".to_string(), JValue::string(format!("Product {i}")));
            p.insert(
                "category".to_string(),
                JValue::string(categories[i % 4]),
            );
            p.insert("price".to_string(), JValue::from(10.0 + i as f64 * 5.5));
            p.insert("inStock".to_string(), JValue::Bool(i % 3 != 0));
            p.insert(
                "rating".to_string(),
                JValue::from(3.0 + (i % 3) as f64 * 0.5),
            );
            p.insert("reviews".to_string(), JValue::from((i * 2) as f64));
            p.insert("tags".to_string(), JValue::array(tags));
            p.insert("vendor".to_string(), JValue::object(vendor));
            JValue::object(p)
        })
        .collect();

    let mut root = IndexMap::new();
    root.insert("products".to_string(), JValue::array(products));
    JValue::object(root)
}

/// 100 order objects used for the multi-step filtered path benchmark.
///
/// Each order: {customer, total, items}.  ~half have total > 100 so the filter
/// is selective (exercises both matching and non-matching elements).
fn orders_100() -> JValue {
    let orders: Vec<JValue> = (0..100_usize)
        .map(|i| {
            let mut m = IndexMap::new();
            m.insert(
                "customer".to_string(),
                JValue::string(format!("Customer {i}")),
            );
            m.insert("total".to_string(), JValue::from(i as f64 * 2.0 + 1.0));
            m.insert("items".to_string(), JValue::from(i as f64 % 5.0 + 1.0));
            JValue::object(m)
        })
        .collect();
    let mut root = IndexMap::new();
    root.insert("orders".to_string(), JValue::array(orders));
    JValue::object(root)
}

// ── Helper: evaluate expression on data ───────────────────────────────────────

#[inline]
fn eval(ast: &jsonata_core::ast::AstNode, data: &JValue) -> JValue {
    Evaluator::new().evaluate(ast, data).unwrap()
}

// ── Bench groups ──────────────────────────────────────────────────────────────

fn bench_simple_paths(c: &mut Criterion) {
    let mut group = c.benchmark_group("simple_path");
    group.sample_size(300);

    // user.name
    {
        let ast = parser::parse("name").unwrap();
        let data = JValue::from_json_str(r#"{"name":"Alice","age":30}"#).unwrap();
        group.bench_function("simple_path", |b| {
            b.iter(|| black_box(eval(black_box(&ast), black_box(&data))))
        });
    }

    // a.b.c.d.e — 5-level deep
    {
        let ast = parser::parse("a.b.c.d.e").unwrap();
        let data =
            JValue::from_json_str(r#"{"a":{"b":{"c":{"d":{"e":42}}}}}"#).unwrap();
        group.bench_function("deep_path_5", |b| {
            b.iter(|| black_box(eval(black_box(&ast), black_box(&data))))
        });
    }

    // arr[42] — array index access on 100-element array
    {
        let ast = parser::parse("arr[42]").unwrap();
        let arr: Vec<JValue> = (0..100).map(|i| JValue::from(i as f64)).collect();
        let mut m = IndexMap::new();
        m.insert("arr".to_string(), JValue::array(arr));
        let data = JValue::object(m);
        group.bench_function("array_index_100", |b| {
            b.iter(|| black_box(eval(black_box(&ast), black_box(&data))))
        });
    }

    // price * quantity — arithmetic
    {
        let ast = parser::parse("price * quantity").unwrap();
        let data = JValue::from_json_str(r#"{"price":10.5,"quantity":3}"#).unwrap();
        group.bench_function("arithmetic", |b| {
            b.iter(|| black_box(eval(black_box(&ast), black_box(&data))))
        });
    }

    // 12-level deep path
    {
        let ast = parser::parse("a.b.c.d.e.f.g.h.i.j.k.l").unwrap();
        let data = JValue::from_json_str(
            r#"{"a":{"b":{"c":{"d":{"e":{"f":{"g":{"h":{"i":{"j":{"k":{"l":42}}}}}}}}}}}}"#,
        )
        .unwrap();
        group.bench_function("deep_path_12", |b| {
            b.iter(|| black_box(eval(black_box(&ast), black_box(&data))))
        });
    }

    group.finish();
}

fn bench_array_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("array_operations");

    // Aggregates on flat numeric arrays at three sizes
    for n in [100_usize, 1000, 10000] {
        let data = numeric_array(n);
        let ast_sum = parser::parse("$sum(values)").unwrap();
        let ast_max = parser::parse("$max(values)").unwrap();
        let ast_count = parser::parse("$count(values)").unwrap();

        group.bench_with_input(BenchmarkId::new("sum", n), &data, |b, d| {
            b.iter(|| black_box(eval(black_box(&ast_sum), black_box(d))))
        });
        group.bench_with_input(BenchmarkId::new("max", n), &data, |b, d| {
            b.iter(|| black_box(eval(black_box(&ast_max), black_box(d))))
        });
        if n == 100 {
            group.bench_with_input(BenchmarkId::new("count", n), &data, |b, d| {
                b.iter(|| black_box(eval(black_box(&ast_count), black_box(d))))
            });
        }
    }

    // Field extraction across 100 product objects
    {
        let data = products_simple_100();
        let ast = parser::parse("products.price").unwrap();
        group.bench_function("map_field_100", |b| {
            b.iter(|| black_box(eval(black_box(&ast), black_box(&data))))
        });
    }

    // Map then aggregate: $sum(products.price)
    {
        let data = products_simple_100();
        let ast = parser::parse("$sum(products.price)").unwrap();
        group.bench_function("map_sum_100", |b| {
            b.iter(|| black_box(eval(black_box(&ast), black_box(&data))))
        });
    }

    // Filter predicate: products[price > 100]
    {
        let data = products_simple_100();
        let ast = parser::parse("products[price > 100]").unwrap();
        group.bench_function("filter_predicate_100", |b| {
            b.iter(|| black_box(eval(black_box(&ast), black_box(&data))))
        });
    }

    group.finish();
}

fn bench_complex_transformations(c: &mut Criterion) {
    let mut group = c.benchmark_group("complex_transformations");
    group.sample_size(300);

    // Object construction (simple)
    {
        let ast =
            parser::parse(r#"{"name": name, "greeting": "Hello, " & name & "!"}"#).unwrap();
        let data = JValue::from_json_str(r#"{"name":"World","value":42}"#).unwrap();
        group.bench_function("object_construction_simple", |b| {
            b.iter(|| black_box(eval(black_box(&ast), black_box(&data))))
        });
    }

    // Object construction (nested)
    {
        let ast = parser::parse(
            r#"{"outer": {"inner": {"value": value * 2, "name": name}}}"#,
        )
        .unwrap();
        let data = JValue::from_json_str(r#"{"name":"test","value":21}"#).unwrap();
        group.bench_function("object_construction_nested", |b| {
            b.iter(|| black_box(eval(black_box(&ast), black_box(&data))))
        });
    }

    // Conditional expression
    {
        let ast = parser::parse(r#"value > 0 ? "positive" : "non-positive""#).unwrap();
        let data = tiny_obj("value", JValue::from(42.0));
        group.bench_function("conditional", |b| {
            b.iter(|| black_box(eval(black_box(&ast), black_box(&data))))
        });
    }

    // Nested function calls
    {
        let ast = parser::parse("$length($uppercase(name))").unwrap();
        let data = tiny_obj("name", JValue::string("JSONata Performance Test"));
        group.bench_function("nested_functions", |b| {
            b.iter(|| black_box(eval(black_box(&ast), black_box(&data))))
        });
    }

    group.finish();
}

fn bench_string_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("string_operations");
    group.sample_size(300);

    let cases: &[(&str, &str, &str)] = &[
        ("uppercase", "$uppercase(name)", r#"{"name":"hello world"}"#),
        ("lowercase", "$lowercase(name)", r#"{"name":"HELLO WORLD"}"#),
        ("length", "$length(name)", r#"{"name":"JSONata Performance Benchmark"}"#),
        (
            "concat",
            r#"first & " " & last"#,
            r#"{"first":"John","last":"Doe"}"#,
        ),
        (
            "substring",
            "$substring(text, 0, 10)",
            r#"{"text":"This is a long string for substring extraction"}"#,
        ),
        (
            "contains",
            r#"$contains(text, "JSONata")"#,
            r#"{"text":"JSONata is a query and transformation language"}"#,
        ),
    ];

    for (name, expr, data_str) in cases {
        let ast = parser::parse(expr).unwrap();
        let data = JValue::from_json_str(data_str).unwrap();
        group.bench_function(*name, |b| {
            b.iter(|| black_box(eval(black_box(&ast), black_box(&data))))
        });
    }

    group.finish();
}

fn bench_higher_order_functions(c: &mut Criterion) {
    let mut group = c.benchmark_group("higher_order_functions");

    let numbers: Vec<JValue> = (1..=100).map(|i| JValue::from(i as f64)).collect();
    let mut root = IndexMap::new();
    root.insert("numbers".to_string(), JValue::array(numbers));
    let data = JValue::object(root);

    let cases: &[(&str, &str)] = &[
        ("map", "$map(numbers, function($v) { $v * 2 })"),
        ("filter", "$filter(numbers, function($v) { $v > 50 })"),
        (
            "reduce",
            "$reduce(numbers, function($acc, $v) { $acc + $v }, 0)",
        ),
    ];

    for (name, expr) in cases {
        let ast = parser::parse(expr).unwrap();
        group.bench_function(*name, |b| {
            b.iter(|| black_box(eval(black_box(&ast), black_box(&data))))
        });
    }

    group.finish();
}

fn bench_realistic_workload(c: &mut Criterion) {
    let mut group = c.benchmark_group("realistic_workload");

    let data = ecommerce_100();

    let cases: &[(&str, &str)] = &[
        (
            "filter_by_category",
            r#"products[category = "Electronics"]"#,
        ),
        (
            "calculate_total_value",
            "$sum(products[inStock].price)",
        ),
        (
            "complex_transformation",
            r#"products[price > 50 and inStock].{"name": name, "price": price, "vendor": vendor.name}"#,
        ),
        (
            "group_by_category",
            r#"{
                "Electronics": $sum(products[category = "Electronics"].price),
                "Clothing":    $sum(products[category = "Clothing"].price),
                "Books":       $sum(products[category = "Books"].price),
                "Home":        $sum(products[category = "Home"].price)
            }"#,
        ),
        (
            "top_rated",
            "$sort(products[rating >= 4], function($l, $r) { $r.rating - $l.rating })",
        ),
    ];

    for (name, expr) in cases {
        let ast = parser::parse(expr).unwrap();
        group.bench_function(*name, |b| {
            b.iter(|| black_box(eval(black_box(&ast), black_box(&data))))
        });
    }

    group.finish();
}

/// Compare tree-walker vs bytecode VM for every expression the compiler handles.
///
/// Run with: `cargo bench --features bench -- vm_vs`
///
/// For each expression, Criterion reports both paths in the same group so you
/// get a direct timing comparison and a speedup ratio in the HTML report.
/// Expressions where `_bench::compile` returns `None` (the compiler falls back
/// to the tree-walker) are skipped for the VM column.
#[cfg(feature = "bench")]
fn bench_vm_vs_tree_walker(c: &mut Criterion) {
    // ── Tiny-data expressions ─────────────────────────────────────────────────

    let tiny_cases: &[(&str, &str, &str)] = &[
        ("simple_path",       "name",                             r#"{"name":"Alice"}"#),
        ("arithmetic",        "price * quantity",                  r#"{"price":10.5,"quantity":3}"#),
        ("conditional",       r#"value > 0 ? "positive" : "non-positive""#, r#"{"value":42}"#),
        ("nested_builtins",   "$length($uppercase(name))",         r#"{"name":"JSONata Performance Test"}"#),
        ("deep_path_5",       "a.b.c.d.e",                        r#"{"a":{"b":{"c":{"d":{"e":42}}}}}"#),
        ("deep_path_12",      "a.b.c.d.e.f.g.h.i.j.k.l",
            r#"{"a":{"b":{"c":{"d":{"e":{"f":{"g":{"h":{"i":{"j":{"k":{"l":42}}}}}}}}}}}}"#),
        ("uppercase",         "$uppercase(name)",                  r#"{"name":"hello world"}"#),
        ("lowercase",         "$lowercase(name)",                  r#"{"name":"HELLO WORLD"}"#),
        ("str_length",        "$length(name)",                     r#"{"name":"JSONata Performance Benchmark"}"#),
        ("substring",         "$substring(text, 0, 10)",           r#"{"text":"This is a long string for substring"}"#),
        ("contains",          r#"$contains(text, "JSONata")"#,     r#"{"text":"JSONata is a query language"}"#),
        ("concat",            r#"first & " " & last"#,             r#"{"first":"John","last":"Doe"}"#),
    ];

    for (name, expr, data_str) in tiny_cases {
        let ast  = parser::parse(expr).unwrap();
        let data = JValue::from_json_str(data_str).unwrap();
        let bc   = _bench::compile(&ast);

        let mut group = c.benchmark_group(format!("vm_vs/{name}"));
        group.sample_size(300);

        group.bench_function("tree_walker", |b| {
            b.iter(|| black_box(Evaluator::new().evaluate(black_box(&ast), black_box(&data)).unwrap()))
        });
        if let Some(bc) = &bc {
            group.bench_function("vm", |b| {
                b.iter(|| black_box(_bench::run(bc, black_box(&data)).unwrap()))
            });
        }
        group.finish();
    }

    // ── Aggregates on 100-element numeric array ───────────────────────────────

    let data100 = numeric_array(100);
    for (name, expr) in [("sum_100", "$sum(values)"), ("max_100", "$max(values)"), ("count_100", "$count(values)")] {
        let ast = parser::parse(expr).unwrap();
        let bc  = _bench::compile(&ast);

        let mut group = c.benchmark_group(format!("vm_vs/{name}"));
        group.bench_function("tree_walker", |b| {
            b.iter(|| black_box(Evaluator::new().evaluate(black_box(&ast), black_box(&data100)).unwrap()))
        });
        if let Some(bc) = &bc {
            group.bench_function("vm", |b| {
                b.iter(|| black_box(_bench::run(bc, black_box(&data100)).unwrap()))
            });
        }
        group.finish();
    }

    // ── Array mapping + aggregate on 100 product objects ─────────────────────

    let products = products_simple_100();
    for (name, expr) in [
        ("map_field_100",    "products.price"),
        ("map_sum_100",      "$sum(products.price)"),
        ("filter_pred_100",  "products[price > 100]"),
    ] {
        let ast = parser::parse(expr).unwrap();
        let bc  = _bench::compile(&ast);

        let mut group = c.benchmark_group(format!("vm_vs/{name}"));
        group.bench_function("tree_walker", |b| {
            b.iter(|| black_box(Evaluator::new().evaluate(black_box(&ast), black_box(&products)).unwrap()))
        });
        if let Some(bc) = &bc {
            group.bench_function("vm", |b| {
                b.iter(|| black_box(_bench::run(bc, black_box(&products)).unwrap()))
            });
        }
        group.finish();
    }

    // ── Multi-step filtered path: filter + field-step on 100 order objects ───
    //
    // orders[total > 100].customer exercises the EvalFallback → compiled_eval_field_path
    // path. Shape caches are auto-built in compiled_apply_filter (filter step) and
    // compiled_field_step (field-step array mapping), making this measurably faster.
    {
        let orders = orders_100();
        let ast = parser::parse("orders[total > 100].customer").unwrap();
        let bc  = _bench::compile(&ast);

        let mut group = c.benchmark_group("vm_vs/filter_nested_100");
        group.bench_function("tree_walker", |b| {
            b.iter(|| black_box(Evaluator::new().evaluate(black_box(&ast), black_box(&orders)).unwrap()))
        });
        if let Some(bc) = &bc {
            group.bench_function("vm", |b| {
                b.iter(|| black_box(_bench::run(bc, black_box(&orders)).unwrap()))
            });
        }
        group.finish();
    }

    // ── Higher-order functions — inline lambda compiled path ─────────────────
    //
    // $map/$filter/$reduce with inline lambda literals compile to MapCall/FilterCall/
    // ReduceCall CompiledExpr variants. The vm variant runs eval_compiled_inner
    // directly (via EvalFallback), bypassing the tree-walker's Context overhead
    // and StoredLambda allocation per call.

    let numbers: Vec<JValue> = (1..=100).map(|i| JValue::from(i as f64)).collect();
    let mut hof_root = IndexMap::new();
    hof_root.insert("numbers".to_string(), JValue::array(numbers));
    let hof_data = JValue::object(hof_root);

    for (name, expr) in [
        ("hof_map",    "$map(numbers, function($v) { $v * 2 })"),
        ("hof_filter", "$filter(numbers, function($v) { $v > 50 })"),
        ("hof_reduce", "$reduce(numbers, function($acc, $v) { $acc + $v }, 0)"),
    ] {
        let ast = parser::parse(expr).unwrap();
        let bc = _bench::compile(&ast);

        let mut group = c.benchmark_group(format!("vm_vs/{name}"));
        group.bench_function("tree_walker", |b| {
            b.iter(|| black_box(Evaluator::new().evaluate(black_box(&ast), black_box(&hof_data)).unwrap()))
        });
        if let Some(bc) = &bc {
            group.bench_function("vm", |b| {
                b.iter(|| black_box(_bench::run(bc, black_box(&hof_data)).unwrap()))
            });
        }
        group.finish();
    }
}

#[cfg(not(feature = "bench"))]
fn bench_vm_vs_tree_walker(_c: &mut Criterion) {}

criterion_group!(
    benches,
    bench_simple_paths,
    bench_array_operations,
    bench_complex_transformations,
    bench_string_operations,
    bench_higher_order_functions,
    bench_realistic_workload,
    bench_vm_vs_tree_walker,
);
criterion_main!(benches);

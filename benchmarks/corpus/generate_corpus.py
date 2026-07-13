#!/usr/bin/env python3
"""Emit corpus.json: the shared expression/data corpus for the Java/.NET FFI
benchmark experiment.

Scenario definitions mirror benchmarks/python/benchmark.py main() Parts 1-7
(Part 8, "Evaluation Path Comparison", is jsonatapy-internal and excluded).
If benchmark.py's scenarios change, re-derive this file from it.

Usage: python3 generate_corpus.py   (writes corpus.json next to itself)
"""

import json
from pathlib import Path


def scenarios() -> list[dict]:
    array_100 = {"values": list(range(100))}
    array_1000 = {"values": list(range(1000))}
    array_10000 = {"values": list(range(10000))}
    products_100 = {
        "products": [
            {"id": i, "name": f"Product {i}", "price": 10.0 + i * 2.5, "inStock": i % 2 == 0}
            for i in range(100)
        ]
    }
    deep_data = {
        "a": {"b": {"c": {"d": {"e": {"f": {"g": {"h": {"i": {"j": {"k": {"l": 42}}}}}}}}}}}
    }
    nested_arrays = {
        "data": [[[[i, i + 1, i + 2] for i in range(0, 30, 3)] for _ in range(3)] for _ in range(3)]
    }
    numbers_data = {"numbers": list(range(1, 101))}
    ecommerce_data = {
        "products": [
            {
                "id": i,
                "name": f"Product {i}",
                "category": ["Electronics", "Clothing", "Books", "Home"][i % 4],
                "price": 10.0 + i * 5.5,
                "inStock": i % 3 != 0,
                "rating": 3.0 + (i % 3) * 0.5,
                "reviews": i * 2,
                "tags": [f"tag{j}" for j in range(i % 5)],
                "vendor": {"name": f"Vendor {i % 10}", "rating": 4.0 + (i % 5) * 0.2},
            }
            for i in range(100)
        ]
    }
    group_by_expression = """
            {
                "Electronics": $sum(products[category = "Electronics"].price),
                "Clothing": $sum(products[category = "Clothing"].price),
                "Books": $sum(products[category = "Books"].price),
                "Home": $sum(products[category = "Home"].price)
            }
        """

    def s(name, category, expression, data, iterations):
        return {
            "name": name,
            "category": category,
            "expression": expression,
            "data": data,
            "iterations": iterations,
        }

    return [
        # Part 1: Simple Paths
        s("Simple Path", "Simple Paths", "user.name",
          {"user": {"name": "Alice", "age": 30}}, 10000),
        s("Deep Path (5 levels)", "Simple Paths", "a.b.c.d.e",
          {"a": {"b": {"c": {"d": {"e": 42}}}}}, 10000),
        s("Array Index Access", "Simple Paths", "values[50]",
          {"values": list(range(100))}, 5000),
        s("Arithmetic Expression", "Simple Paths", "price * quantity",
          {"price": 10.5, "quantity": 3}, 10000),
        # Part 2: Array Operations
        s("Array Sum (100 elements)", "Array Operations", "$sum(values)", array_100, 1000),
        s("Array Max (100 elements)", "Array Operations", "$max(values)", array_100, 1000),
        s("Array Count (100 elements)", "Array Operations", "$count(values)", array_100, 2000),
        s("Array Sum (1000 elements)", "Array Operations", "$sum(values)", array_1000, 200),
        s("Array Max (1000 elements)", "Array Operations", "$max(values)", array_1000, 200),
        s("Array Sum (10000 elements)", "Array Operations", "$sum(values)", array_10000, 50),
        s("Array Mapping (extract field)", "Array Operations", "products.price",
          products_100, 1000),
        s("Array Mapping + Sum", "Array Operations", "$sum(products.price)",
          products_100, 1000),
        s("Array Filtering (predicate)", "Array Operations", "products[price > 100]",
          products_100, 500),
        # Part 3: Complex Transformations
        s("Object Construction (simple)", "Complex Transformations",
          '{"fullName": first & " " & last, "age": age}',
          {"first": "John", "last": "Doe", "age": 30}, 5000),
        s("Object Construction (nested)", "Complex Transformations",
          '{"user": {"name": name, "contact": {"email": email, "phone": phone}}}',
          {"name": "Alice", "email": "alice@example.com", "phone": "555-1234"}, 5000),
        s("Conditional Expression", "Complex Transformations",
          'age >= 18 ? "adult" : "minor"', {"age": 25}, 5000),
        s("Multiple Nested Functions", "Complex Transformations",
          "$length($uppercase(name))", {"name": "JSONata Performance Test"}, 5000),
        # Part 4: Deep Nesting
        s("Deep Path (12 levels)", "Deep Nesting", "a.b.c.d.e.f.g.h.i.j.k.l",
          deep_data, 5000),
        s("Nested Array Access", "Deep Nesting", "data[1][1][1][1]", nested_arrays, 2000),
        # Part 5: String Operations
        s("String Uppercase", "String Operations", "$uppercase(name)",
          {"name": "hello world"}, 10000),
        s("String Lowercase", "String Operations", "$lowercase(name)",
          {"name": "HELLO WORLD"}, 10000),
        s("String Length", "String Operations", "$length(name)",
          {"name": "JSONata Performance Benchmark Suite"}, 10000),
        s("String Concatenation", "String Operations", '$join([first, last], " ")',
          {"first": "John", "last": "Doe"}, 5000),
        s("String Substring", "String Operations", "$substring(text, 0, 10)",
          {"text": "This is a long string that we will extract a substring from"}, 5000),
        s("String Contains", "String Operations", '$contains(text, "JSONata")',
          {"text": "JSONata is a query and transformation language for JSON"}, 5000),
        # Part 6: Higher-Order Functions
        s("$map with lambda", "Higher-Order Functions",
          "$map(numbers, function($v) { $v * 2 })", numbers_data, 200),
        s("$filter with lambda", "Higher-Order Functions",
          "$filter(numbers, function($v) { $v > 50 })", numbers_data, 200),
        s("$reduce with lambda", "Higher-Order Functions",
          "$reduce(numbers, function($acc, $v) { $acc + $v }, 0)", numbers_data, 200),
        # Part 7: Realistic Workload (E-Commerce)
        s("Filter by category", "Realistic Workload",
          'products[category = "Electronics"]', ecommerce_data, 500),
        s("Calculate total value", "Realistic Workload",
          "$sum(products[inStock].price)", ecommerce_data, 500),
        s("Complex transformation", "Realistic Workload",
          'products[price > 50 and inStock].{"name": name, "price": price, "vendor": vendor.name}',
          ecommerce_data, 200),
        s("Group by category (aggregate)", "Realistic Workload",
          group_by_expression, ecommerce_data, 200),
        s("Top rated products", "Realistic Workload",
          "$sort(products[rating >= 4], function($l, $r) { $r.rating - $l.rating })",
          ecommerce_data, 100),
    ]


def main() -> None:
    out = Path(__file__).parent / "corpus.json"
    items = scenarios()
    names = [x["name"] for x in items]
    assert len(names) == len(set(names)), "scenario names must be unique"
    assert len(items) == 33, f"expected 33 scenarios, got {len(items)}"
    out.write_text(json.dumps(items, indent=2) + "\n")
    print(f"wrote {out} ({len(items)} scenarios)")


if __name__ == "__main__":
    main()

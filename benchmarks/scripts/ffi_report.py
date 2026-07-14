#!/usr/bin/env python3
"""Merge Java/JMH + .NET/BDN results into the FFI experiment report.

Usage: uv run python benchmarks/scripts/ffi_report.py [out.md]
Reads (relative to repo root):
  benchmarks/corpus/corpus.json
  benchmarks/java/gate_results.json,   benchmarks/java/jmh_results.json
  benchmarks/dotnet/gate_results.json, benchmarks/dotnet/bdn_results.json
"""

import json
import math
import sys
from pathlib import Path

import jsonatapy

ROOT = Path(__file__).resolve().parents[2]

# JMH: [{benchmark: "...FfiBenchmark.coreSsCompiled", params: {scenario}, primaryMetric: {score us/op}}]
JMH_EXTRACT = jsonatapy.compile(
    '$.{"method": $split(benchmark, ".")[-1], "scenario": params.scenario,'
    ' "us": primaryMetric.score}[]'
)
# BDN full json: {Benchmarks: [{Method, FullName: '...(Scenario: "X")', Statistics: {Mean ns}}]}
# Scenario is parsed from FullName, NOT Parameters — BDN truncates long param
# values in Parameters/DisplayInfo ("Objec(...)sted) [28]"), which collides
# similarly-named scenarios; FullName carries the untruncated value.
BDN_EXTRACT = jsonatapy.compile(
    "Benchmarks.{\"method\": Method,"
    " \"scenario\": $substringBefore($substringAfter(FullName, 'Scenario: \"'), '\"'),"
    " \"us\": Statistics.Mean / 1000}[]"
)

CORE_SS = {"java": "coreSsCompiled", "dotnet": "CoreSsCompiled"}
CORE_SS_EACH = {"java": "coreSsCompileEach", "dotnet": "CoreSsCompileEach"}
COMP_SS = {"java": "dashjoinSsCompiled", "dotnet": "JnnSsCompiled"}
COMP_SS_EACH = {"java": "dashjoinSsCompileEach", "dotnet": "JnnSsCompileEach"}
COMP_HOME = {"java": "dashjoinHomeTurfCompiled", "dotnet": "JnnHomeTurfCompiled"}
COMPETITOR = {"java": "dashjoin/jsonata-java 0.9.10", "dotnet": "Jsonata.Net.Native 3.0.0"}


def load(path: str):
    return json.loads((ROOT / path).read_text())


def geomean(values: list[float]) -> float:
    return math.exp(sum(math.log(v) for v in values) / len(values)) if values else float("nan")


def index_results(rows: list[dict]) -> dict[tuple[str, str], float]:
    return {(r["scenario"], r["method"]): r["us"] for r in rows}


def lang_section(lang: str, rows: list[dict], gate: list[dict], categories: dict[str, str]) -> str:
    by = index_results(rows)
    passed = {g["scenario"] for g in gate if g["status"] == "match"}
    flagged = [g for g in gate if g["status"] != "match"]
    scenarios = sorted({r["scenario"] for r in rows}, key=lambda s: (categories.get(s, ""), s))

    lines = []
    lines.append("### Correctness gate\n")
    lines.append(f"{len(passed)}/{len(gate)} scenarios match. "
                 + ("All scenarios included.\n" if not flagged else
                    "Excluded from aggregates (flagged):\n"))
    for g in flagged:
        lines.append(f"- **{g['scenario']}**: {g['status']} — {g['detail']}\n")

    lines.append("\n### Per-scenario results (µs/op, lower is better)\n")
    lines.append("| Scenario | core s→s | comp s→s | speedup | core s→s (compile-each) |"
                 " comp s→s (compile-each) | speedup | comp home-turf | core-vs-home-turf |")
    lines.append("|---|---|---|---|---|---|---|---|---|")
    ss_speedups, each_speedups, home_speedups = [], [], []
    rows_out = []
    for s in scenarios:
        c = by.get((s, CORE_SS[lang]))
        k = by.get((s, COMP_SS[lang]))
        ce = by.get((s, CORE_SS_EACH[lang]))
        ke = by.get((s, COMP_SS_EACH[lang]))
        h = by.get((s, COMP_HOME[lang]))
        if None in (c, k, ce, ke, h):
            continue
        ss, ee, hh = k / c, ke / ce, h / c
        excl = s not in passed
        if not excl:
            ss_speedups.append(ss)
            each_speedups.append(ee)
            home_speedups.append(hh)
        flag = " ⚠️excluded" if excl else ""
        rows_out.append((s, ss))
        lines.append(f"| {s}{flag} | {c:.2f} | {k:.2f} | {ss:.2f}x | {ce:.2f} | {ke:.2f} |"
                     f" {ee:.2f}x | {h:.2f} | {hh:.2f}x |")

    lines.append("\n### Aggregates (gate-passed scenarios only)\n")
    lines.append(f"- **String→string, compiled (geomean): {geomean(ss_speedups):.2f}x** "
                 f"({COMPETITOR[lang]} time ÷ core time; >1 means core is faster)")
    lines.append(f"- String→string, compile-each (geomean): {geomean(each_speedups):.2f}x")
    lines.append(f"- Home-turf (competitor on pre-parsed native data, core still paying the"
                 f" string boundary; geomean): {geomean(home_speedups):.2f}x")
    ranked = sorted(rows_out, key=lambda t: t[1])
    if len(ranked) >= 3:
        worst = ", ".join(f"{n} ({v:.2f}x)" for n, v in ranked[:3])
        best = ", ".join(f"{n} ({v:.2f}x)" for n, v in ranked[-3:][::-1])
        lines.append(f"- Best 3 (s→s compiled): {best}")
        lines.append(f"- Worst 3 (s→s compiled): {worst}")
    lines.append("")
    return "\n".join(lines)


def main() -> None:
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "benchmarks/results/ffi_experiment_report_2026-07-13.md"
    corpus = load("benchmarks/corpus/corpus.json")
    categories = {c["name"]: c["category"] for c in corpus}

    jmh = JMH_EXTRACT.evaluate(load("benchmarks/java/jmh_results.json"))
    bdn = BDN_EXTRACT.evaluate(load("benchmarks/dotnet/bdn_results.json"))

    parts = [
        "# Java/.NET FFI Benchmark Experiment — Report (2026-07-13)\n",
        "Spike per docs/superpowers/specs/2026-07-13-java-dotnet-ffi-benchmark-experiment-design.md.",
        "jsonata-core consumed over the C ABI (`capi` feature), JSON-as-string boundary.",
        "Java: JMH 1.37, 1 fork, 3×1s warmup, 5×1s measure. .NET: BenchmarkDotNet 0.15.8, ShortRun job.",
        "Spike-scale iteration counts — treat small (<1.2x) differences as noise.\n",
        "**Reading the modes:** *s→s* = JSON text in/out for both sides (symmetric).",
        "*Home-turf* = competitor evaluates pre-parsed native objects with no result",
        "serialization (its best realistic case) while core still pays the full string",
        "boundary (its worst realistic case).\n",
        f"## Java — core (FFM) vs {COMPETITOR['java']}\n",
        lang_section("java", jmh, load("benchmarks/java/gate_results.json"), categories),
        f"## .NET — core (LibraryImport) vs {COMPETITOR['dotnet']}\n",
        lang_section("dotnet", bdn, load("benchmarks/dotnet/gate_results.json"), categories),
    ]
    out.write_text("\n".join(parts))
    print(f"wrote {out}")


if __name__ == "__main__":
    main()

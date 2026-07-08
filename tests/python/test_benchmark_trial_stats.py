"""Tests for benchmarks/python/benchmark.py's repeated-trial statistics."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent.parent / "benchmarks" / "python"))

from benchmark import trial_stats


def test_trial_stats_returns_min_max_mean_median():
    stats = trial_stats([10.0, 12.0, 9.0, 11.0, 15.0])

    assert stats["min"] == 9.0
    assert stats["max"] == 15.0
    assert stats["mean"] == 11.4
    assert stats["median"] == 11.0


def test_trial_stats_single_value():
    stats = trial_stats([7.5])

    assert stats == {"min": 7.5, "max": 7.5, "mean": 7.5, "median": 7.5}


def test_jsonatapy_benchmark_returns_full_stats_dict():
    from benchmark import BenchmarkSuite

    suite = BenchmarkSuite(output_json=False, output_graphs=False)
    stats = suite._run_jsonatapy_benchmark("1 + 1", {}, iterations=50, repeats=3)

    assert stats is not None
    assert set(stats.keys()) == {"min", "max", "mean", "median"}
    assert stats["min"] <= stats["median"] <= stats["max"]
    assert stats["min"] <= stats["mean"] <= stats["max"]


def test_benchmark_populates_stats_by_impl():
    from benchmark import BenchmarkSuite

    suite = BenchmarkSuite(output_json=False, output_graphs=False)
    suite.benchmark(
        name="trivial",
        category="Simple Paths",
        expression="1 + 1",
        data={},
        data_size="tiny",
        iterations=20,
        verbose=False,
    )

    result = suite.results[-1]
    assert "jsonatapy" in result.stats_by_impl
    assert result.stats_by_impl["jsonatapy"]["min"] == result.jsonatapy_ms

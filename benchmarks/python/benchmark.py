#!/usr/bin/env python3
"""
Comprehensive JSONata Benchmark Suite

Compares performance across multiple implementations:
1. jsonatapy (this project - Rust/PyO3)
2. jsonata (JS reference - via Node.js)
3. jsonata-python (rayokota wrapper - if available)
4. jsonata-rs (Stedi Rust - future integration)

Includes test categories:
- Simple paths (warm-up)
- Array operations (map, filter, aggregations)
- Complex transformations (object construction, nested functions)
- Deep nesting (10+ levels)
- String operations
- Higher-order functions (lambdas, $map, $filter, $reduce)
- Realistic workload (e-commerce product catalog)
"""

import gc
import json
import subprocess
import time
import tracemalloc
from dataclasses import asdict, dataclass
from datetime import datetime
from pathlib import Path
from typing import Any

# Try to import all available implementations
try:
    import jsonatapy

    JSONATAPY_AVAILABLE = True
except ImportError:
    print("⚠ jsonatapy not available. Run 'maturin develop' first.")
    JSONATAPY_AVAILABLE = False

try:
    import jsonata as jsonata_python

    JSONATA_PYTHON_AVAILABLE = True
except ImportError:
    JSONATA_PYTHON_AVAILABLE = False


@dataclass
class BenchmarkResult:
    """Single benchmark result."""

    name: str
    category: str
    expression: str
    data_size: str
    iterations: int
    jsonatapy_ms: float | None = None
    jsonatapy_json_ms: float | None = None
    js_ms: float | None = None
    jsonata_python_ms: float | None = None
    jsonata_rs_ms: float | None = None
    jsonatapy_speedup: float | None = None
    jsonatapy_json_speedup: float | None = None
    jsonata_python_speedup: float | None = None
    jsonata_rs_speedup: float | None = None
    jsonatapy_memory_mb: float | None = None
    js_memory_mb: float | None = None
    jsonata_python_memory_mb: float | None = None
    jsonata_rs_memory_mb: float | None = None
    error: str | None = None


class BenchmarkSuite:
    """Run comprehensive performance benchmarks across multiple implementations."""

    def __init__(self, output_json: bool = True, output_graphs: bool = True):
        self.results: list[BenchmarkResult] = []
        self.node_available = self._check_node()
        self.jsonata_rs_available = self._check_jsonata_rs()
        self.output_json = output_json
        self.output_graphs = output_graphs

        # Try to import visualization libraries
        self.has_matplotlib = False
        self.has_rich = False

        if output_graphs:
            try:
                import matplotlib

                matplotlib.use("Agg")  # Non-interactive backend
                import matplotlib.pyplot as plt

                self.plt = plt
                self.has_matplotlib = True
            except ImportError:
                print("⚠ matplotlib not available - graphs will be skipped")

        try:
            from rich.console import Console
            from rich.table import Table

            self.Console = Console
            self.Table = Table
            self.has_rich = True
        except ImportError:
            print("⚠ rich not available - will use plain text output")

    def _check_node(self) -> bool:
        """Check if Node.js is available."""
        try:
            result = subprocess.run(
                ["node", "--version"], capture_output=True, text=True, timeout=5
            )
            if result.returncode == 0:
                print(f"✓ Node.js detected: {result.stdout.strip()}")
                return True
        except (subprocess.SubprocessError, FileNotFoundError):
            pass

        print("⚠ Node.js not found - JavaScript benchmarks will be skipped")
        return False

    def _check_jsonata_rs(self) -> bool:
        """Check if jsonata-rs benchmark binary is available."""
        bench_dir = Path(__file__).parent.parent  # benchmarks/ directory
        binary_path = bench_dir / "rust" / "target" / "release" / "jsonata-rs-bench"

        if binary_path.exists():
            print("✓ jsonata-rs benchmark binary found")
            return True
        else:
            print(f"⚠ jsonata-rs binary not found at {binary_path}")
            print("  Run 'cd benchmarks && cargo build --release' to build it")
            return False

    def _run_js_benchmark(self, expression: str, data: Any, iterations: int) -> float:
        """Run benchmark using JavaScript reference implementation."""
        if not self.node_available:
            return -1.0

        bench_dir = Path(__file__).parent.parent
        js_script = bench_dir / "javascript" / "benchmark.js"

        if not js_script.exists():
            print(f"⚠ JavaScript benchmark script not found at {js_script}")
            return -1.0

        benchmark_data = {"expression": expression, "data": data, "iterations": iterations}

        try:
            result = subprocess.run(
                ["node", str(js_script)],
                input=json.dumps(benchmark_data),
                capture_output=True,
                text=True,
                timeout=60,
            )

            if result.returncode != 0:
                print(f"⚠ JavaScript benchmark failed: {result.stderr}")
                return -1.0

            return float(result.stdout.strip())
        except Exception as e:
            print(f"⚠ Error running JavaScript benchmark: {e}")
            return -1.0

    def _run_jsonatapy_benchmark(self, expression: str, data: Any, iterations: int) -> float:
        """Run benchmark using jsonatapy (Rust/PyO3) implementation."""
        if not JSONATAPY_AVAILABLE:
            return -1.0

        try:
            compiled = jsonatapy.compile(expression)
        except Exception as e:
            print(f"⚠ jsonatapy compilation failed: {e}")
            return -1.0

        # Warm up
        warmup_iters = min(100, max(10, iterations // 10))
        for _ in range(warmup_iters):
            try:
                compiled.evaluate(data)
            except Exception as e:
                print(f"⚠ jsonatapy warmup failed: {e}")
                return -1.0

        # Measure
        start = time.perf_counter()
        for _ in range(iterations):
            compiled.evaluate(data)
        elapsed = time.perf_counter() - start

        return elapsed * 1000  # Convert to milliseconds

    def _run_jsonatapy_json_benchmark(
        self, expression: str, data: Any, iterations: int
    ) -> float:
        """Run benchmark using jsonatapy's pure Rust path (JSON string I/O).

        This bypasses Python↔Rust object conversion by using evaluate_json(),
        giving a fair Rust-to-Rust comparison against jsonata-rs.
        """
        if not JSONATAPY_AVAILABLE:
            return -1.0

        try:
            compiled = jsonatapy.compile(expression)
        except Exception as e:
            print(f"⚠ jsonatapy (json) compilation failed: {e}")
            return -1.0

        # Pre-serialize data to JSON string (not counted in benchmark time)
        json_str = json.dumps(data)

        # Warm up
        warmup_iters = min(100, max(10, iterations // 10))
        for _ in range(warmup_iters):
            try:
                compiled.evaluate_json(json_str)
            except Exception as e:
                print(f"⚠ jsonatapy (json) warmup failed: {e}")
                return -1.0

        # Measure
        start = time.perf_counter()
        for _ in range(iterations):
            compiled.evaluate_json(json_str)
        elapsed = time.perf_counter() - start

        return elapsed * 1000  # Convert to milliseconds

    def _run_jsonata_python_benchmark(self, expression: str, data: Any, iterations: int) -> float:
        """Run benchmark using jsonata-python (rayokota) implementation.

        Note: jsonata-python doesn't support pre-compilation, so we call transform()
        directly in each iteration, which includes parsing overhead.
        """
        if not JSONATA_PYTHON_AVAILABLE:
            return -1.0

        # Compile expression once (new API: Jsonata class)
        try:
            expr = jsonata_python.Jsonata(expression)
        except Exception as e:
            print(f"⚠ jsonata-python compilation failed: {e}")
            return -1.0

        # Warm up
        warmup_iters = min(100, max(10, iterations // 10))
        for _ in range(warmup_iters):
            try:
                expr.evaluate(data)
            except Exception as e:
                print(f"⚠ jsonata-python warmup failed: {e}")
                return -1.0

        # Measure
        start = time.perf_counter()
        for _ in range(iterations):
            try:
                expr.evaluate(data)
            except Exception as e:
                print(f"⚠ jsonata-python evaluation failed: {e}")
                return -1.0
        elapsed = time.perf_counter() - start

        return elapsed * 1000  # Convert to milliseconds

    def _run_jsonata_rs_benchmark(self, expression: str, data: Any, iterations: int) -> float:
        """Run benchmark using jsonata-rs (pure Rust) implementation."""
        if not self.jsonata_rs_available:
            return -1.0

        bench_dir = Path(__file__).parent.parent  # benchmarks/ directory
        binary_path = bench_dir / "rust" / "target" / "release" / "jsonata-rs-bench"

        # Prepare input JSON
        input_data = {
            "expression": expression,
            "data": data,
            "iterations": iterations,
            "warmup": min(100, max(10, iterations // 10)),
        }
        input_json = json.dumps(input_data)

        try:
            result = subprocess.run(
                [str(binary_path)], input=input_json, capture_output=True, text=True, timeout=60
            )

            if result.returncode != 0:
                print(f"⚠ jsonata-rs failed: {result.stderr}")
                return -1.0

            # Parse output JSON
            output = json.loads(result.stdout)
            return output["elapsed_ms"]

        except subprocess.TimeoutExpired:
            print("⚠ jsonata-rs benchmark timeout")
            return -1.0
        except Exception as e:
            print(f"⚠ jsonata-rs benchmark failed: {e}")
            return -1.0

    def _measure_memory_python(self, func, iterations: int = 100) -> float:
        """Measure peak memory usage of a Python function using tracemalloc."""
        gc.collect()
        tracemalloc.start()

        # Run function multiple times
        for _ in range(iterations):
            func()

        _current, peak = tracemalloc.get_traced_memory()
        tracemalloc.stop()

        return peak / (1024 * 1024)  # Convert to MB

    def _measure_memory_subprocess(self, cmd: list[str], input_data: str | None = None) -> float:
        """Measure peak memory usage of a subprocess (for JS and Rust)."""
        try:
            # Use /usr/bin/time if available (Linux)
            result = subprocess.run(
                ["/usr/bin/time", "-v", *cmd],
                input=input_data,
                capture_output=True,
                text=True,
                timeout=60,
            )
            # Parse maximum resident set size from stderr
            for line in result.stderr.split("\n"):
                if "Maximum resident set size" in line:
                    # Value is in KB
                    kb = int(line.split(":")[1].strip())
                    return kb / 1024  # Convert to MB
        except (FileNotFoundError, subprocess.TimeoutExpired):
            pass

        # Fallback: just return -1 if /usr/bin/time not available
        return -1.0

    def benchmark(
        self,
        name: str,
        category: str,
        expression: str,
        data: Any,
        data_size: str,
        iterations: int = 1000,
        verbose: bool = True,
    ):
        """Run a single benchmark test across all implementations."""
        if verbose:
            print(f"\n{'=' * 70}")
            print(f"Benchmark: {name}")
            print(f"Category: {category}")
            print(f"Expression: {expression[:60]}{'...' if len(expression) > 60 else ''}")
            print(f"Data Size: {data_size}")
            print(f"Iterations: {iterations:,}")
            print(f"{'=' * 70}")

        result = BenchmarkResult(
            name=name,
            category=category,
            expression=expression,
            data_size=data_size,
            iterations=iterations,
        )

        # Run jsonatapy benchmark
        if JSONATAPY_AVAILABLE:
            jsonatapy_time = self._run_jsonatapy_benchmark(expression, data, iterations)
            if jsonatapy_time > 0:
                result.jsonatapy_ms = jsonatapy_time
                if verbose:
                    print(
                        f"jsonatapy:       {jsonatapy_time:8.2f} ms ({jsonatapy_time / iterations:8.4f} ms/iter)"
                    )
            else:
                if verbose:
                    print("jsonatapy:       FAILED")
        else:
            if verbose:
                print("jsonatapy:       NOT AVAILABLE")

        # Run jsonatapy (pure Rust JSON path) benchmark
        if JSONATAPY_AVAILABLE:
            jsonatapy_json_time = self._run_jsonatapy_json_benchmark(
                expression, data, iterations
            )
            if jsonatapy_json_time > 0:
                result.jsonatapy_json_ms = jsonatapy_json_time
                if verbose:
                    print(
                        f"jsonatapy(rust): {jsonatapy_json_time:8.2f} ms ({jsonatapy_json_time / iterations:8.4f} ms/iter)"
                    )
            else:
                if verbose:
                    print("jsonatapy(rust): FAILED")
        else:
            if verbose:
                print("jsonatapy(rust): NOT AVAILABLE")

        # Run JavaScript benchmark
        js_time = self._run_js_benchmark(expression, data, iterations)
        if js_time > 0:
            result.js_ms = js_time
            if verbose:
                print(f"JavaScript:      {js_time:8.2f} ms ({js_time / iterations:8.4f} ms/iter)")

            # Calculate speedup vs JS for jsonatapy (Python path)
            if result.jsonatapy_ms and result.jsonatapy_ms > 0:
                result.jsonatapy_speedup = js_time / result.jsonatapy_ms
                if verbose:
                    if result.jsonatapy_speedup > 1:
                        print(f"  → jsonatapy is {result.jsonatapy_speedup:6.2f}x faster than JS")
                    else:
                        print(
                            f"  → jsonatapy is {1 / result.jsonatapy_speedup:6.2f}x slower than JS"
                        )

            # Calculate speedup vs JS for jsonatapy (pure Rust path)
            if result.jsonatapy_json_ms and result.jsonatapy_json_ms > 0:
                result.jsonatapy_json_speedup = js_time / result.jsonatapy_json_ms
                if verbose:
                    if result.jsonatapy_json_speedup > 1:
                        print(
                            f"  → jsonatapy(rust) is {result.jsonatapy_json_speedup:6.2f}x faster than JS"
                        )
                    else:
                        print(
                            f"  → jsonatapy(rust) is {1 / result.jsonatapy_json_speedup:6.2f}x slower than JS"
                        )
        else:
            if verbose:
                print("JavaScript:      SKIPPED")

        # Run jsonata-python benchmark
        if JSONATA_PYTHON_AVAILABLE:
            jsonata_python_time = self._run_jsonata_python_benchmark(expression, data, iterations)
            if jsonata_python_time > 0:
                result.jsonata_python_ms = jsonata_python_time
                if verbose:
                    print(
                        f"jsonata-python:  {jsonata_python_time:8.2f} ms ({jsonata_python_time / iterations:8.4f} ms/iter)"
                    )

                # Calculate speedup vs JS
                if result.js_ms and result.js_ms > 0:
                    result.jsonata_python_speedup = result.js_ms / jsonata_python_time
                    if verbose:
                        if result.jsonata_python_speedup > 1:
                            print(
                                f"  → jsonata-python is {result.jsonata_python_speedup:6.2f}x faster than JS"
                            )
                        else:
                            print(
                                f"  → jsonata-python is {1 / result.jsonata_python_speedup:6.2f}x slower than JS"
                            )
            else:
                if verbose:
                    print("jsonata-python:  FAILED")
        else:
            if verbose:
                print("jsonata-python:  NOT AVAILABLE")

        # Run jsonata-rs benchmark
        if self.jsonata_rs_available:
            jsonata_rs_time = self._run_jsonata_rs_benchmark(expression, data, iterations)
            if jsonata_rs_time > 0:
                result.jsonata_rs_ms = jsonata_rs_time
                if verbose:
                    print(
                        f"jsonata-rs:      {jsonata_rs_time:8.2f} ms ({jsonata_rs_time / iterations:8.4f} ms/iter)"
                    )

                # Calculate speedup vs JS
                if result.js_ms and result.js_ms > 0:
                    result.jsonata_rs_speedup = result.js_ms / jsonata_rs_time
                    if verbose:
                        if result.jsonata_rs_speedup > 1:
                            print(
                                f"  → jsonata-rs is {result.jsonata_rs_speedup:6.2f}x faster than JS"
                            )
                        else:
                            print(
                                f"  → jsonata-rs is {1 / result.jsonata_rs_speedup:6.2f}x slower than JS"
                            )
            else:
                if verbose:
                    print("jsonata-rs:      FAILED")
        else:
            if verbose:
                print("jsonata-rs:      NOT AVAILABLE")

        self.results.append(result)

    def print_summary(self):
        """Print summary of all benchmark results."""
        if self.has_rich:
            self._print_rich_summary()
        else:
            self._print_plain_summary()

    def _print_plain_summary(self):
        """Print plain text summary."""
        print("\n" + "=" * 100)
        print("BENCHMARK SUMMARY")
        print("=" * 100)
        print(f"{'Category':<20} {'Test Name':<30} {'jsonatapy':<12} {'JS':<12} {'speedup':<10}")
        print("-" * 100)

        categories = {}
        for result in self.results:
            if result.category not in categories:
                categories[result.category] = []
            categories[result.category].append(result)

        for category, results in categories.items():
            print(f"\n{category}")
            print("-" * 100)

            for result in results:
                jsonatapy_str = f"{result.jsonatapy_ms:.2f} ms" if result.jsonatapy_ms else "N/A"
                js_str = f"{result.js_ms:.2f} ms" if result.js_ms else "N/A"
                speedup_str = (
                    f"{result.jsonatapy_speedup:.2f}x" if result.jsonatapy_speedup else "N/A"
                )

                print(
                    f"{'':20} {result.name:<30} {jsonatapy_str:<12} {js_str:<12} {speedup_str:<10}"
                )

        # Overall statistics
        print("\n" + "=" * 100)
        print("OVERALL STATISTICS")
        print("=" * 100)

        [r.jsonatapy_ms for r in self.results if r.jsonatapy_ms]
        [r.js_ms for r in self.results if r.js_ms]
        speedups = [r.jsonatapy_speedup for r in self.results if r.jsonatapy_speedup]

        if speedups:
            avg_speedup = sum(speedups) / len(speedups)
            min_speedup = min(speedups)
            max_speedup = max(speedups)

            print(f"Average speedup (jsonatapy vs JS): {avg_speedup:.2f}x")
            print(f"Min speedup: {min_speedup:.2f}x")
            print(f"Max speedup: {max_speedup:.2f}x")

            faster_count = sum(1 for s in speedups if s > 1)
            print(f"Tests where jsonatapy is faster: {faster_count}/{len(speedups)}")

        if JSONATA_PYTHON_AVAILABLE:
            python_speedups = [
                r.jsonata_python_speedup for r in self.results if r.jsonata_python_speedup
            ]
            if python_speedups:
                avg_python_speedup = sum(python_speedups) / len(python_speedups)
                print(f"\nAverage speedup (jsonata-python vs JS): {avg_python_speedup:.2f}x")

    def _print_rich_summary(self):
        """Print rich formatted summary using rich library."""
        from rich.console import Console
        from rich.table import Table

        console = Console()

        console.print("\n[bold cyan]BENCHMARK SUMMARY[/bold cyan]\n")

        # Group by category
        categories = {}
        for result in self.results:
            if result.category not in categories:
                categories[result.category] = []
            categories[result.category].append(result)

        for category, results in categories.items():
            table = Table(
                title=f"[bold]{category}[/bold]", show_header=True, header_style="bold magenta"
            )
            table.add_column("Test Name", style="cyan", width=30)
            table.add_column("jsonatapy", justify="right", style="green")
            table.add_column("rust-only", justify="right", style="bright_green")
            table.add_column("JavaScript", justify="right", style="yellow")

            if JSONATA_PYTHON_AVAILABLE:
                table.add_column("jsonata-py", justify="right", style="blue")

            if self.jsonata_rs_available:
                table.add_column("jsonata-rs", justify="right", style="magenta")

            table.add_column("Speedup vs JS", justify="right", style="bold")

            for result in results:
                jsonatapy_str = f"{result.jsonatapy_ms:.2f} ms" if result.jsonatapy_ms else "N/A"
                jsonatapy_json_str = (
                    f"{result.jsonatapy_json_ms:.2f} ms"
                    if result.jsonatapy_json_ms
                    else "N/A"
                )
                js_str = f"{result.js_ms:.2f} ms" if result.js_ms else "N/A"
                python_str = (
                    f"{result.jsonata_python_ms:.2f} ms" if result.jsonata_python_ms else "N/A"
                )
                rs_str = (
                    f"{result.jsonata_rs_ms:.2f} ms" if result.jsonata_rs_ms else "N/A"
                )

                if result.jsonatapy_speedup:
                    if result.jsonatapy_speedup > 1:
                        speedup_str = f"[green]{result.jsonatapy_speedup:.2f}x faster[/green]"
                    else:
                        speedup_str = f"[red]{1 / result.jsonatapy_speedup:.2f}x slower[/red]"
                else:
                    speedup_str = "N/A"

                row = [result.name, jsonatapy_str, jsonatapy_json_str, js_str]
                if JSONATA_PYTHON_AVAILABLE:
                    row.append(python_str)
                if self.jsonata_rs_available:
                    row.append(rs_str)
                row.append(speedup_str)
                table.add_row(*row)

            console.print(table)
            console.print()

        # Overall statistics
        speedups = [r.jsonatapy_speedup for r in self.results if r.jsonatapy_speedup]

        if speedups:
            stats_table = Table(
                title="[bold]Overall Statistics[/bold]",
                show_header=True,
                header_style="bold magenta",
            )
            stats_table.add_column("Metric", style="cyan")
            stats_table.add_column("Value", justify="right", style="green")

            avg_speedup = sum(speedups) / len(speedups)
            min_speedup = min(speedups)
            max_speedup = max(speedups)
            faster_count = sum(1 for s in speedups if s > 1)

            stats_table.add_row("Average speedup (jsonatapy vs JS)", f"{avg_speedup:.2f}x")
            stats_table.add_row("Min speedup", f"{min_speedup:.2f}x")
            stats_table.add_row("Max speedup", f"{max_speedup:.2f}x")
            stats_table.add_row(
                "Tests where jsonatapy is faster", f"{faster_count}/{len(speedups)}"
            )

            # jsonatapy (pure Rust) stats
            json_speedups = [
                r.jsonatapy_json_speedup for r in self.results if r.jsonatapy_json_speedup
            ]
            if json_speedups:
                avg_json_speedup = sum(json_speedups) / len(json_speedups)
                json_faster = sum(1 for s in json_speedups if s > 1)
                stats_table.add_row(
                    "Average speedup (rust-only vs JS)", f"{avg_json_speedup:.2f}x"
                )
                stats_table.add_row(
                    "Tests where rust-only is faster", f"{json_faster}/{len(json_speedups)}"
                )

            if JSONATA_PYTHON_AVAILABLE:
                python_speedups = [
                    r.jsonata_python_speedup for r in self.results if r.jsonata_python_speedup
                ]
                if python_speedups:
                    avg_python_speedup = sum(python_speedups) / len(python_speedups)
                    stats_table.add_row(
                        "Average speedup (jsonata-python vs JS)", f"{avg_python_speedup:.2f}x"
                    )

            console.print(stats_table)

    def save_results(self, filename: str | None = None):
        """Save results to JSON file."""
        if not self.output_json:
            return

        if filename is None:
            timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
            filename = f"benchmark_results_{timestamp}.json"

        output_dir = Path(__file__).parent.parent / "results"
        output_dir.mkdir(exist_ok=True)

        output_path = output_dir / filename

        data = {
            "timestamp": datetime.now().isoformat(),
            "implementations": {
                "jsonatapy": JSONATAPY_AVAILABLE,
                "javascript": self.node_available,
                "jsonata_python": JSONATA_PYTHON_AVAILABLE,
                "jsonata_rs": self.jsonata_rs_available,
            },
            "results": [asdict(r) for r in self.results],
        }

        with open(output_path, "w") as f:
            json.dump(data, f, indent=2)

        print(f"\n✓ Results saved to {output_path}")
        return output_path

    def generate_graphs(self):
        """Generate performance comparison graphs."""
        if not self.output_graphs or not self.has_matplotlib:
            return

        import matplotlib.pyplot as plt
        import numpy as np

        # Group by category
        categories = {}
        for result in self.results:
            if result.category not in categories:
                categories[result.category] = []
            categories[result.category].append(result)

        # Create output directory
        output_dir = Path(__file__).parent.parent / "results"
        output_dir.mkdir(exist_ok=True)

        # 1. Speedup comparison chart
        _fig, ax = plt.subplots(figsize=(14, 8))

        test_names = []
        speedups = []
        colors = []

        for result in self.results:
            if result.jsonatapy_speedup:
                test_names.append(f"{result.category}:\n{result.name}")
                speedups.append(result.jsonatapy_speedup)
                colors.append("green" if result.jsonatapy_speedup > 1 else "red")

        y_pos = np.arange(len(test_names))
        bars = ax.barh(y_pos, speedups, color=colors, alpha=0.7)

        ax.set_yticks(y_pos)
        ax.set_yticklabels(test_names, fontsize=8)
        ax.set_xlabel("Speedup vs JavaScript (x)", fontsize=10)
        ax.set_title(
            "jsonatapy Performance vs JavaScript Reference", fontsize=12, fontweight="bold"
        )
        ax.axvline(x=1, color="black", linestyle="--", linewidth=0.8, label="Equal performance")
        ax.legend()
        ax.grid(axis="x", alpha=0.3)

        plt.tight_layout()
        speedup_path = output_dir / "speedup_comparison.png"
        plt.savefig(speedup_path, dpi=150, bbox_inches="tight")
        plt.close()

        print(f"✓ Speedup graph saved to {speedup_path}")

        # 2. Category-wise comparison
        _fig, axes = plt.subplots(2, 2, figsize=(16, 12))
        axes = axes.flatten()

        for idx, (category, results) in enumerate(list(categories.items())[:4]):
            if idx >= 4:
                break

            ax = axes[idx]
            test_names_cat = [r.name for r in results]

            x = np.arange(len(test_names_cat))
            width = 0.35

            jsonatapy_times = [r.jsonatapy_ms if r.jsonatapy_ms else 0 for r in results]
            js_times = [r.js_ms if r.js_ms else 0 for r in results]

            ax.bar(
                x - width / 2, jsonatapy_times, width, label="jsonatapy", color="green", alpha=0.7
            )
            ax.bar(x + width / 2, js_times, width, label="JavaScript", color="orange", alpha=0.7)

            if JSONATA_PYTHON_AVAILABLE:
                python_times = [r.jsonata_python_ms if r.jsonata_python_ms else 0 for r in results]
                ax.bar(
                    x + 1.5 * width,
                    python_times,
                    width,
                    label="jsonata-python",
                    color="blue",
                    alpha=0.7,
                )

            ax.set_xlabel("Test", fontsize=9)
            ax.set_ylabel("Time (ms)", fontsize=9)
            ax.set_title(category, fontsize=10, fontweight="bold")
            ax.set_xticks(x)
            ax.set_xticklabels(test_names_cat, rotation=45, ha="right", fontsize=7)
            ax.legend(fontsize=8)
            ax.grid(axis="y", alpha=0.3)

        plt.tight_layout()
        category_path = output_dir / "category_comparison.png"
        plt.savefig(category_path, dpi=150, bbox_inches="tight")
        plt.close()

        print(f"✓ Category comparison graph saved to {category_path}")

        # 3. Overall statistics pie chart
        if speedups:
            _fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(14, 6))

            # Pie chart: faster vs slower
            faster_count = sum(1 for s in speedups if s > 1)
            slower_count = len(speedups) - faster_count

            ax1.pie(
                [faster_count, slower_count],
                labels=["Faster than JS", "Slower than JS"],
                colors=["green", "red"],
                autopct="%1.1f%%",
                startangle=90,
                textprops={"fontsize": 10},
            )
            ax1.set_title(
                "jsonatapy vs JavaScript\n(Number of Tests)", fontsize=11, fontweight="bold"
            )

            # Bar chart: speedup distribution
            speedup_ranges = ["<0.5x", "0.5-1x", "1-2x", "2-5x", "5-10x", ">10x"]
            counts = [
                sum(1 for s in speedups if s < 0.5),
                sum(1 for s in speedups if 0.5 <= s < 1),
                sum(1 for s in speedups if 1 <= s < 2),
                sum(1 for s in speedups if 2 <= s < 5),
                sum(1 for s in speedups if 5 <= s < 10),
                sum(1 for s in speedups if s >= 10),
            ]

            bars = ax2.bar(
                speedup_ranges,
                counts,
                color=["darkred", "red", "yellow", "lightgreen", "green", "darkgreen"],
                alpha=0.7,
            )
            ax2.set_xlabel("Speedup Range", fontsize=10)
            ax2.set_ylabel("Number of Tests", fontsize=10)
            ax2.set_title("Speedup Distribution", fontsize=11, fontweight="bold")
            ax2.grid(axis="y", alpha=0.3)

            # Add value labels on bars
            for bar in bars:
                height = bar.get_height()
                if height > 0:
                    ax2.text(
                        bar.get_x() + bar.get_width() / 2.0,
                        height,
                        f"{int(height)}",
                        ha="center",
                        va="bottom",
                        fontsize=9,
                    )

            plt.tight_layout()
            stats_path = output_dir / "statistics.png"
            plt.savefig(stats_path, dpi=150, bbox_inches="tight")
            plt.close()

            print(f"✓ Statistics graph saved to {stats_path}")


def _run_path_comparison(ecommerce_data: dict, suite: BenchmarkSuite):
    """Compare all 4 evaluation paths on the same data and expression."""
    expressions = [
        ("Filter by category", 'products[category = "Electronics"]'),
        (
            "Complex transformation",
            'products[price > 50 and inStock].{"name": name, "price": price, "vendor": vendor.name}',
        ),
        ("Aggregate", "$sum(products[inStock].price)"),
    ]
    iterations = 500
    json_str = json.dumps(ecommerce_data)
    data_handle = jsonatapy.JsonataData(ecommerce_data)
    data_handle_json = jsonatapy.JsonataData.from_json(json_str)

    for name, expression in expressions:
        compiled = jsonatapy.compile(expression)

        # Warm up all paths
        for _ in range(50):
            compiled.evaluate(ecommerce_data)
            compiled.evaluate_json(json_str)
            compiled.evaluate_with_data(data_handle)
            compiled.evaluate_data_to_json(data_handle_json)

        print(f"\n{'=' * 70}")
        print(f"Path Comparison: {name}")
        print(f"Expression: {expression[:60]}{'...' if len(expression) > 60 else ''}")
        print(f"Data: 100 products, Iterations: {iterations}")
        print(f"{'=' * 70}")

        # Path 1: evaluate(dict)
        start = time.perf_counter()
        for _ in range(iterations):
            compiled.evaluate(ecommerce_data)
        t1 = (time.perf_counter() - start) * 1000

        # Path 2: evaluate_json(str)
        start = time.perf_counter()
        for _ in range(iterations):
            compiled.evaluate_json(json_str)
        t2 = (time.perf_counter() - start) * 1000

        # Path 3: evaluate_with_data(handle)
        start = time.perf_counter()
        for _ in range(iterations):
            compiled.evaluate_with_data(data_handle)
        t3 = (time.perf_counter() - start) * 1000

        # Path 4: evaluate_data_to_json(handle)
        start = time.perf_counter()
        for _ in range(iterations):
            compiled.evaluate_data_to_json(data_handle_json)
        t4 = (time.perf_counter() - start) * 1000

        print(f"  evaluate(dict):             {t1:8.2f} ms ({t1 / iterations:.4f} ms/iter)")
        print(f"  evaluate_json(str):         {t2:8.2f} ms ({t2 / iterations:.4f} ms/iter)")
        print(
            f"  evaluate_with_data(handle): {t3:8.2f} ms ({t3 / iterations:.4f} ms/iter)  [{t1 / t3:.1f}x vs dict]"
        )
        print(
            f"  evaluate_data_to_json():    {t4:8.2f} ms ({t4 / iterations:.4f} ms/iter)  [{t1 / t4:.1f}x vs dict]"
        )

        # Record as benchmark results for the summary
        suite.results.append(
            BenchmarkResult(
                name=f"{name} (data handle)",
                category="Path Comparison",
                expression=expression,
                data_size="100 products",
                iterations=iterations,
                jsonatapy_ms=t3,
            )
        )
        suite.results.append(
            BenchmarkResult(
                name=f"{name} (data→json)",
                category="Path Comparison",
                expression=expression,
                data_size="100 products",
                iterations=iterations,
                jsonatapy_ms=t4,
            )
        )


def main():
    """Run comprehensive benchmark suite."""
    print("=" * 70)
    print("JSONata Comprehensive Benchmark Suite")
    print("=" * 70)
    print("\nAvailable implementations:")
    suite_check = BenchmarkSuite(output_json=False, output_graphs=False)
    print(f"  - jsonatapy (Rust/PyO3): {'✓' if JSONATAPY_AVAILABLE else '✗'}")
    print(f"  - JavaScript (Node.js): {'✓' if suite_check.node_available else '✗'}")
    print(f"  - jsonata-python (rayokota): {'✓' if JSONATA_PYTHON_AVAILABLE else '✗'}")
    print(f"  - jsonata-rs (pure Rust): {'✓' if suite_check.jsonata_rs_available else '✗'}")

    if not JSONATAPY_AVAILABLE:
        print("\n❌ jsonatapy is not available. Please run 'maturin develop' first.")
        return

    suite = BenchmarkSuite(output_json=True, output_graphs=True)

    # ========================================================================
    # PART 1: SIMPLE PATHS (WARM-UP)
    # ========================================================================
    print("\n" + "█" * 70)
    print("PART 1: SIMPLE PATHS (WARM-UP)")
    print("█" * 70)

    suite.benchmark(
        name="Simple Path",
        category="Simple Paths",
        expression="user.name",
        data={"user": {"name": "Alice", "age": 30}},
        data_size="tiny",
        iterations=10000,
    )

    suite.benchmark(
        name="Deep Path (5 levels)",
        category="Simple Paths",
        expression="a.b.c.d.e",
        data={"a": {"b": {"c": {"d": {"e": 42}}}}},
        data_size="tiny",
        iterations=10000,
    )

    suite.benchmark(
        name="Array Index Access",
        category="Simple Paths",
        expression="values[50]",
        data={"values": list(range(100))},
        data_size="100 elements",
        iterations=5000,
    )

    suite.benchmark(
        name="Arithmetic Expression",
        category="Simple Paths",
        expression="price * quantity",
        data={"price": 10.5, "quantity": 3},
        data_size="tiny",
        iterations=10000,
    )

    # ========================================================================
    # PART 2: ARRAY OPERATIONS
    # ========================================================================
    print("\n" + "█" * 70)
    print("PART 2: ARRAY OPERATIONS")
    print("█" * 70)

    # Small arrays (100 elements)
    array_100 = {"values": list(range(100))}

    suite.benchmark(
        name="Array Sum (100 elements)",
        category="Array Operations",
        expression="$sum(values)",
        data=array_100,
        data_size="100 elements",
        iterations=1000,
    )

    suite.benchmark(
        name="Array Max (100 elements)",
        category="Array Operations",
        expression="$max(values)",
        data=array_100,
        data_size="100 elements",
        iterations=1000,
    )

    suite.benchmark(
        name="Array Count (100 elements)",
        category="Array Operations",
        expression="$count(values)",
        data=array_100,
        data_size="100 elements",
        iterations=2000,
    )

    # Medium arrays (1000 elements)
    array_1000 = {"values": list(range(1000))}

    suite.benchmark(
        name="Array Sum (1000 elements)",
        category="Array Operations",
        expression="$sum(values)",
        data=array_1000,
        data_size="1000 elements",
        iterations=200,
    )

    suite.benchmark(
        name="Array Max (1000 elements)",
        category="Array Operations",
        expression="$max(values)",
        data=array_1000,
        data_size="1000 elements",
        iterations=200,
    )

    # Large arrays (10000 elements)
    array_10000 = {"values": list(range(10000))}

    suite.benchmark(
        name="Array Sum (10000 elements)",
        category="Array Operations",
        expression="$sum(values)",
        data=array_10000,
        data_size="10000 elements",
        iterations=50,
    )

    # Array mapping
    products_100 = {
        "products": [
            {"id": i, "name": f"Product {i}", "price": 10.0 + i * 2.5, "inStock": i % 2 == 0}
            for i in range(100)
        ]
    }

    suite.benchmark(
        name="Array Mapping (extract field)",
        category="Array Operations",
        expression="products.price",
        data=products_100,
        data_size="100 objects",
        iterations=1000,
    )

    suite.benchmark(
        name="Array Mapping + Sum",
        category="Array Operations",
        expression="$sum(products.price)",
        data=products_100,
        data_size="100 objects",
        iterations=1000,
    )

    suite.benchmark(
        name="Array Filtering (predicate)",
        category="Array Operations",
        expression="products[price > 100]",
        data=products_100,
        data_size="100 objects",
        iterations=500,
    )

    # ========================================================================
    # PART 3: COMPLEX TRANSFORMATIONS
    # ========================================================================
    print("\n" + "█" * 70)
    print("PART 3: COMPLEX TRANSFORMATIONS")
    print("█" * 70)

    suite.benchmark(
        name="Object Construction (simple)",
        category="Complex Transformations",
        expression='{"fullName": first & " " & last, "age": age}',
        data={"first": "John", "last": "Doe", "age": 30},
        data_size="tiny",
        iterations=5000,
    )

    suite.benchmark(
        name="Object Construction (nested)",
        category="Complex Transformations",
        expression='{"user": {"name": name, "contact": {"email": email, "phone": phone}}}',
        data={"name": "Alice", "email": "alice@example.com", "phone": "555-1234"},
        data_size="tiny",
        iterations=5000,
    )

    suite.benchmark(
        name="Conditional Expression",
        category="Complex Transformations",
        expression='age >= 18 ? "adult" : "minor"',
        data={"age": 25},
        data_size="tiny",
        iterations=5000,
    )

    suite.benchmark(
        name="Multiple Nested Functions",
        category="Complex Transformations",
        expression="$length($uppercase(name))",
        data={"name": "JSONata Performance Test"},
        data_size="tiny",
        iterations=5000,
    )

    # ========================================================================
    # PART 4: DEEP NESTING (10+ LEVELS)
    # ========================================================================
    print("\n" + "█" * 70)
    print("PART 4: DEEP NESTING (10+ LEVELS)")
    print("█" * 70)

    # Create deeply nested structure
    deep_data = {
        "a": {"b": {"c": {"d": {"e": {"f": {"g": {"h": {"i": {"j": {"k": {"l": 42}}}}}}}}}}}
    }

    suite.benchmark(
        name="Deep Path (12 levels)",
        category="Deep Nesting",
        expression="a.b.c.d.e.f.g.h.i.j.k.l",
        data=deep_data,
        data_size="12 levels",
        iterations=5000,
    )

    # Nested arrays
    nested_arrays = {
        "data": [[[[i, i + 1, i + 2] for i in range(0, 30, 3)] for _ in range(3)] for _ in range(3)]
    }

    suite.benchmark(
        name="Nested Array Access",
        category="Deep Nesting",
        expression="data[1][1][1][1]",
        data=nested_arrays,
        data_size="4-level nested arrays",
        iterations=2000,
    )

    # ========================================================================
    # PART 5: STRING OPERATIONS
    # ========================================================================
    print("\n" + "█" * 70)
    print("PART 5: STRING OPERATIONS")
    print("█" * 70)

    suite.benchmark(
        name="String Uppercase",
        category="String Operations",
        expression="$uppercase(name)",
        data={"name": "hello world"},
        data_size="tiny",
        iterations=10000,
    )

    suite.benchmark(
        name="String Lowercase",
        category="String Operations",
        expression="$lowercase(name)",
        data={"name": "HELLO WORLD"},
        data_size="tiny",
        iterations=10000,
    )

    suite.benchmark(
        name="String Length",
        category="String Operations",
        expression="$length(name)",
        data={"name": "JSONata Performance Benchmark Suite"},
        data_size="tiny",
        iterations=10000,
    )

    suite.benchmark(
        name="String Concatenation",
        category="String Operations",
        expression='$join([first, last], " ")',
        data={"first": "John", "last": "Doe"},
        data_size="tiny",
        iterations=5000,
    )

    suite.benchmark(
        name="String Substring",
        category="String Operations",
        expression="$substring(text, 0, 10)",
        data={"text": "This is a long string that we will extract a substring from"},
        data_size="tiny",
        iterations=5000,
    )

    suite.benchmark(
        name="String Contains",
        category="String Operations",
        expression='$contains(text, "JSONata")',
        data={"text": "JSONata is a query and transformation language for JSON"},
        data_size="tiny",
        iterations=5000,
    )

    # ========================================================================
    # PART 6: HIGHER-ORDER FUNCTIONS
    # ========================================================================
    print("\n" + "█" * 70)
    print("PART 6: HIGHER-ORDER FUNCTIONS")
    print("█" * 70)

    numbers_data = {"numbers": list(range(1, 101))}

    suite.benchmark(
        name="$map with lambda",
        category="Higher-Order Functions",
        expression="$map(numbers, function($v) { $v * 2 })",
        data=numbers_data,
        data_size="100 elements",
        iterations=200,
    )

    suite.benchmark(
        name="$filter with lambda",
        category="Higher-Order Functions",
        expression="$filter(numbers, function($v) { $v > 50 })",
        data=numbers_data,
        data_size="100 elements",
        iterations=200,
    )

    suite.benchmark(
        name="$reduce with lambda",
        category="Higher-Order Functions",
        expression="$reduce(numbers, function($acc, $v) { $acc + $v }, 0)",
        data=numbers_data,
        data_size="100 elements",
        iterations=200,
    )

    # ========================================================================
    # PART 7: REALISTIC WORKLOAD (E-COMMERCE PRODUCT CATALOG)
    # ========================================================================
    print("\n" + "█" * 70)
    print("PART 7: REALISTIC WORKLOAD (E-COMMERCE)")
    print("█" * 70)

    # Create realistic e-commerce data
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

    suite.benchmark(
        name="Filter by category",
        category="Realistic Workload",
        expression='products[category = "Electronics"]',
        data=ecommerce_data,
        data_size="100 products",
        iterations=500,
    )

    suite.benchmark(
        name="Calculate total value",
        category="Realistic Workload",
        expression="$sum(products[inStock].price)",
        data=ecommerce_data,
        data_size="100 products",
        iterations=500,
    )

    suite.benchmark(
        name="Complex transformation",
        category="Realistic Workload",
        expression='products[price > 50 and inStock].{"name": name, "price": price, "vendor": vendor.name}',
        data=ecommerce_data,
        data_size="100 products",
        iterations=200,
    )

    suite.benchmark(
        name="Group by category (aggregate)",
        category="Realistic Workload",
        expression="""
            {
                "Electronics": $sum(products[category = "Electronics"].price),
                "Clothing": $sum(products[category = "Clothing"].price),
                "Books": $sum(products[category = "Books"].price),
                "Home": $sum(products[category = "Home"].price)
            }
        """,
        data=ecommerce_data,
        data_size="100 products",
        iterations=200,
    )

    suite.benchmark(
        name="Top rated products",
        category="Realistic Workload",
        expression="$sort(products[rating >= 4], function($l, $r) { $r.rating - $l.rating })",
        data=ecommerce_data,
        data_size="100 products",
        iterations=100,
    )

    # ========================================================================
    # PART 8: EVALUATION PATH COMPARISON (DATA HANDLE OPTIMIZATION)
    # ========================================================================
    print("\n" + "█" * 70)
    print("PART 8: EVALUATION PATH COMPARISON")
    print("█" * 70)

    if JSONATAPY_AVAILABLE:
        _run_path_comparison(ecommerce_data, suite)

    # ========================================================================
    # PRINT SUMMARY AND SAVE RESULTS
    # ========================================================================
    suite.print_summary()
    suite.save_results()
    suite.generate_graphs()

    print("\n" + "=" * 70)
    print("Benchmark suite complete!")
    print("=" * 70)


if __name__ == "__main__":
    main()

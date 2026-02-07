#!/usr/bin/env python3
"""
Enhanced Benchmark Reporting and Visualization

Generates comprehensive reports with:
- Rich colored tables
- Performance comparison charts
- Memory usage analysis
- Regression detection
- HTML export
"""

import json
import sys
from pathlib import Path

try:
    from rich import box
    from rich.console import Console
    from rich.table import Table
    RICH_AVAILABLE = True
except ImportError:
    print("⚠ rich not available - install with: pip install rich")
    RICH_AVAILABLE = False

try:
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    import numpy as np
    MATPLOTLIB_AVAILABLE = True
except ImportError:
    print("⚠ matplotlib not available - install with: pip install matplotlib")
    MATPLOTLIB_AVAILABLE = False


class EnhancedReport:
    """Generate enhanced benchmark reports with visualizations."""

    def __init__(self, results_file: str):
        """Load benchmark results from JSON file."""
        with open(results_file) as f:
            data = json.load(f)

        self.timestamp = data.get("timestamp", "unknown")
        self.implementations = data.get("implementations", {})
        self.results = data.get("results", [])
        self.console = Console() if RICH_AVAILABLE else None

    def print_summary_table(self):
        """Print rich formatted summary table with all implementations."""
        if not RICH_AVAILABLE:
            print("Rich library not available for enhanced tables")
            return

        # Create main comparison table
        table = Table(
            title="[bold cyan]JSONata Performance Comparison[/bold cyan]",
            box=box.ROUNDED,
            show_header=True,
            header_style="bold magenta"
        )

        table.add_column("Category", style="cyan", width=20)
        table.add_column("Test", style="white", width=30)
        table.add_column("jsonatapy", justify="right", style="green")
        table.add_column("JavaScript", justify="right", style="yellow")
        table.add_column("jsonata-python", justify="right", style="blue")
        table.add_column("jsonata-rs", justify="right", style="red")
        table.add_column("Speedup\nvs JS", justify="right", style="bold")

        # Group by category
        categories = {}
        for result in self.results:
            cat = result.get("category", "Unknown")
            if cat not in categories:
                categories[cat] = []
            categories[cat].append(result)

        for category, results in categories.items():
            # Category header
            table.add_row(
                f"[bold]{category}[/bold]",
                "", "", "", "", "", "",
                style="dim"
            )

            for result in results:
                name = result.get("name", "")

                # Format timing columns
                jsonatapy_ms = result.get("jsonatapy_ms")
                js_ms = result.get("js_ms")
                python_ms = result.get("jsonata_python_ms")
                rs_ms = result.get("jsonata_rs_ms")

                jsonatapy_str = f"{jsonatapy_ms:.2f} ms" if jsonatapy_ms else "N/A"
                js_str = f"{js_ms:.2f} ms" if js_ms else "N/A"
                python_str = f"{python_ms:.2f} ms" if python_ms else "N/A"
                rs_str = f"{rs_ms:.2f} ms" if rs_ms else "N/A"

                # Calculate speedup (jsonatapy vs JS)
                speedup = result.get("jsonatapy_speedup")
                if speedup and speedup > 1:
                    speedup_str = f"[green]{speedup:.2f}x faster[/green]"
                elif speedup:
                    speedup_str = f"[red]{1/speedup:.2f}x slower[/red]"
                else:
                    speedup_str = "N/A"

                table.add_row(
                    "",
                    name,
                    jsonatapy_str,
                    js_str,
                    python_str,
                    rs_str,
                    speedup_str
                )

        self.console.print(table)

    def print_statistics(self):
        """Print overall statistics."""
        if not RICH_AVAILABLE:
            return

        stats_table = Table(
            title="[bold cyan]Overall Statistics[/bold cyan]",
            box=box.ROUNDED,
            show_header=True
        )

        stats_table.add_column("Metric", style="cyan")
        stats_table.add_column("Value", style="yellow", justify="right")

        # Calculate statistics
        speedups = [r.get("jsonatapy_speedup") for r in self.results if r.get("jsonatapy_speedup")]

        if speedups:
            avg_speedup = sum(speedups) / len(speedups)
            min_speedup = min(speedups)
            max_speedup = max(speedups)
            faster_count = sum(1 for s in speedups if s > 1)

            stats_table.add_row(
                "Average speedup (jsonatapy vs JS)",
                f"{avg_speedup:.2f}x"
            )
            stats_table.add_row("Min speedup", f"{min_speedup:.2f}x")
            stats_table.add_row("Max speedup", f"{max_speedup:.2f}x")
            stats_table.add_row(
                "Tests where jsonatapy is faster",
                f"{faster_count}/{len(speedups)}"
            )

        # jsonata-python stats
        python_speedups = [r.get("jsonata_python_speedup") for r in self.results
                          if r.get("jsonata_python_speedup")]
        if python_speedups:
            avg_python = sum(python_speedups) / len(python_speedups)
            stats_table.add_row(
                "Average speedup (jsonata-python vs JS)",
                f"{avg_python:.2f}x"
            )

        # jsonata-rs stats
        rs_speedups = [r.get("jsonata_rs_speedup") for r in self.results
                      if r.get("jsonata_rs_speedup")]
        if rs_speedups:
            avg_rs = sum(rs_speedups) / len(rs_speedups)
            stats_table.add_row(
                "Average speedup (jsonata-rs vs JS)",
                f"{avg_rs:.2f}x"
            )

        self.console.print(stats_table)

    def generate_charts(self, output_dir: str = "charts"):
        """Generate performance comparison charts."""
        if not MATPLOTLIB_AVAILABLE:
            print("Matplotlib not available for chart generation")
            return

        output_path = Path(output_dir)
        output_path.mkdir(exist_ok=True)

        # Group by category
        categories = {}
        for result in self.results:
            cat = result.get("category", "Unknown")
            if cat not in categories:
                categories[cat] = []
            categories[cat].append(result)

        # Create comparison chart for each category
        for category, results in categories.items():
            _fig, ax = plt.subplots(figsize=(12, 6))

            names = [r.get("name", "")[:30] for r in results]
            x = np.arange(len(names))
            width = 0.2

            # Collect data for each implementation (replace None with 0)
            jsonatapy_times = [r.get("jsonatapy_ms") or 0 for r in results]
            js_times = [r.get("js_ms") or 0 for r in results]
            python_times = [r.get("jsonata_python_ms") or 0 for r in results]
            rs_times = [r.get("jsonata_rs_ms") or 0 for r in results]

            # Plot bars (only if they have data)
            ax.bar(x - 1.5*width, jsonatapy_times, width, label='jsonatapy', color='green', alpha=0.8)
            ax.bar(x - 0.5*width, js_times, width, label='JavaScript', color='orange', alpha=0.8)
            if any(python_times):
                ax.bar(x + 0.5*width, python_times, width, label='jsonata-python', color='blue', alpha=0.8)
            if any(rs_times):
                ax.bar(x + 1.5*width, rs_times, width, label='jsonata-rs', color='red', alpha=0.8)

            ax.set_xlabel('Test', fontsize=10)
            ax.set_ylabel('Time (ms)', fontsize=10)
            ax.set_title(f'{category} - Performance Comparison', fontsize=12, fontweight='bold')
            ax.set_xticks(x)
            ax.set_xticklabels(names, rotation=45, ha='right', fontsize=8)
            ax.legend(fontsize=9)
            ax.grid(axis='y', alpha=0.3)

            plt.tight_layout()
            chart_file = output_path / f"{category.lower().replace(' ', '_')}.png"
            plt.savefig(chart_file, dpi=150, bbox_inches='tight')
            plt.close()

            print(f"✓ Chart saved: {chart_file}")

        # Create overall speedup chart
        self._create_speedup_chart(output_path)

    def _create_speedup_chart(self, output_path: Path):
        """Create overall speedup comparison chart."""
        _fig, ax = plt.subplots(figsize=(14, 8))

        names = [r.get("name", "")[:40] for r in self.results if r.get("jsonatapy_speedup")]
        speedups = [r.get("jsonatapy_speedup", 0) for r in self.results if r.get("jsonatapy_speedup")]

        # Color code: green for faster, red for slower
        colors = ['green' if s > 1 else 'red' for s in speedups]

        y_pos = np.arange(len(names))
        ax.barh(y_pos, speedups, color=colors, alpha=0.7)
        ax.axvline(x=1, color='black', linestyle='--', linewidth=1, alpha=0.5)

        ax.set_yticks(y_pos)
        ax.set_yticklabels(names, fontsize=8)
        ax.set_xlabel('Speedup vs JavaScript (log scale)', fontsize=10)
        ax.set_xscale('log')
        ax.set_title('jsonatapy Performance vs JavaScript Reference', fontsize=12, fontweight='bold')
        ax.grid(axis='x', alpha=0.3)

        plt.tight_layout()
        chart_file = output_path / "overall_speedup.png"
        plt.savefig(chart_file, dpi=150, bbox_inches='tight')
        plt.close()

        print(f"✓ Speedup chart saved: {chart_file}")


def main():
    """Main entry point."""
    if len(sys.argv) < 2:
        # Find most recent results file
        results_dir = Path(__file__).parent.parent / "results"
        if not results_dir.exists():
            print("No results directory found")
            return

        json_files = sorted(results_dir.glob("benchmark_results_*.json"))
        if not json_files:
            print("No benchmark results found")
            return

        results_file = json_files[-1]
        print(f"Using most recent results: {results_file.name}")
    else:
        results_file = sys.argv[1]

    report = EnhancedReport(results_file)

    print("\n" + "="*70)
    print("ENHANCED BENCHMARK REPORT")
    print("="*70)
    print(f"Timestamp: {report.timestamp}")
    print(f"Implementations: {', '.join(k for k, v in report.implementations.items() if v)}")
    print("="*70 + "\n")

    report.print_summary_table()
    report.print_statistics()

    # Generate charts in the benchmarks/charts directory
    charts_dir = Path(__file__).parent.parent / "charts"
    report.generate_charts(str(charts_dir))

    print("\n✓ Enhanced report complete!")


if __name__ == "__main__":
    main()

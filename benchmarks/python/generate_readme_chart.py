#!/usr/bin/env python3
"""Regenerate the README performance chart from a benchmark_results_*.json file.

Usage: generate_readme_chart.py <results.json> [repo_root]

Writes docs/assets/realistic-workload-{light,dark}.svg under repo_root and
rewrites the marked chart block in repo_root/README.md. repo_root defaults
to this checkout; the benchmark jobs pass the worktree they are about to
commit from, so the README the chart lands in is the one being committed --
never this checkout's, which may be a different ref. The README embeds the
SVGs through a <picture>
element so the viewer's GitHub theme picks one, wrapped in a link to
https://txjmb.github.io/jsonata-core/stable/performance/ (the SVG itself
carries no link -- GitHub renders it as an image, so the anchor has to be
in the README markup around it).

Source data: the "Realistic Workload" category - the five e-commerce dataset
queries, the closest thing the suite has to what a caller actually runs.

Design notes (these are deliberate, not defaults):

- Form is emphasis, not three competing categoricals: jsonatapy and
  jsonata-core carry hues, the jsonata-js reference is de-emphasis gray. It
  is context, not a fourth competitor.
- jsonata-python and jsonata-rs are deliberately NOT plotted. At 5-9 s and
  130-320 ms against jsonatapy's 2-15 ms they would flatten every bar that
  matters into a sliver. The footer says so and links the full table rather
  than quietly dropping them.
- Values are labelled at every bar tip and there are no gridlines: direct
  labels come before gridlines, and a benchmark chart whose numbers are the
  point should show them.
- Presentation attributes only - no <style>, no script, no external font.
  GitHub sanitizes SVG and renders it as an image, so anything else is
  silently dropped. The font is a system stack resolved on the reader's
  machine.
- Palettes are the validated categorical slots 1-2 plus a de-emphasis gray,
  stepped per mode (checked with the data-viz validator: all gates pass in
  both modes; the grays clear 3:1 on their surface).
"""

import json
import re
import sys
from pathlib import Path
from xml.sax.saxutils import escape

CATEGORY = "Realistic Workload"
DOCS_URL = "https://txjmb.github.io/jsonata-core/stable/performance/"
# Absolute, not repo-relative: README.md is also the PyPI long_description
# and the crates.io readme, and PyPI does not resolve relative links.
RAW_BASE = "https://raw.githubusercontent.com/txjmb/jsonata-core/main/docs/assets"

# The generator owns everything between these two markers, so the headline
# multiple quoted in the alt text can never drift from the SVGs beside it -
# the same staleness trap that CHANGELOG.md and docs/performance.md were
# each pulled out of earlier in this project.
BEGIN = "<!-- BEGIN generated performance chart -->"
END = "<!-- END generated performance chart -->"

# Spelled as escapes so ruff's RUF001 (ambiguous-unicode) stays quiet on
# typography we actually want in the rendered SVG.
TIMES = "\u00d7"
EN_DASH = "\u2013"

FONT = "-apple-system, BlinkMacSystemFont, 'Segoe UI', 'Noto Sans', Helvetica, Arial, sans-serif"

THEMES = {
    "light": {
        "surface": "#fcfcfb",
        "text_primary": "#0b0b0b",
        "text_secondary": "#52514e",
        "series": {"py": "#2a78d6", "core": "#eb6834", "js": "#8a8880"},
    },
    "dark": {
        "surface": "#1a1a19",
        "text_primary": "#ffffff",
        "text_secondary": "#c3c2b7",
        "series": {"py": "#3987e5", "core": "#d95926", "js": "#8f8e86"},
    },
}

# Drawn top-to-bottom within each group. Order is fixed: colour follows the
# entity, never its rank, so a run where one gets faster never repaints them.
SERIES = [
    ("py", "jsonatapy (Python)", "jsonatapy_ms"),
    ("core", "jsonata-core (pure Rust)", "jsonata_core_ms"),
    ("js", "jsonata-js (reference)", "js_ms"),
]

# Geometry
WIDTH = 880
PAD_X = 24
LABEL_W = 196  # left gutter: workload names (grows if a name needs it)
VALUE_W = 58  # right gutter: value at bar tip
BAR_H = 14  # <= 24px cap
BAR_GAP = 2  # surface gap between adjacent bars
GROUP_GAP = 20
HEADER_H = 132
FOOTER_H = 34


def fmt_ms(v):
    return f"{v:.1f} ms" if v < 100 else f"{v:.0f} ms"


def geometric_mean(values):
    prod = 1.0
    for v in values:
        prod *= v
    return prod ** (1.0 / len(values))


def load_rows(results_path):
    with open(results_path) as f:
        data = json.load(f)
    rows = []
    for r in data.get("results", []):
        if r.get("category") != CATEGORY:
            continue
        if not r.get("jsonatapy_ms") or not r.get("js_ms"):
            continue
        rows.append(r)
    return rows, data


def build_svg(rows, meta, theme_name, version):
    t = THEMES[theme_name]
    surface, ink, ink2 = t["surface"], t["text_primary"], t["text_secondary"]

    # Only plot series actually present in the data - an older results file
    # predates the jsonata-core column and must still render.
    active = [s for s in SERIES if any(r.get(s[2]) for r in rows)]

    group_h = len(active) * BAR_H + (len(active) - 1) * BAR_GAP
    # Grow the gutter rather than clip: a future benchmark name longer than
    # today's longest must not run off the left edge.
    label_w = max(LABEL_W, max(len(r["name"]) for r in rows) * 6.6 + 20)
    plot_x = PAD_X + label_w
    plot_w = WIDTH - plot_x - VALUE_W - PAD_X
    height = HEADER_H + len(rows) * (group_h + GROUP_GAP) - GROUP_GAP + FOOTER_H

    vmax = max(r[s[2]] for r in rows for s in active if r.get(s[2]))
    scale = plot_w / vmax

    speedup = geometric_mean([r["js_ms"] / r["jsonatapy_ms"] for r in rows])

    o = []
    add = o.append
    add(
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" '
        f'height="{height:.0f}" viewBox="0 0 {WIDTH} {height:.0f}" '
        f'font-family="{FONT}" role="img" '
        f'aria-label="JSONata realistic-workload benchmark: jsonatapy is '
        f'{speedup:.1f} times faster than jsonata-js on average, lower is better">'
    )
    add(f'<rect width="{WIDTH}" height="{height:.0f}" fill="{surface}"/>')

    # ── Header ──────────────────────────────────────────────────────────
    add(
        f'<text x="{PAD_X}" y="30" font-size="15" font-weight="600" '
        f'fill="{ink}">Realistic workloads — 100-product dataset, lower is better</text>'
    )
    add(
        f'<text x="{PAD_X}" y="72" font-size="34" font-weight="700" '
        f'fill="{ink}">{speedup:.1f}{TIMES} faster</text>'
    )
    add(
        f'<text x="{PAD_X}" y="92" font-size="11.5" fill="{ink2}">'
        f"than the jsonata-js reference — geometric mean across the "
        f"{len(rows)} workloads below</text>"
    )

    # Legend (always present for >= 2 series)
    lx = PAD_X
    for key, label, _ in active:
        add(
            f'<rect x="{lx}" y="{HEADER_H - 26}" width="9" height="9" rx="2" '
            f'fill="{t["series"][key]}"/>'
        )
        add(
            f'<text x="{lx + 14}" y="{HEADER_H - 18}" font-size="11.5" '
            f'fill="{ink2}">{escape(label)}</text>'
        )
        lx += 16 + len(label) * 6.0 + 20

    # ── Groups ──────────────────────────────────────────────────────────
    y = HEADER_H
    for r in rows:
        add(
            f'<text x="{plot_x - 12}" y="{y + group_h / 2 + 4:.1f}" '
            f'font-size="12" text-anchor="end" fill="{ink}">'
            f"{escape(r['name'])}</text>"
        )
        by = y
        for key, _, field in active:
            v = r.get(field)
            if v:
                w = max(v * scale, 3.0)
                # 4px rounded data-end, square at the baseline
                rad = min(4.0, w)
                add(
                    f'<path d="M{plot_x} {by} H{plot_x + w - rad:.1f} '
                    f"a{rad:.1f} {rad:.1f} 0 0 1 {rad:.1f} {rad:.1f} "
                    f"V{by + BAR_H - rad:.1f} "
                    f"a{rad:.1f} {rad:.1f} 0 0 1 -{rad:.1f} {rad:.1f} "
                    f'H{plot_x} Z" fill="{t["series"][key]}"/>'
                )
                add(
                    f'<text x="{plot_x + w + 7:.1f}" y="{by + BAR_H - 3.5}" '
                    f'font-size="10.5" fill="{ink2}">{fmt_ms(v)}</text>'
                )
            by += BAR_H + BAR_GAP
        y += group_h + GROUP_GAP

    # ── Footer ──────────────────────────────────────────────────────────
    date = str(meta.get("timestamp", ""))[:10]
    foot = (
        f"jsonatapy {version} · min of {meta.get('repeat_trials', 5)} trials on "
        f"dedicated Apple Silicon · {date} · "
        f"jsonata-python and jsonata-rs omitted for scale — full results in the docs"
    )
    add(
        f'<text x="{PAD_X}" y="{height - 12:.0f}" font-size="10" '
        f'fill="{ink2}">{escape(foot)}</text>'
    )
    add("</svg>")
    return "\n".join(o) + "\n"


README_BLOCK = """{begin}
<!-- Regenerated from the latest benchmark run by
     benchmarks/python/generate_readme_chart.py. Do not hand-edit this block
     or the SVGs it points at; edit the generator instead. -->
<a href="{docs_url}">
  <picture>
    <source media="(prefers-color-scheme: dark)"
            srcset="{raw_base}/realistic-workload-dark.svg">
    <img width="{width}"
         alt="{alt}"
         src="{raw_base}/realistic-workload-light.svg">
  </picture>
</a>

*Click the chart for the full category-by-category tables, including jsonata-python
and jsonata-rs, which are left off the chart because at {omitted_range} they would
flatten everything else into a sliver.*
{end}"""


def omitted_range(rows):
    """Human range for the two implementations the chart leaves out."""
    vals = [r[f] for r in rows for f in ("jsonata_python_ms", "jsonata_rs_ms") if r.get(f)]
    if not vals:
        return "their far larger times"
    lo, hi = min(vals), max(vals)

    def one(v):
        return f"{v / 1000:.0f} s" if v >= 1000 else f"{v:.0f} ms"

    return f"{one(lo)}{EN_DASH}{one(hi)}"


def update_readme(readme_path, rows, speedup):
    """Rewrite the marked block in README.md, alt text and all.

    Returns True if the file changed. Missing markers is an error, not a
    silent no-op: a benchmark job that quietly stops updating the README is
    exactly how the chart would go stale without anyone noticing.
    """
    if not readme_path.exists():
        raise SystemExit(f"::error::{readme_path} does not exist")
    text = readme_path.read_text()
    start, stop = text.find(BEGIN), text.find(END)
    if start == -1 or stop == -1:
        raise SystemExit(f"::error::{readme_path} is missing the {BEGIN} / {END} markers")
    alt = (
        f"Realistic-workload benchmark on a 100-product dataset, lower is "
        f"better: jsonatapy is {speedup:.1f}x faster than the jsonata-js "
        f"reference on the geometric mean of {len(rows)} e-commerce queries, "
        f"and the pure-Rust jsonata-core engine is faster still."
    )
    block = README_BLOCK.format(
        begin=BEGIN,
        end=END,
        docs_url=DOCS_URL,
        raw_base=RAW_BASE,
        width=WIDTH,
        alt=escape(alt, {'"': "&quot;"}),
        omitted_range=omitted_range(rows),
    )
    new = text[:start] + block + text[stop + len(END) :]
    if new == text:
        return False
    readme_path.write_text(new)
    return True


def resolve_version():
    """Version to stamp in the footer.

    Prefers the installed extension (the generator's real home is a
    benchmark job, where the wheel it just measured is importable); falls
    back to the crate manifest so a local run still stamps something true.
    """
    try:
        import jsonatapy

        return f"v{jsonatapy.__version__}"
    except ImportError:
        pass
    manifest = Path(__file__).resolve().parents[2] / "Cargo.toml"
    try:
        for line in manifest.read_text().splitlines():
            m = re.match(r'^version\s*=\s*"([^"]+)"', line)
            if m:
                return f"v{m.group(1)}"
    except OSError:
        pass
    return "dev"


def main():
    if len(sys.argv) < 2:
        print("Usage: generate_readme_chart.py <results.json> [repo_root]", file=sys.stderr)
        return 2
    results_path = Path(sys.argv[1])
    repo_root = (
        Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else Path(__file__).resolve().parents[2]
    )
    out_dir = repo_root / "docs" / "assets"
    out_dir.mkdir(parents=True, exist_ok=True)

    rows, meta = load_rows(results_path)
    if not rows:
        print(
            f"::error::no '{CATEGORY}' rows with both jsonatapy and jsonata-js "
            f"timings in {results_path}",
            file=sys.stderr,
        )
        return 1

    version = resolve_version()

    for theme in ("light", "dark"):
        path = out_dir / f"realistic-workload-{theme}.svg"
        path.write_text(build_svg(rows, meta, theme, version))
        print(f"Wrote {path}")

    readme = repo_root / "README.md"
    speedup = geometric_mean([r["js_ms"] / r["jsonatapy_ms"] for r in rows])
    changed = update_readme(readme, rows, speedup)
    print(f"{'Updated' if changed else 'Unchanged'} {readme}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

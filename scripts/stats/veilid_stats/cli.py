"""Command-line entry point for the veilid stats reporter."""

from __future__ import annotations

import argparse
import os
import sys

from . import __version__, core, render

DESCRIPTION = """\
Turn veilid stats NDJSON into a signal-focused report.

The stats dump is a firehose of cumulative counters, gauges, and rolling
latency summaries. This tool reduces one run to the numbers that drive
optimization decisions — where time goes, how often things run, and whether
anything looks unhealthy — and diffs runs so you can tell whether a change
actually helped.
"""

EPILOG = """\
modes:
  one file    Full report for a single run.
  many files  Comparison: the first file is the baseline, each later run is
              diffed against it (last vs first). The verdict is on per-call
              latency, which is robust to how often an operation ran; call
              counts and total time are shown as context, not judged, because
              they swing with network conditions and retries between runs.

report sections:
  TIME BY OPERATION  Operations ranked by estimated total time
                     (invocations x mean latency) — the optimization targets.
                     Includes DHT ops, crypto calls, and critical-lock hold/wait.
  INVOCATIONS        How often each operation/event fired, and its rate.
  RETRIES            Gated ops that hit a retryable (no-consensus) outcome, and
                     the retry rate; a volume-robust signal for comparisons.
  HEALTH             Signals you want at zero: rejections, flaps, backlogs.
  LEVELS             Gauge distributions (queue depth, in-flight, open records).
  LATENCIES          Latency-only metrics with no call count (workers, relays).
  ANOMALIES          Individual samples that deviate from the run.

examples:
  uv run veilid-stats run.ndjson
  uv run veilid-stats run.ndjson --window 300 --format json -o report.json
  uv run veilid-stats baseline.ndjson candidate.ndjson --labels base,after
"""


def _labels(files, override):
    if override:
        names = [s.strip() for s in override.split(",")]
        if len(names) != len(files):
            raise SystemExit(f"--labels has {len(names)} names but {len(files)} files were given")
        return names
    out, seen = [], {}
    for f in files:
        base = os.path.basename(f)
        for ext in (".ndjson", ".jsonl", ".json"):
            if base.endswith(ext):
                base = base[: -len(ext)]
        seen[base] = seen.get(base, 0) + 1
        out.append(base if seen[base] == 1 else f"{base}#{seen[base]}")
    return out


def main(argv=None):
    p = argparse.ArgumentParser(
        prog="veilid-stats",
        description=DESCRIPTION,
        epilog=EPILOG,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument(
        "files",
        nargs="+",
        metavar="FILE",
        help="NDJSON stats file(s). One = single report; two+ = comparison.",
    )
    p.add_argument(
        "-f",
        "--format",
        choices=("table", "json"),
        default="table",
        help="Output format: 'table' (human, default) or 'json' (for tools).",
    )
    p.add_argument(
        "-o", "--output", metavar="FILE", help="Write the report to FILE instead of stdout."
    )
    p.add_argument(
        "--window",
        type=float,
        metavar="SECONDS",
        help="Only analyze the last SECONDS of each run. Isolates a test "
        "workload from an idle head/tail. Default: the whole run.",
    )
    p.add_argument(
        "--top",
        type=int,
        default=20,
        metavar="N",
        help="Cap ranked sections at N rows (default: 20).",
    )
    p.add_argument(
        "--labels",
        metavar="A,B,...",
        help="Comma-separated run labels, in file order. Default: filenames.",
    )
    p.add_argument("--version", action="version", version=f"veilid-stats {__version__}")
    args = p.parse_args(argv)

    labels = _labels(args.files, args.labels)
    reports = []
    for path, label in zip(args.files, labels):
        if not os.path.exists(path):
            raise SystemExit(f"no such file: {path}")
        samples = core.load_run(path)
        reports.append(core.build_report(samples, label, window=args.window))

    if len(reports) == 1:
        rep = reports[0]
        text = (
            render.to_json({"kind": "single_run", "report": rep})
            if args.format == "json"
            else render.render_single(rep, top=args.top)
        )
    else:
        cmp = core.compare(reports)
        text = (
            render.to_json({"kind": "comparison", "comparison": cmp})
            if args.format == "json"
            else render.render_comparison(cmp, reports[0]["run"]["label"], top=args.top)
        )

    if args.output:
        with open(args.output, "w", encoding="utf-8") as f:
            f.write(text if text.endswith("\n") else text + "\n")
    else:
        sys.stdout.write(text if text.endswith("\n") else text + "\n")


if __name__ == "__main__":
    main()

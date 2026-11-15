"""Format reports as human-readable tables or as a JSON report for tooling."""

from __future__ import annotations

import json


def _strip(s):  # drop trailing zeros and a bare decimal point: "5.000" -> "5"
    return s.rstrip("0").rstrip(".") if "." in s else s


def dur_s(s):
    if s is None:
        return "-"
    if s < 1:
        return f"{s * 1000:.0f}ms"
    if s < 10:
        return f"{s:.1f}s"
    if s < 60:
        return f"{s:.0f}s"
    m, sec = divmod(int(round(s)), 60)
    if m < 60:
        return f"{m}m{sec:02d}s"
    h, m = divmod(m, 60)
    return f"{h}h{m:02d}m"


def dur_us(us):
    """Microseconds as us / ms / s, keeping sub-unit precision (minutes+ via dur_s)."""
    if us is None:
        return "-"
    us = float(us)
    if us < 1000:
        return f"{us:.0f}us"
    if us < 1_000_000:
        return _strip(f"{us / 1000:.3f}") + "ms"
    if us < 60_000_000:
        return _strip(f"{us / 1_000_000:.3f}") + "s"
    return dur_s(us / 1e6)


def bytes_human(n):
    """Byte count as B / KB / MB / GB / TB (1024-based)."""
    if n is None:
        return "-"
    n = float(n)
    for unit in ("B", "KB", "MB", "GB", "TB"):
        if abs(n) < 1024 or unit == "TB":
            return f"{n:.0f}B" if unit == "B" else _strip(f"{n:.3f}") + unit
        n /= 1024


def fmt_unit(v, unit):
    """A value carrying a unit: 'us' -> duration, 'bytes' -> size, else a raw count."""
    if v is None:
        return "-"
    if unit == "us":
        return dur_us(v)
    if unit == "bytes":
        return bytes_human(v)
    return _g(v)


def count(n):
    return "-" if n is None else f"{round(n):,}"


def rate(r):
    if r is None:
        return "-"
    if r >= 10:
        return f"{r:.0f}/s"
    if r >= 1:
        return f"{r:.1f}/s"
    return f"{r:.2f}/s"


def pct(p):
    if p is None:
        return "-"
    if p == float("inf"):
        return "new"
    if p >= 900:  # a ratio reads clearer than a four-digit percentage
        return f"{1 + p / 100:.0f}x"
    return f"{'+' if p >= 0 else ''}{p:.0f}%"


def frac_pct(f):  # a rate in [0,1] as a plain percentage, not a signed delta
    if f is None:
        return "-"
    if f == float("inf"):
        return "new"
    return f"{f * 100:.0f}%"


def _table(headers, rows, right=()):
    if not rows:
        return "  (none)"
    widths = [len(h) for h in headers]
    for row in rows:
        for i, c in enumerate(row):
            widths[i] = max(widths[i], len(str(c)))

    def line(cells):
        return "  ".join(
            (str(c).rjust(widths[i]) if i in right else str(c).ljust(widths[i]))
            for i, c in enumerate(cells)
        ).rstrip()

    out = [line(headers), "  ".join("-" * w for w in widths)]
    out += [line(r) for r in rows]
    return "\n".join("  " + ln for ln in out)


def _section(title, body):
    return f"\n{title}\n{body}\n"


def _alpha(rows, key):
    """Present a section's rows in ascending name order. Selection (--top) still
    happens by magnitude upstream; this only fixes the display order so the
    report is stable line-for-line and diffs cleanly between runs."""
    return sorted(rows, key=key)


def render_single(rep, top=20):
    if rep.get("empty"):
        return f"{rep['run']['label']}: no samples\n"
    r = rep["run"]
    out = [
        f"veilid stats — {r['label']}",
        f"{r['samples']} samples over {dur_s(r['duration_s'])}"
        + (
            f", window {dur_s(r['window_s'])}/{r['window_samples']} samples"
            if r["window_samples"] != r["samples"]
            else ""
        )
        + f", ~{r['interval_s']:.0f}s interval"
        + (f", attachment {r['attachment_state']}" if r["attachment_state"] else ""),
    ]

    ops = [
        [
            f"{o['op']} ({o['dim']})",
            count(o["invocations"]),
            rate(o["rate_per_s"]),
            dur_us(o["mean_us"]),
            dur_us(o["p90_us"]),
            dur_us(o["worst_us"]),
            dur_s(o["est_total_s"]),
        ]
        for o in _alpha(rep["operations"][:top], lambda o: (o["op"], o["dim"]))
    ]
    out.append(
        _section(
            "TIME BY OPERATION  (est. total = invocations x mean; where it's worth optimizing)",
            _table(
                ["operation", "calls", "rate", "mean", "p90", "worst", "est.total"],
                ops,
                right={1, 2, 3, 4, 5, 6},
            ),
        )
    )

    ctr = [
        [c["metric"], count(c["delta"]), rate(c["rate_per_s"])]
        for c in _alpha(rep["counters"][:top], lambda c: c["metric"])
    ]
    out.append(
        _section(
            "INVOCATIONS  (counter growth over the window)",
            _table(["metric", "count", "rate"], ctr, right={1, 2}),
        )
    )

    rt = [
        [r["op"], count(r["ops"]), count(r["retries"]), frac_pct(r["rate"])]
        for r in _alpha(rep["retries"][:top], lambda r: r["op"])
    ]
    out.append(
        _section(
            "RETRIES  (gated ops with a retryable outcome; rate = retries / ops)",
            _table(["operation", "ops", "retries", "rate"], rt, right={1, 2, 3}),
        )
    )

    hl = [
        [h["metric"], fmt_unit(h["value"], h["unit"]), "ok" if h["ok"] else "!"]
        for h in _alpha(rep["health"], lambda h: h["metric"])
    ]
    out.append(
        _section(
            "HEALTH  (want zero; ! = worth a look)", _table(["signal", "value", ""], hl, right={1})
        )
    )

    g = [
        [
            x["metric"],
            fmt_unit(x["min"], x["unit"]),
            fmt_unit(x["mean"], x["unit"]),
            fmt_unit(x["max"], x["unit"]),
            fmt_unit(x["p95"], x["unit"]),
        ]
        for x in _alpha(rep["gauges"][:top], lambda x: x["metric"])
    ]
    out.append(
        _section(
            "LEVELS  (gauge min / mean / max / p95)",
            _table(["metric", "min", "mean", "max", "p95"], g, right={1, 2, 3, 4}),
        )
    )

    lat = [
        [x["metric"], dur_us(x["typical_us"]), dur_us(x["p90_us"]), dur_us(x["worst_us"])]
        for x in _alpha(rep["latencies"][:top], lambda x: x["metric"])
    ]
    out.append(
        _section(
            "LATENCIES  (no call count: workers, relays)",
            _table(["metric", "typical", "p90", "worst"], lat, right={1, 2, 3}),
        )
    )

    an = [
        [
            dur_s(a["t_offset_s"]),
            a["metric"],
            a["kind"],
            f"{fmt_unit(a['value'], a['unit'])} vs ~{fmt_unit(a['typical'], a['unit'])} "
            f"({a['ratio']:.0f}x)",
        ]
        for a in _alpha(rep["anomalies"][:top], lambda a: (a["metric"], a["t_offset_s"]))
    ]
    out.append(
        _section(
            "ANOMALIES  (samples that deviate from the run)",
            _table(["at", "metric", "kind", "detail"], an),
        )
    )
    return "\n".join(out) + "\n"


def _g(v):
    if v is None:
        return "-"
    if v == int(v):
        return f"{int(v):,}"
    return f"{v:,.1f}"


def render_comparison(cmp, base_label, top=20):
    files = cmp["files"]
    labels = [f["label"] for f in files]
    out = [f"veilid stats comparison — baseline: {base_label}"]
    fr = [
        [f["label"], count(f["samples"]), dur_s(f["window_s"]), f["attachment_state"] or "-"]
        for f in files
    ]
    out.append(_section("RUNS", _table(["run", "samples", "window", "attachment"], fr, right={1})))

    def diff_table(title, rows, fmt, judged=True):
        hdr = ["metric"] + labels + ["delta"] + (["verdict"] if judged else [])
        body = []
        for row in sorted(rows, key=lambda r: r["key"]):
            # A row carrying its own unit (e.g. health mixes us and counts)
            # formats per-row; otherwise the whole column shares one formatter.
            f = (lambda v, u=row["unit"]: fmt_unit(v, u)) if "unit" in row else fmt
            cells = [row["key"]] + [f(v) for v in row["values"]] + [pct(row["delta_pct"])]
            if judged:
                cells.append(row["verdict"])
            body.append(cells)
        return _section(title, _table(hdr, body, right=set(range(1, 2 + len(labels)))))

    out.append(
        diff_table(
            "MEAN LATENCY PER CALL  (lower is better; robust to call volume)",
            cmp["op_mean"][:top],
            dur_us,
        )
    )
    out.append(
        diff_table(
            "P90 LATENCY PER CALL  (the tail; lower is better)", cmp["op_p90"][:top], dur_us
        )
    )
    out.append(
        diff_table(
            "WORKER / RELAY LATENCY  (worst; lower is better)", cmp["latencies"][:top], dur_us
        )
    )
    out.append(
        diff_table(
            "RETRY RATE  (retries / ops per kind; lower is better, robust to volume)",
            cmp["retry"][:top],
            frac_pct,
        )
    )
    out.append(
        diff_table(
            "HEALTH  (want zero)", cmp["health"], lambda v: count(v) if v is not None else "-"
        )
    )
    out.append(
        diff_table(
            "CALL VOLUME  (context: varies with network and retries, not a verdict)",
            cmp["op_calls"][:top],
            count,
            judged=False,
        )
    )
    out.append(
        diff_table(
            "TOTAL TIME  (context: calls x mean, so it moves with volume too)",
            cmp["op_total"][:top],
            dur_s,
            judged=False,
        )
    )
    return "\n".join(out) + "\n"


def to_json(payload):
    return json.dumps(payload, indent=2, sort_keys=False)

"""Load NDJSON stats runs and reduce them to a signal-focused report.

The stats dump is a stream of cumulative counters, instantaneous gauges, and
rolling latency summaries. This module discovers those three shapes generically
(no hardcoded schema beyond the latency field names), so instrumentation added
later — lock hold-times, crypto call counts — shows up without changes here.
"""

from __future__ import annotations

import json
from dataclasses import dataclass

# A latency summary is any object with these fields and nothing else.
LATENCY_KEYS = {"fastest", "average", "slowest", "tm90", "tm75", "p90", "p75"}

# Leaf names whose meaning is unambiguous when a series never varies (all-zero
# counters would otherwise look like constant gauges).
COUNTER_LEAVES = {
    "count",
    "added",
    "evicted",
    "rejected_table_full",
    "transitions",
    "state_transitions",
    "total",
    "congested",
    "completed",
    "demand_events",
    "holds",
    "lowers",
    "raises",
    "clamps",
    "entries",
    "time_offline",
}

# Path suffix -> (label, mode, note). Health signals we want at or near zero;
# "peak" reads the gauge's high-water mark, "delta" the growth over the window.
HEALTH = [
    (
        "network_manager.connections.rejected_table_full",
        "conn rejections (table full)",
        "delta",
        "count",
    ),
    ("network_manager.connections.evicted", "conn evictions", "delta", "count"),
    (
        "network_manager.public_internet.transitions",
        "public online/offline flaps",
        "delta",
        "count",
    ),
    ("network_manager.public_internet.time_offline", "public time offline", "delta", "us"),
    ("network_manager.local_network.transitions", "local online/offline flaps", "delta", "count"),
    ("attachment_manager.state_transitions", "attachment state changes", "delta", "count"),
    ("storage_manager.offline_write_records", "records with offline writes", "peak", "count"),
    ("storage_manager.offline_write_subkeys", "offline write backlog (subkeys)", "peak", "count"),
    ("storage_manager.rehydration_requests", "pending rehydrations", "peak", "count"),
    ("rpc_processor.queue_depth", "rpc queue depth", "peak", "count"),
    ("storage_manager.gated_operations_active", "dht ops in flight", "peak", "count"),
]

# Readable path prefixes and list-key rewrites for display.
_PREFIX = {
    "storage_manager": "storage",
    "network_manager": "net",
    "rpc_processor": "rpc",
    "routing_table": "routing",
    "attachment_manager": "attach",
    "stats_manager": "stats",
}


def _num(v):
    if isinstance(v, bool):
        return None
    if isinstance(v, (int, float)):
        return float(v)
    if isinstance(v, str):
        try:
            return float(v)
        except ValueError:
            return None
    return None


def _is_latency(d):
    return isinstance(d, dict) and "average" in d and set(d) <= LATENCY_KEYS


def _list_key(item, index):
    for k in ("kind", "state", "name"):
        if isinstance(item.get(k), str):
            return item[k]
    return str(index)


def _walk(node, path, scalars, latencies):
    if _is_latency(node):
        latencies[path] = {k: _num(v) for k, v in node.items()}
        return
    if isinstance(node, dict):
        for k, v in node.items():
            _walk(v, f"{path}.{k}" if path else k, scalars, latencies)
    elif isinstance(node, list):
        for i, item in enumerate(node):
            if isinstance(item, dict):
                _walk(item, f"{path}[{_list_key(item, i)}]", scalars, latencies)
    else:
        val = _num(node)
        if val is not None:
            scalars[path] = val


@dataclass
class Sample:
    t: float  # seconds
    scalars: dict
    latencies: dict
    state: str | None = None


def load_run(path):
    """Parse an NDJSON stats file into time-ordered samples."""
    samples = []
    with open(path, encoding="utf-8") as f:
        for lineno, line in enumerate(f, 1):
            line = line.strip()
            if not line:
                continue
            try:
                rec = json.loads(line)
            except json.JSONDecodeError as e:
                raise ValueError(f"{path}:{lineno}: invalid JSON: {e}") from e
            comps = rec.get("components", {})
            ts = _num(rec.get("timestamp"))
            scalars, latencies = {}, {}
            _walk(comps, "", scalars, latencies)
            state = comps.get("attachment_manager", {}).get("state")
            samples.append(Sample((ts or 0) / 1e6, scalars, latencies, state))
    samples.sort(key=lambda s: s.t)
    return samples


def pretty(path):
    p = path
    for raw, short in _PREFIX.items():
        p = p.replace(raw, short, 1)
    p = (
        p.replace("gated_operations[", "gate.")
        .replace("process_by_kind[", "")
        .replace("operations[", "")
        .replace("locks[", "lock.")
        .replace("entries_by_state[", "state.")
        .replace("time_in_state[", "in.")
        .replace("]", "")
    )
    return p


def _pct(sorted_vals, q):
    if not sorted_vals:
        return 0.0
    if len(sorted_vals) == 1:
        return sorted_vals[0]
    idx = (len(sorted_vals) - 1) * q / 100.0
    lo = int(idx)
    hi = min(lo + 1, len(sorted_vals) - 1)
    return sorted_vals[lo] * (1 - (idx - lo)) + sorted_vals[hi] * (idx - lo)


def _present(series):
    return [v for v in series if v is not None]


def _pos_increments(series):
    """Positive step sums between consecutive present values (reset-safe)."""
    total, prev = 0.0, None
    steps = []
    for v in series:
        if v is None:
            steps.append(0.0)
            continue
        step = max(0.0, v - prev) if prev is not None else 0.0
        steps.append(step)
        total += step
        prev = v
    return total, steps


def _counter_class(path):
    """event (discrete count) | duration (time accumulator) | bytes (throughput total)."""
    leaf = path.rsplit(".", 1)[-1]
    if leaf in ("uptime", "attached_uptime", "time_offline"):
        return "duration"
    if "time_in_state[" in path and leaf == "total":
        return "duration"
    if "transfer" in path and leaf == "total":
        return "bytes"
    return "event"


def _unit(path):
    """Physical unit of a scalar leaf, for display: 'us' (time), 'bytes', or 'count'."""
    leaf = path.rsplit(".", 1)[-1]
    if leaf in ("uptime", "attached_uptime", "time_offline") or (
        "time_in_state[" in path and leaf == "total"
    ):
        return "us"
    if "transfer" in path or leaf.endswith("_bytes"):
        return "bytes"
    return "count"


def _classify(series):
    vals = _present(series)
    if not vals:
        return "gauge"
    if max(vals) == min(vals):
        return "constant"
    dips = sum(1 for a, b in zip(vals, vals[1:]) if b < a - 1e-9)
    grew = vals[-1] > vals[0]
    if grew and dips <= max(1, 0.1 * len(vals)):
        return "counter"
    return "gauge"


def build_report(samples, label, window=None):
    """Reduce one run to a report dict (raw numeric values; render layer formats)."""
    if not samples:
        return {"run": {"label": label, "samples": 0}, "empty": True}

    t0, t1 = samples[0].t, samples[-1].t
    start = t1 - window if window else t0
    win = [s for s in samples if s.t >= start - 1e-9] or samples
    dur = max(win[-1].t - win[0].t, 1e-9)
    n = len(win)

    scalar_paths = sorted({p for s in win for p in s.scalars})
    latency_paths = sorted({p for s in win for p in s.latencies})
    scols = {p: [s.scalars.get(p) for s in win] for p in scalar_paths}
    lcols = {p: [s.latencies.get(p) for s in win] for p in latency_paths}

    kinds = {p: _classify(scols[p]) for p in scalar_paths}

    # Operations: a latency whose owner has a sibling `count` counter. Total time
    # is integrated per interval (Δcount * concurrent mean latency), so bursts at
    # high latency are weighted correctly rather than smeared by a global average.
    operations = []
    op_latency_paths = set()
    for lp in latency_paths:
        owner, _, dim = lp.rpartition(".")
        cpath = f"{owner}.count"
        if cpath not in scols:
            continue
        counts = scols[cpath]
        avgs = [d["average"] if d else None for d in lcols[lp]]
        inv, steps = _pos_increments(counts)
        if inv <= 0:
            continue
        total_us = sum(step * a for step, a in zip(steps, avgs) if a is not None)
        p90s = sorted(d["p90"] for d in lcols[lp] if d and d["p90"] is not None)
        slow = [d["slowest"] for d in lcols[lp] if d and d["slowest"] is not None]
        op_latency_paths.add(lp)
        operations.append(
            {
                "op": pretty(owner),
                "dim": dim,
                "invocations": round(inv),
                "rate_per_s": inv / dur,
                "mean_us": total_us / inv if inv else 0.0,
                "p90_us": _pct(p90s, 50),
                "worst_us": max(slow) if slow else 0.0,
                "est_total_s": total_us / 1e6,
                "direction": "lower_better",
            }
        )
    operations.sort(key=lambda r: r["est_total_s"], reverse=True)

    # Invocation counters: discrete-event counts only. Duration accumulators
    # (uptime, time-in-state) and byte totals are counters too, but they aren't
    # "how often" signals — throughput shows up in the transfer gauges instead.
    op_count_paths = {f"{lp.rpartition('.')[0]}.count" for lp in op_latency_paths}
    counters = []
    for p in scalar_paths:
        if (
            kinds[p] != "counter"
            or p in op_count_paths
            or p.endswith(".retryable")
            or _counter_class(p) != "event"
        ):
            continue
        delta, _ = _pos_increments(scols[p])
        if delta <= 0:
            continue
        counters.append(
            {
                "metric": pretty(p),
                "delta": round(delta),
                "rate_per_s": delta / dur,
                "direction": "neutral",
            }
        )
    counters.sort(key=lambda r: r["delta"], reverse=True)

    # Retries: operations that ended in a retryable (no-consensus / congested)
    # outcome. The rate normalizes out call volume, so it survives the network
    # variance that makes raw counts noisy between runs.
    retries = []
    for p in scalar_paths:
        if not p.endswith(".retryable"):
            continue
        owner = p[: -len(".retryable")]
        if f"{owner}.count" not in scols:
            continue
        rcount, _ = _pos_increments(scols[p])
        ocount, _ = _pos_increments(scols[f"{owner}.count"])
        retries.append(
            {
                "op": pretty(owner),
                "ops": round(ocount),
                "retries": round(rcount),
                "rate": (rcount / ocount) if ocount else 0.0,
                "direction": "lower_better",
            }
        )
    retries.sort(key=lambda r: (r["rate"], r["retries"]), reverse=True)

    # Gauges: distribution + high-water mark. Constant series carry no
    # distribution, so they are dropped rather than shown as min==max noise.
    gauges = []
    for p in scalar_paths:
        if kinds[p] not in ("gauge", "constant"):
            continue
        vals = _present(scols[p])
        sv = sorted(vals)
        if not sv or sv[0] == sv[-1]:
            continue
        gauges.append(
            {
                "metric": pretty(p),
                "min": sv[0],
                "mean": sum(sv) / len(sv),
                "max": sv[-1],
                "p95": _pct(sv, 95),
                "last": vals[-1],
                "unit": _unit(p),
                "direction": "lower_better",
            }
        )
    gauges.sort(key=lambda r: r["max"], reverse=True)

    # Standalone latencies (no invocation count): workers, relays, future adds.
    latencies = []
    for lp in latency_paths:
        if lp in op_latency_paths:
            continue
        avgs = sorted(d["average"] for d in lcols[lp] if d and d["average"] is not None)
        p90s = sorted(d["p90"] for d in lcols[lp] if d and d["p90"] is not None)
        slow = [d["slowest"] for d in lcols[lp] if d and d["slowest"] is not None]
        if not avgs:
            continue
        latencies.append(
            {
                "metric": pretty(lp),
                "typical_us": _pct(avgs, 50),
                "p90_us": _pct(p90s, 50),
                "worst_us": max(slow) if slow else 0.0,
                "direction": "lower_better",
            }
        )
    latencies.sort(key=lambda r: r["worst_us"], reverse=True)

    health = _health(scols, lcols, kinds, win)
    anomalies = _anomalies(scols, lcols, kinds, win)

    state = next((s.state for s in reversed(win) if s.state), None)

    return {
        "run": {
            "label": label,
            "samples": len(samples),
            "window_samples": n,
            "duration_s": t1 - t0,
            "window_s": dur,
            "interval_s": dur / max(n - 1, 1),
            "attachment_state": state,
        },
        "operations": operations,
        "counters": counters,
        "gauges": gauges,
        "latencies": latencies,
        "health": health,
        "anomalies": anomalies,
        "retries": retries,
    }


def _verdict(base, last, direction, flat_pct=5.0):
    if base is None or last is None:
        return None, "new" if base is None else "gone"
    if base == 0:
        pct = 0.0 if last == 0 else float("inf")
    else:
        pct = (last - base) / abs(base) * 100.0
    if pct == float("inf"):
        return pct, "regressed" if direction == "lower_better" else "changed"
    if abs(pct) < flat_pct:
        return pct, "flat"
    if direction == "lower_better":
        return pct, "improved" if pct < 0 else "regressed"
    return pct, "changed"


def compare(reports):
    """Align runs and diff the last against the first (baseline)."""
    files = [
        {
            k: r["run"].get(k)
            for k in ("label", "samples", "duration_s", "window_s", "attachment_state")
        }
        for r in reports
    ]

    def align(section, key_fn, val_fn, direction):
        keys, rows = [], {}
        for r in reports:
            for row in r.get(section, []):
                k = key_fn(row)
                if k not in rows:
                    rows[k] = {}
                    keys.append(k)
                rows[k][id(r)] = val_fn(row)
        out = []
        for k in keys:
            vals = [rows[k].get(id(r)) for r in reports]
            pct, verd = _verdict(vals[0], vals[-1], direction)
            out.append(
                {
                    "key": k,
                    "values": vals,
                    "delta_pct": pct,
                    "verdict": verd,
                    "direction": direction,
                }
            )
        return out

    # Judge on per-call latency, which is robust to how many times an operation
    # ran. Call volume and total time swing with network conditions and retries
    # across runs, so they are reported as context, never as a verdict.
    opkey = lambda r: f"{r['op']} ({r['dim']})"
    op_mean = align("operations", opkey, lambda r: r["mean_us"], "lower_better")
    op_p90 = align("operations", opkey, lambda r: r["p90_us"], "lower_better")
    op_calls = align("operations", opkey, lambda r: r["invocations"], "context")
    op_total = align("operations", opkey, lambda r: r["est_total_s"], "context")
    lat = align("latencies", lambda r: r["metric"], lambda r: r["worst_us"], "lower_better")
    health = align("health", lambda r: r["metric"], lambda r: r["value"], "lower_better")
    hunit = {h["metric"]: h.get("unit", "count") for rep in reports for h in rep.get("health", [])}
    for row in health:
        row["unit"] = hunit.get(row["key"], "count")
    # Retry rate is a normalized fraction, so unlike call volume it is a fair
    # verdict: a change that cuts retries at equal workload shows up here.
    retry = align("retries", lambda r: r["op"], lambda r: r["rate"], "lower_better")

    def rank_regressions(rows):
        rows.sort(
            key=lambda r: (
                0 if r["verdict"] == "regressed" else 1,
                (
                    -abs(r["delta_pct"])
                    if isinstance(r["delta_pct"], (int, float)) and r["delta_pct"] != float("inf")
                    else -1e18
                ),
            )
        )

    def rank_size(rows, order):
        pos = {r["key"]: i for i, r in enumerate(order)}
        rows.sort(key=lambda r: pos.get(r["key"], 1 << 30))

    rank_regressions(op_mean)
    rank_regressions(op_p90)
    rank_regressions(lat)
    rank_regressions(retry)
    rank_size(op_calls, op_mean)
    rank_size(op_total, op_mean)
    return {
        "files": files,
        "op_mean": op_mean,
        "op_p90": op_p90,
        "op_calls": op_calls,
        "op_total": op_total,
        "latencies": lat,
        "health": health,
        "retry": retry,
    }


def _health(scols, lcols, kinds, win):
    out = []
    for suffix, label, mode, unit in HEALTH:
        path = next((p for p in scols if p == suffix or p.endswith("." + suffix)), None)
        if path is None:
            continue
        vals = _present(scols[path])
        if not vals:
            continue
        val = max(vals) if mode == "peak" else _pos_increments(scols[path])[0]
        out.append(
            {
                "metric": label,
                "value": round(val),
                "mode": mode,
                "unit": unit,
                "ok": val == 0,
                "direction": "lower_better",
            }
        )
    return out


def _anomalies(scols, lcols, kinds, win):
    out = []
    for p, series in scols.items():
        # Throughput naturally bursts; its spikes aren't anomalies worth flagging.
        if kinds[p] not in ("gauge", "constant") or "transfer" in p:
            continue
        vals = _present(series)
        if len(vals) < 4:
            continue
        mean = sum(vals) / len(vals)
        var = sum((v - mean) ** 2 for v in vals) / len(vals)
        std = var**0.5
        if std <= 0:
            continue
        for i, v in enumerate(series):
            if v is None or v <= mean + 3 * std or v <= 0:
                continue
            out.append(
                {
                    "t_offset_s": win[i].t - win[0].t,
                    "metric": pretty(p),
                    "value": v,
                    "typical": mean,
                    "kind": "gauge spike",
                    "unit": _unit(p),
                    "ratio": v / mean if mean else float("inf"),
                }
            )
    for lp, col in lcols.items():
        p90s = sorted(d["p90"] for d in col if d and d["p90"] is not None)
        if not p90s:
            continue
        typ = _pct(p90s, 50)
        slows = [(i, d["slowest"]) for i, d in enumerate(col) if d and d["slowest"]]
        if not slows:
            continue
        i, worst = max(slows, key=lambda x: x[1])
        if typ > 0 and worst > 4 * typ and worst > 100_000:
            out.append(
                {
                    "t_offset_s": win[i].t - win[0].t,
                    "metric": pretty(lp),
                    "value": worst,
                    "typical": typ,
                    "kind": "latency tail",
                    "unit": "us",
                    "ratio": worst / typ,
                }
            )
    out.sort(key=lambda a: a["ratio"], reverse=True)
    return out

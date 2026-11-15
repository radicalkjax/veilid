# veilid-stats

A reporter for the NDJSON produced by veilid's stats logging (`--stats` on
veilid-server, `STATS_PATH` on the flutter bindings). The raw dump is a stream
of cumulative counters, gauges, and rolling latency summaries; this tool pulls
out the numbers that drive optimization decisions and diffs runs so you can see
whether a change helped.

## Run it

```sh
cd scripts/stats
uv run veilid-stats <file.ndjson>                 # single-run report
uv run veilid-stats base.ndjson after.ndjson      # compare (base = first file)
```

No install step — `uv run` builds the env on first use.

## Capturing a run from veilid-server

`veilid-server --stats=<seconds>[,<dir>]` writes one NDJSON record per interval —
into `<dir>/veilid-server.ndjson` when given a directory, or to the `stats` log
facility when not.

Capture a run while the veilid-python suite drives load against it:

```sh
# terminal 1 — stats every second into a directory
mkdir -p ~/testout/run1
cargo run -p veilid-server --bin veilid-server -- --stats=1,$HOME/testout/run1
# add --release for realistic latency numbers

# terminal 2 — drive load, then stop the server (Ctrl-C in terminal 1)
cd veilid-python && uv run pytest -v -s
```

Review the run. `--window` trims the idle attach head so the report reflects the
test window, not the node's whole lifetime:

```sh
cd scripts/stats
uv run veilid-stats ~/testout/run1/veilid-server.ndjson --window 300
```

## Comparing two runs

Capture a second run the same way — typically a baseline and the same workload
after a change — into a separate directory, then diff them:

```sh
mkdir -p ~/testout/run2
cargo run -p veilid-server --bin veilid-server -- --stats=1,$HOME/testout/run2
# re-run: cd veilid-python && uv run pytest -v -s ; then stop the server

cd scripts/stats
uv run veilid-stats \
    ~/testout/run1/veilid-server.ndjson \
    ~/testout/run2/veilid-server.ndjson \
    --labels before,after --window 300
```

The verdict is on per-call latency and retry rate, both robust to how much load
each run happened to see. Pass `-f json -o report.json` for the machine-readable
form.

## What it reports

Every section lists its rows in ascending name order, so a report is stable
line-for-line and diffs cleanly with `diff` from one run to the next. `--top`
still selects the most significant rows by magnitude (est. total time, growth,
change); only the display order is by name. Values carry units — durations as
us/ms/s, sizes as B/KB/MB — while plain counts stay unitless.

Single run:

- **TIME BY OPERATION** — operations, selected by estimated total time
  (`invocations x mean latency`, computed by integrating each interval's call
  growth against the concurrent mean so bursts at high latency are weighted
  correctly) and listed by name. This is where optimization pays off. Covers DHT
  operations, crypto calls (sign/verify/dh/hash/aead/...), and critical-lock
  hold and wait time.
- **INVOCATIONS** — how often each operation/event fired, plus its rate.
- **RETRIES** — gated DHT ops that ended in a retryable (no-consensus) outcome,
  with the retry rate (`retries / ops`). The rate normalizes out call volume, so
  it stays meaningful across runs with different network conditions.
- **HEALTH** — signals you want at zero: connection rejections, online/offline
  flaps, offline-write backlog, pending rehydrations.
- **LEVELS** — gauge distributions (queue depth, in-flight ops, open records).
- **LATENCIES** — latency metrics with no call count (worker/relay pipelines).
- **ANOMALIES** — individual samples that deviate from the run.

Comparison (two or more files): one column per run, diffing the last against
the first. The verdict is on **per-call latency** (mean and p90), which is
robust to how many times an operation ran. Call volume and total time are shown
as context and are never judged — they swing with network conditions and
retries from run to run, and that variance is exactly the noise the per-call
latency view factors out. Retry *rate* (retries / ops), by contrast, is judged:
normalizing by volume makes it a fair verdict on whether a change cut retries.

## Options

- `-f, --format {table,json}` — human table (default) or a JSON report for tools.
- `-o, --output FILE` — write to a file instead of stdout.
- `--window SECONDS` — analyze only the last N seconds of each run, to isolate a
  test workload from an idle head/tail.
- `--top N` — cap ranked sections (default 20).
- `--labels A,B,...` — run labels in file order (default: filenames).

Run `uv run veilid-stats --help` for the full list.

## Schema notes

Metric discovery is generic: any object with the latency-summary fields is
treated as a latency; other numbers are classified as counters or gauges by how
they move across samples. Instrumentation added later (lock hold-times, crypto
call counts) is picked up automatically, and appears in TIME BY OPERATION as
soon as it exposes a `count` alongside a latency.

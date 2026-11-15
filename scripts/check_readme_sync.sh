#!/usr/bin/env bash
# README gate: every crate README carrying cargo-sync-readme markers must match its crate
# documentation. scripts/fmt.sh regenerates them.
set -euo pipefail

cd "$(dirname "$0")/.."
source scripts/_readme_crates.sh

if ! cargo sync-readme --version >/dev/null 2>&1; then
    echo "readme gate FAILED: cargo-sync-readme is not installed. Install it with:" >&2
    echo "  cargo binstall cargo-sync-readme || cargo install --locked cargo-sync-readme" >&2
    exit 1
fi

crates=$(readme_crates)
# A sweep that sees no crates is a broken sweep, not a clean one
if [ -z "$crates" ]; then
    echo "readme gate FAILED: no crate README carries cargo-sync-readme markers." >&2
    exit 1
fi

failed=0
for crate in $crates; do
    if ! (cd "$crate" && cargo sync-readme --check >/dev/null 2>&1); then
        echo "readme gate: $crate/README.md is out of sync with its crate documentation" >&2
        failed=1
    fi
done
if [ "$failed" -ne 0 ]; then
    echo >&2
    echo "readme gate FAILED: run ./scripts/fmt.sh and commit the result." >&2
    exit 1
fi

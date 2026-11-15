#!/usr/bin/env bash
# Rust format gate: fail if any crate is not `cargo fmt` output. scripts/fmt.sh rewrites them.
set -euo pipefail

cd "$(dirname "$0")/.."

if ! cargo fmt --all -- --check; then
    echo >&2
    echo "format gate FAILED: cargo fmt --all --check found unformatted code." >&2
    echo "Run 'cargo fmt --all' and commit the result." >&2
    exit 1
fi

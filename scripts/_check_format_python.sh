#!/usr/bin/env bash
# Python format gate: fail if any Python source is not `black` output. scripts/fmt.sh rewrites
# them. Same roots and config as fmt.sh, from scripts/_python_sources.sh.
set -euo pipefail

cd "$(dirname "$0")/.."
source scripts/_python_sources.sh

if ! python_black --check; then
    echo >&2
    echo "format gate FAILED: the files listed above are not formatted." >&2
    echo "Run ./scripts/fmt.sh and commit the result." >&2
    exit 1
fi

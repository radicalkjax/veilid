#!/usr/bin/env bash
# TypeScript/JavaScript format gate: fail if any source is not `prettier` output. scripts/fmt.sh
# rewrites them. Same globs and config as fmt.sh, from scripts/_typescript_sources.sh.
set -euo pipefail

cd "$(dirname "$0")/.."
source scripts/_typescript_sources.sh

if ! typescript_prettier --check; then
    echo >&2
    echo "format gate FAILED: the files listed above are not formatted." >&2
    echo "Run ./scripts/fmt.sh and commit the result." >&2
    exit 1
fi

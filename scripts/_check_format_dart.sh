#!/usr/bin/env bash
# Dart format gate: fail if any Dart source is not `dart format` output. scripts/fmt.sh rewrites
# them. Same walk and language version as fmt.sh, from scripts/_dart_sources.sh.
set -euo pipefail

cd "$(dirname "$0")/.."
source scripts/_dart_sources.sh

files=$(dart_source_files)
# A sweep that sees no files is a broken sweep, not a clean one
if [ -z "$files" ]; then
    echo "format gate FAILED: no Dart sources found to check." >&2
    echo "The roots in scripts/_dart_sources.sh or the file walk are wrong." >&2
    exit 1
fi

if ! echo "$files" | xargs dart format --language-version="$DART_FORMAT_LANGUAGE_VERSION" \
        --output=none --set-exit-if-changed --; then
    echo >&2
    echo "format gate FAILED: the files listed above are not formatted." >&2
    echo "Run ./scripts/fmt.sh and commit the result." >&2
    exit 1
fi

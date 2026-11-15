#!/usr/bin/env bash
# Format every Dart, Python, TypeScript/JavaScript and Rust source scripts/check_format.sh checks,
# and regenerate the crate READMEs scripts/check_readme_sync.sh checks. Run it, then commit the
# result.
set -euo pipefail

cd "$(dirname "$0")/.."
source scripts/_dart_sources.sh
source scripts/_python_sources.sh
source scripts/_typescript_sources.sh
source scripts/_readme_crates.sh

files=$(dart_source_files)
if [ -z "$files" ]; then
    echo "fmt: no Dart sources found; the roots in scripts/_dart_sources.sh are wrong" >&2
    exit 1
fi

echo "$files" | xargs dart format --language-version="$DART_FORMAT_LANGUAGE_VERSION" --
python_black
typescript_prettier --write --log-level warn
cargo fmt --all

for crate in $(readme_crates); do
    if ! (cd "$crate" && cargo sync-readme --check >/dev/null 2>&1); then
        echo "fmt: regenerating $crate/README.md"
        (cd "$crate" && cargo sync-readme)
    fi
done

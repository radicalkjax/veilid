#!/bin/bash
# Build the dart-flavored veilid-flutter wasm blob and copy it into this
# example's web/wasm, which is where web/index.html loads it from.
#
# Run this before `flutter run -d chrome` (or any web build of the example);
# run_integration_tests_web.sh calls it for you. web/wasm used to be a symlink
# into the debug target directory, which picked builds up for free but pointed
# at only one build mode, so a release build silently served the debug blob.
#
# Arguments are forwarded to wasm_build.sh (e.g. `release`, `--features=...`).

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
set -eo pipefail

APPDIR="$( cd "$SCRIPTDIR/.." >/dev/null 2>&1 && pwd )"
VEILIDFLUTTERDIR="$( cd "$APPDIR/.." >/dev/null 2>&1 && pwd )"
WASMDIR="$APPDIR/web/wasm"

PKGDIR="$("$VEILIDFLUTTERDIR/wasm_build.sh" "$@" | grep SUCCESS:OUTPUTDIR | cut -d= -f2)"
if [ -z "$PKGDIR" ]; then
    echo "wasm build failed" >&2
    exit 1
fi

echo "Updating WASM from $PKGDIR to $WASMDIR"
rm -rf "$WASMDIR"
mkdir -p "$WASMDIR"
cp -f "$PKGDIR"/* "$WASMDIR/"

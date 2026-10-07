#!/bin/bash
# Build the JavaScript/TypeScript wasm bundle for veilid-wasm.
#
# Pipeline: cargo build → wasm-bindgen → wasm-opt (release only).
#
# Usage:
#   ./wasm_build_js.sh                 # debug build (keeps DWARF info)
#   ./wasm_build_js.sh release         # release build (wasm-opt -Oz)
#   ./wasm_build_js.sh release --features=foo
#
# Extra args after the optional `release` token are forwarded to cargo build.

set -eo pipefail

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
pushd "$SCRIPTDIR" &>/dev/null

if [ $# -ge 1 ] && [ "$1" = "release" ]; then
    echo "Release build enabled."
    RELEASE=1
    shift
else
    RELEASE=0
fi

OUTPUTDIR="$SCRIPTDIR/pkg"
WORKSPACE_TARGET="$SCRIPTDIR/../target/wasm32-unknown-unknown"

# --- cargo build -------------------------------------------------------------
if [ "$RELEASE" -eq 1 ]; then
    cargo build --target wasm32-unknown-unknown --release "$@"
    WASM_IN="$WORKSPACE_TARGET/release/veilid_wasm.wasm"
else
    RUSTFLAGS="-g $RUSTFLAGS" cargo build --target wasm32-unknown-unknown --features debug-api "$@"
    WASM_IN="$WORKSPACE_TARGET/debug/veilid_wasm.wasm"
fi

# --- wasm-bindgen ------------------------------------------------------------
mkdir -p "$OUTPUTDIR"
if [ "$RELEASE" -eq 1 ]; then
    wasm-bindgen --target bundler --weak-refs --out-dir "$OUTPUTDIR" "$WASM_IN"
else
    wasm-bindgen --target bundler --weak-refs --out-dir "$OUTPUTDIR" --keep-debug --debug "$WASM_IN"
fi

# --- wasm-opt (release only) ------------------------------------------------
if [ "$RELEASE" -eq 1 ]; then
    wasm-opt \
        --converge \
        --low-memory-unused \
        --flatten \
        --rereloop \
        -Oz \
        "$OUTPUTDIR/veilid_wasm_bg.wasm" \
        -o "$OUTPUTDIR/veilid_wasm_bg.wasm.opt"
    mv "$OUTPUTDIR/veilid_wasm_bg.wasm.opt" "$OUTPUTDIR/veilid_wasm_bg.wasm"
fi

# --- package.json + license + readme ----------------------------------------
VERSION="$(awk -F'"' '/^version = "/ {print $2; exit}' "$SCRIPTDIR/Cargo.toml")"
sed "s/{{VERSION}}/$VERSION/" "$SCRIPTDIR/pkg.template.json" > "$OUTPUTDIR/package.json"
cp -f "$SCRIPTDIR/LICENSE.md" "$OUTPUTDIR/LICENSE.md" 2>/dev/null || true
cp -f "$SCRIPTDIR/README.md" "$OUTPUTDIR/README.md" 2>/dev/null || true

popd &>/dev/null

echo "Built veilid-wasm $VERSION ($( [ $RELEASE -eq 1 ] && echo release || echo debug ))"
echo "Output: $OUTPUTDIR"

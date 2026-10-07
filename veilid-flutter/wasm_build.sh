#!/bin/bash
SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"

set -eo pipefail

get_abs_filename() {
    # $1 : relative filename
    echo "$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
}

pushd $SCRIPTDIR &> /dev/null

if [ $# -ge 1 ] && [ "$1" = "release" ] ; then
    echo "Release build enabled."
    RELEASE=1
    shift
else
    RELEASE=0
fi

# Output filenames: veilid_flutter.js + veilid_flutter_bg.wasm.
if [ "$RELEASE" -eq 1 ]; then
    OUTPUTDIR=$SCRIPTDIR/../target/wasm32-unknown-unknown/release/pkg
    INPUTDIR=$SCRIPTDIR/../target/wasm32-unknown-unknown/release

    ./wasm_remap_paths.sh cargo build --target wasm32-unknown-unknown --release --no-default-features --features=default-wasm -p veilid-flutter $@
    mkdir -p $OUTPUTDIR
    wasm-bindgen --out-dir $OUTPUTDIR --target web $INPUTDIR/veilid_flutter.wasm
    wasm-opt \
        --converge \
        --low-memory-unused \
        --flatten \
        --rereloop \
        -Oz \
        $OUTPUTDIR/veilid_flutter_bg.wasm \
        -o $OUTPUTDIR/veilid_flutter_bg.wasm.optimized
    mv $OUTPUTDIR/veilid_flutter_bg.wasm.optimized $OUTPUTDIR/veilid_flutter_bg.wasm
else
    OUTPUTDIR=$SCRIPTDIR/../target/wasm32-unknown-unknown/debug/pkg
    INPUTDIR=$SCRIPTDIR/../target/wasm32-unknown-unknown/debug

    RUSTFLAGS="-O -g $RUSTFLAGS" cargo build --target wasm32-unknown-unknown --no-default-features --features=default-wasm,debug-api -p veilid-flutter $@
    mkdir -p $OUTPUTDIR
    wasm-bindgen --out-dir $OUTPUTDIR --target web --keep-debug --debug $INPUTDIR/veilid_flutter.wasm
fi

popd &> /dev/null

# Print for use with scripts
echo SUCCESS:OUTPUTDIR=$(get_abs_filename $OUTPUTDIR)

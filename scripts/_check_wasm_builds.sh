#!/bin/bash
set -eou pipefail

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
pushd $SCRIPTDIR/.. >/dev/null


if [ $# -ge 1 ] && [ "$1" = "--cleanup" ] ; then
    echo "Cleanup enabled for WASM build."
    CLEANUP=1
    shift
else
    CLEANUP=0
fi

if [ $# -ge 1 ] && [ "$1" = "dart" ] ; then
    echo "Ensuring WASM builds for Dart..."
    veilid-flutter/wasm_build.sh --locked
elif [ $# -ge 1 ] && [ "$1" = "js" ] ; then
    echo "Ensuring WASM builds for JS..."
    veilid-wasm/wasm_build_js.sh --locked
else
    echo "Ensuring WASM builds for both Dart and JS..."
    veilid-flutter/wasm_build.sh --locked
    veilid-wasm/wasm_build_js.sh --locked
fi
if [ $CLEANUP -eq 1 ] ; then
    rm -rf target/wasm32-unknown-unknown
fi


popd >/dev/null
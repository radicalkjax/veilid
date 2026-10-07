#!/bin/bash
set -eou pipefail

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
pushd $SCRIPTDIR/.. >/dev/null

if [ $# -ge 1 ] && [ "$1" = "--cleanup" ] ; then
    echo "Cleanup enabled for clippy."
    CLEANUP=1
    shift
else
    CLEANUP=0
fi

echo "Running clippy for wasm32-unknown-unknown (veilid-wasm typescript bindings)..."
cargo clippy --manifest-path=veilid-wasm/Cargo.toml --target wasm32-unknown-unknown --all-targets --locked $@

echo "Running clippy for wasm32-unknown-unknown (veilid-flutter dart bindings)..."
cargo clippy --manifest-path=veilid-flutter/rust/Cargo.toml --target wasm32-unknown-unknown --no-default-features --features=default-wasm --all-targets --locked $@

if [ $CLEANUP -eq 1 ] ; then
    rm -rf target/wasm32-unknown-unknown
fi

popd >/dev/null

#!/bin/bash
set -o pipefail
SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"

pushd "$SCRIPTDIR" >/dev/null || exit 1

function smoketest() {
    local mode=$1
    shift

    echo "Updating dependencies..."
    cargo update

    echo "Running native smoketest (tokio/$mode)..."
    cargo run --locked $@
    status=$?
    if [ $status -ne 0 ]; then
        echo "Native smoketest (tokio) failed"
        popd >/dev/null
        exit $status
    fi
    echo "Native smoketest (tokio/$mode) passed"

    echo "Running native smoketest (async-std/$mode)..."
    cargo run --locked --no-default-features --features=rt-async-std $@
    status=$?
    if [ $status -ne 0 ]; then
        echo "Native smoketest (async-std/$mode) failed"
        popd >/dev/null
        exit $status
    fi
    echo "Native smoketest (async-std/$mode) passed"
}

# Smoketest for debug and release builds
smoketest "Debug" 
smoketest "Release" "--release"

popd >/dev/null
exit 0

#!/bin/bash
set -eou pipefail

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
pushd $SCRIPTDIR/.. >/dev/null

if ! command -v cargo-msrv > /dev/null 2>&1; then
    echo "cargo-msrv is not installed. Please install it with: cargo install cargo-msrv --locked"
    exit 1
fi

#echo "Checking MSRV for veilid-tools"
#cargo msrv verify --manifest-path=veilid-tools/Cargo.toml

#echo "Checking MSRV for veilid-core"
#cargo msrv verify --manifest-path=veilid-core/Cargo.toml

echo "Checking MSRV for veilid-server"
cargo msrv verify --manifest-path=veilid-server/Cargo.toml &

# echo "Checking MSRV for veilid-cli"
# cargo msrv verify --manifest-path=veilid-cli/Cargo.toml &

# echo "Checking MSRV for veilid-flutter"
# cargo msrv verify --manifest-path=veilid-flutter/rust/Cargo.toml &

wait

popd >/dev/null

#!/bin/bash
SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
pushd $SCRIPTDIR/.. >/dev/null

cargo msrv verify --manifest-path=veilid-tools/Cargo.toml
cargo msrv verify --manifest-path=veilid-core/Cargo.toml
cargo msrv verify --manifest-path=veilid-server/Cargo.toml
cargo msrv verify --manifest-path=veilid-cli/Cargo.toml
cargo msrv verify --manifest-path=veilid-flutter/rust/Cargo.toml

popd >/dev/null

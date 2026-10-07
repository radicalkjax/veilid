#!/bin/bash
set -eou pipefail

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
pushd $SCRIPTDIR/.. >/dev/null

TARGET=${DEFAULT_CARGO_TARGET:-$(rustup default | cut -d- -f2- | cut -d' ' -f1)}

if ! command -v zig > /dev/null 2>&1; then
    echo "zig is not installed. Please install 'zig':"
    echo "  macos: brew install zig"
    echo "  other: https://ziglang.org/download/"
    exit 1
fi

if ! command -v cargo-zigbuild > /dev/null 2>&1; then
    echo "cargo-zigbuild is not installed. Install it with: cargo install cargo-zigbuild --locked"
    exit 1
fi

if [ $# -ge 1 ] && [ "$1" = "--cleanup" ] ; then
    echo "Cleanup enabled for clippy."
    CLEANUP=1
    shift
else
    CLEANUP=0
fi

echo "Running clippy for x86_64-unknown-linux-gnu with veilid-server for async-std..."
cargo-zigbuild clippy --target x86_64-unknown-linux-gnu --all-targets --manifest-path=veilid-server/Cargo.toml --no-default-features --features=default-async-std --locked $@
if [ $CLEANUP -eq 1 ] ; then
    if [ "$TARGET" != "x86_64-unknown-linux-gnu" ] ; then
        rm -rf target/x86_64-unknown-linux-gnu
    fi
fi

popd >/dev/null

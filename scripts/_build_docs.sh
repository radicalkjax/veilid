#!/bin/bash
set -eou pipefail

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
pushd $SCRIPTDIR/.. >/dev/null

if ! command -v cargo-docs-rs > /dev/null 2>&1; then
    echo "cargo-docs-rs is not installed. Please install it with: cargo install cargo-docs-rs --locked"
    exit 1
fi

if [ $# -ge 1 ] && [ "$1" = "--cleanup" ] ; then
    echo "Cleanup enabled for doc build."
    CLEANUP=1
    shift
else
    CLEANUP=0
fi

TOOLCHAIN=${1:-nightly}
TARGET=${DEFAULT_CARGO_TARGET:-$(rustup default | cut -d- -f2- | cut -d' ' -f1)}

echo "Building documentation for target $TARGET with toolchain $TOOLCHAIN..."
cargo test --doc --target $TARGET --locked
if [ $CLEANUP -eq 1 ] ; then
    find target/$TARGET -name "*.d" -delete 2>/dev/null; true
fi

cargo +$TOOLCHAIN docs-rs -p veilid-core --target $TARGET --locked
if [ $CLEANUP -eq 1 ] ; then
    rm -rf target/$TARGET/doc
fi

cargo +$TOOLCHAIN docs-rs -p veilid-tools --target $TARGET --locked
if [ $CLEANUP -eq 1 ] ; then
    rm -rf target/$TARGET/doc
fi

cargo +$TOOLCHAIN docs-rs -p veilid-remote-api --target $TARGET --locked
if [ $CLEANUP -eq 1 ] ; then
    rm -rf target/$TARGET/doc
fi

popd >/dev/null
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

DEFAULT_TARGET=$(rustc --version --verbose | grep host | cut -c 7-)

echo "Running clippy for host ($DEFAULT_TARGET)..."
cargo clippy --target $DEFAULT_TARGET --locked $@

if [ $CLEANUP -eq 1 ] ; then
    rm -rf target/$DEFAULT_TARGET
fi

popd >/dev/null

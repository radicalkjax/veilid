#!/bin/bash
set -eou pipefail

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
pushd $SCRIPTDIR/.. >/dev/null

if [ $# -ge 1 ] && [ "$1" = "--cleanup" ] ; then
    echo "Cleanup enabled for smoketests."
    CLEANUP=1
    shift
else
    CLEANUP=0
fi

echo "=================== Running smoketest for veilid-tools ===================="
( cd veilid-tools/smoketest && ./run_smoketest.sh $@ && if [ $CLEANUP -eq 1 ] ; then rm -rf target/; fi)

echo "=================== Running smoketest for veilid-core ===================="
( cd veilid-core/smoketest && ./run_smoketest.sh $@ && if [ $CLEANUP -eq 1 ] ; then rm -rf target/; fi)

echo "=================== Running smoketest for veilid-remote-api ===================="
( cd veilid-remote-api/smoketest && ./run_smoketest.sh $@ && if [ $CLEANUP -eq 1 ] ; then rm -rf target/; fi)

echo "=================== Smoketests completed ===================="

popd >/dev/null

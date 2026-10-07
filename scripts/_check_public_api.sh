#!/bin/bash
SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
pushd $SCRIPTDIR/.. >/dev/null

# Check the public API and exit if there are changes
PUBLIC_API_CHECK_ONLY=1 scripts/update_public_api.sh
public_api_check_status=$?
if [ "$public_api_check_status" -ne 0 ]; then
    echo "Public API check FAILED. Exiting."
    exit 1
fi
echo "Public API check passed."

popd >/dev/null

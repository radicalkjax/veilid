#!/bin/bash
set -eo pipefail
SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
VEILIDDIR="$( cd "${SCRIPTDIR}/.." >/dev/null 2>&1 && pwd )"

for SCHEMA in "Request" "RecvMessage"; do
    OUT="${SCRIPTDIR}/veilid/schema/${SCHEMA}.json"
    echo -n "Updating ${SCHEMA}..."
    if cargo run --quiet --manifest-path "${VEILIDDIR}/Cargo.toml" \
        -p veilid-remote-api --bin emit_schema -- "${SCHEMA}" > "${OUT}.tmp"; then
        mv "${OUT}.tmp" "${OUT}"
        echo " done."
    else
        rm -f "${OUT}.tmp"
        echo " error!"
        exit 1
    fi
done

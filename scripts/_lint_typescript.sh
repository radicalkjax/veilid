#!/bin/bash
# eslint over veilid-wasm/tests and the example test bench, each from its own config and
# package-lock. The type-aware rules read the generated pkg/veilid_wasm.d.ts, so this runs after
# scripts/_check_wasm_builds.sh js.
set -eou pipefail

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
pushd "$SCRIPTDIR/.." >/dev/null

source "$SCRIPTDIR/_node_pinned.sh"

if [ ! -f veilid-wasm/pkg/veilid_wasm.d.ts ]; then
    echo "lint: veilid-wasm/pkg is not built; run ./scripts/_check_wasm_builds.sh js first" >&2
    exit 1
fi

for dir in veilid-wasm/tests veilid-wasm/example/test-bench; do
    echo "Running eslint for $dir..."
    (cd "$dir" && npm ci --no-audit --no-fund && npx eslint .)
done

popd >/dev/null

#!/bin/bash
# mypy over veilid-python with the mypy its uv.lock pins; [tool.mypy] in its pyproject.toml
# carries the settings and the file list.
set -eou pipefail

SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
pushd "$SCRIPTDIR/.." >/dev/null
source scripts/_python_sources.sh

echo "Running mypy for veilid-python..."
(cd veilid-python && uv run mypy)

popd >/dev/null

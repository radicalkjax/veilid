#!/bin/bash
SCRIPTDIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"

uv run python $SCRIPTDIR/src/veilid/ignore_patterns.py

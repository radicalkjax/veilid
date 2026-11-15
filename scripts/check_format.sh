#!/usr/bin/env bash
# Format gate: every scripts/_check_format_*.sh in sorted order, the one-command counterpart of
# scripts/fmt.sh. Only the _check_format_* scripts; the other scripts/_check* are not format
# checks. Each is announced and the first failure exits with that script's own exit code, the
# way scripts/check_all.sh steps. CI runs the per-language scripts one job each instead.
set -u

cd "$(dirname "$0")/.." || exit 1

# Run a command, announce it, and propagate its exit code on failure
step() {
    local rc
    echo "==> $*"
    "$@"
    rc=$?
    if [ $rc -ne 0 ]; then
        echo "check_format: FAILED (exit $rc): $*" >&2
        exit $rc
    fi
}

# A glob expands sorted, so the order is stable
checks=(scripts/_check_format_*.sh)
if [ ! -e "${checks[0]}" ]; then
    echo "check_format: no scripts/_check_format_*.sh found" >&2
    exit 1
fi
for check in "${checks[@]}"; do
    step "./$check"
done

echo "check_format: all format checks passed."

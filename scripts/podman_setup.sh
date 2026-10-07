#!/usr/bin/env bash
# Idempotently ensure a podman machine exists, is running, and is configured.
#
# Used by dagger CI (see DAGGER.md) and by veilidchat's Linux desktop integration
# tests (../veilidchat/run_integration_tests_linux.sh). Safe to run repeatedly:
# creates the machine if missing, starts it if stopped, (re)applies the config,
# and otherwise no-ops. On native Linux there is no machine to manage.
#
# Usage: scripts/podman_setup.sh [machine-name]   (default: podman-machine-default)
set -euo pipefail

PODMAN_MACHINE="${1:-podman-machine-default}"

# podman uses a managed VM only on macOS/Windows; native Linux runs rootless.
if [ "$(uname -s)" = "Linux" ]; then
    echo "* Native Linux podman: no machine required."
    exit 0
fi

if ! command -v podman >/dev/null 2>&1; then
    echo "error: podman not found on PATH." >&2
    exit 1
fi

if ! podman machine inspect "$PODMAN_MACHINE" >/dev/null 2>&1; then
    echo "* Creating podman machine '$PODMAN_MACHINE' (4 vCPU, 16GB RAM, 50GB disk)"
    # Resources match the gitlab-ci medium runner (4 vCPU, 16GB RAM, 50GB disk).
    if [ "$(uname -s)" = "Darwin" ]; then
        # podman 6 defaults Apple Silicon to libkrun, which needs a separate krunkit binary; pin the built-in applehv.
        podman machine init -m 16384 --cpus 4 --disk-size 50 --provider "${PODMAN_PROVIDER:-applehv}" --now "$PODMAN_MACHINE"
    else
        podman machine init -m 16384 --cpus 4 --disk-size 50 --now "$PODMAN_MACHINE"
    fi
else
    state="$(podman machine inspect "$PODMAN_MACHINE" --format '{{.State}}' 2>/dev/null || true)"
    if [ "$state" = "running" ]; then
        echo "* podman machine '$PODMAN_MACHINE' already running"
    else
        echo "* Starting podman machine '$PODMAN_MACHINE' (was: ${state:-unknown})"
        podman machine start "$PODMAN_MACHINE"
    fi
fi

echo "* Configuring '$PODMAN_MACHINE'"
# iptable_nat: NAT for networked tests. setenforce Permissive: avoid SELinux
# label denials on bind/overlay mounts inside the VM. Both best-effort/idempotent.
podman machine ssh "$PODMAN_MACHINE" sudo modprobe iptable_nat || true
podman machine ssh "$PODMAN_MACHINE" sudo setenforce Permissive || true

echo "* podman machine '$PODMAN_MACHINE' ready."

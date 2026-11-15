#!/bin/sh
# How many crates cargo should compile in parallel, scaled to whatever machine the build landed on.
#
# Every rust job here is compile-bound -- test_unit_tests_all measured 732s compiling against 37s
# actually running tests -- so this is the single knob that tracks runner size. GitLab SaaS runners
# are all 4GB/vCPU, so nproc governs there; the memory term only binds on a core-rich, memory-poor
# host, where two concurrent rustc on veilid-core is a real OOM risk.
#
# CAVEAT: cgroup limits are invisible here -- a container capped at 1 CPU still reports the host's
# nproc. GitLab SaaS linux runners are ephemeral VMs so the count is real, but a cgroup-limited
# self-hosted runner would over-report and oversubscribe. Override with `--CARGO_JOBS=N` there.
set -eu

cpus=$(nproc 2>/dev/null || echo 1)
mem_gb=$(awk '/MemTotal/{printf "%d", $2/1048576}' /proc/meminfo 2>/dev/null || echo 2)

# Budget 2GB per concurrent rustc.
cap=$(( mem_gb / 2 ))
[ "$cap" -lt 1 ] && cap=1
[ "$cpus" -lt 1 ] && cpus=1

if [ "$cpus" -lt "$cap" ]; then echo "$cpus"; else echo "$cap"; fi

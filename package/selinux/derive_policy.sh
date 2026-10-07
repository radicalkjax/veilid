#!/bin/bash
# Derive/refine the veilid-server SELinux policy on an enforcing host.
#
# Puts veilid_server_t in permissive mode ("monitor": labeled and transitioned,
# denials logged not blocked), drives a representative workload (server start +
# veilid-cli IPC connect as the veilid user), then prints audit2allow output to
# merge by hand into veilid_server.te. Re-run after each edit until enforcing is
# clean. Run as root on a SELinux machine (Fedora/RHEL family assumed for dnf).
#
# Pass --enforce to verify the finished policy: global enforcing, domain NOT
# permissive, expect zero denials.
set -uo pipefail

SELF="$(cd "$(dirname "$0")" && pwd)"
MODULE=veilid_server
DOMAIN=veilid_server_t
PATHS=(/usr/bin/veilid-server /etc/veilid-server /var/db/veilid-server)
ENFORCE=0
[ "${1:-}" = "--enforce" ] && ENFORCE=1

[ "$(id -u)" = 0 ] || exec sudo -E "$0" "$@"

# Collect audit AVCs regardless of whether auditd writes /var/log/audit/audit.log
# (some hosts only route audit to the journal).
collect_avc() { # $1 = since time
    if [ -f /var/log/audit/audit.log ]; then
        ausearch -m AVC,USER_AVC,SELINUX_ERR -ts "$1" 2>/dev/null
    else
        journalctl _TRANSPORT=audit --since "$1" --no-pager 2>/dev/null | grep -E "AVC|SELINUX_ERR"
    fi
}

echo "== tooling =="
command -v audit2allow >/dev/null || dnf install -y policycoreutils-python-utils
[ -f /usr/share/selinux/devel/Makefile ] || dnf install -y selinux-policy-devel

echo "== build + (re)load module =="
"$SELF/build_module.sh"
semodule -i "$SELF/${MODULE}.pp"

echo "== relabel daemon paths =="
restorecon -RFv "${PATHS[@]}" >/dev/null || true

if [ "$ENFORCE" = 1 ]; then
    echo "== ENFORCING verification =="
    semanage permissive -d "${DOMAIN}" 2>/dev/null || true
    setenforce 1
else
    echo "== set domain permissive (monitor) =="
    semanage permissive -a "${DOMAIN}" 2>/dev/null || true
fi

echo "== drive workload =="
MARK="$(date '+%H:%M:%S')"
systemctl reset-failed veilid-server 2>/dev/null || true
systemctl restart veilid-server
for _ in $(seq 1 60); do
    journalctl -u veilid-server --since "$MARK" 2>/dev/null | grep -q "PublicInternet ready" && break
    sleep 2
done
journalctl -u veilid-server --since "$MARK" 2>/dev/null | grep -E "PublicInternet ready|FAILURE|panicked|error" | tail -5 || true

# veilid-cli is a cursive TUI; drive it under a pty (python3) as the veilid user
# just long enough to establish the IPC connection, then let timeout end it.
echo "== veilid-cli IPC connect (as veilid user) =="
sudo -u veilid timeout 12 python3 -c 'import pty; pty.spawn(["/usr/bin/veilid-cli"])' \
    </dev/null >/tmp/veilid_cli.out 2>&1 || true
grep -aiE "server version|connect|ipc|attach|state" /tmp/veilid_cli.out | head -5 || true

sleep 2
echo
echo "================ AVC denials since ${MARK} ================"
DEN=$(collect_avc "$MARK" | grep -c "denied" || true)
echo "denial records: ${DEN:-0}"
echo "----- audit2allow -R (interface-aware; review before merging) -----"
collect_avc "$MARK" | grep "denied" | audit2allow -R 2>/dev/null || true
echo "----- audit2allow (raw allow rules) -----"
collect_avc "$MARK" | grep "denied" | audit2allow 2>/dev/null || true
echo
if [ "$ENFORCE" = 1 ]; then
    [ "${DEN:-0}" = 0 ] && echo "PASS: enforcing, zero denials." || echo "FAIL: denials under enforcing (see above)."
else
    echo "Merge needed rules into $SELF/${MODULE}.te, then re-run. Verify with: $0 --enforce"
fi

#!/bin/bash
# Compile the veilid-server SELinux policy module into veilid_server.pp[.bz2].
# Needs the refpolicy devel headers: selinux-policy-devel (Fedora/RHEL) or
# selinux-policy-dev (Debian/Ubuntu).
set -euo pipefail

cd "$(dirname "$0")"
MODULE=veilid_server
MAKEFILE=/usr/share/selinux/devel/Makefile

if [ ! -f "$MAKEFILE" ]; then
    echo "ERROR: $MAKEFILE missing. Install selinux-policy-devel (Fedora) or selinux-policy-dev (Debian/Ubuntu)." >&2
    exit 1
fi

make -f "$MAKEFILE" "${MODULE}.pp"
bzip2 -kf "${MODULE}.pp"
echo "built $(pwd)/${MODULE}.pp and ${MODULE}.pp.bz2"

# veilid-server SELinux policy

Confines `veilid-server` in its own domain (`veilid_server_t`) and lets `veilid-cli`,
run as the `veilid` service user, reach the server's client-API IPC socket. Without it,
on an enforcing host (default Fedora, `targeted`) the daemon is denied access to
`/var/db/veilid-server`.

## Files
- `veilid_server.te` — types and allow rules. One source for both distro families; the
  Fedora-only `init_nnp_daemon_domain` call is `ifdef`-guarded so it also compiles against
  Debian/Ubuntu refpolicy.
- `veilid_server.fc` — labels `/usr/bin/veilid-server` (`veilid_server_exec_t`),
  `/etc/veilid-server` (`veilid_server_conf_t`), `/var/db/veilid-server` (`veilid_server_var_lib_t`).
- `veilid_server.if` — `veilid_server_stream_connect()` for a confined caller to reach the
  IPC socket. Under default `targeted`, `veilid-cli` runs `unconfined_t` and already connects;
  the interface covers confined admins.
- `build_module.sh` — compile to `veilid_server.pp[.bz2]`. Needs `selinux-policy-devel`
  (Fedora/RHEL) or `selinux-policy-dev` (Debian/Ubuntu).
- `derive_policy.sh` — derivation/verification harness (see below).

## Packaging
Each package compiles the `.pp` from this source at build time and installs
`/usr/share/selinux/packages/veilid_server.pp.bz2`. The `.pp` is arch-independent, so one
build covers amd64 and arm64.
- RPM: built on its `rockylinux:9` base (forward-compatible to Fedora). `%post` runs
  `semodule -i` + `restorecon` (guarded by `selinuxenabled`); `%preun` stops the service
  before `%postun` unloads the module.
- DEB: built on its Ubuntu 18.04 base (Debian refpolicy). `postinst` loads the module only
  when SELinux is enabled, otherwise no-ops (Debian/Ubuntu default to AppArmor); `postrm`
  removes it.

Wiring: `Earthfile` (`package-linux-*-{rpm,deb}`) and `.dagger/src/veilid/main.py`
(`package_rpm` / `package_deb`).

## Deriving / updating
Run on a real enforcing host with the package installed. A container won't work: a
systemd service inside one runs `container_init_t` and never transitions to `veilid_server_t`.

```
sudo ./derive_policy.sh            # permissive: load module, run server + cli, print audit2allow
# merge any new rules into veilid_server.te, repeat
sudo ./derive_policy.sh --enforce  # enforcing; expect "PASS: enforcing, zero denials"
```

It sets `veilid_server_t` permissive, restarts the service, drives a `veilid-cli` IPC
connect as the `veilid` user, and reports denials. Where `auditd` doesn't write
`/var/log/audit/audit.log`, it reads AVCs from `journalctl _TRANSPORT=audit`.

## Notes
- `/var/db/veilid-server` defaults to `system_db_t`; the `.fc` plus `restorecon` relabel it.
  Required, or the daemon is denied its own state directory.
- Verified on Fedora 44 Server (enforcing, `targeted`): RPM install, service runs as
  `veilid_server_t`, `veilid-cli` connects over IPC as the `veilid` user, zero denials.

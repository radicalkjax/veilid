Summary: Install a server grade, headless Veilid node
Name: veilid-server
Version: $RELEASE_VERSION
Release: 1
URL: https://veilid.com
Group: System
License: MPL 2.0
Packager: Veilid Foundation, Inc.
Requires: glibc-common >= 2.23
Requires(post): policycoreutils
Requires(postun): policycoreutils
BuildRoot: /rpm-work-dir/veilid-server
BuildArch: $ARCH

%description
A server grade, headless Veilid node

%install
mkdir -p %{buildroot}/usr/bin/
cp /veilid/target/$CARGO_ARCH/release/veilid-server %{buildroot}/usr/bin/veilid-server

mkdir -p %{buildroot}/etc/systemd/system
cp /veilid/package/systemd/veilid-server.service %{buildroot}/etc/systemd/system/veilid-server.service

mkdir -p %{buildroot}/etc/veilid-server
cp /veilid/package/linux/veilid-server.conf %{buildroot}/etc/veilid-server/veilid-server.conf

mkdir -p %{buildroot}/usr/share/selinux/packages
cp /veilid/package/selinux/veilid_server.pp.bz2 %{buildroot}/usr/share/selinux/packages/veilid_server.pp.bz2

%files
/usr/bin/veilid-server
/etc/systemd/system/veilid-server.service
%config(noreplace) /etc/veilid-server/veilid-server.conf
/usr/share/selinux/packages/veilid_server.pp.bz2

%post
adduser --system -U veilid &>/dev/null || true
mkdir -p /var/db/veilid-server/protected_store
mkdir -p /var/db/veilid-server/table_store
mkdir -p /var/db/veilid-server/block_store
mkdir -p /var/db/veilid-server/ipc
chown -R veilid:veilid /var/db/veilid-server
chmod 0750 /var/db/veilid-server/protected_store
chmod 0750 /var/db/veilid-server/table_store
chmod 0750 /var/db/veilid-server/block_store 
chmod 0750 /var/db/veilid-server/ipc
chmod 0750 /var/db/veilid-server
chmod 755 /usr/bin/veilid-server

# Load the SELinux policy module and relabel the daemon paths. No-op if SELinux is off.
if /usr/sbin/selinuxenabled 2>/dev/null; then
    semodule -i /usr/share/selinux/packages/veilid_server.pp.bz2 || :
    /usr/sbin/restorecon -R /usr/bin/veilid-server /etc/veilid-server /var/db/veilid-server || :
fi

systemctl daemon-reload

echo "Congratulations! To start your Veilid node and set it to start at boot, run the command systemctl enable --now veilid-server"

%preun
# On final removal, stop the service before %postun unloads the SELinux module,
# otherwise the still-running daemon is left in an unlabeled domain and denied.
if [ $1 -eq 0 ]; then
    systemctl --no-reload disable --now veilid-server.service >/dev/null 2>&1 || :
fi

%postun
systemctl daemon-reload
# Remove the SELinux policy module on full uninstall.
if [ $1 -eq 0 ] && /usr/sbin/selinuxenabled 2>/dev/null; then
    semodule -r veilid_server 2>/dev/null || :
fi
# On full uninstall, remove the config dir only if empty. A user-modified
# veilid-server.conf is preserved by %config(noreplace) and keeps the dir non-empty.
if [ $1 -eq 0 ]; then
    rmdir /etc/veilid-server 2>/dev/null || :
fi

%posttrans
if systemctl is-active --quiet veilid-server.service; then
    systemctl restart veilid-server.service
else
    echo "Veilid-Server is installed but not currently running. Configure the service to start immediatly and at boot time by running the following command: systemctl enable --now veilid-server.service"
fi

%changelog
* Sat Jun 21 2026 Christien Rioux <chris@veilid.org>
- ship and load an SELinux policy module confining veilid_server_t
* Sun Apr 28 2024 Christien Rioux <chris@veilid.org>
- add ipc directory to installation
* Sun Jul 2 2023 TC <tc@veilid.org>
- experimental RPM building


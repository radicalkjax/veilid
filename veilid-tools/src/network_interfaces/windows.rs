#![cfg(target_os = "windows")]

// Copyright 2018 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under the MIT license <LICENSE-MIT
// http://opensource.org/licenses/MIT> or the Modified BSD license <LICENSE-BSD
// https://opensource.org/licenses/BSD-3-Clause>, at your option. This file may not be copied,
// modified, or distributed except according to those terms. Please review the Licences for the
// specific language governing permissions and limitations relating to use of the SAFE Network
// Software.

use super::*;

use libc::{self, c_ulong, c_void, size_t};
use std::ffi::CStr;
use std::marker::PhantomData;
use std::{io, ptr};
use winapi::shared::ifdef::IfOperStatusUp;
use winapi::shared::ipifcons::{IF_TYPE_SOFTWARE_LOOPBACK, IF_TYPE_TUNNEL};
use winapi::shared::nldef::{
    IpDadStatePreferred, IpPrefixOriginDhcp, IpSuffixOriginDhcp, IpSuffixOriginRandom,
};
use winapi::shared::winerror::{ERROR_BUFFER_OVERFLOW, ERROR_SUCCESS};

use winapi::um::iphlpapi::GetAdaptersAddresses;
use winapi::um::iptypes::{
    GAA_FLAG_INCLUDE_GATEWAYS, GAA_FLAG_INCLUDE_PREFIX, GAA_FLAG_SKIP_ANYCAST,
    GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_FRIENDLY_NAME, GAA_FLAG_SKIP_MULTICAST,
    IP_ADAPTER_ADDRESSES, IP_ADAPTER_PREFIX, IP_ADAPTER_UNICAST_ADDRESS,
};

/// GetAdaptersAddresses reports the size it needs; retries cover a table growing in between
const ADAPTER_TABLE_READ_ATTEMPTS: usize = 4;
const ADAPTER_TABLE_INITIAL_LEN: c_ulong = 16384;

pub struct PlatformSupportWindows {}

impl PlatformSupportWindows {
    pub fn new() -> Self {
        PlatformSupportWindows {}
    }

    fn get_interface_flags(intf: &IpAdapterAddresses) -> InterfaceFlags {
        InterfaceFlags {
            is_loopback: intf.get_flag_loopback(),
            is_running: intf.get_flag_running(),
            is_point_to_point: intf.get_flag_point_to_point(),
            // Windows doesn't typically run CLAT
            is_clat: false,
        }
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "turns the OS's remaining address lifetime into an absolute epoch expiry"
    )]
    fn get_address_flags(addr: &IP_ADAPTER_UNICAST_ADDRESS) -> AddressFlags {
        let ds = addr.DadState;
        let po = addr.PrefixOrigin;
        let so = addr.SuffixOrigin;
        // ValidLifetime is the remaining valid lifetime in seconds; 0xFFFFFFFF means infinite
        let valid_lifetime = addr.ValidLifetime;
        let expiration_secs = if valid_lifetime == u32::MAX {
            None
        } else {
            let now_secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            Some(now_secs.saturating_add(valid_lifetime as u64))
        };
        AddressFlags {
            is_temporary: so == IpSuffixOriginRandom,
            is_dynamic: po == IpPrefixOriginDhcp || so == IpSuffixOriginDhcp,
            is_preferred: ds == IpDadStatePreferred,
            // Windows doesn't surface a CLAT-equivalent address flag
            is_clat: false,
            expiration_secs,
        }
    }

    #[expect(clippy::unused_async)]
    pub async fn get_interfaces(
        &mut self,
        interfaces: &mut BTreeMap<String, NetworkInterface>,
    ) -> io::Result<()> {
        // Iterate all the interfaces
        let windows_interfaces = WindowsInterfaces::try_new()?;
        for windows_interface in windows_interfaces.iter() {
            // Get name
            let intf_name = windows_interface.name();

            // Get flags
            let flags = Self::get_interface_flags(&windows_interface);

            let mut network_interface = NetworkInterface::new(intf_name.clone(), flags);
            network_interface.mtu = windows_interface.mtu();

            // Go through all addresses and add them if appropriate
            for addr in windows_interface.unicast_addresses() {
                let intf_addr = match sockaddr_tools::to_ipaddr(&addr.Address) {
                    None => continue,
                    Some(IpAddr::V4(ipv4_addr)) => {
                        // The first on-link prefix covering the address is its subnet
                        let netmask = windows_interface.prefixes().find_map(|prefix| {
                            match sockaddr_tools::to_ipaddr(&prefix.Address) {
                                Some(IpAddr::V4(p)) => {
                                    v4_prefix_netmask(ipv4_addr, p, prefix.PrefixLength)
                                }
                                _ => None,
                            }
                        });
                        let broadcast =
                            netmask.map(|m| Ipv4Addr::from(u32::from(ipv4_addr) | !u32::from(m)));
                        IfAddr::V4(Ifv4Addr {
                            ip: ipv4_addr,
                            netmask: netmask.unwrap_or(Ipv4Addr::UNSPECIFIED),
                            broadcast,
                        })
                    }
                    Some(IpAddr::V6(ipv6_addr)) => {
                        let netmask = windows_interface.prefixes().find_map(|prefix| {
                            match sockaddr_tools::to_ipaddr(&prefix.Address) {
                                Some(IpAddr::V6(p)) => {
                                    v6_prefix_netmask(ipv6_addr, p, prefix.PrefixLength)
                                }
                                _ => None,
                            }
                        });
                        IfAddr::V6(Ifv6Addr {
                            ip: ipv6_addr,
                            netmask: netmask.unwrap_or(Ipv6Addr::UNSPECIFIED),
                            broadcast: None,
                        })
                    }
                };

                let address_flags = Self::get_address_flags(addr);

                network_interface
                    .addrs
                    .push(InterfaceAddress::new(intf_addr, address_flags))
            }

            for gateway_address in windows_interface.gateway_addresses() {
                network_interface.add_gateway(gateway_address);
            }

            interfaces.insert(intf_name, network_interface);
        }

        Ok(())
    }
}

/// One adapter, borrowed from the `WindowsInterfaces` buffer for `'a`
pub struct IpAdapterAddresses<'a> {
    data: &'a IP_ADAPTER_ADDRESSES,
}

impl IpAdapterAddresses<'_> {
    pub fn name(&self) -> String {
        if self.data.AdapterName.is_null() {
            return String::new();
        }
        // SAFETY: AdapterName is a NUL-terminated string inside the adapter buffer
        unsafe { CStr::from_ptr(self.data.AdapterName) }
            .to_string_lossy()
            .into_owned()
    }

    pub fn prefixes(&self) -> PrefixesIterator<'_> {
        PrefixesIterator {
            _phantom: PhantomData,
            next: self.data.FirstPrefix,
        }
    }

    pub fn unicast_addresses(&self) -> UnicastAddressesIterator<'_> {
        UnicastAddressesIterator {
            _phantom: PhantomData,
            next: self.data.FirstUnicastAddress,
        }
    }

    pub fn get_flag_loopback(&self) -> bool {
        self.data.IfType == IF_TYPE_SOFTWARE_LOOPBACK
    }
    pub fn get_flag_running(&self) -> bool {
        self.data.OperStatus == IfOperStatusUp
    }
    pub fn get_flag_point_to_point(&self) -> bool {
        self.data.IfType == IF_TYPE_TUNNEL
    }
    pub fn mtu(&self) -> Option<u32> {
        // Zero means the adapter reported none
        (self.data.Mtu != 0).then_some(self.data.Mtu)
    }
    pub fn gateway_addresses(&self) -> Vec<IpAddr> {
        let mut out = vec![];
        let mut fga = self.data.FirstGatewayAddress;
        // SAFETY: the gateway list lives inside the adapter buffer and is null-terminated
        while let Some(gateway) = unsafe { fga.as_ref() } {
            if let Some(address) = sockaddr_tools::to_ipaddr(&gateway.Address) {
                out.push(address);
            }
            fga = gateway.Next;
        }
        out
    }
}

struct WindowsInterfaces {
    data: *const IP_ADAPTER_ADDRESSES,
}

impl WindowsInterfaces {
    pub fn try_new() -> io::Result<Self> {
        let mut buffersize: c_ulong = ADAPTER_TABLE_INITIAL_LEN;
        for _ in 0..ADAPTER_TABLE_READ_ATTEMPTS {
            // SAFETY: the buffer is buffersize bytes and freed on every path but success, where
            // the returned owner frees it
            let ifaddrs =
                unsafe { libc::malloc(buffersize as size_t) } as *mut IP_ADAPTER_ADDRESSES;
            if ifaddrs.is_null() {
                return Err(io_error_other!(
                    "Failed to allocate buffer in get_if_addrs()"
                ));
            }
            // SAFETY: ifaddrs is a live buffer of buffersize bytes and buffersize is written back
            let retcode = unsafe {
                GetAdaptersAddresses(
                    0,
                    GAA_FLAG_SKIP_ANYCAST
                        | GAA_FLAG_SKIP_MULTICAST
                        | GAA_FLAG_SKIP_DNS_SERVER
                        | GAA_FLAG_INCLUDE_PREFIX
                        | GAA_FLAG_SKIP_FRIENDLY_NAME
                        | GAA_FLAG_INCLUDE_GATEWAYS,
                    ptr::null_mut(),
                    ifaddrs,
                    &mut buffersize,
                )
            };
            match retcode {
                ERROR_SUCCESS => return Ok(Self { data: ifaddrs }),
                // buffersize now holds the size the table needs
                // SAFETY: the buffer is ours and not yet owned by anything else; buffersize holds the needed size
                ERROR_BUFFER_OVERFLOW => unsafe { libc::free(ifaddrs as *mut c_void) },
                _ => {
                    // SAFETY: the buffer is ours and not yet owned by anything else
                    unsafe { libc::free(ifaddrs as *mut c_void) };
                    // GetAdaptersAddresses returns its error rather than setting GetLastError
                    return Err(io::Error::from_raw_os_error(retcode as i32));
                }
            }
        }
        Err(io_error_other!("adapter table kept changing size"))
    }

    pub fn iter(&self) -> WindowsInterfacesIterator<'_> {
        WindowsInterfacesIterator {
            next: self.data,
            _phantom: PhantomData,
        }
    }
}

impl Drop for WindowsInterfaces {
    fn drop(&mut self) {
        // SAFETY: data is the malloc'd table, freed once; every borrow carries this lifetime
        unsafe { libc::free(self.data as *mut c_void) };
    }
}

pub struct WindowsInterfacesIterator<'a> {
    next: *const IP_ADAPTER_ADDRESSES,
    _phantom: PhantomData<&'a WindowsInterfaces>,
}

impl<'a> Iterator for WindowsInterfacesIterator<'a> {
    type Item = IpAdapterAddresses<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        // SAFETY: next is null or an entry inside the table the owner holds for 'a
        let data = unsafe { self.next.as_ref()? };
        self.next = data.Next;
        Some(IpAdapterAddresses { data })
    }
}

pub struct PrefixesIterator<'a> {
    _phantom: PhantomData<&'a IP_ADAPTER_ADDRESSES>,
    next: *const IP_ADAPTER_PREFIX,
}

impl<'a> Iterator for PrefixesIterator<'a> {
    type Item = &'a IP_ADAPTER_PREFIX;

    fn next(&mut self) -> Option<Self::Item> {
        // SAFETY: next is null or an entry inside the adapter table borrowed for 'a
        let entry = unsafe { self.next.as_ref()? };
        self.next = entry.Next;
        Some(entry)
    }
}

pub struct UnicastAddressesIterator<'a> {
    _phantom: PhantomData<&'a IP_ADAPTER_ADDRESSES>,
    next: *const IP_ADAPTER_UNICAST_ADDRESS,
}

impl<'a> Iterator for UnicastAddressesIterator<'a> {
    type Item = &'a IP_ADAPTER_UNICAST_ADDRESS;

    fn next(&mut self) -> Option<Self::Item> {
        // SAFETY: next is null or an entry inside the adapter table borrowed for 'a
        let entry = unsafe { self.next.as_ref()? };
        self.next = entry.Next;
        Some(entry)
    }
}

/// The netmask for `plen` when `prefix` covers `addr`
fn v4_prefix_netmask(addr: Ipv4Addr, prefix: Ipv4Addr, plen: u32) -> Option<Ipv4Addr> {
    if plen > 32 {
        return None;
    }
    let mask = u32::MAX.checked_shl(32 - plen).unwrap_or(0);
    ((u32::from(addr) & mask) == (u32::from(prefix) & mask)).then(|| Ipv4Addr::from(mask))
}

/// The netmask for `plen` when `prefix` covers `addr`
fn v6_prefix_netmask(addr: Ipv6Addr, prefix: Ipv6Addr, plen: u32) -> Option<Ipv6Addr> {
    if plen > 128 {
        return None;
    }
    let mask = u128::MAX.checked_shl(128 - plen).unwrap_or(0);
    ((u128::from(addr) & mask) == (u128::from(prefix) & mask)).then(|| Ipv6Addr::from(mask))
}

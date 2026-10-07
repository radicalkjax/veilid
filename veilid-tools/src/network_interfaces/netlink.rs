#![cfg(any(target_os = "linux", target_os = "android"))]

use super::*;

use futures_util::stream::TryStreamExt;
use libc::{
    close, if_indextoname, ifreq, ioctl, socket, AF_INET, IFA_F_DADFAILED, IFA_F_DEPRECATED,
    IFA_F_OPTIMISTIC, IFA_F_PERMANENT, IFA_F_TEMPORARY, IFA_F_TENTATIVE, IFF_LOOPBACK,
    IFF_POINTOPOINT, IFF_RUNNING, IFNAMSIZ, IF_NAMESIZE, SIOCGIFFLAGS, SOCK_DGRAM,
};
use netlink_packet_route::address::{AddressAttribute, AddressMessage, CacheInfo};
use netlink_packet_route::route::{RouteAddress, RouteAttribute, RouteMessage};
use netlink_packet_route::AddressFamily;
use rtnetlink::{new_connection_with_socket, Handle};
use std::collections::btree_map::Entry;
cfg_if! {
    if #[cfg(feature="rt-async-std")] {
        use netlink_sys::{SmolSocket as RTNetLinkSocket};
    } else if #[cfg(feature="rt-tokio")] {
        use netlink_sys::{TokioSocket as RTNetLinkSocket};
    } else {
        compile_error!("needs executor implementation");
    }
}
use std::ffi::CStr;
use std::io;
use std::os::raw::c_int;
use tools::*;

fn ifreq_from_name(ifname: &str) -> io::Result<ifreq> {
    let name = ifname.as_bytes();
    if name.len() >= IFNAMSIZ {
        bail_io_error_other!("interface name too long");
    }
    let mut req: ifreq = unsafe { core::mem::zeroed() };
    for (dst, src) in req.ifr_name.iter_mut().zip(name) {
        *dst = *src as libc::c_char;
    }
    Ok(req)
}

fn get_interface_name(index: u32) -> io::Result<String> {
    let mut ifnamebuf = [0u8; (IF_NAMESIZE + 1)];
    cfg_if! {
        if #[cfg(all(any(target_os = "android", target_os="linux"), any(target_arch = "arm", target_arch = "aarch64")))] {
            if unsafe { if_indextoname(index, ifnamebuf.as_mut_ptr()) }.is_null() {
                bail_io_error_other!("if_indextoname returned null");
            }
        } else {
            use std::os::raw::c_char;
            if unsafe { if_indextoname(index, ifnamebuf.as_mut_ptr() as *mut c_char) }.is_null() {
                bail_io_error_other!("if_indextoname returned null");
            }
        }
    }

    let ifnamebuflen = ifnamebuf
        .iter()
        .position(|c| *c == 0u8)
        .ok_or_else(|| io_error_other!("null not found in interface name"))?;
    let ifname_str = CStr::from_bytes_with_nul(&ifnamebuf[0..=ifnamebuflen])
        .map_err(|e| io_error_other!(e))?
        .to_str()
        .map_err(|e| io_error_other!(e))?;
    Ok(ifname_str.to_owned())
}

fn flags_to_address_flags(flags: u32, opt_cache_info: Option<&CacheInfo>) -> AddressFlags {
    // CacheInfo.ifa_valid is the remaining valid lifetime in seconds; u32::MAX means infinite
    let expiration_secs = opt_cache_info.and_then(|ci| {
        if ci.ifa_valid == u32::MAX {
            None
        } else {
            let now_secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            Some(now_secs.saturating_add(ci.ifa_valid as u64))
        }
    });

    AddressFlags {
        is_temporary: (flags & IFA_F_TEMPORARY) != 0,
        is_dynamic: (flags & IFA_F_PERMANENT) == 0,
        is_preferred: (flags
            & (IFA_F_TENTATIVE | IFA_F_DADFAILED | IFA_F_DEPRECATED | IFA_F_OPTIMISTIC))
            == 0,
        // Linux/Android surfaces CLAT via the owning interface, not the address (see InterfaceFlags::is_clat)
        is_clat: false,
        expiration_secs,
    }
}

pub struct PlatformSupportNetlink {
    connection_jh: Option<MustJoinHandle<()>>,
    handle: Option<Handle>,
    gateways: BTreeMap<u32, BTreeSet<IpAddr>>,
}

impl PlatformSupportNetlink {
    pub fn new() -> Self {
        PlatformSupportNetlink {
            connection_jh: None,
            handle: None,
            gateways: BTreeMap::new(),
        }
    }

    // Figure out which interfaces have default routes
    async fn refresh_default_route_interfaces(&mut self) {
        self.gateways.clear();
        let mut routes_req = self
            .handle
            .as_ref()
            .unwrap_or_log()
            .route()
            .get(RouteMessage::default());

        routes_req.message_mut().header.address_family = AddressFamily::Inet;
        let mut routesv4 = routes_req.execute();

        while let Some(routev4) = routesv4.try_next().await.unwrap_or_default() {
            let opt_gateway_ipaddr = routev4.attributes.iter().find_map(|x| match x {
                RouteAttribute::Gateway(RouteAddress::Inet(gateway)) => Some(IpAddr::V4(*gateway)),
                _ => None,
            });
            let oif_index = routev4.attributes.iter().find_map(|x| match x {
                RouteAttribute::Oif(index) => Some(*index),
                _ => None,
            });

            if let (Some(gateway_ipaddr), Some(oif_index)) = (opt_gateway_ipaddr, oif_index) {
                //info!("*** ipv4 route: {:#?}", routev4);
                if routev4.header.destination_prefix_length == 0 {
                    self.gateways
                        .entry(oif_index)
                        .or_default()
                        .insert(gateway_ipaddr);
                }
            }
        }

        let mut routes_req = self
            .handle
            .as_ref()
            .unwrap_or_log()
            .route()
            .get(RouteMessage::default());

        routes_req.message_mut().header.address_family = AddressFamily::Inet6;
        let mut routesv6 = routes_req.execute();

        while let Some(routev6) = routesv6.try_next().await.unwrap_or_default() {
            let opt_gateway_ipaddr = routev6.attributes.iter().find_map(|x| match x {
                RouteAttribute::Gateway(RouteAddress::Inet6(gateway)) => Some(IpAddr::V6(*gateway)),
                _ => None,
            });
            let oif_index = routev6.attributes.iter().find_map(|x| match x {
                RouteAttribute::Oif(index) => Some(*index),
                _ => None,
            });

            if let (Some(gateway_ipaddr), Some(oif_index)) = (opt_gateway_ipaddr, oif_index) {
                //info!("*** ipv6 route: {:#?}", routev6);
                if routev6.header.destination_prefix_length == 0 {
                    self.gateways
                        .entry(oif_index)
                        .or_default()
                        .insert(gateway_ipaddr);
                }
            }
        }
    }

    fn get_interface_flags(&self, ifname: &str) -> io::Result<InterfaceFlags> {
        let mut req = ifreq_from_name(ifname)?;

        let sock = unsafe { socket(AF_INET, SOCK_DGRAM, 0) };
        if sock < 0 {
            return Err(io::Error::last_os_error());
        }

        cfg_if! {
            if #[cfg(any(target_os = "android", target_env = "musl"))] {
                let res = unsafe { ioctl(sock, SIOCGIFFLAGS as i32, &mut req) };
            } else {
                let res = unsafe { ioctl(sock, SIOCGIFFLAGS, &mut req) };
            }
        }
        unsafe { close(sock) };
        if res < 0 {
            return Err(io::Error::last_os_error());
        }

        let flags = unsafe { req.ifr_ifru.ifru_flags } as c_int;

        // Android CLAT (RFC 6877) lives on a virtual interface named `clat4*` or `v4-*`
        let is_clat = ifname.starts_with("clat4") || ifname.starts_with("v4-");

        Ok(InterfaceFlags {
            is_loopback: (flags & IFF_LOOPBACK) != 0,
            is_running: (flags & IFF_RUNNING) != 0,
            is_point_to_point: (flags & IFF_POINTOPOINT) != 0,
            is_clat,
        })
    }

    fn process_address_message_v4(msg: AddressMessage) -> Option<InterfaceAddress> {
        // Get ip address
        let ip = msg.attributes.iter().find_map(|attr| {
            if let AddressAttribute::Address(IpAddr::V4(a)) = attr {
                Some(*a)
            } else {
                None
            }
        })?;

        // get netmask
        let plen = msg.header.prefix_len as i16;
        let mut netmask = [0u8; 4];
        get_netmask_from_prefix_length_v4(&mut netmask, plen);
        let netmask = Ipv4Addr::from(netmask);

        // get broadcast address
        let broadcast = msg.attributes.iter().find_map(|attr| {
            if let AddressAttribute::Broadcast(b) = attr {
                Some(*b)
            } else {
                None
            }
        });

        // get address flags
        let flags: u32 = msg
            .attributes
            .iter()
            .find_map(|attr| {
                if let AddressAttribute::Flags(f) = attr {
                    Some(f.bits())
                } else {
                    None
                }
            })
            .unwrap_or(msg.header.flags.bits() as u32);

        let opt_cache_info = msg.attributes.iter().find_map(|attr| {
            if let AddressAttribute::CacheInfo(ci) = attr {
                Some(ci)
            } else {
                None
            }
        });

        Some(InterfaceAddress::new(
            IfAddr::V4(Ifv4Addr {
                ip,
                // The netmask of the interface.
                netmask,
                // The broadcast address of the interface.
                broadcast,
            }),
            flags_to_address_flags(flags, opt_cache_info),
        ))
    }

    fn process_address_message_v6(msg: AddressMessage) -> Option<InterfaceAddress> {
        // Get ip address
        let ip = msg.attributes.iter().find_map(|attr| {
            if let AddressAttribute::Address(IpAddr::V6(a)) = attr {
                Some(*a)
            } else {
                None
            }
        })?;

        // get netmask
        let plen = msg.header.prefix_len as i16;
        let mut netmask = [0u8; 16];
        get_netmask_from_prefix_length_v6(&mut netmask, plen);
        let netmask = Ipv6Addr::from(netmask);

        // get address flags
        let flags: u32 = msg
            .attributes
            .iter()
            .find_map(|attr| {
                if let AddressAttribute::Flags(f) = attr {
                    Some(f.bits())
                } else {
                    None
                }
            })
            .unwrap_or(msg.header.flags.bits() as u32);

        // Skip addresses going through duplicate address detection, or ones that have failed it
        if ((flags & IFA_F_TENTATIVE) != 0) || ((flags & IFA_F_DADFAILED) != 0) {
            return None;
        }

        let opt_cache_info = msg.attributes.iter().find_map(|attr| {
            if let AddressAttribute::CacheInfo(ci) = attr {
                Some(ci)
            } else {
                None
            }
        });

        Some(InterfaceAddress::new(
            IfAddr::V6(Ifv6Addr {
                ip,
                // The netmask of the interface.
                netmask,
                // The broadcast address of the interface.
                broadcast: None,
            }),
            flags_to_address_flags(flags, opt_cache_info),
        ))
    }

    async fn get_interfaces_internal(
        &mut self,
        interfaces: &mut BTreeMap<String, NetworkInterface>,
    ) -> io::Result<()> {
        // Refresh the routes
        self.refresh_default_route_interfaces().await;

        // Ask for all the addresses we have
        let mut names = BTreeMap::<u32, String>::new();
        let mut addresses = self
            .handle
            .as_ref()
            .unwrap_or_log()
            .address()
            .get()
            .execute();
        while let Some(msg) = addresses.try_next().await.map_err(|e| io_error_other!(e))? {
            // Have we seen this interface index yet?
            // Get the name from the index, cached, if we can
            let ifname = match names.entry(msg.header.index) {
                Entry::Vacant(v) => {
                    // If not, get the name for the index if we can
                    let ifname = match get_interface_name(msg.header.index) {
                        Ok(v) => v,
                        Err(e) => {
                            warn!(target: "net",
                                "couldn't get interface name for index {}: {}",
                                msg.header.index,
                                e
                            );
                            continue;
                        }
                    };
                    v.insert(ifname).clone()
                }
                Entry::Occupied(o) => o.get().clone(),
            };

            // Map the name to a NetworkInterface
            if !interfaces.contains_key(&ifname) {
                // If we have no NetworkInterface yet, make one
                let flags = self.get_interface_flags(&ifname)?;
                let mut net_intf = NetworkInterface::new(ifname.clone(), flags);
                // Add gateways if we have them
                if let Some(gateways) = self.gateways.get(&msg.header.index) {
                    for gateway in gateways {
                        net_intf.add_gateway(*gateway);
                    }
                }
                interfaces.insert(ifname.clone(), net_intf);
            }
            let intf = interfaces.get_mut(&ifname).unwrap_or_log();

            // Process the address
            let intf_addr = match msg.header.family {
                AddressFamily::Inet => match Self::process_address_message_v4(msg) {
                    Some(ia) => ia,
                    None => {
                        continue;
                    }
                },
                AddressFamily::Inet6 => match Self::process_address_message_v6(msg) {
                    Some(ia) => ia,
                    None => {
                        continue;
                    }
                },
                _ => {
                    continue;
                }
            };

            intf.addrs.push(intf_addr);
        }

        Ok(())
    }

    pub async fn get_interfaces(
        &mut self,
        interfaces: &mut BTreeMap<String, NetworkInterface>,
    ) -> io::Result<()> {
        // Get the netlink connection
        let (connection, handle, _) = new_connection_with_socket::<RTNetLinkSocket>()?;

        // Spawn a connection handler
        let connection_jh = spawn("rtnetlink connection", connection);

        // Save the connection
        self.connection_jh = Some(connection_jh);
        self.handle = Some(handle);

        // Do the work
        let out = self.get_interfaces_internal(interfaces).await;

        // Clean up connection
        drop(self.handle.take());
        self.connection_jh.take().unwrap_or_log().abort().await;

        out
    }
}

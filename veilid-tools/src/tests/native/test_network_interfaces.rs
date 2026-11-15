use crate::*;

cfg_if! {
    if #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))] {
        use network_interfaces::{
            AddressFlags, EgressIdentity, IfAddr, Ifv4Addr, Ifv6Addr, InterfaceAddress,
            InterfaceFlags, NetworkInterface, NetworkInterfaceAddressState, NetworkInterfaces,
        };

        const STABLE: &str = "2600:1702:7d80:a0f:ac:47d2:bae1:c87";
        const TEMP_OLD: &str = "2600:1702:7d80:a0f:69b6:2e0a:b8d0:936b";
        const TEMP_NEW: &str = "2600:1702:7d80:a0f:9d2c:4736:2e56:424c";
        const LAN: Ipv4Addr = Ipv4Addr::new(192, 168, 0, 58);

        fn running(name: &str) -> NetworkInterface {
            NetworkInterface::new(
                name.to_owned(),
                InterfaceFlags {
                    is_running: true,
                    ..InterfaceFlags::default()
                },
            )
        }

        fn v4(ip: Ipv4Addr) -> InterfaceAddress {
            InterfaceAddress {
                if_addr: IfAddr::V4(Ifv4Addr {
                    ip,
                    netmask: Ipv4Addr::new(255, 255, 255, 0),
                    broadcast: None,
                }),
                flags: AddressFlags {
                    is_dynamic: true,
                    is_preferred: true,
                    ..AddressFlags::default()
                },
            }
        }

        fn v6(ip: &str, is_temporary: bool, is_preferred: bool) -> InterfaceAddress {
            InterfaceAddress {
                if_addr: IfAddr::V6(Ifv6Addr {
                    ip: ip.parse().unwrap(),
                    netmask: "ffff:ffff:ffff:ffff::".parse().unwrap(),
                    broadcast: None,
                }),
                flags: AddressFlags {
                    is_dynamic: true,
                    is_temporary,
                    is_preferred,
                    ..AddressFlags::default()
                },
            }
        }

        /// One interface as macOS presents it around a privacy-address rotation: a stable
        /// global, a deprecated temporary and a fresh one
        fn wifi() -> BTreeMap<String, NetworkInterface> {
            let mut en0 = running("en0");
            en0.add_address(v4(LAN));
            en0.add_address(v6(STABLE, false, true));
            en0.add_address(v6(TEMP_OLD, true, false));
            en0.add_address(v6(TEMP_NEW, true, true));
            en0.add_address(v6("fe80::58:e37e:2bab:d83f", false, true));
            en0.add_gateway(IpAddr::V4(Ipv4Addr::new(192, 168, 0, 1)));
            BTreeMap::from([("en0".to_owned(), en0)])
        }

        /// What `refresh` does with the kernel's primaries: filter, then distill
        fn state(
            interfaces: &BTreeMap<String, NetworkInterface>,
            primary_v4: Option<Ipv4Addr>,
            primary_v6: Option<Ipv6Addr>,
        ) -> NetworkInterfaceAddressState {
            let mut interfaces = interfaces.clone();
            NetworkInterfaces::filter_non_primary_global_addresses(
                &mut interfaces,
                primary_v4,
                primary_v6,
            );
            NetworkInterfaces::interface_address_state_for(&interfaces, primary_v4, primary_v6)
        }

        fn state_with_primary_v6(primary: &str) -> NetworkInterfaceAddressState {
            state(&wifi(), Some(LAN), Some(primary.parse().unwrap()))
        }

        const WIRED: Ipv4Addr = Ipv4Addr::new(192, 168, 1, 146);
        const WIFI: Ipv4Addr = Ipv4Addr::new(192, 168, 1, 69);
        const GATEWAY: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));

        fn point_to_point(name: &str) -> NetworkInterface {
            NetworkInterface::new(
                name.to_owned(),
                InterfaceFlags {
                    is_running: true,
                    is_point_to_point: true,
                    ..InterfaceFlags::default()
                },
            )
        }

        /// Wired and wifi on one LAN with a ULA prefix and no global IPv6, wired carrying the
        /// default route
        fn lan() -> BTreeMap<String, NetworkInterface> {
            let mut en19 = running("en19");
            en19.add_address(v4(WIRED));
            en19.add_address(v6("fd5c:1f15:d332:47c5:8ed:15bf:814e:9f50", false, true));
            en19.add_address(v6("fe80::14e6:77a:90e9:435b", false, true));
            en19.add_gateway(GATEWAY);
            let mut en0 = running("en0");
            en0.add_address(v4(WIFI));
            en0.add_address(v6("fd5c:1f15:d332:47c5:14c2:1f05:3413:bb92", false, true));
            en0.add_gateway(GATEWAY);
            BTreeMap::from([("en19".to_owned(), en19), ("en0".to_owned(), en0)])
        }

        /// What Xcode's device tunnel adds: an IPv6-only point-to-point interface with a random
        /// ULA and its own scoped default route
        fn with_tunnel(
            mut interfaces: BTreeMap<String, NetworkInterface>,
            prefix: &str,
        ) -> BTreeMap<String, NetworkInterface> {
            let mut utun8 = point_to_point("utun8");
            utun8.add_address(v6(&format!("{prefix}::2"), false, true));
            utun8.add_gateway(format!("{prefix}::").parse().unwrap());
            interfaces.insert("utun8".to_owned(), utun8);
            interfaces
        }

        fn renumber(interfaces: &mut BTreeMap<String, NetworkInterface>, name: &str, from: Ipv4Addr, to: Ipv4Addr) {
            let intf = interfaces.get_mut(name).unwrap();
            intf.addrs.retain(|a| a.if_addr().ip() != IpAddr::V4(from));
            intf.add_address(v4(to));
        }

        /// A tunnel appearing beside the egress interface stays visible in the state and
        /// changes no egress
        pub fn test_tunnel_flip_keeps_the_egress() {
            info!("test_tunnel_flip_keeps_the_egress");
            let before = state(&lan(), Some(WIRED), None);
            let after = state(&with_tunnel(lan(), "fdf9:5b89:8cdb"), Some(WIRED), None);
            assert_eq!(before.egress_v4, after.egress_v4);
            assert_eq!(after.egress_v6, None);
            let wired = before.egress_v4.clone().expect("wired egress");
            assert_eq!(wired.interface, "en19");
            assert_eq!(wired.source, Some(IpAddr::V4(WIRED)));
            assert_eq!(wired.gateways, vec![GATEWAY]);

            let tunnel: IpAddr = "fdf9:5b89:8cdb::2".parse().unwrap();
            assert!(after.interface_addresses.iter().any(|a| a.ip() == tunnel));
            assert!(after.own_addresses.contains(&tunnel));
            assert!(after
                .gateway_addresses
                .contains(&"fdf9:5b89:8cdb::".parse().unwrap()));
            assert_ne!(before, after);
        }

        /// The egress follows the kernel's source: a new address or default route on the wired
        /// interface, the route moving to wifi or into a VPN; wifi renumbering under wired is
        /// nothing, and no default route is no egress
        pub fn test_egress_follows_the_primary_source() {
            info!("test_egress_follows_the_primary_source");
            let base = state(&lan(), Some(WIRED), None);

            let mut renumbered = lan();
            let new_wired = Ipv4Addr::new(192, 168, 1, 200);
            renumber(&mut renumbered, "en19", WIRED, new_wired);
            let moved = state(&renumbered, Some(new_wired), None).egress_v4.unwrap();
            assert_eq!(moved.interface, "en19");
            assert_eq!(moved.source, Some(IpAddr::V4(new_wired)));

            let on_wifi = state(&lan(), Some(WIFI), None).egress_v4.unwrap();
            assert_eq!(on_wifi.interface, "en0");
            assert_eq!(on_wifi.source, Some(IpAddr::V4(WIFI)));

            let mut vpn = lan();
            let mut utun9 = point_to_point("utun9");
            let vpn_ip = Ipv4Addr::new(10, 8, 0, 2);
            utun9.add_address(v4(vpn_ip));
            utun9.add_gateway(IpAddr::V4(Ipv4Addr::new(10, 8, 0, 1)));
            vpn.insert("utun9".to_owned(), utun9);
            let tunneled = state(&vpn, Some(vpn_ip), None).egress_v4.unwrap();
            assert_eq!(tunneled.interface, "utun9");
            assert_eq!(tunneled.gateways, vec![IpAddr::V4(Ipv4Addr::new(10, 8, 0, 1))]);

            let mut wifi_changed = lan();
            renumber(&mut wifi_changed, "en0", WIFI, Ipv4Addr::new(192, 168, 1, 70));
            assert_eq!(base.egress_v4, state(&wifi_changed, Some(WIRED), None).egress_v4);

            assert_eq!(state(&lan(), None, None).egress_v4, None);
        }

        /// An alias on the egress interface coming or going is nothing while the kernel's
        /// source stays, in either direction; the source going to the alias is a move
        pub fn test_alias_on_the_egress_is_not_a_change() {
            info!("test_alias_on_the_egress_is_not_a_change");
            let alias = Ipv4Addr::new(192, 168, 1, 200);
            let plain = state(&lan(), Some(WIRED), None);
            let mut with_alias = lan();
            with_alias.get_mut("en19").unwrap().add_address(v4(alias));
            let aliased = state(&with_alias, Some(WIRED), None);
            assert_eq!(plain.egress_v4, aliased.egress_v4);
            assert_ne!(plain.interface_addresses, aliased.interface_addresses);
            assert_eq!(state(&lan(), Some(WIRED), None).egress_v4, aliased.egress_v4);

            let mut lost_primary = with_alias.clone();
            renumber(&mut lost_primary, "en19", WIRED, alias);
            let moved = state(&lost_primary, Some(alias), None).egress_v4.unwrap();
            assert_eq!(moved.source, Some(IpAddr::V4(alias)));
            assert_ne!(Some(moved), plain.egress_v4);
        }

        /// On IPv6-only cellular the IPv4 source is the CLAT address, which publication skips;
        /// the egress still names its interface and source
        pub fn test_clat_egress_names_its_interface() {
            info!("test_clat_egress_names_its_interface");
            let mut clat = NetworkInterface::new(
                "v4-rmnet_data0".to_owned(),
                InterfaceFlags {
                    is_running: true,
                    is_clat: true,
                    ..InterfaceFlags::default()
                },
            );
            let clat_ip = Ipv4Addr::new(192, 0, 0, 4);
            clat.add_address(v4(clat_ip));
            let interfaces = BTreeMap::from([("v4-rmnet_data0".to_owned(), clat)]);
            let cellular = state(&interfaces, Some(clat_ip), None);
            assert_eq!(
                cellular.egress_v4,
                Some(EgressIdentity {
                    interface: "v4-rmnet_data0".to_owned(),
                    source: Some(IpAddr::V4(clat_ip)),
                    gateways: vec![],
                })
            );
            assert!(cellular.interface_addresses.is_empty());
        }

        /// Carrier NAT addresses never reach publication, the only set discovery classifies
        /// from: a Tailscale-style 100.x tunnel beside the LAN changes neither that set nor the
        /// egress, and a carrier NAT egress keeps its source for the hole punch TTL
        pub fn test_carrier_nat_addresses_are_never_published() {
            info!("test_carrier_nat_addresses_are_never_published");
            let plain = state(&lan(), Some(WIRED), None);
            let mut tailnet = lan();
            let mut utun4 = point_to_point("utun4");
            let tailscale_ip = Ipv4Addr::new(100, 101, 102, 103);
            utun4.add_address(v4(tailscale_ip));
            tailnet.insert("utun4".to_owned(), utun4);
            let with_tailnet = state(&tailnet, Some(WIRED), None);
            assert_eq!(plain.interface_addresses, with_tailnet.interface_addresses);
            assert_eq!(plain.egress_v4, with_tailnet.egress_v4);
            assert!(with_tailnet.own_addresses.contains(&IpAddr::V4(tailscale_ip)));

            let mut pdp_ip0 = running("pdp_ip0");
            let carrier_ip = Ipv4Addr::new(100, 72, 14, 9);
            pdp_ip0.add_address(v4(carrier_ip));
            let cellular = state(
                &BTreeMap::from([("pdp_ip0".to_owned(), pdp_ip0)]),
                Some(carrier_ip),
                None,
            );
            assert!(cellular.interface_addresses.is_empty());
            assert_eq!(
                cellular.egress_v4.and_then(|egress| egress.source),
                Some(IpAddr::V4(carrier_ip))
            );
        }

        /// The kernel sends from whichever temporary it prefers; that choice must not move
        /// the published address or read as an interface change
        pub fn test_privacy_address_flip_is_not_a_change() {
            info!("test_privacy_address_flip_is_not_a_change");
            let with_old = state_with_primary_v6(TEMP_OLD);
            let with_new = state_with_primary_v6(TEMP_NEW);
            assert_eq!(with_old, with_new);

            let stable: IpAddr = STABLE.parse().unwrap();
            assert_eq!(
                with_new.egress_v6.as_ref().and_then(|e| e.source),
                Some(stable),
                "egress source is the stable global, not the privacy address the kernel picked"
            );
            let published: Vec<IpAddr> = with_new
                .interface_addresses
                .iter()
                .map(|a| a.ip())
                .collect();
            assert!(published.contains(&stable), "stable global published: {published:?}");
            assert!(with_new.own_addresses.contains(&stable));
            for temp in [TEMP_OLD, TEMP_NEW] {
                let temp: IpAddr = temp.parse().unwrap();
                assert!(!published.contains(&temp), "privacy address published: {temp}");
                assert!(with_new.own_addresses.contains(&temp), "privacy address not ours: {temp}");
            }
        }

        /// A global on an interface other than the primary one is unreachable and goes; its
        /// local addresses stay for LocalNetwork, and an interface left empty goes with them
        pub fn test_non_primary_interface_keeps_only_locals() {
            info!("test_non_primary_interface_keeps_only_locals");
            let mut interfaces = wifi();
            let mut cell = running("pdp_ip0");
            cell.add_address(v6("2600:1702:1::5", false, true));
            cell.add_address(v6("fe80::1", false, true));
            interfaces.insert("pdp_ip0".to_owned(), cell);
            let mut lone = running("pdp_ip1");
            lone.add_address(v6("2607:f8b0::9", false, true));
            interfaces.insert("pdp_ip1".to_owned(), lone);

            NetworkInterfaces::filter_non_primary_global_addresses(
                &mut interfaces,
                Some(LAN),
                Some(TEMP_NEW.parse().unwrap()),
            );

            let cell_ips: Vec<IpAddr> = interfaces["pdp_ip0"]
                .addresses()
                .iter()
                .map(|a| a.if_addr().ip())
                .collect();
            assert_eq!(cell_ips, vec!["fe80::1".parse::<IpAddr>().unwrap()]);
            assert!(!interfaces.contains_key("pdp_ip1"));
            assert_eq!(interfaces["en0"].addresses().len(), 5);
        }

        /// Link-local IPv6 is never published, so an AirDrop or tunnel interface rotating its
        /// link-local address changes nothing peers see
        pub fn test_link_local_ipv6_is_never_published() {
            info!("test_link_local_ipv6_is_never_published");
            let with_rotation = |rotated: &str| {
                let mut interfaces = wifi();
                let mut awdl = running("awdl0");
                awdl.add_address(v6(rotated, false, true));
                interfaces.insert("awdl0".to_owned(), awdl);
                let mut utun = NetworkInterface::new(
                    "utun0".to_owned(),
                    InterfaceFlags {
                        is_running: true,
                        is_point_to_point: true,
                        ..InterfaceFlags::default()
                    },
                );
                utun.add_address(v6("fe80::81a1:1a42:8f9d:249c", false, true));
                interfaces.insert("utun0".to_owned(), utun);
                state(&interfaces, Some(LAN), Some(TEMP_NEW.parse().unwrap()))
            };

            let before = with_rotation("fe80::4003:dfff:fefc:8c01");
            let after = with_rotation("fe80::caf:f3ff:feab:c0c4");

            let link_local = |a: &IfAddr| match a {
                IfAddr::V6(v6) => ipv6addr_is_unicast_link_local(&v6.ip),
                IfAddr::V4(_) => false,
            };
            assert!(!before.interface_addresses.iter().any(link_local));
            assert_eq!(before.interface_addresses.len(), 2, "the LAN address and the stable global");
            assert!(before.is_own_address("fe80::58:e37e:2bab:d83f".parse().unwrap()));
            assert_eq!(before.interface_addresses, after.interface_addresses);
            assert_eq!(before.gateway_addresses, after.gateway_addresses);
            assert_eq!(before.egress_v4, after.egress_v4);
        }

        pub async fn test_network_interfaces() {
            info!("testing network interfaces");
            let t1 = get_raw_timestamp();
            let interfaces = NetworkInterfaces::new();
            let count = 100;
            for x in 0..count {
                info!("loop {}", x);
                if let Err(e) = interfaces.refresh().await {
                    error!("error refreshing interfaces: {}", e);
                }
            }
            let t2 = get_raw_timestamp();
            let tdiff = ((t2 - t1) as f64)/1000000.0f64;
            info!("running network interface test with {} iterations took {} seconds", count, tdiff);
            //info!("interfaces: {:#?}", interfaces)

            #[cfg(not(target_os = "openbsd"))]
            assert!(
                interfaces.with_interfaces(|interfaces| interfaces
                    .values()
                    .any(|intf| !intf.is_loopback() && intf.mtu().is_some())),
                "some interface should report its mtu"
            );
        }
    }
}

pub async fn test_all() {
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    {
        test_privacy_address_flip_is_not_a_change();
        test_non_primary_interface_keeps_only_locals();
        test_tunnel_flip_keeps_the_egress();
        test_egress_follows_the_primary_source();
        test_alias_on_the_egress_is_not_a_change();
        test_clat_egress_names_its_interface();
        test_carrier_nat_addresses_are_never_published();
        test_link_local_ipv6_is_never_published();
        test_network_interfaces().await;
    }
}

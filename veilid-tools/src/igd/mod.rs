#![cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]

pub(crate) mod gateway;
mod http;
mod messages;
pub(crate) mod search;
pub(crate) mod xml;

pub use gateway::Gateway;
pub use search::search_gateway;

use std::collections::HashMap;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::time::Duration;

/// Protocol for a port mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)]
pub enum PortMappingProtocol {
    /// TCP mapping
    TCP,
    /// UDP mapping
    UDP,
}

impl fmt::Display for PortMappingProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PortMappingProtocol::TCP => write!(f, "TCP"),
            PortMappingProtocol::UDP => write!(f, "UDP"),
        }
    }
}

/// IPv6 scope to search for a gateway in.
pub enum Ipv6SearchScope {
    /// Link-local search scope
    LinkLocal,
    /// Subnet search scope
    Subnet,
    /// Administrative search scope
    Admin,
    /// Site-local search scope
    SiteLocal,
}

/// Gateway search configuration.
pub struct SearchOptions {
    /// Bind address for the discovery UDP socket
    pub bind_addr: SocketAddr,
    /// Broadcast address for discovery packets
    pub broadcast_address: SocketAddr,
    /// Timeout for the whole search
    pub timeout: Option<Duration>,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self::new_v4(10_000)
    }
}

impl SearchOptions {
    /// Create a new IPv4 search.
    #[must_use]
    pub fn new_v4(timeout_ms: u64) -> Self {
        Self {
            bind_addr: (IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0).into(),
            broadcast_address: "239.255.255.250:1900".parse().unwrap(),
            timeout: Some(Duration::from_millis(timeout_ms)),
        }
    }

    /// Create a new IPv6 search.
    #[must_use]
    pub fn new_v6(scope: Ipv6SearchScope, timeout_ms: u64) -> Self {
        Self {
            bind_addr: (IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0).into(),
            broadcast_address: match scope {
                Ipv6SearchScope::LinkLocal => "[FF02::C]:1900",
                Ipv6SearchScope::Subnet => "[FF03::C]:1900",
                Ipv6SearchScope::Admin => "[FF04::C]:1900",
                Ipv6SearchScope::SiteLocal => "[FF05::C]:1900",
            }
            .parse()
            .unwrap(),
            timeout: Some(Duration::from_millis(timeout_ms)),
        }
    }
}

/// Error from gateway discovery or port mapping operations.
#[derive(Debug, Clone)]
pub struct IgdError {
    soap_code: Option<u16>,
    message: String,
}

impl IgdError {
    fn message(message: impl Into<String>) -> Self {
        Self {
            soap_code: None,
            message: message.into(),
        }
    }

    fn parse(message: impl Into<String>) -> Self {
        Self {
            soap_code: None,
            message: format!("parse error: {}", message.into()),
        }
    }

    fn http(message: impl Into<String>) -> Self {
        Self {
            soap_code: None,
            message: format!("http error: {}", message.into()),
        }
    }

    fn soap(code: u16, description: String) -> Self {
        Self {
            soap_code: Some(code),
            message: format!("upnp error {}: {}", code, description),
        }
    }

    fn soap_code(&self) -> Option<u16> {
        self.soap_code
    }
}

impl fmt::Display for IgdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for IgdError {}

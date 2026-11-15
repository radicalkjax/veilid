use super::*;

/// Counts its own use, so a path that must not resolve anything can prove it
#[derive(Debug, Clone, Default)]
pub struct MockDialInfoConverter {
    resolve_count: Arc<AtomicUsize>,
}

impl MockDialInfoConverter {
    pub fn resolve_count(&self) -> usize {
        self.resolve_count.load(Ordering::Acquire)
    }
}

impl DialInfoConverterResolver for MockDialInfoConverter {
    fn ptr_lookup(&self, _ip_addr: IpAddr) -> PinBoxFuture<'_, EyreResult<String>> {
        self.resolve_count.fetch_add(1, Ordering::AcqRel);
        pin_dyn_future!(async move { Ok("fake_hostname".to_string()) })
    }

    fn to_socket_addrs(
        &self,
        _host: &str,
        default: SocketAddr,
    ) -> PinBoxFuture<'_, std::io::Result<Vec<SocketAddr>>> {
        self.resolve_count.fetch_add(1, Ordering::AcqRel);
        pin_dyn_future!(async move { Ok(vec![default]) })
    }
}

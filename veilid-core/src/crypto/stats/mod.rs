use super::*;

mod metered;
pub(crate) use metered::{CryptoStatsAccounting, MeteredCryptoSystem};

#[derive(Debug, Serialize)]
pub(super) struct CryptoStats {
    dh_cache: DhCacheStats,
    operations: Vec<CryptoKindStats>,
}

#[derive(Debug, Serialize)]
struct DhCacheStats {
    hits: u64,
    misses: u64,
    lru_evictions: u64,
}

#[derive(Debug, Serialize)]
struct CryptoKindStats {
    kind: String,
    count: u64,
    latency: LatencyStats,
}

impl Crypto {
    pub(super) fn stats_impl(&self) -> Option<Box<dyn VeilidComponentStats>> {
        let dh_cache = {
            let inner = self.inner.lock();
            DhCacheStats {
                hits: inner.dh_cache_hits as u64,
                misses: inner.dh_cache_misses as u64,
                lru_evictions: inner.dh_cache_lru as u64,
            }
        };
        let operations = self
            .crypto_stats
            .snapshot()
            .into_iter()
            .map(|s| CryptoKindStats {
                kind: s.kind.to_string(),
                count: s.count,
                latency: s.latency,
            })
            .collect();
        Some(Box::new(CryptoStats {
            dh_cache,
            operations,
        }))
    }
}

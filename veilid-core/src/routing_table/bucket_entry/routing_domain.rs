use super::*;

/// A bucket entry's state in one routing domain
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct BucketEntryRoutingDomain {
    /// The peer info, saved as its signed message
    #[serde(with = "signed_peer_info::option")]
    pub peer_info: Option<Arc<PeerInfo>>,
    /// The last node info timestamp of ours that this entry has seen
    pub last_seen_our_node_info_ts: Timestamp,
    /// Last known node status
    pub node_status: Option<NodeStatus>,
}

impl BucketEntryRoutingDomain {
    /// Whether `saved` holds this peer info and seen timestamp
    pub(super) fn matches_saved(&self, saved: &Self) -> bool {
        self.last_seen_our_node_info_ts == saved.last_seen_our_node_info_ts
            && match (&self.peer_info, &saved.peer_info) {
                (Some(current), Some(saved)) => Arc::ptr_eq(current, saved),
                (None, None) => true,
                _ => false,
            }
    }
}

impl fmt::Display for BucketEntryRoutingDomain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(pi) = &self.peer_info {
            writeln!(f, "peer_info:")?;
            writeln!(f, "    {}", indent_string(f.to_string(&**pi)))?;
        } else {
            writeln!(f, "peer_info: None")?;
        }
        writeln!(
            f,
            "last_seen_our_node_info_ts: {}",
            f.to_string(self.last_seen_our_node_info_ts)
        )?;
        writeln!(f, "node_status: {:?}", self.node_status)?;
        Ok(())
    }
}

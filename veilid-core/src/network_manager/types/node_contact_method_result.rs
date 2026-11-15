use super::*;

/// Whether a contact method lookup precedes an actual send. Only a real send may
/// spend the suppression recovery canary or count as a skipped send; an advisory
/// lookup that consumed the canary would starve the send that follows it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ContactMethodIntent {
    /// A send follows immediately
    Send,
    /// Inspection only (bootstrap pre-check, debug)
    Advisory,
}

/// Pre-send outcome of choosing how to reach a node.
/// Resolved means we picked a contact method; other variants explain why we couldn't.
#[derive(Clone, Debug)]
pub enum NodeContactMethodResult {
    /// A contact method was picked and the send was attempted.
    Resolved(ContactMethod),
    /// Destination has no routing domain (e.g. target's relay address doesn't match any of ours).
    NoRoutingDomain,
    /// Target has no PeerInfo in the routing table for the chosen routing domain.
    NoPeerInfo,
    /// No compatible contact method could be derived from peer info.
    NoContactMethod,
    /// Every contact method for this node rides a transport our own send history says is
    /// dead, and the recovery canary isn't due, so attempting anyway only buys a timeout.
    TransportSuppressed,
    /// Target node id is administratively punished.
    Punished,
}

impl NodeContactMethodResult {
    pub fn opt_transport_type(&self) -> Option<TransportType> {
        match self {
            NodeContactMethodResult::Resolved(cm) => cm.opt_transport_type(),
            _ => None,
        }
    }

    pub fn direct_dial_info(&self) -> Option<DialInfo> {
        match self {
            NodeContactMethodResult::Resolved(cm) => cm.direct_dial_info(),
            _ => None,
        }
    }
}

impl fmt::Display for NodeContactMethodResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeContactMethodResult::Resolved(cm) => write!(f, "{}", f.to_string(cm)),
            NodeContactMethodResult::NoRoutingDomain => write!(f, "NoRoutingDomain"),
            NodeContactMethodResult::NoPeerInfo => write!(f, "NoPeerInfo"),
            NodeContactMethodResult::NoContactMethod => write!(f, "NoContactMethod"),
            NodeContactMethodResult::TransportSuppressed => write!(f, "TransportSuppressed"),
            NodeContactMethodResult::Punished => write!(f, "Punished"),
        }
    }
}

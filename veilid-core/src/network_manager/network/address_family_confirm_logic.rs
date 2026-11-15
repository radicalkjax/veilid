//! Decision logic for empirical address-family confirmation, used by the wasm
//! network and unit-tested on every platform.
//!
//! Invariant: a probe round that condemns every family defers rather than
//! confirming the empty set. An empty confirmed set stops all traffic, and with
//! no traffic no network-change event re-arms confirmation, so the node stays
//! dark until restart. Only a domain with no configured address types confirms
//! empty.

use super::*;

/// Minimum number of candidate nodes to probe an address family
pub(crate) const ADDR_FAMILY_MIN_NODES_TO_CONFIRM: usize = 4;

/// Address family probe result
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum FamilyProbeDisposition {
    /// A probe got an answer over this family
    Supported,
    /// Enough probe failures to condemn this family
    Unsupported,
    /// Probes ran out before either conclusion
    Inconclusive,
    /// The stop token fired mid-round
    Cancelled,
}

/// Address family confirmation state
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum AddressFamilyConfirmState {
    /// No address types are configured for this routing domain
    NoConfiguredAddressTypes,
    /// All configured address types have been confirmed
    AllConfiguredConfirmed,
    /// Not enough candidates to verify unconfirmed address type
    NotEnoughCandidates,
    /// Confirm candidates to determine final set of address types
    ConfirmCandidates,
}

/// Outcome of one confirmation pass
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ConfirmationPassOutcome {
    /// Publish this set and mark the domain confirmed
    Confirm(AddressTypeSet),
    /// Not enough evidence to conclude; leave unconfirmed so the worker retries
    Deferred,
}

/// Candidate inventory for one confirmation pass, generic over the candidate
/// handle so the decision logic tests off-network
pub(crate) struct AddressFamilyConfirmCandidates<N> {
    /// All address types configured for this routing domain
    pub(crate) configured_address_types: AddressTypeSet,
    /// All address types that have been positively seen since we started the confirmation process
    pub(crate) confirmed_address_types: AddressTypeSet,
    /// Nodes that can confirm an unconfirmed address type
    pub(crate) candidates: BTreeMap<AddressType, Vec<N>>,
}

impl<N> AddressFamilyConfirmCandidates<N> {
    /// Determine the current state of the address family confirmation process
    pub fn state(&self) -> AddressFamilyConfirmState {
        if self.configured_address_types.is_empty() {
            AddressFamilyConfirmState::NoConfiguredAddressTypes
        } else if self.confirmed_address_types == self.configured_address_types {
            AddressFamilyConfirmState::AllConfiguredConfirmed
        } else {
            let missing_address_types =
                self.configured_address_types - self.confirmed_address_types;
            for mat in missing_address_types {
                let candidates = self
                    .candidates
                    .get(&mat)
                    .as_ref()
                    .map(|c| c.len())
                    .unwrap_or(0);
                if candidates < ADDR_FAMILY_MIN_NODES_TO_CONFIRM {
                    return AddressFamilyConfirmState::NotEnoughCandidates;
                }
            }
            AddressFamilyConfirmState::ConfirmCandidates
        }
    }
}

/// Fold a probe round into a pass outcome. `seen_live` holds families already
/// confirmed by real traffic since the confirm request.
pub(crate) fn resolve_probe_dispositions(
    seen_live: AddressTypeSet,
    dispositions: &[(AddressType, FamilyProbeDisposition)],
) -> ConfirmationPassOutcome {
    let mut confirmed_address_types = seen_live;
    for (at, disposition) in dispositions {
        match disposition {
            FamilyProbeDisposition::Cancelled => return ConfirmationPassOutcome::Deferred,
            FamilyProbeDisposition::Supported => {
                confirmed_address_types.insert(*at);
            }
            FamilyProbeDisposition::Unsupported | FamilyProbeDisposition::Inconclusive => {}
        }
    }
    // All-condemned must defer, never confirm empty (see module doc)
    if confirmed_address_types.is_empty() {
        return ConfirmationPassOutcome::Deferred;
    }
    ConfirmationPassOutcome::Confirm(confirmed_address_types)
}

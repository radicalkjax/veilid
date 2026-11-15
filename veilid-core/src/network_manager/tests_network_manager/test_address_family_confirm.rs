use super::*;

// Candidate handles are irrelevant to the decision logic, so tests use ()
fn candidates_with(
    configured: AddressTypeSet,
    confirmed: AddressTypeSet,
    candidate_counts: &[(AddressType, usize)],
) -> AddressFamilyConfirmCandidates<()> {
    let mut candidates: BTreeMap<AddressType, Vec<()>> = BTreeMap::new();
    for (at, count) in candidate_counts {
        candidates.insert(*at, vec![(); *count]);
    }
    AddressFamilyConfirmCandidates {
        configured_address_types: configured,
        confirmed_address_types: confirmed,
        candidates,
    }
}

pub fn test_state_classification() {
    info!("--- test_state_classification ---");

    // Nothing configured is its own state, not a failure
    assert_eq!(
        candidates_with(AddressTypeSet::empty(), AddressTypeSet::empty(), &[]).state(),
        AddressFamilyConfirmState::NoConfiguredAddressTypes
    );

    // Everything already proven by live traffic
    assert_eq!(
        candidates_with(AddressTypeSet::all(), AddressTypeSet::all(), &[]).state(),
        AddressFamilyConfirmState::AllConfiguredConfirmed
    );

    // A missing family with too few candidates blocks probing entirely
    assert_eq!(
        candidates_with(
            AddressTypeSet::all(),
            AddressTypeSet::only(AddressType::IPV4),
            &[(AddressType::IPV6, ADDR_FAMILY_MIN_NODES_TO_CONFIRM - 1)],
        )
        .state(),
        AddressFamilyConfirmState::NotEnoughCandidates
    );

    // A missing family with no candidate entry at all is the same
    assert_eq!(
        candidates_with(
            AddressTypeSet::all(),
            AddressTypeSet::only(AddressType::IPV4),
            &[],
        )
        .state(),
        AddressFamilyConfirmState::NotEnoughCandidates
    );

    // Enough candidates for every missing family: ready to probe
    assert_eq!(
        candidates_with(
            AddressTypeSet::all(),
            AddressTypeSet::empty(),
            &[
                (AddressType::IPV4, ADDR_FAMILY_MIN_NODES_TO_CONFIRM),
                (AddressType::IPV6, ADDR_FAMILY_MIN_NODES_TO_CONFIRM),
            ],
        )
        .state(),
        AddressFamilyConfirmState::ConfirmCandidates
    );
}

pub fn test_resolve_confirms_probe_survivors() {
    info!("--- test_resolve_confirms_probe_survivors ---");

    // A supported/condemned mix confirms the survivors
    assert_eq!(
        resolve_probe_dispositions(
            AddressTypeSet::empty(),
            &[
                (AddressType::IPV4, FamilyProbeDisposition::Supported),
                (AddressType::IPV6, FamilyProbeDisposition::Unsupported),
            ],
        ),
        ConfirmationPassOutcome::Confirm(AddressTypeSet::only(AddressType::IPV4))
    );

    // A family proven by real traffic survives even when every probe fails
    assert_eq!(
        resolve_probe_dispositions(
            AddressTypeSet::only(AddressType::IPV4),
            &[(AddressType::IPV6, FamilyProbeDisposition::Unsupported)],
        ),
        ConfirmationPassOutcome::Confirm(AddressTypeSet::only(AddressType::IPV4))
    );

    // Inconclusive is not proof of support
    assert_eq!(
        resolve_probe_dispositions(
            AddressTypeSet::empty(),
            &[
                (AddressType::IPV4, FamilyProbeDisposition::Supported),
                (AddressType::IPV6, FamilyProbeDisposition::Inconclusive),
            ],
        ),
        ConfirmationPassOutcome::Confirm(AddressTypeSet::only(AddressType::IPV4))
    );
}

pub fn test_resolve_cancelled_defers() {
    info!("--- test_resolve_cancelled_defers ---");

    // Shutdown mid-probe must not publish a partial verdict
    assert_eq!(
        resolve_probe_dispositions(
            AddressTypeSet::empty(),
            &[
                (AddressType::IPV4, FamilyProbeDisposition::Supported),
                (AddressType::IPV6, FamilyProbeDisposition::Cancelled),
            ],
        ),
        ConfirmationPassOutcome::Deferred
    );
}

pub fn test_resolve_all_condemned_defers() {
    info!("--- test_resolve_all_condemned_defers ---");

    // Confirming the empty set would latch the node dark until restart:
    // - an empty confirmed set stops all traffic
    // - no traffic means no network-change event to re-arm confirmation
    assert_eq!(
        resolve_probe_dispositions(
            AddressTypeSet::empty(),
            &[
                (AddressType::IPV4, FamilyProbeDisposition::Unsupported),
                (AddressType::IPV6, FamilyProbeDisposition::Unsupported),
            ],
        ),
        ConfirmationPassOutcome::Deferred
    );
}

pub fn test_all() {
    test_state_classification();
    test_resolve_confirms_probe_survivors();
    test_resolve_cancelled_defers();
    test_resolve_all_condemned_defers();
}

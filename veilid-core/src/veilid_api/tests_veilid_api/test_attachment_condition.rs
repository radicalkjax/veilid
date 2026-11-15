use super::*;

pub fn attachment_state_ordering() {
    let states = [
        AttachmentState::Detached,
        AttachmentState::Detaching,
        AttachmentState::Attaching,
        AttachmentState::AttachedWeak,
        AttachmentState::AttachedFair,
        AttachmentState::AttachedGood,
        AttachmentState::AttachedStrong,
        AttachmentState::AttachedFull,
    ];
    assert!(states.windows(2).all(|pair| pair[0] < pair[1]));
}

pub fn attachment_condition_satisfied_by() {
    let attachment = VeilidStateAttachment {
        state: AttachmentState::AttachedGood,
        public_internet_ready: true,
        local_network_ready: false,
        ..Default::default()
    };

    // Nothing set, nothing checked
    assert!(VeilidStateAttachmentCondition::default().is_satisfied_by(&attachment));

    let exact = VeilidStateAttachmentCondition {
        opt_min_state: Some(AttachmentState::AttachedGood),
        opt_max_state: Some(AttachmentState::AttachedGood),
        ..Default::default()
    };
    assert!(exact.is_satisfied_by(&attachment));

    let more_attached = VeilidStateAttachmentCondition {
        opt_min_state: Some(AttachmentState::AttachedStrong),
        ..Default::default()
    };
    assert!(!more_attached.is_satisfied_by(&attachment));

    let less_attached = VeilidStateAttachmentCondition {
        opt_max_state: Some(AttachmentState::AttachedFair),
        ..Default::default()
    };
    assert!(!less_attached.is_satisfied_by(&attachment));

    let readiness = VeilidStateAttachmentCondition {
        opt_public_internet_ready: Some(true),
        opt_local_network_ready: Some(false),
        ..Default::default()
    };
    assert!(readiness.is_satisfied_by(&attachment));

    let local_ready = VeilidStateAttachmentCondition {
        opt_local_network_ready: Some(true),
        ..Default::default()
    };
    assert!(!local_ready.is_satisfied_by(&attachment));
}

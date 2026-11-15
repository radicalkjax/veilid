use super::*;
use crate::tests::fixture_veilid_core;

const DETACHED: VeilidStateAttachmentCondition = VeilidStateAttachmentCondition {
    opt_min_state: None,
    opt_max_state: Some(AttachmentState::Detached),
    opt_public_internet_ready: None,
    opt_local_network_ready: None,
};
const WAIT_TIMEOUT_SECS: u32 = 30;
const POLL_MS: u32 = 10;

/// Start a node whose TCP listen port is held, so every startup stops at its bind wait without
/// reaching the network
async fn start_with_tcp_port_held() -> (VeilidAPI, std::net::TcpListener) {
    let holder = std::net::TcpListener::bind(("0.0.0.0", 0)).expect("should hold a port");
    let port = holder.local_addr().expect("should have an address").port();
    let (update_callback, mut config) = fixture_veilid_core();
    config.network.protocol.tcp.listen_address = format!(":{}", port);
    let api = api_startup(update_callback, config)
        .await
        .expect("startup failed");
    (api, holder)
}

/// Wait for a startup to fail its bind and leave a retry pending
async fn wait_for_bind_wait(api: &VeilidAPI) {
    let core_context = api.core_context().expect("api not started");
    let attachment_manager = core_context.attachment_manager();
    let deadline = Timestamp::now().later(TimestampDuration::new_secs(WAIT_TIMEOUT_SECS));
    while attachment_manager.inner.lock().bind_retry_ts.is_none() {
        assert!(Timestamp::now() < deadline, "no bind wait");
        sleep(POLL_MS).await;
    }
    assert!(!attachment_manager.inner.lock().network_started);
}

async fn attachment_state(api: &VeilidAPI) -> AttachmentState {
    api.get_state()
        .await
        .expect("should get state")
        .attachment
        .state
}

/// The bind wait is part of Attaching: attach succeeds again there, and detach ends it without a
/// network to shut down
pub async fn test_bind_wait_is_attaching() {
    info!("--- test_bind_wait_is_attaching ---");
    let (api, _holder) = start_with_tcp_port_held().await;

    api.attach().await.expect("should attach");
    assert_eq!(attachment_state(&api).await, AttachmentState::Attaching);
    wait_for_bind_wait(&api).await;
    assert_eq!(attachment_state(&api).await, AttachmentState::Attaching);

    api.attach()
        .await
        .expect("attach is idempotent while attaching");
    wait_for_bind_wait(&api).await;
    assert_eq!(attachment_state(&api).await, AttachmentState::Attaching);

    api.detach().await.expect("should detach");
    let detached = api
        .wait_for_attachment_condition(
            Some(DETACHED),
            Some(Duration::from_secs(WAIT_TIMEOUT_SECS.into())),
        )
        .await
        .expect("should detach without a network");
    assert_eq!(detached.state, AttachmentState::Detached);
    assert!(matches!(
        api.detach().await,
        Err(VeilidAPIError::Generic { .. })
    ));

    // Terminating mid bind wait has nothing to shut down either
    api.attach().await.expect("should attach again");
    wait_for_bind_wait(&api).await;
    api.shutdown().await;
}

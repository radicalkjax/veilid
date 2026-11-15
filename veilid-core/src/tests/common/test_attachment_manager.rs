use super::*;

/// The attach has taken effect, whether or not any peer answers
const ATTACH_STARTED: VeilidStateAttachmentCondition = VeilidStateAttachmentCondition {
    opt_min_state: Some(AttachmentState::Attaching),
    opt_max_state: None,
    opt_public_internet_ready: None,
    opt_local_network_ready: None,
};
const DETACHED: VeilidStateAttachmentCondition = VeilidStateAttachmentCondition {
    opt_min_state: None,
    opt_max_state: Some(AttachmentState::Detached),
    opt_public_internet_ready: None,
    opt_local_network_ready: None,
};
const ATTACHMENT_WAIT_TIMEOUT: Duration = Duration::from_secs(30);

async fn test_startup_shutdown() {
    info!("test_startup_shutdown: starting");
    let (update_callback, config) = fixture_veilid_core();
    let api = api_startup(update_callback, config)
        .await
        .expect_or_log("startup failed");

    // Test initial state
    assert!(!api.is_shutdown(), "API should not be shut down initially");

    info!("test_startup_shutdown: shutting down");
    let api_clone = api.clone();
    api.shutdown().await;

    // Test state after shutdown
    assert!(
        api_clone.is_shutdown(),
        "API should be shut down after shutdown()"
    );

    info!("test_startup_shutdown: finished");
}

async fn test_startup_shutdown_from_config() {
    info!("test_startup_from_config: starting");
    let (update_callback, config) = fixture_veilid_core();
    let config_json = serialize_json(config);
    let api = api_startup_json(update_callback, config_json)
        .await
        .expect("startup failed");
    info!("test_startup_from_config: shutting down");
    api.shutdown().await;
    info!("test_startup_from_config: finished");
}

async fn test_attach_detach() {
    info!("test_attach_detach: --- test normal order ---");
    let (update_callback, config) = fixture_veilid_core();
    let api = api_startup(update_callback, config)
        .await
        .expect("startup failed");
    api.attach().await.unwrap();
    let _ = api
        .wait_for_attachment_condition(Some(ATTACH_STARTED), Some(ATTACHMENT_WAIT_TIMEOUT))
        .await
        .unwrap();
    api.detach().await.unwrap();
    let _ = api
        .wait_for_attachment_condition(Some(DETACHED), Some(ATTACHMENT_WAIT_TIMEOUT))
        .await
        .unwrap();
    api.shutdown().await;

    info!("test_attach_detach: --- test auto detach ---");
    let (update_callback, config) = fixture_veilid_core();
    let api = api_startup(update_callback, config)
        .await
        .expect("startup failed");
    api.attach().await.unwrap();
    let _ = api
        .wait_for_attachment_condition(Some(ATTACH_STARTED), Some(ATTACHMENT_WAIT_TIMEOUT))
        .await
        .unwrap();
    api.shutdown().await;

    info!("test_attach_detach: --- test detach without attach ---");
    let (update_callback, config) = fixture_veilid_core();
    let api = api_startup(update_callback, config)
        .await
        .expect("startup failed");
    assert!(api.detach().await.is_err());
    api.shutdown().await;
}

async fn test_wait_for_attachment_condition() {
    info!("test_wait_for_attachment_condition: starting");
    let (update_callback, config) = fixture_veilid_core();
    let api = api_startup(update_callback, config)
        .await
        .expect("startup failed");

    let state = api
        .wait_for_attachment_condition(Some(DETACHED), None)
        .await
        .unwrap();
    assert_eq!(state.state, AttachmentState::Detached);

    // Never attached, so the default condition cannot hold
    assert!(matches!(
        api.wait_for_attachment_condition(None, Some(Duration::from_millis(500)))
            .await,
        Err(VeilidAPIError::Timeout)
    ));

    let inverted = VeilidStateAttachmentCondition {
        opt_min_state: Some(AttachmentState::AttachedFull),
        opt_max_state: Some(AttachmentState::AttachedWeak),
        ..Default::default()
    };
    assert!(matches!(
        api.wait_for_attachment_condition(Some(inverted), None)
            .await,
        Err(VeilidAPIError::InvalidArgument { .. })
    ));

    // An unbounded wait must not hold up shutdown
    let waiting = spawn("test_wait_for_attachment_condition", {
        let api = api.clone();
        async move { api.wait_for_attachment_condition(None, None).await }
    });
    api.shutdown().await;
    assert!(matches!(waiting.await, Err(VeilidAPIError::NotInitialized)));
    info!("test_wait_for_attachment_condition: finished");
}

async fn test_startup_shutdown_multiple() {
    info!("test_startup_shutdown_multiple: starting");
    let namespaces = (0..3).map(|x| format!("ns_{}", x)).collect::<Vec<_>>();
    let mut apis = vec![];
    for ns in &namespaces {
        let (update_callback, config) = fixture_veilid_core_with_namespace(ns);
        let api = api_startup(update_callback, config)
            .await
            .expect("startup failed");
        apis.push(api);
    }
    info!("test_startup_shutdown_multiple: shutting down");
    for api in apis {
        api.shutdown().await;
    }
    info!("test_startup_shutdown_multiple: finished");
}

async fn test_startup_shutdown_from_config_multiple() {
    info!("test_startup_from_config_multiple: starting");

    let namespaces = (0..3).map(|x| format!("ns_{}", x)).collect::<Vec<_>>();
    let mut apis = vec![];
    for ns in &namespaces {
        let (update_callback, config) = fixture_veilid_core_with_namespace(ns);
        let api = api_startup(update_callback, config)
            .await
            .expect("startup failed");
        apis.push(api);
    }
    info!("test_startup_from_config_multiple: shutting down");
    for api in apis {
        api.shutdown().await;
    }
    info!("test_startup_from_config_multiple: finished");
}

async fn test_attach_detach_multiple() {
    info!("test_attach_detach_multiple: --- test normal order ---");
    let namespaces = (0..3).map(|x| format!("ns_{}", x)).collect::<Vec<_>>();
    let mut apis = vec![];
    for ns in &namespaces {
        let (update_callback, config) = fixture_veilid_core_with_namespace(ns);
        let api = api_startup(update_callback, config)
            .await
            .expect("startup failed");
        apis.push(api);
    }
    for api in &apis {
        api.attach().await.unwrap();
    }
    for api in &apis {
        let _ = api
            .wait_for_attachment_condition(Some(ATTACH_STARTED), Some(ATTACHMENT_WAIT_TIMEOUT))
            .await
            .unwrap();
    }
    for api in &apis {
        api.detach().await.unwrap();
    }
    for api in &apis {
        let _ = api
            .wait_for_attachment_condition(Some(DETACHED), Some(ATTACHMENT_WAIT_TIMEOUT))
            .await
            .unwrap();
    }
    for api in apis {
        api.shutdown().await;
    }

    info!("test_attach_detach_multiple: --- test auto detach ---");
    let mut apis = vec![];
    for ns in &namespaces {
        let (update_callback, config) = fixture_veilid_core_with_namespace(ns);
        let api = api_startup(update_callback, config)
            .await
            .expect("startup failed");
        apis.push(api);
    }

    for api in &apis {
        api.attach().await.unwrap();
    }
    for api in &apis {
        let _ = api
            .wait_for_attachment_condition(Some(ATTACH_STARTED), Some(ATTACHMENT_WAIT_TIMEOUT))
            .await
            .unwrap();
    }
    for api in apis {
        api.shutdown().await;
    }

    info!("test_attach_detach_multiple: --- test detach without attach ---");
    let mut apis = vec![];
    for ns in &namespaces {
        let (update_callback, config) = fixture_veilid_core_with_namespace(ns);
        let api = api_startup(update_callback, config)
            .await
            .expect("startup failed");
        apis.push(api);
    }
    for api in &apis {
        assert!(api.detach().await.is_err());
    }
    for api in apis {
        api.shutdown().await;
    }
}

async fn test_change_config() {
    info!("test_change_config: starting");
    let (update_callback, config) = fixture_veilid_core();
    let api = api_startup(update_callback, config)
        .await
        .expect("startup failed");
    let current = api.config().unwrap();

    // Detached, a changeable key takes at once
    let mut flipped = (*current).clone();
    flipped.network.upnp = !flipped.network.upnp;
    api.change_config(flipped.clone()).unwrap();
    assert_eq!(api.config().unwrap().network.upnp, flipped.network.upnp);

    // A key that needs a restart is refused and nothing changes
    let mut restart = flipped.clone();
    restart.namespace = format!("{}x", restart.namespace);
    assert!(matches!(
        api.change_config(restart),
        Err(VeilidAPIError::InvalidArgument { .. })
    ));
    assert_eq!(api.config().unwrap().namespace, flipped.namespace);

    // A config as get_state reports it carries no secrets, and still applies
    let safe = (*api.config().unwrap().safe()).clone();
    api.change_config(safe).unwrap();
    assert_eq!(
        api.config().unwrap().network.routing_table.secret_keys,
        current.network.routing_table.secret_keys
    );

    // Attaching or attached, the call is refused
    api.change_config((*current).clone()).unwrap();
    api.attach().await.unwrap();
    let _ = api
        .wait_for_attachment_condition(Some(ATTACH_STARTED), Some(ATTACHMENT_WAIT_TIMEOUT))
        .await
        .unwrap();
    assert!(matches!(
        api.change_config((*current).clone()),
        Err(VeilidAPIError::Generic { .. })
    ));
    api.detach().await.unwrap();
    let _ = api
        .wait_for_attachment_condition(Some(DETACHED), Some(ATTACHMENT_WAIT_TIMEOUT))
        .await
        .unwrap();
    api.shutdown().await;
    info!("test_change_config: finished");
}

pub async fn test_all() {
    test_startup_shutdown().await;
    test_startup_shutdown_from_config().await;
    test_attach_detach().await;
    test_wait_for_attachment_condition().await;
    test_startup_shutdown_multiple().await;
    test_startup_shutdown_from_config_multiple().await;
    test_attach_detach_multiple().await;
    test_change_config().await;
}

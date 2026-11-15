use super::*;
use crate::tests::*;

async fn startup() -> VeilidAPI {
    info!("test_protected_store: starting");
    let (update_callback, config) = fixture_veilid_core();
    api_startup(update_callback, config)
        .await
        .expect("startup failed")
}

async fn shutdown(api: VeilidAPI) {
    info!("test_protected_store: shutting down");
    api.shutdown().await;
    info!("test_protected_store: finished");
}

async fn test_protected_store(ps: &ProtectedStore) {
    info!("testing protected store");

    let _ = ps
        .remove_user_secret(ProtectedStoreContext::Internal, "_test_key")
        .await;
    let _ = ps
        .remove_user_secret(ProtectedStoreContext::Internal, "_test_broken")
        .await;

    let d1: [u8; 0] = [];

    assert!(!ps
        .save_user_secret(
            ProtectedStoreContext::Internal,
            "_test_key",
            &[2u8, 3u8, 4u8]
        )
        .await
        .unwrap());
    info!("testing saving user secret");
    assert!(ps
        .save_user_secret(ProtectedStoreContext::Internal, "_test_key", &d1)
        .await
        .unwrap());
    info!("testing loading user secret");
    assert_eq!(
        ps.load_user_secret(ProtectedStoreContext::Internal, "_test_key")
            .await
            .unwrap(),
        Some(d1.to_vec())
    );
    info!("testing loading user secret again");
    assert_eq!(
        ps.load_user_secret(ProtectedStoreContext::Internal, "_test_key")
            .await
            .unwrap(),
        Some(d1.to_vec())
    );
    info!("testing loading broken user secret");
    assert_eq!(
        ps.load_user_secret(ProtectedStoreContext::Internal, "_test_broken")
            .await
            .unwrap(),
        None
    );
    info!("testing loading broken user secret again");
    assert_eq!(
        ps.load_user_secret(ProtectedStoreContext::Internal, "_test_broken")
            .await
            .unwrap(),
        None
    );
    info!("testing remove user secret");
    assert!(ps
        .remove_user_secret(ProtectedStoreContext::Internal, "_test_key")
        .await
        .unwrap());
    info!("testing remove user secret again");
    assert!(!ps
        .remove_user_secret(ProtectedStoreContext::Internal, "_test_key")
        .await
        .unwrap());
    info!("testing remove broken user secret");
    assert!(!ps
        .remove_user_secret(ProtectedStoreContext::Internal, "_test_broken")
        .await
        .unwrap());
    info!("testing remove broken user secret again");
    assert!(!ps
        .remove_user_secret(ProtectedStoreContext::Internal, "_test_broken")
        .await
        .unwrap());

    let d2: [u8; 10] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];

    assert!(!ps
        .save_user_secret(
            ProtectedStoreContext::Internal,
            "_test_key",
            &[2u8, 3u8, 4u8]
        )
        .await
        .unwrap());
    assert!(ps
        .save_user_secret(ProtectedStoreContext::Internal, "_test_key", &d2)
        .await
        .unwrap());
    assert_eq!(
        ps.load_user_secret(ProtectedStoreContext::Internal, "_test_key")
            .await
            .unwrap(),
        Some(d2.to_vec())
    );
    assert_eq!(
        ps.load_user_secret(ProtectedStoreContext::Internal, "_test_key")
            .await
            .unwrap(),
        Some(d2.to_vec())
    );
    assert_eq!(
        ps.load_user_secret(ProtectedStoreContext::Internal, "_test_broken")
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        ps.load_user_secret(ProtectedStoreContext::Internal, "_test_broken")
            .await
            .unwrap(),
        None
    );

    info!("testing list reflects a stored key");
    assert!(ps
        .list_user_secrets(ProtectedStoreContext::Internal)
        .await
        .unwrap()
        .contains(&"_test_key".to_owned()));

    assert!(ps
        .remove_user_secret(ProtectedStoreContext::Internal, "_test_key")
        .await
        .unwrap());
    assert!(!ps
        .remove_user_secret(ProtectedStoreContext::Internal, "_test_key")
        .await
        .unwrap());
    assert!(!ps
        .remove_user_secret(ProtectedStoreContext::Internal, "_test_key")
        .await
        .unwrap());
    assert!(!ps
        .remove_user_secret(ProtectedStoreContext::Internal, "_test_broken")
        .await
        .unwrap());

    info!("testing list no longer reports a removed key");
    assert!(!ps
        .list_user_secrets(ProtectedStoreContext::Internal)
        .await
        .unwrap()
        .contains(&"_test_key".to_owned()));

    info!("testing delete_all_user_secrets clears the store");
    ps.save_user_secret(ProtectedStoreContext::Internal, "_test_key", b"hello")
        .await
        .unwrap();
    ps.delete_all_user_secrets(ProtectedStoreContext::Internal)
        .await
        .unwrap();
    assert!(ps
        .list_user_secrets(ProtectedStoreContext::Internal)
        .await
        .unwrap()
        .is_empty());
}

/// Veilid's own keys must be invisible and untouchable through the application-facing context.
async fn test_reserved_keys_hidden_from_api(ps: &ProtectedStore) {
    info!("testing reserved keys are inaccessible via the api context");

    // Stored the way veilid-core itself does.
    ps.save_user_secret(
        ProtectedStoreContext::Internal,
        "device_encryption_key",
        b"dek",
    )
    .await
    .unwrap();
    ps.save_user_secret(ProtectedStoreContext::Api, "_app_key", b"app")
        .await
        .unwrap();

    let api_keys = ps
        .list_user_secrets(ProtectedStoreContext::Api)
        .await
        .unwrap();
    assert!(!api_keys.contains(&"device_encryption_key".to_owned()));
    assert!(api_keys.contains(&"_app_key".to_owned()));

    let internal_keys = ps
        .list_user_secrets(ProtectedStoreContext::Internal)
        .await
        .unwrap();
    assert!(internal_keys.contains(&"device_encryption_key".to_owned()));

    // Direct access by name is rejected, not silently ignored.
    assert!(ps
        .load_user_secret(ProtectedStoreContext::Api, "device_encryption_key")
        .await
        .is_err());
    assert!(ps
        .save_user_secret(ProtectedStoreContext::Api, "device_encryption_key", b"evil")
        .await
        .is_err());
    assert!(ps
        .remove_user_secret(ProtectedStoreContext::Api, "device_encryption_key")
        .await
        .is_err());

    // Wiping app secrets leaves veilid's key intact.
    ps.delete_all_user_secrets(ProtectedStoreContext::Api)
        .await
        .unwrap();
    assert_eq!(
        ps.load_user_secret(ProtectedStoreContext::Internal, "device_encryption_key")
            .await
            .unwrap(),
        Some(b"dek".to_vec())
    );

    ps.remove_user_secret(ProtectedStoreContext::Internal, "device_encryption_key")
        .await
        .unwrap();
}

pub async fn test_all() {
    let api = startup().await;
    let ps = api.protected_store().unwrap();
    test_protected_store(&ps).await;
    test_reserved_keys_hidden_from_api(&ps).await;

    shutdown(api).await;
}

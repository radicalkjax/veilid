use super::*;
use crate::routing_table::{PrivateRoute, RouteNode};
use crate::rpc_processor::Destination;
use crate::tests::*;

fn record_key(n: u8) -> OpaqueRecordKey {
    OpaqueRecordKey::new(CRYPTO_KIND_VLD0, BareOpaqueRecordKey::new(&[n; 32]))
}

fn watch_params(
    tag: u8,
    subkeys: ValueSubkeyRangeSet,
    expiration: Timestamp,
) -> InboundWatchParameters {
    let ck = CRYPTO_KIND_VLD0;
    InboundWatchParameters {
        subkeys,
        expiration,
        count: 100,
        watcher_member_id: MemberId::new(ck, BareMemberId::new(&[tag; 32])),
        dest: Destination::private_route(
            Arc::new(PrivateRoute::new_stub(
                PublicKey::new(ck, BarePublicKey::new(&[tag; 32])),
                RouteNode::NodeId(NodeId::new(ck, BareNodeId::new(&[tag; 32]))),
            )),
            SafetySelection::Unsafe(Sequencing::PreferOrdered),
        ),
    }
}

fn ignore_dropped<T>(_: T) {}

fn fail_on_error(e: VeilidAPIError) {
    panic!("sweep error: {e}");
}

/// The sweep drops what is due and reports the earliest due time left; a watch over no subkeys
/// can never fire, so it is due at once
pub fn test_inbound_watch_next_due() {
    let now = Timestamp::new(1_000_000_000);
    let secs = |n: u32| now.later(TimestampDuration::new_secs(n));

    let mut watches = InboundWatches::new();
    for (key, tag, subkeys, expiration) in [
        (record_key(1), 1u8, ValueSubkeyRangeSet::single(0), secs(10)),
        (record_key(1), 2u8, ValueSubkeyRangeSet::new(), secs(30)),
        (record_key(2), 3u8, ValueSubkeyRangeSet::single(0), secs(20)),
    ] {
        watches
            .allocate(&key, watch_params(tag, subkeys, expiration))
            .expect("allocate");
    }

    let sweep = |watches: &mut InboundWatches, at: Timestamp| {
        watches.remove_expired_watches(at, &ignore_dropped, &fail_on_error)
    };
    assert_eq!(sweep(&mut watches, now), Some(secs(10)));
    assert_eq!(sweep(&mut watches, secs(10)), Some(secs(20)));
    assert_eq!(sweep(&mut watches, secs(20)), None);
}

/// The sweep drops the transactions that expired and reports the earliest expiration left
pub fn test_inbound_transaction_next_expiration() {
    let ck = CRYPTO_KIND_VLD0;
    let now = Timestamp::new(1_000_000_000);
    let secs = |n: u32| now.later(TimestampDuration::new_secs(n));

    let schema_data: Bytes = DHTSchema::dflt(1).expect("schema").compile().into();
    let descriptor = Arc::new(SignedValueDescriptor::new(
        PublicKey::new(ck, BarePublicKey::new(&[9u8; 32])),
        schema_data,
        Signature::new(ck, BareSignature::new(&[0u8; 64])),
    ));

    let mut transactions = InboundTransactions::new();
    for (key, tag, expiration) in [
        (record_key(1), 1u8, secs(20)),
        (record_key(1), 2u8, secs(10)),
        (record_key(2), 3u8, secs(30)),
    ] {
        transactions
            .allocate(
                &key,
                expiration,
                MemberId::new(ck, BareMemberId::new(&[tag; 32])),
                descriptor.clone(),
                None,
            )
            .expect("allocate");
    }

    let sweep = |transactions: &mut InboundTransactions, at: Timestamp| {
        transactions.remove_expired_transactions(at, &ignore_dropped, &fail_on_error)
    };
    assert_eq!(sweep(&mut transactions, now), Some(secs(10)));
    assert_eq!(sweep(&mut transactions, secs(10)), Some(secs(20)));
    assert_eq!(sweep(&mut transactions, secs(25)), Some(secs(30)));
    assert_eq!(sweep(&mut transactions, secs(30)), None);
}

/// A watch on the remote store goes when it expires, with no tick: creating it arms the storage
/// manager's timer, and the sweep arms it again for the watch left
pub async fn test_inbound_watch_expiry_timer() {
    let (update_callback, config) = fixture_veilid_core();
    let api = api_startup(update_callback, config)
        .await
        .expect("startup failed");
    let registry = api.core_context().unwrap().registry();
    let storage_manager = registry.storage_manager();
    let crypto = api.crypto().unwrap();

    let ck = CRYPTO_KIND_VLD0;
    let vcrypto = crypto.get(ck).expect("vld0 cryptosystem");
    let owner = vcrypto.generate_keypair();
    let schema_data: Bytes = DHTSchema::dflt(1).expect("schema").compile().into();
    let opaque_record_key =
        make_opaque_record_key(&vcrypto.as_async(), owner.key().ref_value(), &schema_data).await;
    let descriptor = SignedValueDescriptor::new(
        owner.key(),
        schema_data,
        Signature::new(ck, BareSignature::new(&[0u8; 64])),
    );

    let store = storage_manager
        .get_remote_record_store()
        .expect("remote record store");
    let record = Record::<RemoteRecordDetail>::try_new(
        Timestamp::now(),
        Arc::new(descriptor),
        RemoteRecordDetail {},
    )
    .expect("record");
    store
        .try_new_record(opaque_record_key.clone(), record)
        .await
        .expect("try_new_record");

    // Straight to the inner store: watch_record refuses lifetimes under the rpc timeout
    let watch = |tag: u8, expiration: Timestamp| {
        let params = watch_params(tag, ValueSubkeyRangeSet::single(0), expiration);
        let result = store
            .inner
            .lock()
            .create_new_watch(&opaque_record_key, params, Box::new(|_| false))
            .expect("create_new_watch");
        match result {
            InboundWatchValueResult::Created { id, .. } => id,
            other => panic!("watch {tag} should have been created, got {other:?}"),
        }
    };
    let is_alive = |id: InboundWatchId| {
        store
            .lookup_inbound_watch_id(id.into())
            .expect("lookup")
            .is_some()
    };

    let long_id = watch(1, Timestamp::now().later(TimestampDuration::new_secs(60)));
    let expiration = Timestamp::now().later(TimestampDuration::new_ms(300));
    let short_id = watch(2, expiration);

    let gone_at = loop {
        if !is_alive(short_id) {
            break Timestamp::now();
        }
        assert!(
            Timestamp::now() < expiration.later(TimestampDuration::new_secs(2)),
            "the expired watch was never dropped"
        );
        sleep(10).await;
    };
    assert!(gone_at >= expiration, "dropped before it expired");
    assert!(
        gone_at.duration_since(expiration) < TimestampDuration::new_ms(250),
        "dropped {} after it expired",
        gone_at.duration_since(expiration)
    );
    assert!(is_alive(long_id), "the watch that has not expired is kept");

    api.shutdown().await;
}

pub async fn test_all() {
    test_inbound_watch_next_due();
    test_inbound_transaction_next_expiration();
    test_inbound_watch_expiry_timer().await;
}

use super::*;

/// A question written without the field, as every older node writes it, asks for the default
pub fn test_absent_fanout_config_is_the_default() {
    let mut message = ::capnp::message::Builder::new_default();
    let builder = message.init_root::<veilid_capnp::operation_get_value_q::Builder>();
    let reader = builder.into_reader();
    assert_eq!(
        FanoutConfig::decode(&reader.get_fanout_config().expect("default struct")),
        FanoutConfig::default()
    );
    assert_eq!(
        FanoutConfig::default().max_peers,
        veilid_capnp::MAX_INSPECT_VALUE_A_PEERS_LEN,
        "the default asks for everything an inspect answer may hold"
    );
}

/// Zero is a real wish, and a wish past the node's limit gets the limit, not a refusal
pub fn test_fanout_config_round_trip_and_cap() {
    for max_peers in [0u32, 3, 20, 100, u32::MAX] {
        let mut message = ::capnp::message::Builder::new_default();
        let mut builder = message.init_root::<veilid_capnp::fanout_config::Builder>();
        FanoutConfig { max_peers }.encode(&mut builder);
        let decoded = FanoutConfig::decode(&builder.into_reader());
        assert_eq!(decoded.max_peers, max_peers);
        assert_eq!(decoded.peers_limit(20), (max_peers as usize).min(20));
    }
    assert_eq!(FanoutConfig::NO_PEERS.peers_limit(20), 0);
}

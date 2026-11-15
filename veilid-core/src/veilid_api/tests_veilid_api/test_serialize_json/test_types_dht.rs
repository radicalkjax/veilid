use super::*;
use crate::crypto::tests_crypto::*;

// dht_record_descriptors

pub fn test_dht_record_descriptor() {
    let orig = DHTRecordDescriptor::new(
        fake_record_key(),
        fake_public_key(),
        Some(fake_secret_key()),
        DHTSchema::dflt(321).unwrap(),
    );
    let copy = deserialize_json(&serialize_json(&orig)).unwrap();

    assert_eq!(orig, copy);
}

// value_data

pub fn test_value_data() {
    let orig = ValueData::try_new_with_seq(42.into(), b"Brent Spiner".to_vec(), fake_public_key());
    let copy = deserialize_json(&serialize_json(&orig)).unwrap();

    assert_eq!(orig, copy);
}

// value_subkey_range_set

pub fn test_value_subkey_range_set() {
    let orig = ValueSubkeyRangeSet::single_range(20, 30);
    let copy = deserialize_json(&serialize_json(&orig)).unwrap();

    assert_eq!(orig, copy);
}

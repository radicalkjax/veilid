use super::*;
use crate::crypto::tests_crypto::*;
use data_encoding::BASE64URL_NOPAD;

fn random_value(size: usize) -> EncryptedValueData {
    let mut data = vec![0u8; size];
    random_bytes(&mut data);
    EncryptedValueData::try_new(42.into(), Bytes::from(data), fake_public_key(), None).unwrap()
}

fn max_value_size() -> usize {
    VeilidConfigTableStore::default().max_value_size_mb as usize * 1024 * 1024
}

pub fn test_value_data_ok() {
    assert!(EncryptedValueData::try_new(
        ValueSeqNum::ZERO,
        Bytes::from(vec![0; EncryptedValueData::MAX_LEN]),
        fake_public_key(),
        None,
    )
    .is_ok());
}

pub fn test_value_data_too_long() {
    assert!(EncryptedValueData::try_new(
        ValueSeqNum::ZERO,
        Bytes::from(vec![0; EncryptedValueData::MAX_LEN + 1]),
        fake_public_key(),
        None,
    )
    .is_err());
}

/// JSON carries the blob as base64 text
pub fn test_serialize_deserialize() {
    let orig = EncryptedValueData::try_new(
        42.into(),
        Bytes::copy_from_slice(b"Brent Spiner"),
        fake_public_key(),
        None,
    )
    .unwrap();
    let json = serialize_json(&orig);
    assert_eq!(
        json,
        format!("\"{}\"", BASE64URL_NOPAD.encode(orig.raw_blob()))
    );
    let copy = deserialize_json(&json).unwrap();

    assert_eq!(orig, copy);
}

/// The table store carries the blob as bytes, smaller than the text form it also still reads
pub fn test_table_value_forms() {
    let orig = random_value(32 * 1024);

    let bytes_form = serialize_table_value(&orig);
    let copy: EncryptedValueData = deserialize_table_value(&bytes_form, max_value_size()).unwrap();
    assert_eq!(orig, copy);

    let text_form = serialize_table_value(&BASE64URL_NOPAD.encode(orig.raw_blob()));
    let copy: EncryptedValueData = deserialize_table_value(&text_form, max_value_size()).unwrap();
    assert_eq!(orig, copy);

    assert!(bytes_form.0.len() < text_form.0.len());
}

/// A row written before the capnp blob, in the field form, still reads
pub fn test_deserialize_legacy_fields() {
    let data = Bytes::copy_from_slice(b"Brent Spiner");
    let writer = fake_public_key();
    let legacy = serialize_json(
        ValueData::try_new_with_seq(42.into(), data.clone(), writer.clone()).unwrap(),
    );
    let orig = EncryptedValueData::try_new(42.into(), data, writer, None).unwrap();

    let copy: EncryptedValueData = deserialize_json(&legacy).unwrap();

    assert_eq!(orig, copy);
}

#[expect(clippy::unused_async)]
pub async fn test_all() {
    test_value_data_ok();
    test_value_data_too_long();
    test_serialize_deserialize();
    test_table_value_forms();
    test_deserialize_legacy_fields();
}

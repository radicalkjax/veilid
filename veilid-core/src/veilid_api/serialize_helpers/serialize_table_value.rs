//! The table store's own value encoding: `"CBOR"` + ciborium, wrapped as `"LZ4C"` + lz4 size-prepended
//! when that is smaller. Reading also accepts the bare JSON a v0 store wrote.
#![cfg_attr(
    feature = "mock-api",
    allow(dead_code, reason = "only the tests use the encoding against the mock")
)]

use super::*;

/// A value in the table store's own encoding. Nested in another serialized value it travels as
/// bytes (base64 in JSON).
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct TableDBSerializedData(#[serde(with = "as_human_base64")] pub Bytes);

/// Serialize `value` for the table store
pub(crate) fn serialize_table_value<T: Serialize>(value: &T) -> TableDBSerializedData {
    let mut cbor_buf = Vec::with_capacity(4096);
    cbor_buf.extend_from_slice(b"CBOR");
    ciborium::into_writer(value, &mut cbor_buf).expect("serialize into Vec must not fail");

    let compressed_cbor = compress_prepend_size(&cbor_buf);
    if compressed_cbor.len() >= cbor_buf.len() {
        return TableDBSerializedData(Bytes::from(cbor_buf));
    }
    let mut compressed_buf = Vec::with_capacity(4 + compressed_cbor.len());
    compressed_buf.extend_from_slice(b"LZ4C");
    compressed_buf.extend_from_slice(&compressed_cbor);
    TableDBSerializedData(Bytes::from(compressed_buf))
}

/// Deserialize a value `serialize_table_value` wrote, or the bare JSON a v0 store wrote,
/// decompressing at most `max_value_size` bytes
pub(crate) fn deserialize_table_value<T: for<'a> Deserialize<'a>>(
    serialized: &TableDBSerializedData,
    max_value_size: usize,
) -> VeilidAPIResult<T> {
    let mut buf = Cow::Borrowed(serialized.0.as_ref());
    if let Some(compressed) = buf.strip_prefix(b"LZ4C") {
        buf = Cow::Owned(decompress_size_prepended(compressed, max_value_size)?);
    }

    let fail = |err: &dyn fmt::Display| {
        VeilidAPIError::internal(format!("{err}: {}", human_byte_data(&buf, Some(64))))
    };
    if let Some(cbor) = buf.strip_prefix(b"CBOR") {
        return ciborium::from_reader(cbor).map_err(|e| fail(&e));
    }
    // No JSON text starts with a format header
    serde_json::from_slice(&buf).map_err(|e| fail(&e))
}

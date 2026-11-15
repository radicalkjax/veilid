use super::*;
use data_encoding::BASE64URL_NOPAD;

#[derive(Clone, Eq, PartialEq, Hash)]
#[must_use]
pub struct EncryptedValueData {
    // capnp struct ValueData encoded without packing
    blob: Bytes,
}

impl EncryptedValueData {
    pub const MAX_LEN: usize = MAX_SUBKEY_SIZE;

    pub fn try_new(
        seq: ValueSeqNum,
        data: Bytes,
        writer: PublicKey,
        nonce: Option<Nonce>,
    ) -> VeilidAPIResult<Self> {
        if seq.is_none() {
            apibail_generic!("invalid sequence number");
        }
        if data.len() > Self::MAX_LEN {
            apibail_generic!("invalid size");
        }

        let mut message_builder = ::capnp::message::Builder::new_default();
        let mut builder = message_builder.init_root::<veilid_capnp::value_data::Builder>();

        builder.set_seq(seq.into());

        builder.set_data(&data);

        let mut wb = builder.reborrow().init_writer();
        capnp_encode_public_key(&writer, &mut wb);

        if let Some(nonce_val) = nonce {
            let mut nb = builder.reborrow().init_nonce();
            capnp_encode_nonce(&nonce_val, &mut nb);
        }

        let blob = canonical_message_builder_to_bytes_writer_unpacked(message_builder, |size| {
            BytesWriter::with_capacity(size)
        })
        .map_err(VeilidAPIError::internal)?
        .into_inner()
        .freeze();

        // Ensure the blob could be decoded without errors, allowing to do unwrap() in getter methods
        validate_value_data_blob(&blob).map_err(VeilidAPIError::internal)?;

        Ok(Self { blob })
    }

    #[must_use]
    pub fn seq(&self) -> ValueSeqNum {
        let message_reader = capnp::serialize::read_message_from_flat_slice(
            &mut &self.blob[..],
            capnp::message::ReaderOptions::new(),
        )
        .unwrap_or_log();
        let reader = message_reader
            .get_root::<veilid_capnp::value_data::Reader>()
            .unwrap_or_log();

        reader.get_seq().into()
    }

    pub fn writer(&self) -> PublicKey {
        let message_reader = capnp::serialize::read_message_from_flat_slice(
            &mut &self.blob[..],
            capnp::message::ReaderOptions::new(),
        )
        .unwrap_or_log();
        let reader = message_reader
            .get_root::<veilid_capnp::value_data::Reader>()
            .unwrap_or_log();

        let w = reader.get_writer().unwrap_or_log();
        PublicKey::new(
            CryptoKind::from(w.get_kind()),
            BarePublicKey::new(w.get_value().unwrap_or_log()),
        )
    }

    #[must_use]
    pub fn data(&self) -> Bytes {
        let message_reader = capnp::serialize::read_message_from_flat_slice(
            &mut &self.blob[..],
            capnp::message::ReaderOptions::new(),
        )
        .unwrap_or_log();
        let reader = message_reader
            .get_root::<veilid_capnp::value_data::Reader>()
            .unwrap_or_log();

        Bytes::copy_from_slice(reader.get_data().unwrap_or_log())
    }

    #[must_use]
    pub fn nonce(&self) -> Option<Nonce> {
        let message_reader = capnp::serialize::read_message_from_flat_slice(
            &mut &self.blob[..],
            capnp::message::ReaderOptions::new(),
        )
        .unwrap_or_log();
        let reader = message_reader
            .get_root::<veilid_capnp::value_data::Reader>()
            .unwrap_or_log();

        if reader.has_nonce() {
            let n = reader.get_nonce().unwrap_or_log();
            Some(Nonce::new(n.get_value().unwrap_or_log()))
        } else {
            None
        }
    }

    #[must_use]
    pub fn data_size(&self) -> usize {
        let message_reader = capnp::serialize::read_message_from_flat_slice(
            &mut &self.blob[..],
            capnp::message::ReaderOptions::new(),
        )
        .unwrap_or_log();
        let reader = message_reader
            .get_root::<veilid_capnp::value_data::Reader>()
            .unwrap_or_log();

        reader.get_data().unwrap_or_log().len()
    }

    /// In-memory footprint, including the capnp blob
    #[must_use]
    pub fn total_size(&self) -> usize {
        mem::size_of::<Self>() + self.blob.len()
    }

    #[must_use]
    pub fn raw_blob(&self) -> &[u8] {
        &self.blob
    }
}

fn validate_value_data_blob(blob: &[u8]) -> capnp::Result<()> {
    let message_reader = capnp::serialize::read_message_from_flat_slice(
        &mut &blob[..],
        capnp::message::ReaderOptions::new(),
    )?;
    let reader = message_reader.get_root::<veilid_capnp::value_data::Reader>()?;
    if ValueSeqNum::from(reader.get_seq()).is_none() {
        return capnp::Result::Err(capnp::Error::failed("invalid seq".to_owned()));
    }
    let _ = reader.get_data()?;
    let _ = reader.get_writer()?;
    if reader.has_nonce() {
        let n = reader.get_nonce()?;
        let _ = n.get_value()?;
    }
    Ok(())
}

impl fmt::Debug for EncryptedValueData {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let seq = self.seq();
        let data = self.data();
        let writer = self.writer();
        let nonce = self.nonce();

        fmt.debug_struct("EncryptedValueData")
            .field("seq", &seq)
            .field("data", &human_byte_data(&data, Some(64)))
            .field("writer", &writer)
            .field("nonce", &nonce)
            .finish()
    }
}

impl serde::Serialize for EncryptedValueData {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            serializer.serialize_str(&BASE64URL_NOPAD.encode(&self.blob))
        } else {
            serializer.serialize_bytes(&self.blob)
        }
    }
}

impl<'de> serde::Deserialize<'de> for EncryptedValueData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        /// The field form of rows written before the capnp blob
        #[derive(Deserialize)]
        struct LegacyValueData {
            seq: ValueSeqNum,
            #[cfg_attr(
                not(all(target_arch = "wasm32", target_os = "unknown")),
                serde(with = "as_human_base64")
            )]
            data: Vec<u8>,
            #[serde(with = "public_key_try_untyped_vld0")]
            writer: PublicKey,
        }

        /// Bytes, base64 text or the field form, in any format: rows written before the bytes
        /// form hold text, and serde's buffered replay claims every format is human-readable
        struct BlobVisitor;

        impl BlobVisitor {
            fn from_blob<E: serde::de::Error>(blob: Vec<u8>) -> Result<EncryptedValueData, E> {
                validate_value_data_blob(&blob).map_err(|e| {
                    E::custom(format!(
                        "Decoded blob is not a valid ValueData capnp struct: {e}"
                    ))
                })?;
                Ok(EncryptedValueData { blob: blob.into() })
            }
        }

        impl<'de> serde::de::Visitor<'de> for BlobVisitor {
            type Value = EncryptedValueData;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a ValueData blob as bytes, base64 text or fields")
            }

            fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
                Self::from_blob(v.to_vec())
            }

            fn visit_byte_buf<E: serde::de::Error>(self, v: Vec<u8>) -> Result<Self::Value, E> {
                Self::from_blob(v)
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                let blob = BASE64URL_NOPAD
                    .decode(v.as_bytes())
                    .map_err(|e| E::custom(format!("Failed to decode base64: {e}")))?;
                Self::from_blob(blob)
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                let legacy = LegacyValueData::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )?;
                EncryptedValueData::try_new(legacy.seq, legacy.data.into(), legacy.writer, None)
                    .map_err(serde::de::Error::custom)
            }
        }

        deserializer.deserialize_any(BlobVisitor)
    }
}

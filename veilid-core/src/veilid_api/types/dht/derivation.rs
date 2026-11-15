use super::*;

/// Build the record key for a given schema and owner
pub(crate) async fn dht_record_key(
    crypto: &Crypto,
    schema: DHTSchema,
    owner_key: &PublicKey,
    encryption_key: Option<SharedSecret>,
) -> VeilidAPIResult<RecordKey> {
    let Some(vcrypto) = crypto.get_async(owner_key.kind()) else {
        apibail_generic!("unsupported cryptosystem");
    };

    // Encryption key must match owner key
    if let Some(ek) = &encryption_key {
        vcrypto.check_shared_secret(ek)?;
    }

    schema.validate()?;
    let schema_data = schema.compile();

    Ok(make_record_key(
        &vcrypto,
        owner_key.ref_value(),
        &schema_data,
        encryption_key.map(|x| x.into_value()),
    )
    .await)
}

/// Validate a record key
pub(crate) fn check_dht_record_key(crypto: &Crypto, record_key: &RecordKey) -> VeilidAPIResult<()> {
    let Some(vcrypto) = crypto.get(record_key.kind()) else {
        apibail_generic!("unsupported record key kind");
    };

    if record_key.value().key().len() != HASH_COORDINATE_LENGTH {
        apibail_generic!(
            "invalid record key length: {} != {}",
            record_key.value().key().len(),
            HASH_COORDINATE_LENGTH
        );
    }
    if let Some(encryption_key) = record_key.value().encryption_key() {
        if encryption_key.len() != vcrypto.shared_secret_length() {
            apibail_generic!(
                "invalid encryption key length: {} != {}",
                encryption_key.len(),
                vcrypto.shared_secret_length()
            );
        }
    }

    Ok(())
}

pub(crate) async fn make_opaque_record_key(
    vcrypto: &AsyncCryptoSystemGuard<'_>,
    owner_key: &BarePublicKey,
    schema_data: &[u8],
) -> OpaqueRecordKey {
    let mut hash_data = BytesMut::with_capacity(owner_key.len() + 4 + schema_data.len());
    hash_data.extend_from_slice(vcrypto.kind().bytes());
    hash_data.extend_from_slice(owner_key);
    hash_data.extend_from_slice(schema_data);
    let hash = vcrypto.generate_hash(hash_data.freeze()).await;

    OpaqueRecordKey::new(vcrypto.kind(), BareOpaqueRecordKey::new(hash.ref_value()))
}

pub(crate) async fn make_record_key(
    vcrypto: &AsyncCryptoSystemGuard<'_>,
    owner_key: &BarePublicKey,
    schema_data: &[u8],
    encryption_key: Option<BareSharedSecret>,
) -> RecordKey {
    let opaque = make_opaque_record_key(vcrypto, owner_key, schema_data).await;

    RecordKey::new(
        vcrypto.kind(),
        BareRecordKey::new(opaque.into_value(), encryption_key),
    )
}

/// Produce member id from writer public key
pub(crate) fn generate_member_id(
    crypto: &Crypto,
    writer_key: &PublicKey,
) -> VeilidAPIResult<MemberId> {
    if writer_key.ref_value().len() == MEMBER_ID_LENGTH {
        return Ok(MemberId::new(
            writer_key.kind(),
            BareMemberId::new(writer_key.ref_value()),
        ));
    }

    let Some(vcrypto) = crypto.get(writer_key.kind()) else {
        apibail_generic!("unsupported cryptosystem");
    };

    let idhash = vcrypto.generate_hash(writer_key.ref_value());
    if idhash.ref_value().len() < MEMBER_ID_LENGTH {
        apibail_internal!(format!(
            "generate_hash needs to produce at least {} bytes",
            MEMBER_ID_LENGTH
        ));
    }
    Ok(MemberId::new(
        writer_key.kind(),
        BareMemberId::new(&idhash.ref_value()[0..MEMBER_ID_LENGTH]),
    ))
}

/// Check a subkey write against the schema
pub(crate) fn check_subkey_value_data(
    crypto: &Crypto,
    schema: &DHTSchema,
    owner: &PublicKey,
    subkey: ValueSubkey,
    writer: &PublicKey,
    data_size: usize,
) -> VeilidAPIResult<()> {
    match schema {
        DHTSchema::DFLT(d) => check_subkey_value_data_dflt(d, owner, subkey, writer, data_size),
        DHTSchema::SMPL(s) => {
            check_subkey_value_data_smpl(crypto, s, owner, subkey, writer, data_size)
        }
    }
}

/// Check a subkey write against the DFLT schema
pub(crate) fn check_subkey_value_data_dflt(
    schema_dflt: &DHTSchemaDFLT,
    owner: &PublicKey,
    subkey: ValueSubkey,
    writer: &PublicKey,
    data_size: usize,
) -> VeilidAPIResult<()> {
    let subkey = subkey as usize;

    // Check if subkey is in owner range
    if subkey < (schema_dflt.o_cnt() as usize) {
        // Check value data has valid writer
        if writer == owner {
            let max_value_len = usize::min(
                MAX_SUBKEY_SIZE,
                MAX_RECORD_DATA_SIZE / schema_dflt.o_cnt() as usize,
            );

            // Ensure value size is within additional limit
            if data_size <= max_value_len {
                return Ok(());
            }

            // Value too big
            apibail_invalid_argument!(
                "value too big",
                "data",
                format!("{} > {}", data_size, max_value_len)
            );
        }

        // Wrong writer
        apibail_invalid_argument!(
            "wrong writer",
            "writer",
            format!("{:?} != {:?}", writer, owner)
        );
    }

    // Subkey out of range
    apibail_invalid_argument!("subkey out of range", "subkey", subkey);
}

/// Check a subkey write against the SMPL schema
pub(crate) fn check_subkey_value_data_smpl(
    crypto: &Crypto,
    schema_smpl: &DHTSchemaSMPL,
    owner: &PublicKey,
    subkey: ValueSubkey,
    writer: &PublicKey,
    data_size: usize,
) -> VeilidAPIResult<()> {
    let mut cur_subkey = subkey as usize;

    let max_value_len = usize::min(
        MAX_SUBKEY_SIZE,
        MAX_RECORD_DATA_SIZE / schema_smpl.subkey_count(),
    );

    // Check if subkey is in owner range
    if cur_subkey < (schema_smpl.o_cnt() as usize) {
        // Check value data has valid writer
        if writer == owner {
            // Ensure value size is within additional limit
            if data_size <= max_value_len {
                return Ok(());
            }

            // Value too big
            apibail_invalid_argument!(
                "value too big",
                "data",
                format!("{} > {}", data_size, max_value_len)
            );
        }
        // Wrong writer
        apibail_invalid_argument!("wrong writer", "writer", format!("{:?}", writer));
    }
    cur_subkey -= schema_smpl.o_cnt() as usize;

    let writer_hash = generate_member_id(crypto, writer)?;

    // Check all member ranges
    for m in schema_smpl.members() {
        // Check if subkey is in member range
        if cur_subkey < (m.m_cnt as usize) {
            // Check value data has valid writer
            if writer_hash.ref_value() == &m.m_key {
                // Ensure value size is in allowed range
                if data_size <= max_value_len {
                    return Ok(());
                }

                // Value too big
                apibail_invalid_argument!(
                    "value too big",
                    "data",
                    format!("{} > {}", data_size, max_value_len)
                );
            }
            // Wrong writer
            apibail_invalid_argument!("wrong writer", "writer", format!("{:?}", writer));
        }
        cur_subkey -= m.m_cnt as usize;
    }

    // Subkey out of range
    apibail_invalid_argument!("subkey out of range", "subkey", subkey);
}

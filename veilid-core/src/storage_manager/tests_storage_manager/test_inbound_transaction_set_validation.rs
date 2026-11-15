use super::*;
use crate::tests::*;

/// A DFLT record whose only writer is `owner`, on a node that stores it for the network
struct Fixture {
    api: VeilidAPI,
    owner: KeyPair,
    outsider: KeyPair,
    descriptor: SignedValueDescriptor,
    opaque_record_key: OpaqueRecordKey,
}

impl Fixture {
    async fn startup() -> Self {
        let (update_callback, config) = fixture_veilid_core();
        let api = api_startup(update_callback, config)
            .await
            .expect("startup failed");
        let crypto = api.crypto().unwrap();
        let vcrypto = crypto.get(CRYPTO_KIND_VLD0).expect("vld0 cryptosystem");
        let owner = vcrypto.generate_keypair();
        let outsider = vcrypto.generate_keypair();
        let avcrypto = vcrypto.as_async();

        let schema_data: Bytes = DHTSchema::dflt(1).expect("schema").compile().into();
        let descriptor = SignedValueDescriptor::make_signature(
            owner.key(),
            schema_data.clone(),
            &avcrypto,
            owner.secret(),
        )
        .await
        .expect("descriptor");
        let opaque_record_key =
            make_opaque_record_key(&avcrypto, owner.key().ref_value(), &schema_data).await;

        Self {
            api,
            owner,
            outsider,
            descriptor,
            opaque_record_key,
        }
    }

    fn storage_manager(&self) -> VeilidComponentGuard<'_, StorageManager> {
        self.api
            .core_context()
            .unwrap()
            .registry()
            .storage_manager()
    }

    fn value_data(&self, writer: &KeyPair, seq: u32, data: &'static [u8]) -> EncryptedValueData {
        let crypto = self.api.crypto().unwrap();
        let vcrypto = crypto.get(CRYPTO_KIND_VLD0).expect("vld0 cryptosystem");
        EncryptedValueData::try_new(
            ValueSeqNum::from(seq),
            Bytes::from_static(data),
            writer.key(),
            Some(vcrypto.random_nonce()),
        )
        .expect("value data")
    }

    /// A value `writer` signed, as its own client would send it
    async fn signed_value(
        &self,
        writer: &KeyPair,
        seq: u32,
        data: &'static [u8],
    ) -> Arc<SignedValueData> {
        let crypto = self.api.crypto().unwrap();
        let avcrypto = crypto
            .get_async(CRYPTO_KIND_VLD0)
            .expect("vld0 cryptosystem");
        Arc::new(
            SignedValueData::make_signature(
                self.value_data(writer, seq, data),
                &self.owner.key(),
                0,
                &avcrypto,
                &writer.secret(),
            )
            .await
            .expect("signature"),
        )
    }

    /// A value naming `writer` that nobody signed
    fn forged_value(
        &self,
        writer: &KeyPair,
        seq: u32,
        data: &'static [u8],
    ) -> Arc<SignedValueData> {
        // All-zero is a syntactically valid 64-byte Ed25519 signature that verifies against nothing
        let garbage_sig = Signature::new(CRYPTO_KIND_VLD0, BareSignature::new(&[0u8; 64]));
        Arc::new(SignedValueData::new(
            self.value_data(writer, seq, data),
            garbage_sig,
        ))
    }

    /// Begin, set, end and commit one value on subkey 0 as `signer`'s client would over the
    /// wire, stopping at the first command that is not a success
    async fn transact_set(
        &self,
        signer: &KeyPair,
        value: Arc<SignedValueData>,
    ) -> Vec<NetworkResult<InboundTransactCommandResult>> {
        let storage_manager = self.storage_manager();
        let signing_member_id = storage_manager
            .generate_member_id(&signer.key())
            .expect("member id");
        let NetworkResult::Value(InboundTransactBeginResult::Success(begin)) = storage_manager
            .inbound_transact_begin(
                self.opaque_record_key.clone(),
                Some(self.descriptor.clone()),
                false,
                signing_member_id,
            )
            .await
            .expect("begin should not error")
        else {
            panic!("begin refused");
        };
        let transaction_id = u64::from(begin.transaction_id);

        let mut results = vec![];
        for (command, opt_subkey, opt_value) in [
            (TransactCommand::Set, Some(0), Some(value)),
            (TransactCommand::End, None, None),
            (TransactCommand::Commit, None, None),
        ] {
            let result = storage_manager
                .inbound_transact_command(
                    &self.opaque_record_key,
                    transaction_id,
                    command,
                    None,
                    opt_subkey,
                    opt_value,
                )
                .await
                .expect("transact command should not error");
            let success = matches!(
                result,
                NetworkResult::Value(InboundTransactCommandResult::Success(_))
            );
            results.push(result);
            if !success {
                break;
            }
        }
        results
    }

    /// What the node would answer a get for subkey 0 with
    async fn stored_value(&self) -> Option<Arc<SignedValueData>> {
        match self
            .storage_manager()
            .inbound_get_value(&self.opaque_record_key, 0, true)
            .await
            .expect("inbound_get_value should not error")
        {
            NetworkResult::Value(InboundGetValueResult::Success(get_result)) => {
                get_result.opt_value
            }
            _ => None,
        }
    }

    async fn shutdown(self) {
        self.api.shutdown().await;
    }
}

fn assert_committed(results: &[NetworkResult<InboundTransactCommandResult>]) {
    assert_eq!(results.len(), 3, "transaction stopped early: {:?}", results);
    for result in results {
        assert!(
            matches!(
                result,
                NetworkResult::Value(InboundTransactCommandResult::Success(_))
            ),
            "transaction command failed: {:?}",
            results
        );
    }
}

// A writer the schema does not admit must not stage a value, however valid its signature
pub async fn test_inbound_transaction_set_rejects_non_member_writer() {
    let f = Fixture::startup().await;

    let outsider_value = f.signed_value(&f.outsider, 5, b"outsider").await;
    let results = f.transact_set(&f.outsider, outsider_value).await;
    assert!(
        !matches!(
            results[0],
            NetworkResult::Value(InboundTransactCommandResult::Success(_))
        ),
        "non-member transactional set was accepted: {:?}",
        results
    );
    assert!(
        f.stored_value().await.is_none(),
        "non-member value was stored"
    );

    // The owner's lower sequence number lands, which the outsider's would have blocked
    let owner_value = f.signed_value(&f.owner, 1, b"owner").await;
    let results = f.transact_set(&f.owner, owner_value.clone()).await;
    assert_committed(&results);
    let stored = f.stored_value().await.expect("owner value was not stored");
    assert_eq!(stored.value_data(), owner_value.value_data());

    f.shutdown().await;
}

// A value naming an admitted writer must carry that writer's signature
pub async fn test_inbound_transaction_set_rejects_forged_signature() {
    let f = Fixture::startup().await;

    let forged_value = f.forged_value(&f.owner, 5, b"forged");
    let results = f.transact_set(&f.outsider, forged_value).await;
    assert!(
        !matches!(
            results[0],
            NetworkResult::Value(InboundTransactCommandResult::Success(_))
        ),
        "forged transactional set was accepted: {:?}",
        results
    );
    assert!(f.stored_value().await.is_none(), "forged value was stored");

    f.shutdown().await;
}

pub async fn test_all() {
    test_inbound_transaction_set_rejects_non_member_writer().await;
    test_inbound_transaction_set_rejects_forged_signature().await;
}

use super::*;

/// A record as the simulated network holds it.
pub(crate) struct MockRecord {
    /// Owner public key
    pub owner: PublicKey,
    /// Schema the record was created with
    pub schema: DHTSchema,
    /// Stored values by subkey
    pub subkeys: HashMap<ValueSubkey, ValueData>,
    /// Watches held on this record
    pub watches: Vec<MockWatch>,
    /// Signing key currently holding a transaction on this record, mapped to its id
    pub tx_holders: HashMap<Option<PublicKey>, u64>,
    /// Bumped on every committed change, so a transaction can tell its snapshot went stale
    pub version: u64,
}

/// The shared DHT, holding what every node in the process can see.
///
/// Kept behind its own lock rather than inside [MockNetwork] so that no operation ever needs the
/// network lock and a node lock at once. Nothing in this module holds both.
static MOCK_RECORDS: LazyLock<Mutex<HashMap<OpaqueRecordKey, MockRecord>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// How often a waiting flush rechecks, since nothing wakes it: there is no tick task under mock.
const FLUSH_POLL_INTERVAL_MS: u32 = 100;

/// A record this node has opened. The node's copy of the values lives in
/// [MockNode::local_records], which outlives the open.
pub(crate) struct MockOpenedRecord {
    pub writer: Option<KeyPair>,
}

/// Read from the shared DHT.
pub(crate) fn with_records<R>(f: impl FnOnce(&mut HashMap<OpaqueRecordKey, MockRecord>) -> R) -> R {
    let mut records = MOCK_RECORDS.lock();
    f(&mut records)
}

/// Whether an incoming value supersedes what is stored.
///
/// Mirrors the storage node's rule: a greater sequence number wins, and an equal sequence number is
/// accepted only when it carries byte-identical data from the same writer, which makes a retry of
/// the same write idempotent.
fn accepts(existing: Option<&ValueData>, incoming: &ValueData) -> bool {
    let Some(existing) = existing else {
        return true;
    };
    match (existing.seq().to_option(), incoming.seq().to_option()) {
        (Some(e), Some(i)) if i > e => true,
        (Some(e), Some(i)) if i == e => {
            existing.data() == incoming.data() && existing.ref_writer() == incoming.ref_writer()
        }
        _ => false,
    }
}

impl VeilidAPI {
    /// Transform value data between plaintext and what a storage node holds.
    ///
    /// The real record layer stream-ciphers subkey values under the encryption key half of the
    /// record key (`storage_manager/record_encryption.rs`). That is what makes a record key the
    /// read capability for its record: handing one out hands out the ability to read it, and
    /// reopening the same opaque storage under a different encryption key takes that ability away.
    /// Modelled here so a holder of the wrong key gets the same undecipherable bytes the real
    /// network gives it instead of reading straight through.
    ///
    /// Symmetric, because the cipher is: encrypt and decrypt are the same call.
    ///
    /// Real core stores a random nonce beside each value; the mock keeps plain `ValueData` with
    /// nowhere to put one, so the nonce is derived from the slot instead. That keeps ciphertext the
    /// same length as plaintext, and keeps a rewrite of identical bytes identical - which the
    /// equal-sequence idempotency arm in [accepts] compares. It does mean one nonce per slot rather
    /// than per write, which no real deployment should imitate; there is no adversary inside a
    /// single mock process.
    pub(crate) fn mock_crypt_value(
        &self,
        record_key: &RecordKey,
        subkey: ValueSubkey,
        data: &[u8],
    ) -> VeilidAPIResult<Vec<u8>> {
        let Some(encryption_key) = record_key.ref_value().ref_encryption_key() else {
            return Ok(data.to_vec());
        };
        let crypto = self.crypto()?;
        let Some(vcrypto) = crypto.get(record_key.kind()) else {
            apibail_generic!("unsupported cryptosystem for record encryption key");
        };

        let mut nonce_material = record_key.ref_value().ref_key().bytes().to_vec();
        nonce_material.extend_from_slice(&subkey.to_le_bytes());
        let digest = vcrypto.generate_hash(&nonce_material);
        let nonce = Nonce::new(&digest.ref_value().bytes()[..vcrypto.nonce_length()]);

        let encryption_key = SharedSecret::new(record_key.kind(), encryption_key.clone());
        let mut out = vec![0u8; data.len()];
        vcrypto.crypt_b2b_no_auth(data, &mut out, &nonce, &encryption_key)?;
        Ok(out)
    }

    /// [Self::mock_crypt_value] over a whole stored value, keeping seq and writer
    pub(crate) fn mock_crypt_value_data(
        &self,
        record_key: &RecordKey,
        subkey: ValueSubkey,
        value: &ValueData,
    ) -> VeilidAPIResult<ValueData> {
        let data = self.mock_crypt_value(record_key, subkey, value.data())?;
        ValueData::try_new_with_seq(value.seq(), data, value.ref_writer().clone())
    }
}

impl MockNode {
    fn opened(&self, record_key: &RecordKey) -> VeilidAPIResult<&MockOpenedRecord> {
        self.opened_records
            .get(&record_key.opaque())
            .ok_or_else(|| {
                VeilidAPIError::invalid_argument(
                    "record not open",
                    "record_key",
                    record_key.opaque(),
                )
            })
    }

    pub(crate) fn local_value(
        &self,
        record_key: &RecordKey,
        subkey: ValueSubkey,
    ) -> Option<ValueData> {
        self.local_records
            .get(&record_key.opaque())
            .and_then(|m| m.get(&subkey))
            .cloned()
    }

    pub(crate) fn set_local_value(
        &mut self,
        record_key: &RecordKey,
        subkey: ValueSubkey,
        value: ValueData,
    ) {
        self.local_records
            .entry(record_key.opaque())
            .or_default()
            .insert(subkey, value);
    }
}

impl VeilidAPI {
    /// Create a record in the shared DHT and open it for writing.
    pub(crate) async fn mock_create_dht_record(
        &self,
        kind: CryptoKind,
        schema: DHTSchema,
        owner: Option<KeyPair>,
    ) -> VeilidAPIResult<DHTRecordDescriptor> {
        check_fault(MockFaultOp::Create, None, None)?;
        let crypto = self.crypto()?;
        let Some(vcrypto) = crypto.get_async(kind) else {
            apibail_generic!("unsupported cryptosystem");
        };

        let owner = match owner {
            Some(owner) => owner,
            None => vcrypto.generate_keypair().await,
        };
        // The real create always mints an encryption key, and it is part of the derived record key
        let encryption_key = vcrypto.random_shared_secret().await;
        let record_key =
            dht_record_key(&crypto, schema.clone(), &owner.key(), Some(encryption_key)).await?;

        with_records(|records| {
            records.insert(
                record_key.opaque(),
                MockRecord {
                    owner: owner.key(),
                    schema: schema.clone(),
                    subkeys: HashMap::new(),
                    watches: Vec::new(),
                    tx_holders: HashMap::new(),
                    version: 0,
                },
            )
        });

        MockNetwork::with_node(&self.mock_key()?, |node| {
            node.opened_records.insert(
                record_key.opaque(),
                MockOpenedRecord {
                    writer: Some(owner.clone()),
                },
            );
            // The local store holds a record from the moment it is created, values or not
            node.local_records.entry(record_key.opaque()).or_default();
            node.local_descriptors
                .insert(record_key.opaque(), (owner.key(), schema.clone()));
        })?;

        Ok(DHTRecordDescriptor::new(
            record_key,
            owner.key(),
            Some(owner.secret()),
            schema,
        ))
    }

    /// Open a record: from this node's local store when it has one, else from the shared DHT.
    ///
    /// A record already in the local store opens with no network at all, which is what
    /// `open_record` does - it returns the local descriptor and only queues a rehydration.
    /// Only a record this node has never held has to be fetched, and that is the one case
    /// that needs to be attached. The shared DHT never loses a value, so there is nothing to
    /// rehydrate and [OpenDHTRecordOptions::rehydrate] changes nothing here.
    pub(crate) fn mock_open_dht_record(
        &self,
        record_key: RecordKey,
        default_writer: Option<KeyPair>,
        _options: Option<OpenDHTRecordOptions>,
    ) -> VeilidAPIResult<DHTRecordDescriptor> {
        check_fault(MockFaultOp::Open, Some(&record_key), None)?;
        let local = MockNetwork::with_node(&self.mock_key()?, |node| {
            node.local_descriptors.get(&record_key.opaque()).cloned()
        })?;

        let (owner, schema) = match local {
            Some(descriptor) => descriptor,
            None => {
                // Nothing held locally, so this open has to reach the network
                if !self.mock_is_attached()? {
                    apibail_try_again!("offline, try again later");
                }
                with_records(|records| {
                    records
                        .get(&record_key.opaque())
                        .map(|r| (r.owner.clone(), r.schema.clone()))
                        .ok_or_else(|| VeilidAPIError::key_not_found(record_key.opaque()))
                })?
            }
        };

        // Only an owner-keyed writer conveys the owner secret back, matching the real descriptor
        let owner_secret = default_writer
            .as_ref()
            .filter(|w| w.key() == owner)
            .map(|w| w.secret());

        MockNetwork::with_node(&self.mock_key()?, |node| {
            node.opened_records
                .entry(record_key.opaque())
                .or_insert_with(|| MockOpenedRecord { writer: None })
                .writer = default_writer;
            // The local store holds a record from the moment it is opened, values or not
            node.local_records.entry(record_key.opaque()).or_default();
            node.local_descriptors
                .entry(record_key.opaque())
                .or_insert_with(|| (owner.clone(), schema.clone()));
        })?;

        Ok(DHTRecordDescriptor::new(
            record_key,
            owner,
            owner_secret,
            schema,
        ))
    }

    /// Stop tracking a record on this node.
    pub(crate) fn mock_close_dht_record(&self, record_key: RecordKey) -> VeilidAPIResult<()> {
        MockNetwork::with_node(&self.mock_key()?, |node| {
            node.opened_records
                .remove(&record_key.opaque())
                .map(|_| ())
                .ok_or_else(|| {
                    VeilidAPIError::invalid_argument(
                        "record not open",
                        "record_key",
                        record_key.opaque(),
                    )
                })
        })?
    }

    /// Drop this node's copy of a record. The shared DHT keeps its own, as the network would.
    ///
    /// Scoped to the local store like the real delete: a record this node no longer holds is
    /// `KeyNotFound` even though the network still has it.
    pub(crate) fn mock_delete_dht_record(&self, record_key: RecordKey) -> VeilidAPIResult<()> {
        // Deleting an open record closes it first rather than refusing
        MockNetwork::with_node(&self.mock_key()?, |node| {
            node.opened_records.remove(&record_key.opaque());
            node.offline_writes.remove(&record_key.opaque());
            node.local_descriptors.remove(&record_key.opaque());
            node.local_records
                .remove(&record_key.opaque())
                .map(|_| ())
                .ok_or_else(|| VeilidAPIError::key_not_found(record_key.opaque()))
        })?
    }

    /// The subkeys of a record this node still owes the network.
    pub(crate) fn mock_offline_subkeys(
        &self,
        record_key: &RecordKey,
    ) -> VeilidAPIResult<ValueSubkeyRangeSet> {
        MockNetwork::with_node(&self.mock_key()?, |node| {
            node.offline_writes
                .get(&record_key.opaque())
                .map(|pending| {
                    pending.keys().fold(ValueSubkeyRangeSet::new(), |acc, sk| {
                        acc.union(&ValueSubkeyRangeSet::single(*sk))
                    })
                })
                .unwrap_or_default()
        })
    }

    /// Push this node's queued writes for a record to the shared DHT.
    ///
    /// Returns false while anything is still owed, which is what a detached node always reports.
    pub(crate) fn mock_drain_offline_writes(
        &self,
        record_key: &RecordKey,
    ) -> VeilidAPIResult<bool> {
        let key = self.mock_key()?;
        let pending = MockNetwork::with_node(&key, |node| {
            node.offline_writes
                .get(&record_key.opaque())
                .cloned()
                .unwrap_or_default()
        })?;
        if pending.is_empty() {
            return Ok(true);
        }
        if !self.mock_is_attached()? {
            return Ok(false);
        }

        let mut changed = ValueSubkeyRangeSet::new();
        with_records(|records| {
            let Some(record) = records.get_mut(&record_key.opaque()) else {
                return;
            };
            for (subkey, value) in &pending {
                if accepts(record.subkeys.get(subkey), value) {
                    record.version += 1;
                    record.subkeys.insert(*subkey, value.clone());
                    changed = changed.union(&ValueSubkeyRangeSet::single(*subkey));
                }
            }
        });

        MockNetwork::with_node(&key, |node| {
            node.offline_writes.remove(&record_key.opaque());
        })?;

        if !changed.is_empty() {
            self.report_value_change(record_key, &changed);
        }
        Ok(true)
    }

    /// Push every queued write this node holds, for when it comes back online.
    pub(crate) fn mock_drain_all_offline_writes(&self) -> VeilidAPIResult<()> {
        let keys = MockNetwork::with_node(&self.mock_key()?, |node| {
            node.offline_writes.keys().cloned().collect::<Vec<_>>()
        })?;
        for opaque in keys {
            // The encryption key is irrelevant here: everything downstream keys off the opaque form
            let record_key = RecordKey::from_opaque(opaque, None);
            self.mock_drain_offline_writes(&record_key)?;
        }
        Ok(())
    }

    /// Wait for this node's queued writes for a record to reach the shared DHT.
    ///
    /// Detached there is nothing to wait for, so a bounded flush reports the timeout and an
    /// unbounded one keeps waiting, which is the real contract.
    pub(crate) async fn mock_flush_dht_record(
        &self,
        record_key: RecordKey,
        timeout: Option<Duration>,
    ) -> VeilidAPIResult<bool> {
        let deadline = timeout
            .map(|t| Timestamp::now().later(TimestampDuration::new_ms(t.as_millis() as u64)));
        loop {
            if self.mock_drain_offline_writes(&record_key)? {
                return Ok(true);
            }
            if deadline.is_some_and(|d| Timestamp::now() >= d) {
                return Ok(false);
            }
            sleep(FLUSH_POLL_INTERVAL_MS).await;
        }
    }

    /// Read a subkey, refreshing this node's cached copy from the shared DHT.
    ///
    /// A cached value satisfies the read unless a refresh was demanded. Only a read that has to
    /// reach the network cares whether this node is attached. What the network returns is stored by
    /// the real rule (`StorageManager::get_result_replaces_local`): a newer value replaces the
    /// cached one, a different value at the same sequence number only on a forced refresh with no
    /// write of this node's still queued, and an older one never.
    pub(crate) fn mock_get_dht_value(
        &self,
        record_key: RecordKey,
        subkey: ValueSubkey,
        force_refresh: bool,
    ) -> VeilidAPIResult<Option<ValueData>> {
        let action = check_fault(MockFaultOp::Get, Some(&record_key), Some(subkey))?;
        if matches!(action, MockFaultAction::Silent) {
            return Ok(None);
        }
        let key = self.mock_key()?;
        let (cached, pending) = MockNetwork::with_node(&key, |node| {
            node.opened(&record_key)?;
            let pending = node
                .offline_writes
                .get(&record_key.opaque())
                .is_some_and(|p| p.contains_key(&subkey));
            VeilidAPIResult::Ok((node.local_value(&record_key, subkey), pending))
        })??;
        if let MockFaultAction::Rival(data) = action {
            self.mock_plant_rival(&record_key, subkey, data)?;
        }

        if !force_refresh {
            if let Some(cached) = cached {
                return Ok(Some(self.mock_crypt_value_data(
                    &record_key,
                    subkey,
                    &cached,
                )?));
            }
        }
        if !self.mock_is_attached()? {
            apibail_try_again!("offline, try again later");
        }

        let value = with_records(|records| {
            let record = records
                .get(&record_key.opaque())
                .ok_or_else(|| VeilidAPIError::key_not_found(record_key.opaque()))?;
            // A subkey past the schema is an error, not an absent value
            if subkey > record.schema.max_subkey() {
                apibail_invalid_argument!("subkey out of range", "subkey", subkey);
            }
            VeilidAPIResult::Ok(record.subkeys.get(&subkey).cloned())
        })?;

        let value = match (value, cached) {
            (None, cached) => cached,
            (Some(value), Some(local))
                if value.seq() < local.seq()
                    || (value.seq() == local.seq()
                        && (!force_refresh || value == local || pending)) =>
            {
                Some(local)
            }
            (Some(value), _) => {
                MockNetwork::with_node(&key, |node| {
                    node.set_local_value(&record_key, subkey, value.clone());
                })?;
                Some(value)
            }
        };
        value
            .map(|v| self.mock_crypt_value_data(&record_key, subkey, &v))
            .transpose()
    }

    /// Replace the shared DHT's value for a subkey with a different one at the same sequence
    /// number and from the same writer, the outcome of a split write settling the other way.
    ///
    /// No watcher hears it, so only a read that reaches the network sees it.
    fn mock_plant_rival(
        &self,
        record_key: &RecordKey,
        subkey: ValueSubkey,
        data: Option<Vec<u8>>,
    ) -> VeilidAPIResult<()> {
        let crypto = self.crypto()?;
        with_records(|records| {
            let record = records
                .get_mut(&record_key.opaque())
                .ok_or_else(|| VeilidAPIError::key_not_found(record_key.opaque()))?;
            let Some(stored) = record.subkeys.get(&subkey) else {
                apibail_generic!(
                    "injected fault: no value to rival at {}:{}",
                    record_key,
                    subkey
                );
            };
            let plaintext = self.mock_crypt_value(record_key, subkey, stored.data())?;
            let rival = match data {
                Some(data) => data,
                None if plaintext.is_empty() => vec![0],
                None => plaintext.iter().map(|b| !b).collect(),
            };
            if rival == plaintext {
                apibail_invalid_argument!(
                    "rival must differ from the stored value",
                    "data",
                    hex::encode(&rival)
                );
            }
            if check_subkey_value_data(
                &crypto,
                &record.schema,
                &record.owner,
                subkey,
                stored.ref_writer(),
                rival.len(),
            )
            .is_err()
            {
                apibail_generic!("failed schema validation: {}:{}", record_key, subkey);
            }
            let rival = ValueData::try_new_with_seq(
                stored.seq(),
                self.mock_crypt_value(record_key, subkey, &rival)?,
                stored.ref_writer().clone(),
            )?;
            record.subkeys.insert(subkey, rival);
            record.version += 1;
            Ok(())
        })
    }

    /// Write a subkey through to the shared DHT.
    ///
    /// Returns `Ok(None)` when the write landed, or `Ok(Some(v))` with the value the network kept
    /// when it did not, matching the real return contract.
    ///
    /// Sequence numbers come from this node's copy, not the network's, so a writer whose cache was
    /// dropped signs from scratch and loses to the stored value.
    pub(crate) fn mock_set_dht_value(
        &self,
        record_key: RecordKey,
        subkey: ValueSubkey,
        data: Vec<u8>,
        options: Option<SetDHTValueOptions>,
    ) -> VeilidAPIResult<Option<ValueData>> {
        // A silent write reports success and drops the value, the failure nothing retries
        if matches!(
            check_fault(MockFaultOp::Set, Some(&record_key), Some(subkey))?,
            MockFaultAction::Silent
        ) {
            return Ok(None);
        }
        let key = self.mock_key()?;
        let options = options.unwrap_or_default();

        let (default_writer, cached) = MockNetwork::with_node(&key, |node| {
            let writer = node.opened(&record_key)?.writer.clone();
            VeilidAPIResult::Ok((writer, node.local_value(&record_key, subkey)))
        })??;
        let Some(writer) = options.writer.clone().or(default_writer) else {
            apibail_generic!("value is not writable");
        };

        let (owner, schema) = with_records(|records| {
            let record = records
                .get(&record_key.opaque())
                .ok_or_else(|| VeilidAPIError::key_not_found(record_key.opaque()))?;
            // A record inside a transaction takes its writes through that transaction
            if !record.tx_holders.is_empty() {
                apibail_try_again!("record is in a transaction");
            }
            VeilidAPIResult::Ok((record.owner.clone(), record.schema.clone()))
        })?;

        // Refused with the same wording as the real set path so callers can classify the failure
        let crypto = self.crypto()?;
        if check_subkey_value_data(&crypto, &schema, &owner, subkey, &writer.key(), data.len())
            .is_err()
        {
            apibail_generic!("failed schema validation: {}:{}", record_key, subkey);
        }

        let min_seqnum = options.min_seqnum.unwrap_or(ValueSeqNum::NONE);

        // From here on the data is ciphertext, which is what a storage node applies its rules to
        let data = self.mock_crypt_value(&record_key, subkey, &data)?;

        // Rewriting identical bytes as the same writer keeps the number rather than burning one
        let identical = cached
            .as_ref()
            .is_some_and(|c| c.data() == data.as_slice() && c.ref_writer() == &writer.key());

        let mut seq = match cached.as_ref().map(|c| c.seq()) {
            None => ValueSeqNum::ZERO,
            Some(last) if identical => last,
            Some(last) => match last.next() {
                Ok(next) => next,
                // A write already meeting the floor reuses the number, so a closing write can retry
                Err(_) if min_seqnum.is_some() && last >= min_seqnum => last,
                Err(_) => {
                    apibail_generic!("sequence number at max: {}:{}", record_key, subkey);
                }
            },
        };
        if min_seqnum.is_some() && seq < min_seqnum {
            seq = min_seqnum;
        }

        let incoming = ValueData::try_new_with_seq(seq, data, writer.key())?;

        // Detached, the write goes no further than the local store and the offline queue
        if !self.mock_is_attached()? {
            if options.allow_offline == Some(AllowOffline(false)) {
                apibail_try_again!("offline, try again later");
            }
            MockNetwork::with_node(&key, |node| {
                node.set_local_value(&record_key, subkey, incoming.clone());
                node.offline_writes
                    .entry(record_key.opaque())
                    .or_default()
                    .insert(subkey, incoming);
            })?;
            return Ok(None);
        }

        let result = with_records(|records| {
            let record = records
                .get_mut(&record_key.opaque())
                .ok_or_else(|| VeilidAPIError::key_not_found(record_key.opaque()))?;
            let existing = record.subkeys.get(&subkey).cloned();

            if accepts(existing.as_ref(), &incoming) {
                // An idempotent rewrite is accepted but changes nothing, so no watcher hears it
                let changed = existing
                    .as_ref()
                    .is_none_or(|e| e.seq() != incoming.seq() || e.data() != incoming.data());
                if changed {
                    record.version += 1;
                }
                record.subkeys.insert(subkey, incoming.clone());
                VeilidAPIResult::Ok(SetOutcome::Accepted {
                    value: incoming,
                    changed,
                })
            } else {
                // No consensus to miss here, so a rejected write never defers with TryAgain
                VeilidAPIResult::Ok(SetOutcome::Superseded(existing.unwrap_or_log()))
            }
        })?;

        match result {
            SetOutcome::Accepted { value, changed } => {
                MockNetwork::with_node(&key, |node| {
                    node.set_local_value(&record_key, subkey, value);
                    // This write superseded whatever was queued for the subkey
                    if let Some(pending) = node.offline_writes.get_mut(&record_key.opaque()) {
                        pending.remove(&subkey);
                    }
                })?;
                if changed {
                    self.report_value_change(&record_key, &ValueSubkeyRangeSet::single(subkey));
                }
                Ok(None)
            }
            SetOutcome::Superseded(existing) => {
                MockNetwork::with_node(&key, |node| {
                    node.set_local_value(&record_key, subkey, existing.clone());
                })?;
                Ok(Some(self.mock_crypt_value_data(
                    &record_key,
                    subkey,
                    &existing,
                )?))
            }
        }
    }

    /// Report this node's cached sequence numbers against the shared DHT's.
    pub(crate) fn mock_inspect_dht_record(
        &self,
        record_key: RecordKey,
        subkeys: Option<ValueSubkeyRangeSet>,
        scope: DHTReportScope,
    ) -> VeilidAPIResult<DHTRecordReport> {
        check_fault(MockFaultOp::Inspect, Some(&record_key), None)?;
        let key = self.mock_key()?;
        let schema = with_records(|records| {
            records
                .get(&record_key.opaque())
                .map(|r| r.schema.clone())
                .ok_or_else(|| VeilidAPIError::key_not_found(record_key.opaque()))
        })?;

        // An empty request means the whole record, as it does on the real path
        let subkeys = subkeys.unwrap_or_default();
        let subkeys = if subkeys.is_empty() {
            ValueSubkeyRangeSet::full()
        } else {
            subkeys
        };
        let subkeys = schema.truncate_subkeys(&subkeys, Some(DHTSchema::MAX_SUBKEY_COUNT));

        // Whatever this node still owes the network, within the range asked about
        let offline_subkeys = self.mock_offline_subkeys(&record_key)?.intersect(&subkeys);

        let local_seqs = MockNetwork::with_node(&key, |node| {
            node.opened(&record_key)?;
            let local = node.local_records.get(&record_key.opaque());
            VeilidAPIResult::Ok(
                subkeys
                    .iter()
                    .map(|sk| {
                        local
                            .and_then(|m| m.get(&sk))
                            .map(|v| v.seq())
                            .unwrap_or(ValueSeqNum::NONE)
                    })
                    .collect::<Vec<_>>(),
            )
        })??;

        // A local-scope report never consults the network, but still reports a slot per subkey
        if matches!(scope, DHTReportScope::Local) {
            return DHTRecordReport::try_new(
                subkeys,
                offline_subkeys,
                local_seqs.clone(),
                vec![ValueSeqNum::NONE; local_seqs.len()],
            );
        }

        // Every other scope inspects the network
        if !self.mock_is_attached()? {
            apibail_try_again!("offline, try again later");
        }

        // Simulating a set reports the numbers a write would produce, not the ones stored
        let local_seqs: Vec<ValueSeqNum> = if matches!(scope, DHTReportScope::UpdateSet) {
            local_seqs
                .into_iter()
                .map(|s| s.saturating_next())
                .collect()
        } else {
            local_seqs
        };

        let network_seqs = with_records(|records| {
            let record = records.get(&record_key.opaque());
            subkeys
                .iter()
                .map(|sk| {
                    record
                        .and_then(|r| r.subkeys.get(&sk))
                        .map(|v| v.seq())
                        .unwrap_or(ValueSeqNum::NONE)
                })
                .collect::<Vec<_>>()
        });

        // An update scope takes the network's answer only when it is strictly newer than ours
        let network_seqs = match scope {
            DHTReportScope::SyncGet | DHTReportScope::SyncSet => network_seqs,
            _ => local_seqs
                .iter()
                .zip(network_seqs)
                .map(|(l, n)| if n > *l { n } else { *l })
                .collect(),
        };

        DHTRecordReport::try_new(subkeys, offline_subkeys, local_seqs, network_seqs)
    }
}

enum SetOutcome {
    Accepted { value: ValueData, changed: bool },
    Superseded(ValueData),
}

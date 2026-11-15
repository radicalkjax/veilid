use super::*;

/// Identifies one open transaction on a simulated node.
///
/// Mirrors the real handle so `DHTTransaction` is unchanged between builds.
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) struct OutboundTransactionHandle {
    txid: u64,
}

impl fmt::Display for OutboundTransactionHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "txid-{}", self.txid)
    }
}

/// One record's participation in a transaction.
struct MockTransactionRecord {
    /// Record covered by the transaction
    record_key: RecordKey,
    /// The key that took this record's holder slot, or `None` for an anonymous transaction
    signer: Option<PublicKey>,
}

/// A transaction staging writes until commit.
struct MockTransaction {
    /// Records the transaction covers
    records: Vec<MockTransactionRecord>,
    /// Writes waiting for commit, invisible to every other transaction until then
    staged: HashMap<(OpaqueRecordKey, ValueSubkey), ValueData>,
    /// Values read in this transaction, which reach the node's cache only on commit
    reads: HashMap<(OpaqueRecordKey, ValueSubkey), ValueData>,
    /// Each record's version at begin: a commit lands only if none of them moved
    begin_versions: HashMap<OpaqueRecordKey, u64>,
    /// What each covered subkey held at begin, which is all a transactional get ever sees
    begin_values: HashMap<(OpaqueRecordKey, ValueSubkey), ValueData>,
}

static MOCK_TRANSACTIONS: LazyLock<Mutex<HashMap<u64, MockTransaction>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static NEXT_TXID: AtomicU64 = AtomicU64::new(1);

fn with_transactions<R>(f: impl FnOnce(&mut HashMap<u64, MockTransaction>) -> R) -> R {
    let mut txns = MOCK_TRANSACTIONS.lock();
    f(&mut txns)
}

fn not_found() -> VeilidAPIError {
    VeilidAPIError::transaction_not_found("missing transaction")
}

/// Take each record's holder slot, or report the first one already held.
///
/// A schema member gets one concurrent transaction per signing key, so a second begin under the
/// same key is refused rather than queued.
fn claim_records(txid: u64, records: &[MockTransactionRecord]) -> VeilidAPIResult<()> {
    with_records(|store| {
        // Check every record first so a refusal leaves no partial claim behind
        for r in records {
            let record = store
                .get(&r.record_key.opaque())
                .ok_or_else(|| VeilidAPIError::key_not_found(r.record_key.opaque()))?;
            if record.tx_holders.contains_key(&r.signer) {
                return Err(VeilidAPIError::generic(format!(
                    "Record {} already has a transaction open",
                    r.record_key
                )));
            }
        }
        for r in records {
            if let Some(record) = store.get_mut(&r.record_key.opaque()) {
                record.tx_holders.insert(r.signer.clone(), txid);
            }
        }
        Ok(())
    })
}

/// Freeze each covered record: its version, so commit can tell whether the ground moved, and its
/// subkey values, which are the only ones a transactional get returns.
type RecordSnapshot = (
    HashMap<OpaqueRecordKey, u64>,
    HashMap<(OpaqueRecordKey, ValueSubkey), ValueData>,
);
fn snapshot_records(records: &[MockTransactionRecord]) -> RecordSnapshot {
    with_records(|store| {
        let mut versions = HashMap::new();
        let mut values = HashMap::new();
        for r in records {
            let opaque = r.record_key.opaque();
            let Some(rec) = store.get(&opaque) else {
                continue;
            };
            versions.insert(opaque.clone(), rec.version);
            for (subkey, value) in &rec.subkeys {
                values.insert((opaque.clone(), *subkey), value.clone());
            }
        }
        (versions, values)
    })
}

fn release_records(txid: u64) {
    with_records(|store| {
        for record in store.values_mut() {
            record.tx_holders.retain(|_, v| *v != txid);
        }
    });
}

impl VeilidAPI {
    /// Open a transaction over a set of records this node already has open.
    pub(crate) fn mock_begin_transaction(
        &self,
        record_keys: Vec<RecordKey>,
        options: Option<TransactDHTRecordsOptions>,
    ) -> VeilidAPIResult<OutboundTransactionHandle> {
        self.mock_require_dht_online()?;
        for record_key in &record_keys {
            check_fault(MockFaultOp::Begin, Some(record_key), None)?;
        }
        let key = self.mock_key()?;
        if record_keys.is_empty() {
            apibail_invalid_argument!("no records in transaction", "record_keys", "[]");
        }
        let default_signer = options.and_then(|o| o.default_signing_keypair);

        let records = self.mock_transaction_records(&key, &record_keys, default_signer)?;
        let txid = NEXT_TXID.fetch_add(1, Ordering::Relaxed);
        claim_records(txid, &records)?;
        let (begin_versions, begin_values) = snapshot_records(&records);

        with_transactions(|txns| {
            txns.insert(
                txid,
                MockTransaction {
                    records,
                    staged: HashMap::new(),
                    reads: HashMap::new(),
                    begin_versions,
                    begin_values,
                },
            )
        });
        Ok(OutboundTransactionHandle { txid })
    }

    /// Resolve the records a transaction will cover, and which key claims each one.
    fn mock_transaction_records(
        &self,
        key: &MockNodeKey,
        record_keys: &[RecordKey],
        default_signer: Option<KeyPair>,
    ) -> VeilidAPIResult<Vec<MockTransactionRecord>> {
        let mut seen = HashSet::new();
        for rk in record_keys {
            if !seen.insert(rk.opaque()) {
                apibail_missing_argument!("duplicate record key", "record_keys");
            }
        }

        MockNetwork::with_node(key, |node| {
            record_keys
                .iter()
                .map(|rk| {
                    let opened = node.opened_records.get(&rk.opaque()).ok_or_else(|| {
                        VeilidAPIError::invalid_argument(
                            "record not open",
                            "record_key",
                            rk.to_string(),
                        )
                    })?;
                    let signer = opened
                        .writer
                        .clone()
                        .or_else(|| default_signer.clone())
                        .map(|w| w.key());
                    VeilidAPIResult::Ok(MockTransactionRecord {
                        record_key: rk.clone(),
                        signer,
                    })
                })
                .collect::<VeilidAPIResult<Vec<_>>>()
        })?
    }

    /// Add more records to an open transaction.
    pub(crate) fn mock_extend_transaction(
        &self,
        handle: OutboundTransactionHandle,
        record_keys: Vec<RecordKey>,
        options: Option<TransactDHTRecordsOptions>,
    ) -> VeilidAPIResult<()> {
        self.mock_require_dht_online()?;
        let key = self.mock_key()?;
        let default_signer = options.and_then(|o| o.default_signing_keypair);

        // Records already in the transaction are a no-op, so an extend can be repeated
        let existing: Vec<OpaqueRecordKey> = with_transactions(|txns| {
            txns.get(&handle.txid)
                .map(|tx| tx.records.iter().map(|r| r.record_key.opaque()).collect())
                .ok_or_else(not_found)
        })?;
        let fresh: Vec<RecordKey> = record_keys
            .into_iter()
            .filter(|rk| !existing.contains(&rk.opaque()))
            .collect();
        if fresh.is_empty() {
            return Ok(());
        }

        let records = self.mock_transaction_records(&key, &fresh, default_signer)?;
        claim_records(handle.txid, &records)?;
        let (added_versions, added_values) = snapshot_records(&records);
        with_transactions(|txns| {
            let tx = txns.get_mut(&handle.txid).ok_or_else(not_found)?;
            tx.records.extend(records);
            tx.begin_versions.extend(added_versions);
            tx.begin_values.extend(added_values);
            VeilidAPIResult::Ok(())
        })
    }

    /// Stage a write inside a transaction.
    ///
    /// Every set of a subkey counts from the newer of this node's copy and what the transaction has
    /// read, never from what it staged, and is judged as a storage node judges it: staged only
    /// above the value held at begin, otherwise answered with that value, which un-stages the
    /// subkey and becomes what the next set counts from.
    pub(crate) fn mock_transaction_set(
        &self,
        handle: OutboundTransactionHandle,
        record_key: RecordKey,
        subkey: ValueSubkey,
        data: Vec<u8>,
        options: Option<DHTTransactionSetValueOptions>,
    ) -> VeilidAPIResult<Option<ValueData>> {
        self.mock_require_dht_online()?;
        let key = self.mock_key()?;
        let options = options.unwrap_or_default();

        with_transactions(|txns| {
            let tx = txns.get(&handle.txid).ok_or_else(not_found)?;
            if !tx.records.iter().any(|r| r.record_key == record_key) {
                apibail_invalid_argument!(
                    "record is not in the transaction",
                    "record_key",
                    &record_key
                );
            }
            VeilidAPIResult::Ok(())
        })?;

        let (default_writer, cached) = MockNetwork::with_node(&key, |node| {
            let writer = node
                .opened_records
                .get(&record_key.opaque())
                .map(|o| o.writer.clone())
                .ok_or_else(|| {
                    VeilidAPIError::invalid_argument(
                        "record not open",
                        "record_key",
                        record_key.opaque(),
                    )
                })?;
            VeilidAPIResult::Ok((writer, node.local_value(&record_key, subkey)))
        })??;
        let Some(writer) = options.writer.clone().or(default_writer) else {
            apibail_generic!("value is not writable");
        };

        let (owner, schema) = with_records(|store| {
            store
                .get(&record_key.opaque())
                .map(|r| (r.owner.clone(), r.schema.clone()))
                .ok_or_else(|| VeilidAPIError::key_not_found(record_key.opaque()))
        })?;
        // Refused with the same wording as the real set path so callers can classify the failure
        let crypto = self.crypto()?;
        if check_subkey_value_data(&crypto, &schema, &owner, subkey, &writer.key(), data.len())
            .is_err()
        {
            apibail_generic!("failed schema validation: {}:{}", record_key, subkey);
        }

        // Staged as ciphertext, which is what a storage node applies its rules to
        let data = self.mock_crypt_value(&record_key, subkey, &data)?;

        with_transactions(|txns| {
            let tx = txns.get_mut(&handle.txid).ok_or_else(not_found)?;
            let slot = (record_key.opaque(), subkey);

            // At an equal sequence number the network's copy wins
            let base = match (tx.reads.get(&slot).cloned(), cached) {
                (Some(read), Some(cached)) if cached.seq() > read.seq() => Some(cached),
                (Some(read), _) => Some(read),
                (None, cached) => cached,
            };
            let min_seqnum = options.min_seqnum.unwrap_or(ValueSeqNum::NONE);
            let identical = base
                .as_ref()
                .is_some_and(|b| b.data() == data.as_slice() && b.ref_writer() == &writer.key());

            // No floor reuse: identical retries are handled above; new bytes under a reused number is refused
            let mut seq = match base.as_ref().map(|b| b.seq()) {
                None => ValueSeqNum::ZERO,
                Some(last) if identical => last,
                Some(last) => match last.next() {
                    Ok(next) => next,
                    Err(_) => {
                        apibail_generic!("sequence number at max: {}:{}", record_key, subkey);
                    }
                },
            };
            if min_seqnum.is_some() && seq < min_seqnum {
                seq = min_seqnum;
            }

            let value = ValueData::try_new_with_seq(seq, data, writer.key())?;
            match tx.begin_values.get(&slot).cloned() {
                Some(held) if held.seq() >= value.seq() => {
                    tx.staged.remove(&slot);
                    tx.reads.insert(slot, held.clone());
                    VeilidAPIResult::Ok(Some(held))
                }
                _ => {
                    tx.staged.insert(slot, value);
                    VeilidAPIResult::Ok(None)
                }
            }
        })?
        .map(|v| self.mock_crypt_value_data(&record_key, subkey, &v))
        .transpose()
    }

    /// Read inside a transaction, which sees the record as it stood at begin.
    ///
    /// A storage node answers a transactional get from the begin-time snapshot, so a transaction
    /// reads back neither its own staged writes nor anything committed since it started.
    pub(crate) fn mock_transaction_get(
        &self,
        handle: OutboundTransactionHandle,
        record_key: RecordKey,
        subkey: ValueSubkey,
    ) -> VeilidAPIResult<Option<ValueData>> {
        self.mock_require_dht_online()?;
        with_records(|store| {
            let record = store
                .get(&record_key.opaque())
                .ok_or_else(|| VeilidAPIError::key_not_found(record_key.opaque()))?;
            if subkey > record.schema.max_subkey() {
                apibail_invalid_argument!("subkey out of range", "subkey", subkey);
            }
            VeilidAPIResult::Ok(())
        })?;

        with_transactions(|txns| {
            let tx = txns.get_mut(&handle.txid).ok_or_else(not_found)?;
            if !tx.records.iter().any(|r| r.record_key == record_key) {
                apibail_invalid_argument!(
                    "record is not in the transaction",
                    "record_key",
                    &record_key
                );
            }
            let slot = (record_key.opaque(), subkey);
            let value = tx.begin_values.get(&slot).cloned();
            // Staged, not written through, so a rollback leaves the node's cache as it was
            if let Some(value) = value.clone() {
                tx.reads.insert(slot, value);
            }
            VeilidAPIResult::Ok(value)
        })?
        .map(|v| self.mock_crypt_value_data(&record_key, subkey, &v))
        .transpose()
    }

    /// Report a transaction's view of a record without touching the network.
    pub(crate) fn mock_transaction_inspect(
        &self,
        handle: OutboundTransactionHandle,
        record_key: RecordKey,
        subkeys: Option<ValueSubkeyRangeSet>,
        scope: DHTReportScope,
    ) -> VeilidAPIResult<DHTRecordReport> {
        with_transactions(|txns| {
            let tx = txns.get(&handle.txid).ok_or_else(not_found)?;
            if !tx.records.iter().any(|r| r.record_key == record_key) {
                apibail_invalid_argument!(
                    "record is not in the transaction",
                    "record_key",
                    &record_key
                );
            }
            VeilidAPIResult::Ok(())
        })?;

        let schema = with_records(|store| {
            store
                .get(&record_key.opaque())
                .map(|r| r.schema.clone())
                .ok_or_else(|| VeilidAPIError::key_not_found(record_key.opaque()))
        })?;

        let subkeys = subkeys.unwrap_or_default();
        let subkeys = if subkeys.is_empty() {
            ValueSubkeyRangeSet::full()
        } else {
            subkeys
        };
        let subkeys = schema.truncate_subkeys(&subkeys, Some(DHTSchema::MAX_SUBKEY_COUNT));

        let network_seqs: Vec<ValueSeqNum> = with_records(|store| {
            let record = store.get(&record_key.opaque());
            subkeys
                .iter()
                .map(|sk| {
                    record
                        .and_then(|r| r.subkeys.get(&sk))
                        .map(|v| v.seq())
                        .unwrap_or(ValueSeqNum::NONE)
                })
                .collect()
        });

        // Local must stay distinct from network, or a watcher could never see it fall behind
        let key = self.mock_key()?;
        let local_seqs: Vec<ValueSeqNum> = MockNetwork::with_node(&key, |node| {
            let local = node.local_records.get(&record_key.opaque());
            subkeys
                .iter()
                .map(|sk| {
                    local
                        .and_then(|m| m.get(&sk))
                        .map(|v| v.seq())
                        .unwrap_or(ValueSeqNum::NONE)
                })
                .collect()
        })?;

        // Simulating a set reports the numbers a write would produce
        let local_seqs: Vec<ValueSeqNum> = if matches!(scope, DHTReportScope::UpdateSet) {
            local_seqs
                .into_iter()
                .map(|s| s.saturating_next())
                .collect()
        } else {
            local_seqs
        };

        DHTRecordReport::try_new(
            subkeys,
            ValueSubkeyRangeSet::new(),
            local_seqs,
            network_seqs,
        )
    }

    /// Apply every staged write at once, then report the changes to watchers.
    ///
    /// Refused outright if another writer landed on a covered record since begin. A commit that
    /// fails still ends the transaction: its handle is spent, and a real node rolls it back.
    pub(crate) fn mock_commit_transaction(
        &self,
        handle: OutboundTransactionHandle,
    ) -> VeilidAPIResult<()> {
        let key = self.mock_key()?;
        let tx = with_transactions(|txns| txns.remove(&handle.txid).ok_or_else(not_found))?;
        let changed = self.mock_apply_transaction(&tx);
        release_records(handle.txid);
        let changed = changed?;

        for r in &tx.records {
            if let Some(subkeys) = changed.get(&r.record_key.opaque()) {
                self.report_value_change(&r.record_key, subkeys);
            }
        }

        // The committing node's own cache follows its writes
        MockNetwork::with_node(&key, |node| {
            for ((opaque, subkey), value) in tx.reads.iter().chain(tx.staged.iter()) {
                node.local_records
                    .entry(opaque.clone())
                    .or_default()
                    .insert(*subkey, value.clone());
            }
        })?;
        Ok(())
    }

    /// Write a transaction's staged values to the shared DHT, returning the subkeys that changed.
    fn mock_apply_transaction(
        &self,
        tx: &MockTransaction,
    ) -> VeilidAPIResult<HashMap<OpaqueRecordKey, ValueSubkeyRangeSet>> {
        self.mock_require_dht_online()?;
        check_fault(MockFaultOp::Commit, None, None)?;

        // One lock, so a reader never sees half a transaction
        with_records(|store| {
            for (opaque, begin_version) in &tx.begin_versions {
                let current = store
                    .get(opaque)
                    .map(|r| r.version)
                    .unwrap_or(*begin_version);
                if current != *begin_version {
                    return Err(VeilidAPIError::generic(
                        "transaction snapshot is stale: the record changed since begin",
                    ));
                }
            }
            let mut changed: HashMap<OpaqueRecordKey, ValueSubkeyRangeSet> = HashMap::new();
            for ((opaque, subkey), value) in &tx.staged {
                let Some(record) = store.get_mut(opaque) else {
                    continue;
                };
                let existing = record.subkeys.get(subkey);
                let differs =
                    existing.is_none_or(|e| e.seq() != value.seq() || e.data() != value.data());
                record.subkeys.insert(*subkey, value.clone());
                if differs {
                    record.version += 1;
                    changed.entry(opaque.clone()).or_default().insert(*subkey);
                }
            }
            VeilidAPIResult::Ok(changed)
        })
    }

    /// Discard every staged write and release the records.
    pub(crate) fn mock_rollback_transaction(
        &self,
        handle: OutboundTransactionHandle,
    ) -> VeilidAPIResult<()> {
        with_transactions(|txns| txns.remove(&handle.txid).ok_or_else(not_found))?;
        release_records(handle.txid);
        Ok(())
    }
}

/// Tear down a transaction dropped without commit or rollback.
///
/// Takes no node identity: releasing the records needs only the transaction id, which is what lets
/// `Drop` call it with only a registry in hand.
pub(crate) fn mock_drop_transaction(handle: OutboundTransactionHandle) {
    with_transactions(|txns| txns.remove(&handle.txid));
    release_records(handle.txid);
}

//! Test utils.

use std::{
    collections::HashMap,
    fmt, future,
    sync::{Arc, RwLock},
};

use async_trait::async_trait;
use tokio::sync::watch;
use zksync_object_store::{Bucket, MockObjectStore, ObjectStore, ObjectStoreError, StoredObject};
use zksync_types::{
    api,
    block::L2BlockHeader,
    bytecode::{BytecodeHash, BytecodeMarker},
    snapshots::{
        SnapshotFactoryDependencies, SnapshotFactoryDependency, SnapshotHeader,
        SnapshotRecoveryStatus, SnapshotStorageLog, SnapshotStorageLogsChunk,
        SnapshotStorageLogsChunkMetadata, SnapshotStorageLogsStorageKey, SnapshotVersion,
    },
    tokens::{TokenInfo, TokenMetadata},
    web3::Bytes,
    AccountTreeId, Address, L1BatchNumber, L2BlockNumber, ProtocolVersionId, StorageKey,
    StorageValue, H256,
};
use zksync_web3_decl::error::{EnrichedClientError, EnrichedClientResult};

use crate::SnapshotsApplierMainNodeClient;

pub(super) trait SnapshotLogKey: Clone {
    const VERSION: SnapshotVersion;

    fn random() -> Self;
}

impl SnapshotLogKey for H256 {
    const VERSION: SnapshotVersion = SnapshotVersion::Version1;

    fn random() -> Self {
        Self::random()
    }
}

impl SnapshotLogKey for StorageKey {
    const VERSION: SnapshotVersion = SnapshotVersion::Version0;

    fn random() -> Self {
        Self::new(AccountTreeId::new(Address::random()), H256::random())
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct MockMainNodeClient {
    pub fetch_l1_batch_responses: HashMap<L1BatchNumber, api::L1BatchDetails>,
    pub fetch_l2_block_responses: HashMap<L2BlockNumber, api::BlockDetails>,
    pub fetch_newest_snapshot_response: Option<SnapshotHeader>,
    pub tokens_response: Vec<TokenInfo>,
    pub tokens_response_error: Arc<RwLock<Option<EnrichedClientError>>>,
}

impl MockMainNodeClient {
    /// Sets the error to be returned by the `fetch_tokens` method.
    /// Error will be returned just once. Next time the request will succeed.
    pub(super) fn set_token_response_error(&self, error: EnrichedClientError) {
        *self.tokens_response_error.write().unwrap() = Some(error);
    }

    fn take_token_response_error(&self) -> Option<EnrichedClientError> {
        self.tokens_response_error.write().unwrap().take()
    }
}

#[async_trait]
impl SnapshotsApplierMainNodeClient for MockMainNodeClient {
    async fn fetch_l1_batch_details(
        &self,
        number: L1BatchNumber,
    ) -> EnrichedClientResult<Option<api::L1BatchDetails>> {
        Ok(self.fetch_l1_batch_responses.get(&number).cloned())
    }

    async fn fetch_l2_block_details(
        &self,
        number: L2BlockNumber,
    ) -> EnrichedClientResult<Option<api::BlockDetails>> {
        Ok(self.fetch_l2_block_responses.get(&number).cloned())
    }

    async fn fetch_newest_snapshot_l1_batch_number(
        &self,
    ) -> EnrichedClientResult<Option<L1BatchNumber>> {
        Ok(self
            .fetch_newest_snapshot_response
            .as_ref()
            .map(|response| response.l1_batch_number))
    }

    async fn fetch_snapshot(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> EnrichedClientResult<Option<SnapshotHeader>> {
        Ok(self
            .fetch_newest_snapshot_response
            .clone()
            .filter(|response| response.l1_batch_number == l1_batch_number))
    }

    async fn fetch_tokens(
        &self,
        _at_l2_block: L2BlockNumber,
    ) -> EnrichedClientResult<Vec<TokenInfo>> {
        if let Some(error) = self.take_token_response_error() {
            return Err(error);
        }

        Ok(self.tokens_response.clone())
    }
}

type ValidateFn = dyn Fn(&str) -> Result<(), ObjectStoreError> + Send + Sync;

pub(super) struct ObjectStoreWithErrors {
    inner: Arc<dyn ObjectStore>,
    validate_fn: Box<ValidateFn>,
}

impl fmt::Debug for ObjectStoreWithErrors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.as_ref().fmt(formatter)
    }
}

impl ObjectStoreWithErrors {
    pub fn new(
        inner: Arc<dyn ObjectStore>,
        validate_fn: impl Fn(&str) -> Result<(), ObjectStoreError> + Send + Sync + 'static,
    ) -> Self {
        Self {
            inner,
            validate_fn: Box::new(validate_fn),
        }
    }
}

#[async_trait]
impl ObjectStore for ObjectStoreWithErrors {
    async fn get_raw(&self, bucket: Bucket, key: &str) -> Result<Vec<u8>, ObjectStoreError> {
        (self.validate_fn)(key)?;
        self.inner.get_raw(bucket, key).await
    }

    async fn put_raw(
        &self,
        _bucket: Bucket,
        _key: &str,
        _value: Vec<u8>,
    ) -> Result<(), ObjectStoreError> {
        unreachable!("Should not be used in snapshot applier")
    }

    async fn remove_raw(&self, _bucket: Bucket, _key: &str) -> Result<(), ObjectStoreError> {
        unreachable!("Should not be used in snapshot applier")
    }

    fn storage_prefix_raw(&self, bucket: Bucket) -> String {
        self.inner.storage_prefix_raw(bucket)
    }
}

pub(super) fn mock_l2_block_header(l2_block_number: L2BlockNumber) -> L2BlockHeader {
    L2BlockHeader {
        number: l2_block_number,
        timestamp: 0,
        hash: H256::from_low_u64_be(u64::from(l2_block_number.0)),
        l1_tx_count: 0,
        l2_tx_count: 0,
        fee_account_address: Address::repeat_byte(1),
        base_fee_per_gas: 0,
        gas_per_pubdata_limit: 0,
        batch_fee_input: Default::default(),
        base_system_contracts_hashes: Default::default(),
        protocol_version: Some(Default::default()),
        virtual_blocks: 0,
        gas_limit: 0,
        logs_bloom: Default::default(),
        pubdata_params: Default::default(),
        rolling_txs_hash: Some(H256::zero()),
    }
}

fn block_details_base(hash: H256) -> api::BlockDetailsBase {
    api::BlockDetailsBase {
        timestamp: 0,
        l1_tx_count: 0,
        l2_tx_count: 0,
        root_hash: Some(hash),
        status: api::BlockStatus::Sealed,
        commit_tx_hash: None,
        committed_at: None,
        commit_chain_id: None,
        prove_tx_hash: None,
        proven_at: None,
        prove_chain_id: None,
        execute_tx_hash: None,
        executed_at: None,
        execute_chain_id: None,
        precommit_tx_hash: None,
        precommitted_at: None,
        precommit_chain_id: None,
        l1_gas_price: 0,
        l2_fair_gas_price: 0,
        fair_pubdata_price: None,
        base_system_contracts_hashes: Default::default(),
    }
}

fn l2_block_details(
    number: L2BlockNumber,
    l1_batch_number: L1BatchNumber,
    hash: H256,
) -> api::BlockDetails {
    api::BlockDetails {
        number,
        l1_batch_number,
        base: block_details_base(hash),
        operator_address: Default::default(),
        protocol_version: Some(ProtocolVersionId::latest()),
    }
}

fn l1_batch_details(number: L1BatchNumber, root_hash: H256) -> api::L1BatchDetails {
    api::L1BatchDetails {
        number,
        base: block_details_base(root_hash),
    }
}

pub(super) fn mock_factory_deps(kind: Option<BytecodeMarker>) -> SnapshotFactoryDependencies {
    // This works both as an EraVM and EVM bytecode.
    let factory_dep_bytes: Vec<u8> = (0..32).collect();

    SnapshotFactoryDependencies {
        factory_deps: vec![SnapshotFactoryDependency {
            hash: kind.map(|kind| {
                let hash = match kind {
                    BytecodeMarker::EraVm => BytecodeHash::for_bytecode(&factory_dep_bytes),
                    BytecodeMarker::Evm => BytecodeHash::for_evm_bytecode(23, &factory_dep_bytes),
                };
                hash.value()
            }),
            bytecode: Bytes::from(factory_dep_bytes),
        }],
    }
}

pub(super) fn random_storage_logs<K: SnapshotLogKey>(
    l1_batch_number: L1BatchNumber,
    count: u64,
) -> Vec<SnapshotStorageLog<K>> {
    (0..count)
        .map(|i| SnapshotStorageLog {
            key: K::random(),
            value: StorageValue::random(),
            l1_batch_number_of_initial_write: l1_batch_number,
            enumeration_index: i + 1,
        })
        .collect()
}

pub(super) fn mock_recovery_status() -> SnapshotRecoveryStatus {
    SnapshotRecoveryStatus {
        l1_batch_number: L1BatchNumber(123),
        l1_batch_root_hash: H256::random(),
        l1_batch_timestamp: 0,
        l2_block_number: L2BlockNumber(321),
        l2_block_hash: H256::random(),
        l2_block_timestamp: 0,
        protocol_version: ProtocolVersionId::default(),
        storage_logs_chunks_processed: vec![true, true],
    }
}

pub(super) fn mock_tokens() -> Vec<TokenInfo> {
    vec![
        TokenInfo {
            l1_address: Address::zero(),
            l2_address: Address::zero(),
            metadata: TokenMetadata {
                name: "Ether".to_string(),
                symbol: "ETH".to_string(),
                decimals: 18,
            },
        },
        TokenInfo {
            l1_address: Address::random(),
            l2_address: Address::random(),
            metadata: TokenMetadata {
                name: "Test".to_string(),
                symbol: "TST".to_string(),
                decimals: 10,
            },
        },
    ]
}

pub(super) fn mock_snapshot_header(
    version: u16,
    status: &SnapshotRecoveryStatus,
) -> SnapshotHeader {
    SnapshotHeader {
        version,
        l1_batch_number: status.l1_batch_number,
        l2_block_number: status.l2_block_number,
        storage_logs_chunks: (0..status.storage_logs_chunks_processed.len() as u64)
            .map(|chunk_id| SnapshotStorageLogsChunkMetadata {
                chunk_id,
                filepath: format!("file{chunk_id}"),
            })
            .collect(),
        factory_deps_filepath: "some_filepath".to_string(),
    }
}

pub(super) async fn prepare_clients<K>(
    status: &SnapshotRecoveryStatus,
    factory_deps: &SnapshotFactoryDependencies,
    logs: &[SnapshotStorageLog<K>],
) -> (Arc<dyn ObjectStore>, MockMainNodeClient)
where
    K: SnapshotLogKey,
    for<'a> SnapshotStorageLogsChunk<K>: StoredObject<Key<'a> = SnapshotStorageLogsStorageKey>,
{
    let object_store = MockObjectStore::arc();
    let mut client = MockMainNodeClient::default();
    object_store
        .put(status.l1_batch_number, factory_deps)
        .await
        .unwrap();

    let chunk_size = logs
        .len()
        .div_ceil(status.storage_logs_chunks_processed.len());
    assert!(chunk_size > 0);

    for (chunk_id, chunk) in logs.chunks(chunk_size).enumerate() {
        let chunk_storage_logs = SnapshotStorageLogsChunk {
            storage_logs: chunk.to_vec(),
        };
        let chunk_key = SnapshotStorageLogsStorageKey {
            l1_batch_number: status.l1_batch_number,
            chunk_id: chunk_id as u64,
        };
        object_store
            .put(chunk_key, &chunk_storage_logs)
            .await
            .unwrap();
    }

    client.fetch_newest_snapshot_response = Some(mock_snapshot_header(K::VERSION.into(), status));
    client.fetch_l1_batch_responses.insert(
        status.l1_batch_number,
        l1_batch_details(status.l1_batch_number, status.l1_batch_root_hash),
    );
    client.fetch_l2_block_responses.insert(
        status.l2_block_number,
        l2_block_details(
            status.l2_block_number,
            status.l1_batch_number,
            status.l2_block_hash,
        ),
    );
    (object_store, client)
}

/// Object store wrapper that hangs up after processing the specified number of requests.
/// Used to emulate the snapshot applier being restarted since, if it's configured to have concurrency 1,
/// the applier will request an object from the store strictly after fully processing all previously requested objects.
#[derive(Debug)]
pub(super) struct HangingObjectStore {
    inner: Arc<dyn ObjectStore>,
    stop_after_count: usize,
    count_sender: watch::Sender<usize>,
}

impl HangingObjectStore {
    pub fn new(
        inner: Arc<dyn ObjectStore>,
        stop_after_count: usize,
    ) -> (Self, watch::Receiver<usize>) {
        let (count_sender, count_receiver) = watch::channel(0);
        let this = Self {
            inner,
            stop_after_count,
            count_sender,
        };
        (this, count_receiver)
    }
}

#[async_trait]
impl ObjectStore for HangingObjectStore {
    async fn get_raw(&self, bucket: Bucket, key: &str) -> Result<Vec<u8>, ObjectStoreError> {
        let mut should_proceed = true;
        self.count_sender.send_modify(|count| {
            *count += 1;
            if *count > self.stop_after_count {
                should_proceed = false;
            }
        });

        if should_proceed {
            self.inner.get_raw(bucket, key).await
        } else {
            future::pending().await // Hang up the snapshot applier task
        }
    }

    async fn put_raw(
        &self,
        _bucket: Bucket,
        _key: &str,
        _value: Vec<u8>,
    ) -> Result<(), ObjectStoreError> {
        unreachable!("Should not be used in snapshot applier")
    }

    async fn remove_raw(&self, _bucket: Bucket, _key: &str) -> Result<(), ObjectStoreError> {
        unreachable!("Should not be used in snapshot applier")
    }

    fn storage_prefix_raw(&self, bucket: Bucket) -> String {
        self.inner.storage_prefix_raw(bucket)
    }
}

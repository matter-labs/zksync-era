//! Test utils.

use std::{collections::HashMap, fmt, sync::Arc};

use async_trait::async_trait;
use zksync_object_store::{Bucket, ObjectStore, ObjectStoreError, ObjectStoreFactory};
use zksync_types::{
    api,
    block::L2BlockHeader,
    snapshots::{
        SnapshotFactoryDependencies, SnapshotFactoryDependency, SnapshotHeader,
        SnapshotRecoveryStatus, SnapshotStorageLog, SnapshotStorageLogsChunk,
        SnapshotStorageLogsChunkMetadata, SnapshotStorageLogsStorageKey, SnapshotVersion,
    },
    tokens::{TokenInfo, TokenMetadata},
    web3::Bytes,
    AccountTreeId, Address, L1BatchNumber, L2BlockNumber, ProtocolVersionId, StorageKey,
    StorageValue, H160, H256,
};
use zksync_web3_decl::error::EnrichedClientResult;

use crate::SnapshotsApplierMainNodeClient;

#[derive(Debug, Clone, Default)]
pub(super) struct MockMainNodeClient {
    pub fetch_l1_batch_responses: HashMap<L1BatchNumber, api::L1BatchDetails>,
    pub fetch_l2_block_responses: HashMap<L2BlockNumber, api::BlockDetails>,
    pub fetch_newest_snapshot_response: Option<SnapshotHeader>,
    pub tokens_response: Vec<TokenInfo>,
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

    async fn fetch_newest_snapshot(&self) -> EnrichedClientResult<Option<SnapshotHeader>> {
        Ok(self.fetch_newest_snapshot_response.clone())
    }

    async fn fetch_tokens(
        &self,
        _at_l2_block: L2BlockNumber,
    ) -> EnrichedClientResult<Vec<TokenInfo>> {
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
        prove_tx_hash: None,
        proven_at: None,
        execute_tx_hash: None,
        executed_at: None,
        l1_gas_price: 0,
        l2_fair_gas_price: 0,
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

pub(super) fn random_storage_logs(
    l1_batch_number: L1BatchNumber,
    count: u64,
) -> Vec<SnapshotStorageLog> {
    (0..count)
        .map(|i| SnapshotStorageLog {
            key: StorageKey::new(
                AccountTreeId::from_fixed_bytes(H160::random().to_fixed_bytes()),
                H256::random(),
            ),
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

pub(super) fn mock_snapshot_header(status: &SnapshotRecoveryStatus) -> SnapshotHeader {
    SnapshotHeader {
        version: SnapshotVersion::Version0.into(),
        l1_batch_number: status.l1_batch_number,
        l2_block_number: status.l2_block_number,
        storage_logs_chunks: vec![
            SnapshotStorageLogsChunkMetadata {
                chunk_id: 0,
                filepath: "file0".to_string(),
            },
            SnapshotStorageLogsChunkMetadata {
                chunk_id: 1,
                filepath: "file1".to_string(),
            },
        ],
        factory_deps_filepath: "some_filepath".to_string(),
    }
}

pub(super) async fn prepare_clients(
    status: &SnapshotRecoveryStatus,
    logs: &[SnapshotStorageLog],
) -> (Arc<dyn ObjectStore>, MockMainNodeClient) {
    let object_store_factory = ObjectStoreFactory::mock();
    let object_store = object_store_factory.create_store().await;
    let mut client = MockMainNodeClient::default();
    let factory_dep_bytes: Vec<u8> = (0..32).collect();
    let factory_deps = SnapshotFactoryDependencies {
        factory_deps: vec![SnapshotFactoryDependency {
            bytecode: Bytes::from(factory_dep_bytes),
        }],
    };
    object_store
        .put(status.l1_batch_number, &factory_deps)
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

    client.fetch_newest_snapshot_response = Some(mock_snapshot_header(status));
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

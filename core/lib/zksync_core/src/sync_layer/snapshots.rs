use crate::metadata_calculator::{MetadataCalculator, MetadataCalculatorConfig};
use crate::state_keeper::{L1BatchExecutorBuilder, MainBatchExecutorBuilder, ZkSyncStateKeeper};
use crate::sync_layer::fetcher::MainNodeFetcherCursor;
use crate::sync_layer::snapshots::SnapshotApplierError::*;
use crate::sync_layer::{ActionQueue, ExternalIO, MainNodeClient, SyncState};
use anyhow::Context;
use multivm::vm_1_3_2::zk_evm_1_3_3::ethereum_types::Address;
use serde_json::Value::Object;
use std::collections::HashMap;
use std::convert::TryInto;
use std::path::Path;
use tokio::sync::watch;
use zksync_config::configs::object_store::ObjectStoreMode;
use zksync_config::ObjectStoreConfig;
use zksync_dal::connection::DbVariant;
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_merkle_tree::recovery::{MerkleTreeRecovery, RecoveryEntry};
use zksync_merkle_tree::RocksDBWrapper;
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_types::block::{BlockGasCount, MiniblockHeader};
use zksync_types::snapshots::{
    AppliedSnapshotStatus, SnapshotFactoryDependencies, SnapshotHeader, SnapshotStorageLog,
    SnapshotStorageLogsChunk, SnapshotStorageLogsStorageKey,
};
use zksync_types::{L1BatchNumber, L2ChainId, ProtocolVersionId, H256};

pub struct StateKeeperConfig {
    pub connection_pool: ConnectionPool,
    pub state_keeper_db_path: String,
    pub l2_erc20_bridge_addr: Address,
    pub chain_id: L2ChainId,
    pub main_node_url: String,
    pub enum_index_migration_chunk_size: usize,
}
pub struct SnapshotApplier<'a, 'b, 'd> {
    connection_pool: &'a ConnectionPool,
    client: &'a dyn MainNodeClient,
    recovery: MerkleTreeRecovery<'b, RocksDBWrapper>,
    blob_store: Box<dyn ObjectStore>,
    applied_snapshot_status: AppliedSnapshotStatus,
    snapshot: SnapshotHeader,
    state_keeper_config: StateKeeperConfig,
    metadata_calculator_config: MetadataCalculatorConfig<'d>,
}

#[derive(thiserror::Error, Debug)]
pub enum SnapshotApplierError {
    #[error("canceled")]
    Canceled(String),
    #[error(transparent)]
    Fatal(#[from] anyhow::Error),
    #[error("retryable")]
    Retryable(String),
}

impl<'a, 'b, 'd> SnapshotApplier<'a, 'b, 'd> {
    pub async fn new(
        connection_pool: &'a ConnectionPool,
        client: &'a dyn MainNodeClient,
        merkle_tree_db_path: &String,
        state_keeper_config: StateKeeperConfig,
        metadata_calculator_config: MetadataCalculatorConfig<'d>,
    ) -> anyhow::Result<SnapshotApplier<'a, 'b, 'd>, SnapshotApplierError> {
        let mut storage = connection_pool
            .access_storage_tagged("snapshots_applier")
            .await
            .unwrap();
        let mut storage = storage.start_transaction().await.unwrap();

        let mut applied_snapshot_status = storage
            .applied_snapshot_status_dal()
            .get_applied_snapshot_status()
            .await
            .unwrap();

        if !storage.blocks_dal().is_genesis_needed().await.unwrap()
            && applied_snapshot_status.is_none()
        {
            return Err(Canceled(
                "This node has already been initialized without a snapshot".to_string(),
            ));
        }

        if applied_snapshot_status.is_some()
            && applied_snapshot_status.as_ref().unwrap().is_finished
        {
            return Err(Canceled(
                "This node has already been initialized from a snapshot".to_string(),
            ));
        }

        let snapshot_response = client.fetch_newest_snapshot().await.unwrap();
        if snapshot_response.is_none() {
            return Err(Canceled("Main node does not have any ready snapshots, skipping initialization from snapshot!".to_string()));
        }
        let snapshot = snapshot_response.unwrap();
        let l1_batch_number = snapshot.l1_batch_number;
        tracing::info!("Found snapshot with data up to l1_batch {}, created at {}, storage_logs are divided into {} chunk(s)", l1_batch_number, snapshot.generated_at, snapshot.storage_logs_chunks.len());

        if applied_snapshot_status.is_none() {
            applied_snapshot_status = Some(AppliedSnapshotStatus {
                l1_batch_number,
                is_finished: false,
                last_finished_chunk_id: None,
            });
        };
        let applied_snapshot_status = applied_snapshot_status.unwrap();

        let recovered_version = l1_batch_number.0 as u64;

        let rocks_db = RocksDBWrapper::new(Path::new(&merkle_tree_db_path));

        let recovery = MerkleTreeRecovery::new(rocks_db, recovered_version);

        let object_store_config = ObjectStoreConfig {
            bucket_base_url: "".to_string(),
            mode: ObjectStoreMode::FileBacked,
            file_backed_base_path: "artifacts".to_string(),
            gcs_credential_file_path: "".to_string(),
            max_retries: 5,
        };
        let blob_store = ObjectStoreFactory::new(object_store_config)
            .create_store()
            .await;

        storage
            .applied_snapshot_status_dal()
            .set_applied_snapshot_status(&applied_snapshot_status)
            .await
            .unwrap();
        Ok(Self {
            connection_pool,
            client,
            recovery,
            blob_store,
            applied_snapshot_status,
            snapshot,
            state_keeper_config,
            metadata_calculator_config,
        })
    }

    async fn build_state_keeper(config: StateKeeperConfig) -> ZkSyncStateKeeper {
        let (_, stop_receiver) = watch::channel(false);

        let sync_state = SyncState::new();
        let (action_queue_sender, action_queue) = ActionQueue::new();

        let max_allowed_l2_tx_gas_limit = u32::MAX.into();
        let validation_computational_gas_limit = u32::MAX;

        let batch_executor_base: Box<dyn L1BatchExecutorBuilder> =
            Box::new(MainBatchExecutorBuilder::new(
                config.state_keeper_db_path,
                config.connection_pool.clone(),
                max_allowed_l2_tx_gas_limit,
                false,
                false,
                config.enum_index_migration_chunk_size,
            ));

        let main_node_url = config.main_node_url.clone();
        let main_node_client = <dyn MainNodeClient>::json_rpc(&main_node_url)
            .expect("Failed creating JSON-RPC client for main node");

        let fetcher_cursor = {
            let mut storage = config
                .connection_pool
                .access_storage_tagged("sync_layer")
                .await
                .unwrap();
            MainNodeFetcherCursor::new(&mut storage)
                .await
                .context("failed to load `MainNodeFetcher` cursor from Postgres")
                .unwrap()
        };
        tracing::info!("Initializing fetcher for snapshot applier!");
        let fetcher = fetcher_cursor.into_fetcher(
            Box::new(main_node_client),
            action_queue_sender,
            sync_state.clone(),
            stop_receiver.clone(),
        );
        fetcher.run(true).await.unwrap();
        tracing::info!("Finished running fetcher!");

        let main_node_url = config.main_node_url;
        let main_node_client = <dyn MainNodeClient>::json_rpc(&main_node_url)
            .expect("Failed creating JSON-RPC client for main node");

        tracing::info!("Creating external io instance!");
        let io = ExternalIO::new(
            config.connection_pool,
            action_queue,
            sync_state,
            Box::new(main_node_client),
            config.l2_erc20_bridge_addr,
            validation_computational_gas_limit,
            config.chain_id,
        )
        .await;

        ZkSyncStateKeeper::without_sealer(stop_receiver, Box::new(io), batch_executor_base)
    }
    async fn sync_protocol_version(
        &mut self,
        id: ProtocolVersionId,
        storage: &mut StorageProcessor<'_>,
    ) {
        let protocol_version = self
            .client
            .fetch_protocol_version(id)
            .await
            .expect("Failed to fetch protocol version from the main node");
        storage
            .protocol_versions_dal()
            .save_protocol_version(
                protocol_version.version_id.try_into().unwrap(),
                protocol_version.timestamp,
                protocol_version.verification_keys_hashes,
                protocol_version.base_system_contracts,
                // Verifier is not used in the external node, so we can pass an empty
                Default::default(),
                protocol_version.l2_system_upgrade_tx_hash,
            )
            .await;
    }

    async fn insert_dummy_miniblock_header(&mut self, storage: &mut StorageProcessor<'_>) {
        let sync_block = self
            .client
            .fetch_l2_block(self.snapshot.miniblock_number, false)
            .await
            .expect("Failed to fetch block from the main node")
            .expect("Block must exist");
        self.sync_protocol_version(sync_block.protocol_version, storage)
            .await;
        storage
            .blocks_dal()
            .insert_miniblock(&MiniblockHeader {
                number: sync_block.number,
                timestamp: sync_block.timestamp,
                hash: sync_block.hash.unwrap(),
                l1_tx_count: 0,
                l2_tx_count: 0,
                base_fee_per_gas: 0,
                l1_gas_price: sync_block.l1_gas_price,
                l2_fair_gas_price: sync_block.l2_fair_gas_price,
                base_system_contracts_hashes: sync_block.base_system_contracts_hashes,
                protocol_version: Some(sync_block.protocol_version),
                virtual_blocks: sync_block.virtual_blocks.unwrap(),
            })
            .await
            .unwrap();
        storage
            .blocks_dal()
            .update_hashes(&[(sync_block.number, sync_block.hash.unwrap())])
            .await
            .unwrap();
        tracing::info!("Fetched miniblock {} from main node", sync_block.number)
    }

    async fn insert_dummy_l1_batch_metadata(&mut self, storage: &mut StorageProcessor<'_>) {
        let l1_batch_header_with_metadata = &self.snapshot.last_l1_batch_with_metadata;
        let l1_batch_header = &l1_batch_header_with_metadata.header;
        let l1_batch_metadata = &l1_batch_header_with_metadata.metadata;

        storage
            .blocks_dal()
            .insert_l1_batch(l1_batch_header, &[], BlockGasCount::default(), &[], &[])
            .await
            .unwrap();
        storage
            .blocks_dal()
            .save_l1_batch_metadata(l1_batch_header.number, l1_batch_metadata, H256::zero())
            .await
            .unwrap();
        storage
            .blocks_dal()
            .mark_miniblocks_as_executed_in_l1_batch(l1_batch_header.number)
            .await
            .unwrap();
    }

    async fn sync_initial_writes_chunk(
        &mut self,
        storage_logs: &[SnapshotStorageLog],
        storage: &mut StorageProcessor<'_>,
    ) {
        tracing::info!("Loading {} storage logs into postgres", storage_logs.len());
        storage
            .storage_logs_dedup_dal()
            .insert_initial_writes_from_snapshot(&storage_logs)
            .await;
    }
    async fn sync_storage_logs_chunk(
        &mut self,
        storage_logs: &[SnapshotStorageLog],
        storage: &mut StorageProcessor<'_>,
    ) {
        storage
            .storage_logs_dal()
            .insert_storage_logs_from_snapshot(self.snapshot.miniblock_number, storage_logs)
            .await;
    }

    async fn sync_tree_chunk(&mut self, storage_logs: &[SnapshotStorageLog]) {
        tracing::info!("syncing tree with {} storage logs", storage_logs.len());
        let logs_for_merkle_tree = storage_logs
            .iter()
            .map(|log| RecoveryEntry {
                key: log.key.hashed_key_u256(),
                value: log.value,
                leaf_index: log.enumeration_index,
            })
            .collect();

        self.recovery.extend(logs_for_merkle_tree);
    }

    async fn sync_factory_deps(&mut self, storage: &mut StorageProcessor<'_>) {
        let factory_deps: SnapshotFactoryDependencies = self
            .blob_store
            .get(self.snapshot.l1_batch_number)
            .await
            .unwrap();

        let all_deps_hashmap: HashMap<H256, Vec<u8>> = factory_deps
            .factory_deps
            .into_iter()
            .map(|dep| (dep.bytecode_hash, dep.bytecode))
            .collect();
        storage
            .storage_dal()
            .insert_factory_deps(self.snapshot.miniblock_number, &all_deps_hashmap)
            .await;
    }

    async fn sync_storage_logs_single_chunk(
        &mut self,
        l1_batch_number: L1BatchNumber,
        chunk_id: u64,
        filepath: &str,
    ) {
        let mut storage = self
            .connection_pool
            .access_storage_tagged("snapshots_applier")
            .await
            .unwrap();
        let mut storage = storage.start_transaction().await.unwrap();

        let storage_key = SnapshotStorageLogsStorageKey {
            l1_batch_number,
            chunk_id,
        };

        if self
            .applied_snapshot_status
            .last_finished_chunk_id
            .is_some()
            && chunk_id > self.applied_snapshot_status.last_finished_chunk_id.unwrap()
        {
            tracing::info!(
                "Skipping processing chunk {}, file already processed",
                chunk_id
            );
        }
        tracing::info!("Processing chunk {chunk_id} located in {filepath}");

        let storage_snapshot_chunk: SnapshotStorageLogsChunk =
            self.blob_store.get(storage_key).await.unwrap();

        let storage_logs = &storage_snapshot_chunk.storage_logs;
        self.sync_storage_logs_chunk(storage_logs, &mut storage)
            .await;

        self.sync_initial_writes_chunk(storage_logs, &mut storage)
            .await;

        self.sync_tree_chunk(storage_logs).await;

        self.applied_snapshot_status.last_finished_chunk_id = Some(chunk_id);
        storage
            .applied_snapshot_status_dal()
            .set_applied_snapshot_status(&self.applied_snapshot_status)
            .await
            .unwrap();

        storage.commit().await.unwrap();
    }

    async fn clear_dummy_headers(storage: &mut StorageProcessor<'_>, snapshot: SnapshotHeader) {
        storage
            .blocks_dal()
            .clear_dummy_snapshot_headers(snapshot.l1_batch_number, snapshot.miniblock_number)
            .await
            .unwrap();
    }
    async fn finalize_applying_snapshot(mut self) {
        let mut storage = self
            .connection_pool
            .access_storage_tagged("snapshots_applier")
            .await
            .unwrap();
        let mut storage = storage.start_transaction().await.unwrap();

        tracing::info!("Processing chunks finished, finalizing merkle tree");
        {
            self.recovery.finalize();
        }

        tracing::info!("Finished finalizing merkle tree, Running state keeper");
        let state_keeper = SnapshotApplier::build_state_keeper(self.state_keeper_config).await;
        state_keeper.run(true).await.unwrap();
        tracing::info!("Finished running state keeper for snapshot applier, updating status");
        let metadata_calculator = MetadataCalculator::new(&self.metadata_calculator_config).await;

        let (_, stop_receiver) = watch::channel(false);
        let tree_stop_receiver = stop_receiver.clone();
        let tree_pool = ConnectionPool::singleton(DbVariant::Master)
            .build()
            .await
            .context("failed to build a tree_pool")
            .unwrap();
        let prover_tree_pool = ConnectionPool::singleton(DbVariant::Prover)
            .build()
            .await
            .context("failed to build a prover_tree_pool")
            .unwrap();

        metadata_calculator
            .run(tree_pool, prover_tree_pool, tree_stop_receiver, true)
            .await
            .unwrap();

        self.applied_snapshot_status.is_finished = true;
        storage
            .applied_snapshot_status_dal()
            .set_applied_snapshot_status(&self.applied_snapshot_status)
            .await
            .unwrap();
        SnapshotApplier::clear_dummy_headers(&mut storage, self.snapshot).await;

        storage.commit().await.unwrap();
        tracing::info!("Finished applying snapshot");
    }

    pub async fn sync_initial_data_and_factory_deps(&mut self) {
        let mut storage = self
            .connection_pool
            .access_storage_tagged("snapshots_applier")
            .await
            .unwrap();
        let mut storage = storage.start_transaction().await.unwrap();

        self.insert_dummy_miniblock_header(&mut storage).await;

        self.insert_dummy_l1_batch_metadata(&mut storage).await;

        self.sync_factory_deps(&mut storage).await;

        storage.commit().await.unwrap();
    }

    pub async fn load_snapshot(mut self) -> Result<(), SnapshotApplierError> {
        self.sync_initial_data_and_factory_deps().await;

        for chunk_metadata in self.snapshot.storage_logs_chunks.clone().iter() {
            self.sync_storage_logs_single_chunk(
                self.snapshot.l1_batch_number,
                chunk_metadata.chunk_id,
                &chunk_metadata.filepath,
            )
            .await;
        }

        self.finalize_applying_snapshot().await;

        Ok(())
    }
}

pub async fn load_from_snapshot_if_needed(
    pool: ConnectionPool,
    client: &dyn MainNodeClient,
    merkle_tree_db_path: &String,
    state_keeper_params: StateKeeperConfig,
    metadata_calculator_config: MetadataCalculatorConfig<'_>,
) -> anyhow::Result<()> {
    let applier = SnapshotApplier::new(
        &pool,
        client,
        merkle_tree_db_path,
        state_keeper_params,
        metadata_calculator_config,
    )
    .await
    .unwrap();

    applier.load_snapshot().await.unwrap();

    Ok(())
}

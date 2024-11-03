use std::{collections::HashMap, sync::Arc, time::SystemTime};

use async_trait::async_trait;
use tempfile::TempDir;
use zksync_basic_types::{
    protocol_version::ProtocolVersionId, Address, L1BatchNumber, L2BlockNumber, H256,
};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_eth_client::EnrichedClientResult;
use zksync_object_store::ObjectStore;
use zksync_snapshots_applier::{L1BlockMetadata, L2BlockMetadata, SnapshotsApplierMainNodeClient};
use zksync_types::{
    snapshots::{
        SnapshotFactoryDependencies, SnapshotHeader, SnapshotRecoveryStatus,
        SnapshotStorageLogsChunk, SnapshotStorageLogsChunkMetadata, SnapshotStorageLogsStorageKey,
        SnapshotVersion,
    },
    tokens::TokenInfo,
};
use zksync_utils::bytecode::hash_bytecode;
use zksync_web3_decl::client::{DynClient, L1};

use crate::{
    l1_fetcher::{
        blob_http_client::BlobClient,
        l1_fetcher::{L1Fetcher, L1FetcherConfig, ProtocolVersioning::OnlyV3},
    },
    processor::snapshot::StateCompressor,
};

pub async fn recover_db(
    connection_pool: ConnectionPool<Core>,
    l1_client: Box<DynClient<L1>>,
    blob_client: Arc<dyn BlobClient>,
    diamond_proxy_addr: Address,
) {
    let temp_dir = TempDir::new().unwrap().into_path().join("db");

    let l1_fetcher = L1Fetcher::new(
        L1FetcherConfig {
            block_step: 100000,
            diamond_proxy_addr,
            versioning: OnlyV3,
        },
        l1_client,
        blob_client,
    );
    let blocks = l1_fetcher.unwrap().get_all_blocks_to_process().await;
    let last_l1_batch_number = blocks.last().unwrap().l1_batch_number;
    let dummy_miniblock_number = L2BlockNumber(last_l1_batch_number as u32);
    let timestamp_now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let mut processor = StateCompressor::new(temp_dir).await;
    processor.process_genesis_state(None).await;
    processor.process_blocks(blocks).await;

    let mut storage = connection_pool
        .connection_tagged("l1_recovery")
        .await
        .unwrap();

    storage
        .storage_logs_dal()
        .insert_storage_logs_from_snapshot(
            dummy_miniblock_number,
            &processor.export_storage_logs().await,
        )
        .await
        .unwrap();

    let chunk_deps_hashmap: HashMap<H256, Vec<u8>> = processor
        .export_factory_deps()
        .await
        .iter()
        .map(|dep| (hash_bytecode(&dep.bytecode.0), dep.bytecode.0.clone()))
        .collect();
    storage
        .factory_deps_dal()
        .insert_factory_deps(dummy_miniblock_number, &chunk_deps_hashmap)
        .await
        .unwrap();

    let status = SnapshotRecoveryStatus {
        l1_batch_number: L1BatchNumber(last_l1_batch_number as u32),
        l1_batch_root_hash: processor.get_root_hash(),
        l1_batch_timestamp: timestamp_now,
        l2_block_number: dummy_miniblock_number,
        l2_block_hash: H256::zero(),
        l2_block_timestamp: timestamp_now,
        protocol_version: Default::default(),
        storage_logs_chunks_processed: vec![true],
    };
    storage
        .snapshot_recovery_dal()
        .insert_initial_recovery_status(&status)
        .await
        .unwrap()
}

pub async fn create_l1_snapshot(
    l1_client: Box<DynClient<L1>>,
    blob_client: Arc<dyn BlobClient>,
    object_store: &Arc<dyn ObjectStore>,
    diamond_proxy_addr: Address,
) -> (L1BatchNumber, H256) {
    let temp_dir = TempDir::new().unwrap().into_path().join("db");

    let l1_fetcher = L1Fetcher::new(
        L1FetcherConfig {
            block_step: 100000,
            diamond_proxy_addr,
            versioning: OnlyV3,
        },
        l1_client,
        blob_client,
    );
    let blocks = l1_fetcher.unwrap().get_all_blocks_to_process().await;
    let last_l1_batch_number = L1BatchNumber(blocks.last().unwrap().l1_batch_number as u32);
    let mut processor = StateCompressor::new(temp_dir).await;
    processor.process_genesis_state(None).await;
    processor.process_blocks(blocks).await;

    tracing::info!(
        "Processing L1 data finished, recovered tree root hash {:?}",
        processor.get_root_hash()
    );

    let key = SnapshotStorageLogsStorageKey {
        l1_batch_number: last_l1_batch_number,
        chunk_id: 0,
    };
    let storage_logs = SnapshotStorageLogsChunk {
        storage_logs: processor.export_storage_logs().await,
    };
    tracing::info!("Dumping {} storage logs", storage_logs.storage_logs.len());
    object_store.put(key, &storage_logs).await.unwrap();

    let factory_deps = SnapshotFactoryDependencies {
        factory_deps: processor.export_factory_deps().await,
    };
    tracing::info!("Dumping {} factory deps", factory_deps.factory_deps.len());
    object_store
        .put(last_l1_batch_number, &factory_deps)
        .await
        .unwrap();

    (last_l1_batch_number, processor.get_root_hash())
}

#[derive(Debug, Clone)]
struct FakeClient {
    newest_l1_batch_number: L1BatchNumber,
    root_hash: H256,
}

#[async_trait]
impl SnapshotsApplierMainNodeClient for FakeClient {
    async fn fetch_l1_batch_details(
        &self,
        number: L1BatchNumber,
    ) -> EnrichedClientResult<Option<L1BlockMetadata>> {
        assert_eq!(number, self.newest_l1_batch_number);
        Ok(Some(L1BlockMetadata {
            root_hash: Some(self.root_hash),
            timestamp: 0,
        }))
    }

    async fn fetch_l2_block_details(
        &self,
        number: L2BlockNumber,
    ) -> EnrichedClientResult<Option<L2BlockMetadata>> {
        assert_eq!(number, L2BlockNumber(0));
        Ok(Some(L2BlockMetadata {
            // TODO calculate this using storage logs
            block_hash: Some(H256::zero()),
            protocol_version: Some(ProtocolVersionId::latest()),
            timestamp: 0,
        }))
    }

    async fn fetch_newest_snapshot_l1_batch_number(
        &self,
    ) -> EnrichedClientResult<Option<L1BatchNumber>> {
        unimplemented!("Shouldn't be called")
    }

    async fn fetch_snapshot(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> EnrichedClientResult<Option<SnapshotHeader>> {
        assert_eq!(l1_batch_number, self.newest_l1_batch_number);
        Ok(Some(SnapshotHeader {
            version: SnapshotVersion::Version1.into(),
            l1_batch_number,
            l2_block_number: Default::default(),
            storage_logs_chunks: vec![SnapshotStorageLogsChunkMetadata {
                chunk_id: 0,
                filepath: "".to_string(),
            }],
            factory_deps_filepath: "".to_string(),
        }))
    }

    async fn fetch_tokens(
        &self,
        _at_l2_block: L2BlockNumber,
    ) -> EnrichedClientResult<Vec<TokenInfo>> {
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use std::{str::FromStr, sync::Arc};

    use tokio::sync::watch;
    use zksync_basic_types::url::SensitiveUrl;
    use zksync_dal::{ConnectionPool, Core};
    use zksync_object_store::MockObjectStore;
    use zksync_snapshots_applier::{SnapshotsApplierConfig, SnapshotsApplierTask};
    use zksync_web3_decl::client::Client;

    use crate::{
        l1_fetcher::{blob_http_client::LocalDbBlobSource, constants::local_diamond_proxy_addr},
        processor::db_recovery::{create_l1_snapshot, FakeClient},
        recover_db,
    };

    #[test_log::test(tokio::test)]
    async fn test_process_blocks() {
        let connection_pool = ConnectionPool::<Core>::builder(
            "postgres://postgres:notsecurepassword@localhost:5432/zksync_server_localhost_era"
                .parse()
                .unwrap(),
            10,
        )
        .build()
        .await
        .unwrap();
        let url = SensitiveUrl::from_str(&"http://127.0.0.1:8545").unwrap();
        let eth_client = Client::http(url).unwrap().build();
        let blob_client = Arc::new(LocalDbBlobSource::new(connection_pool.clone()));
        recover_db(
            ConnectionPool::test_pool().await,
            Box::new(eth_client),
            blob_client,
            local_diamond_proxy_addr().parse().unwrap(),
        )
        .await;
    }

    #[test_log::test(tokio::test)]
    async fn test_export_storage_logs() {
        let connection_pool = ConnectionPool::<Core>::builder(
            "postgres://postgres:notsecurepassword@localhost:5432/zksync_server_localhost_era"
                .parse()
                .unwrap(),
            10,
        )
        .build()
        .await
        .unwrap();
        let blob_client = Arc::new(LocalDbBlobSource::new(connection_pool.clone()));
        let url = SensitiveUrl::from_str(&"http://127.0.0.1:8545").unwrap();
        let eth_client = Client::http(url).unwrap().build();
        let object_store = MockObjectStore::arc();
        let (newest_l1_batch_number, root_hash) = create_l1_snapshot(
            Box::new(eth_client),
            blob_client,
            &object_store,
            local_diamond_proxy_addr().parse().unwrap(),
        )
        .await;

        let applier_config = SnapshotsApplierConfig::default();
        let (_, stop_rx) = watch::channel(false);
        let mut applier = SnapshotsApplierTask::new(
            applier_config,
            connection_pool,
            Box::new(FakeClient {
                newest_l1_batch_number,
                root_hash,
            }),
            object_store,
        );
        applier.set_snapshot_l1_batch(newest_l1_batch_number);
        applier.run(stop_rx).await.unwrap();
    }
}

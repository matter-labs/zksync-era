use std::{collections::HashMap, time::SystemTime};

use tempfile::TempDir;
use zksync_basic_types::{Address, L1BatchNumber, L2BlockNumber, H256};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_types::snapshots::{SnapshotFactoryDependency, SnapshotRecoveryStatus};
use zksync_utils::bytecode::hash_bytecode;
use zksync_web3_decl::client::{DynClient, L1};

use crate::{
    l1_fetcher::l1_fetcher::{L1Fetcher, L1FetcherConfig, ProtocolVersioning::OnlyV3},
    processor::{genesis::get_genesis_factory_deps, snapshot::StateCompressor},
};

pub async fn recover_db(
    connection_pool: ConnectionPool<Core>,
    l1_client: Box<DynClient<L1>>,
    diamond_proxy_addr: Address,
) {
    let temp_dir = TempDir::new().unwrap().into_path().join("db");

    let l1_fetcher = L1Fetcher::new(
        L1FetcherConfig {
            blobs_url: "LOCAL_BLOBS_ONLY!".to_string(),
            block_step: 100000,
            diamond_proxy_addr,
            versioning: OnlyV3,
        },
        l1_client,
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

    let mut factory_deps = get_genesis_factory_deps()
        .iter()
        .map(|dep| SnapshotFactoryDependency {
            bytecode: dep.clone().into(),
        })
        .collect::<Vec<_>>();
    factory_deps.extend(processor.export_factory_deps().await);

    let chunk_deps_hashmap: HashMap<H256, Vec<u8>> = factory_deps
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use zksync_basic_types::url::SensitiveUrl;
    use zksync_dal::{ConnectionPool, Core};
    use zksync_web3_decl::client::Client;

    use crate::recover_db;

    #[test_log::test(tokio::test)]
    async fn test_process_blocks() {
        let connection_pool = ConnectionPool::<Core>::test_pool().await;
        let url = SensitiveUrl::from_str(&"http://127.0.0.1:8545").unwrap();
        let eth_client = Client::http(url).unwrap().build();
        recover_db(
            connection_pool,
            Box::new(eth_client),
            "0x461993b882e2e6a5dea726db44d98c958a67365b"
                .parse()
                .unwrap(),
        )
        .await;
    }
}

//! Snapshot applier tests.

use std::collections::HashMap;

use zksync_dal::ConnectionPool;
use zksync_object_store::ObjectStoreFactory;
use zksync_types::{
    snapshots::{
        SnapshotFactoryDependencies, SnapshotFactoryDependency, SnapshotHeader,
        SnapshotRecoveryStatus, SnapshotStorageLog, SnapshotStorageLogsChunk,
        SnapshotStorageLogsChunkMetadata, SnapshotStorageLogsStorageKey,
    },
    Bytes, L1BatchNumber, MiniblockNumber, H256,
};

use self::utils::{l1_block_metadata, miniblock_metadata, random_storage_logs, MockMainNodeClient};
use crate::SnapshotsApplier;

mod utils;

#[tokio::test]
async fn snapshots_creator_can_successfully_recover_db() {
    let pool = ConnectionPool::test_pool().await;
    let object_store_factory = ObjectStoreFactory::mock();
    let object_store = object_store_factory.create_store().await;
    let mut client = MockMainNodeClient::default();
    let miniblock_number = MiniblockNumber(1234);
    let l1_batch_number = L1BatchNumber(123);
    let l1_batch_root_hash = H256::random();
    let miniblock_hash = H256::random();
    let factory_dep_bytes: Vec<u8> = (0..32).collect();
    let factory_deps = SnapshotFactoryDependencies {
        factory_deps: vec![SnapshotFactoryDependency {
            bytecode: Bytes::from(factory_dep_bytes),
        }],
    };
    object_store
        .put(l1_batch_number, &factory_deps)
        .await
        .unwrap();

    let mut all_snapshot_storage_logs = HashMap::<H256, SnapshotStorageLog>::new();
    for chunk_id in 0..2 {
        let chunk_storage_logs = SnapshotStorageLogsChunk {
            storage_logs: random_storage_logs(l1_batch_number, chunk_id, 10),
        };
        let chunk_key = SnapshotStorageLogsStorageKey {
            l1_batch_number,
            chunk_id,
        };
        object_store
            .put(chunk_key, &chunk_storage_logs)
            .await
            .unwrap();

        all_snapshot_storage_logs.extend(
            chunk_storage_logs
                .storage_logs
                .into_iter()
                .map(|log| (log.key.hashed_key(), log)),
        );
    }

    let snapshot_header = SnapshotHeader {
        l1_batch_number,
        miniblock_number,
        last_l1_batch_with_metadata: l1_block_metadata(l1_batch_number, l1_batch_root_hash),
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
    };
    client.fetch_newest_snapshot_response = Some(snapshot_header);
    client.fetch_l2_block_responses.insert(
        miniblock_number,
        miniblock_metadata(miniblock_number, l1_batch_number, miniblock_hash),
    );

    SnapshotsApplier::load_snapshot(&pool, &client, &object_store)
        .await
        .unwrap();

    let mut storage = pool.access_storage().await.unwrap();
    let mut recovery_dal = storage.snapshot_recovery_dal();

    let expected_status = SnapshotRecoveryStatus {
        l1_batch_number,
        l1_batch_root_hash,
        miniblock_number,
        miniblock_root_hash: miniblock_hash,
        storage_logs_chunks_processed: vec![true, true],
    };

    let current_db_status = recovery_dal.get_applied_snapshot_status().await.unwrap();
    assert_eq!(current_db_status.unwrap(), expected_status);

    let all_initial_writes = storage
        .storage_logs_dedup_dal()
        .dump_all_initial_writes_for_tests()
        .await;
    assert_eq!(all_initial_writes.len(), all_snapshot_storage_logs.len());
    for initial_write in all_initial_writes {
        let log = &all_snapshot_storage_logs[&initial_write.hashed_key];
        assert_eq!(
            initial_write.l1_batch_number,
            log.l1_batch_number_of_initial_write
        );
        assert_eq!(initial_write.index, log.enumeration_index);
    }

    let all_storage_logs = storage
        .storage_logs_dal()
        .dump_all_storage_logs_for_tests()
        .await;
    assert_eq!(all_storage_logs.len(), all_snapshot_storage_logs.len());
    for db_log in all_storage_logs {
        let expected_log = &all_snapshot_storage_logs[&db_log.hashed_key];
        assert_eq!(db_log.address, *expected_log.key.address());
        assert_eq!(db_log.key, *expected_log.key.key());
        assert_eq!(db_log.value, expected_log.value);
        assert_eq!(db_log.miniblock_number, miniblock_number);
    }
}

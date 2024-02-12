//! Snapshot applier tests.

use std::collections::HashMap;

use assert_matches::assert_matches;
use test_casing::test_casing;
use zksync_object_store::ObjectStoreFactory;
use zksync_types::{
    block::{L1BatchHeader, MiniblockHeader},
    snapshots::{SnapshotFactoryDependency, SnapshotStorageLogsChunkMetadata},
    Address, Bytes, L1BatchNumber, ProtocolVersion, ProtocolVersionId,
};

use self::utils::{l1_block_metadata, miniblock_metadata, random_storage_logs, MockMainNodeClient};
use super::*;

mod utils;

#[test_casing(2, [None, Some(2)])]
#[tokio::test]
async fn snapshots_creator_can_successfully_recover_db(pool_size: Option<u32>) {
    let pool = if let Some(pool_size) = pool_size {
        ConnectionPool::constrained_test_pool(pool_size).await
    } else {
        ConnectionPool::test_pool().await
    };
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

    let outcome = SnapshotsApplier::load_snapshot(&pool, &client, &object_store)
        .await
        .unwrap();
    assert_matches!(outcome, SnapshotsApplierOutcome::Ok);

    let mut storage = pool.access_storage().await.unwrap();
    let mut recovery_dal = storage.snapshot_recovery_dal();

    let expected_status = SnapshotRecoveryStatus {
        l1_batch_number,
        l1_batch_root_hash,
        l1_batch_timestamp: 0,
        miniblock_number,
        miniblock_hash,
        miniblock_timestamp: 0,
        protocol_version: ProtocolVersionId::default(),
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

    // Try recovering again; it should return early.
    let err = SnapshotsApplier::load_snapshot_inner(&pool, &client, &object_store)
        .await
        .unwrap_err();
    assert_matches!(
        err,
        SnapshotsApplierError::EarlyReturn(SnapshotsApplierOutcome::Ok)
    );
}

#[tokio::test]
async fn applier_returns_early_after_genesis() {
    let pool = ConnectionPool::test_pool().await;

    // We don't want to depend on the core crate, so instead we cheaply emulate it.
    let mut storage = pool.access_storage().await.unwrap();
    storage
        .protocol_versions_dal()
        .save_protocol_version_with_tx(ProtocolVersion::default())
        .await;
    let genesis_miniblock = MiniblockHeader {
        number: MiniblockNumber(0),
        timestamp: 0,
        hash: H256::zero(),
        l1_tx_count: 0,
        l2_tx_count: 0,
        fee_account_address: Address::repeat_byte(1),
        base_fee_per_gas: 1,
        batch_fee_input: Default::default(),
        gas_per_pubdata_limit: 2,
        base_system_contracts_hashes: Default::default(),
        protocol_version: Some(ProtocolVersionId::latest()),
        virtual_blocks: 0,
    };
    storage
        .blocks_dal()
        .insert_miniblock(&genesis_miniblock)
        .await
        .unwrap();
    let genesis_l1_batch = L1BatchHeader::new(
        L1BatchNumber(0),
        0,
        Default::default(),
        ProtocolVersionId::latest(),
    );
    storage
        .blocks_dal()
        .insert_mock_l1_batch(&genesis_l1_batch)
        .await
        .unwrap();
    storage
        .blocks_dal()
        .mark_miniblocks_as_executed_in_l1_batch(L1BatchNumber(0))
        .await
        .unwrap();

    let object_store_factory = ObjectStoreFactory::mock();
    let object_store = object_store_factory.create_store().await;
    let client = MockMainNodeClient::default();

    let outcome = SnapshotsApplier::load_snapshot(&pool, &client, &object_store)
        .await
        .unwrap();
    assert_matches!(outcome, SnapshotsApplierOutcome::InitializedWithoutSnapshot);
}

#[tokio::test]
async fn applier_returns_early_without_snapshots() {
    let pool = ConnectionPool::test_pool().await;
    let object_store_factory = ObjectStoreFactory::mock();
    let object_store = object_store_factory.create_store().await;
    let client = MockMainNodeClient::default();

    let outcome = SnapshotsApplier::load_snapshot(&pool, &client, &object_store)
        .await
        .unwrap();
    assert_matches!(outcome, SnapshotsApplierOutcome::NoSnapshotsOnMainNode);
}

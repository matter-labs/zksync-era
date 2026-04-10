//! General-purpose tree manager tests.

use std::{collections::HashMap, path::Path, sync::Arc};

use assert_matches::assert_matches;
use tempfile::TempDir;
use test_casing::test_casing;
use tokio::sync::Barrier;
use zk_os_merkle_tree::{
    BatchOutput, Blake2Hasher, DefaultTreeParams, MerkleTree, PatchSet, TreeOperation, TreeParams,
};
use zksync_dal::{Connection, CoreDal};
use zksync_health_check::HealthStatus;
use zksync_node_genesis::{insert_genesis_batch, GenesisParamsInitials};
use zksync_node_test_utils::{
    create_l1_batch, create_l2_block, generate_storage_logs, insert_initial_writes_for_batch,
};
use zksync_types::{L1BatchNumber, StorageLog, H256};

use super::*;
use crate::{batch::L1BatchWithLogs, health::MerkleTreeInfo};

pub(crate) async fn setup_tree_manager(db_path: &Path, pool: ConnectionPool<Core>) -> TreeManager {
    let mut conn = pool.connection().await.unwrap();
    if conn.blocks_dal().is_genesis_needed().await.unwrap() {
        // Insert the max tree guard, so that leaf indices are assigned correctly for further keys.
        // The min guard is not inserted because `insert_initial_writes()` starts enum indices from 1.
        // TODO: move to `insert_genesis_batch()` (also make genesis batch root hash computations dependent on the tree type)
        conn.storage_logs_dedup_dal()
            .insert_initial_writes(L1BatchNumber(0), &[H256::repeat_byte(0xff)])
            .await
            .unwrap();

        insert_genesis_batch(&mut conn, &GenesisParamsInitials::mock())
            .await
            .unwrap();
    }

    let config = TreeManagerConfig {
        db_path: db_path.to_owned(),
        max_open_files: None,
        delay_interval: Duration::from_millis(10),
        max_l1_batches_per_iter: NonZeroUsize::new(5).unwrap(),
        multi_get_chunk_size: 500,
        block_cache_capacity: 16 << 20,
        include_indices_and_filters_in_block_cache: false,
    };
    TreeManager::new(config, pool)
}

async fn insert_l1_batch(conn: &mut Connection<'_, Core>, storage_logs: &[StorageLog]) {
    let mut conn = conn.start_transaction().await.unwrap();
    let l1_batch_number = conn
        .blocks_dal()
        .get_sealed_l1_batch_number()
        .await
        .unwrap()
        .expect("no batches in storage");
    let l1_batch_header = create_l1_batch(l1_batch_number.0 + 1);
    let block_header = create_l2_block(l1_batch_number.0 + 1);

    conn.blocks_dal()
        .insert_mock_l1_batch(&l1_batch_header)
        .await
        .unwrap();
    conn.blocks_dal()
        .insert_l2_block(&block_header)
        .await
        .unwrap();
    conn.storage_logs_dal()
        .insert_storage_logs(block_header.number, storage_logs)
        .await
        .unwrap();
    conn.blocks_dal()
        .mark_l2_blocks_as_executed_in_l1_batch(l1_batch_header.number)
        .await
        .unwrap();
    insert_initial_writes_for_batch(&mut conn, l1_batch_header.number).await;
    conn.commit().await.unwrap();
}

async fn expected_tree_hash(conn: &mut Connection<'_, Core>) -> H256 {
    let processed_l1_batch_number = conn
        .blocks_dal()
        .get_sealed_l1_batch_number()
        .await
        .unwrap()
        .expect("No L1 batches in Postgres");

    let mut in_memory_tree = MerkleTree::new(PatchSet::default()).unwrap();
    let mut root_hash = MerkleTree::<PatchSet>::empty_tree_hash();
    for i in 0..=processed_l1_batch_number.0 {
        let logs = L1BatchWithLogs::new(conn, L1BatchNumber(i))
            .await
            .unwrap()
            .expect("no L1 batch");
        root_hash = in_memory_tree
            .extend_with_reference(&logs.tree_logs)
            .unwrap()
            .root_hash;
    }
    root_hash
}

#[tokio::test]
async fn genesis_creation() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let temp_dir = TempDir::new().expect("failed to get temporary directory for RocksDB");

    let tree_manager = setup_tree_manager(temp_dir.path(), pool.clone()).await;
    let tree_reader = tree_manager.tree_reader();
    let mut batches_subscriber = tree_manager.subscribe_to_l1_batches();
    let (stop_sender, stop_receiver) = watch::channel(false);
    let manager_task = tokio::spawn(tree_manager.run(stop_receiver));

    // Wait until the genesis batch is processed.
    batches_subscriber
        .wait_for(|&batch| batch == L1BatchNumber(1))
        .await
        .unwrap();
    let tree_reader = tree_reader.wait().await.unwrap();
    let tree_info = tree_reader.info().await.unwrap();
    assert_eq!(tree_info.min_version, Some(0));
    assert_eq!(tree_info.next_version, 1);
    assert!(tree_info.leaf_count > 10);

    // Check that the manager is responsive to stop requests.
    stop_sender.send_replace(true);
    manager_task.await.unwrap().unwrap();

    // Check that the tree manager can be restarted.
    let tree_manager = setup_tree_manager(temp_dir.path(), pool.clone()).await;
    let tree_reader = tree_manager.tree_reader();
    let (_stop_sender, stop_receiver) = watch::channel(false);
    tokio::spawn(tree_manager.run(stop_receiver));

    let tree_reader = tree_reader.wait().await.unwrap();
    let tree_info = tree_reader.clone().info().await.unwrap();
    assert_eq!(tree_info.next_version, 1);
    assert!(tree_info.leaf_count > 10);

    let mut conn = pool.connection().await.unwrap();
    assert_leaf_indices(&mut conn, tree_reader, &tree_info).await;
}

/// Checks that leaf indices are assigned identically in the tree and Postgres.
async fn assert_leaf_indices(
    conn: &mut Connection<'_, Core>,
    tree_reader: AsyncTreeReader,
    current_tree_info: &MerkleTreeInfo,
) {
    let all_initial_writes = conn
        .storage_logs_dedup_dal()
        .dump_all_initial_writes_for_tests()
        .await;
    let max_l1_batch = all_initial_writes
        .iter()
        .map(|write| write.l1_batch_number)
        .max()
        .expect("no initial writes");
    let keys_to_indices: HashMap<_, _> = all_initial_writes
        .into_iter()
        .map(|write| (write.hashed_key, write.index))
        .collect();
    let keys: Vec<_> = keys_to_indices.keys().copied().collect();

    let proof = tree_reader
        .prove(max_l1_batch.0.into(), keys.clone())
        .await
        .unwrap();
    assert!(proof.operations.is_empty());
    for (op, key) in proof.read_operations.iter().zip(&keys) {
        let index = keys_to_indices[key];
        assert_eq!(*op, TreeOperation::Hit { index });
    }

    let tree_output = BatchOutput {
        root_hash: current_tree_info.root_hash,
        leaf_count: current_tree_info.leaf_count,
    };
    proof
        .verify_reads(
            &Blake2Hasher,
            <DefaultTreeParams>::TREE_DEPTH,
            tree_output,
            &keys,
        )
        .unwrap();
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn basic_workflow(start_after_batch: bool) {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let temp_dir = TempDir::new().expect("failed to get temporary directory for RocksDB");

    let tree_manager = setup_tree_manager(temp_dir.path(), pool.clone()).await;
    let health_check = tree_manager.tree_health_check();
    assert_matches!(
        health_check.check_health().await.status(),
        HealthStatus::NotReady
    );

    let tree_reader = tree_manager.tree_reader();
    let mut batches_subscriber = tree_manager.subscribe_to_l1_batches();
    let (stop_sender, stop_receiver) = watch::channel(false);
    let barrier = Arc::new(Barrier::new(2));
    let manager_barrier = barrier.clone();
    let manager_task = tokio::spawn(async move {
        manager_barrier.wait().await;
        tree_manager.run(stop_receiver).await
    });

    if !start_after_batch {
        barrier.wait().await;
    }
    let storage_logs = generate_storage_logs(100..200);

    let mut conn = pool.connection().await.unwrap();
    insert_l1_batch(&mut conn, &storage_logs).await;
    if start_after_batch {
        tokio::time::sleep(Duration::from_millis(50)).await;
        barrier.wait().await;
    }

    // Check the hash against the reference.
    batches_subscriber
        .wait_for(|&batch| batch == L1BatchNumber(2))
        .await
        .unwrap();
    let health = health_check.check_health().await;
    assert_matches!(health.status(), HealthStatus::Ready);
    assert!(
        health.details().unwrap()["root_hash"].is_string(),
        "{health:?}"
    );

    let tree_reader = tree_reader.wait().await.unwrap();
    let tree_info = tree_reader.clone().info().await.unwrap();
    assert_eq!(tree_info.next_version, 2);
    let expected_tree_hash = expected_tree_hash(&mut conn).await;
    assert_eq!(tree_info.root_hash, expected_tree_hash);
    assert!(tree_info.leaf_count > 100, "{tree_info:?}");

    assert_leaf_indices(&mut conn, tree_reader, &tree_info).await;

    // Check that the hash is persisted in Postgres as well.
    let tree_data = conn
        .blocks_dal()
        .get_l1_batch_tree_data(L1BatchNumber(1))
        .await
        .unwrap()
        .expect("no tree data");
    assert_eq!(tree_data.hash, expected_tree_hash);
    assert_eq!(tree_data.rollup_last_leaf_index, tree_info.leaf_count + 1);

    stop_sender.send_replace(true);
    manager_task.await.unwrap().unwrap();
    assert_matches!(
        health_check.check_health().await.status(),
        HealthStatus::ShutDown
    );
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn workflow_with_multiple_batches(sleep_between_batches: bool) {
    const BATCH_COUNT: usize = 5;

    let pool = ConnectionPool::<Core>::test_pool().await;
    let temp_dir = TempDir::new().expect("failed to get temporary directory for RocksDB");

    let tree_manager = setup_tree_manager(temp_dir.path(), pool.clone()).await;
    let tree_reader = tree_manager.tree_reader();
    let mut batches_subscriber = tree_manager.subscribe_to_l1_batches();
    let (_stop_sender, stop_receiver) = watch::channel(false);
    tokio::spawn(tree_manager.run(stop_receiver));

    let mut conn = pool.connection().await.unwrap();
    let all_storage_logs = generate_storage_logs(100..200);
    for storage_logs in all_storage_logs.chunks(all_storage_logs.len() / BATCH_COUNT) {
        insert_l1_batch(&mut conn, storage_logs).await;
        if sleep_between_batches {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    batches_subscriber
        .wait_for(|&batch| batch == L1BatchNumber(BATCH_COUNT as u32) + 1)
        .await
        .unwrap();

    let tree_reader = tree_reader.wait().await.unwrap();
    let tree_info = tree_reader.clone().info().await.unwrap();
    assert_eq!(tree_info.next_version, BATCH_COUNT as u64 + 1);
    let expected_tree_hash = expected_tree_hash(&mut conn).await;
    assert_eq!(tree_info.root_hash, expected_tree_hash);

    assert_leaf_indices(&mut conn, tree_reader, &tree_info).await;
}

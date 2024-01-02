//! Various helpers for the metadata calculator.

use std::{
    collections::BTreeMap,
    future::Future,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};
#[cfg(test)]
use tokio::sync::mpsc;
use zksync_config::configs::database::MerkleTreeMode;
use zksync_dal::StorageProcessor;
use zksync_merkle_tree::{
    domain::{TreeMetadata, ZkSyncTree, ZkSyncTreeReader},
    recovery::MerkleTreeRecovery,
    Database, Key, NoVersionError, RocksDBWrapper, TreeEntry, TreeEntryWithProof, TreeInstruction,
};
use zksync_storage::{RocksDB, RocksDBOptions, StalledWritesRetries};
use zksync_types::{block::L1BatchHeader, L1BatchNumber, StorageKey, H256};

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct L1BatchWithLogs {
    pub header: L1BatchHeader,
    pub storage_logs: Vec<TreeInstruction<StorageKey>>,
}

impl L1BatchWithLogs {
    pub async fn new(
        storage: &mut StorageProcessor<'_>,
        l1_batch_number: L1BatchNumber,
    ) -> Option<Self> {
        tracing::debug!("Loading storage logs data for L1 batch #{l1_batch_number}");

        let header = storage
            .blocks_dal()
            .get_l1_batch_header(l1_batch_number)
            .await
            .unwrap()?;

        let protective_reads = storage
            .storage_logs_dedup_dal()
            .get_protective_reads_for_l1_batch(l1_batch_number)
            .await;

        let mut touched_slots = storage
            .storage_logs_dal()
            .get_touched_slots_for_l1_batch(l1_batch_number)
            .await;

        let hashed_keys_for_writes: Vec<_> =
            touched_slots.keys().map(StorageKey::hashed_key).collect();
        let l1_batches_for_initial_writes = storage
            .storage_logs_dal()
            .get_l1_batches_and_indices_for_initial_writes(&hashed_keys_for_writes)
            .await;

        let mut storage_logs = BTreeMap::new();
        for storage_key in protective_reads {
            touched_slots.remove(&storage_key);
            // ^ As per deduplication rules, all keys in `protective_reads` haven't *really* changed
            // in the considered L1 batch. Thus, we can remove them from `touched_slots` in order to simplify
            // their further processing.
            let log = TreeInstruction::Read(storage_key);
            storage_logs.insert(storage_key, log);
        }
        tracing::debug!(
            "Made touched slots disjoint with protective reads; remaining touched slots: {}",
            touched_slots.len()
        );

        for (storage_key, value) in touched_slots {
            if let Some(&(initial_write_batch_for_key, leaf_index)) =
                l1_batches_for_initial_writes.get(&storage_key.hashed_key())
            {
                // Filter out logs that correspond to deduplicated writes.
                if initial_write_batch_for_key <= l1_batch_number {
                    storage_logs.insert(
                        storage_key,
                        TreeInstruction::write(storage_key, leaf_index, value),
                    );
                }
            }
        }

        Some(Self {
            header,
            storage_logs: storage_logs.into_values().collect(),
        })
    }
}

#[cfg(dupa)]
#[cfg(test)]
mod tests {
    use tempfile::TempDir;
    use zksync_dal::ConnectionPool;
    use zksync_types::{proofs::PrepareBasicCircuitsJob, L2ChainId, StorageKey, StorageLog};

    use super::*;
    use crate::{
        genesis::{ensure_genesis_state, GenesisParams},
        metadata_calculator::tests::{extend_db_state, gen_storage_logs, reset_db_state},
    };

    impl L1BatchWithLogs {
        /// Old, slower method of loading storage logs. We want to test its equivalence to the new implementation.
        async fn slow(
            storage: &mut StorageProcessor<'_>,
            l1_batch_number: L1BatchNumber,
        ) -> Option<Self> {
            let header = storage
                .blocks_dal()
                .get_l1_batch_header(l1_batch_number)
                .await
                .unwrap()?;
            let protective_reads = storage
                .storage_logs_dedup_dal()
                .get_protective_reads_for_l1_batch(l1_batch_number)
                .await;
            let touched_slots = storage
                .storage_logs_dal()
                .get_touched_slots_for_l1_batch(l1_batch_number)
                .await;

            let mut storage_logs = BTreeMap::new();

            let hashed_keys: Vec<_> = protective_reads
                .iter()
                .chain(touched_slots.keys())
                .map(StorageKey::hashed_key)
                .collect();
            let previous_values = storage
                .storage_logs_dal()
                .get_previous_storage_values(&hashed_keys, l1_batch_number)
                .await;
            let l1_batches_for_initial_writes = storage
                .storage_logs_dal()
                .get_l1_batches_and_indices_for_initial_writes(&hashed_keys)
                .await;

            for storage_key in protective_reads {
                let previous_value = previous_values[&storage_key.hashed_key()].unwrap_or_default();
                // Sanity check: value must not change for slots that require protective reads.
                if let Some(value) = touched_slots.get(&storage_key) {
                    assert_eq!(
                        previous_value, *value,
                        "Value was changed for slot that requires protective read"
                    );
                }

                storage_logs.insert(storage_key, TreeInstruction::Read(storage_key));
            }

            for (storage_key, value) in touched_slots {
                let previous_value = previous_values[&storage_key.hashed_key()].unwrap_or_default();
                if previous_value != value {
                    let (_, leaf_index) = l1_batches_for_initial_writes[&storage_key.hashed_key()];
                    storage_logs.insert(
                        storage_key,
                        TreeInstruction::write(storage_key, leaf_index, value),
                    );
                }
            }

            Some(Self {
                header,
                storage_logs: storage_logs.into_values().collect(),
            })
        }
    }

    #[tokio::test]
    async fn loaded_logs_equivalence_basics() {
        let pool = ConnectionPool::test_pool().await;
        ensure_genesis_state(
            &mut pool.access_storage().await.unwrap(),
            L2ChainId::from(270),
            &GenesisParams::mock(),
        )
        .await
        .unwrap();
        reset_db_state(&pool, 5).await;

        let mut storage = pool.access_storage().await.unwrap();
        for l1_batch_number in 0..=5 {
            let l1_batch_number = L1BatchNumber(l1_batch_number);
            let batch_with_logs = L1BatchWithLogs::new(&mut storage, l1_batch_number)
                .await
                .unwrap();
            let slow_batch_with_logs = L1BatchWithLogs::slow(&mut storage, l1_batch_number)
                .await
                .unwrap();
            assert_eq!(batch_with_logs, slow_batch_with_logs);
        }
    }

    #[tokio::test]
    async fn loaded_logs_equivalence_with_zero_no_op_logs() {
        let pool = ConnectionPool::test_pool().await;
        let mut storage = pool.access_storage().await.unwrap();
        ensure_genesis_state(&mut storage, L2ChainId::from(270), &GenesisParams::mock())
            .await
            .unwrap();

        let mut logs = gen_storage_logs(100..200, 2);
        for log in &mut logs[0] {
            log.value = H256::zero();
        }
        for log in logs[1].iter_mut().step_by(3) {
            log.value = H256::zero();
        }
        extend_db_state(&mut storage, logs).await;

        let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
        let mut tree = create_tree(&temp_dir).await;
        for number in 0..3 {
            assert_log_equivalence(&mut storage, &mut tree, L1BatchNumber(number)).await;
        }
    }

    async fn create_tree(temp_dir: &TempDir) -> AsyncTree {
        let db = create_db(
            temp_dir.path().to_owned(),
            0,
            16 << 20,       // 16 MiB,
            Duration::ZERO, // writes should never be stalled in tests
            500,
        )
        .await;
        AsyncTree::new(db, MerkleTreeMode::Full)
    }

    async fn assert_log_equivalence(
        storage: &mut StorageProcessor<'_>,
        tree: &mut AsyncTree,
        l1_batch_number: L1BatchNumber,
    ) {
        let l1_batch_with_logs = L1BatchWithLogs::new(storage, l1_batch_number)
            .await
            .unwrap();
        let slow_l1_batch_with_logs = L1BatchWithLogs::slow(storage, l1_batch_number)
            .await
            .unwrap();

        // Sanity check: L1 batch headers must be identical
        assert_eq!(l1_batch_with_logs.header, slow_l1_batch_with_logs.header);

        tree.save().await; // Necessary for `reset()` below to work properly
        let tree_metadata = tree.process_l1_batch(l1_batch_with_logs.storage_logs).await;
        tree.as_mut().reset();
        let slow_tree_metadata = tree
            .process_l1_batch(slow_l1_batch_with_logs.storage_logs)
            .await;
        assert_eq!(tree_metadata.root_hash, slow_tree_metadata.root_hash);
        assert_eq!(
            tree_metadata.rollup_last_leaf_index,
            slow_tree_metadata.rollup_last_leaf_index
        );
        assert_eq!(
            tree_metadata.initial_writes,
            slow_tree_metadata.initial_writes
        );
        assert_eq!(
            tree_metadata.initial_writes,
            slow_tree_metadata.initial_writes
        );
        assert_eq!(
            tree_metadata.repeated_writes,
            slow_tree_metadata.repeated_writes
        );
        assert_equivalent_witnesses(
            tree_metadata.witness.unwrap(),
            slow_tree_metadata.witness.unwrap(),
        );
    }

    fn assert_equivalent_witnesses(lhs: PrepareBasicCircuitsJob, rhs: PrepareBasicCircuitsJob) {
        assert_eq!(lhs.next_enumeration_index(), rhs.next_enumeration_index());
        let lhs_paths = lhs.into_merkle_paths();
        let rhs_paths = rhs.into_merkle_paths();
        assert_eq!(lhs_paths.len(), rhs_paths.len());
        for (lhs_path, rhs_path) in lhs_paths.zip(rhs_paths) {
            assert_eq!(lhs_path, rhs_path);
        }
    }

    #[tokio::test]
    async fn loaded_logs_equivalence_with_non_zero_no_op_logs() {
        let pool = ConnectionPool::test_pool().await;
        let mut storage = pool.access_storage().await.unwrap();
        ensure_genesis_state(&mut storage, L2ChainId::from(270), &GenesisParams::mock())
            .await
            .unwrap();

        let mut logs = gen_storage_logs(100..120, 1);
        // Entire batch of no-op logs (writing previous values).
        let copied_logs = logs[0].clone();
        logs.push(copied_logs);

        // Batch of effectively no-op logs (overwriting values, then writing old values back).
        let mut updated_and_then_copied_logs: Vec<_> = logs[0]
            .iter()
            .map(|log| StorageLog {
                value: H256::repeat_byte(0xff),
                ..*log
            })
            .collect();
        updated_and_then_copied_logs.extend_from_slice(&logs[0]);
        logs.push(updated_and_then_copied_logs);

        // Batch where half of logs are copied and the other half is writing zero values (which is
        // not a no-op).
        let mut partially_copied_logs = logs[0].clone();
        for log in partially_copied_logs.iter_mut().step_by(2) {
            log.value = H256::zero();
        }
        logs.push(partially_copied_logs);

        // Batch where 2/3 of logs are copied and the other 1/3 is writing new non-zero values.
        let mut partially_copied_logs = logs[0].clone();
        for log in partially_copied_logs.iter_mut().step_by(3) {
            log.value = H256::repeat_byte(0x11);
        }
        logs.push(partially_copied_logs);
        extend_db_state(&mut storage, logs).await;

        let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
        let mut tree = create_tree(&temp_dir).await;
        for batch_number in 0..5 {
            assert_log_equivalence(&mut storage, &mut tree, L1BatchNumber(batch_number)).await;
        }
    }

    #[tokio::test]
    async fn loaded_logs_equivalence_with_protective_reads() {
        let pool = ConnectionPool::test_pool().await;
        let mut storage = pool.access_storage().await.unwrap();
        ensure_genesis_state(&mut storage, L2ChainId::from(270), &GenesisParams::mock())
            .await
            .unwrap();

        let mut logs = gen_storage_logs(100..120, 1);
        let logs_copy = logs[0].clone();
        logs.push(logs_copy);
        let read_logs: Vec<_> = logs[1]
            .iter()
            .step_by(3)
            .map(StorageLog::to_test_log_query)
            .collect();
        extend_db_state(&mut storage, logs).await;
        storage
            .storage_logs_dedup_dal()
            .insert_protective_reads(L1BatchNumber(2), &read_logs)
            .await;

        let l1_batch_with_logs = L1BatchWithLogs::new(&mut storage, L1BatchNumber(2))
            .await
            .unwrap();
        // Check that we have protective reads transformed into read logs
        let read_logs_count = l1_batch_with_logs
            .storage_logs
            .iter()
            .filter(|log| matches!(log, TreeInstruction::Read(_)))
            .count();
        assert_eq!(read_logs_count, 7);

        let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
        let mut tree = create_tree(&temp_dir).await;
        for batch_number in 0..3 {
            assert_log_equivalence(&mut storage, &mut tree, L1BatchNumber(batch_number)).await;
        }
    }
}

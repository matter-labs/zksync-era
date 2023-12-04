//! High-level recovery logic for the Merkle tree.

use anyhow::Context as _;
use futures::future;
use tokio::sync::{watch, Mutex};

use std::ops;

use zksync_dal::{storage_logs_dal::StorageTreeEntry, ConnectionPool};
use zksync_health_check::HealthUpdater;
use zksync_merkle_tree::TreeEntry;
use zksync_types::{L1BatchNumber, MiniblockNumber, H256, U256};
use zksync_utils::u256_to_h256;

use super::helpers::{AsyncTree, AsyncTreeRecovery, GenericAsyncTree};

impl GenericAsyncTree {
    pub async fn ensure_ready(
        self,
        pool: &ConnectionPool,
        stop_receiver: &watch::Receiver<bool>,
        _health_updater: &HealthUpdater,
    ) -> anyhow::Result<AsyncTree> {
        let (tree, l1_batch) = match self {
            Self::Ready(tree) => return Ok(tree),
            Self::Recovering(tree) => {
                let l1_batch = snapshot_l1_batch(pool).await?.context(
                    "Merkle tree is recovering, but Postgres doesn't contain snapshot L1 batch",
                )?;
                let recovered_version = tree.recovered_version();
                anyhow::ensure!(
                    u64::from(l1_batch.0) == recovered_version,
                    "Snapshot L1 batch in Postgres ({l1_batch}) differs from the recovered Merkle tree version \
                     ({recovered_version})"
                );
                tracing::info!("Resuming tree recovery with snapshot L1 batch #{l1_batch}");
                (tree, l1_batch)
            }
            Self::Empty { db, mode } => {
                if let Some(l1_batch) = snapshot_l1_batch(pool).await? {
                    tracing::info!(
                        "Starting Merkle tree recovery with snapshot L1 batch #{l1_batch}"
                    );
                    let tree = AsyncTreeRecovery::new(db, l1_batch.0.into(), mode);
                    (tree, l1_batch)
                } else {
                    // Start the tree from scratch. The genesis block will be filled in `TreeUpdater::loop_updating_tree()`.
                    return Ok(AsyncTree::new(db, mode));
                }
            }
        };

        // FIXME: make choice of ranges more intelligent. NB: ranges must be the same for the entire recovery!
        let chunk_count = 256;
        tree.recover(l1_batch, chunk_count, pool, stop_receiver)
            .await
    }
}

impl AsyncTreeRecovery {
    async fn recover(
        self,
        l1_batch: L1BatchNumber,
        chunk_count: usize,
        pool: &ConnectionPool,
        stop_receiver: &watch::Receiver<bool>,
    ) -> anyhow::Result<AsyncTree> {
        let mut storage = pool.access_storage().await?;
        let (_, snapshot_miniblock) = storage
            .blocks_dal()
            .get_miniblock_range_of_l1_batch(l1_batch)
            .await
            .with_context(|| format!("Failed getting miniblock range for L1 batch #{l1_batch}"))?
            .with_context(|| format!("L1 batch #{l1_batch} doesn't have miniblocks"))?;
        let expected_root_hash = storage
            .blocks_dal()
            .get_l1_batch_metadata(l1_batch)
            .await
            .with_context(|| format!("Failed getting metadata for L1 batch #{l1_batch}"))?
            .with_context(|| format!("L1 batch #{l1_batch} has no metadata"))?
            .metadata
            .root_hash;
        drop(storage);

        let chunks = Self::hashed_key_ranges(chunk_count);
        let tree = Mutex::new(self);

        let chunl_tasks = chunks.map(|chunk| {
            Self::recover_key_chunk(&tree, snapshot_miniblock, chunk, pool, stop_receiver)
        });
        future::try_join_all(chunl_tasks).await?;

        let mut tree = tree.into_inner();
        let actual_root_hash = tree.root_hash().await;
        anyhow::ensure!(
            actual_root_hash == expected_root_hash,
            "Root hash of recovered tree {actual_root_hash:?} differs from expected root hash {expected_root_hash:?}"
        );
        tracing::info!("Finished tree recovery; resuming normal tree operation");
        Ok(tree.finalize().await)
    }

    fn hashed_key_ranges(count: usize) -> impl Iterator<Item = ops::RangeInclusive<H256>> {
        assert!(count > 0);
        let mut stride = U256::MAX / count;
        let stride_minus_one = if stride < U256::MAX {
            stride += U256::one();
            stride - 1
        } else {
            stride // `stride` is really 1 << 256 == U256::MAX + 1
        };

        (0..count).map(move |i| {
            let start = stride * i;
            let (mut end, is_overflow) = stride_minus_one.overflowing_add(start);
            if is_overflow {
                end = U256::MAX;
            }
            u256_to_h256(start)..=u256_to_h256(end)
        })
    }

    async fn recover_key_chunk(
        tree: &Mutex<AsyncTreeRecovery>,
        snapshot_miniblock: MiniblockNumber,
        key_chunk: ops::RangeInclusive<H256>,
        pool: &ConnectionPool,
        stop_receiver: &watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        let mut storage = pool.access_storage().await?;
        let first_entry = storage
            .storage_logs_dal()
            .get_tree_entries_for_miniblock(snapshot_miniblock, key_chunk.clone(), Some(1))
            .await
            .with_context(|| {
                format!("Failed getting first entry for chunk {key_chunk:?} in snapshot for miniblock #{snapshot_miniblock}")
            })?;
        let Some(first_entry) = first_entry.get(0) else {
            tracing::debug!(
                "Key chunk {key_chunk:?} has no entries in Postgres; skipping recovery"
            );
            return Ok(());
        };
        let first_entry = map_tree_entry(first_entry)?;

        // FIXME: inefficient – tree may be locked doing writes
        if let Some(existing_entry) = tree.lock().await.entry(first_entry.key).await {
            anyhow::ensure!(
                existing_entry.value == first_entry.value && existing_entry.leaf_index == first_entry.leaf_index,
                "Mismatch between entry for key {:0>64x} in Postgres snapshot for miniblock #{snapshot_miniblock} \
                 ({first_entry:?}) and tree ({existing_entry:?}); the recovery procedure may be corrupted",
                first_entry.key
            );
            tracing::debug!("Key chunk {key_chunk:?} is already recovered");
            return Ok(());
        }

        if *stop_receiver.borrow() {
            return Ok(());
        }

        // FIXME: metrics
        let all_entries = storage
            .storage_logs_dal()
            .get_tree_entries_for_miniblock(snapshot_miniblock, key_chunk.clone(), None)
            .await
            .with_context(|| {
                format!("Failed getting entries for chunk {key_chunk:?} in snapshot for miniblock #{snapshot_miniblock}")
            })?;
        drop(storage);
        if *stop_receiver.borrow() {
            return Ok(());
        }

        let all_entries = all_entries
            .iter()
            .map(map_tree_entry)
            .collect::<anyhow::Result<_>>()?;
        let mut tree = tree.lock().await;
        if *stop_receiver.borrow() {
            return Ok(());
        }

        tree.extend(all_entries).await;
        Ok(())
    }
}

async fn snapshot_l1_batch(_pool: &ConnectionPool) -> anyhow::Result<Option<L1BatchNumber>> {
    Ok(None)
}

fn map_tree_entry(src: &StorageTreeEntry) -> anyhow::Result<TreeEntry> {
    anyhow::ensure!(
        src.hashed_key.len() == 32,
        "Invalid StorageTreeEntry.hashed_key"
    );
    anyhow::ensure!(src.value.len() == 32, "Invalid StorageTreeEntry.value");
    Ok(TreeEntry {
        key: U256::from_little_endian(&src.hashed_key),
        value: H256::from_slice(&src.value),
        leaf_index: src.index as u64,
    })
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;
    use test_casing::test_casing;

    use std::{path::PathBuf, time::Duration};

    use zksync_config::configs::database::MerkleTreeMode;
    use zksync_types::{L2ChainId, StorageLog};
    use zksync_utils::h256_to_u256;

    use super::*;
    use crate::{
        genesis::{ensure_genesis_state, GenesisParams},
        metadata_calculator::{
            helpers::create_db,
            tests::{extend_db_state, gen_storage_logs, run_calculator, setup_calculator},
        },
    };

    #[test]
    fn calculating_hashed_key_ranges_with_single_chunk() {
        let mut ranges = AsyncTreeRecovery::hashed_key_ranges(1);
        let full_range = ranges.next().unwrap();
        assert_eq!(full_range, H256::zero()..=H256([0xff; 32]));
    }

    #[test]
    fn calculating_hashed_key_ranges_for_256_chunks() {
        let ranges = AsyncTreeRecovery::hashed_key_ranges(256);
        let mut start = H256::zero();
        let mut end = H256([0xff; 32]);

        for (i, range) in ranges.enumerate() {
            let i = u8::try_from(i).unwrap();
            start.0[0] = i;
            end.0[0] = i;
            assert_eq!(range, start..=end);
        }
    }

    #[test_casing(5, [3, 7, 23, 100, 255])]
    fn calculating_hashed_key_ranges_for_arbitrary_chunks(chunk_count: usize) {
        let ranges: Vec<_> = AsyncTreeRecovery::hashed_key_ranges(chunk_count).collect();
        assert_eq!(ranges.len(), chunk_count);

        for window in ranges.windows(2) {
            let [prev_range, range] = window else {
                unreachable!();
            };
            assert_eq!(
                h256_to_u256(*range.start()),
                h256_to_u256(*prev_range.end()) + 1
            );
        }
        assert_eq!(*ranges.first().unwrap().start(), H256::zero());
        assert_eq!(*ranges.last().unwrap().end(), H256([0xff; 32]));
    }

    async fn create_tree_recovery(path: PathBuf, l1_batch: L1BatchNumber) -> AsyncTreeRecovery {
        let db = create_db(
            path,
            0,
            16 << 20,       // 16 MiB,
            Duration::ZERO, // writes should never be stalled in tests
            500,
        )
        .await;
        AsyncTreeRecovery::new(db, l1_batch.0.into(), MerkleTreeMode::Full)
    }

    #[tokio::test]
    async fn basic_recovery_workflow() {
        let pool = ConnectionPool::test_pool().await;
        let mut storage = pool.access_storage().await.unwrap();
        ensure_genesis_state(&mut storage, L2ChainId::from(270), &GenesisParams::mock())
            .await
            .unwrap();
        let mut logs = gen_storage_logs(100..300, 1).pop().unwrap();

        // Add all logs from the genesis L1 batch to `logs` so that they cover all state keys.
        let genesis_logs = storage
            .storage_logs_dal()
            .get_touched_slots_for_l1_batch(L1BatchNumber(0))
            .await;
        let genesis_logs = genesis_logs
            .into_iter()
            .map(|(key, value)| StorageLog::new_write_log(key, value));
        logs.extend(genesis_logs);
        extend_db_state(&mut storage, vec![logs]).await;

        let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
        // Ensure that metadata for L1 batch #1 is present in the DB.
        let (calculator, _) = setup_calculator(&temp_dir.path().join("init"), &pool).await;
        let prover_pool = ConnectionPool::test_pool().await;
        let root_hash = run_calculator(calculator, pool.clone(), prover_pool).await;

        let (_stop_sender, stop_receiver) = watch::channel(false);
        for chunk_count in [1, 4, 9, 16, 60, 256] {
            println!("Recovering tree with {chunk_count} chunks");
            let tree_path = temp_dir.path().join(format!("recovery-{chunk_count}"));
            let tree = create_tree_recovery(tree_path, L1BatchNumber(1)).await;
            let tree = tree
                .recover(L1BatchNumber(1), chunk_count, &pool, &stop_receiver)
                .await
                .unwrap();
            assert_eq!(tree.root_hash(), root_hash);
        }
    }
}

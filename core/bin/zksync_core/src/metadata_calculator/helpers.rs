//! Various helpers for the metadata calculator.

#[cfg(test)]
use tokio::sync::mpsc;

use std::{collections::BTreeMap, future::Future, mem, time::Duration};

use zksync_dal::StorageProcessor;
use zksync_merkle_tree::domain::{TreeMetadata, ZkSyncTree};
use zksync_types::{
    block::WitnessBlockWithLogs, L1BatchNumber, StorageKey, StorageLog, WitnessStorageLog, H256,
};

/// Wrapper around the "main" tree implementation used by [`MetadataCalculator`].
///
/// Async methods provided by this wrapper are not cancel-safe! This is probably not an issue;
/// `ZkSyncTree` is only indirectly available via `MetadataCalculator::run()` entrypoint
/// which consumes `self`. That is, if `MetadataCalculator::run()` is cancelled (which we don't currently do,
/// at least not explicitly), all `MetadataCalculator` data including `ZkSyncTree` is discarded.
/// In the unlikely case you get a "`ZkSyncTree` is in inconsistent state" panic,
/// cancellation is most probably the reason.
#[derive(Debug, Default)]
pub(super) struct AsyncTree(Option<ZkSyncTree>);

impl AsyncTree {
    const INCONSISTENT_MSG: &'static str =
        "`ZkSyncTree` is in inconsistent state, which could occur after one of its blocking futures was cancelled";

    pub fn new(tree: ZkSyncTree) -> Self {
        Self(Some(tree))
    }

    fn as_ref(&self) -> &ZkSyncTree {
        self.0.as_ref().expect(Self::INCONSISTENT_MSG)
    }

    fn as_mut(&mut self) -> &mut ZkSyncTree {
        self.0.as_mut().expect(Self::INCONSISTENT_MSG)
    }

    pub fn is_empty(&self) -> bool {
        self.as_ref().is_empty()
    }

    pub fn block_number(&self) -> u32 {
        self.as_ref().block_number()
    }

    pub fn root_hash(&self) -> H256 {
        self.as_ref().root_hash()
    }

    pub async fn process_block(&mut self, block: Vec<WitnessStorageLog>) -> TreeMetadata {
        let mut tree = mem::take(self);
        let (tree, metadata) = tokio::task::spawn_blocking(move || {
            let metadata = tree.as_mut().process_block(&block);
            (tree, metadata)
        })
        .await
        .unwrap();

        *self = tree;
        metadata
    }

    pub async fn process_blocks(
        &mut self,
        blocks: Vec<Vec<WitnessStorageLog>>,
    ) -> Vec<TreeMetadata> {
        let mut tree = mem::take(self);
        let (tree, metadata) = tokio::task::spawn_blocking(move || {
            tree.as_mut().reset(); // For compatibility with the old implementation
            let metadata = blocks
                .iter()
                .map(|block| tree.as_mut().process_block(block))
                .collect();
            (tree, metadata)
        })
        .await
        .unwrap();

        *self = tree;
        metadata
    }

    pub async fn save(&mut self) {
        let mut tree = mem::take(self);
        *self = tokio::task::spawn_blocking(|| {
            tree.as_mut().save();
            tree
        })
        .await
        .unwrap();
    }
}

/// Component implementing the delay policy in [`MetadataCalculator`] when there are no
/// blocks to seal.
#[derive(Debug, Clone)]
pub(super) struct Delayer {
    delay_interval: Duration,
    // Notifies the tests about the block count and tree root hash when the calculator
    // runs out of blocks to process. (Since RocksDB is exclusive, we cannot just create
    // another instance to check these params on the test side without stopping the calc.)
    #[cfg(test)]
    pub delay_notifier: mpsc::UnboundedSender<(u32, H256)>,
}

impl Delayer {
    pub fn new(delay_interval: Duration) -> Self {
        Self {
            delay_interval,
            #[cfg(test)]
            delay_notifier: mpsc::unbounded_channel().0,
        }
    }

    #[cfg_attr(not(test), allow(unused))] // `tree` is only used in test mode
    pub fn wait(&self, tree: &AsyncTree) -> impl Future<Output = ()> {
        #[cfg(test)]
        self.delay_notifier
            .send((tree.block_number(), tree.root_hash()))
            .ok();
        tokio::time::sleep(self.delay_interval)
    }
}

pub(crate) async fn get_logs_for_l1_batch(
    storage: &mut StorageProcessor<'_>,
    l1_batch_number: L1BatchNumber,
) -> Option<WitnessBlockWithLogs> {
    let header = storage
        .blocks_dal()
        .get_block_header(l1_batch_number)
        .await?;

    // `BTreeMap` is used because tree needs to process slots in lexicographical order.
    let mut storage_logs: BTreeMap<StorageKey, WitnessStorageLog> = BTreeMap::new();

    let protective_reads = storage
        .storage_logs_dedup_dal()
        .get_protective_reads_for_l1_batch(l1_batch_number)
        .await;
    let touched_slots = storage
        .storage_logs_dal()
        .get_touched_slots_for_l1_batch(l1_batch_number)
        .await;

    let hashed_keys: Vec<_> = protective_reads
        .iter()
        .chain(touched_slots.keys())
        .map(StorageKey::hashed_key)
        .collect();
    let previous_values = storage
        .storage_logs_dal()
        .get_previous_storage_values(&hashed_keys, l1_batch_number)
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

        storage_logs.insert(
            storage_key,
            WitnessStorageLog {
                storage_log: StorageLog::new_read_log(storage_key, previous_value),
                previous_value,
            },
        );
    }

    for (storage_key, value) in touched_slots {
        let previous_value = previous_values[&storage_key.hashed_key()].unwrap_or_default();
        if previous_value != value {
            storage_logs.insert(
                storage_key,
                WitnessStorageLog {
                    storage_log: StorageLog::new_write_log(storage_key, value),
                    previous_value,
                },
            );
        }
    }

    Some(WitnessBlockWithLogs {
        header,
        storage_logs: storage_logs.into_values().collect(),
    })
}

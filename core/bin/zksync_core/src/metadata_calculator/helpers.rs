//! Various helpers for the metadata calculator.

#[cfg(test)]
use std::sync::mpsc;
use std::{collections::BTreeMap, thread, time::Duration};

use zksync_dal::StorageProcessor;
use zksync_merkle_tree::{TreeMetadata, TreeMode, ZkSyncTree as OldTree};
use zksync_merkle_tree2::domain::{TreeMetadata as NewTreeMetadata, ZkSyncTree as NewTree};
use zksync_types::{
    block::WitnessBlockWithLogs, L1BatchNumber, StorageKey, StorageLog, StorageLogKind,
    WitnessStorageLog, H256,
};

/// Wrapper around the "main" tree implementation used by [`MetadataCalculator`].
#[derive(Debug)]
pub(super) enum ZkSyncTree {
    Old(OldTree),
    New(NewTree),
}

impl ZkSyncTree {
    pub fn map_metadata(new_metadata: NewTreeMetadata) -> TreeMetadata {
        TreeMetadata {
            root_hash: new_metadata.root_hash,
            rollup_last_leaf_index: new_metadata.rollup_last_leaf_index,
            initial_writes: new_metadata.initial_writes,
            repeated_writes: new_metadata.repeated_writes,
            witness_input: new_metadata.witness,
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Self::Old(tree) => tree.is_empty(),
            Self::New(tree) => tree.is_empty(),
        }
    }

    pub fn block_number(&self) -> u32 {
        match self {
            Self::Old(tree) => tree.block_number(),
            Self::New(tree) => tree.block_number(),
        }
    }

    pub fn root_hash(&self) -> H256 {
        match self {
            Self::Old(tree) => tree.root_hash(),
            Self::New(tree) => tree.root_hash(),
        }
    }

    pub fn process_block(&mut self, block: &[WitnessStorageLog]) -> TreeMetadata {
        match self {
            Self::Old(tree) => tree.process_block(block),
            Self::New(tree) => {
                tree.reset(); // For compatibility with the old implementation
                let new_metadata = tree.process_block(block);
                Self::map_metadata(new_metadata)
            }
        }
    }

    pub fn process_blocks<'a>(
        &mut self,
        blocks: impl Iterator<Item = &'a [WitnessStorageLog]>,
    ) -> Vec<TreeMetadata> {
        match self {
            Self::Old(tree) => {
                let mode = tree.mode();
                let blocks = blocks.map(|logs| Self::filter_block_logs(logs, mode));
                tree.process_blocks(blocks)
            }
            Self::New(tree) => {
                tree.reset(); // For compatibility with the old implementation
                blocks
                    .map(|block| Self::map_metadata(tree.process_block(block)))
                    .collect()
            }
        }
    }

    fn filter_block_logs(
        logs: &[WitnessStorageLog],
        mode: TreeMode,
    ) -> impl Iterator<Item = &WitnessStorageLog> + '_ {
        logs.iter().filter(move |log| {
            matches!(mode, TreeMode::Full) || log.storage_log.kind == StorageLogKind::Write
        })
    }

    pub fn save(&mut self) {
        match self {
            Self::Old(tree) => tree.save().expect("failed saving Merkle tree"),
            Self::New(tree) => tree.save(),
        }
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
    pub delay_notifier: mpsc::Sender<(u32, H256)>,
}

impl Delayer {
    pub fn new(delay_interval: Duration) -> Self {
        Self {
            delay_interval,
            #[cfg(test)]
            delay_notifier: mpsc::channel().0,
        }
    }

    #[cfg_attr(not(test), allow(unused))] // `tree` is only used in test mode
    pub fn wait(&self, tree: &ZkSyncTree) {
        #[cfg(test)]
        self.delay_notifier
            .send((tree.block_number(), tree.root_hash()))
            .ok();

        thread::sleep(self.delay_interval);
    }
}

pub(crate) fn get_logs_for_l1_batch(
    storage: &mut StorageProcessor<'_>,
    l1_batch_number: L1BatchNumber,
) -> Option<WitnessBlockWithLogs> {
    let header = storage.blocks_dal().get_block_header(l1_batch_number)?;

    // `BTreeMap` is used because tree needs to process slots in lexicographical order.
    let mut storage_logs: BTreeMap<StorageKey, WitnessStorageLog> = BTreeMap::new();

    let protective_reads = storage
        .storage_logs_dedup_dal()
        .get_protective_reads_for_l1_batch(l1_batch_number);
    let touched_slots = storage
        .storage_logs_dal()
        .get_touched_slots_for_l1_batch(l1_batch_number);

    let hashed_keys = protective_reads
        .iter()
        .chain(touched_slots.keys())
        .map(StorageKey::hashed_key)
        .collect();
    let previous_values = storage
        .storage_logs_dal()
        .get_previous_storage_values(hashed_keys, l1_batch_number);

    for storage_key in protective_reads {
        let previous_value = previous_values[&storage_key.hashed_key()];
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
        let previous_value = previous_values[&storage_key.hashed_key()];
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

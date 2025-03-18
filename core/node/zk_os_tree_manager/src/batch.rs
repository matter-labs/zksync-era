use std::{collections::BTreeMap, time::Duration};

use anyhow::Context;
use zk_os_merkle_tree::TreeEntry;
use zksync_dal::{Connection, Core, CoreDal};
use zksync_types::{
    block::L1BatchTreeData, writes::TreeWrite, AccountTreeId, L1BatchNumber, StorageKey,
};

use crate::helpers::AsyncMerkleTree;

#[derive(Debug)]
pub(crate) struct L1BatchWithLogs {
    /// Updated / inserted tree entries. Insertions must be sorted to align with index assignment.
    pub(crate) tree_logs: Vec<TreeEntry>,
}

impl L1BatchWithLogs {
    pub(crate) async fn new(
        storage: &mut Connection<'_, Core>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<Self>> {
        tracing::debug!("Loading storage logs data for L1 batch #{l1_batch_number}");

        let mut tree_writes = storage
            .blocks_dal()
            .get_tree_writes(l1_batch_number)
            .await?;
        if tree_writes.is_none() && l1_batch_number.0 > 0 {
            // If `tree_writes` are present for the previous L1 batch, then it is expected them to be eventually present for the current batch as well.
            // Waiting for tree writes should be faster than constructing them, so we wait with a reasonable timeout.
            let tree_writes_present_for_previous_batch = storage
                .blocks_dal()
                .check_tree_writes_presence(l1_batch_number - 1)
                .await?;
            if tree_writes_present_for_previous_batch {
                tree_writes = Self::wait_for_tree_writes(storage, l1_batch_number).await?;
            }
        }

        let tree_logs = if let Some(mut tree_writes) = tree_writes {
            // If tree writes are present in DB then simply use them. Sort writes by the enumeration index so that insertions are correctly ordered.
            tree_writes.sort_unstable_by_key(|write| write.leaf_index);
            let writes = tree_writes.into_iter().map(|tree_write| {
                let storage_key =
                    StorageKey::new(AccountTreeId::new(tree_write.address), tree_write.key);
                TreeEntry {
                    key: storage_key.hashed_key(),
                    value: tree_write.value,
                }
            });
            writes.collect()
        } else {
            // Otherwise, load writes' data from other tables.
            Self::extract_logs_from_db(storage, l1_batch_number).await?
        };

        Ok(Some(Self { tree_logs }))
    }

    async fn wait_for_tree_writes(
        connection: &mut Connection<'_, Core>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<Vec<TreeWrite>>> {
        const INTERVAL: Duration = Duration::from_millis(50);
        const TIMEOUT: Duration = Duration::from_secs(5);

        tokio::time::timeout(TIMEOUT, async {
            loop {
                if let Some(tree_writes) = connection
                    .blocks_dal()
                    .get_tree_writes(l1_batch_number)
                    .await?
                {
                    break anyhow::Ok(tree_writes);
                }
                tokio::time::sleep(INTERVAL).await;
            }
        })
        .await
        .ok()
        .transpose()
    }

    // Unlike with the Era tree, we don't need to filter out protective reads here because each L1 batch / VM run
    // is a single block (i.e., a situation in which there are multiple storage logs in a batch for the same storage slot doesn't occur).
    async fn extract_logs_from_db(
        connection: &mut Connection<'_, Core>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Vec<TreeEntry>> {
        let touched_slots = connection
            .storage_logs_dal()
            .get_touched_slots_for_executed_l1_batch(l1_batch_number)
            .await
            .context("cannot fetch touched slots")?;

        let hashed_keys_for_writes: Vec<_> =
            touched_slots.keys().map(StorageKey::hashed_key).collect();
        let l1_batches_for_initial_writes = connection
            .storage_logs_dal()
            .get_l1_batches_and_indices_for_initial_writes(&hashed_keys_for_writes)
            .await
            .context("cannot fetch initial writes batch numbers and indices")?;

        // Sort tree logs by the enumeration index to get the correct order for inserts.
        let mut tree_logs = BTreeMap::new();
        for (storage_key, value) in touched_slots {
            if let Some(&(initial_write_batch_for_key, leaf_index)) =
                l1_batches_for_initial_writes.get(&storage_key.hashed_key())
            {
                // Filter out logs that correspond to deduplicated writes.
                if initial_write_batch_for_key <= l1_batch_number {
                    tree_logs.insert(
                        leaf_index,
                        TreeEntry {
                            key: storage_key.hashed_key(),
                            value,
                        },
                    );
                }
            }
        }

        Ok(tree_logs.into_values().collect())
    }
}

impl AsyncMerkleTree {
    pub(crate) async fn process_l1_batch(
        &mut self,
        batch: L1BatchWithLogs,
    ) -> anyhow::Result<L1BatchTreeData> {
        self.try_invoke_tree(move |tree| {
            let output = tree.extend(&batch.tree_logs)?;
            Ok(L1BatchTreeData {
                hash: output.root_hash,
                rollup_last_leaf_index: output.leaf_count + 1,
            })
        })
        .await
    }
}

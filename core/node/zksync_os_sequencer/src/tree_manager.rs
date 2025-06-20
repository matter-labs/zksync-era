use std::ops::Div;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use anyhow::Context;
use tokio::time::Instant;
use vise::{Histogram, LabeledFamily, EncodeLabelSet, Family, Gauge, Metrics, Unit, Buckets, Counter};
use zk_os_forward_system::run::BatchOutput;
use zksync_storage::RocksDB;
use zksync_storage::rocksdb::Error;
use zksync_types::commitment::ZkosCommitment;
use zksync_types::{ExecuteTransactionCommon, Transaction};
use zksync_zk_os_merkle_tree::{MerkleTree, RocksDBWrapper, TreeEntry};
use zksync_zkos_vm_runner::zkos_conversions::bytes32_to_h256;
use crate::CHAIN_ID;
use crate::l1_sender::L1SenderHandle;
use crate::storage::StateHandle;

const LATENCIES_FAST: Buckets = Buckets::exponential(0.0000001..=1.0, 2.0);

const BLOCK_RANGE_SIZE: Buckets = Buckets::exponential(1.0..=1000.0, 2.0);

#[derive(Debug, Metrics)]
#[metrics(prefix = "tree")]
pub struct TreeMetrics {
    #[metrics(unit = Unit::Seconds, buckets = LATENCIES_FAST)]
    pub entry_time: Histogram<Duration>,
    #[metrics(unit = Unit::Seconds, buckets = LATENCIES_FAST)]
    pub block_time: Histogram<Duration>,
    #[metrics(unit = Unit::Seconds, buckets = LATENCIES_FAST)]
    pub range_time: Histogram<Duration>,
    #[metrics(buckets = BLOCK_RANGE_SIZE)]
    pub processing_range: Histogram<u64>,
}

#[vise::register]
pub(crate) static TREE_METRICS: vise::Global<TreeMetrics> = vise::Global::new();

// todo: replace with the proper TreeManager implementation (currently it only works with Postgres)
pub struct TreeManager {
    tree: Arc<RwLock<MerkleTree<RocksDBWrapper>>>,
    state_handle: StateHandle,
    last_processed_block: u64,
}

#[derive(Debug, Clone, Copy)]
pub enum MerkleTreeColumnFamily {
    /// Column family containing versioned tree information in the form of
    /// `NodeKey` -> `Node` mapping.
    Tree,
    /// Resolves keys to (index, version) tuples.
    KeyIndices,
    // TODO: stale keys
}


impl TreeManager {
    pub fn new(
        wrapper: RocksDBWrapper,
        state_handle: StateHandle,
        last_processed_block: u64,
    ) -> Self {
        // todo: error handling
        let tree = MerkleTree::new(wrapper).unwrap();
        Self {
            tree: Arc::new(RwLock::new(tree)),
            state_handle,
            last_processed_block,
        }
    }

    pub async fn run_loop(mut self, l1_sender_handle: L1SenderHandle) -> anyhow::Result<()> {
        loop {
            // Limit block production to one at a time. We need each block's tree root for
            // commitments hence it has to be limited for now.
            let last_block_to_process = self.state_handle.last_canonized_block_number().min(
                self.last_processed_block + 1
            );
            if self.last_processed_block >= last_block_to_process {
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }

            let started_at = Instant::now();
            tracing::info!(
                    "Processing {} blocks ({}-{}) in tree",
                    last_block_to_process - self.last_processed_block,
                    self.last_processed_block + 1,
                    last_block_to_process
                );

            let diffs = self.state_handle.0.in_memory_storage.collect_diffs_range(
                self.last_processed_block + 1,
                last_block_to_process,
            ).context("Failed to get diffs for block")?;

            let tree_entries = diffs
                .into_iter()
                .map(|(key, value)|
                TreeEntry {
                    key: bytes32_to_h256(key),
                    value: bytes32_to_h256(key),
                }
                )
                .collect::<Vec<_>>();

            let clone = self.tree.clone();
            let count = tree_entries.len();
            let tree_output = tokio::task::spawn_blocking(move || {
                clone.write().unwrap().extend(&tree_entries)
            }).await??;
            tracing::info!("Processed block {} in tree, output: {:?}", last_block_to_process, tree_output);

            let (batch_output, transactions) = self.state_handle.0.in_memory_block_receipts.get(last_block_to_process).unwrap();
            let commitment = build_commitment(batch_output, transactions, tree_output);
            l1_sender_handle.commit(commitment).await?;

            TREE_METRICS
                .entry_time
                .observe(started_at.elapsed().div(count as u32));

            TREE_METRICS
                .block_time
                .observe(started_at.elapsed() / ((last_block_to_process - self.last_processed_block) as u32));

            TREE_METRICS
                .range_time
                .observe(started_at.elapsed());

            TREE_METRICS
                .processing_range
                .observe(last_block_to_process - self.last_processed_block);

            self.last_processed_block = last_block_to_process;

        }
    }
}

fn build_commitment(
    batch_output: BatchOutput,
    transactions: Vec<Transaction>,
    tree_output: zksync_zk_os_merkle_tree::BatchOutput,
) -> ZkosCommitment {
    let mut l1_tx_count = 0;
    let mut l2_tx_count = 0;
    let mut priority_ops_onchain_data = Vec::new();
    for tx in &transactions {
        if matches!(tx.common_data, ExecuteTransactionCommon::L1(_)) {
            l1_tx_count += 1;
        } else {
            l2_tx_count += 1;
        }

        if let ExecuteTransactionCommon::L1(data) = &tx.common_data {
            let onchain_metadata = data.onchain_metadata().onchain_data;
            priority_ops_onchain_data.push(onchain_metadata);
        }
    }

    ZkosCommitment {
        batch_number: batch_output.header.number as u32,
        block_timestamp: batch_output.header.timestamp,
        tree_root_hash: tree_output.root_hash,
        tree_next_free_index: tree_output.leaf_count,
        number_of_layer1_txs: l1_tx_count,
        number_of_layer2_txs: l2_tx_count,
        priority_ops_onchain_data,
        l2_to_l1_logs_root_hash: Default::default(),
        pubdata: batch_output.pubdata.clone(),
        chain_id: CHAIN_ID as u32,
    }
}

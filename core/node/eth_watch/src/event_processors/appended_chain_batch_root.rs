use std::sync::Arc;

use anyhow::Context;
use itertools::Itertools;
use zksync_dal::{eth_watcher_dal::EventType, Connection, Core, CoreDal, DalError};
use zksync_mini_merkle_tree::MiniMerkleTree;
use zksync_types::{
    api::{ChainAggProof, Log},
    ethabi, h256_to_u256,
    l2_to_l1_log::{
        BatchAndChainMerklePath, BATCH_LEAF_PADDING, LOG_PROOF_SUPPORTED_METADATA_VERSION,
    },
    u256_to_h256, L1BatchNumber, L2BlockNumber, L2ChainId, SLChainId, H256, U256,
};

use crate::{
    client::ZkSyncExtentionEthClient,
    event_processors::{EventProcessor, EventProcessorError, EventsSource},
};

/// Listens to `AppendedChainBatchRoot` events and saves `BatchAndChainMerklePath` for batches.
/// These events are emitted on SL each time L1 batch is executed. Processor uses them to track which batches are already executed
/// and group them by SL's batch number they are executed in as this data is required to build `BatchAndChainMerklePath`.
#[derive(Debug)]
pub struct BatchRootProcessor {
    last_processed_sl_l1_batch_number: L1BatchNumber,
    appended_chain_batch_root_signature: H256,
    merkle_tree: MiniMerkleTree<[u8; 96]>,
    l2_chain_id: L2ChainId,
    sl_l2_client: Arc<dyn ZkSyncExtentionEthClient>,
}

impl BatchRootProcessor {
    pub fn new(
        last_processed_sl_l1_batch_number: L1BatchNumber,
        merkle_tree: MiniMerkleTree<[u8; 96]>,
        l2_chain_id: L2ChainId,
        sl_l2_client: Arc<dyn ZkSyncExtentionEthClient>,
    ) -> Self {
        Self {
            last_processed_sl_l1_batch_number,
            appended_chain_batch_root_signature: ethabi::long_signature(
                "AppendedChainBatchRoot",
                &[
                    ethabi::ParamType::Uint(256),
                    ethabi::ParamType::Uint(256),
                    ethabi::ParamType::FixedBytes(32),
                ],
            ),
            merkle_tree,
            l2_chain_id,
            sl_l2_client,
        }
    }
}

#[async_trait::async_trait]
impl EventProcessor for BatchRootProcessor {
    async fn process_events(
        &mut self,
        storage: &mut Connection<'_, Core>,
        events: Vec<Log>,
    ) -> Result<usize, EventProcessorError> {
        let events_count = events.len();
        let mut transaction = storage
            .start_transaction()
            .await
            .map_err(DalError::generalize)?;

        let grouped_events: Vec<_> = events
            .into_iter()
            .map(|log| {
                let sl_block_number = L2BlockNumber(
                    log.block_number
                        .expect("Missing block number for finalized event")
                        .as_u32(),
                );
                let sl_l1_batch_number = L1BatchNumber(
                    log.l1_batch_number
                        .expect("Missing L1 batch number for finalized event")
                        .as_u32(),
                );
                let chain_l1_batch_number = L1BatchNumber(h256_to_u256(log.topics[2]).as_u32());
                let logs_root_hash = H256::from_slice(&log.data.0);

                (
                    sl_block_number,
                    sl_l1_batch_number,
                    chain_l1_batch_number,
                    logs_root_hash,
                )
            })
            .chunk_by(|(sl_block_number, _, _, _)| *sl_block_number)
            .into_iter()
            .map(|(sl_block_number, group)| {
                let group: Vec<_> = group
                    .into_iter()
                    .map(
                        |(_, sl_l1_batch_number, chain_l1_batch_number, logs_root_hash)| {
                            (sl_l1_batch_number, chain_l1_batch_number, logs_root_hash)
                        },
                    )
                    .collect();

                (sl_block_number, group)
            })
            .collect();

        let sl_chain_id = self.sl_l2_client.chain_id().await?;
        let last_processed_sl_l1_batch_number = self.last_processed_sl_l1_batch_number;
        let mut sl_l1_chain_proof_vector = self
            .sl_l1_chain_proof_vector(
                last_processed_sl_l1_batch_number,
                self.l2_chain_id,
                sl_chain_id,
            )
            .await?;

        for (sl_block_number, chain_batches) in grouped_events {
            let sl_l1_batch_number = chain_batches.first().unwrap().0;
            if sl_l1_batch_number > last_processed_sl_l1_batch_number {
                // Update the SL L1 chain agg proof if we are in a new L1 batch
                sl_l1_chain_proof_vector = self
                    .sl_l1_chain_proof_vector(
                        last_processed_sl_l1_batch_number,
                        self.l2_chain_id,
                        sl_chain_id,
                    )
                    .await?;
                self.last_processed_sl_l1_batch_number = sl_l1_batch_number;

                let chain_root_local = self.merkle_tree.merkle_root();
                let chain_root_remote = self
                    .sl_l2_client
                    .get_chain_root_l2(sl_l1_batch_number, self.l2_chain_id)
                    .await?;
                assert_eq!(
                    chain_root_local,
                    chain_root_remote.unwrap(),
                    "Chain root mismatch, l1 batch number #{sl_l1_batch_number}"
                );
            }

            // Update the tree with the new batches first
            // Else we may store an incorrect batch proof if the chain happens to execute again in the same block
            for (_, batch_number, logs_root_hash) in &chain_batches {
                let root_from_db = transaction
                    .blocks_dal()
                    .get_l1_batch_l2_l1_merkle_root(*batch_number)
                    .await
                    .map_err(DalError::generalize)?
                    .context("Missing l2_l1_merkle_root for finalized batch")?;
                assert_eq!(root_from_db, *logs_root_hash);

                self.merkle_tree
                    .push(BatchRootProcessor::batch_leaf_preimage(
                        *logs_root_hash,
                        *batch_number,
                    ));
            }

            // Define the chain agg proof until msg root, shared by all batches in the block
            let chain_agg_proof_until_msg_root = self
                .sl_l2_client
                .get_chain_log_proof_until_msg_root(sl_block_number, self.l2_chain_id)
                .await?
                .context("Missing chain log proof until msg root for finalized batch")?;
            let chain_proof_vector_until_msg_root = BatchRootProcessor::chain_proof_vector(
                sl_block_number.0,
                chain_agg_proof_until_msg_root,
                sl_chain_id,
            );

            // Get the batch chain proof and batch chain proof until msg root for each batch in the block
            let number_of_leaves = self.merkle_tree.length();
            let batch_proofs = (0..chain_batches.len()).map(|i| {
                let leaf_position = number_of_leaves - chain_batches.len() + i;
                let batch_proof = self
                    .merkle_tree
                    .merkle_root_and_path_by_absolute_index(leaf_position)
                    .1;
                let batch_proof_len = batch_proof.len() as u32;
                let mut proof = vec![H256::from_low_u64_be(leaf_position as u64)];
                proof.extend(batch_proof);
                let mut proof_until_msg_root = proof.clone();
                proof.extend(sl_l1_chain_proof_vector.clone());
                proof_until_msg_root.extend(chain_proof_vector_until_msg_root.clone());

                (
                    BatchAndChainMerklePath {
                        batch_proof_len,
                        proof,
                    },
                    BatchAndChainMerklePath {
                        batch_proof_len,
                        proof: proof_until_msg_root,
                    },
                )
            });

            // Set the batch chain proof and batch chain proof until msg root for each batch in the block
            for ((batch_number, _, _), (proof, proof_until_msg_root)) in
                chain_batches.iter().zip(batch_proofs)
            {
                tracing::info!(%batch_number, "Saving batch-chain merkle path");
                transaction
                    .blocks_dal()
                    .set_batch_chain_merkle_path(*batch_number, proof)
                    .await
                    .map_err(DalError::generalize)?;
                tracing::info!(%batch_number, "Saving batch-chain merkle path until msg root");
                transaction
                    .blocks_dal()
                    .set_batch_chain_merkle_path_until_msg_root(*batch_number, proof_until_msg_root)
                    .await
                    .map_err(DalError::generalize)?;
            }
        }

        transaction.commit().await.map_err(DalError::generalize)?;

        Ok(events_count)
    }

    fn topic1(&self) -> Option<H256> {
        Some(self.appended_chain_batch_root_signature)
    }

    fn topic2(&self) -> Option<H256> {
        Some(H256::from_low_u64_be(self.l2_chain_id.as_u64()))
    }

    fn event_source(&self) -> EventsSource {
        EventsSource::SL
    }

    fn event_type(&self) -> EventType {
        EventType::ChainBatchRoot
    }

    fn only_finalized_block(&self) -> bool {
        true
    }
}

impl BatchRootProcessor {
    pub(crate) fn batch_leaf_preimage(batch_root: H256, batch_number: L1BatchNumber) -> [u8; 96] {
        let mut full_preimage = [0u8; 96];

        full_preimage[0..32].copy_from_slice(BATCH_LEAF_PADDING.as_bytes());
        full_preimage[32..64].copy_from_slice(batch_root.as_bytes());
        full_preimage[64..96]
            .copy_from_slice(H256::from_low_u64_be(batch_number.0 as u64).as_bytes());

        full_preimage
    }

    async fn sl_l1_chain_proof_vector(
        &self,
        sl_l1_batch_number: L1BatchNumber,
        l2_chain_id: L2ChainId,
        sl_chain_id: SLChainId,
    ) -> anyhow::Result<Vec<H256>> {
        let sl_l1_chain_agg_proof = self
            .sl_l2_client
            .get_chain_log_proof(sl_l1_batch_number, l2_chain_id)
            .await?
            .context("Missing chain log proof for finalized batch")?;

        Ok(Self::chain_proof_vector(
            sl_l1_batch_number.0,
            sl_l1_chain_agg_proof,
            sl_chain_id,
        ))
    }

    fn chain_proof_vector(
        batch_or_block_number: u32,
        chain_agg_proof: ChainAggProof,
        sl_chain_id: SLChainId,
    ) -> Vec<H256> {
        let sl_encoded_data = (U256::from(batch_or_block_number) << 128u32)
            + chain_agg_proof.chain_id_leaf_proof_mask;

        let mut metadata = [0u8; 32];
        metadata[0] = LOG_PROOF_SUPPORTED_METADATA_VERSION;
        metadata[1] = chain_agg_proof.chain_id_leaf_proof.len() as u8;

        // Chain proofs are always final nodes in the proofs.
        metadata[3] = 1;

        let mut chain_proof_vector = vec![
            u256_to_h256(sl_encoded_data),
            H256::from_low_u64_be(sl_chain_id.0),
            H256(metadata),
        ];
        chain_proof_vector.extend(chain_agg_proof.chain_id_leaf_proof);

        chain_proof_vector
    }
}

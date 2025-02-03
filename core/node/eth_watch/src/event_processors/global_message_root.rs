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
    u256_to_h256, L1BatchNumber, L2ChainId, SLChainId, H256, U256,
};

use crate::{
    client::L2EthClient,
    event_processors::{EventProcessor, EventProcessorError, EventsSource},
};

/// Responsible for `AppendedChainBatchRoot` events and saving `BatchAndChainMerklePath` for batches.
#[derive(Debug)]
pub struct GlobalMessageRootProcessor {
    appended_message_root_signature: H256,
}

impl GlobalMessageRootProcessor {
    pub fn new() -> Self {
        Self {
            appended_message_root_signature: ethabi::long_signature(
                "NewGlobalMessageRoot",
                &[ethabi::ParamType::FixedBytes(32)],
            ),
        }
    }
}

#[async_trait::async_trait]
impl EventProcessor for GlobalMessageRootProcessor {
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

        for event in events {
            println!("event {:?}", event);
            let root = event.topics[0];
            assert_eq!(event.topics[0], self.appended_message_root_signature); // guaranteed by the watcher
            tracing::info!(%root, "Saving global message root");
            let block_number = event.block_number; // kl todo
            let block_number = block_number.unwrap().0[0] as u64;
            let chain_id = event.topics[0]; // kl todo
            let chain_id = chain_id.0[15] as u64;
            transaction
                .message_root_dal()
                .set_message_root(
                    SLChainId(chain_id),
                    L1BatchNumber(block_number as u32),
                    root,
                )
                .await
                .map_err(DalError::generalize)?;
        }

        transaction.commit().await.map_err(DalError::generalize)?;

        Ok(events_count)
    }

    fn topic1(&self) -> H256 {
        // println!("appended_message_root_signature: {:?}", self.appended_message_root_signature);

        self.appended_message_root_signature
    }

    // fn topic2(&self) -> Option<H256> {
    //     Some(H256::from_low_u64_be(self.l2_chain_id.0))
    // }

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

impl GlobalMessageRootProcessor {
    pub(crate) fn batch_leaf_preimage(batch_root: H256, batch_number: L1BatchNumber) -> [u8; 96] {
        let mut full_preimage = [0u8; 96];

        full_preimage[0..32].copy_from_slice(BATCH_LEAF_PADDING.as_bytes());
        full_preimage[32..64].copy_from_slice(batch_root.as_bytes());
        full_preimage[64..96]
            .copy_from_slice(H256::from_low_u64_be(batch_number.0 as u64).as_bytes());

        full_preimage
    }

    fn chain_proof_vector(
        sl_l1_batch_number: L1BatchNumber,
        chain_agg_proof: ChainAggProof,
        sl_chain_id: SLChainId,
    ) -> Vec<H256> {
        let sl_encoded_data = U256::from(sl_l1_batch_number.0) * U256::from(2).pow(128.into())
            + chain_agg_proof.chain_id_leaf_proof_mask;

        let mut metadata = [0u8; 32];
        metadata[0] = LOG_PROOF_SUPPORTED_METADATA_VERSION;
        metadata[1] = chain_agg_proof.chain_id_leaf_proof.len() as u8;

        let mut chain_proof_vector = vec![
            u256_to_h256(sl_encoded_data),
            H256::from_low_u64_be(sl_chain_id.0),
            H256(metadata),
        ];
        chain_proof_vector.extend(chain_agg_proof.chain_id_leaf_proof);

        chain_proof_vector
    }
}

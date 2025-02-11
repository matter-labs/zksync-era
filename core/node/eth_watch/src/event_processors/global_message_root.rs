use zksync_dal::{eth_watcher_dal::EventType, Connection, Core, CoreDal, DalError};
use zksync_types::{
    api::{Log},
    ethabi,
    L1BatchNumber, SLChainId, H256,
};

use crate::{
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
                "NewMessageRoot",
                &[
                    ethabi::ParamType::Uint(256),
                    ethabi::ParamType::Uint(256),
                    ethabi::ParamType::FixedBytes(32),
                ],
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
            println!("event in global {:?}", event);
            let root = event.topics[3];
            assert_eq!(event.topics[0], self.appended_message_root_signature); // guaranteed by the watcher
            tracing::info!(%root, "Saving global message root");
            // let block_number = event.block_number; // kl todo
            // let block_number = block_number.unwrap().0[0] as u64;
            let block_bytes: [u8; 8] = event.topics[2].as_bytes()[24..32].try_into().unwrap();
            let chain_id_bytes: [u8; 8] = event.topics[1].as_bytes()[24..32].try_into().unwrap();
            let block_number: u64 = u64::from_be_bytes(block_bytes);
            let chain_id = u64::from_be_bytes(chain_id_bytes);
            println!("block_number in global {:?}", block_number);
            println!("chain_id in global {:?}", chain_id);
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
}

use std::{sync::Arc, time::SystemTime};

use anyhow::Context;
use zksync_dal::{eth_watcher_dal::EventType, Connection, Core, CoreDal, DalError};
use zksync_system_constants::L1_MESSENGER_ADDRESS;
use zksync_types::{api::Log, ethabi, L1BatchNumber, L2ChainId, SLChainId, H256};

use crate::{
    client::ZkSyncExtentionEthClient,
    event_processors::{EventProcessor, EventProcessorError, EventsSource},
};

/// Responsible for `AppendedChainBatchRoot` events and saving `BatchAndChainMerklePath` for batches.
#[derive(Debug)]
pub struct InteropRootProcessor {
    appended_interop_root_signature: H256,
    event_source: EventsSource,
    l2_chain_id: L2ChainId,
    pub sl_l2_client: Option<Arc<dyn ZkSyncExtentionEthClient>>,
    pub sl_chain_id: Option<SLChainId>,
}

impl InteropRootProcessor {
    pub async fn new(
        event_source: EventsSource,
        l2_chain_id: L2ChainId,
        sl_l2_client: Option<Arc<dyn ZkSyncExtentionEthClient>>,
    ) -> Self {
        let sl_chain_id = if let Some(sl_l2_client) = sl_l2_client.clone() {
            Some(sl_l2_client.chain_id().await.unwrap())
        } else {
            None
        };
        Self {
            appended_interop_root_signature: ethabi::long_signature(
                "NewInteropRoot",
                &[
                    ethabi::ParamType::Uint(256),
                    ethabi::ParamType::Uint(256),
                    ethabi::ParamType::Uint(256),
                    ethabi::ParamType::Array(Box::new(ethabi::ParamType::FixedBytes(32))),
                ],
            ),
            event_source,
            l2_chain_id,
            sl_l2_client,
            sl_chain_id,
        }
    }
}

#[async_trait::async_trait]
impl EventProcessor for InteropRootProcessor {
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
            let mut tokens = ethabi::decode(
                &[ethabi::ParamType::Array(Box::new(
                    ethabi::ParamType::FixedBytes(32),
                ))],
                event.data.0.as_slice(),
            )
            .expect("Failed to decode BytecodeL1PublicationRequested message");
            let token = tokens.remove(0);

            let mut root: Vec<H256> = vec![];
            if event.address == L1_MESSENGER_ADDRESS {
                root.push(H256::zero());
            }
            root = [
                root,
                token
                    .into_array()
                    .unwrap()
                    .into_iter()
                    .map(|t| H256::from_slice(&t.clone().into_fixed_bytes().unwrap()))
                    .collect::<Vec<_>>(),
            ]
            .concat();
            assert_eq!(event.topics[0], self.appended_interop_root_signature); // guaranteed by the watcher

            let block_bytes: [u8; 8] = event.topics[2].as_bytes()[24..32].try_into().unwrap();
            let chain_id_bytes: [u8; 8] = event.topics[1].as_bytes()[24..32].try_into().unwrap();
            let block_number: u64 = u64::from_be_bytes(block_bytes);
            let chain_id = u64::from_be_bytes(chain_id_bytes);
            if let Some(sl_l2_client) = self.sl_l2_client.clone() {
                // we skip precommit message roots ( local roots) for GW.
                let sl_chain_id = sl_l2_client.chain_id().await?;
                if sl_chain_id.0 == chain_id && event.address == L1_MESSENGER_ADDRESS {
                    continue;
                }
            }
            if event.address == L1_MESSENGER_ADDRESS {
                // kl todo we skip precommit for now.
                continue;
            }
            if L2ChainId::new(chain_id).unwrap() == self.l2_chain_id {
                // we ignore our chainBatchRoots
                continue;
            }
            if self.event_source == EventsSource::L1
                && self.sl_chain_id.is_some()
                && self.sl_chain_id.unwrap().0 == chain_id
            {
                // we ignore the chainBatchRoot of GW.
                // GW -> chain interop not allowed.
                continue;
            }

            let timestamp = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .context("incorrect system time")?
                .as_secs();
            transaction
                .interop_root_dal()
                .set_interop_root(
                    SLChainId(chain_id),
                    L1BatchNumber(block_number as u32),
                    &root,
                    timestamp,
                )
                .await
                .map_err(DalError::generalize)?;
        }

        transaction.commit().await.map_err(DalError::generalize)?;

        Ok(events_count)
    }

    fn topic1(&self) -> Option<H256> {
        Some(self.appended_interop_root_signature)
    }

    fn event_source(&self) -> EventsSource {
        self.event_source.clone()
    }

    fn event_type(&self) -> EventType {
        EventType::InteropRoot
    }

    fn only_finalized_block(&self) -> bool {
        true
    }
}

impl InteropRootProcessor {}

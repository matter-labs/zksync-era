use anyhow::Context as _;
use zksync_contracts::server_notifier_contract;
use zksync_dal::{eth_watcher_dal::EventType, Connection, Core, CoreDal};
use zksync_types::{api::Log, L1BlockNumber, L2ChainId, H256, U256};

use crate::event_processors::{EventProcessor, EventProcessorError, EventsSource};

#[derive(Debug)]
pub struct GatewayMigrationProcessor {
    l2_chain_id: L2ChainId,
    possible_main_topics: Vec<H256>,
}

impl GatewayMigrationProcessor {
    pub fn new(l2_chain_id: L2ChainId) -> Self {
        let contract = server_notifier_contract();
        let topics = vec![
            contract.event("MigrateToGateway").unwrap().signature(),
            contract.event("MigrateFromGateway").unwrap().signature(),
        ];

        Self {
            l2_chain_id,
            possible_main_topics: topics,
        }
    }
}

#[async_trait::async_trait]
impl EventProcessor for GatewayMigrationProcessor {
    async fn process_events(
        &mut self,
        storage: &mut Connection<'_, Core>,
        events: Vec<Log>,
    ) -> Result<usize, EventProcessorError> {
        for event in &events {
            let main_topic = event
                .topics
                .first()
                .copied()
                .context("missing topic 0")
                .map_err(EventProcessorError::internal)?;
            if !self.possible_main_topics.contains(&main_topic) {
                continue;
            }

            let chain_id = U256::from_big_endian(
                event
                    .topics
                    .get(1)
                    .copied()
                    .context("missing topic 1")
                    .map_err(EventProcessorError::internal)?
                    .as_bytes(),
            );

            if L2ChainId::from(chain_id.as_u32()) != self.l2_chain_id {
                continue;
            }

            storage
                .server_notifications_dal()
                .save_notification(
                    *event.topics.first().unwrap(),
                    L1BlockNumber(event.block_number.unwrap().as_u32()),
                )
                .await
                .unwrap();
        }
        Ok(events.len())
    }

    fn topic1(&self) -> Option<H256> {
        None
    }

    fn topic2(&self) -> Option<H256> {
        Some(H256::from_low_u64_be(self.l2_chain_id.as_u64()))
    }

    fn event_source(&self) -> EventsSource {
        EventsSource::L1
    }

    fn event_type(&self) -> EventType {
        EventType::ServerNotification
    }
}

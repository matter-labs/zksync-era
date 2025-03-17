use anyhow::Context as _;
use zksync_dal::{eth_watcher_dal::EventType, Connection, Core, CoreDal};
use zksync_types::{api::Log, L1BlockNumber, L2ChainId, H256, U256};

use crate::event_processors::{EventProcessor, EventProcessorError, EventsSource};

#[derive(Debug)]
pub struct GatewayMigrationProcessor {
    l2chain_id: L2ChainId,
}

impl GatewayMigrationProcessor {
    pub fn new(l2chain_id: L2ChainId) -> Self {
        Self { l2chain_id }
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
            let chain_id = U256::from_big_endian(
                event
                    .topics
                    .get(1)
                    .copied()
                    .context("missing topic 1")?
                    .as_bytes(),
            );

            if L2ChainId::from(chain_id.as_u32()) != self.l2chain_id {
                continue;
            }

            storage
                .server_notifications_dal()
                .save_notification(
                    *event.topics.first().unwrap(),
                    L1BlockNumber(event.block_number.unwrap().as_u32()),
                    Default::default(),
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
        Some(H256::from_low_u64_be(self.l2chain_id.as_u64()))
    }

    fn event_source(&self) -> EventsSource {
        EventsSource::L1
    }

    fn event_type(&self) -> EventType {
        EventType::ServerNotification
    }
}

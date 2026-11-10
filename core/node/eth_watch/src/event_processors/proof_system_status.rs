use anyhow::Context as _;
use zksync_dal::{eth_watcher_dal::EventType, Connection, Core, CoreDal, DalError};
use zksync_types::{
    api::Log,
    server_notification::{
        ProofSystemStatusNotification, PROOF_SYSTEM_STATUS_NOTIFICATION_SIGNATURE,
    },
    L1BlockNumber, L2ChainId, H256, U256,
};

use super::{EventProcessor, EventProcessorError, EventsSource};

#[derive(Debug)]
pub struct ProofSystemStatusProcessor {
    l2_chain_id: L2ChainId,
}

impl ProofSystemStatusProcessor {
    pub fn new(l2_chain_id: L2ChainId) -> Self {
        Self { l2_chain_id }
    }

    fn parse_event(
        &self,
        event: &Log,
    ) -> anyhow::Result<Option<(L1BlockNumber, ProofSystemStatusNotification)>> {
        if event.topics.first() != Some(&*PROOF_SYSTEM_STATUS_NOTIFICATION_SIGNATURE)
            || event.topics.get(1) != self.topic2().as_ref()
            || event.is_removed()
        {
            return Ok(None);
        }
        anyhow::ensure!(
            event.topics.len() == 2 && event.data.0.len() == 64,
            "invalid proof system notification encoding"
        );
        let boojum = U256::from_big_endian(&event.data.0[..32]);
        let airbender = U256::from_big_endian(&event.data.0[32..]);
        anyhow::ensure!(
            boojum <= U256::one() && airbender <= U256::one(),
            "invalid disabled proof systems booleans"
        );
        let mask = boojum.as_u32() as u8 | ((airbender.as_u32() as u8) << 1);
        anyhow::ensure!(mask != 3, "both proof systems cannot be disabled");
        let block: u32 = event
            .block_number
            .context("missing notification block number")?
            .as_u64()
            .try_into()?;
        let log_index: u64 = event
            .log_index
            .context("missing notification log index")?
            .try_into()
            .map_err(anyhow::Error::msg)?;
        Ok(Some((
            L1BlockNumber(block),
            ProofSystemStatusNotification {
                disabled_proof_systems: mask,
                log_index,
            },
        )))
    }
}

#[async_trait::async_trait]
impl EventProcessor for ProofSystemStatusProcessor {
    async fn process_events(
        &mut self,
        storage: &mut Connection<'_, Core>,
        events: Vec<Log>,
    ) -> Result<usize, EventProcessorError> {
        for event in &events {
            let Some((block, notification)) = self
                .parse_event(event)
                .map_err(|err| EventProcessorError::log_parse(err, "ProofSystemStatusNotified"))?
            else {
                continue;
            };
            storage
                .server_notifications_dal()
                .save_proof_system_status_notification(block, notification)
                .await
                .map_err(DalError::generalize)
                .map_err(EventProcessorError::internal)?;
        }
        Ok(events.len())
    }

    fn topic1(&self) -> Option<H256> {
        Some(*PROOF_SYSTEM_STATUS_NOTIFICATION_SIGNATURE)
    }

    fn topic2(&self) -> Option<H256> {
        Some(H256::from_low_u64_be(self.l2_chain_id.as_u64()))
    }

    fn event_source(&self) -> EventsSource {
        EventsSource::L1
    }

    fn event_type(&self) -> EventType {
        EventType::ProofSystemStatus
    }
}

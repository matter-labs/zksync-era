use std::convert::TryFrom;
use zksync_dal::StorageProcessor;
use zksync_types::{web3::types::Log, ProtocolUpgrade, ProtocolVersionId, H256};

use crate::eth_watch::{
    client::{Error, EthClient},
    event_processors::EventProcessor,
    metrics::{PollStage, METRICS},
};

pub(crate) const UPGRADE_PROPOSAL_SIGNATURE: H256 = H256([
    105, 17, 91, 73, 175, 231, 166, 16, 26, 46, 122, 241, 125, 66, 30, 218, 29, 193, 83, 189, 38,
    214, 153, 240, 19, 196, 255, 240, 64, 70, 70, 166,
]);

/// Responsible for saving new protocol upgrade proposals to the database.
#[derive(Debug)]
pub struct UpgradesEventProcessor {
    last_seen_version_id: ProtocolVersionId,
}

impl UpgradesEventProcessor {
    pub fn new(last_seen_version_id: ProtocolVersionId) -> Self {
        Self {
            last_seen_version_id,
        }
    }
}

#[async_trait::async_trait]
impl<W: EthClient + Sync> EventProcessor<W> for UpgradesEventProcessor {
    async fn process_events(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        client: &W,
        events: Vec<Log>,
    ) -> Result<(), Error> {
        let mut upgrades = Vec::new();
        for event in events
            .into_iter()
            .filter(|event| event.topics[0] == UPGRADE_PROPOSAL_SIGNATURE)
        {
            let upgrade = ProtocolUpgrade::try_from(event)
                .map_err(|err| Error::LogParse(format!("{:?}", err)))?;
            // Scheduler VK is not present in proposal event. It is hardcoded in verifier contract.
            let scheduler_vk_hash = if let Some(address) = upgrade.verifier_address {
                Some(client.scheduler_vk_hash(address).await?)
            } else {
                None
            };
            upgrades.push((upgrade, scheduler_vk_hash));
        }

        if upgrades.is_empty() {
            return Ok(());
        }

        let ids_str: Vec<_> = upgrades
            .iter()
            .map(|(u, _)| format!("{}", u.id as u16))
            .collect();
        tracing::debug!("Received upgrades with ids: {}", ids_str.join(", "));

        let new_upgrades: Vec<_> = upgrades
            .into_iter()
            .skip_while(|(v, _)| v.id as u16 <= self.last_seen_version_id as u16)
            .collect();
        if new_upgrades.is_empty() {
            return Ok(());
        }

        let last_id = new_upgrades.last().unwrap().0.id;
        let stage_latency = METRICS.poll_eth_node[&PollStage::PersistUpgrades].start();
        for (upgrade, scheduler_vk_hash) in new_upgrades {
            let previous_version = storage
                .protocol_versions_dal()
                .load_previous_version(upgrade.id)
                .await
                .expect("Expected previous version to be present in DB");
            let new_version = previous_version.apply_upgrade(upgrade, scheduler_vk_hash);
            storage
                .protocol_versions_dal()
                .save_protocol_version_with_tx(new_version)
                .await;
        }
        stage_latency.observe();
        self.last_seen_version_id = last_id;
        Ok(())
    }

    fn relevant_topic(&self) -> H256 {
        UPGRADE_PROPOSAL_SIGNATURE
    }
}

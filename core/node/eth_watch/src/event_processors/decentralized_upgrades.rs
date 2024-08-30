use std::ops::RangeInclusive;

use anyhow::Context as _;
use zksync_dal::{eth_watcher_dal::EventType, Connection, Core, CoreDal, DalError};
use zksync_types::{
    ethabi::Contract, protocol_version::ProtocolSemanticVersion, web3::Log, L1BatchNumber,
    ProtocolUpgrade, H256, U256,
};

use crate::{
    client::EthClient,
    event_processors::{EventProcessor, EventProcessorError, EventsSource},
    metrics::{PollStage, METRICS},
};

/// Listens to scheduling events coming from the chain admin contract and saves new protocol upgrade proposals to the database.
#[derive(Debug)]
pub struct DecentralizedUpgradesEventProcessor {
    /// Last protocol version seen. Used to skip events for already known upgrade proposals.
    last_seen_protocol_version: ProtocolSemanticVersion,
    update_upgrade_timestamp_signature: H256,
}

impl DecentralizedUpgradesEventProcessor {
    pub fn new(
        last_seen_protocol_version: ProtocolSemanticVersion,
        chain_admin_contract: &Contract,
    ) -> Self {
        Self {
            last_seen_protocol_version,
            update_upgrade_timestamp_signature: chain_admin_contract
                .event("UpdateUpgradeTimestamp")
                .context("UpdateUpgradeTimestamp event is missing in ABI")
                .unwrap()
                .signature(),
        }
    }
}

#[async_trait::async_trait]
impl EventProcessor for DecentralizedUpgradesEventProcessor {
    async fn process_events(
        &mut self,
        storage: &mut Connection<'_, Core>,
        sl_client: &dyn EthClient,
        events: Vec<Log>,
    ) -> Result<usize, EventProcessorError> {
        let mut upgrades = Vec::new();
        for event in &events {
            let version = event.topics.get(1).copied().context("missing topic 1")?;
            let timestamp: u64 = U256::from_big_endian(&event.data.0)
                .try_into()
                .ok()
                .context("upgrade timestamp is too big")?;

            let diamond_cut = sl_client
                .diamond_cut_by_version(version)
                .await?
                .context("missing upgrade data on STM")?;

            let upgrade = ProtocolUpgrade {
                timestamp,
                ..ProtocolUpgrade::try_from_diamond_cut(&diamond_cut)?
            };
            // Scheduler VK is not present in proposal event. It is hard coded in verifier contract.
            let scheduler_vk_hash = if let Some(address) = upgrade.verifier_address {
                Some(sl_client.scheduler_vk_hash(address).await?)
            } else {
                None
            };
            upgrades.push((upgrade, scheduler_vk_hash));
        }

        let new_upgrades: Vec<_> = upgrades
            .into_iter()
            .skip_while(|(v, _)| v.version <= self.last_seen_protocol_version)
            .collect();

        let Some((last_upgrade, _)) = new_upgrades.last() else {
            return Ok(events.len());
        };
        let versions: Vec<_> = new_upgrades
            .iter()
            .map(|(u, _)| u.version.to_string())
            .collect();
        tracing::debug!("Received upgrades with versions: {versions:?}");

        let last_version = last_upgrade.version;
        let stage_latency = METRICS.poll_eth_node[&PollStage::PersistUpgrades].start();
        for (upgrade, scheduler_vk_hash) in new_upgrades {
            let latest_semantic_version = storage
                .protocol_versions_dal()
                .latest_semantic_version()
                .await
                .map_err(DalError::generalize)?
                .context("expected some version to be present in DB")?;

            if upgrade.version > latest_semantic_version {
                let latest_version = storage
                    .protocol_versions_dal()
                    .get_protocol_version_with_latest_patch(latest_semantic_version.minor)
                    .await
                    .map_err(DalError::generalize)?
                    .with_context(|| {
                        format!(
                            "expected minor version {} to be present in DB",
                            latest_semantic_version.minor as u16
                        )
                    })?;

                let new_version = latest_version.apply_upgrade(upgrade, scheduler_vk_hash);
                if new_version.version.minor == latest_semantic_version.minor {
                    // Only verification parameters may change if only patch is bumped.
                    assert_eq!(
                        new_version.base_system_contracts_hashes,
                        latest_version.base_system_contracts_hashes
                    );
                    assert!(new_version.tx.is_none());
                }
                storage
                    .protocol_versions_dal()
                    .save_protocol_version_with_tx(&new_version)
                    .await
                    .map_err(DalError::generalize)?;
            }
        }
        stage_latency.observe();

        self.last_seen_protocol_version = last_version;
        Ok(events.len())
    }

    fn relevant_topic(&self) -> H256 {
        self.update_upgrade_timestamp_signature
    }

    fn event_source(&self) -> EventsSource {
        EventsSource::SL
    }

    fn event_type(&self) -> EventType {
        EventType::ProtocolUpgrades
    }
}

use std::{collections::HashMap, sync::Arc};

use anyhow::Context as _;
use itertools::Itertools;
use zksync_contracts::chain_admin_contract;
use zksync_dal::{eth_watcher_dal::EventType, Connection, Core, CoreDal, DalError};
use zksync_types::{
    api::Log,
    h256_to_u256,
    protocol_upgrade::ProtocolUpgradePreimageOracle,
    protocol_version::{ProtocolSemanticVersion, ProtocolVersionId},
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
    sl_client: Arc<dyn EthClient>,
    l1_client: Arc<dyn EthClient>,
}

impl DecentralizedUpgradesEventProcessor {
    pub fn new(
        last_seen_protocol_version: ProtocolSemanticVersion,
        sl_client: Arc<dyn EthClient>,
        l1_client: Arc<dyn EthClient>,
    ) -> Self {
        Self {
            last_seen_protocol_version,
            update_upgrade_timestamp_signature: chain_admin_contract()
                .event("UpdateUpgradeTimestamp")
                .context("UpdateUpgradeTimestamp event is missing in ABI")
                .unwrap()
                .signature(),
            sl_client,
            l1_client,
        }
    }
}

#[async_trait::async_trait]
impl ProtocolUpgradePreimageOracle for &dyn EthClient {
    async fn get_protocol_upgrade_preimages(
        &self,
        hashes: Vec<H256>,
    ) -> anyhow::Result<Vec<Vec<u8>>> {
        let preimages = self.get_published_preimages(hashes.clone()).await?;

        let mut result = vec![];
        for (i, preimage) in preimages.into_iter().enumerate() {
            let preimage = preimage.with_context(|| {
                format!(
                    "Protocol upgrade preimage under id {i} for {:#?} is missing",
                    hashes[i]
                )
            })?;
            result.push(preimage);
        }

        Ok(result)
    }
}

#[async_trait::async_trait]
impl EventProcessor for DecentralizedUpgradesEventProcessor {
    async fn process_events(
        &mut self,
        storage: &mut Connection<'_, Core>,
        events: Vec<Log>,
    ) -> Result<usize, EventProcessorError> {
        let mut upgrades = HashMap::new();
        let mut processed_events = 0;
        for event in &events {
            let version = event
                .topics
                .get(1)
                .copied()
                .context("missing topic 1")
                .map_err(EventProcessorError::internal)?;
            let timestamp: u64 = U256::from_big_endian(&event.data.0)
                .try_into()
                .ok()
                .context("upgrade timestamp is too big")
                .map_err(EventProcessorError::internal)?;

            let latest_protocol_version =
                ProtocolSemanticVersion::try_from_packed(h256_to_u256(version))
                    .map_err(|err| EventProcessorError::internal(anyhow::anyhow!(err)))?;
            if latest_protocol_version <= self.last_seen_protocol_version {
                // This version has been already processed, skip it.
                processed_events += 1;
                continue;
            }

            let diamond_cuts = self
                .sl_client
                .diamond_cuts_since_version(self.last_seen_protocol_version)
                .await
                .map_err(EventProcessorError::client)?;
            if diamond_cuts.is_empty() {
                tracing::info!(
                    %latest_protocol_version,
                    "Upgrade timestamp observed before diamond cut data; retrying later"
                );
                break;
            }

            for diamond_cut in diamond_cuts {
                let (mut upgrade, init_address) = ProtocolUpgrade::try_from_diamond_cut(
                    &diamond_cut,
                    self.l1_client.as_ref(),
                    self.l1_client
                        .get_chain_gateway_upgrade_info()
                        .await
                        .map_err(EventProcessorError::contract_call)?,
                )
                .await
                .map_err(EventProcessorError::internal)?;

                upgrade.timestamp = timestamp;

                // v31 upgrade txs carry generic placeholder calldata in the diamond cut.
                // Call the upgrader contract on L1 to get the chain-specific version.
                if upgrade.version.minor == ProtocolVersionId::Version31 {
                    if let Some(ref mut tx) = upgrade.tx {
                        let placeholder = std::mem::take(&mut tx.execute.calldata);
                        tx.execute.calldata = self
                            .l1_client
                            .get_l2_upgrade_tx_data(init_address, placeholder)
                            .await
                            .map_err(EventProcessorError::contract_call)?;
                    }
                }

                if upgrade.version > latest_protocol_version {
                    continue;
                }

                // Scheduler VK is not present in proposal event. It is hard coded in verifier contract.
                let scheduler_vk_hash = if let Some(address) = upgrade.verifier_address {
                    Some(
                        self.sl_client
                            .scheduler_vk_hash(address)
                            .await
                            .map_err(EventProcessorError::contract_call)?,
                    )
                } else {
                    None
                };

                // Scheduler VK is not present in proposal event. It is hard coded in verifier contract.
                let fflonk_scheduler_vk_hash = if let Some(address) = upgrade.verifier_address {
                    self.sl_client
                        .fflonk_scheduler_vk_hash(address)
                        .await
                        .map_err(EventProcessorError::contract_call)?
                } else {
                    None
                };
                upgrades.insert(
                    upgrade.version,
                    (upgrade, scheduler_vk_hash, fflonk_scheduler_vk_hash),
                );
            }
            // eth_watch advances its cursor by this return value. If the diamond cut
            // is not visible yet, leave that event unprocessed so it is retried.
            processed_events += 1;
        }

        let new_upgrades: Vec<_> = upgrades
            .values()
            .cloned()
            .skip_while(|(v, _, _)| v.version <= self.last_seen_protocol_version)
            .sorted_by(|(a, _, _), (b, _, _)| a.version.cmp(&b.version))
            .collect();

        let Some((last_upgrade, _, _)) = new_upgrades.last() else {
            return Ok(processed_events);
        };
        let versions: Vec<_> = new_upgrades
            .iter()
            .map(|(u, _, _)| u.version.to_string())
            .collect();
        tracing::debug!("Received upgrades with versions: {versions:?}");

        let last_version = last_upgrade.version;
        let stage_latency = METRICS.poll_eth_node[&PollStage::PersistUpgrades].start();
        for (upgrade, scheduler_vk_hash, fflonk_scheduler_vk_hash) in new_upgrades {
            let latest_semantic_version = storage
                .protocol_versions_dal()
                .latest_semantic_version()
                .await
                .map_err(DalError::generalize)
                .map_err(EventProcessorError::internal)?
                .context("expected some version to be present in DB")
                .map_err(EventProcessorError::internal)?;

            if upgrade.version > latest_semantic_version {
                let latest_version = storage
                    .protocol_versions_dal()
                    .get_protocol_version_with_latest_patch(latest_semantic_version.minor)
                    .await
                    .map_err(DalError::generalize)
                    .map_err(EventProcessorError::internal)?
                    .with_context(|| {
                        format!(
                            "expected minor version {} to be present in DB",
                            latest_semantic_version.minor as u16
                        )
                    })
                    .map_err(EventProcessorError::internal)?;

                let new_version = latest_version.apply_upgrade(
                    upgrade,
                    scheduler_vk_hash,
                    fflonk_scheduler_vk_hash,
                );
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
                    .map_err(DalError::generalize)
                    .map_err(EventProcessorError::internal)?;
            }
        }
        stage_latency.observe();

        self.last_seen_protocol_version = last_version;
        Ok(processed_events)
    }

    fn topic1(&self) -> Option<H256> {
        Some(self.update_upgrade_timestamp_signature)
    }

    fn event_source(&self) -> EventsSource {
        EventsSource::L1
    }

    fn event_type(&self) -> EventType {
        EventType::ProtocolUpgrades
    }
}

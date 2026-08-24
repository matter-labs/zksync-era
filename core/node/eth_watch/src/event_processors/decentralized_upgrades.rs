use std::{collections::HashMap, sync::Arc};

use anyhow::Context as _;
use itertools::Itertools;
use zksync_contracts::server_notifier_contract;
use zksync_dal::{eth_watcher_dal::EventType, Connection, Core, CoreDal, DalError};
use zksync_types::{
    api::Log, h256_to_u256, protocol_upgrade::ProtocolUpgradePreimageOracle,
    protocol_version::ProtocolSemanticVersion, Address, L2ChainId, ProtocolUpgrade, H256, U256,
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
    upgrade_timestamp_updated_signature: H256,
    sl_client: Arc<dyn EthClient>,
    l1_client: Arc<dyn EthClient>,
    l2_chain_id: L2ChainId,
}

impl DecentralizedUpgradesEventProcessor {
    pub fn new(
        last_seen_protocol_version: ProtocolSemanticVersion,
        sl_client: Arc<dyn EthClient>,
        l1_client: Arc<dyn EthClient>,
        l2_chain_id: L2ChainId,
    ) -> Self {
        Self {
            last_seen_protocol_version,
            upgrade_timestamp_updated_signature: server_notifier_contract()
                .event("UpgradeTimestampUpdated")
                .context("UpgradeTimestampUpdated event is missing in ABI")
                .unwrap()
                .signature(),
            sl_client,
            l1_client,
            l2_chain_id,
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

    async fn prepare_l2_upgrade_tx_data(
        &self,
        init_address: Address,
        existing_tx_data: Vec<u8>,
    ) -> anyhow::Result<Vec<u8>> {
        self.get_l2_upgrade_tx_data(init_address, existing_tx_data)
            .await
            .map_err(Into::into)
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
        for event in &events {
            let version = event
                .topics
                .get(2)
                .copied()
                .context("missing topic 2")
                .map_err(EventProcessorError::internal)?;
            let timestamp: u64 = U256::from_big_endian(&event.data.0)
                .try_into()
                .ok()
                .context("upgrade timestamp is too big")
                .map_err(EventProcessorError::internal)?;

            let old_protocol_version =
                ProtocolSemanticVersion::try_from_packed(h256_to_u256(version))
                    .map_err(|err| EventProcessorError::internal(anyhow::anyhow!(err)))?;
            // In normal operation, `old_protocol_version` should equal `last_seen_protocol_version` exactly:
            // ServerNotifier validates that cut data exists for the current on-chain version before emitting
            // the event, so we should only ever see one pending upgrade at a time.
            if old_protocol_version < self.last_seen_protocol_version {
                // This version has been already processed, skip it.
                continue;
            }

            // The CTM emits a `NewProtocolVersion` event mapping the old protocol version (topic1)
            // to the new protocol version (topic2). Both CTM generations emit it, and it pins down
            // the block in which the upgrade was scheduled.
            let scheduled = self
                .sl_client
                .scheduled_protocol_version(old_protocol_version)
                .await
                .map_err(EventProcessorError::client)?
                .with_context(|| {
                    format!(
                        "No NewProtocolVersion event found for old protocol version \
                         {old_protocol_version} in the CTM look-back window (event {event:?})"
                    )
                })
                .map_err(EventProcessorError::internal)?;

            // Modern CTMs key `NewUpgradeCutData` by the *old* protocol version, which lets
            // governance rewrite the cut after the upgrade was scheduled. Legacy CTMs key it by the
            // *new* protocol version and cannot express a rewrite at all, so the new-version key is
            // only consulted when the old-version one yields nothing. Both look-ups start at the
            // scheduling block so that cuts emitted for earlier upgrades are not picked up — on a
            // modern CTM the previous upgrade also emitted a backwards-compatible
            // `NewUpgradeCutData` keyed by what is now the old version.
            let diamond_cut = match self
                .sl_client
                .diamond_cut_for_version(old_protocol_version, scheduled.block_number)
                .await
                .map_err(EventProcessorError::client)?
            {
                Some(diamond_cut) => diamond_cut,
                None => self
                    .sl_client
                    .diamond_cut_for_version(scheduled.version, scheduled.block_number)
                    .await
                    .map_err(EventProcessorError::client)?
                    .with_context(|| {
                        format!(
                            "No diamond cuts found for upgrade from protocol version \
                             {old_protocol_version} to {} scheduled in block {}",
                            scheduled.version, scheduled.block_number
                        )
                    })
                    .map_err(EventProcessorError::internal)?,
            };

            let upgrade = ProtocolUpgrade {
                timestamp,
                ..ProtocolUpgrade::try_from_diamond_cut(
                    &diamond_cut,
                    self.l1_client.as_ref(),
                    self.l1_client
                        .get_chain_gateway_upgrade_info()
                        .await
                        .map_err(EventProcessorError::contract_call)?,
                )
                .await
                .map_err(EventProcessorError::internal)?
            };

            if upgrade.version <= old_protocol_version {
                return Err(EventProcessorError::internal(anyhow::anyhow!(
                    "Upgrade from protocol version {old_protocol_version} points to non-newer version {}",
                    upgrade.version
                )));
            }

            if upgrade.version != scheduled.version {
                tracing::warn!(
                    "Diamond cut for the upgrade from {old_protocol_version} declares version {}, \
                     while the CTM scheduled {}",
                    upgrade.version,
                    scheduled.version
                );
            }

            // Modern CTMs set the verifier for the new protocol version on the CTM itself and emit
            // it via the `NewProtocolVersionVerifier` event next to the upgrade diamond cut. It
            // takes precedence over the verifier address in the upgrade data, which may be absent
            // (e.g. for verifier-only patch upgrades). Legacy CTMs have no such event, so there the
            // verifier from the upgrade data is the only source.
            let verifier_address = self
                .sl_client
                .verifier_address_for_version(upgrade.version, scheduled.block_number)
                .await
                .map_err(EventProcessorError::client)?
                .or(upgrade.verifier_address);

            // Scheduler VK is not present in proposal event. It is hard coded in verifier contract.
            let scheduler_vk_hash = if let Some(address) = verifier_address {
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
            let fflonk_scheduler_vk_hash = if let Some(address) = verifier_address {
                self.sl_client
                    .fflonk_scheduler_vk_hash(address)
                    .await
                    .map_err(EventProcessorError::contract_call)?
            } else {
                None
            };
            // Airbender SNARK-wrapper VK is likewise hard coded in the (dual) verifier contract;
            // `None` if the verifier doesn't route an Airbender verifier yet.
            let airbender_scheduler_vk_hash = if let Some(address) = verifier_address {
                self.sl_client
                    .airbender_scheduler_vk_hash(address)
                    .await
                    .map_err(EventProcessorError::contract_call)?
            } else {
                None
            };
            upgrades.insert(
                old_protocol_version,
                (
                    old_protocol_version,
                    upgrade,
                    scheduler_vk_hash,
                    fflonk_scheduler_vk_hash,
                    airbender_scheduler_vk_hash,
                ),
            );
        }

        let new_upgrades: Vec<_> = upgrades
            .into_values()
            .sorted_by_key(|(old_protocol_version, ..)| *old_protocol_version)
            .collect();

        let Some((_, last_upgrade, ..)) = new_upgrades.last() else {
            return Ok(events.len());
        };
        let versions: Vec<_> = new_upgrades
            .iter()
            .map(|(_, u, ..)| u.version.to_string())
            .collect();
        tracing::debug!("Received upgrades with versions: {versions:?}");

        let last_version = last_upgrade.version;
        let stage_latency = METRICS.poll_eth_node[&PollStage::PersistUpgrades].start();
        for (
            old_protocol_version,
            upgrade,
            scheduler_vk_hash,
            fflonk_scheduler_vk_hash,
            airbender_scheduler_vk_hash,
        ) in new_upgrades
        {
            let latest_semantic_version = storage
                .protocol_versions_dal()
                .latest_semantic_version()
                .await
                .map_err(DalError::generalize)
                .map_err(EventProcessorError::internal)?
                .context("expected some version to be present in DB")
                .map_err(EventProcessorError::internal)?;

            if old_protocol_version != latest_semantic_version {
                return Err(EventProcessorError::internal(anyhow::anyhow!(
                    "Received upgrade event for old_version={old_protocol_version}, but DB is at {latest_semantic_version}; missing prerequisite upgrade"
                )));
            }

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
                    airbender_scheduler_vk_hash,
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
        Ok(events.len())
    }

    fn topic1(&self) -> Option<H256> {
        Some(self.upgrade_timestamp_updated_signature)
    }

    fn topic2(&self) -> Option<H256> {
        Some(H256::from_low_u64_be(self.l2_chain_id.as_u64()))
    }

    fn event_source(&self) -> EventsSource {
        EventsSource::L1
    }

    fn event_type(&self) -> EventType {
        EventType::ProtocolUpgrades
    }
}

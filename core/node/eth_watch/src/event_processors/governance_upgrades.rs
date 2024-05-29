use anyhow::Context as _;
use zksync_dal::{Connection, Core, CoreDal, DalError};
use zksync_types::{
    ethabi::Contract, protocol_upgrade::GovernanceOperation, web3::Log, Address, ProtocolUpgrade,
    ProtocolVersionId, H256,
};

use crate::{
    client::EthClient,
    event_processors::{EventProcessor, EventProcessorError},
    metrics::{PollStage, METRICS},
};

/// Listens to operation events coming from the governance contract and saves new protocol upgrade proposals to the database.
#[derive(Debug)]
pub struct GovernanceUpgradesEventProcessor {
    // zkSync diamond proxy if pre-shared bridge; state transition manager if post shared bridge.
    target_contract_address: Address,
    /// Last protocol version seen. Used to skip events for already known upgrade proposals.
    last_seen_version_id: ProtocolVersionId,
    upgrade_proposal_signature: H256,
}

impl GovernanceUpgradesEventProcessor {
    pub fn new(
        target_contract_address: Address,
        last_seen_version_id: ProtocolVersionId,
        governance_contract: &Contract,
    ) -> Self {
        Self {
            target_contract_address,
            last_seen_version_id,
            upgrade_proposal_signature: governance_contract
                .event("TransparentOperationScheduled")
                .context("TransparentOperationScheduled event is missing in ABI")
                .unwrap()
                .signature(),
        }
    }
}

#[async_trait::async_trait]
impl EventProcessor for GovernanceUpgradesEventProcessor {
    async fn process_events(
        &mut self,
        storage: &mut Connection<'_, Core>,
        client: &dyn EthClient,
        events: Vec<Log>,
    ) -> Result<(), EventProcessorError> {
        let mut upgrades = Vec::new();
        for event in events {
            assert_eq!(event.topics[0], self.upgrade_proposal_signature); // guaranteed by the watcher

            let governance_operation = GovernanceOperation::try_from(event)
                .map_err(|err| EventProcessorError::log_parse(err, "governance operation"))?;
            // Some calls can target other contracts than Diamond proxy, skip them.
            for call in governance_operation
                .calls
                .into_iter()
                .filter(|call| call.target == self.target_contract_address)
            {
                // We might not get an upgrade operation here, but something else instead
                // (e.g. `acceptGovernor` call), so if parsing doesn't work, just skip the call.
                let Ok(upgrade) = ProtocolUpgrade::try_from(call) else {
                    tracing::warn!(
                        "Failed to parse governance operation call as protocol upgrade, skipping"
                    );
                    continue;
                };
                // Scheduler VK is not present in proposal event. It is hard coded in verifier contract.
                let scheduler_vk_hash = if let Some(address) = upgrade.verifier_address {
                    Some(client.scheduler_vk_hash(address).await?)
                } else {
                    None
                };
                upgrades.push((upgrade, scheduler_vk_hash));
            }
        }

        let new_upgrades: Vec<_> = upgrades
            .into_iter()
            .skip_while(|(v, _)| v.id as u16 <= self.last_seen_version_id as u16)
            .collect();

        let Some((last_upgrade, _)) = new_upgrades.last() else {
            return Ok(());
        };
        let ids: Vec<_> = new_upgrades.iter().map(|(u, _)| u.id as u16).collect();
        tracing::debug!("Received upgrades with ids: {ids:?}");

        let last_id = last_upgrade.id;
        let stage_latency = METRICS.poll_eth_node[&PollStage::PersistUpgrades].start();
        for (upgrade, scheduler_vk_hash) in new_upgrades {
            let previous_version = storage
                .protocol_versions_dal()
                .load_previous_version(upgrade.id)
                .await
                .map_err(DalError::generalize)?
                .with_context(|| {
                    format!(
                        "expected some version preceding {:?} to be present in DB",
                        upgrade.id
                    )
                })?;
            let new_version = previous_version.apply_upgrade(upgrade, scheduler_vk_hash);
            storage
                .protocol_versions_dal()
                .save_protocol_version_with_tx(&new_version)
                .await
                .map_err(DalError::generalize)?;
        }
        stage_latency.observe();

        self.last_seen_version_id = last_id;
        Ok(())
    }

    fn relevant_topic(&self) -> H256 {
        self.upgrade_proposal_signature
    }
}

use crate::eth_watch::{
    client::{Error, EthClient},
    event_processors::EventProcessor,
};
use std::convert::TryFrom;
use std::time::Instant;
use zksync_dal::StorageProcessor;
use zksync_types::{
    ethabi::Contract, protocol_version::GovernanceOperation, web3::types::Log, Address,
    ProtocolUpgrade, ProtocolVersionId, H256,
};

/// Listens to operation events coming from the governance contract and saves new protocol upgrade proposals to the database.
#[derive(Debug)]
pub struct GovernanceUpgradesEventProcessor {
    diamond_proxy_address: Address,
    /// Last protocol version seen. Used to skip events for already known upgrade proposals.
    last_seen_version_id: ProtocolVersionId,
    upgrade_proposal_signature: H256,
}

impl GovernanceUpgradesEventProcessor {
    pub fn new(
        diamond_proxy_address: Address,
        last_seen_version_id: ProtocolVersionId,
        governance_contract: &Contract,
    ) -> Self {
        Self {
            diamond_proxy_address,
            last_seen_version_id,
            upgrade_proposal_signature: governance_contract
                .event("TransparentOperationScheduled")
                .expect("TransparentOperationScheduled event is missing in abi")
                .signature(),
        }
    }
}

#[async_trait::async_trait]
impl<W: EthClient + Sync> EventProcessor<W> for GovernanceUpgradesEventProcessor {
    async fn process_events(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        client: &W,
        events: Vec<Log>,
    ) -> Result<(), Error> {
        let mut upgrades = Vec::new();
        for event in events
            .into_iter()
            .filter(|event| event.topics[0] == self.upgrade_proposal_signature)
        {
            let governance_operation = GovernanceOperation::try_from(event)
                .map_err(|err| Error::LogParse(format!("{:?}", err)))?;
            // Some calls can target other contracts than Diamond proxy, skip them.
            for call in governance_operation
                .calls
                .into_iter()
                .filter(|call| call.target == self.diamond_proxy_address)
            {
                // We might not get an upgrade operation here, but something else instead
                // (e.g. `acceptGovernor` call), so if parsing doesn't work, just skip the call.
                let Ok(upgrade) = ProtocolUpgrade::try_from(call) else {
                    tracing::warn!(
                        "Failed to parse governance operation call as protocol upgrade, skipping"
                    );
                    continue;
                };
                // Scheduler VK is not present in proposal event. It is hardcoded in verifier contract.
                let scheduler_vk_hash = if let Some(address) = upgrade.verifier_address {
                    Some(client.scheduler_vk_hash(address).await?)
                } else {
                    None
                };
                upgrades.push((upgrade, scheduler_vk_hash));
            }
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
        let stage_start = Instant::now();
        for (upgrade, scheduler_vk_hash) in new_upgrades {
            let previous_version = storage
                .protocol_versions_dal()
                .load_previous_version(upgrade.id)
                .await
                .unwrap_or_else(|| {
                    panic!(
                        "Expected some version preceding {:?} be present in DB",
                        upgrade.id
                    )
                });
            let new_version = previous_version.apply_upgrade(upgrade, scheduler_vk_hash);
            storage
                .protocol_versions_dal()
                .save_protocol_version_with_tx(new_version)
                .await;
        }
        metrics::histogram!("eth_watcher.poll_eth_node", stage_start.elapsed(), "stage" => "persist_upgrades");

        self.last_seen_version_id = last_id;

        Ok(())
    }

    fn relevant_topic(&self) -> H256 {
        self.upgrade_proposal_signature
    }
}

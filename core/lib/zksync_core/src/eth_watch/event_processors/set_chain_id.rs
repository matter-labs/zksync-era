use std::convert::TryFrom;
// use std::time::Instant;
use zksync_contracts::{governance_contract, state_transition_chain_contract, diamond_init_contract};
use zksync_dal::StorageProcessor;
use zksync_types::{
    protocol_version::GovernanceOperation, web3::types::Log, Address, ProtocolUpgrade,
    ProtocolVersionId, H256
};

use crate::eth_watch::{
    client::{Error, EthClient},
    event_processors::EventProcessor,
    metrics::{PollStage, METRICS},
};

/// Responsible for saving new protocol upgrade proposals to the database.
#[derive(Debug)]
pub struct SetChainIDEventProcessor {
    diamond_proxy_address: Address,
    last_seen_version_id: ProtocolVersionId,
    upgrade_proposal_signature: H256,
    set_chain_id_signature: H256,
    execute_upgrade_short_signature: [u8; 4],
}

impl SetChainIDEventProcessor {
    pub fn new(diamond_proxy_address: Address, last_seen_version_id: ProtocolVersionId) -> Self {
        Self {
            diamond_proxy_address,
            last_seen_version_id,
            upgrade_proposal_signature: governance_contract()
                .event("TransparentOperationScheduled")
                .expect("TransparentOperationScheduled event is missing in abi")
                .signature(),
            set_chain_id_signature: diamond_init_contract()
                .event("SetChainIdUpgrade")
                .expect("SetChainIdUpgrade event is missing in abi")
                .signature(),
            execute_upgrade_short_signature: state_transition_chain_contract()
                .function("executeUpgrade")
                .unwrap()
                .short_signature(),
        }
    }
}

#[async_trait::async_trait]
impl<W: EthClient + Sync> EventProcessor<W> for SetChainIDEventProcessor {
    async fn process_events(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        client: &W,
        events: Vec<Log>,
    ) -> Result<(), Error> {
        let mut upgrades = Vec::new();
        let events_iter = events.into_iter();

        // SetChainId does not go throught the governance contract, so we need to parse it separately.
        for event in events_iter.filter(|event| event.topics[0] == self.set_chain_id_signature) {
            tracing::debug!("KL todo Set chain id upgrade");

            let upgrade = ProtocolUpgrade::try_from(event)
                .map_err(|err| Error::LogParse(format!("{:?}", err)))?;
            tracing::debug!("KL todo Set chain id upgrade {:#?}", upgrade.clone());

            upgrades.push((upgrade, None));
            
        }

        if upgrades.is_empty() {
            return Ok(());
        }

        let ids_str: Vec<_> = upgrades
            .iter()
            .map(|(u, _)| format!("{}", u.id as u16))
            .collect();
        tracing::debug!("Received upgrades with ids: {}", ids_str.join(", "));

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

    fn relevant_topics(&self) -> H256 {
        self.set_chain_id_signature
    }
}

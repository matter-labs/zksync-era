use std::convert::TryFrom;
// use std::time::Instant;
use zksync_contracts::diamond_init_contract;
use zksync_dal::StorageProcessor;
use zksync_types::{web3::types::Log, ProtocolUpgrade, H256};

use crate::eth_watch::{
    client::{Error, EthClient},
    event_processors::EventProcessor,
    metrics::{PollStage, METRICS},
};

/// Responsible for saving new protocol upgrade proposals to the database.
#[derive(Debug)]
pub struct SetChainIDEventProcessor {
    set_chain_id_signature: H256,
}

impl SetChainIDEventProcessor {
    pub fn new() -> Self {
        Self {
            set_chain_id_signature: diamond_init_contract()
                .event("SetChainIdUpgrade")
                .expect("SetChainIdUpgrade event is missing in abi")
                .signature(),
        }
    }
}

#[async_trait::async_trait]
impl<W: EthClient + Sync> EventProcessor<W> for SetChainIDEventProcessor {
    async fn process_events(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        _client: &W,
        events: Vec<Log>,
    ) -> Result<(), Error> {
        let mut upgrades = Vec::new();
        let events_iter = events.into_iter();

        // SetChainId does not go throught the governance contract, so we need to parse it separately.
        for event in events_iter.filter(|event| event.topics[0] == self.set_chain_id_signature) {
            tracing::debug!("KL todo Set chain id upgrade");

            let upgrade = ProtocolUpgrade::try_from(event)
                .map_err(|err| Error::LogParse(format!("{:?}", err)))?;

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

        let stage_latency = METRICS.poll_eth_node[&PollStage::PersistUpgrades].start();
        for (upgrade, scheduler_vk_hash) in upgrades {
            let previous_version = storage
                .protocol_versions_dal()
                .get_protocol_version(upgrade.id)
                .await
                .expect("Expected the version to be in the DB");
            let new_version = previous_version.apply_upgrade(upgrade, scheduler_vk_hash);

            let mut db_transaction = storage.start_transaction().await.unwrap();
            if let Some(tx) = new_version.tx {
                db_transaction
                    .transactions_dal()
                    .insert_system_transaction(tx)
                    .await;
            }

            db_transaction.commit().await.unwrap();
        }
        stage_latency.observe();
        Ok(())
    }

    fn relevant_topic(&self) -> H256 {
        self.set_chain_id_signature
    }
}

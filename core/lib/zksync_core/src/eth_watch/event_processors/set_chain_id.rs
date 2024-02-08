use zksync_contracts::SET_CHAIN_ID_EVENT;
use zksync_dal::StorageProcessor;
use zksync_types::{protocol_version::decode_set_chain_id_event, web3::types::Log, Address, H256};

use crate::eth_watch::{
    client::{Error, EthClient},
    event_processors::EventProcessor,
    metrics::{PollStage, METRICS},
};

/// Responsible for saving `setChainId` upgrade transactions to the database.
#[derive(Debug)]
pub struct SetChainIDEventProcessor {
    /// Address of the `DiamondProxy` contract of the chain that the `SetChainId` event is for.
    diamond_proxy_address: Address,
    /// Signature of the `SetChainIdUpgrade` event.
    /// The event is emitted by the `StateTransitionManager` contract.
    set_chain_id_signature: H256,
}

impl SetChainIDEventProcessor {
    pub fn new(diamond_proxy_address: Address) -> Self {
        Self {
            diamond_proxy_address,
            set_chain_id_signature: SET_CHAIN_ID_EVENT.signature(),
        }
    }
}

#[async_trait::async_trait]
impl EventProcessor for SetChainIDEventProcessor {
    async fn process_events(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        _client: &dyn EthClient,
        events: Vec<Log>,
    ) -> Result<usize, Error> {
        // SetChainId does not go through the governance contract, so we need to parse it separately.
        let upgrades = events
            .into_iter()
            .filter(|log| {
                log.topics[0] == self.set_chain_id_signature
                    && log.topics[1] == self.diamond_proxy_address.into()
            })
            .map(|event| {
                decode_set_chain_id_event(event)
                    .map_err(|err| Error::LogParse(format!("{:?}", err)))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let upgrades_count = upgrades.len();
        if upgrades.is_empty() {
            return Ok(0);
        }

        let ids: Vec<_> = upgrades.iter().map(|(id, _tx)| *id as u16).collect();
        tracing::debug!("Received setChainId upgrade with version_id: {:?}", ids);

        let stage_latency = METRICS.poll_eth_node[&PollStage::PersistUpgrades].start();
        for (version_id, tx) in upgrades {
            storage
                .protocol_versions_dal()
                .save_genesis_upgrade_with_tx(version_id, tx)
                .await;
        }
        stage_latency.observe();
        Ok(upgrades_count)
    }

    fn relevant_topic(&self) -> H256 {
        self.set_chain_id_signature
    }
}

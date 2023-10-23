use crate::eth_watch::client::{Error, EthClient};
use zksync_dal::StorageProcessor;
use zksync_types::{web3::types::Log, H256};

pub mod governance_upgrades;
pub mod priority_ops;
pub mod upgrades;

#[async_trait::async_trait]
pub trait EventProcessor<W: EthClient + Sync>: Send + std::fmt::Debug {
    /// Processes given events
    async fn process_events(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        client: &W,
        events: Vec<Log>,
    ) -> Result<(), Error>;

    /// Relevant topic which defines what events to be processed
    fn relevant_topic(&self) -> H256;
}

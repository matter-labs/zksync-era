use std::fmt;

use zksync_dal::{Connection, Core};
use zksync_eth_client::{ContractCallError, EnrichedClientError};
use zksync_types::{web3::Log, H256};

pub(crate) use self::{
    decentralized_upgrades::DecentralizedUpgradesEventProcessor,
    governance_upgrades::GovernanceUpgradesEventProcessor, priority_ops::PriorityOpsEventProcessor,
};
use crate::client::EthClient;

mod decentralized_upgrades;
mod governance_upgrades;
pub mod priority_ops;

/// Errors issued by an [`EventProcessor`].
#[derive(Debug, thiserror::Error)]
pub(super) enum EventProcessorError {
    #[error("failed parsing a log into {log_kind}: {source:?}")]
    LogParse {
        log_kind: &'static str,
        #[source]
        source: anyhow::Error,
    },
    #[error("Eth client error: {0}")]
    Client(#[from] EnrichedClientError),
    #[error("Contract call error: {0}")]
    ContractCall(#[from] ContractCallError),
    /// Internal errors are considered fatal (i.e., they bubble up and lead to the watcher termination).
    #[error("internal processing error: {0:?}")]
    Internal(#[from] anyhow::Error),
}

impl EventProcessorError {
    pub fn log_parse(source: impl Into<anyhow::Error>, log_kind: &'static str) -> Self {
        Self::LogParse {
            log_kind,
            source: source.into(),
        }
    }
}

/// Processor for a single type of events emitted by the L1 contract. [`EthWatch`](crate::EthWatch)
/// feeds events to all processors one-by-one.
#[async_trait::async_trait]
pub(super) trait EventProcessor: 'static + fmt::Debug + Send + Sync {
    /// Processes given events. All events are guaranteed to match [`Self::relevant_topic()`].
    async fn process_events(
        &mut self,
        storage: &mut Connection<'_, Core>,
        client: &dyn EthClient,
        events: Vec<Log>,
    ) -> Result<(), EventProcessorError>;

    /// Relevant topic which defines what events to be processed
    fn relevant_topic(&self) -> H256;
}

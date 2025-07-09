use std::fmt;

use zksync_dal::{eth_watcher_dal::EventType, Connection, Core};
use zksync_eth_client::{ContractCallError, EnrichedClientError};
use zksync_types::{api::Log, H256};

pub(crate) use self::{
    appended_chain_batch_root::BatchRootProcessor,
    decentralized_upgrades::DecentralizedUpgradesEventProcessor,
    gateway_migration::GatewayMigrationProcessor, priority_ops::PriorityOpsEventProcessor,
};

mod appended_chain_batch_root;
mod decentralized_upgrades;
mod gateway_migration;
mod priority_ops;

/// Errors issued by an [`EventProcessor`].
#[derive(Debug, thiserror::Error)]
pub(super) enum EventProcessorError {
    #[error("Fatal: {0:?}")]
    Fatal(#[from] FatalError),
    #[error("Transient: {0:?}")]
    Transient(#[from] TransientError),
}

#[derive(Debug, thiserror::Error)]
pub(super) enum FatalError {
    #[error("failed parsing a log into {log_kind}: {source:?}")]
    LogParse {
        log_kind: &'static str,
        #[source]
        source: anyhow::Error,
    },
    /// Internal errors are considered fatal (i.e., they bubble up and lead to the watcher termination).
    #[error("internal processing error: {0:?}")]
    Internal(#[from] anyhow::Error),
}

#[derive(Debug, thiserror::Error)]
pub(super) enum TransientError {
    #[error("Eth client error: {0}")]
    Client(#[from] EnrichedClientError),
    #[error("Contract call error: {0}")]
    ContractCall(#[from] ContractCallError),
}

#[derive(Debug)]
pub(super) enum EventsSource {
    L1,
    SL,
}

impl EventProcessorError {
    pub fn log_parse(source: impl Into<anyhow::Error>, log_kind: &'static str) -> Self {
        Self::Fatal(FatalError::LogParse {
            log_kind,
            source: source.into(),
        })
    }

    pub fn internal(source: impl Into<anyhow::Error>) -> Self {
        Self::Fatal(FatalError::Internal(source.into()))
    }

    pub fn client(source: impl Into<EnrichedClientError>) -> Self {
        Self::Transient(TransientError::Client(source.into()))
    }

    pub fn contract_call(source: impl Into<ContractCallError>) -> Self {
        Self::Transient(TransientError::ContractCall(source.into()))
    }
}

/// Processor for a single type of events emitted by the L1 contract. [`EthWatch`](crate::EthWatch)
/// feeds events to all processors one-by-one.
#[async_trait::async_trait]
pub(super) trait EventProcessor: 'static + fmt::Debug + Send + Sync {
    /// Processes given events. All events are guaranteed to match [`Self::topic1()`] and [`Self::topic2()`].
    /// Returns number of processed events, this result is used to update last processed block.
    async fn process_events(
        &mut self,
        storage: &mut Connection<'_, Core>,
        events: Vec<Log>,
    ) -> Result<usize, EventProcessorError>;

    /// Relevant topic1 which defines what events to be processed
    fn topic1(&self) -> Option<H256>;

    /// Relevant topic2 which defines what events to be processed
    fn topic2(&self) -> Option<H256> {
        None
    }

    fn event_source(&self) -> EventsSource;

    fn event_type(&self) -> EventType;

    /// Whether processor expect events only from finalized blocks.
    fn only_finalized_block(&self) -> bool {
        false
    }
}

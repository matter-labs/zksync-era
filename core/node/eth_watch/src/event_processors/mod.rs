use std::fmt;

use zksync_dal::{Connection, Core};
use zksync_types::{web3::types::Log, H256};

use crate::client::{EthClient, EthClientError};

pub mod governance_upgrades;
pub mod priority_ops;
pub mod upgrades;

/// Errors issued by the
#[derive(Debug, thiserror::Error)]
pub(super) enum EventProcessorError {
    #[error("failed parsing a log into {log_kind}: {source:?}")]
    LogParse {
        log_kind: &'static str,
        #[source]
        source: anyhow::Error,
    },
    #[error("Eth client error: {0}")]
    Client(#[from] EthClientError),
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

#[async_trait::async_trait]
pub(super) trait EventProcessor: 'static + fmt::Debug + Send + Sync {
    /// Processes given events
    async fn process_events(
        &mut self,
        storage: &mut Connection<'_, Core>,
        client: &dyn EthClient,
        events: Vec<Log>,
    ) -> Result<(), EventProcessorError>;

    /// Relevant topic which defines what events to be processed
    fn relevant_topic(&self) -> H256;
}

use std::{fmt, time::Duration};

use thiserror::Error;
use tokio::sync::{oneshot, watch};
use zksync_config::configs::chain::CircuitBreakerConfig;

pub mod l1_txs;
mod metrics;
pub mod replication_lag;

#[derive(Debug, Error)]
pub enum CircuitBreakerError {
    #[error("System has failed L1 transaction")]
    FailedL1Transaction,
    #[error("Replication lag ({0}) is above the threshold ({1})")]
    ReplicationLag(u32, u32),
    #[error("Internal error running circuit breaker checks")]
    Internal(#[from] anyhow::Error),
}

/// Checks circuit breakers
#[derive(Debug)]
pub struct CircuitBreakerChecker {
    circuit_breakers: Vec<Box<dyn CircuitBreaker>>,
    sync_interval: Duration,
    sender: oneshot::Sender<CircuitBreakerError>,
}

#[async_trait::async_trait]
pub trait CircuitBreaker: fmt::Debug + Send + Sync {
    async fn check(&self) -> Result<(), CircuitBreakerError>;
}

impl CircuitBreakerChecker {
    pub fn new(
        circuit_breakers: Vec<Box<dyn CircuitBreaker>>,
        config: &CircuitBreakerConfig,
    ) -> (Self, oneshot::Receiver<CircuitBreakerError>) {
        let (sender, receiver) = oneshot::channel();
        let this = Self {
            circuit_breakers,
            sync_interval: config.sync_interval(),
            sender,
        };
        (this, receiver)
    }

    pub async fn check(&self) -> Result<(), CircuitBreakerError> {
        for circuit_breaker in &self.circuit_breakers {
            circuit_breaker.check().await?;
        }
        Ok(())
    }

    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        tracing::info!("running circuit breaker checker...");
        while !*stop_receiver.borrow_and_update() {
            if let Err(error) = self.check().await {
                return self
                    .sender
                    .send(error)
                    .map_err(|_| anyhow::anyhow!("failed to send circuit breaker message"));
            }
            // Error here corresponds to a timeout w/o `stop_receiver` changed; we're OK with this.
            tokio::time::timeout(self.sync_interval, stop_receiver.changed())
                .await
                .ok();
        }
        tracing::info!("received a stop signal; circuit breaker is shut down");
        Ok(())
    }
}

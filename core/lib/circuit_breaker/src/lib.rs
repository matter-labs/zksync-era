use std::time::Duration;

use anyhow::Context as _;
use futures::channel::oneshot;
use thiserror::Error;
use tokio::sync::watch;

use zksync_config::configs::chain::CircuitBreakerConfig;

pub mod l1_txs;
pub mod replication_lag;
pub mod utils;

#[derive(Debug, Error)]
pub enum CircuitBreakerError {
    #[error("System has failed L1 transaction")]
    FailedL1Transaction,
    #[error("Replication lag ({0:?}) is above the threshold ({1:?})")]
    ReplicationLag(u32, u32),
}

/// Checks circuit breakers
#[derive(Debug)]
pub struct CircuitBreakerChecker {
    circuit_breakers: Vec<Box<dyn CircuitBreaker>>,
    sync_interval: Duration,
}

#[async_trait::async_trait]
pub trait CircuitBreaker: std::fmt::Debug + Send + Sync {
    async fn check(&self) -> Result<(), CircuitBreakerError>;
}

impl CircuitBreakerChecker {
    pub fn new(
        circuit_breakers: Vec<Box<dyn CircuitBreaker>>,
        config: &CircuitBreakerConfig,
    ) -> Self {
        Self {
            circuit_breakers,
            sync_interval: config.sync_interval(),
        }
    }

    pub async fn check(&self) -> Result<(), CircuitBreakerError> {
        for circuit_breaker in &self.circuit_breakers {
            circuit_breaker.check().await?;
        }
        Ok(())
    }

    pub async fn run(
        self,
        circuit_breaker_sender: oneshot::Sender<CircuitBreakerError>,
        stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        tracing::info!("running circuit breaker checker...");
        loop {
            if *stop_receiver.borrow() {
                break;
            }
            if let Err(error) = self.check().await {
                return circuit_breaker_sender
                    .send(error)
                    .ok()
                    .context("failed to send circuit breaker messsage");
            }
            tokio::time::sleep(self.sync_interval).await;
        }
        Ok(())
    }
}

use std::{fmt, sync::Arc, time::Duration};

use thiserror::Error;
use tokio::sync::{watch, Mutex};

pub mod l1_txs;
mod metrics;
pub mod replication_lag;

#[derive(Default, Debug)]
pub struct CircuitBreakers(Mutex<Vec<Box<dyn CircuitBreaker>>>);

impl CircuitBreakers {
    pub async fn insert(&self, circuit_breaker: Box<dyn CircuitBreaker>) {
        let mut guard = self.0.lock().await;
        if !guard
            .iter()
            .any(|existing_breaker| existing_breaker.name() == circuit_breaker.name())
        {
            guard.push(circuit_breaker);
        }
    }

    pub async fn check(&self) -> Result<(), CircuitBreakerError> {
        for circuit_breaker in self.0.lock().await.iter() {
            circuit_breaker.check().await?;
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum CircuitBreakerError {
    #[error("System has failed L1 transaction")]
    FailedL1Transaction,
    #[error("Replication lag ({lag:?}) is above the threshold ({threshold:?})")]
    ReplicationLag { lag: Duration, threshold: Duration },
    #[error("Internal error running circuit breaker checks")]
    Internal(#[from] anyhow::Error),
}

impl From<zksync_dal::DalError> for CircuitBreakerError {
    fn from(err: zksync_dal::DalError) -> Self {
        Self::Internal(err.generalize())
    }
}

/// Checks circuit breakers
#[derive(Debug)]
pub struct CircuitBreakerChecker {
    circuit_breakers: Arc<CircuitBreakers>,
    sync_interval: Duration,
}

#[async_trait::async_trait]
pub trait CircuitBreaker: fmt::Debug + Send + Sync {
    fn name(&self) -> &'static str;

    async fn check(&self) -> Result<(), CircuitBreakerError>;
}

impl CircuitBreakerChecker {
    pub fn new(circuit_breakers: Arc<CircuitBreakers>, sync_interval: Duration) -> Self {
        Self {
            circuit_breakers,
            sync_interval,
        }
    }

    pub async fn check(&self) -> Result<(), CircuitBreakerError> {
        self.circuit_breakers.check().await?;

        Ok(())
    }

    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        tracing::info!("running circuit breaker checker...");
        while !*stop_receiver.borrow_and_update() {
            if let Err(error) = self.check().await {
                return Err(anyhow::format_err!(
                    "Circuit breaker error. Reason: {error}"
                ));
            }
            // Error here corresponds to a timeout w/o `stop_receiver` changed; we're OK with this.
            tokio::time::timeout(self.sync_interval, stop_receiver.changed())
                .await
                .ok();
        }
        tracing::info!("received a stop request; circuit breaker is shut down");
        Ok(())
    }
}

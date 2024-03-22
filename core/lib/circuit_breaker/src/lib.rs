use std::{sync::Arc, time::Duration};

use thiserror::Error;
use tokio::sync::{watch, Mutex};

pub mod l1_txs;
pub mod replication_lag;
pub mod utils;

#[derive(Default, Debug)]
pub struct CircuitBreakers(pub Mutex<Vec<Box<dyn CircuitBreaker>>>);

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
}

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
    circuit_breakers: Arc<CircuitBreakers>,
    sync_interval: Duration,
}

#[async_trait::async_trait]
pub trait CircuitBreaker: std::fmt::Debug + Send + Sync {
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
        for circuit_breaker in self.circuit_breakers.0.lock().await.iter() {
            circuit_breaker.check().await?;
        }
        Ok(())
    }

    pub async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        tracing::info!("running circuit breaker checker...");
        loop {
            if *stop_receiver.borrow() {
                break;
            }
            if let Err(error) = self.check().await {
                return Err(anyhow::format_err!(
                    "Circuit breaker error. Reason: {error}"
                ));
            }
            tokio::time::sleep(self.sync_interval).await;
        }
        Ok(())
    }
}

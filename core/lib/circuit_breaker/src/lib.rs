use std::sync::Arc;

use thiserror::Error;
use tokio::sync::{watch, Mutex};
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
    circuit_breakers: Mutex<Vec<Box<dyn CircuitBreaker>>>,
    config: CircuitBreakerConfig,
}

#[async_trait::async_trait]
pub trait CircuitBreaker: std::fmt::Debug + Send + Sync {
    fn id(&self) -> &'static str;

    async fn check(&self) -> Result<(), CircuitBreakerError>;
}

impl PartialEq for dyn CircuitBreaker {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl CircuitBreakerChecker {
    pub fn new(
        circuit_breakers: Option<Vec<Box<dyn CircuitBreaker>>>,
        config: &CircuitBreakerConfig,
    ) -> Self {
        Self {
            circuit_breakers: Mutex::new(circuit_breakers.unwrap_or_default()),
            config: config.clone(),
        }
    }

    pub fn replication_lag_limit_sec(&self) -> Option<u32> {
        self.config.replication_lag_limit_sec
    }

    pub async fn check(&self) -> Result<(), CircuitBreakerError> {
        for circuit_breaker in self.circuit_breakers.lock().await.iter() {
            circuit_breaker.check().await?;
        }
        Ok(())
    }

    pub async fn insert_breaker_if_not_exists(&self, circuit_breaker: Box<dyn CircuitBreaker>) {
        let mut guard = self.circuit_breakers.lock().await;
        if !guard.contains(&circuit_breaker) {
            guard.push(circuit_breaker);
        }
    }

    pub async fn run(self: Arc<Self>, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
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
            tokio::time::sleep(self.config.sync_interval()).await;
        }
        Ok(())
    }
}

use anyhow::Context as _;
use zksync_dal::{ConnectionPool, Core, CoreDal};

use crate::{metrics::METRICS, CircuitBreaker, CircuitBreakerError};

#[derive(Debug)]
pub struct ReplicationLagChecker {
    pub pool: ConnectionPool<Core>,
    pub replication_lag_limit_sec: Option<u32>,
}

#[async_trait::async_trait]
impl CircuitBreaker for ReplicationLagChecker {
    async fn check(&self) -> Result<(), CircuitBreakerError> {
        let lag = self
            .pool
            .connection_tagged("circuit_breaker")
            .await?
            .system_dal()
            .get_replication_lag_sec()
            .await
            .context("failed getting replication lag")?;
        METRICS.replication_lag.set(lag.into());

        match self.replication_lag_limit_sec {
            Some(replication_lag_limit_sec) if lag > replication_lag_limit_sec => Err(
                CircuitBreakerError::ReplicationLag(lag, replication_lag_limit_sec),
            ),
            _ => Ok(()),
        }
    }
}

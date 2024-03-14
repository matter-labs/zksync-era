use anyhow::Context as _;
use zksync_dal::ConnectionPool;

use crate::{metrics::METRICS, CircuitBreaker, CircuitBreakerError};

#[derive(Debug)]
pub struct ReplicationLagChecker {
    pub pool: ConnectionPool,
    pub replication_lag_limit_sec: Option<u32>,
}

#[async_trait::async_trait]
impl CircuitBreaker for ReplicationLagChecker {
    async fn check(&self) -> Result<(), CircuitBreakerError> {
        let lag = self
            .pool
            .access_storage()
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

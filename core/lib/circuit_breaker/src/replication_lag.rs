use zksync_dal::{ConnectionPool, Core, CoreDal};

use crate::{CircuitBreaker, CircuitBreakerError};

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
            .connection()
            .await
            .unwrap()
            .system_dal()
            .get_replication_lag_sec()
            .await;

        metrics::gauge!("circuit_breaker.replication_lag", lag as f64);
        match self.replication_lag_limit_sec {
            Some(replication_lag_limit_sec) if lag > replication_lag_limit_sec => Err(
                CircuitBreakerError::ReplicationLag(lag, replication_lag_limit_sec),
            ),
            _ => Ok(()),
        }
    }
}

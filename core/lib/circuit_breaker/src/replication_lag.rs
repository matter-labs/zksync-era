use std::time::Duration;

use zksync_dal::{ConnectionPool, Core, CoreDal, DalError};

use crate::{metrics::METRICS, CircuitBreaker, CircuitBreakerError};

#[derive(Debug)]
pub struct ReplicationLagChecker {
    pub pool: ConnectionPool<Core>,
    pub replication_lag_limit: Option<Duration>,
}

#[async_trait::async_trait]
impl CircuitBreaker for ReplicationLagChecker {
    fn name(&self) -> &'static str {
        "replication_lag"
    }

    async fn check(&self) -> Result<(), CircuitBreakerError> {
        let lag = self
            .pool
            .connection_tagged("circuit_breaker")
            .await?
            .system_dal()
            .get_replication_lag()
            .await
            .map_err(DalError::generalize)?;
        METRICS.replication_lag.set(lag);

        match self.replication_lag_limit {
            Some(threshold) if lag > threshold => {
                Err(CircuitBreakerError::ReplicationLag { lag, threshold })
            }
            _ => Ok(()),
        }
    }
}

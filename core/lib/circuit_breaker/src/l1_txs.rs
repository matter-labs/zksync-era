use anyhow::Context as _;
use zksync_dal::{ConnectionPool, Core, CoreDal};

use crate::{CircuitBreaker, CircuitBreakerError};

#[derive(Debug)]
pub struct FailedL1TransactionChecker {
    pub pool: ConnectionPool<Core>,
}

#[async_trait::async_trait]
impl CircuitBreaker for FailedL1TransactionChecker {
    async fn check(&self) -> Result<(), CircuitBreakerError> {
        let number_of_failed_transactions = self
            .pool
            .connection_tagged("circuit_breaker")
            .await?
            .eth_sender_dal()
            .get_number_of_failed_transactions()
            .await
            .context("cannot get number of failed L1 transactions")?;
        if number_of_failed_transactions > 0 {
            return Err(CircuitBreakerError::FailedL1Transaction);
        }
        Ok(())
    }
}

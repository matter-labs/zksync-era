use zksync_dal::{ConnectionPool, Core, CoreDal};

use crate::{CircuitBreaker, CircuitBreakerError};

#[derive(Debug)]
pub struct FailedL1TransactionChecker {
    pub pool: ConnectionPool<Core>,
}

#[async_trait::async_trait]
impl CircuitBreaker for FailedL1TransactionChecker {
    fn id(&self) -> &'static str {
        "failed_l1_transaction"
    }

    async fn check(&self) -> Result<(), CircuitBreakerError> {
        if self
            .pool
            .connection()
            .await
            .unwrap()
            .eth_sender_dal()
            .get_number_of_failed_transactions()
            .await
            .unwrap()
            > 0
        {
            return Err(CircuitBreakerError::FailedL1Transaction);
        }
        Ok(())
    }
}

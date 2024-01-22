use zksync_dal::ConnectionPool;

use crate::{CircuitBreaker, CircuitBreakerError};

#[derive(Debug)]
pub struct FailedL1TransactionChecker {
    pub pool: ConnectionPool,
}

#[async_trait::async_trait]
impl CircuitBreaker for FailedL1TransactionChecker {
    async fn check(&self) -> Result<(), CircuitBreakerError> {
        let storage = self.pool.access_storage().await.map_err(|_| CircuitBreakerError::StorageAccessFailed)?;
        let failed_transactions = storage.eth_sender_dal().get_number_of_failed_transactions().await.map_err(|_| CircuitBreakerError::FailedTransactionCheck)?;
        
        if failed_transactions > 0 {
            return Err(CircuitBreakerError::FailedL1Transaction);
        }
        Ok(())
    }
}

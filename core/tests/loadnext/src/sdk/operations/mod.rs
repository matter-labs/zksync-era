//! This file contains representation of not signed transactions and builders for them.

use std::time::{Duration, Instant};

use zksync_types::{
    api::{BlockNumber, TransactionReceipt},
    H256,
};

pub use self::{
    deploy_contract::DeployContractBuilder,
    execute_contract::ExecuteContractBuilder,
    transfer::{create_transfer_calldata, TransferBuilder},
    withdraw::WithdrawBuilder,
};
use crate::sdk::{error::ClientError, EthNamespaceClient};

mod deploy_contract;
mod execute_contract;
mod transfer;
mod withdraw;

/// Handle for transaction, providing an interface to control its execution.
/// For obtained handle it's possible to set the polling interval, commit timeout
/// and verify timeout values.
///
/// By default, awaiting for transaction may run up to forever, and the polling is
/// performed once a second.
#[derive(Debug)]
pub struct SyncTransactionHandle<'a, P> {
    hash: H256,
    provider: &'a P,
    polling_interval: Duration,
    commit_timeout: Option<Duration>,
    finalize_timeout: Option<Duration>,
}

impl<'a, P> SyncTransactionHandle<'a, P>
where
    P: EthNamespaceClient + Sync,
{
    pub fn new(hash: H256, provider: &'a P) -> Self {
        Self {
            hash,
            provider,
            polling_interval: Duration::from_secs(1), // 1 second.
            commit_timeout: None,                     // Wait until forever
            finalize_timeout: None,                   // Wait until forever
        }
    }

    const MIN_POLLING_INTERVAL: Duration = Duration::from_millis(200);

    /// Sets the polling interval. Must be at least 200 milliseconds.
    pub fn polling_interval(&mut self, polling_interval: Duration) -> Result<(), ClientError> {
        if polling_interval >= Self::MIN_POLLING_INTERVAL {
            self.polling_interval = polling_interval;
            Ok(())
        } else {
            Err(ClientError::PollingIntervalIsTooSmall)
        }
    }

    /// Returns the transaction hash.
    pub fn hash(&self) -> H256 {
        self.hash
    }

    /// Sets the timeout for commit operation.
    /// With this value set, `SyncTransactionHandle::wait_for_commit` will return a `ClientError::OperationTimeout`
    /// error if block will not be committed within provided time range.
    pub fn commit_timeout(mut self, commit_timeout: Duration) -> Self {
        self.commit_timeout = Some(commit_timeout);
        self
    }

    /// Sets the timeout for finalize operation.
    /// With this value set, `SyncTransactionHandle::wait_for_finalize` will return a `ClientError::OperationTimeout`
    /// error if block will not be finalized within provided time range.
    pub fn finalize_timeout(mut self, verify_timeout: Duration) -> Self {
        self.finalize_timeout = Some(verify_timeout);
        self
    }

    /// Awaits for the transaction commit and returns the information about its execution.
    pub async fn wait_for_commit(&self) -> Result<TransactionReceipt, ClientError> {
        self.wait_for(self.commit_timeout, BlockNumber::Committed)
            .await
    }

    /// Awaits for the transaction finalization and returns the information about its execution.
    pub async fn wait_for_finalize(&self) -> Result<TransactionReceipt, ClientError> {
        self.wait_for(self.finalize_timeout, BlockNumber::Finalized)
            .await
    }

    async fn wait_for(
        &self,
        timeout: Option<Duration>,
        status: BlockNumber,
    ) -> Result<TransactionReceipt, ClientError> {
        let mut timer = tokio::time::interval(self.polling_interval);
        let start = Instant::now();

        loop {
            timer.tick().await;

            if let Some(timeout) = timeout {
                if start.elapsed() >= timeout {
                    return Err(ClientError::OperationTimeout);
                }
            }

            let receipt =
                if let Some(receipt) = self.provider.get_transaction_receipt(self.hash).await? {
                    receipt
                } else {
                    continue;
                };

            // Wait for transaction to be included into the committed
            // or finalized block:
            // Fetch the latest block with the given status and
            // check if it's greater than or equal to the one from receipt.

            let block_number = receipt.block_number;

            let response = self.provider.get_block_by_number(status, false).await?;
            if let Some(received_number) = response.map(|block| block.number) {
                if block_number <= received_number {
                    return Ok(receipt);
                }
            }
        }
    }
}

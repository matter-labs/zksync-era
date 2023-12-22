use std::{fmt, time::Duration};

use anyhow::Context as _;
use async_trait::async_trait;
use tokio::sync::watch;
use zksync_contracts::PRE_BOOJUM_COMMIT_FUNCTION;
use zksync_dal::ConnectionPool;
use zksync_types::{
    web3::{self, ethabi, transports::Http, types::TransactionId, Web3},
    L1BatchNumber, H256, U64,
};

use crate::{
    metrics::{CheckerComponent, EN_METRICS},
    utils::wait_for_l1_batch_with_metadata,
};

#[cfg(test)]
mod tests;

#[derive(Debug, thiserror::Error)]
enum CheckError {
    #[error("Web3 error communicating with L1")]
    Web3(#[from] web3::Error),
    #[error("Internal error")]
    Internal(#[from] anyhow::Error),
}

impl From<zksync_dal::SqlxError> for CheckError {
    fn from(err: zksync_dal::SqlxError) -> Self {
        Self::Internal(err.into())
    }
}

#[async_trait]
trait L1Client: fmt::Debug + Send + Sync {
    async fn transaction_status(&self, tx_hash: H256) -> Result<Option<U64>, web3::Error>;

    /// **NB.** Must include the 4-byte Solidity method selector.
    async fn transaction_input_data(&self, tx_hash: H256) -> Result<Option<Vec<u8>>, web3::Error>;
}

#[async_trait]
impl L1Client for Web3<Http> {
    async fn transaction_status(&self, tx_hash: H256) -> Result<Option<U64>, web3::Error> {
        Ok(self
            .eth()
            .transaction_receipt(tx_hash)
            .await?
            .and_then(|receipt| receipt.status))
    }

    async fn transaction_input_data(&self, tx_hash: H256) -> Result<Option<Vec<u8>>, web3::Error> {
        let transaction = self.eth().transaction(TransactionId::Hash(tx_hash)).await?;
        Ok(transaction.map(|tx| tx.input.0))
    }
}

trait UpdateCheckedBatch: fmt::Debug + Send + Sync {
    fn update_checked_batch(&mut self, last_checked_batch: L1BatchNumber);
}

/// Default [`UpdateCheckedBatch`] implementation that reports the batch number as a metric.
impl UpdateCheckedBatch for () {
    fn update_checked_batch(&mut self, last_checked_batch: L1BatchNumber) {
        EN_METRICS.last_correct_batch[&CheckerComponent::ConsistencyChecker]
            .set(last_checked_batch.0.into());
    }
}

#[derive(Debug)]
pub struct ConsistencyChecker {
    /// ABI of the zkSync contract
    contract: ethabi::Contract,
    /// How many past batches to check when starting
    max_batches_to_recheck: u32,
    sleep_interval: Duration,
    l1_client: Box<dyn L1Client>,
    l1_batch_updater: Box<dyn UpdateCheckedBatch>,
    pool: ConnectionPool,
}

impl ConsistencyChecker {
    const DEFAULT_SLEEP_INTERVAL: Duration = Duration::from_secs(5);

    pub fn new(web3_url: &str, max_batches_to_recheck: u32, pool: ConnectionPool) -> Self {
        let web3 = Web3::new(Http::new(web3_url).unwrap());
        Self {
            contract: zksync_contracts::zksync_contract(),
            max_batches_to_recheck,
            sleep_interval: Self::DEFAULT_SLEEP_INTERVAL,
            l1_client: Box::new(web3),
            l1_batch_updater: Box::new(()),
            pool,
        }
    }

    async fn check_commitments(&self, batch_number: L1BatchNumber) -> Result<bool, CheckError> {
        let mut storage = self.pool.access_storage().await?;

        let storage_l1_batch = storage
            .blocks_dal()
            .get_storage_l1_batch(batch_number)
            .await?
            .with_context(|| format!("L1 batch #{batch_number} not found in the database"))?;
        let commit_tx_id = storage_l1_batch
            .eth_commit_tx_id
            .with_context(|| format!("Commit tx not found for L1 batch #{batch_number}"))?
            as u32;
        let block_metadata = storage
            .blocks_dal()
            .get_l1_batch_with_metadata(storage_l1_batch)
            .await?
            .with_context(|| {
                format!("Metadata for L1 batch #{batch_number} not found in the database")
            })?;
        let commit_tx_hash = storage
            .eth_sender_dal()
            .get_confirmed_tx_hash_by_eth_tx_id(commit_tx_id)
            .await?
            .with_context(|| {
                format!("Commit tx hash not found in the database for tx id {commit_tx_id}")
            })?;
        drop(storage);

        tracing::info!("Checking commit tx {commit_tx_hash} for L1 batch #{batch_number}");

        let commit_tx_status = self
            .l1_client
            .transaction_status(commit_tx_hash)
            .await?
            .with_context(|| format!("Receipt for tx {commit_tx_hash:?} not found on L1"))?;
        if commit_tx_status != 1.into() {
            let err = anyhow::anyhow!("Main node gave us a failed commit tx");
            return Err(err.into());
        }

        // We can't get tx calldata from db because it can be fake.
        let commit_tx_input_data = self
            .l1_client
            .transaction_input_data(commit_tx_hash)
            .await?
            .with_context(|| format!("Commit for tx {commit_tx_hash:?} not found on L1"))?;
        // FIXME: shouldn't the receiving contract and selector be checked as well?

        let is_pre_boojum = block_metadata
            .header
            .protocol_version
            .map_or(true, |ver| ver.is_pre_boojum());
        let commit_function = if is_pre_boojum {
            &*PRE_BOOJUM_COMMIT_FUNCTION
        } else {
            self.contract
                .function("commitBatches")
                .context("L1 contract does not have `commitBatches` function")?
        };

        let commitment =
            Self::extract_commit_data(&commit_tx_input_data, commit_function, batch_number)
                .with_context(|| {
                    format!("Failed extracting commit data for transaction {commit_tx_hash:?}")
                })?;
        Ok(commitment == block_metadata.l1_commit_data())
    }

    fn extract_commit_data(
        commit_tx_input_data: &[u8],
        commit_function: &ethabi::Function,
        batch_number: L1BatchNumber,
    ) -> anyhow::Result<ethabi::Token> {
        let mut commit_input_tokens = commit_function
            .decode_input(&commit_tx_input_data[4..])
            .with_context(|| format!("Failed decoding calldata for L1 commit function"))?;
        let mut commitments = commit_input_tokens
            .pop()
            .context("Unexpected signature for L1 commit function")?
            .into_array()
            .context("Unexpected signature for L1 commit function")?;

        // Commit transactions usually publish multiple commitments at once, so we need to find
        // the one that corresponds to the batch we're checking.
        let first_batch_commitment = commitments
            .first()
            .with_context(|| format!("L1 batch commitment is empty"))?;
        let ethabi::Token::Tuple(first_batch_commitment) = first_batch_commitment else {
            anyhow::bail!("Unexpected signature for L1 commit function");
        };
        let first_batch_number = first_batch_commitment
            .first()
            .context("Unexpected signature for L1 commit function")?;
        let first_batch_number = first_batch_number
            .clone()
            .into_uint()
            .context("Unexpected signature for L1 commit function")?;
        let first_batch_number = usize::try_from(first_batch_number)
            .map_err(|_| anyhow::anyhow!("Integer overflow for L1 batch number"))?;
        // ^ `TryFrom` has `&str` error here, so we can't use `.context()`.

        let commitment = (batch_number.0 as usize)
            .checked_sub(first_batch_number)
            .and_then(|offset| {
                (offset < commitments.len()).then(|| commitments.swap_remove(offset))
            });
        commitment.with_context(|| {
            let actual_range = first_batch_number..(first_batch_number + commitments.len());
            format!(
                "Malformed commitment data; it should prove L1 batch #{batch_number}, \
                 but it actually proves batches #{actual_range:?}"
            )
        })
    }

    async fn last_committed_batch(&self) -> anyhow::Result<Option<L1BatchNumber>> {
        Ok(self
            .pool
            .access_storage()
            .await?
            .blocks_dal()
            .get_number_of_last_l1_batch_committed_on_eth()
            .await?)
    }

    pub async fn run(mut self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        // It doesn't make sense to start the checker until we have at least one L1 batch with metadata.
        let earliest_l1_batch_number =
            wait_for_l1_batch_with_metadata(&self.pool, self.sleep_interval, &mut stop_receiver)
                .await?;

        let Some(earliest_l1_batch_number) = earliest_l1_batch_number else {
            return Ok(()); // Stop signal received
        };

        let last_committed_batch = self
            .last_committed_batch()
            .await?
            .unwrap_or(earliest_l1_batch_number);
        let first_batch_to_check: L1BatchNumber = last_committed_batch
            .0
            .saturating_sub(self.max_batches_to_recheck)
            .into();
        // We shouldn't check batches not present in the storage, and skip the genesis batch since
        // it's not committed on L1.
        let first_batch_to_check = first_batch_to_check
            .max(earliest_l1_batch_number)
            .max(L1BatchNumber(1));
        tracing::info!(
            "Last committed L1 batch is #{last_committed_batch}; starting checks from L1 batch #{first_batch_to_check}"
        );

        let mut batch_number = first_batch_to_check;
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, consistency_checker is shutting down");
                break;
            }

            let batch_with_metadata = self
                .pool
                .access_storage()
                .await?
                .blocks_dal()
                .get_l1_batch_metadata(batch_number)
                .await?;
            let batch_has_metadata = batch_with_metadata.map_or(false, |batch| {
                // FIXME: how does this work for pre-Boojum batches?
                batch
                    .metadata
                    .bootloader_initial_content_commitment
                    .is_some()
                    && batch.metadata.events_queue_commitment.is_some()
            });

            // The batch might be already committed but not yet processed by the external node's tree
            // OR the batch might be processed by the external node's tree but not yet committed.
            // We need both.
            if !batch_has_metadata || self.last_committed_batch().await? < Some(batch_number) {
                tokio::time::sleep(self.sleep_interval).await;
                continue;
            }

            match self.check_commitments(batch_number).await {
                Ok(true) => {
                    tracing::info!("L1 batch #{batch_number} is consistent with L1");
                    self.l1_batch_updater.update_checked_batch(batch_number);
                    batch_number += 1;
                }
                Ok(false) => {
                    anyhow::bail!("L1 Batch #{batch_number} is inconsistent with L1");
                }
                Err(CheckError::Web3(err)) => {
                    tracing::warn!("Error accessing L1; will retry after a delay: {err}");
                    tokio::time::sleep(self.sleep_interval).await;
                }
                Err(CheckError::Internal(err)) => {
                    let context =
                        format!("Failed verifying consistency of L1 batch #{batch_number}");
                    return Err(err.context(context));
                }
            }
        }
        Ok(())
    }
}

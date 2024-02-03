use std::{fmt, sync::Arc, time::Duration};

use anyhow::Context as _;
use tokio::sync::watch;
use zksync_contracts::PRE_BOOJUM_COMMIT_FUNCTION;
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_eth_client::{clients::QueryClient, Error as L1ClientError, EthInterface};
use zksync_l1_contract_interface::{i_executor::structures::CommitBatchInfo, Tokenizable};
use zksync_types::{
    l1_batch_commit_data_generator::L1BatchCommitDataGenerator, web3::ethabi, L1BatchNumber, H256,
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
    Web3(#[from] L1ClientError),
    #[error("Internal error")]
    Internal(#[from] anyhow::Error),
}

impl From<zksync_dal::SqlxError> for CheckError {
    fn from(err: zksync_dal::SqlxError) -> Self {
        Self::Internal(err.into())
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

/// Consistency checker behavior when L1 commit data divergence is detected.
// This is a temporary workaround for a bug that sometimes leads to incorrect L1 batch data returned by the server
// (and thus persisted by external nodes). Eventually, we want to go back to bailing on L1 data mismatch;
// for now, it's only enabled for the unit tests.
#[derive(Debug)]
enum L1DataMismatchBehavior {
    #[cfg(test)]
    Bail,
    Log,
}

/// L1 commit data loaded from Postgres.
#[derive(Debug)]
struct LocalL1BatchCommitData {
    is_pre_boojum: bool,
    l1_commit_data: ethabi::Token,
    commit_tx_hash: H256,
}

impl LocalL1BatchCommitData {
    /// Returns `Ok(None)` if Postgres doesn't contain all data necessary to check L1 commitment
    /// for the specified batch.
    async fn new(
        storage: &mut StorageProcessor<'_>,
        batch_number: L1BatchNumber,
        l1_batch_commit_data_generator: Arc<dyn L1BatchCommitDataGenerator>,
    ) -> anyhow::Result<Option<Self>> {
        let Some(storage_l1_batch) = storage
            .blocks_dal()
            .get_storage_l1_batch(batch_number)
            .await?
        else {
            return Ok(None);
        };

        let Some(commit_tx_id) = storage_l1_batch.eth_commit_tx_id else {
            return Ok(None);
        };
        let commit_tx_hash = storage
            .eth_sender_dal()
            .get_confirmed_tx_hash_by_eth_tx_id(commit_tx_id as u32)
            .await?
            .with_context(|| {
                format!("Commit tx hash not found in the database for tx id {commit_tx_id}")
            })?;

        let Some(l1_batch) = storage
            .blocks_dal()
            .get_l1_batch_with_metadata(storage_l1_batch)
            .await?
        else {
            return Ok(None);
        };

        let is_pre_boojum = l1_batch
            .header
            .protocol_version
            .map_or(true, |version| version.is_pre_boojum());
        let metadata = &l1_batch.metadata;

        // For Boojum batches, `bootloader_initial_content_commitment` and `events_queue_commitment`
        // are (temporarily) only computed by the metadata calculator if it runs with the full tree.
        // I.e., for these batches, we may have partial metadata in Postgres, which would not be sufficient
        // to compute local L1 commitment.
        if !is_pre_boojum
            && (metadata.bootloader_initial_content_commitment.is_none()
                || metadata.events_queue_commitment.is_none())
        {
            return Ok(None);
        }

        Ok(Some(Self {
            is_pre_boojum,
            l1_commit_data: CommitBatchInfo::new(&l1_batch, l1_batch_commit_data_generator)
                .into_token(),
            commit_tx_hash,
        }))
    }
}

#[derive(Debug)]
pub struct ConsistencyChecker {
    /// ABI of the zkSync contract
    contract: ethabi::Contract,
    /// How many past batches to check when starting
    max_batches_to_recheck: u32,
    sleep_interval: Duration,
    l1_client: Box<dyn EthInterface>,
    l1_batch_updater: Box<dyn UpdateCheckedBatch>,
    l1_data_mismatch_behavior: L1DataMismatchBehavior,
    pool: ConnectionPool,
}

impl ConsistencyChecker {
    const DEFAULT_SLEEP_INTERVAL: Duration = Duration::from_secs(5);

    pub fn new(web3_url: &str, max_batches_to_recheck: u32, pool: ConnectionPool) -> Self {
        let web3 = QueryClient::new(web3_url).unwrap();
        Self {
            contract: zksync_contracts::zksync_contract(),
            max_batches_to_recheck,
            sleep_interval: Self::DEFAULT_SLEEP_INTERVAL,
            l1_client: Box::new(web3),
            l1_batch_updater: Box::new(()),
            l1_data_mismatch_behavior: L1DataMismatchBehavior::Log,
            pool,
        }
    }

    async fn check_commitments(
        &self,
        batch_number: L1BatchNumber,
        local: &LocalL1BatchCommitData,
    ) -> Result<bool, CheckError> {
        let commit_tx_hash = local.commit_tx_hash;
        tracing::info!("Checking commit tx {commit_tx_hash} for L1 batch #{batch_number}");

        let commit_tx_status = self
            .l1_client
            .get_tx_status(commit_tx_hash, "consistency_checker")
            .await?
            .with_context(|| format!("Receipt for tx {commit_tx_hash:?} not found on L1"))?;
        if !commit_tx_status.success {
            let err = anyhow::anyhow!("Main node gave us a failed commit tx");
            return Err(err.into());
        }

        // We can't get tx calldata from db because it can be fake.
        let commit_tx_input_data = self
            .l1_client
            .get_tx(commit_tx_hash, "consistency_checker")
            .await?
            .with_context(|| format!("Commit for tx {commit_tx_hash:?} not found on L1"))?
            .input;
        // TODO (PLA-721): Check receiving contract and selector

        let commit_function = if local.is_pre_boojum {
            &*PRE_BOOJUM_COMMIT_FUNCTION
        } else {
            self.contract
                .function("commitBatches")
                .context("L1 contract does not have `commitBatches` function")?
        };
        let commitment =
            Self::extract_commit_data(&commit_tx_input_data.0, commit_function, batch_number)
                .with_context(|| {
                    format!("Failed extracting commit data for transaction {commit_tx_hash:?}")
                })?;
        Ok(commitment == local.l1_commit_data)
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

    pub async fn run(
        mut self,
        mut stop_receiver: watch::Receiver<bool>,
        l1_batch_commit_data_generator: Arc<dyn L1BatchCommitDataGenerator>,
    ) -> anyhow::Result<()> {
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

            let mut storage = self.pool.access_storage().await?;
            // The batch might be already committed but not yet processed by the external node's tree
            // OR the batch might be processed by the external node's tree but not yet committed.
            // We need both.
            let Some(local) = LocalL1BatchCommitData::new(
                &mut storage,
                batch_number,
                l1_batch_commit_data_generator.clone(),
            )
            .await?
            else {
                tokio::time::sleep(self.sleep_interval).await;
                continue;
            };
            drop(storage);

            match self.check_commitments(batch_number, &local).await {
                Ok(true) => {
                    tracing::info!("L1 batch #{batch_number} is consistent with L1");
                    self.l1_batch_updater.update_checked_batch(batch_number);
                    batch_number += 1;
                }
                Ok(false) => match &self.l1_data_mismatch_behavior {
                    #[cfg(test)]
                    L1DataMismatchBehavior::Bail => {
                        anyhow::bail!("L1 Batch #{batch_number} is inconsistent with L1");
                    }
                    L1DataMismatchBehavior::Log => {
                        tracing::warn!("L1 Batch #{batch_number} is inconsistent with L1");
                        batch_number += 1; // We don't want to infinitely loop failing the check on the same batch
                    }
                },
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

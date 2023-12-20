use std::time::Duration;

use anyhow::Context as _;
use zksync_contracts::PRE_BOOJUM_COMMIT_FUNCTION;
use zksync_dal::ConnectionPool;
use zksync_types::{
    web3::{self, ethabi, transports::Http, types::TransactionId, Web3},
    L1BatchNumber,
};

use crate::metrics::{CheckerComponent, EN_METRICS};

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

#[derive(Debug)]
pub struct ConsistencyChecker {
    // ABI of the zkSync contract
    contract: ethabi::Contract,
    // How many past batches to check when starting
    max_batches_to_recheck: u32,
    web3: Web3<Http>,
    db: ConnectionPool,
}

const SLEEP_DELAY: Duration = Duration::from_secs(5);

impl ConsistencyChecker {
    pub fn new(web3_url: &str, max_batches_to_recheck: u32, db: ConnectionPool) -> Self {
        let web3 = Web3::new(Http::new(web3_url).unwrap());
        let contract = zksync_contracts::zksync_contract();
        Self {
            web3,
            contract,
            max_batches_to_recheck,
            db,
        }
    }

    async fn check_commitments(&self, batch_number: L1BatchNumber) -> Result<bool, CheckError> {
        let mut storage = self.db.access_storage().await?;

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

        tracing::info!("Checking commit tx {commit_tx_hash} for L1 batch #{batch_number}");

        // We can't get tx calldata from db because it can be fake.
        let commit_tx = self
            .web3
            .eth()
            .transaction(TransactionId::Hash(commit_tx_hash))
            .await?
            .with_context(|| format!("Commit for tx {commit_tx_hash:?} not found on L1"))?;

        let commit_tx_status = self
            .web3
            .eth()
            .transaction_receipt(commit_tx_hash)
            .await?
            .with_context(|| format!("Receipt for tx {commit_tx_hash:?} not found on L1"))?
            .status;

        if commit_tx_status != Some(1.into()) {
            let err = anyhow::anyhow!("Main node gave us a failed commit tx");
            return Err(err.into());
        }

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

        let mut commit_input_tokens = commit_function
            .decode_input(&commit_tx.input.0[4..])
            .with_context(|| {
                format!("Failed decoding L1 commit function for transaction {commit_tx_hash:?}")
            })?;
        let commitments = commit_input_tokens
            .pop()
            .context("Unexpected signature for L1 commit function")?
            .into_array()
            .context("Unexpected signature for L1 commit function")?;

        // Commit transactions usually publish multiple commitments at once, so we need to find
        // the one that corresponds to the batch we're checking.
        let first_batch_commitment = commitments.first().with_context(|| {
            format!("L1 batch commitment in transaction {commit_tx_hash:?} is empty")
        })?;
        let ethabi::Token::Tuple(first_batch_commitment) = first_batch_commitment else {
            let err = anyhow::anyhow!("Unexpected signature for L1 commit function");
            return Err(err.into());
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
            .and_then(|offset| commitments.get(offset));
        let commitment = commitment.with_context(|| {
            let actual_range = first_batch_number..(first_batch_number + commitments.len());
            format!(
                "Malformed commitment data in transaction {commit_tx_hash:?}; it should prove L1 batch #{batch_number}, \
                 but it actually proves batches #{actual_range:?}"
            )
        })?;

        Ok(*commitment == block_metadata.l1_commit_data())
    }

    async fn last_committed_batch(&self) -> anyhow::Result<L1BatchNumber> {
        Ok(self
            .db
            .access_storage()
            .await?
            .blocks_dal()
            .get_number_of_last_l1_batch_committed_on_eth()
            .await?
            .unwrap_or(L1BatchNumber(0))) // FIXME: not always valid
    }

    pub async fn run(
        self,
        stop_receiver: tokio::sync::watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        let mut batch_number: L1BatchNumber = self
            .last_committed_batch()
            .await?
            .0
            .saturating_sub(self.max_batches_to_recheck)
            .max(1) // FIXME: not always valid
            .into();

        tracing::info!(
            "Starting consistency checker from L1 batch #{}",
            batch_number.0
        );

        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, consistency_checker is shutting down");
                break;
            }

            let batch_with_metadata = self
                .db
                .access_storage()
                .await?
                .blocks_dal()
                .get_l1_batch_metadata(batch_number)
                .await?;
            let batch_has_metadata = batch_with_metadata.map_or(false, |batch| {
                batch
                    .metadata
                    .bootloader_initial_content_commitment
                    .is_some()
                    && batch.metadata.events_queue_commitment.is_some()
            });

            // The batch might be already committed but not yet processed by the external node's tree
            // OR the batch might be processed by the external node's tree but not yet committed.
            // We need both.
            if !batch_has_metadata || self.last_committed_batch().await? < batch_number {
                tokio::time::sleep(SLEEP_DELAY).await;
                continue;
            }

            match self.check_commitments(batch_number).await {
                Ok(true) => {
                    tracing::info!("L1 batch #{batch_number} is consistent with L1");
                    EN_METRICS.last_correct_batch[&CheckerComponent::ConsistencyChecker]
                        .set(batch_number.0.into());
                    batch_number += 1;
                }
                Ok(false) => {
                    anyhow::bail!("L1 Batch #{batch_number} is inconsistent with L1");
                }
                Err(CheckError::Web3(err)) => {
                    tracing::warn!("Error accessing L1; will retry after a delay: {err}");
                    tokio::time::sleep(SLEEP_DELAY).await;
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

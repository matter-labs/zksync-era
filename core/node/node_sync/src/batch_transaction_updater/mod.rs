//! Component responsible for updating L1 batch status.

use std::{num::NonZeroU64, time::Duration};

use anyhow::Context;
use serde::Serialize;
use tokio::sync::watch;
use zksync_dal::{blocks_dal::L1BatchWithOptionalMetadata, ConnectionPool, Core, CoreDal};
use zksync_eth_client::EthInterface;
use zksync_health_check::{Health, HealthStatus, HealthUpdater, ReactiveHealthCheck};
use zksync_types::{
    aggregated_operations::{
        AggregatedActionType, L1BatchAggregatedActionType, L2BlockAggregatedActionType,
    },
    eth_sender::{EthTxFinalityStatus, L1BlockNumbers, TxHistory},
    web3::TransactionReceipt,
    Address, L1BatchNumber, L2BlockNumber, ProtocolVersionId, SLChainId,
};

use self::l1_transaction_verifier::L1TransactionVerifier;
use crate::batch_transaction_updater::l1_transaction_verifier::TransactionValidationError;

mod l1_transaction_verifier;

#[cfg(test)]
mod tests;

#[derive(Debug, Serialize)]
struct BatchTransactionUpdaterHealthDetails {
    unfinalized_txs_checked: usize,
}

const FIRST_VALIDATED_PROTOCOL_VERSION_ID: ProtocolVersionId = if cfg!(test) {
    ProtocolVersionId::latest() // necessary for tests
} else {
    ProtocolVersionId::Version29
};

/// Module responsible for updating L1 batch status. (Pending->FastFinalized->Finalized)
/// Its currently the component that should terminate EN when gateway migration happens.
/// The EN only is stopped when all batches from previous SL are finalized.
#[derive(Debug)]
pub struct BatchTransactionUpdater {
    sl_client: Box<dyn EthInterface>,
    l1_transaction_verifier: L1TransactionVerifier,
    pool: ConnectionPool<Core>,
    health_updater: HealthUpdater,
    sleep_interval: Duration,
    processing_batch_size: NonZeroU64,
}

impl BatchTransactionUpdater {
    pub fn new(
        sl_client: Box<dyn EthInterface>,
        diamond_proxy_addr: Address,
        pool: ConnectionPool<Core>,
        sleep_interval: Duration,
        processing_batch_size: NonZeroU64,
    ) -> Self {
        Self {
            sl_client,
            l1_transaction_verifier: L1TransactionVerifier::new(diamond_proxy_addr),
            pool,
            health_updater: ReactiveHealthCheck::new("batch_transaction_updater").1,
            sleep_interval,
            processing_batch_size,
        }
    }

    pub fn health_check(&self) -> ReactiveHealthCheck {
        self.health_updater.subscribe()
    }

    /// Applies the finality update for given transaction (eth_history_id) using its
    /// receipt under l1_block_numbers.
    /// On retryable error, returns Ok(false), but does not update DB.
    /// Validation errors are fatal.
    async fn apply_status_update(
        &self,
        db_eth_history_id: u32,
        receipt: TransactionReceipt,
        l1_block_numbers: &L1BlockNumbers,
    ) -> anyhow::Result<bool> {
        let updated_status =
            l1_block_numbers.get_finality_status_for_block(receipt.block_number.unwrap().as_u32());

        if updated_status == EthTxFinalityStatus::Pending {
            tracing::warn!(
                "Tried updating status of {} transaction that is still pending",
                receipt.transaction_hash
            );
            return Ok(false);
        }

        let mut connection = self
            .pool
            .connection_tagged("batch_transaction_updater")
            .await?;

        let eth_history_tx = connection
            .eth_sender_dal()
            .get_eth_tx_history_by_id(db_eth_history_id)
            .await?;

        // variable only for logging
        let for_blocks_or_batches = match eth_history_tx.tx_type {
            AggregatedActionType::L1Batch(tx_type) => {
                let batches = connection
                    .blocks_dal()
                    .get_l1_batches_statistics_for_eth_tx_id(eth_history_tx.eth_tx_id)
                    .await?;

                if batches.is_empty() {
                    anyhow::bail!(
                        "Transaction {} is not associated with any batch",
                        eth_history_tx.tx_hash
                    );
                }

                // validate the transaction against db
                for batch in &batches {
                    let batch_number = L1BatchNumber(batch.number);

                    let Some(protocol_version) = connection
                        .blocks_dal()
                        .get_batch_protocol_version_id(batch_number)
                        .await?
                    else {
                        tracing::debug!(
                            "Batch {} protocol version is not found in the database. Cannot verify transaction {} right now",
                            batch_number,
                            eth_history_tx.tx_hash
                        );
                        return Ok(false);
                    };

                    // Do not validate transactions for batches with protocol version before 29
                    if protocol_version < FIRST_VALIDATED_PROTOCOL_VERSION_ID {
                        continue;
                    }

                    let batch_metadata = match connection
                        .blocks_dal()
                        .get_optional_l1_batch_metadata(batch_number)
                        .await?
                    {
                        Some(L1BatchWithOptionalMetadata {
                            header: _,
                            metadata: Ok(batch_metadata),
                        }) => batch_metadata,
                        _ => {
                            tracing::debug!(
                            "Batch {} metadata is not found in the database. Cannot verify transaction {} right now",
                            batch_number,
                            eth_history_tx.tx_hash
                        );
                            return Ok(false);
                        }
                    };

                    match tx_type {
                        L1BatchAggregatedActionType::Commit => self
                            .l1_transaction_verifier
                            .validate_commit_tx(&receipt, batch_metadata, batch_number)?,
                        L1BatchAggregatedActionType::PublishProofOnchain => self
                            .l1_transaction_verifier
                            .validate_prove_tx(&receipt, batch_metadata, batch_number)?,
                        L1BatchAggregatedActionType::Execute => self
                            .l1_transaction_verifier
                            .validate_execute_tx(&receipt, batch_metadata, batch_number)?,
                    };
                    tracing::debug!(
                        "Verified transaction {} ({}) for batch {}",
                        receipt.transaction_hash,
                        eth_history_tx.tx_type,
                        batch_number
                    );
                }
                batches.iter().map(|batch| batch.number).collect::<Vec<_>>()
            }
            AggregatedActionType::L2Block(L2BlockAggregatedActionType::Precommit) => {
                let miniblocks = connection
                    .blocks_dal()
                    .get_l2_blocks_statistics_for_eth_tx_id(eth_history_tx.eth_tx_id)
                    .await?;

                if miniblocks.is_empty() {
                    anyhow::bail!(
                        "Transaction {} is not associated with any miniblock",
                        eth_history_tx.tx_hash
                    );
                }

                // a precommit tx is associated with a consecutive series of txs. We validate this property.

                let mut miniblock_numbers = miniblocks
                    .iter()
                    .map(|miniblock| miniblock.number)
                    .collect::<Vec<_>>();
                miniblock_numbers.sort();
                for i in 1..miniblock_numbers.len() {
                    if miniblock_numbers[i] != miniblock_numbers[i - 1] + 1 {
                        anyhow::bail!(
                            "Transaction {} is associated with non consecutive set of miniblocks: miniblock {} is not consecutive with miniblock {}",
                            eth_history_tx.tx_hash,
                            miniblock_numbers[i],
                            miniblock_numbers[i - 1]
                        );
                    }
                }

                // Precommit tx emits commitment to the rolling_tx_hash of the highest miniblock
                // So we validate against the highest miniblock
                let highest_miniblock_number = L2BlockNumber(
                    miniblocks
                        .iter()
                        .map(|miniblock| miniblock.number)
                        .max()
                        .unwrap(),
                ); // safe because we just checked that miniblocks is not empty

                // miniblock should be in db due to how we perform fetching
                let Some(miniblock_header) = connection
                    .blocks_dal()
                    .get_l2_block_header(highest_miniblock_number)
                    .await?
                else {
                    tracing::debug!(
                                "Miniblock {} is not found in the database. Cannot verify transaction {} right now",
                                highest_miniblock_number,
                                eth_history_tx.tx_hash
                            );
                    return Ok(false);
                };

                let result = self
                    .l1_transaction_verifier
                    .validate_precommit_tx(&receipt, miniblock_header);

                if let Err(err) = result {
                    match err {
                        TransactionValidationError::MissingExpectedLog { .. } => {
                            // allow retry in case of MissingExpectedLog error.
                            // We might now have synced precommit tx for the last
                            // miniblock in series. We can only validate when we
                            // have all the miniblocks associations in db
                            tracing::warn!(
                                "Transaction {} cannot be validated, because it is missing expected log for miniblock {}. We may need to have a higher miniblock to validate it",
                                eth_history_tx.tx_hash,
                                highest_miniblock_number
                            );
                            return Ok(false);
                        }
                        _ => {
                            // all other errors are critical
                            return Err(err.into());
                        }
                    }
                }

                miniblocks
                    .iter()
                    .map(|miniblock| miniblock.number)
                    .collect::<Vec<_>>()
            }
        };

        connection
            .eth_sender_dal()
            .confirm_tx(
                receipt.transaction_hash,
                updated_status,
                receipt.gas_used.unwrap_or_default(), // we don't care about gas used for synced transaction
            )
            .await?;

        tracing::info!(
            "Updated finality status for transaction {} ({} for {:?}) from {:?} to {:?}",
            eth_history_tx.tx_hash,
            eth_history_tx.tx_type,
            for_blocks_or_batches,
            eth_history_tx.eth_tx_finality_status,
            updated_status
        );

        Ok(true)
    }

    async fn update_statuses(
        &self,
        to_process: Vec<TxHistory>,
        l1_block_numbers: L1BlockNumbers,
    ) -> anyhow::Result<usize> {
        let mut updated_count: usize = 0;

        tracing::debug!("Checking {} unfinalized transactions", to_process.len());

        for mut eth_tx_history in to_process {
            // we save receipt here to avoid fetching multiple times
            let mut receipt: Option<TransactionReceipt> = None;

            // if we didn't see this transaction mined, fetch it
            let sent_at_block = match eth_tx_history.sent_at_block {
                Some(block) => block,
                None => {
                    receipt = match self.sl_client.tx_receipt(eth_tx_history.tx_hash).await {
                        Ok(receipt) => receipt,
                        Err(e) => {
                            tracing::warn!(
                                "Skipping transaction {} as failed to fetch transaction receipt: {}",
                                eth_tx_history.tx_hash,
                                e
                            );
                            continue;
                        }
                    };
                    match &receipt {
                        None => {
                            // transaction was not included, skip. We will try fetching on next iteration.
                            continue;
                        }
                        Some(receipt) => {
                            let sent_at_block: u32 = receipt.block_number.unwrap().as_u32();
                            eth_tx_history.sent_at_block = Some(sent_at_block);
                            self.pool
                                .connection_tagged("batch_transaction_updater")
                                .await?
                                .eth_sender_dal()
                                .set_sent_at_block(eth_tx_history.id, sent_at_block)
                                .await?;
                            sent_at_block
                        }
                    }
                }
            };

            // transaction as included, check if we can potentially update
            // its finality status. This value is untrusted as sent_at_block may have been cached
            let Some(finality_update) = l1_block_numbers
                .get_finality_update(eth_tx_history.eth_tx_finality_status, sent_at_block)
            else {
                continue;
            };

            // we can potentially update finality status, but we need to
            // validate all properties including block number as it may
            // have been cached
            let receipt = if receipt.is_none() {
                match self.sl_client.tx_receipt(eth_tx_history.tx_hash).await {
                    Ok(receipt) => receipt,
                    Err(e) => {
                        tracing::warn!(
                            "Skipping transaction {} as failed to fetch transaction receipt: {}",
                            eth_tx_history.tx_hash,
                            e
                        );
                        continue;
                    }
                }
            } else {
                receipt
            };

            let Some(receipt) = receipt else {
                tracing::warn!(
                            "Expected transaction {} to be mined at block {}, but transaction not mined at all on SL",
                            eth_tx_history.tx_hash,
                            sent_at_block
                        );
                self.pool
                    .connection_tagged("batch_transaction_updater")
                    .await?
                    .eth_sender_dal()
                    .unset_sent_at_block(eth_tx_history.id)
                    .await?;
                continue;
            };

            let eth_history_id = eth_tx_history.id;
            let result = self
                .apply_status_update(
                    eth_history_id,
                    receipt,
                    &l1_block_numbers,
                )
                .await
                .with_context(|| {
                    format!(
                        "Failed to update finality status for transaction {} with type {} from {:?} to {:?}",
                        eth_tx_history.tx_hash,
                        eth_tx_history.tx_type,
                        eth_tx_history.eth_tx_finality_status,
                        finality_update
                    )
                })?;

            if result {
                updated_count += 1;
            }
        }
        Ok(updated_count)
    }

    pub async fn loop_iteration(
        &self,
        sl_chain_id: SLChainId,
        l1_block_numbers: L1BlockNumbers,
    ) -> anyhow::Result<usize> {
        let mut connection = self
            .pool
            .connection_tagged("batch_transaction_updater")
            .await?;
        let to_process: Vec<TxHistory> = connection
            .eth_sender_dal()
            .get_unfinalized_transactions(self.processing_batch_size, Some(sl_chain_id))
            .await?;

        if to_process.is_empty() {
            // Check if there are any unfinalized transactions for other chain_ids.
            // If there are, we need to restart to make the node change the SL chain_id.
            let all_chain_ids_transactions = connection
                .eth_sender_dal()
                .get_unfinalized_transactions(NonZeroU64::new(1).unwrap(), None)
                .await?;
            if !all_chain_ids_transactions.is_empty()
                // following check is needed, because we might get a new transaction 
                // with proper chain_id on this SELECT due to a race condition
                && all_chain_ids_transactions[0].chain_id != Some(sl_chain_id)
            {
                anyhow::bail!(
                    "No batch transactions to process for chain id {} while there are some for {:?} chain_id.\
                        Error is thrown so node can restart and reload SL data. If node doesn't \
                        make any progress after restart, then it's bug, please contact developers.",
                    sl_chain_id,
                    all_chain_ids_transactions[0].chain_id
                );
            }
        }
        drop(connection);

        // unfinalized_txs_checked is the number of transactions considered for
        // finalization in this iteration. It is essentially the count of unfinalized
        // transactions, but including the transactions finalized in current iteration.
        self.health_updater
            .update(Health::from(HealthStatus::Ready).with_details(
                BatchTransactionUpdaterHealthDetails {
                    unfinalized_txs_checked: to_process.len(),
                },
            ));

        self.update_statuses(to_process, l1_block_numbers).await
    }

    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        self.health_updater
            .update(Health::from(HealthStatus::Ready));
        let sl_chain_id = self.sl_client.fetch_chain_id().await?;

        while !*stop_receiver.borrow_and_update() {
            let l1_block_numbers = self.sl_client.get_block_numbers(None).await?;
            let updates = self.loop_iteration(sl_chain_id, l1_block_numbers).await?;
            if updates == 0 {
                tracing::debug!("No updates made, waiting for the next iteration");
                if tokio::time::timeout(self.sleep_interval, stop_receiver.changed())
                    .await
                    .is_ok()
                {
                    break;
                }
            } else {
                tracing::debug!("Updated {} transactions", updates);
            }
        }

        tracing::info!("Stop request received, exiting the batch transaction updater routine");
        Ok(())
    }
}

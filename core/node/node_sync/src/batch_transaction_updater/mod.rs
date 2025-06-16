//! Component responsible for updating L1 batch status.

use std::time::Duration;

use tokio::sync::watch;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_eth_client::EthInterface;
use zksync_health_check::{Health, HealthStatus, HealthUpdater, ReactiveHealthCheck};
use zksync_types::{
    aggregated_operations::AggregatedActionType,
    eth_sender::{EthTxFinalityStatus, L1BlockNumbers, TxHistory},
    web3::TransactionReceipt,
    Address, SLChainId,
};

use self::l1_transaction_verifier::L1TransactionVerifier;

mod l1_transaction_verifier;

#[cfg(test)]
mod tests;

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
}

impl BatchTransactionUpdater {
    const DEFAULT_SLEEP_INTERVAL: Duration = Duration::from_secs(5);

    pub fn new(
        sl_client: Box<dyn EthInterface>,
        diamond_proxy_addr: Address,
        pool: ConnectionPool<Core>,
    ) -> Self {
        Self::from_parts(
            sl_client,
            diamond_proxy_addr,
            pool,
            Self::DEFAULT_SLEEP_INTERVAL,
        )
    }

    fn from_parts(
        sl_client: Box<dyn EthInterface>,
        diamond_proxy_addr: Address,
        pool: ConnectionPool<Core>,
        sleep_interval: Duration,
    ) -> Self {
        Self {
            sl_client,
            l1_transaction_verifier: L1TransactionVerifier::new(diamond_proxy_addr, pool.clone()),
            pool,
            health_updater: ReactiveHealthCheck::new("batch_transaction_updater").1,
            sleep_interval,
        }
    }

    pub fn health_check(&self) -> ReactiveHealthCheck {
        self.health_updater.subscribe()
    }

    async fn apply_status_update(
        &self,
        connection: &mut Connection<'_, Core>,
        db_eth_history_id: u32,
        receipt: TransactionReceipt,
        l1_block_numbers: &L1BlockNumbers,
    ) -> anyhow::Result<()> {
        let updated_status =
            l1_block_numbers.get_finality_status_for_block(receipt.block_number.unwrap().as_u32());

        if updated_status == EthTxFinalityStatus::Pending {
            anyhow::bail!(
                "Transaction {} is still pending on SL",
                receipt.transaction_hash
            );
        }

        let db_eth_history_tx = connection
            .eth_sender_dal()
            .get_eth_tx_history_by_id(db_eth_history_id)
            .await?;

        let batch_number = connection
            .blocks_dal()
            .get_l1_batch_number_by_eth_tx_id(db_eth_history_tx.eth_tx_id)
            .await?
            .expect("eth_tx_history row must match a l1 batch");

        // validate the transaction against db
        match db_eth_history_tx.tx_type {
            AggregatedActionType::Commit => {
                self.l1_transaction_verifier
                    .validate_commit_tx(&receipt, batch_number)
                    .await?
            }
            AggregatedActionType::PublishProofOnchain => {
                self.l1_transaction_verifier
                    .validate_prove_tx(&receipt, batch_number)
                    .await?
            }
            AggregatedActionType::Execute => {
                self.l1_transaction_verifier
                    .validate_execute_tx(&receipt, batch_number)
                    .await?
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
            "Updated finality status for transaction {} ({} for batch {}) from {:?} to {:?}",
            db_eth_history_tx.tx_hash,
            db_eth_history_tx.tx_type,
            batch_number,
            db_eth_history_tx.eth_tx_finality_status,
            updated_status
        );

        Ok(())
    }

    async fn update_statuses(
        &self,
        mut connection: Connection<'_, Core>,
        to_process: Vec<TxHistory>,
        l1_block_numbers: L1BlockNumbers,
    ) -> anyhow::Result<i32> {
        let mut updated_count = 0;

        tracing::debug!("Checking {} nonfinalized transactions", to_process.len());

        for mut db_eth_tx_history in to_process {
            // we save receipt here to avoid fetching multiple times
            let mut receipt: Option<TransactionReceipt> = None;

            // if we didn't see this transaction mined, fetch it
            let sent_at_block = match db_eth_tx_history.sent_at_block {
                Some(block) => block,
                None => {
                    receipt = self.sl_client.tx_receipt(db_eth_tx_history.tx_hash).await?;
                    match receipt {
                        None => {
                            // transaction was not included, skip. We will try fetching on next iteration.
                            continue;
                        }
                        Some(ref receipt) => {
                            let sent_at_block: u32 = receipt.block_number.unwrap().as_u32();
                            db_eth_tx_history.sent_at_block = Some(sent_at_block);
                            connection
                                .eth_sender_dal()
                                .set_sent_at_block(db_eth_tx_history.id, sent_at_block)
                                .await?;
                            sent_at_block
                        }
                    }
                }
            };

            // transaction as included, check if we can potentially update
            // its finality status. This value is untrusted as seen_at_block may have been cached
            let Some(finality_update) = l1_block_numbers
                .get_finality_update(db_eth_tx_history.eth_tx_finality_status, sent_at_block)
            else {
                continue;
            };

            // we can potentially update finality status, but we need to
            // validate all properties including block number as it may
            // have been cached
            let receipt = match receipt {
                Some(receipt) => receipt,
                None => {
                    let Some(receipt) =
                        self.sl_client.tx_receipt(db_eth_tx_history.tx_hash).await?
                    else {
                        tracing::warn!(
                            "Expected transaction {} to be mined at block {}, but transaction not mined at all on SL",
                            db_eth_tx_history.tx_hash,
                            db_eth_tx_history.sent_at_block.unwrap()
                        );
                        connection
                            .eth_sender_dal()
                            .unset_sent_at_block(db_eth_tx_history.id)
                            .await?;
                        continue;
                    };
                    receipt
                }
            };

            let db_eth_history_id = db_eth_tx_history.id;
            let result = self
                .apply_status_update(
                    &mut connection,
                    db_eth_history_id,
                    receipt,
                    &l1_block_numbers,
                )
                .await;

            if result.is_ok() {
                updated_count += 1;
            } else {
                tracing::warn!(
                    "Failed to update finality status for transaction {} with type {} from {:?} to {:?} due to error: {}",
                    db_eth_tx_history.tx_hash,
                    db_eth_tx_history.tx_type,
                    db_eth_tx_history.eth_tx_finality_status,
                    finality_update,
                    result.err().unwrap()
                );
            }
        }
        Ok(updated_count)
    }

    pub async fn step(
        &self,
        sl_chain_id: SLChainId,
        l1_block_numbers: L1BlockNumbers,
    ) -> anyhow::Result<i32> {
        let mut connection = self
            .pool
            .connection_tagged("batch_transaction_updater")
            .await?;
        let to_process: Vec<TxHistory> = connection
            .eth_sender_dal()
            .get_unfinalized_tranasctions(10_000, Some(sl_chain_id))
            .await?;

        if to_process.is_empty() {
            // Check if there are any unfinalized transactions for other chain_ids.
            // If there are, we need to restart to make the node change the SL chain_id.
            let all_chain_ids_transactions = connection
                .eth_sender_dal()
                .get_unfinalized_tranasctions(1, None)
                .await?;
            if !all_chain_ids_transactions.is_empty()
                // following check is needd, becouse we might get a new transaction 
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

        self.update_statuses(connection, to_process, l1_block_numbers)
            .await
    }

    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        self.health_updater
            .update(Health::from(HealthStatus::Ready));
        let sl_chain_id = self.sl_client.fetch_chain_id().await?;

        while !*stop_receiver.borrow_and_update() {
            let l1_block_numbers = self.sl_client.get_block_numbers(None).await?;
            let updates = self.step(sl_chain_id, l1_block_numbers).await?;
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

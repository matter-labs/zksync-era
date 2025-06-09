//! Component responsible for updating L1 batch status.

use std::time::Duration;

use tokio::sync::watch;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_eth_client::EthInterface;
use zksync_health_check::{Health, HealthStatus, HealthUpdater, ReactiveHealthCheck};
use zksync_types::{
    aggregated_operations::AggregatedActionType,
    eth_sender::EthTxFinalityStatus,
    web3::{BlockId, BlockNumber, TransactionReceipt},
    Address, H256, U64,
};
use zksync_web3_decl::error::EnrichedClientError;

use self::l1_transaction_verifier::L1TransactionVerifier;

mod l1_transaction_verifier;

#[cfg(test)]
mod tests;

struct SLBlockNumbers {
    fast_finality: U64,
    finalized: U64,
}

impl SLBlockNumbers {
    fn get_finality_status_for_block(&self, block_number: U64) -> EthTxFinalityStatus {
        if block_number <= self.finalized {
            EthTxFinalityStatus::Finalized
        } else if block_number <= self.fast_finality {
            EthTxFinalityStatus::FastFinalized
        } else {
            EthTxFinalityStatus::Pending
        }
    }

    /// returns new finality status if finality status changed
    fn get_finality_update(
        &self,
        current_status: EthTxFinalityStatus,
        included_at_block: U64,
    ) -> Option<EthTxFinalityStatus> {
        let finality_status = self.get_finality_status_for_block(included_at_block);
        if finality_status == current_status {
            return None;
        }
        Some(finality_status)
    }
}

const TRANSACTION_TYPES: [AggregatedActionType; 3] = [
    AggregatedActionType::Commit,
    AggregatedActionType::PublishProofOnchain,
    AggregatedActionType::Execute,
];

const UPDATABLE_FINALITY_STATUS: [EthTxFinalityStatus; 2] = [
    EthTxFinalityStatus::Pending,
    EthTxFinalityStatus::FastFinalized,
];

#[derive(Debug, Clone, Copy)]
struct NextToProcess {
    tx_hash: H256,
    db_eth_history_id: u32,
    seen_at_block: Option<U64>,
}

/// TODO DOC
#[derive(Debug)]
pub struct BatchTransactionUpdater {
    next_to_process:
        [[Option<NextToProcess>; TRANSACTION_TYPES.len()]; UPDATABLE_FINALITY_STATUS.len()],
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
            next_to_process: core::array::from_fn(|_| core::array::from_fn(|_| None)),
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

    async fn get_block_numbers(&self) -> Result<SLBlockNumbers, EnrichedClientError> {
        let finalized = self
            .sl_client
            .block(BlockId::Number(BlockNumber::Finalized))
            .await?
            .expect("Finalized block must be present on L1")
            .number
            .expect("Finalized block must contain number")
            .as_u32()
            .into();

        let fast_finality = self
            .sl_client
            .block(BlockId::Number(BlockNumber::Safe))
            .await?
            .expect("Safe block must be present on L1")
            .number
            .expect("Safe block must contain number")
            .as_u32()
            .into();

        Ok(SLBlockNumbers {
            finalized,
            fast_finality,
        })
    }

    async fn apply_status_update(
        &mut self,
        db_eth_history_id: u32,
        receipt: TransactionReceipt,
        sl_block_numbers: &SLBlockNumbers,
    ) -> anyhow::Result<()> {
        let updated_status =
            sl_block_numbers.get_finality_status_for_block(receipt.block_number.unwrap());

        let mut connection = self
            .pool
            .connection_tagged("batch_transaction_updater")
            .await?;

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
        let () = match db_eth_history_tx.tx_type {
            AggregatedActionType::Commit => {
                self.l1_transaction_verifier
                    .validate_commit_tx(receipt, batch_number)
                    .await?
            }
            AggregatedActionType::PublishProofOnchain => {
                self.l1_transaction_verifier
                    .validate_prove_tx(receipt, batch_number)
                    .await?
            }
            AggregatedActionType::Execute => {
                self.l1_transaction_verifier
                    .validate_execute_tx(receipt, batch_number)
                    .await?
            }
        };

        connection
            .eth_sender_dal()
            .mark_received_eth_tx_as_verified(db_eth_history_id, updated_status)
            .await?;
        Ok(())
    }

    async fn update_statuses(&mut self, sl_block_numbers: SLBlockNumbers) -> anyhow::Result<i32> {
        let mut updated_count = 0;
        for transaction_type in TRANSACTION_TYPES {
            for finality_status in UPDATABLE_FINALITY_STATUS {
                let to_process_entry =
                    &mut self.next_to_process[finality_status as usize][transaction_type as usize];

                // if not cached, try to load from DB
                if to_process_entry.is_none() {
                    let next_to_process = self
                        .pool
                        .connection_tagged("batch_transaction_updater")
                        .await?
                        .eth_sender_dal()
                        .get_oldest_tx_by_status_and_type(finality_status, transaction_type)
                        .await?;
                    *to_process_entry = next_to_process.map(|tx| NextToProcess {
                        tx_hash: tx.tx_hash,
                        db_eth_history_id: tx.id,
                        seen_at_block: None,
                    });
                }

                if to_process_entry.is_none() {
                    continue;
                }

                let current_entry = to_process_entry.as_mut().unwrap();

                // we save receipt here to avoid fetching multiple times
                let mut receipt: Option<TransactionReceipt> = None;

                // if we didn't see this transaction mined, fetch it
                if current_entry.seen_at_block.is_none() {
                    receipt = self.sl_client.tx_receipt(current_entry.tx_hash).await?;
                    match receipt {
                        None => {
                            // transaction was not included, skip. We will try fetching on next iteration.
                            continue;
                        }
                        Some(ref receipt) => {
                            current_entry.seen_at_block = Some(receipt.block_number.unwrap());
                        }
                    }
                }

                // transaction as included, check if we can potentially update
                // its finality status. This value is untrusted as seen_at_block may have been cached
                let finality_update = sl_block_numbers.get_finality_update(
                    finality_status,
                    current_entry.seen_at_block.unwrap(), //must be `Some` at this point
                );
                if finality_update.is_none() {
                    continue;
                }

                // we can potentially update finality status, but we need to
                // validate all properties including block number as it may
                // have been cached
                if receipt.is_none() {
                    // only fetch if it was not fetched in this iteration
                    receipt = self.sl_client.tx_receipt(current_entry.tx_hash).await?;
                }
                match receipt {
                    None => {
                        tracing::warn!(
                            "Expected transaction {} to be mined at block {}, but receipt not returnned by SL. Skipping finality status update",
                            current_entry.tx_hash,
                            current_entry.seen_at_block.unwrap()
                        );
                        current_entry.seen_at_block = None; // removed to refetch on next iteration
                        continue;
                    }
                    Some(receipt) => {
                        tracing::debug!(
                            "Updating finality status for transaction {} with status {} and type {}",
                            current_entry.tx_hash,
                            finality_status,
                            transaction_type
                        );
                        let db_eth_history_id = current_entry.db_eth_history_id;
                        let result = self
                            .apply_status_update(db_eth_history_id, receipt, &sl_block_numbers)
                            .await;
                        if result.is_ok() {
                            updated_count += 1;
                            // cannot use to_process_entry becouse apply_status_update borrows self
                            self.next_to_process[finality_status as usize]
                                [transaction_type as usize] = None;
                        }
                    }
                }
            }
        }
        Ok(updated_count)
    }

    pub async fn run(mut self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        self.health_updater
            .update(Health::from(HealthStatus::Ready));

        while !*stop_receiver.borrow_and_update() {
            let sl_block_numbers = self.get_block_numbers().await?;
            let updates = self.update_statuses(sl_block_numbers).await?;

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

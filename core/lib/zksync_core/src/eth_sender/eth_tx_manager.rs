use anyhow::Context as _;
use tokio::sync::watch;

use std::sync::Arc;
use std::time::Duration;

use zksync_config::configs::eth_sender::SenderConfig;
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_eth_client::{
    types::{Error, ExecutedTxStatus, SignedCallResult},
    BoundEthInterface,
};
use zksync_types::{
    eth_sender::EthTx,
    web3::{contract::Options, error::Error as Web3Error},
    L1BlockNumber, Nonce, H256, U256,
};
use zksync_utils::time::seconds_since_epoch;

use super::{metrics::METRICS, ETHSenderError};
use crate::l1_gas_price::L1TxParamsProvider;
use crate::metrics::BlockL1Stage;

#[derive(Debug)]
struct EthFee {
    base_fee_per_gas: u64,
    priority_fee_per_gas: u64,
}

#[derive(Debug, Clone, Copy)]
struct OperatorNonce {
    // Nonce on finalized block
    finalized: Nonce,
    // Nonce on latest block
    latest: Nonce,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct L1BlockNumbers {
    pub finalized: L1BlockNumber,
    pub latest: L1BlockNumber,
}

/// The component is responsible for managing sending eth_txs attempts:
/// Based on eth_tx queue the component generates new attempt with the minimum possible fee,
/// save it to the database, and send it to ethereum.
/// Based on eth_tx_history queue the component can mark txs as stuck and create the new attempt
/// with higher gas price
#[derive(Debug)]
pub struct EthTxManager<E, G> {
    ethereum_gateway: E,
    config: SenderConfig,
    gas_adjuster: Arc<G>,
}

impl<E, G> EthTxManager<E, G>
where
    E: BoundEthInterface + Sync,
    G: L1TxParamsProvider,
{
    pub fn new(config: SenderConfig, gas_adjuster: Arc<G>, ethereum_gateway: E) -> Self {
        Self {
            ethereum_gateway,
            config,
            gas_adjuster,
        }
    }

    async fn get_tx_status(
        &self,
        tx_hash: H256,
    ) -> Result<Option<ExecutedTxStatus>, ETHSenderError> {
        self.ethereum_gateway
            .get_tx_status(tx_hash, "eth_tx_manager")
            .await
            .map_err(Into::into)
    }

    async fn check_all_sending_attempts(
        &self,
        storage: &mut StorageProcessor<'_>,
        op: &EthTx,
    ) -> Option<ExecutedTxStatus> {
        // Checking history items, starting from most recently sent.
        for history_item in storage
            .eth_sender_dal()
            .get_tx_history_to_check(op.id)
            .await
            .unwrap()
        {
            // `status` is a Result here and we don't unwrap it with `?`
            // because if we do and get an `Err`, we won't finish the for loop,
            // which means we might miss the transaction that actually succeeded.
            match self.get_tx_status(history_item.tx_hash).await {
                Ok(Some(s)) => return Some(s),
                Ok(_) => continue,
                Err(err) => tracing::warn!(
                    "Can't check transaction {:?}: {:?}",
                    history_item.tx_hash,
                    err
                ),
            }
        }
        None
    }

    async fn calculate_fee(
        &self,
        storage: &mut StorageProcessor<'_>,
        tx: &EthTx,
        time_in_mempool: u32,
    ) -> Result<EthFee, ETHSenderError> {
        let base_fee_per_gas = self.gas_adjuster.get_base_fee(time_in_mempool);

        let priority_fee_per_gas = if time_in_mempool != 0 {
            METRICS.transaction_resent.inc();
            let priority_fee_per_gas = self
                .increase_priority_fee(storage, tx.id, base_fee_per_gas)
                .await?;
            tracing::info!(
                "Resending operation {} with base fee {:?} and priority fee {:?}",
                tx.id,
                base_fee_per_gas,
                priority_fee_per_gas
            );
            priority_fee_per_gas
        } else {
            self.gas_adjuster.get_priority_fee()
        };

        // Extra check to prevent sending transaction will extremely high priority fee.
        if priority_fee_per_gas > self.config.max_acceptable_priority_fee_in_gwei {
            panic!(
                "Extremely high value of priority_fee_per_gas is suggested: {}, while max acceptable is {}",
                priority_fee_per_gas,
                self.config.max_acceptable_priority_fee_in_gwei
            );
        }

        Ok(EthFee {
            base_fee_per_gas,
            priority_fee_per_gas,
        })
    }

    async fn increase_priority_fee(
        &self,
        storage: &mut StorageProcessor<'_>,
        eth_tx_id: u32,
        base_fee_per_gas: u64,
    ) -> Result<u64, ETHSenderError> {
        let previous_sent_tx = storage
            .eth_sender_dal()
            .get_last_sent_eth_tx(eth_tx_id)
            .await
            .unwrap()
            .unwrap();

        let previous_base_fee = previous_sent_tx.base_fee_per_gas;
        let previous_priority_fee = previous_sent_tx.priority_fee_per_gas;
        let next_block_minimal_base_fee = self.gas_adjuster.get_next_block_minimal_base_fee();

        if base_fee_per_gas <= next_block_minimal_base_fee.min(previous_base_fee) {
            // If the base fee is lower than the previous used one
            // or is lower than the minimal possible value for the next block, sending is skipped.
            tracing::info!(
                "Skipping gas adjustment for operation {}, \
                 base_fee_per_gas: suggested for resending {:?}, previously sent {:?}, next block minimum {:?}",
                eth_tx_id,
                base_fee_per_gas,
                previous_base_fee,
                next_block_minimal_base_fee
            );
            return Err(ETHSenderError::from(Error::from(Web3Error::Internal)));
        }

        // Increase `priority_fee_per_gas` by at least 20% to prevent "replacement transaction underpriced" error.
        Ok((previous_priority_fee + (previous_priority_fee / 5) + 1)
            .max(self.gas_adjuster.get_priority_fee()))
    }

    pub(crate) async fn send_eth_tx(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        tx: &EthTx,
        time_in_mempool: u32,
        current_block: L1BlockNumber,
    ) -> Result<H256, ETHSenderError> {
        let EthFee {
            base_fee_per_gas,
            priority_fee_per_gas,
        } = self.calculate_fee(storage, tx, time_in_mempool).await?;

        METRICS.used_base_fee_per_gas.observe(base_fee_per_gas);
        METRICS
            .used_priority_fee_per_gas
            .observe(priority_fee_per_gas);

        let signed_tx = self
            .sign_tx(tx, base_fee_per_gas, priority_fee_per_gas)
            .await;

        if let Some(tx_history_id) = storage
            .eth_sender_dal()
            .insert_tx_history(
                tx.id,
                base_fee_per_gas,
                priority_fee_per_gas,
                signed_tx.hash,
                signed_tx.raw_tx.clone(),
            )
            .await
            .unwrap()
        {
            if let Err(error) = self
                .send_raw_transaction(storage, tx_history_id, signed_tx.raw_tx, current_block)
                .await
            {
                tracing::warn!(
                    "Error when sending new signed tx for tx {}, base_fee_per_gas {}, priority_fee_per_gas: {}: {}",
                    tx.id,
                    base_fee_per_gas,
                    priority_fee_per_gas,
                    error
                );
            }
        }
        Ok(signed_tx.hash)
    }

    async fn send_raw_transaction(
        &self,
        storage: &mut StorageProcessor<'_>,
        tx_history_id: u32,
        raw_tx: Vec<u8>,
        current_block: L1BlockNumber,
    ) -> Result<H256, ETHSenderError> {
        match self.ethereum_gateway.send_raw_tx(raw_tx).await {
            Ok(tx_hash) => {
                storage
                    .eth_sender_dal()
                    .set_sent_at_block(tx_history_id, current_block.0)
                    .await
                    .unwrap();
                Ok(tx_hash)
            }
            Err(error) => {
                storage
                    .eth_sender_dal()
                    .remove_tx_history(tx_history_id)
                    .await
                    .unwrap();
                Err(error.into())
            }
        }
    }

    async fn get_operator_nonce(
        &self,
        block_numbers: L1BlockNumbers,
    ) -> Result<OperatorNonce, ETHSenderError> {
        let finalized = self
            .ethereum_gateway
            .nonce_at(block_numbers.finalized.0.into(), "eth_tx_manager")
            .await?
            .as_u32()
            .into();

        let latest = self
            .ethereum_gateway
            .nonce_at(block_numbers.latest.0.into(), "eth_tx_manager")
            .await?
            .as_u32()
            .into();
        Ok(OperatorNonce { finalized, latest })
    }

    async fn get_l1_block_numbers(&self) -> Result<L1BlockNumbers, ETHSenderError> {
        let finalized = if let Some(confirmations) = self.config.wait_confirmations {
            let latest_block_number = self
                .ethereum_gateway
                .block_number("eth_tx_manager")
                .await?
                .as_u64();
            (latest_block_number.saturating_sub(confirmations) as u32).into()
        } else {
            self.ethereum_gateway
                .block("finalized".to_string(), "eth_tx_manager")
                .await?
                .expect("Finalized block must be present on L1")
                .number
                .expect("Finalized block must contain number")
                .as_u32()
                .into()
        };

        let latest = self
            .ethereum_gateway
            .block_number("eth_tx_manager")
            .await?
            .as_u32()
            .into();
        Ok(L1BlockNumbers { finalized, latest })
    }

    // Monitors the inflight transactions, marks mined ones as confirmed,
    // returns the one that has to be resent (if there is one).
    pub(super) async fn monitor_inflight_transactions(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        l1_block_numbers: L1BlockNumbers,
    ) -> Result<Option<(EthTx, u32)>, ETHSenderError> {
        METRICS
            .last_known_l1_block
            .set(l1_block_numbers.latest.0.into());
        let operator_nonce = self.get_operator_nonce(l1_block_numbers).await?;
        let inflight_txs = storage.eth_sender_dal().get_inflight_txs().await.unwrap();
        METRICS.number_of_inflight_txs.set(inflight_txs.len());

        tracing::trace!(
            "Going through not confirmed txs. \
             Block numbers: latest {}, finalized {}, \
             operator's nonce: latest {}, finalized {}",
            l1_block_numbers.latest,
            l1_block_numbers.finalized,
            operator_nonce.latest,
            operator_nonce.finalized,
        );

        // Not confirmed transactions, ordered by nonce
        for tx in inflight_txs {
            tracing::trace!("Checking tx id: {}", tx.id,);

            // If the `operator_nonce.latest` <= `tx.nonce`, this means
            // that `tx` is not mined and we should resend it.
            // We only resend the first unmined transaction.
            if operator_nonce.latest <= tx.nonce {
                // None means txs hasn't been sent yet
                let first_sent_at_block = storage
                    .eth_sender_dal()
                    .get_block_number_on_first_sent_attempt(tx.id)
                    .await
                    .unwrap()
                    .unwrap_or(l1_block_numbers.latest.0);
                return Ok(Some((tx, first_sent_at_block)));
            }

            // If on finalized block sender's nonce was > tx.nonce,
            // then `tx` is mined and confirmed (either successful or reverted).
            // Only then we will check the history to find the receipt.
            // Otherwise, `tx` is mined but not confirmed, so we skip to the next one.
            if operator_nonce.finalized <= tx.nonce {
                continue;
            }

            tracing::trace!(
                "Sender's nonce on finalized block is greater than current tx's nonce. \
                 Checking transaction with id {}. Tx nonce is equal to {}",
                tx.id,
                tx.nonce,
            );

            match self.check_all_sending_attempts(storage, &tx).await {
                Some(tx_status) => {
                    self.apply_tx_status(storage, &tx, tx_status, l1_block_numbers.finalized)
                        .await;
                }
                None => {
                    // The nonce has increased but we did not find the receipt.
                    // This is an error because such a big reorg may cause transactions that were
                    // previously recorded as confirmed to become pending again and we have to
                    // make sure it's not the case - otherwise eth_sender may not work properly.
                    tracing::error!(
                        "Possible block reorgs: finalized nonce increase detected, but no tx receipt found for tx {:?}",
                        &tx
                    );
                }
            }
        }
        Ok(None)
    }

    async fn sign_tx(
        &self,
        tx: &EthTx,
        base_fee_per_gas: u64,
        priority_fee_per_gas: u64,
    ) -> SignedCallResult {
        self.ethereum_gateway
            .sign_prepared_tx_for_addr(
                tx.raw_tx.clone(),
                tx.contract_address,
                Options::with(|opt| {
                    // TODO Calculate gas for every operation SMA-1436
                    opt.gas = Some(self.config.max_aggregated_tx_gas.into());
                    opt.max_fee_per_gas = Some(U256::from(base_fee_per_gas + priority_fee_per_gas));
                    opt.max_priority_fee_per_gas = Some(U256::from(priority_fee_per_gas));
                    opt.nonce = Some(tx.nonce.0.into());
                }),
                "eth_tx_manager",
            )
            .await
            .expect("Failed to sign transaction")
    }

    async fn send_unsent_txs(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        l1_block_numbers: L1BlockNumbers,
    ) {
        for tx in storage.eth_sender_dal().get_unsent_txs().await.unwrap() {
            // Check already sent txs not marked as sent and mark them as sent.
            // The common reason for this behaviour is that we sent tx and stop the server
            // before updating the database
            let tx_status = self.get_tx_status(tx.tx_hash).await;

            if let Ok(Some(tx_status)) = tx_status {
                tracing::info!("The tx {:?} has been already sent", tx.tx_hash);
                storage
                    .eth_sender_dal()
                    .set_sent_at_block(tx.id, tx_status.receipt.block_number.unwrap().as_u32())
                    .await
                    .unwrap();

                let eth_tx = storage
                    .eth_sender_dal()
                    .get_eth_tx(tx.eth_tx_id)
                    .await
                    .unwrap()
                    .expect("Eth tx should exist");

                self.apply_tx_status(storage, &eth_tx, tx_status, l1_block_numbers.finalized)
                    .await;
            } else if let Err(error) = self
                .send_raw_transaction(
                    storage,
                    tx.id,
                    tx.signed_raw_tx.clone(),
                    l1_block_numbers.latest,
                )
                .await
            {
                tracing::warn!("Error {:?} in sending tx {:?}", error, &tx);
            }
        }
    }

    async fn apply_tx_status(
        &self,
        storage: &mut StorageProcessor<'_>,
        tx: &EthTx,
        tx_status: ExecutedTxStatus,
        finalized_block: L1BlockNumber,
    ) {
        let receipt_block_number = tx_status.receipt.block_number.unwrap().as_u32();
        if receipt_block_number <= finalized_block.0 {
            if tx_status.success {
                self.confirm_tx(storage, tx, tx_status).await;
            } else {
                self.fail_tx(storage, tx, tx_status).await;
            }
        } else {
            tracing::debug!(
                "Transaction {} with id {} is not yet finalized: block in receipt {receipt_block_number}, finalized block {finalized_block}",
                tx_status.tx_hash,
                tx.id,
            );
        }
    }

    pub async fn fail_tx(
        &self,
        storage: &mut StorageProcessor<'_>,
        tx: &EthTx,
        tx_status: ExecutedTxStatus,
    ) {
        storage
            .eth_sender_dal()
            .mark_failed_transaction(tx.id)
            .await
            .unwrap();
        let failure_reason = self
            .ethereum_gateway
            .failure_reason(tx_status.receipt.transaction_hash)
            .await
            .expect(
                "Tx is already failed, it's safe to fail here and apply the status on the next run",
            );

        tracing::error!(
            "Eth tx failed {:?}, {:?}, failure reason {:?}",
            tx,
            tx_status.receipt,
            failure_reason
        );
        panic!("We can't operate after tx fail");
    }

    pub async fn confirm_tx(
        &self,
        storage: &mut StorageProcessor<'_>,
        tx: &EthTx,
        tx_status: ExecutedTxStatus,
    ) {
        let tx_hash = tx_status.receipt.transaction_hash;
        let gas_used = tx_status
            .receipt
            .gas_used
            .expect("light ETH clients are not supported");

        storage
            .eth_sender_dal()
            .confirm_tx(tx_status.tx_hash, gas_used)
            .await
            .unwrap();

        METRICS
            .track_eth_tx_metrics(storage, BlockL1Stage::Mined, tx)
            .await;

        if gas_used > U256::from(tx.predicted_gas_cost) {
            tracing::error!(
                "Predicted gas {} lower than used gas {gas_used} for tx {:?} {}",
                tx.predicted_gas_cost,
                tx.tx_type,
                tx.id
            );
        }
        tracing::info!(
            "eth_tx {} with hash {tx_hash:?} for {} is confirmed. Gas spent: {gas_used:?}",
            tx.id,
            tx.tx_type
        );
        let tx_type_label = tx.tx_type.into();
        METRICS.l1_gas_used[&tx_type_label].observe(gas_used.low_u128() as f64);
        METRICS.l1_tx_mined_latency[&tx_type_label].observe(Duration::from_secs(
            seconds_since_epoch() - tx.created_at_timestamp,
        ));

        let sent_at_block = storage
            .eth_sender_dal()
            .get_block_number_on_first_sent_attempt(tx.id)
            .await
            .unwrap()
            .unwrap_or(0);
        let waited_blocks = tx_status.receipt.block_number.unwrap().as_u32() - sent_at_block;
        METRICS.l1_blocks_waited_in_mempool[&tx_type_label].observe(waited_blocks.into());
    }

    pub async fn run(
        mut self,
        pool: ConnectionPool,
        stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        {
            let l1_block_numbers = self
                .get_l1_block_numbers()
                .await
                .context("get_l1_block_numbers()")?;
            let mut storage = pool.access_storage_tagged("eth_sender").await.unwrap();
            self.send_unsent_txs(&mut storage, l1_block_numbers).await;
        }

        // It's mandatory to set last_known_l1_block to zero, otherwise the first iteration
        // will never check inflight txs status
        let mut last_known_l1_block = L1BlockNumber(0);
        loop {
            let mut storage = pool.access_storage_tagged("eth_sender").await.unwrap();

            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, eth_tx_manager is shutting down");
                break;
            }

            match self.loop_iteration(&mut storage, last_known_l1_block).await {
                Ok(block) => last_known_l1_block = block,
                Err(e) => {
                    // Web3 API request failures can cause this,
                    // and anything more important is already properly reported.
                    tracing::warn!("eth_sender error {:?}", e);
                }
            }

            tokio::time::sleep(self.config.tx_poll_period()).await;
        }
        Ok(())
    }

    async fn send_new_eth_txs(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        current_block: L1BlockNumber,
    ) {
        let number_inflight_txs = storage
            .eth_sender_dal()
            .get_inflight_txs()
            .await
            .unwrap()
            .len();
        let number_of_available_slots_for_eth_txs = self
            .config
            .max_txs_in_flight
            .saturating_sub(number_inflight_txs as u64);

        if number_of_available_slots_for_eth_txs > 0 {
            // Get the new eth tx and create history item for them
            let new_eth_tx = storage
                .eth_sender_dal()
                .get_new_eth_txs(number_of_available_slots_for_eth_txs)
                .await
                .unwrap();

            for tx in new_eth_tx {
                let _ = self.send_eth_tx(storage, &tx, 0, current_block).await;
            }
        }
    }

    #[tracing::instrument(skip(self, storage))]
    async fn loop_iteration(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        previous_block: L1BlockNumber,
    ) -> Result<L1BlockNumber, ETHSenderError> {
        let l1_block_numbers = self.get_l1_block_numbers().await?;

        self.send_new_eth_txs(storage, l1_block_numbers.latest)
            .await;

        if l1_block_numbers.latest <= previous_block {
            // Nothing to do - no new blocks were mined.
            return Ok(previous_block);
        }

        if let Some((tx, sent_at_block)) = self
            .monitor_inflight_transactions(storage, l1_block_numbers)
            .await?
        {
            // New gas price depends on the time this tx spent in mempool.
            let time_in_mempool = l1_block_numbers.latest.0 - sent_at_block;

            // We don't want to return early in case resend does not succeed -
            // the error is logged anyway, but early returns will prevent
            // sending new operations.
            let _ = self
                .send_eth_tx(storage, &tx, time_in_mempool, l1_block_numbers.latest)
                .await;
        }

        Ok(l1_block_numbers.latest)
    }
}

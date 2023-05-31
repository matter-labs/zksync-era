use std::sync::Arc;
use tokio::sync::watch;

use zksync_config::configs::eth_sender::SenderConfig;
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_eth_client::{
    types::{Error, ExecutedTxStatus, SignedCallResult},
    BoundEthInterface,
};
use zksync_types::{
    eth_sender::EthTx,
    web3::{contract::Options, error::Error as Web3Error},
    L1BlockNumber, H256, U256,
};
use zksync_utils::time::seconds_since_epoch;

use crate::eth_sender::ETHSenderError;
use crate::{eth_sender::grafana_metrics::track_eth_tx_metrics, l1_gas_price::L1TxParamsProvider};

#[derive(Debug)]
struct EthFee {
    base_fee_per_gas: u64,
    priority_fee_per_gas: u64,
}

#[derive(Debug)]
struct OperatorNonce {
    // Nonce on block `current_block - self.wait_confirmations`
    lagging: u64,
    // Nonce on block `current_block`
    current: u64,
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

    async fn get_tx_status_and_confirmations_count(
        &self,
        tx_hash: H256,
        current_block: L1BlockNumber,
    ) -> Result<Option<(ExecutedTxStatus, u64)>, ETHSenderError> {
        let status = self
            .ethereum_gateway
            .get_tx_status(tx_hash, "eth_tx_manager")
            .await?;
        if let Some(status) = status {
            // Amount of confirmations for a block containing the transaction.
            let confirmations = (current_block.0 as u64)
                .saturating_sub(status.receipt.block_number.unwrap().as_u64());
            return Ok(Some((status, confirmations)));
        }
        Ok(None)
    }

    async fn check_all_sending_attempts(
        &self,
        storage: &mut StorageProcessor<'_>,
        op: &EthTx,
        current_block: L1BlockNumber,
    ) -> Option<(ExecutedTxStatus, u64)> {
        // Checking history items, starting from most recently sent.
        for history_item in storage.eth_sender_dal().get_tx_history_to_check(op.id) {
            // `status` is a Result here and we don't unwrap it with `?`
            // because if we do and get an `Err`, we won't finish the for loop,
            // which means we might miss the transaction that actually succeeded.
            match self
                .get_tx_status_and_confirmations_count(history_item.tx_hash, current_block)
                .await
            {
                Ok(Some(s)) => return Some(s),
                Ok(_) => continue,
                Err(err) => vlog::warn!("Can't check transaction {:?}", err),
            }
        }
        None
    }

    fn calculate_fee(
        &self,
        storage: &mut StorageProcessor<'_>,
        tx: &EthTx,
        time_in_mempool: u32,
    ) -> Result<EthFee, ETHSenderError> {
        let base_fee_per_gas = self.gas_adjuster.get_base_fee(time_in_mempool);

        let priority_fee_per_gas = if time_in_mempool != 0 {
            metrics::increment_counter!("server.eth_sender.transaction_resent");
            let priority_fee_per_gas =
                self.increase_priority_fee(storage, tx.id, base_fee_per_gas)?;
            vlog::info!(
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

    fn increase_priority_fee(
        &self,
        storage: &mut StorageProcessor<'_>,
        eth_tx_id: u32,
        base_fee_per_gas: u64,
    ) -> Result<u64, ETHSenderError> {
        let previous_sent_tx = storage
            .eth_sender_dal()
            .get_last_sent_eth_tx(eth_tx_id)
            .unwrap();

        let previous_base_fee = previous_sent_tx.base_fee_per_gas;
        let previous_priority_fee = previous_sent_tx.priority_fee_per_gas;
        let next_block_minimal_base_fee = self.gas_adjuster.get_next_block_minimal_base_fee();

        if base_fee_per_gas <= next_block_minimal_base_fee.min(previous_base_fee) {
            // If the base fee is lower than the previous used one
            // or is lower than the minimal possible value for the next block, sending is skipped.
            vlog::info!(
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
        } = self.calculate_fee(storage, tx, time_in_mempool)?;

        metrics::histogram!(
            "server.eth_sender.used_base_fee_per_gas",
            base_fee_per_gas as f64
        );

        metrics::histogram!(
            "server.eth_sender.used_priority_fee_per_gas",
            priority_fee_per_gas as f64
        );

        let signed_tx = self
            .sign_tx(tx, base_fee_per_gas, priority_fee_per_gas)
            .await;

        if let Some(tx_history_id) = storage.eth_sender_dal().insert_tx_history(
            tx.id,
            base_fee_per_gas,
            priority_fee_per_gas,
            signed_tx.hash,
            signed_tx.raw_tx.clone(),
        ) {
            if let Err(error) = self
                .send_raw_transaction(storage, tx_history_id, signed_tx.raw_tx, current_block)
                .await
            {
                vlog::warn!(
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
                    .set_sent_at_block(tx_history_id, current_block.0);
                Ok(tx_hash)
            }
            Err(error) => {
                storage.eth_sender_dal().remove_tx_history(tx_history_id);
                Err(error.into())
            }
        }
    }

    async fn get_operator_nonce(
        &self,
        current_block: L1BlockNumber,
    ) -> Result<OperatorNonce, ETHSenderError> {
        let lagging = self
            .ethereum_gateway
            .nonce_at(
                current_block
                    .saturating_sub(self.config.wait_confirmations as u32)
                    .into(),
                "eth_tx_manager",
            )
            .await?
            .as_u64();

        let current = self
            .ethereum_gateway
            .current_nonce("eth_tx_manager")
            .await?
            .as_u64();
        Ok(OperatorNonce { lagging, current })
    }

    // Monitors the inflight transactions, marks mined ones as confirmed,
    // returns the one that has to be resent (if there is one).
    pub(super) async fn monitor_inflight_transactions(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        current_block: L1BlockNumber,
    ) -> Result<Option<(EthTx, u32)>, ETHSenderError> {
        metrics::gauge!(
            "server.eth_sender.last_known_l1_block",
            current_block.0 as f64
        );

        let operator_nonce = self.get_operator_nonce(current_block).await?;

        let inflight_txs = storage.eth_sender_dal().get_inflight_txs();
        metrics::gauge!(
            "server.eth_sender.number_of_inflight_txs",
            inflight_txs.len() as f64,
        );

        // Not confirmed transactions, ordered by nonce
        for tx in inflight_txs {
            vlog::trace!(
                "Going through not confirmed txs. \
                 Current block: {}, current tx id: {}, \
                 sender's nonce on block `current block - number of confirmations`: {}",
                current_block,
                tx.id,
                operator_nonce.lagging
            );

            // If the `current_sender_nonce` <= `tx.nonce`, this means
            // that `tx` is not mined and we should resend it.
            // We only resend the first unmined transaction.
            if operator_nonce.current <= tx.nonce {
                // None means txs hasn't been sent yet
                let first_sent_at_block = storage
                    .eth_sender_dal()
                    .get_block_number_on_first_sent_attempt(tx.id)
                    .unwrap_or(current_block.0);
                return Ok(Some((tx, first_sent_at_block)));
            }

            // If on block `current_block - self.wait_confirmations`
            // sender's nonce was > tx.nonce, then `tx` is mined and confirmed (either successful or reverted).
            // Only then we will check the history to find the receipt.
            // Otherwise, `tx` is mined but not confirmed, so we skip to the next one.
            if operator_nonce.lagging <= tx.nonce {
                continue;
            }

            vlog::trace!(
                "Sender's nonce on block `current block - number of confirmations` is greater than current tx's nonce. \
                 Checking transaction with id {}. Tx nonce is equal to {}",
                tx.id,
                tx.nonce,
            );

            match self
                .check_all_sending_attempts(storage, &tx, current_block)
                .await
            {
                Some((tx_status, confirmations)) => {
                    self.apply_tx_status(storage, &tx, tx_status, confirmations, current_block)
                        .await;
                }
                None => {
                    // The nonce has increased but we did not find the receipt.
                    // This is an error because such a big reorg may cause transactions that were
                    // previously recorded as confirmed to become pending again and we have to
                    // make sure it's not the case - otherwire eth_sender may not work properly.
                    vlog::error!(
                        "Possible block reorgs: nonce increase detected {} blocks ago, but no tx receipt found for tx {:?}",
                        self.config.wait_confirmations,
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
                    opt.gas = Some(self.config.max_aggregated_tx_gas.into());
                    opt.max_fee_per_gas = Some(U256::from(base_fee_per_gas + priority_fee_per_gas));
                    opt.max_priority_fee_per_gas = Some(U256::from(priority_fee_per_gas));
                    opt.nonce = Some(tx.nonce.into());
                }),
                "eth_tx_manager",
            )
            .await
            .expect("Failed to sign transaction")
    }

    async fn send_unsent_txs(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        current_block: L1BlockNumber,
    ) {
        for tx in storage.eth_sender_dal().get_unsent_txs() {
            // Check already sent txs not marked as sent and mark them as sent.
            // The common reason for this behaviour is that we sent tx and stop the server
            // before updating the database
            let tx_status = self
                .get_tx_status_and_confirmations_count(tx.tx_hash, current_block)
                .await;

            if let Ok(Some((tx_status, confirmations))) = tx_status {
                vlog::info!("The tx {:?} has been already sent", tx.tx_hash);
                storage
                    .eth_sender_dal()
                    .set_sent_at_block(tx.id, tx_status.receipt.block_number.unwrap().as_u32());

                let eth_tx = storage
                    .eth_sender_dal()
                    .get_eth_tx(tx.eth_tx_id)
                    .expect("Eth tx should exist");

                self.apply_tx_status(storage, &eth_tx, tx_status, confirmations, current_block)
                    .await;
            } else if let Err(error) = self
                .send_raw_transaction(storage, tx.id, tx.signed_raw_tx.clone(), current_block)
                .await
            {
                vlog::warn!("Error {:?} in sending tx {:?}", error, &tx);
            }
        }
    }

    async fn apply_tx_status(
        &self,
        storage: &mut StorageProcessor<'_>,
        tx: &EthTx,
        tx_status: ExecutedTxStatus,
        confirmations: u64,
        current_block: L1BlockNumber,
    ) {
        if confirmations >= self.config.wait_confirmations {
            if tx_status.success {
                self.confirm_tx(storage, tx, tx_status, current_block);
            } else {
                self.fail_tx(storage, tx, tx_status).await;
            }
        } else {
            vlog::debug!(
                "Transaction {} with id {} has {} out of {} required confirmations",
                tx_status.tx_hash,
                tx.id,
                confirmations,
                self.config.wait_confirmations
            );
        }
    }

    pub async fn fail_tx(
        &self,
        storage: &mut StorageProcessor<'_>,
        tx: &EthTx,
        tx_status: ExecutedTxStatus,
    ) {
        storage.eth_sender_dal().mark_failed_transaction(tx.id);
        let failure_reason = self
            .ethereum_gateway
            .failure_reason(tx_status.receipt.transaction_hash)
            .await
            .expect(
                "Tx is already failed, it's safe to fail here and apply the status on the next run",
            );

        vlog::error!(
            "Eth tx failed {:?}, {:?}, failure reason {:?}",
            tx,
            tx_status.receipt,
            failure_reason
        );
        panic!("We can't operate after tx fail");
    }

    pub fn confirm_tx(
        &self,
        storage: &mut StorageProcessor<'_>,
        tx: &EthTx,
        tx_status: ExecutedTxStatus,
        current_block: L1BlockNumber,
    ) {
        let tx_hash = tx_status.receipt.transaction_hash;
        let gas_used = tx_status
            .receipt
            .gas_used
            .expect("light ETH clients are not supported");

        storage
            .eth_sender_dal()
            .confirm_tx(tx_status.tx_hash, gas_used);

        track_eth_tx_metrics(storage, "mined", tx);

        if gas_used > U256::from(tx.predicted_gas_cost) {
            vlog::error!(
                "Predicted gas {} lower than used gas {} for tx {:?} {}",
                tx.predicted_gas_cost,
                gas_used,
                tx.tx_type,
                tx.id
            );
        }
        vlog::info!(
            "eth_tx {} with hash {:?} for {} is confirmed. Gas spent: {:?}",
            tx.id,
            tx_hash,
            tx.tx_type.to_string(),
            gas_used
        );
        metrics::histogram!(
            "server.eth_sender.l1_gas_used",
            gas_used.low_u128() as f64,
            "type" => tx.tx_type.to_string()
        );
        metrics::histogram!(
            "server.eth_sender.l1_tx_mined_latency",
            (seconds_since_epoch() - tx.created_at_timestamp) as f64,
            "type" => tx.tx_type.to_string()
        );

        let sent_at_block = storage
            .eth_sender_dal()
            .get_block_number_on_first_sent_attempt(tx.id)
            .unwrap_or(0);
        metrics::histogram!(
            "server.eth_sender.l1_blocks_waited_in_mempool",
            (current_block.0 - sent_at_block - self.config.wait_confirmations as u32) as f64,
            "type" => tx.tx_type.to_string()
        );
    }

    pub async fn run(mut self, pool: ConnectionPool, stop_receiver: watch::Receiver<bool>) {
        {
            let current_block = L1BlockNumber(
                self.ethereum_gateway
                    .block_number("etx_tx_manager")
                    .await
                    .unwrap()
                    .as_u32(),
            );
            let mut storage = pool.access_storage_blocking();
            self.send_unsent_txs(&mut storage, current_block).await;
        }

        // It's mandatory to set last_known_l1_block to zero, otherwise the first iteration
        // will never check inflight txs status
        let mut last_known_l1_block = L1BlockNumber(0);
        loop {
            let mut storage = pool.access_storage_blocking();

            if *stop_receiver.borrow() {
                vlog::info!("Stop signal received, eth_tx_manager is shutting down");
                break;
            }

            match self.loop_iteration(&mut storage, last_known_l1_block).await {
                Ok(block) => last_known_l1_block = block,
                Err(e) => {
                    // Web3 API request failures can cause this,
                    // and anything more important is already properly reported.
                    vlog::warn!("eth_sender error {:?}", e);
                }
            }

            tokio::time::sleep(self.config.tx_poll_period()).await;
        }
    }

    async fn send_new_eth_txs(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        current_block: L1BlockNumber,
    ) {
        let number_inflight_txs = storage.eth_sender_dal().get_inflight_txs().len();
        let number_of_available_slots_for_eth_txs = self
            .config
            .max_txs_in_flight
            .saturating_sub(number_inflight_txs as u64);

        if number_of_available_slots_for_eth_txs > 0 {
            // Get the new eth tx and create history item for them
            let new_eth_tx = storage
                .eth_sender_dal()
                .get_new_eth_txs(number_of_available_slots_for_eth_txs);

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
        let current_block = L1BlockNumber(
            self.ethereum_gateway
                .block_number("eth_tx_manager")
                .await?
                .as_u32(),
        );

        self.send_new_eth_txs(storage, current_block).await;

        if current_block <= previous_block {
            // Nothing to do - no new blocks were mined.
            return Ok(current_block);
        }

        if let Some((tx, sent_at_block)) = self
            .monitor_inflight_transactions(storage, current_block)
            .await?
        {
            // New gas price depends on the time this tx spent in mempool.
            let time_in_mempool = current_block.0 - sent_at_block;

            // We don't want to return early in case resend does not succeed -
            // the error is logged anyway, but early returns will prevent
            // sending new operations.
            let _ = self
                .send_eth_tx(storage, &tx, time_in_mempool, current_block)
                .await;
        }

        Ok(current_block)
    }
}

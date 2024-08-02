use std::{sync::Arc, time::Duration};

use tokio::sync::watch;
use zksync_config::configs::eth_sender::SenderConfig;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_eth_client::{
    clients::L1, encode_blob_tx_with_sidecar, BoundEthInterface, ExecutedTxStatus,
    RawTransactionBytes,
};
use zksync_node_fee_model::l1_gas_price::L1TxParamsProvider;
use zksync_shared_metrics::BlockL1Stage;
use zksync_types::{eth_sender::EthTx, Address, L1BlockNumber, H256, U256};
use zksync_utils::time::seconds_since_epoch;

use super::{metrics::METRICS, EthSenderError};
use crate::{
    abstract_l1_interface::{
        AbstractL1Interface, L1BlockNumbers, OperatorNonce, OperatorType, RealL1Interface,
    },
    eth_fees_oracle::{EthFees, EthFeesOracle, GasAdjusterFeesOracle},
    metrics::TransactionType,
};

/// The component is responsible for managing sending eth_txs attempts:
/// Based on eth_tx queue the component generates new attempt with the minimum possible fee,
/// save it to the database, and send it to Ethereum.
/// Based on eth_tx_history queue the component can mark txs as stuck and create the new attempt
/// with higher gas price
#[derive(Debug)]
pub struct EthTxManager {
    l1_interface: Box<dyn AbstractL1Interface>,
    config: SenderConfig,
    fees_oracle: Box<dyn EthFeesOracle>,
    pool: ConnectionPool<Core>,
}

impl EthTxManager {
    pub fn new(
        pool: ConnectionPool<Core>,
        config: SenderConfig,
        gas_adjuster: Arc<dyn L1TxParamsProvider>,
        ethereum_gateway: Box<dyn BoundEthInterface<L1>>,
        ethereum_gateway_blobs: Option<Box<dyn BoundEthInterface<L1>>>,
    ) -> Self {
        let ethereum_gateway = ethereum_gateway.for_component("eth_tx_manager");
        let ethereum_gateway_blobs =
            ethereum_gateway_blobs.map(|eth| eth.for_component("eth_tx_manager"));
        let fees_oracle = GasAdjusterFeesOracle {
            gas_adjuster,
            max_acceptable_priority_fee_in_gwei: config.max_acceptable_priority_fee_in_gwei,
        };
        Self {
            l1_interface: Box::new(RealL1Interface {
                ethereum_gateway,
                ethereum_gateway_blobs,
                wait_confirmations: config.wait_confirmations,
            }),
            config,
            fees_oracle: Box::new(fees_oracle),
            pool,
        }
    }

    #[cfg(test)]
    pub(crate) fn l1_interface(&self) -> &dyn AbstractL1Interface {
        self.l1_interface.as_ref()
    }

    async fn check_all_sending_attempts(
        &self,
        storage: &mut Connection<'_, Core>,
        op: &EthTx,
    ) -> Result<Option<ExecutedTxStatus>, EthSenderError> {
        // Checking history items, starting from most recently sent.
        for history_item in storage
            .eth_sender_dal()
            .get_tx_history_to_check(op.id)
            .await
            .unwrap()
        {
            let operator_type = if op.blob_sidecar.is_some() {
                OperatorType::Blob
            } else {
                OperatorType::NonBlob
            };

            // `status` is a Result here and we don't unwrap it with `?`
            // because if we do and get an `Err`, we won't finish the for loop,
            // which means we might miss the transaction that actually succeeded.
            match self
                .l1_interface
                .get_tx_status(history_item.tx_hash, operator_type)
                .await
            {
                Ok(Some(s)) => return Ok(Some(s)),
                Ok(_) => continue,
                Err(err) => {
                    tracing::warn!(
                        "Can't check transaction {:?}: {:?}",
                        history_item.tx_hash,
                        err
                    );
                    return Err(err);
                }
            }
        }
        Ok(None)
    }

    pub(crate) async fn send_eth_tx(
        &mut self,
        storage: &mut Connection<'_, Core>,
        tx: &EthTx,
        time_in_mempool: u32,
        current_block: L1BlockNumber,
    ) -> Result<H256, EthSenderError> {
        let previous_sent_tx = storage
            .eth_sender_dal()
            .get_last_sent_eth_tx(tx.id)
            .await
            .unwrap();
        let has_blob_sidecar = tx.blob_sidecar.is_some();

        let EthFees {
            base_fee_per_gas,
            priority_fee_per_gas,
            blob_base_fee_per_gas,
        } = self.fees_oracle.calculate_fees(
            &previous_sent_tx,
            has_blob_sidecar,
            time_in_mempool,
        )?;

        let operator_type = if tx.blob_sidecar.is_some() {
            OperatorType::Blob
        } else {
            OperatorType::NonBlob
        };

        if let Some(previous_sent_tx) = previous_sent_tx {
            METRICS.transaction_resent.inc();
            tracing::info!(
                "Resending {operator_type:?} tx {} (nonce {}) \
                at block {current_block} with \
                base_fee_per_gas {base_fee_per_gas:?}, \
                priority_fee_per_gas {priority_fee_per_gas:?}, \
                blob_fee_per_gas {blob_base_fee_per_gas:?}, \
                previously sent with \
                base_fee_per_gas {:?}, \
                priority_fee_per_gas {:?}, \
                blob_fee_per_gas {:?}, \
                ",
                tx.id,
                tx.nonce,
                previous_sent_tx.base_fee_per_gas,
                previous_sent_tx.priority_fee_per_gas,
                previous_sent_tx.blob_base_fee_per_gas
            );
        } else {
            tracing::info!(
                "Sending {operator_type:?} tx {} (nonce {}) \
                at block {current_block} with \
                base_fee_per_gas {base_fee_per_gas:?}, \
                priority_fee_per_gas {priority_fee_per_gas:?}, \
                blob_fee_per_gas {blob_base_fee_per_gas:?}",
                tx.id,
                tx.nonce
            );
        }

        if let Some(blob_base_fee_per_gas) = blob_base_fee_per_gas {
            METRICS.used_blob_fee_per_gas[&TransactionType::Blob].observe(blob_base_fee_per_gas);
            METRICS.used_base_fee_per_gas[&TransactionType::Blob].observe(base_fee_per_gas);
            METRICS.used_priority_fee_per_gas[&TransactionType::Blob].observe(priority_fee_per_gas);
        } else {
            METRICS.used_base_fee_per_gas[&TransactionType::Regular].observe(base_fee_per_gas);
            METRICS.used_priority_fee_per_gas[&TransactionType::Regular]
                .observe(priority_fee_per_gas);
        }

        let blob_gas_price = if has_blob_sidecar {
            Some(
                blob_base_fee_per_gas
                    .expect("always ready to query blob gas price for blob transactions; qed")
                    .into(),
            )
        } else {
            None
        };

        let mut signed_tx = self
            .l1_interface
            .sign_tx(
                tx,
                base_fee_per_gas,
                priority_fee_per_gas,
                blob_gas_price,
                self.config.max_aggregated_tx_gas.into(),
            )
            .await;

        if let Some(blob_sidecar) = &tx.blob_sidecar {
            signed_tx.raw_tx = RawTransactionBytes::new_unchecked(encode_blob_tx_with_sidecar(
                signed_tx.raw_tx.as_ref(),
                blob_sidecar,
            ));
        }

        if let Some(tx_history_id) = storage
            .eth_sender_dal()
            .insert_tx_history(
                tx.id,
                base_fee_per_gas,
                priority_fee_per_gas,
                blob_base_fee_per_gas,
                signed_tx.hash,
                signed_tx.raw_tx.as_ref(),
                current_block.0,
            )
            .await
            .unwrap()
        {
            if let Err(error) = self
                .send_raw_transaction(storage, tx_history_id, signed_tx.raw_tx, operator_type)
                .await
            {
                tracing::warn!(
                    "Error Sending {operator_type:?} tx {} (nonce {}) at block {current_block} with \
                    base_fee_per_gas {base_fee_per_gas:?}, \
                    priority_fee_per_gas {priority_fee_per_gas:?}, \
                    blob_fee_per_gas {blob_base_fee_per_gas:?},\
                    error {error}",
                    tx.id,
                    tx.nonce,
                );
            }
        }
        Ok(signed_tx.hash)
    }

    async fn send_raw_transaction(
        &self,
        storage: &mut Connection<'_, Core>,
        tx_history_id: u32,
        raw_tx: RawTransactionBytes,
        operator_type: OperatorType,
    ) -> Result<(), EthSenderError> {
        match self.l1_interface.send_raw_tx(raw_tx, operator_type).await {
            Ok(_) => Ok(()),
            Err(error) => {
                // In transient errors, server may have received the transaction
                // we don't want to loose record about it in case that happens
                if !error.is_transient() {
                    storage
                        .eth_sender_dal()
                        .remove_tx_history(tx_history_id)
                        .await
                        .unwrap();
                } else {
                    METRICS.l1_transient_errors.inc();
                }
                Err(error.into())
            }
        }
    }

    pub(crate) fn operator_address(&self, operator_type: OperatorType) -> Option<Address> {
        if operator_type == OperatorType::NonBlob {
            None
        } else {
            self.l1_interface.get_blobs_operator_account()
        }
    }
    // Monitors the in-flight transactions, marks mined ones as confirmed,
    // returns the one that has to be resent (if there is one).
    pub(super) async fn monitor_inflight_transactions_single_operator(
        &mut self,
        storage: &mut Connection<'_, Core>,
        l1_block_numbers: L1BlockNumbers,
        operator_type: OperatorType,
    ) -> Result<Option<(EthTx, u32)>, EthSenderError> {
        let operator_nonce = self
            .l1_interface
            .get_operator_nonce(l1_block_numbers, operator_type)
            .await?;

        if let Some(operator_nonce) = operator_nonce {
            let inflight_txs = storage
                .eth_sender_dal()
                .get_inflight_txs(self.operator_address(operator_type))
                .await
                .unwrap();
            METRICS.number_of_inflight_txs[&operator_type].set(inflight_txs.len());

            Ok(self
                .apply_inflight_txs_statuses_and_get_first_to_resend(
                    storage,
                    l1_block_numbers,
                    operator_nonce,
                    inflight_txs,
                )
                .await?)
        } else {
            Ok(None)
        }
    }

    async fn apply_inflight_txs_statuses_and_get_first_to_resend(
        &mut self,
        storage: &mut Connection<'_, Core>,
        l1_block_numbers: L1BlockNumbers,
        operator_nonce: OperatorNonce,
        inflight_txs: Vec<EthTx>,
    ) -> Result<Option<(EthTx, u32)>, EthSenderError> {
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
            tracing::info!(
                "Checking tx id: {}, operator_nonce: {:?}, tx nonce: {}",
                tx.id,
                operator_nonce,
                tx.nonce,
            );

            // If the `operator_nonce.latest` <= `tx.nonce`, this means
            // that `tx` is not mined and we should resend it.
            // We only resend the first un-mined transaction.
            if operator_nonce.latest <= tx.nonce {
                let last_sent_at_block = storage
                    .eth_sender_dal()
                    .get_block_number_on_last_sent_attempt(tx.id)
                    .await
                    .unwrap();
                // the transaction may still be included in last block, we shouldn't resend it yet
                if last_sent_at_block >= Some(l1_block_numbers.latest.0) {
                    continue;
                }

                // None means txs hasn't been sent yet
                let first_sent_at_block = storage
                    .eth_sender_dal()
                    .get_block_number_on_first_sent_attempt(tx.id)
                    .await
                    .unwrap();
                return Ok(Some((
                    tx,
                    first_sent_at_block.unwrap_or(l1_block_numbers.latest.0),
                )));
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

            tracing::info!(
                "Updating status of tx {} of type {} with nonce {}",
                tx.id,
                tx.tx_type,
                tx.nonce
            );
            match self.check_all_sending_attempts(storage, &tx).await {
                Ok(Some(tx_status)) => {
                    self.apply_tx_status(storage, &tx, tx_status, l1_block_numbers.finalized)
                        .await;
                }
                Ok(None) => {
                    // The nonce has increased but we did not find the receipt.
                    // This is an error because such a big re-org may cause transactions that were
                    // previously recorded as confirmed to become pending again and we have to
                    // make sure it's not the case - otherwise `eth_sender` may not work properly.
                    tracing::error!(
                        "Possible block reorgs: finalized nonce increase detected, but no tx receipt found for tx {:?}",
                        &tx
                    );
                }
                Err(err) => {
                    // An error here means that we weren't able to check status of one of the txs
                    // we can't continue to avoid situations with out-of-order confirmed txs
                    // (for instance Execute tx confirmed before PublishProof tx) as this would make
                    // our API return inconsistent block info
                    return Err(err);
                }
            }
        }
        Ok(None)
    }

    async fn apply_tx_status(
        &self,
        storage: &mut Connection<'_, Core>,
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
            tracing::trace!(
                "Transaction {} with id {} is not yet finalized: block in receipt {receipt_block_number}, finalized block {finalized_block}",
                tx_status.tx_hash,
                tx.id,
            );
        }
    }

    pub async fn fail_tx(
        &self,
        storage: &mut Connection<'_, Core>,
        tx: &EthTx,
        tx_status: ExecutedTxStatus,
    ) {
        storage
            .eth_sender_dal()
            .mark_failed_transaction(tx.id)
            .await
            .unwrap();
        let failure_reason = self
            .l1_interface
            .failure_reason(tx_status.receipt.transaction_hash)
            .await;

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
        storage: &mut Connection<'_, Core>,
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

    pub async fn run(mut self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let pool = self.pool.clone();

        loop {
            let mut storage = pool.connection_tagged("eth_sender").await.unwrap();

            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, eth_tx_manager is shutting down");
                break;
            }
            let l1_block_numbers = self.l1_interface.get_l1_block_numbers().await?;
            METRICS.track_block_numbers(&l1_block_numbers);

            self.loop_iteration(&mut storage, l1_block_numbers).await;
            tokio::time::sleep(self.config.tx_poll_period()).await;
        }
        Ok(())
    }

    async fn send_new_eth_txs(
        &mut self,
        storage: &mut Connection<'_, Core>,
        current_block: L1BlockNumber,
        operator_type: OperatorType,
    ) {
        let number_inflight_txs = storage
            .eth_sender_dal()
            .get_inflight_txs(self.operator_address(operator_type))
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
                .get_new_eth_txs(
                    number_of_available_slots_for_eth_txs,
                    &self.operator_address(operator_type),
                )
                .await
                .unwrap();

            if !new_eth_tx.is_empty() {
                tracing::info!(
                    "Sending {} {operator_type:?} new transactions",
                    new_eth_tx.len()
                );
            } else {
                tracing::debug!("No new {operator_type:?} transactions to send");
            }
            for tx in new_eth_tx {
                let result = self.send_eth_tx(storage, &tx, 0, current_block).await;
                // If one of the transactions doesn't succeed, this means we should return
                // as new transactions have increasing nonces, so they will also result in an error
                // about gapped nonces
                if result.is_err() {
                    tracing::info!("Skipping sending rest of new transactions because of error");
                    break;
                }
            }
        }
    }

    async fn update_statuses_and_resend_if_needed(
        &mut self,
        storage: &mut Connection<'_, Core>,
        l1_block_numbers: L1BlockNumbers,
        operator_type: OperatorType,
    ) -> Result<(), EthSenderError> {
        if let Some((tx, sent_at_block)) = self
            .monitor_inflight_transactions_single_operator(storage, l1_block_numbers, operator_type)
            .await?
        {
            // New gas price depends on the time this tx spent in mempool.
            let time_in_mempool = l1_block_numbers.latest.0 - sent_at_block;

            // We don't want to return early in case resend does not succeed -
            // the error is logged anyway, but early returns will prevent
            // sending new operations.
            let _ = self
                .send_eth_tx(storage, &tx, time_in_mempool, l1_block_numbers.latest)
                .await?;
        }
        Ok(())
    }

    #[tracing::instrument(skip_all, name = "EthTxManager::loop_iteration")]
    pub async fn loop_iteration(
        &mut self,
        storage: &mut Connection<'_, Core>,
        l1_block_numbers: L1BlockNumbers,
    ) {
        tracing::debug!("Loop iteration at block {}", l1_block_numbers.latest);
        // We can treat those two operators independently as they have different nonces and
        // aggregator makes sure that corresponding Commit transaction is confirmed before creating
        // a PublishProof transaction
        for operator_type in [OperatorType::NonBlob, OperatorType::Blob] {
            self.send_new_eth_txs(storage, l1_block_numbers.latest, operator_type)
                .await;
            let result = self
                .update_statuses_and_resend_if_needed(storage, l1_block_numbers, operator_type)
                .await;

            //We don't want an error in sending non-blob transactions interrupt sending blob txs
            if let Err(error) = result {
                // Web3 API request failures can cause this,
                // and anything more important is already properly reported.
                tracing::warn!("eth_sender error {:?}", error);
                if error.is_transient() {
                    METRICS.l1_transient_errors.inc();
                }
            }
        }
    }
}

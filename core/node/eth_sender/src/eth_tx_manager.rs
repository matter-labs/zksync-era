use std::{sync::Arc, time::Duration};

use anyhow::Context as _;
use tokio::sync::watch;
use zksync_config::configs::eth_sender::SenderConfig;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_eth_client::{
    encode_blob_tx_with_sidecar, BoundEthInterface, ExecutedTxStatus, RawTransactionBytes,
};
use zksync_node_fee_model::l1_gas_price::L1TxParamsProvider;
use zksync_shared_metrics::BlockL1Stage;
use zksync_types::{eth_sender::EthTx, Address, L1BlockNumber, H256, U256};
use zksync_utils::time::seconds_since_epoch;

use super::{metrics::METRICS, EthSenderError};
use crate::{
    abstract_l1_interface::{AbstractL1Interface, L1BlockNumbers, OperatorNonce, RealL1Interface},
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
        ethereum_gateway: Box<dyn BoundEthInterface>,
        ethereum_gateway_blobs: Option<Box<dyn BoundEthInterface>>,
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
            // `status` is a Result here and we don't unwrap it with `?`
            // because if we do and get an `Err`, we won't finish the for loop,
            // which means we might miss the transaction that actually succeeded.
            match self.l1_interface.get_tx_status(history_item.tx_hash).await {
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

        if let Some(previous_sent_tx) = previous_sent_tx {
            METRICS.transaction_resent.inc();
            tracing::info!(
                "Resending tx {} at block {current_block} with \
                base_fee_per_gas {base_fee_per_gas:?}, \
                priority_fee_per_gas {priority_fee_per_gas:?}, \
                blob_fee_per_gas {blob_base_fee_per_gas:?}, \
                previously sent with \
                base_fee_per_gas {:?}, \
                priority_fee_per_gas {:?}, \
                blob_fee_per_gas {:?}, \
                ",
                tx.id,
                previous_sent_tx.base_fee_per_gas,
                previous_sent_tx.priority_fee_per_gas,
                previous_sent_tx.blob_base_fee_per_gas
            );
        } else {
            tracing::info!(
                "Sending tx {} at block {current_block} with \
                base_fee_per_gas {base_fee_per_gas:?}, \
                priority_fee_per_gas {priority_fee_per_gas:?}, \
                blob_fee_per_gas {blob_base_fee_per_gas:?}",
                tx.id
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
                .send_raw_transaction(storage, tx_history_id, signed_tx.raw_tx)
                .await
            {
                tracing::warn!(
                    "Error Sending tx {} at block {current_block} with \
                    base_fee_per_gas {base_fee_per_gas:?}, \
                    priority_fee_per_gas {priority_fee_per_gas:?}, \
                    blob_fee_per_gas {blob_base_fee_per_gas:?},\
                    error {error}",
                    tx.id
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
    ) -> Result<(), EthSenderError> {
        match self.l1_interface.send_raw_tx(raw_tx).await {
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

    // Monitors the in-flight transactions, marks mined ones as confirmed,
    // returns the one that has to be resent (if there is one).
    pub(super) async fn monitor_inflight_transactions(
        &mut self,
        storage: &mut Connection<'_, Core>,
        l1_block_numbers: L1BlockNumbers,
    ) -> Result<Option<(EthTx, u32)>, EthSenderError> {
        METRICS.track_block_numbers(&l1_block_numbers);
        let operator_nonce = self
            .l1_interface
            .get_operator_nonce(l1_block_numbers)
            .await?;

        let non_blob_tx_to_resend = self
            .apply_inflight_txs_statuses_and_get_first_to_resend(
                storage,
                l1_block_numbers,
                operator_nonce,
                None,
            )
            .await?;

        let blobs_operator_nonce = self
            .l1_interface
            .get_blobs_operator_nonce(l1_block_numbers)
            .await?;
        let blobs_operator_address = self.l1_interface.get_blobs_operator_account();

        let mut blob_tx_to_resend = None;
        if let Some(blobs_operator_nonce) = blobs_operator_nonce {
            // need to check if both nonce and address are `Some`
            if blobs_operator_address.is_none() {
                panic!("blobs_operator_address has to be set its nonce is known; qed");
            }
            blob_tx_to_resend = self
                .apply_inflight_txs_statuses_and_get_first_to_resend(
                    storage,
                    l1_block_numbers,
                    blobs_operator_nonce,
                    blobs_operator_address,
                )
                .await?;
        }

        // We have to resend non-blob transactions first, otherwise in case of a temporary
        // spike in activity, all Execute and PublishProof would need to wait until all commit txs
        // are sent, which may take some time. We treat them as if they had higher priority.
        if non_blob_tx_to_resend.is_some() {
            Ok(non_blob_tx_to_resend)
        } else {
            Ok(blob_tx_to_resend)
        }
    }

    async fn apply_inflight_txs_statuses_and_get_first_to_resend(
        &mut self,
        storage: &mut Connection<'_, Core>,
        l1_block_numbers: L1BlockNumbers,
        operator_nonce: OperatorNonce,
        operator_address: Option<Address>,
    ) -> Result<Option<(EthTx, u32)>, EthSenderError> {
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
            tracing::trace!(
                "Checking tx id: {}, operator_nonce: {:?}, tx nonce: {}",
                tx.id,
                operator_nonce,
                tx.nonce,
            );

            if tx.from_addr != operator_address {
                continue;
            }

            // If the `operator_nonce.latest` <= `tx.nonce`, this means
            // that `tx` is not mined and we should resend it.
            // We only resend the first un-mined transaction.
            if operator_nonce.latest <= tx.nonce {
                // None means txs hasn't been sent yet
                let first_sent_at_block = storage
                    .eth_sender_dal()
                    .get_block_number_on_first_sent_attempt(tx.id)
                    .await
                    .unwrap();
                // the transaction may still be included in block, we shouldn't resend it yet
                if first_sent_at_block == Some(l1_block_numbers.latest.0) {
                    continue;
                }
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

    async fn send_unsent_txs(
        &mut self,
        storage: &mut Connection<'_, Core>,
        l1_block_numbers: L1BlockNumbers,
    ) {
        for tx in storage.eth_sender_dal().get_unsent_txs().await.unwrap() {
            // Check already sent txs not marked as sent and mark them as sent.
            // The common reason for this behavior is that we sent tx and stop the server
            // before updating the database
            let tx_status = self.l1_interface.get_tx_status(tx.tx_hash).await;

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
            } else {
                storage
                    .eth_sender_dal()
                    .set_sent_at_block(tx.id, l1_block_numbers.latest.0)
                    .await
                    .unwrap();
                if let Err(error) = self
                    .send_raw_transaction(
                        storage,
                        tx.id,
                        RawTransactionBytes::new_unchecked(tx.signed_raw_tx.clone()),
                    )
                    .await
                {
                    tracing::warn!("Error sending transaction {tx:?}: {error}");
                }
            }
        }
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
            tracing::debug!(
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
        {
            let l1_block_numbers = self
                .l1_interface
                .get_l1_block_numbers()
                .await
                .context("get_l1_block_numbers()")?;
            let mut storage = pool.connection_tagged("eth_sender").await.unwrap();
            self.send_unsent_txs(&mut storage, l1_block_numbers).await;
        }

        // It's mandatory to set `last_known_l1_block` to zero, otherwise the first iteration
        // will never check in-flight txs status
        let mut last_known_l1_block = L1BlockNumber(0);
        loop {
            let mut storage = pool.connection_tagged("eth_sender").await.unwrap();

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
                    if e.is_transient() {
                        METRICS.l1_transient_errors.inc();
                    }
                }
            }

            tokio::time::sleep(self.config.tx_poll_period()).await;
        }
        Ok(())
    }

    async fn send_new_eth_txs(
        &mut self,
        storage: &mut Connection<'_, Core>,
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
        storage: &mut Connection<'_, Core>,
        previous_block: L1BlockNumber,
    ) -> Result<L1BlockNumber, EthSenderError> {
        let l1_block_numbers = self.l1_interface.get_l1_block_numbers().await?;

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

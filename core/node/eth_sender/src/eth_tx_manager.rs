use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

use tokio::sync::watch;
use zksync_config::configs::eth_sender::{GasLimitMode, SenderConfig};
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_eth_client::{
    encode_blob_tx_with_sidecar, BoundEthInterface, ExecutedTxStatus, RawTransactionBytes,
};
use zksync_health_check::{Health, HealthStatus, HealthUpdater, ReactiveHealthCheck};
use zksync_node_fee_model::l1_gas_price::TxParamsProvider;
use zksync_shared_metrics::L1Stage;
use zksync_types::{
    aggregated_operations::{AggregatedActionType, L1BatchAggregatedActionType},
    eth_sender::{EthTx, EthTxBlobSidecar, EthTxFinalityStatus, L1BlockNumbers},
    Address, L1BlockNumber, GATEWAY_CALLDATA_PROCESSING_ROLLUP_OVERHEAD_GAS, H256,
    L1_CALLDATA_PROCESSING_ROLLUP_OVERHEAD_GAS, L1_GAS_PER_PUBDATA_BYTE, U256,
};

use super::{metrics::METRICS, EthSenderError};
use crate::{
    abstract_l1_interface::{AbstractL1Interface, OperatorNonce, OperatorType, RealL1Interface},
    eth_fees_oracle::{EthFees, EthFeesOracle, GasAdjusterFeesOracle},
    health::{EthTxDetails, EthTxManagerHealthDetails},
    metrics::TransactionType,
};

/// The component is responsible for managing sending eth_txs attempts.
///
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
    health_updater: HealthUpdater,
}

impl EthTxManager {
    pub fn new(
        pool: ConnectionPool<Core>,
        config: SenderConfig,
        gas_adjuster: Arc<dyn TxParamsProvider>,
        ethereum_client: Option<Box<dyn BoundEthInterface>>,
        ethereum_client_blobs: Option<Box<dyn BoundEthInterface>>,
        l2_client: Option<Box<dyn BoundEthInterface>>,
    ) -> Self {
        let ethereum_client = ethereum_client.map(|eth| eth.for_component("eth_tx_manager"));
        let ethereum_client_blobs =
            ethereum_client_blobs.map(|eth| eth.for_component("eth_tx_manager"));
        // If `time_in_mempool_multiplier_cap` is set in config then we use it to derive cap for `l1_blocks_cap`.
        // Otherwise we use `time_in_mempool_in_l1_blocks_cap`.
        let time_in_mempool_in_l1_blocks_cap =
            if let Some(multiplier_cap) = config.time_in_mempool_multiplier_cap {
                derive_l1_block_cap(multiplier_cap, gas_adjuster.get_parameter_b())
            } else {
                config.time_in_mempool_in_l1_blocks_cap
            };
        let fees_oracle = GasAdjusterFeesOracle {
            gas_adjuster,
            max_acceptable_priority_fee_in_gwei: config.max_acceptable_priority_fee_in_gwei,
            time_in_mempool_in_l1_blocks_cap,
            max_acceptable_base_fee_in_wei: config.max_acceptable_base_fee_in_wei,
        };
        let l1_interface = Box::new(RealL1Interface {
            ethereum_client,
            ethereum_client_blobs,
            sl_client: l2_client,
            wait_confirmations: config.wait_confirmations,
        });
        tracing::info!(
            "Started eth_tx_manager supporting {:?} operators",
            l1_interface.supported_operator_types()
        );
        Self {
            l1_interface,
            config,
            fees_oracle: Box::new(fees_oracle),
            pool,
            health_updater: ReactiveHealthCheck::new("eth_tx_manager").1,
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
            match self
                .l1_interface
                .get_tx_status(history_item.tx_hash, self.operator_type(op))
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
        time_in_mempool_in_l1_blocks: u32,
        current_block: L1BlockNumber,
    ) -> Result<H256, EthSenderError> {
        let previous_sent_tx = storage
            .eth_sender_dal()
            .get_last_sent_successfully_eth_tx(tx.id)
            .await
            .unwrap();

        let operator_type = self.operator_type(tx);
        let EthFees {
            base_fee_per_gas,
            priority_fee_per_gas,
            blob_base_fee_per_gas,
            max_gas_per_pubdata_price,
        } = self.fees_oracle.calculate_fees(
            &previous_sent_tx,
            time_in_mempool_in_l1_blocks,
            operator_type,
        )?;

        let blob_gas_price = if tx.blob_sidecar.is_some() {
            Some(
                blob_base_fee_per_gas
                    .expect("always ready to query blob gas price for blob transactions; qed")
                    .into(),
            )
        } else {
            None
        };

        let gas_limit = self.gas_limit(tx, max_gas_per_pubdata_price);

        if let Some(previous_sent_tx) = previous_sent_tx {
            METRICS.transaction_resent.inc();
            tracing::info!(
                "Resending {operator_type:?} tx {} (nonce {}) \
                at block {current_block} with \
                base_fee_per_gas {base_fee_per_gas:?}, \
                priority_fee_per_gas {priority_fee_per_gas:?}, \
                blob_fee_per_gas {blob_base_fee_per_gas:?}, \
                max_gas_per_pubdata_price {max_gas_per_pubdata_price:?}, \
                previously sent with \
                base_fee_per_gas {:?}, \
                priority_fee_per_gas {:?}, \
                blob_fee_per_gas {:?}, \
                max_gas_per_pubdata_price {:?}, \
                gas_limit {gas_limit:?}, \
                ",
                tx.id,
                tx.nonce,
                previous_sent_tx.base_fee_per_gas,
                previous_sent_tx.priority_fee_per_gas,
                previous_sent_tx.blob_base_fee_per_gas,
                previous_sent_tx.max_gas_per_pubdata,
            );
        } else {
            tracing::info!(
                "Sending {operator_type:?} tx {} (nonce {}) \
                at block {current_block} with \
                base_fee_per_gas {base_fee_per_gas:?}, \
                priority_fee_per_gas {priority_fee_per_gas:?}, \
                blob_fee_per_gas {blob_base_fee_per_gas:?},\
                max_gas_per_pubdata_price {max_gas_per_pubdata_price:?},\
                gas_limit {gas_limit:?}, \
                ",
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

        let mut signed_tx = self
            .l1_interface
            .sign_tx(
                tx,
                base_fee_per_gas,
                priority_fee_per_gas,
                blob_gas_price,
                gas_limit,
                operator_type,
                max_gas_per_pubdata_price.map(Into::into),
            )
            .await;

        if let Some(blob_sidecar) = &tx.blob_sidecar {
            let settings = c_kzg::ethereum_kzg_settings_arc(0);
            let EthTxBlobSidecar::EthTxBlobSidecarV1(blob_sidecar_v1) = blob_sidecar;
            let mut new_blob_sidecar = blob_sidecar_v1.clone();
            for blob in &mut new_blob_sidecar.blobs {
                let c_kzg_blob = c_kzg::Blob::from_bytes(&blob.blob).unwrap();
                let (_cells, kzg_proofs) =
                    settings.compute_cells_and_kzg_proofs(&c_kzg_blob).unwrap();
                blob.cell_proofs = Some(
                    kzg_proofs
                        .into_iter()
                        .map(|p| p.to_bytes().into_inner().to_vec())
                        .collect(),
                );
            }
            let new_blob_sidecar = EthTxBlobSidecar::EthTxBlobSidecarV1(new_blob_sidecar);
            signed_tx.raw_tx = RawTransactionBytes::new_unchecked(encode_blob_tx_with_sidecar(
                signed_tx.raw_tx.as_ref(),
                &new_blob_sidecar,
            ));
        }

        let inserted_tx_history_id = storage
            .eth_sender_dal()
            .insert_tx_history(
                tx.id,
                base_fee_per_gas,
                priority_fee_per_gas,
                blob_base_fee_per_gas,
                max_gas_per_pubdata_price,
                signed_tx.hash,
                signed_tx.raw_tx.as_ref(),
                current_block.0,
                Some(gas_limit.as_u64()),
            )
            .await
            .unwrap();

        let tx_history_id = if let Some(tx_history_id) = inserted_tx_history_id {
            tx_history_id
        } else {
            // Insertion failed, it means such tx was already sent but presumably
            // we didn't confirm that node has received it.
            storage
                .eth_sender_dal()
                .tx_history_by_hash(tx.id, signed_tx.hash)
                .await
                .unwrap()
                .unwrap_or_else(|| {
                    panic!(
                        "Could not insert not select tx with hash {:#?} and eth_tx_id {}",
                        signed_tx.hash, tx.id
                    )
                })
        };

        let send_result = self
            .send_raw_transaction(storage, tx_history_id, signed_tx.raw_tx, operator_type)
            .await;
        if let Err(error) = send_result {
            tracing::warn!(
                "Error Sending {operator_type:?} tx {} (nonce {}) at block {current_block} with \
                base_fee_per_gas {base_fee_per_gas:?}, \
                priority_fee_per_gas {priority_fee_per_gas:?}, \
                blob_fee_per_gas {blob_base_fee_per_gas:?},\
                gas_limit {gas_limit:?},
                error {error}",
                tx.id,
                tx.nonce,
            );
            return Err(error);
        }
        Ok(signed_tx.hash)
    }

    fn gas_limit(&self, tx: &EthTx, max_gas_per_pubdata_price: Option<u64>) -> U256 {
        if self.config.gas_limit_mode == GasLimitMode::Maximum {
            return self.config.max_aggregated_tx_gas.into();
        }

        let operator_type = self.operator_type(tx);

        // Gas limit saved in predicted gas_cost, doesn't include gas_limit for pubdata usage.
        let Some(gas_without_pubdata) = tx.predicted_gas_cost else {
            return self.config.max_aggregated_tx_gas.into();
        };

        // Adjust gas limit based ob pubdata cost. Commit is the only pubdata intensive part
        if tx.tx_type == AggregatedActionType::L1Batch(L1BatchAggregatedActionType::Commit) {
            match operator_type {
                OperatorType::Blob | OperatorType::NonBlob => {
                    // Settlement mode is L1.
                    (gas_without_pubdata
                        + ((L1_GAS_PER_PUBDATA_BYTE + L1_CALLDATA_PROCESSING_ROLLUP_OVERHEAD_GAS)
                            * tx.raw_tx.len() as u32) as u64)
                        .into()
                }
                OperatorType::Gateway => {
                    // Settlement mode is Gateway.
                    self.adjust_gateway_pubdata_gas_limit(
                        tx,
                        max_gas_per_pubdata_price,
                        gas_without_pubdata,
                    )
                }
            }
        } else if tx.tx_type == AggregatedActionType::L1Batch(L1BatchAggregatedActionType::Execute)
            && operator_type == OperatorType::Gateway
        {
            // Execute tx on Gateway can become pubdata intensive due to interop
            self.adjust_gateway_pubdata_gas_limit(
                tx,
                max_gas_per_pubdata_price,
                gas_without_pubdata,
            )
        } else {
            gas_without_pubdata.into()
        }
    }

    fn adjust_gateway_pubdata_gas_limit(
        &self,
        tx: &EthTx,
        max_gas_per_pubdata_price: Option<u64>,
        gas_without_pubdata: u64,
    ) -> U256 {
        if let Some(max_gas_per_pubdata_price) = max_gas_per_pubdata_price {
            (gas_without_pubdata
                + ((max_gas_per_pubdata_price
                    + GATEWAY_CALLDATA_PROCESSING_ROLLUP_OVERHEAD_GAS as u64)
                    * tx.raw_tx.len() as u64))
                .into()
        } else {
            self.config.max_aggregated_tx_gas.into()
        }
    }

    async fn send_raw_transaction(
        &self,
        connection: &mut Connection<'_, Core>,
        tx_history_id: u32,
        raw_tx: RawTransactionBytes,
        operator_type: OperatorType,
    ) -> Result<(), EthSenderError> {
        match self.l1_interface.send_raw_tx(raw_tx, operator_type).await {
            Ok(_) => {
                // Node has accepted tx and we mark tx as such.
                // It will be used for fee calculation on resent attempt (if needed).
                connection
                    .eth_sender_dal()
                    .set_sent_success(tx_history_id)
                    .await
                    .unwrap();
                Ok(())
            }
            Err(error) => {
                // Error does not guarantee that node hasn't accepted tx.
                // We do not remove tx from DB so we will monitor tx status anyway
                // but will not use it for fee calculation on resent attempt.
                Err(error.into())
            }
        }
    }

    pub(crate) fn operator_address(&self, operator_type: OperatorType) -> Address {
        self.l1_interface.get_operator_account(operator_type)
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
            let non_final_txs = storage
                .eth_sender_dal()
                .get_non_final_txs(
                    self.operator_address(operator_type),
                    operator_type == OperatorType::Gateway,
                )
                .await
                .unwrap();

            let result = self
                .apply_inflight_txs_statuses_and_get_first_to_resend(
                    storage,
                    l1_block_numbers,
                    operator_nonce,
                    non_final_txs,
                )
                .await?;
            if let Some((eth_tx, _)) = result {
                tracing::warn!("Fast finalized transaction has been reverted {:?}", &eth_tx);
                storage
                    .eth_sender_dal()
                    .unfinalize_txs(
                        self.operator_address(operator_type),
                        operator_type == OperatorType::Gateway,
                        eth_tx.id,
                    )
                    .await
                    .unwrap();
            }

            let inflight_txs = storage
                .eth_sender_dal()
                .get_inflight_txs(
                    self.operator_address(operator_type),
                    operator_type == OperatorType::Gateway,
                )
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
             Block numbers: latest {}, fast_finality {}, finalized {}, \
             operator's nonce: latest {}, fast_finality {}, finalized {}",
            l1_block_numbers.latest,
            l1_block_numbers.fast_finality,
            l1_block_numbers.finalized,
            operator_nonce.latest,
            operator_nonce.fast_finality,
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

            // If on fast_finality block sender's nonce was > tx.nonce,
            // then `tx` is mined and confirmed (either successful or reverted).
            // Only then we will check the history to find the receipt.
            // Otherwise, `tx` is mined but not confirmed, so we skip to the next one.
            if operator_nonce.fast_finality <= tx.nonce {
                continue;
            }

            tracing::trace!(
                "Sender's nonce on finalized block is greater than current tx's nonce. \
                 Checking transaction with id {} and type {}. Tx nonce is equal to {}",
                tx.id,
                tx.tx_type,
                tx.nonce,
            );

            match self.check_all_sending_attempts(storage, &tx).await {
                Ok(Some(tx_status)) => {
                    self.apply_tx_status(storage, &tx, tx_status, l1_block_numbers)
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
        blocks: L1BlockNumbers,
    ) {
        let receipt_block_number = tx_status.receipt.block_number.unwrap().as_u32();
        let finality_status = if blocks.finalized.0 >= receipt_block_number {
            EthTxFinalityStatus::Finalized
        } else if blocks.fast_finality.0 >= receipt_block_number {
            EthTxFinalityStatus::FastFinalized
        } else {
            tracing::trace!(
                "Transaction {} with id {} is not finalized: block in receipt {receipt_block_number}, finalized block {}",
                tx_status.tx_hash,
                tx.id,
                blocks.finalized.0
            );
            return;
        };

        tracing::trace!(
                "Transaction {} with id {} is {:?}: block in receipt {receipt_block_number}, safe block {}, finalized block {}",
                tx_status.tx_hash,
                tx.id,
                finality_status,
                blocks.fast_finality.0,
                blocks.finalized.0
            );

        if finality_status == EthTxFinalityStatus::Finalized {
            self.health_updater.update(
                EthTxManagerHealthDetails {
                    last_finalized_tx: EthTxDetails::new(tx, Some((&tx_status).into())),
                    finalized_block: blocks.finalized,
                }
                .into(),
            );
        }

        if tx_status.success {
            self.confirm_tx(storage, tx, finality_status, tx_status)
                .await;
        } else {
            self.fail_tx(storage, tx, tx_status).await;
        }
    }

    fn operator_type(&self, tx: &EthTx) -> OperatorType {
        if tx.is_gateway {
            OperatorType::Gateway
        } else {
            match tx.from_addr {
                Some(a) if a == self.operator_address(OperatorType::NonBlob) => {
                    OperatorType::NonBlob
                }
                Some(a) if a == self.operator_address(OperatorType::Blob) => OperatorType::Blob,
                Some(a) => panic!("Cannot infer operator type for {a:#?}"),
                None => OperatorType::NonBlob,
            }
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

        if let Some(batch_number) = storage
            .blocks_dal()
            .get_batch_number_of_prove_tx_id(tx.id)
            .await
            .unwrap()
        {
            storage
                .eth_proof_manager_dal()
                .mark_batch_as_proven(batch_number, false)
                .await
                .unwrap();
            storage
                .eth_proof_manager_dal()
                .fallback_batch(batch_number)
                .await
                .unwrap();
            tracing::info!("Batch {} proven with false result", batch_number);
        }

        let failure_reason = self
            .l1_interface
            .failure_reason(tx_status.receipt.transaction_hash, self.operator_type(tx))
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
        eth_tx_finality_status: EthTxFinalityStatus,
        tx_status: ExecutedTxStatus,
    ) {
        let tx_hash = tx_status.receipt.transaction_hash;
        let gas_used = tx_status
            .receipt
            .gas_used
            .expect("light ETH clients are not supported");

        storage
            .eth_sender_dal()
            .confirm_tx(tx_status.tx_hash, eth_tx_finality_status, gas_used)
            .await
            .unwrap();

        METRICS
            .track_eth_tx_metrics(storage, L1Stage::Mined, tx)
            .await;

        tracing::info!(
            "eth_tx {} with hash {tx_hash:?} for {} is {:?}. Gas spent: {gas_used:?}",
            tx.id,
            tx.tx_type,
            eth_tx_finality_status,
        );
        let tx_type_label = tx.tx_type.into();
        METRICS.l1_gas_used[&tx_type_label].observe(gas_used.low_u128() as f64);

        let duration_since_epoch = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("incorrect system time");
        let tx_latency =
            duration_since_epoch.saturating_sub(Duration::from_secs(tx.created_at_timestamp));
        match eth_tx_finality_status {
            EthTxFinalityStatus::FastFinalized => {
                METRICS.l1_tx_fast_finalized_latency[&tx_type_label].observe(tx_latency);
            }
            EthTxFinalityStatus::Finalized => {
                METRICS.l1_tx_mined_latency[&tx_type_label].observe(tx_latency);
            }
            EthTxFinalityStatus::Pending => {
                // Do nothing txs were created, but not sent yet
            }
        }

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
        self.health_updater
            .update(Health::from(HealthStatus::Ready));

        let pool = self.pool.clone();

        loop {
            tokio::time::sleep(self.config.tx_poll_period).await;
            let mut storage = pool.connection_tagged("eth_sender").await.unwrap();

            if *stop_receiver.borrow() {
                tracing::info!("Stop request received, eth_tx_manager is shutting down");
                break;
            }
            let operator_to_track = self.l1_interface.supported_operator_types()[0];
            let l1_block_numbers = self
                .l1_interface
                .get_l1_block_numbers(operator_to_track)
                .await;

            if let Err(ref error) = l1_block_numbers {
                // Web3 API request failures can cause this,
                // and anything more important is already properly reported.
                tracing::warn!("eth_sender error {:?}", error);
                if error.is_retriable() {
                    METRICS.l1_transient_errors.inc();
                    continue;
                }
            }

            METRICS.track_block_numbers(&l1_block_numbers?);

            self.loop_iteration(&mut storage).await;
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
            .get_inflight_txs(
                self.operator_address(operator_type),
                operator_type == OperatorType::Gateway,
            )
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
                    self.operator_address(operator_type),
                    operator_type == OperatorType::Gateway,
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
                // If sending didn't succeed, we do not try to send next transactions
                // as we rely on `sent_successfully` being set sequentially.
                // Also, it doesn't make much sense to try anyway since we will get an error most likely
                // (nonce-too-high for blob transactions is guaranteed).
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
            let time_in_mempool_in_l1_blocks = l1_block_numbers.latest.0 - sent_at_block;

            self.send_eth_tx(
                storage,
                &tx,
                time_in_mempool_in_l1_blocks,
                l1_block_numbers.latest,
            )
            .await?;
        }
        Ok(())
    }

    #[tracing::instrument(skip_all, name = "EthTxManager::loop_iteration")]
    pub async fn loop_iteration(&mut self, storage: &mut Connection<'_, Core>) {
        // We can treat blob and non-blob operators independently as they have different nonces and
        // aggregator makes sure that corresponding Commit transaction is confirmed before creating
        // a PublishProof transaction
        for operator_type in self.l1_interface.supported_operator_types() {
            let l1_block_numbers = self
                .l1_interface
                .get_l1_block_numbers(operator_type)
                .await
                .unwrap();
            tracing::debug!(
                "Loop iteration at block {} for {operator_type:?} operator",
                l1_block_numbers.latest
            );
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
                if error.is_retriable() {
                    METRICS.l1_transient_errors.inc();
                }
            }
        }
    }

    /// Returns the health check for eth tx manager.
    pub fn health_check(&self) -> ReactiveHealthCheck {
        self.health_updater.subscribe()
    }
}

fn derive_l1_block_cap(multiplier_cap: u32, b: f64) -> u32 {
    (multiplier_cap as f64).log(b).ceil() as u32
}

#[cfg(test)]
mod tests {
    use super::derive_l1_block_cap;

    #[test]
    fn check_derive_l1_block_cap() {
        let multiplier_cap = 10;
        let b = 2.0;
        let expected_l1_block_cap = 4; // ceil(log_2(10))
        let actual_l1_block_cap = derive_l1_block_cap(multiplier_cap, b);
        assert_eq!(actual_l1_block_cap, expected_l1_block_cap);

        let multiplier_cap = 10;
        let b = 1.01;
        let expected_l1_block_cap = 232;
        let actual_l1_block_cap = derive_l1_block_cap(multiplier_cap, b);
        assert_eq!(actual_l1_block_cap, expected_l1_block_cap);
    }
}

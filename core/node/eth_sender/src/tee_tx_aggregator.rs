use std::collections::HashMap;

use tokio::sync::watch;
use zksync_config::configs::eth_sender::SenderConfig;
use zksync_dal::{
    tee_dcap_collateral_dal::{
        PendingCollateral, PendingFieldCollateral, PendingTcbInfoCollateral,
    },
    Connection, ConnectionPool, Core, CoreDal,
};
use zksync_eth_client::BoundEthInterface;
use zksync_health_check::{Health, HealthStatus, HealthUpdater, ReactiveHealthCheck};
use zksync_types::{
    aggregated_operations::AggregatedActionType, settlement::SettlementLayer, Address, SLChainId,
};

use crate::{publish_criterion::L1GasCriterion, EthSenderError};

#[derive(Debug)]
pub struct TeeTxAggregator {
    config: SenderConfig,
    eth_client_tee_dcap: Option<Box<dyn BoundEthInterface>>,
    pool: ConnectionPool<Core>,
    sl_chain_id: SLChainId,
    health_updater: HealthUpdater,
    settlement_layer: Option<SettlementLayer>,
    initial_pending_nonces: HashMap<Address, u64>,
}

impl TeeTxAggregator {
    /// Returns the health check for tee tx aggregator.
    pub fn health_check(&self) -> ReactiveHealthCheck {
        self.health_updater.subscribe()
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        pool: ConnectionPool<Core>,
        config: SenderConfig,
        eth_client: Box<dyn BoundEthInterface>,
        eth_client_blobs: Option<Box<dyn BoundEthInterface>>,
        eth_client_tee_dcap: Option<Box<dyn BoundEthInterface>>,
        settlement_layer: Option<SettlementLayer>,
    ) -> Self {
        let eth_client = eth_client.for_component("tee_tx_aggregator");
        let eth_client_blobs = eth_client_blobs.map(|c| c.for_component("tee_tx_aggregator"));
        let eth_client_tee_dcap = eth_client_tee_dcap.map(|c| c.for_component("tee_tx_aggregator"));

        let mut initial_pending_nonces = HashMap::new();
        for client in eth_client_blobs
            .iter()
            .chain(std::iter::once(&eth_client))
            .chain(eth_client_tee_dcap.iter())
        {
            let address = client.sender_account();
            let nonce = client.pending_nonce().await.unwrap().as_u64();

            initial_pending_nonces.insert(address, nonce);
        }

        let sl_chain_id = (*eth_client).as_ref().fetch_chain_id().await.unwrap();

        Self {
            config,
            eth_client_tee_dcap,
            pool,
            sl_chain_id,
            health_updater: ReactiveHealthCheck::new("tee_tx_aggregator").1,
            settlement_layer,
            initial_pending_nonces,
        }
    }

    pub async fn run(mut self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        self.health_updater
            .update(Health::from(HealthStatus::Ready));

        tracing::info!(
            "Initialized tee_tx_aggregator with is_pre_fflonk_verifier: {:?}",
            self.config.is_verifier_pre_fflonk
        );

        let pool = self.pool.clone();
        loop {
            let mut storage = pool.connection_tagged("eth_sender").await.unwrap();

            if *stop_receiver.borrow() {
                tracing::info!("Stop request received, tee_tx_aggregator is shutting down");
                break;
            }

            if let Err(err) = self.loop_iteration(&mut storage).await {
                // Web3 API request failures can cause this,
                // and anything more important is already properly reported.
                tracing::warn!("eth_sender error {err:?}");
            }

            tokio::time::sleep(self.config.aggregate_tx_poll_period).await;
        }
        Ok(())
    }

    #[tracing::instrument(skip_all, name = "EthTxAggregator::loop_iteration")]
    async fn loop_iteration(
        &mut self,
        storage: &mut Connection<'_, Core>,
    ) -> Result<(), EthSenderError> {
        self.aggregate_tee_dcap_transactions(storage).await?;
        self.aggregate_tee_attestations(storage).await?;
        self.aggregate_tee_proofs(storage).await?;
        Ok(())
    }

    // FIXME: TEE
    async fn aggregate_tee_proofs(
        &self,
        storage: &mut Connection<'_, Core>,
    ) -> Result<(), EthSenderError> {
        let pending = storage
            .tee_proof_generation_dal()
            .get_tee_proofs_for_eth_sender()
            .await
            .map_err(|_e| {
                // FIXME: TEE
                EthSenderError::ExceedMaxBaseFee
            })?;
        for (sig, calldata) in pending.into_iter() {
            // Start a database transaction
            let mut transaction = storage.start_transaction().await.unwrap();

            // Choose the appropriate client for TEE operations
            let sender_addr = self.eth_client_tee_dcap.as_ref().unwrap().sender_account();

            // Get the next nonce for the TEE transactions
            let nonce = self
                .get_next_nonce(&mut transaction, sender_addr, true)
                .await?;

            // Save the TEE transaction to the database
            let mut eth_tx = transaction
                .eth_sender_dal()
                .save_eth_tx(
                    nonce,
                    calldata,
                    AggregatedActionType::Tee,
                    self.eth_client_tee_dcap.as_ref().unwrap().contract_addr(),
                    // FIXME: TEE
                    Some(L1GasCriterion::total_tee_gas_amount()),
                    Some(sender_addr),
                    None, // No sidecar for TEE operations
                    false,
                )
                .await
                .unwrap();

            transaction
                .eth_sender_dal()
                .set_chain_id(eth_tx.id, self.sl_chain_id.0)
                .await
                .unwrap();
            eth_tx.chain_id = Some(self.sl_chain_id);

            transaction
                .tee_proof_generation_dal()
                .set_eth_tx_id_for_proof(&sig, eth_tx.id)
                .await
                .map_err(|_e| {
                    // FIXME: TEE
                    EthSenderError::ExceedMaxBaseFee
                })?;

            // Commit the transaction
            transaction.commit().await.unwrap();

            tracing::info!("eth_tx with ID {} for block proof", eth_tx.id);
        }
        Ok(())
    }

    async fn aggregate_tee_dcap_transactions(
        &self,
        storage: &mut Connection<'_, Core>,
    ) -> Result<(), EthSenderError> {
        let pending = storage
            .tee_dcap_collateral_dal()
            .get_pending_collateral_for_eth_tx()
            .await
            .map_err(|_e| {
                // FIXME: TEE
                EthSenderError::ExceedMaxBaseFee
            })?;

        // Generate TEE data
        for pending_collateral in pending.into_iter() {
            // Start a database transaction
            let mut transaction = storage.start_transaction().await.unwrap();

            // Choose the appropriate client for TEE operations
            let sender_addr = self.eth_client_tee_dcap.as_ref().unwrap().sender_account();

            // Get the next nonce for the TEE transactions
            let nonce = self
                .get_next_nonce(&mut transaction, sender_addr, true)
                .await?;

            // Save the TEE transaction to the database
            let mut eth_tx = transaction
                .eth_sender_dal()
                .save_eth_tx(
                    nonce,
                    pending_collateral.calldata().to_vec(),
                    AggregatedActionType::Tee,
                    self.eth_client_tee_dcap.as_ref().unwrap().contract_addr(),
                    // FIXME: TEE
                    Some(L1GasCriterion::total_tee_gas_amount()),
                    Some(sender_addr),
                    None, // No sidecar for TEE operations
                    false,
                )
                .await
                .unwrap();

            transaction
                .eth_sender_dal()
                .set_chain_id(eth_tx.id, self.sl_chain_id.0)
                .await
                .unwrap();
            eth_tx.chain_id = Some(self.sl_chain_id);

            match pending_collateral {
                PendingCollateral::Field(PendingFieldCollateral { kind, .. }) => {
                    transaction
                        .tee_dcap_collateral_dal()
                        .set_field_eth_tx_id(kind, eth_tx.id as i32)
                        .await
                        .map_err(|_e| {
                            // FIXME: TEE
                            EthSenderError::ExceedMaxBaseFee
                        })?;
                }
                PendingCollateral::TcbInfo(PendingTcbInfoCollateral { kind, fmspc, .. }) => {
                    transaction
                        .tee_dcap_collateral_dal()
                        .set_tcb_info_eth_tx_id(kind, &fmspc, eth_tx.id as i32)
                        .await
                        .map_err(|_e| {
                            // FIXME: TEE
                            EthSenderError::ExceedMaxBaseFee
                        })?;
                }
            }

            // Commit the transaction
            transaction.commit().await.unwrap();

            tracing::info!("eth_tx with ID {} for op TEE", eth_tx.id);
        }

        Ok(())
    }

    async fn aggregate_tee_attestations(
        &self,
        storage: &mut Connection<'_, Core>,
    ) -> Result<(), EthSenderError> {
        let pending = storage
            .tee_proof_generation_dal()
            .get_pending_attestations_for_eth_tx()
            .await
            .map_err(|_e| {
                // FIXME: TEE
                EthSenderError::ExceedMaxBaseFee
            })?;
        // Generate TEE data
        for (pubkey, calldata) in pending.into_iter() {
            // Start a database transaction
            let mut transaction = storage.start_transaction().await.unwrap();

            // Choose the appropriate client for TEE operations
            let sender_addr = self.eth_client_tee_dcap.as_ref().unwrap().sender_account();

            // Get the next nonce for the TEE transactions
            let nonce = self
                .get_next_nonce(&mut transaction, sender_addr, true)
                .await?;

            // Save the TEE transaction to the database
            let mut eth_tx = transaction
                .eth_sender_dal()
                .save_eth_tx(
                    nonce,
                    calldata,
                    AggregatedActionType::Tee,
                    self.eth_client_tee_dcap.as_ref().unwrap().contract_addr(),
                    // FIXME: TEE
                    Some(L1GasCriterion::total_tee_gas_amount()),
                    Some(sender_addr),
                    None, // No sidecar for TEE operations
                    false,
                )
                .await
                .unwrap();

            transaction
                .eth_sender_dal()
                .set_chain_id(eth_tx.id, self.sl_chain_id.0)
                .await
                .unwrap();
            eth_tx.chain_id = Some(self.sl_chain_id);

            transaction
                .tee_proof_generation_dal()
                .set_eth_tx_id_for_attestation(&pubkey, eth_tx.id)
                .await
                .map_err(|_e| {
                    // FIXME: TEE
                    EthSenderError::ExceedMaxBaseFee
                })?;

            // Commit the transaction
            transaction.commit().await.unwrap();

            tracing::info!(
                "eth_tx with ID {} for attestation for TEE pubkey {}",
                eth_tx.id,
                hex::encode(pubkey)
            );
        }

        Ok(())
    }

    // Just because we block all operations during gateway migration,
    // this function should not be called when the settlement layer is unknown
    fn is_gateway(&self) -> bool {
        self.settlement_layer
            .as_ref()
            .map(|sl| sl.is_gateway())
            .unwrap_or(false)
    }

    async fn get_next_nonce(
        &self,
        storage: &mut Connection<'_, Core>,
        from_addr: Address,
        is_non_blob_sender: bool,
    ) -> Result<u64, EthSenderError> {
        let is_gateway = self.is_gateway();
        let db_nonce = storage
            .eth_sender_dal()
            .get_next_nonce(from_addr, is_non_blob_sender, is_gateway)
            .await
            .unwrap()
            .unwrap_or(0);
        // Between server starts we can execute some txs using operator account or remove some txs from the database
        // At the start we have to consider this fact and get the max nonce.
        let l1_nonce = self.initial_pending_nonces[&from_addr];
        tracing::info!(
            "Next nonce from db: {}, nonce from L1: {} for address: {:?}",
            db_nonce,
            l1_nonce,
            from_addr
        );
        Ok(db_nonce.max(l1_nonce))
    }
}

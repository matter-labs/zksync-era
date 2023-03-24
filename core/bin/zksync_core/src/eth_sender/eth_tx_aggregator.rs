use crate::eth_sender::grafana_metrics::track_eth_tx_metrics;
use crate::eth_sender::zksync_functions::ZkSyncFunctions;
use crate::eth_sender::{zksync_functions, Aggregator, ETHSenderError};
use crate::gas_tracker::agg_block_base_cost;
use std::cmp::max;
use tokio::sync::watch;
use zksync_config::configs::eth_sender::SenderConfig;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_eth_client::clients::http_client::EthereumClient;
use zksync_types::{aggregated_operations::AggregatedOperation, eth_sender::EthTx, Address, H256};

/// The component is responsible for aggregating l1 batches into eth_txs:
/// Such as CommitBlocks, PublishProofBlocksOnchain and ExecuteBlock
/// These eth_txs will be used as a queue for generating signed txs and send them later
#[derive(Debug)]
pub struct EthTxAggregator {
    aggregator: Aggregator,
    config: SenderConfig,
    contract_address: Address,
    functions: ZkSyncFunctions,
    base_nonce: u64,
}

impl EthTxAggregator {
    pub fn new(
        config: SenderConfig,
        aggregator: Aggregator,
        contract_address: Address,
        base_nonce: u64,
    ) -> Self {
        let functions = zksync_functions::get_zksync_functions();
        Self {
            base_nonce,
            aggregator,
            config,
            contract_address,
            functions,
        }
    }

    pub async fn run(
        mut self,
        pool: ConnectionPool,
        eth_client: EthereumClient,
        stop_receiver: watch::Receiver<bool>,
    ) {
        loop {
            let base_system_contracts_hashes = self
                .get_l1_base_system_contracts_hashes(&eth_client)
                .await
                .unwrap();
            let mut storage = pool.access_storage().await;

            if *stop_receiver.borrow() {
                vlog::info!("Stop signal received, eth_tx_aggregator is shutting down");
                break;
            }

            if let Err(e) = self
                .loop_iteration(&mut storage, base_system_contracts_hashes)
                .await
            {
                // Web3 API request failures can cause this,
                // and anything more important is already properly reported.
                vlog::warn!("eth_sender error {:?}", e);
            }

            tokio::time::sleep(self.config.aggregate_tx_poll_period()).await;
        }
    }

    async fn get_l1_base_system_contracts_hashes(
        &mut self,
        eth_client: &EthereumClient,
    ) -> Result<BaseSystemContractsHashes, ETHSenderError> {
        let bootloader_code_hash: H256 = eth_client
            .call_main_contract_function(
                "getL2BootloaderBytecodeHash",
                (),
                None,
                Default::default(),
                None,
            )
            .await?;

        let default_account_code_hash: H256 = eth_client
            .call_main_contract_function(
                "getL2DefaultAccountBytecodeHash",
                (),
                None,
                Default::default(),
                None,
            )
            .await?;
        Ok(BaseSystemContractsHashes {
            bootloader: bootloader_code_hash,
            default_aa: default_account_code_hash,
        })
    }

    #[tracing::instrument(skip(self, storage, base_system_contracts_hashes))]
    async fn loop_iteration(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        base_system_contracts_hashes: BaseSystemContractsHashes,
    ) -> Result<(), ETHSenderError> {
        if let Some(agg_op) = self
            .aggregator
            .get_next_ready_operation(storage, base_system_contracts_hashes)
            .await
        {
            let tx = self.save_eth_tx(storage, &agg_op).await?;
            Self::log_eth_tx_saving(storage, agg_op, &tx).await;
        }
        Ok(())
    }

    async fn log_eth_tx_saving(
        storage: &mut StorageProcessor<'_>,
        aggregated_op: AggregatedOperation,
        tx: &EthTx,
    ) {
        vlog::info!(
            "eth_tx {} {} ({}-{}): saved",
            tx.id,
            aggregated_op.get_action_caption(),
            aggregated_op.get_block_range().0 .0,
            aggregated_op.get_block_range().1 .0,
        );

        if let AggregatedOperation::CommitBlocks(commit_op) = &aggregated_op {
            for block in &commit_op.blocks {
                metrics::histogram!(
                    "server.eth_sender.pubdata_size",
                    block.metadata.l2_l1_messages_compressed.len() as f64,
                    "kind" => "l2_l1_messages_compressed"
                );
                metrics::histogram!(
                    "server.eth_sender.pubdata_size",
                    block.metadata.initial_writes_compressed.len() as f64,
                    "kind" => "initial_writes_compressed"
                );
                metrics::histogram!(
                    "server.eth_sender.pubdata_size",
                    block.metadata.repeated_writes_compressed.len() as f64,
                    "kind" => "repeated_writes_compressed"
                );
            }
        }

        metrics::histogram!(
            "server.eth_sender.block_range_size",
            (aggregated_op.get_block_range().1.0 - aggregated_op.get_block_range().0.0 + 1) as f64,
            "type" => aggregated_op.get_action_type().to_string()
        );
        track_eth_tx_metrics(storage, "save", tx);
    }

    fn encode_aggregated_op(&self, op: &AggregatedOperation) -> Vec<u8> {
        match &op {
            AggregatedOperation::CommitBlocks(commit_blocks) => self
                .functions
                .commit_blocks
                .encode_input(&commit_blocks.get_eth_tx_args()),
            AggregatedOperation::PublishProofBlocksOnchain(prove_blocks) => self
                .functions
                .prove_blocks
                .encode_input(&prove_blocks.get_eth_tx_args()),
            AggregatedOperation::ExecuteBlocks(execute_blocks) => self
                .functions
                .execute_blocks
                .encode_input(&execute_blocks.get_eth_tx_args()),
        }
        .expect("Failed to encode transaction data.")
        .to_vec()
    }

    pub(super) async fn save_eth_tx(
        &self,
        storage: &mut StorageProcessor<'_>,
        aggregated_op: &AggregatedOperation,
    ) -> Result<EthTx, ETHSenderError> {
        let mut transaction = storage.start_transaction().await;
        let nonce = self.get_next_nonce(&mut transaction).await?;
        let calldata = self.encode_aggregated_op(aggregated_op);
        let (first_block, last_block) = aggregated_op.get_block_range();
        let op_type = aggregated_op.get_action_type();

        let blocks_predicted_gas =
            transaction
                .blocks_dal()
                .get_blocks_predicted_gas(first_block, last_block, op_type);
        let eth_tx_predicted_gas = agg_block_base_cost(op_type) + blocks_predicted_gas;

        let eth_tx = transaction.eth_sender_dal().save_eth_tx(
            nonce,
            calldata,
            op_type,
            self.contract_address,
            eth_tx_predicted_gas,
        );

        transaction
            .blocks_dal()
            .set_eth_tx_id(first_block, last_block, eth_tx.id, op_type);
        transaction.commit().await;
        Ok(eth_tx)
    }

    async fn get_next_nonce(
        &self,
        storage: &mut StorageProcessor<'_>,
    ) -> Result<u64, ETHSenderError> {
        let db_nonce = storage.eth_sender_dal().get_next_nonce().unwrap_or(0);
        // Between server starts we can execute some txs using operator account or remove some txs from the database
        // At the start we have to consider this fact and get the max nonce.
        Ok(max(db_nonce, self.base_nonce))
    }
}

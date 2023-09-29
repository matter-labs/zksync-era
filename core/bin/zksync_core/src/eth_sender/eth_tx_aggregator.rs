use tokio::sync::watch;

use zksync_config::configs::eth_sender::SenderConfig;
use zksync_contracts::{multicall_contract, zksync_contract, BaseSystemContractsHashes};
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_eth_client::BoundEthInterface;
use zksync_types::{
    aggregated_operations::AggregatedOperation,
    contracts::{Multicall3Call, Multicall3Result},
    eth_sender::EthTx,
    ethabi::Token,
    web3::contract::{tokens::Tokenizable, Error, Options},
    Address, H256,
};

use crate::eth_sender::{
    grafana_metrics::track_eth_tx_metrics, zksync_functions::ZkSyncFunctions, Aggregator,
    ETHSenderError,
};
use crate::gas_tracker::agg_l1_batch_base_cost;

/// The component is responsible for aggregating l1 batches into eth_txs:
/// Such as CommitBlocks, PublishProofBlocksOnchain and ExecuteBlock
/// These eth_txs will be used as a queue for generating signed txs and send them later
#[derive(Debug)]
pub struct EthTxAggregator {
    aggregator: Aggregator,
    config: SenderConfig,
    timelock_contract_address: Address,
    l1_multicall3_address: Address,
    pub(super) main_zksync_contract_address: Address,
    functions: ZkSyncFunctions,
    base_nonce: u64,
}

impl EthTxAggregator {
    pub fn new(
        config: SenderConfig,
        aggregator: Aggregator,
        timelock_contract_address: Address,
        l1_multicall3_address: Address,
        main_zksync_contract_address: Address,
        base_nonce: u64,
    ) -> Self {
        let functions = ZkSyncFunctions::default();
        Self {
            config,
            aggregator,
            timelock_contract_address,
            l1_multicall3_address,
            main_zksync_contract_address,
            functions,
            base_nonce,
        }
    }

    pub async fn run<E: BoundEthInterface>(
        mut self,
        pool: ConnectionPool,
        prover_pool: ConnectionPool,
        eth_client: E,
        stop_receiver: watch::Receiver<bool>,
    ) {
        loop {
            let mut storage = pool.access_storage_tagged("eth_sender").await;
            let mut prover_storage = prover_pool.access_storage_tagged("eth_sender").await;

            if *stop_receiver.borrow() {
                vlog::info!("Stop signal received, eth_tx_aggregator is shutting down");
                break;
            }

            if let Err(err) = self
                .loop_iteration(&mut storage, &mut prover_storage, &eth_client)
                .await
            {
                // Web3 API request failures can cause this,
                // and anything more important is already properly reported.
                vlog::warn!("eth_sender error {err:?}");
            }

            tokio::time::sleep(self.config.aggregate_tx_poll_period()).await;
        }
    }

    pub(super) async fn get_l1_base_system_contracts_hashes<E: BoundEthInterface>(
        &mut self,
        eth_client: &E,
    ) -> Result<BaseSystemContractsHashes, ETHSenderError> {
        let calldata = self.generate_calldata_for_multicall();

        let base_system_contracts_hashes_result = eth_client
            .call_contract_function(
                "aggregate3",
                calldata,
                None,
                Options::default(),
                None,
                self.l1_multicall3_address,
                multicall_contract(),
            )
            .await?;

        self.parse_base_system_contracts_hashes(base_system_contracts_hashes_result)
    }

    // Multicall's aggregate function accepts 1 argument - arrays of different contract calls.
    // The role of the method below is to tokenize input for multicall, which is actually a vector of tokens.
    // Each token describes a specific contract call.
    pub(super) fn generate_calldata_for_multicall(&self) -> Vec<Token> {
        let zksync_contract = zksync_contract();
        const ALLOW_FAILURE: bool = false;

        // First zksync contract call
        let get_l2_bootloader_hash_input = zksync_contract
            .function("getL2BootloaderBytecodeHash")
            .unwrap()
            .encode_input(&[])
            .unwrap();
        let get_bootloader_hash_call = Multicall3Call {
            target: self.main_zksync_contract_address,
            allow_failure: ALLOW_FAILURE,
            calldata: get_l2_bootloader_hash_input,
        };

        // Second zksync contract call
        let get_l2_default_aa_hash_input = zksync_contract
            .function("getL2DefaultAccountBytecodeHash")
            .unwrap()
            .encode_input(&[])
            .unwrap();
        let get_default_aa_hash_call = Multicall3Call {
            target: self.main_zksync_contract_address,
            allow_failure: ALLOW_FAILURE,
            calldata: get_l2_default_aa_hash_input,
        };

        // Convert structs into tokens and return vector with them
        vec![
            get_bootloader_hash_call.into_token(),
            get_default_aa_hash_call.into_token(),
        ]
    }

    // The role of the method below is to detokenize multicall call's result, which is actually a token.
    // This token is an array of tuples like (bool, bytes), that contain the status and result for each contract call.
    pub(super) fn parse_base_system_contracts_hashes(
        &self,
        token: Token,
    ) -> Result<BaseSystemContractsHashes, ETHSenderError> {
        let parse_error = |tokens: &[Token]| {
            Err(ETHSenderError::ParseError(Error::InvalidOutputType(
                format!(
                    "Failed to parse base system contracts hashes token: {:?}",
                    tokens
                ),
            )))
        };

        if let Token::Array(call_results) = token {
            // only 2 calls are aggregated in multicall
            if call_results.len() != 2 {
                return parse_error(&call_results);
            }
            let mut call_results_iterator = call_results.into_iter();

            let multicall3_bootloader =
                Multicall3Result::from_token(call_results_iterator.next().unwrap())?.return_data;

            if multicall3_bootloader.len() != 32 {
                return Err(ETHSenderError::ParseError(Error::InvalidOutputType(
                    format!(
                        "multicall3 bootloader hash data are not the len of 32: {:?}",
                        multicall3_bootloader
                    ),
                )));
            }
            let bootloader = H256::from_slice(&multicall3_bootloader);

            let multicall3_default_aa =
                Multicall3Result::from_token(call_results_iterator.next().unwrap())?.return_data;
            if multicall3_default_aa.len() != 32 {
                return Err(ETHSenderError::ParseError(Error::InvalidOutputType(
                    format!(
                        "multicall3 default aa hash data are not the len of 32: {:?}",
                        multicall3_default_aa
                    ),
                )));
            }
            let default_aa = H256::from_slice(&multicall3_default_aa);

            return Ok(BaseSystemContractsHashes {
                bootloader,
                default_aa,
            });
        }
        parse_error(&[token])
    }

    #[tracing::instrument(skip(self, storage, eth_client))]
    async fn loop_iteration<E: BoundEthInterface>(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        prover_storage: &mut StorageProcessor<'_>,
        eth_client: &E,
    ) -> Result<(), ETHSenderError> {
        let base_system_contracts_hashes =
            self.get_l1_base_system_contracts_hashes(eth_client).await?;
        if let Some(agg_op) = self
            .aggregator
            .get_next_ready_operation(storage, prover_storage, base_system_contracts_hashes)
            .await
        {
            let tx = self.save_eth_tx(storage, &agg_op).await?;
            Self::report_eth_tx_saving(storage, agg_op, &tx).await;
        }
        Ok(())
    }

    async fn report_eth_tx_saving(
        storage: &mut StorageProcessor<'_>,
        aggregated_op: AggregatedOperation,
        tx: &EthTx,
    ) {
        let l1_batch_number_range = aggregated_op.l1_batch_range();
        vlog::info!(
            "eth_tx with ID {} for op {} was saved for L1 batches {l1_batch_number_range:?}",
            tx.id,
            aggregated_op.get_action_caption()
        );

        if let AggregatedOperation::Commit(commit_op) = &aggregated_op {
            for batch in &commit_op.l1_batches {
                metrics::histogram!(
                    "server.eth_sender.pubdata_size",
                    batch.metadata.l2_l1_messages_compressed.len() as f64,
                    "kind" => "l2_l1_messages_compressed"
                );
                metrics::histogram!(
                    "server.eth_sender.pubdata_size",
                    batch.metadata.initial_writes_compressed.len() as f64,
                    "kind" => "initial_writes_compressed"
                );
                metrics::histogram!(
                    "server.eth_sender.pubdata_size",
                    batch.metadata.repeated_writes_compressed.len() as f64,
                    "kind" => "repeated_writes_compressed"
                );
            }
        }

        let range_size = l1_batch_number_range.end().0 - l1_batch_number_range.start().0 + 1;
        metrics::histogram!(
            "server.eth_sender.block_range_size",
            range_size as f64,
            "type" => aggregated_op.get_action_type().as_str()
        );
        track_eth_tx_metrics(storage, "save", tx).await;
    }

    fn encode_aggregated_op(&self, op: &AggregatedOperation) -> Vec<u8> {
        match &op {
            AggregatedOperation::Commit(op) => self
                .functions
                .commit_l1_batches
                .encode_input(&op.get_eth_tx_args()),
            AggregatedOperation::PublishProofOnchain(op) => self
                .functions
                .prove_l1_batches
                .encode_input(&op.get_eth_tx_args()),
            AggregatedOperation::Execute(op) => self
                .functions
                .execute_l1_batches
                .encode_input(&op.get_eth_tx_args()),
        }
        .expect("Failed to encode transaction data")
    }

    pub(super) async fn save_eth_tx(
        &self,
        storage: &mut StorageProcessor<'_>,
        aggregated_op: &AggregatedOperation,
    ) -> Result<EthTx, ETHSenderError> {
        let mut transaction = storage.start_transaction().await;
        let nonce = self.get_next_nonce(&mut transaction).await?;
        let calldata = self.encode_aggregated_op(aggregated_op);
        let l1_batch_number_range = aggregated_op.l1_batch_range();
        let op_type = aggregated_op.get_action_type();

        let predicted_gas_for_batches = transaction
            .blocks_dal()
            .get_l1_batches_predicted_gas(l1_batch_number_range.clone(), op_type)
            .await;
        let eth_tx_predicted_gas = agg_l1_batch_base_cost(op_type) + predicted_gas_for_batches;

        let eth_tx = transaction
            .eth_sender_dal()
            .save_eth_tx(
                nonce,
                calldata,
                op_type,
                self.timelock_contract_address,
                eth_tx_predicted_gas,
            )
            .await;

        transaction
            .blocks_dal()
            .set_eth_tx_id(l1_batch_number_range, eth_tx.id, op_type)
            .await;
        transaction.commit().await;
        Ok(eth_tx)
    }

    async fn get_next_nonce(
        &self,
        storage: &mut StorageProcessor<'_>,
    ) -> Result<u64, ETHSenderError> {
        let db_nonce = storage.eth_sender_dal().get_next_nonce().await.unwrap_or(0);
        // Between server starts we can execute some txs using operator account or remove some txs from the database
        // At the start we have to consider this fact and get the max nonce.
        Ok(db_nonce.max(self.base_nonce))
    }
}

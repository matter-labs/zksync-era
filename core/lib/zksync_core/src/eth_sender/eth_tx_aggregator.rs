use std::{convert::TryInto, sync::Arc};

use tokio::sync::watch;
use zksync_config::configs::eth_sender::SenderConfig;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_eth_client::{BoundEthInterface, CallFunctionArgs};
use zksync_l1_contract_interface::{
    multicall3::{Multicall3Call, Multicall3Result},
    pre_boojum_verifier::old_l1_vk_commitment,
    Detokenize, Tokenizable, Tokenize,
};
use zksync_types::{
    commitment::SerializeCommitment,
    eth_sender::EthTx,
    ethabi::{Contract, Token},
    l2_to_l1_log::UserL2ToL1Log,
    protocol_version::{L1VerifierConfig, VerifierParams},
    web3::contract::Error as Web3ContractError,
    Address, ProtocolVersionId, H256, U256,
};

use super::aggregated_operations::AggregatedOperation;
use crate::{
    eth_sender::{
        metrics::{PubdataKind, METRICS},
        zksync_functions::ZkSyncFunctions,
        Aggregator, ETHSenderError,
    },
    gas_tracker::agg_l1_batch_base_cost,
    metrics::BlockL1Stage,
};

/// Data queried from L1 using multicall contract.
#[derive(Debug)]
pub struct MulticallData {
    pub base_system_contracts_hashes: BaseSystemContractsHashes,
    pub verifier_params: VerifierParams,
    pub verifier_address: Address,
    pub protocol_version_id: ProtocolVersionId,
}

/// The component is responsible for aggregating l1 batches into eth_txs:
/// Such as CommitBlocks, PublishProofBlocksOnchain and ExecuteBlock
/// These eth_txs will be used as a queue for generating signed txs and send them later
#[derive(Debug)]
pub struct EthTxAggregator {
    aggregator: Aggregator,
    eth_client: Arc<dyn BoundEthInterface>,
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
        eth_client: Arc<dyn BoundEthInterface>,
        timelock_contract_address: Address,
        l1_multicall3_address: Address,
        main_zksync_contract_address: Address,
        base_nonce: u64,
    ) -> Self {
        let functions = ZkSyncFunctions::default();
        Self {
            config,
            aggregator,
            eth_client,
            timelock_contract_address,
            l1_multicall3_address,
            main_zksync_contract_address,
            functions,
            base_nonce,
        }
    }

    pub async fn run(
        mut self,
        pool: ConnectionPool,
        stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        loop {
            let mut storage = pool.access_storage_tagged("eth_sender").await.unwrap();

            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, eth_tx_aggregator is shutting down");
                break;
            }

            if let Err(err) = self.loop_iteration(&mut storage).await {
                // Web3 API request failures can cause this,
                // and anything more important is already properly reported.
                tracing::warn!("eth_sender error {err:?}");
            }

            tokio::time::sleep(self.config.aggregate_tx_poll_period()).await;
        }
        Ok(())
    }

    pub(super) async fn get_multicall_data(&mut self) -> Result<MulticallData, ETHSenderError> {
        let calldata = self.generate_calldata_for_multicall();
        let args = CallFunctionArgs::new(&self.functions.aggregate3.name, calldata).for_contract(
            self.l1_multicall3_address,
            self.functions.multicall_contract.clone(),
        );
        let aggregate3_result = self.eth_client.call_contract_function(args).await?;
        self.parse_multicall_data(Token::from_tokens(aggregate3_result)?)
    }

    // Multicall's aggregate function accepts 1 argument - arrays of different contract calls.
    // The role of the method below is to tokenize input for multicall, which is actually a vector of tokens.
    // Each token describes a specific contract call.
    pub(super) fn generate_calldata_for_multicall(&self) -> Vec<Token> {
        const ALLOW_FAILURE: bool = false;

        // First zksync contract call
        let get_l2_bootloader_hash_input = self
            .functions
            .get_l2_bootloader_bytecode_hash
            .encode_input(&[])
            .unwrap();
        let get_bootloader_hash_call = Multicall3Call {
            target: self.main_zksync_contract_address,
            allow_failure: ALLOW_FAILURE,
            calldata: get_l2_bootloader_hash_input,
        };

        // Second zksync contract call
        let get_l2_default_aa_hash_input = self
            .functions
            .get_l2_default_account_bytecode_hash
            .encode_input(&[])
            .unwrap();
        let get_default_aa_hash_call = Multicall3Call {
            target: self.main_zksync_contract_address,
            allow_failure: ALLOW_FAILURE,
            calldata: get_l2_default_aa_hash_input,
        };

        // Third zksync contract call
        let get_verifier_params_input = self
            .functions
            .get_verifier_params
            .encode_input(&[])
            .unwrap();
        let get_verifier_params_call = Multicall3Call {
            target: self.main_zksync_contract_address,
            allow_failure: ALLOW_FAILURE,
            calldata: get_verifier_params_input,
        };

        // Fourth zksync contract call
        let get_verifier_input = self.functions.get_verifier.encode_input(&[]).unwrap();
        let get_verifier_call = Multicall3Call {
            target: self.main_zksync_contract_address,
            allow_failure: ALLOW_FAILURE,
            calldata: get_verifier_input,
        };

        // Fifth zksync contract call
        let get_protocol_version_input = self
            .functions
            .get_protocol_version
            .encode_input(&[])
            .unwrap();
        let get_protocol_version_call = Multicall3Call {
            target: self.main_zksync_contract_address,
            allow_failure: ALLOW_FAILURE,
            calldata: get_protocol_version_input,
        };

        // Convert structs into tokens and return vector with them
        vec![
            get_bootloader_hash_call.into_token(),
            get_default_aa_hash_call.into_token(),
            get_verifier_params_call.into_token(),
            get_verifier_call.into_token(),
            get_protocol_version_call.into_token(),
        ]
    }

    // The role of the method below is to de-tokenize multicall call's result, which is actually a token.
    // This token is an array of tuples like `(bool, bytes)`, that contain the status and result for each contract call.
    pub(super) fn parse_multicall_data(
        &self,
        token: Token,
    ) -> Result<MulticallData, ETHSenderError> {
        let parse_error = |tokens: &[Token]| {
            Err(ETHSenderError::ParseError(
                Web3ContractError::InvalidOutputType(format!(
                    "Failed to parse multicall token: {:?}",
                    tokens
                )),
            ))
        };

        if let Token::Array(call_results) = token {
            // 5 calls are aggregated in multicall
            if call_results.len() != 5 {
                return parse_error(&call_results);
            }
            let mut call_results_iterator = call_results.into_iter();

            let multicall3_bootloader =
                Multicall3Result::from_token(call_results_iterator.next().unwrap())?.return_data;

            if multicall3_bootloader.len() != 32 {
                return Err(ETHSenderError::ParseError(
                    Web3ContractError::InvalidOutputType(format!(
                        "multicall3 bootloader hash data is not of the len of 32: {:?}",
                        multicall3_bootloader
                    )),
                ));
            }
            let bootloader = H256::from_slice(&multicall3_bootloader);

            let multicall3_default_aa =
                Multicall3Result::from_token(call_results_iterator.next().unwrap())?.return_data;
            if multicall3_default_aa.len() != 32 {
                return Err(ETHSenderError::ParseError(
                    Web3ContractError::InvalidOutputType(format!(
                        "multicall3 default aa hash data is not of the len of 32: {:?}",
                        multicall3_default_aa
                    )),
                ));
            }
            let default_aa = H256::from_slice(&multicall3_default_aa);
            let base_system_contracts_hashes = BaseSystemContractsHashes {
                bootloader,
                default_aa,
            };

            let multicall3_verifier_params =
                Multicall3Result::from_token(call_results_iterator.next().unwrap())?.return_data;
            if multicall3_verifier_params.len() != 96 {
                return Err(ETHSenderError::ParseError(
                    Web3ContractError::InvalidOutputType(format!(
                        "multicall3 verifier params data is not of the len of 96: {:?}",
                        multicall3_default_aa
                    )),
                ));
            }
            let recursion_node_level_vk_hash = H256::from_slice(&multicall3_verifier_params[..32]);
            let recursion_leaf_level_vk_hash =
                H256::from_slice(&multicall3_verifier_params[32..64]);
            let recursion_circuits_set_vks_hash =
                H256::from_slice(&multicall3_verifier_params[64..]);
            let verifier_params = VerifierParams {
                recursion_node_level_vk_hash,
                recursion_leaf_level_vk_hash,
                recursion_circuits_set_vks_hash,
            };

            let multicall3_verifier_address =
                Multicall3Result::from_token(call_results_iterator.next().unwrap())?.return_data;
            if multicall3_verifier_address.len() != 32 {
                return Err(ETHSenderError::ParseError(
                    Web3ContractError::InvalidOutputType(format!(
                        "multicall3 verifier address data is not of the len of 32: {:?}",
                        multicall3_verifier_address
                    )),
                ));
            }
            let verifier_address = Address::from_slice(&multicall3_verifier_address[12..]);

            let multicall3_protocol_version =
                Multicall3Result::from_token(call_results_iterator.next().unwrap())?.return_data;
            if multicall3_protocol_version.len() != 32 {
                return Err(ETHSenderError::ParseError(
                    Web3ContractError::InvalidOutputType(format!(
                        "multicall3 protocol version data is not of the len of 32: {:?}",
                        multicall3_protocol_version
                    )),
                ));
            }
            let protocol_version_id = U256::from_big_endian(&multicall3_protocol_version)
                .try_into()
                .unwrap();

            return Ok(MulticallData {
                base_system_contracts_hashes,
                verifier_params,
                verifier_address,
                protocol_version_id,
            });
        }
        parse_error(&[token])
    }

    /// Loads current verifier config on L1
    async fn get_recursion_scheduler_level_vk_hash(
        &mut self,
        verifier_address: Address,
        contracts_are_pre_boojum: bool,
    ) -> Result<H256, ETHSenderError> {
        // This is here for backward compatibility with the old verifier:
        // Pre-boojum verifier returns the full verification key;
        // New verifier returns the hash of the verification key
        tracing::debug!("Calling get_verification_key");
        if contracts_are_pre_boojum {
            let abi = Contract {
                functions: [(
                    self.functions.get_verification_key.name.clone(),
                    vec![self.functions.get_verification_key.clone()],
                )]
                .into(),
                ..Default::default()
            };
            let args = CallFunctionArgs::new(&self.functions.get_verification_key.name, ())
                .for_contract(verifier_address, abi);

            let vk = self.eth_client.call_contract_function(args).await?;
            Ok(old_l1_vk_commitment(Token::from_tokens(vk)?))
        } else {
            let get_vk_hash = self.functions.verification_key_hash.as_ref();
            tracing::debug!("Calling verificationKeyHash");
            let args = CallFunctionArgs::new(&get_vk_hash.unwrap().name, ())
                .for_contract(verifier_address, self.functions.verifier_contract.clone());
            let vk_hash = self.eth_client.call_contract_function(args).await?;
            Ok(H256::from_tokens(vk_hash)?)
        }
    }

    #[tracing::instrument(skip(self, storage))]
    async fn loop_iteration(
        &mut self,
        storage: &mut StorageProcessor<'_>,
    ) -> Result<(), ETHSenderError> {
        let MulticallData {
            base_system_contracts_hashes,
            verifier_params,
            verifier_address,
            protocol_version_id,
        } = self.get_multicall_data().await.map_err(|err| {
            tracing::error!("Failed to get multicall data {err:?}");
            err
        })?;
        let contracts_are_pre_boojum = protocol_version_id.is_pre_boojum();

        let recursion_scheduler_level_vk_hash = self
            .get_recursion_scheduler_level_vk_hash(verifier_address, contracts_are_pre_boojum)
            .await
            .map_err(|err| {
                tracing::error!("Failed to get VK hash from the Verifier {err:?}");
                err
            })?;
        let l1_verifier_config = L1VerifierConfig {
            params: verifier_params,
            recursion_scheduler_level_vk_hash,
        };
        if let Some(agg_op) = self
            .aggregator
            .get_next_ready_operation(
                storage,
                base_system_contracts_hashes,
                protocol_version_id,
                l1_verifier_config,
            )
            .await
        {
            let tx = self
                .save_eth_tx(storage, &agg_op, contracts_are_pre_boojum)
                .await?;
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
        tracing::info!(
            "eth_tx with ID {} for op {} was saved for L1 batches {l1_batch_number_range:?}",
            tx.id,
            aggregated_op.get_action_caption()
        );

        if let AggregatedOperation::Commit(commit_op) = &aggregated_op {
            for batch in &commit_op.l1_batches {
                METRICS.pubdata_size[&PubdataKind::StateDiffs]
                    .observe(batch.metadata.state_diffs_compressed.len());
                METRICS.pubdata_size[&PubdataKind::UserL2ToL1Logs]
                    .observe(batch.header.l2_to_l1_logs.len() * UserL2ToL1Log::SERIALIZED_SIZE);
                METRICS.pubdata_size[&PubdataKind::LongL2ToL1Messages]
                    .observe(batch.header.l2_to_l1_messages.iter().map(Vec::len).sum());
                METRICS.pubdata_size[&PubdataKind::RawPublishedBytecodes]
                    .observe(batch.raw_published_factory_deps.iter().map(Vec::len).sum());
            }
        }

        let range_size = l1_batch_number_range.end().0 - l1_batch_number_range.start().0 + 1;
        METRICS.block_range_size[&aggregated_op.get_action_type().into()]
            .observe(range_size.into());
        METRICS
            .track_eth_tx_metrics(storage, BlockL1Stage::Saved, tx)
            .await;
    }

    fn encode_aggregated_op(
        &self,
        op: &AggregatedOperation,
        contracts_are_pre_boojum: bool,
    ) -> Vec<u8> {
        let operation_is_pre_boojum = op.protocol_version().is_pre_boojum();

        // For "commit" and "prove" operations it's necessary that the contracts are of the same version as L1 batches are.
        // For "execute" it's not required, i.e. we can "execute" pre-boojum batches with post-boojum contracts.
        match op.clone() {
            AggregatedOperation::Commit(op) => {
                assert_eq!(contracts_are_pre_boojum, operation_is_pre_boojum);
                let f = if contracts_are_pre_boojum {
                    &self.functions.pre_boojum_commit
                } else {
                    self.functions
                        .post_boojum_commit
                        .as_ref()
                        .expect("Missing ABI for commitBatches")
                };
                f.encode_input(&op.into_tokens())
                    .expect("Failed to encode commit transaction data")
            }
            AggregatedOperation::PublishProofOnchain(op) => {
                assert_eq!(contracts_are_pre_boojum, operation_is_pre_boojum);
                let f = if contracts_are_pre_boojum {
                    &self.functions.pre_boojum_prove
                } else {
                    self.functions
                        .post_boojum_prove
                        .as_ref()
                        .expect("Missing ABI for proveBatches")
                };
                f.encode_input(&op.into_tokens())
                    .expect("Failed to encode prove transaction data")
            }
            AggregatedOperation::Execute(op) => {
                let f = if contracts_are_pre_boojum {
                    &self.functions.pre_boojum_execute
                } else {
                    self.functions
                        .post_boojum_execute
                        .as_ref()
                        .expect("Missing ABI for executeBatches")
                };
                f.encode_input(&op.into_tokens())
                    .expect("Failed to encode execute transaction data")
            }
        }
    }

    pub(super) async fn save_eth_tx(
        &self,
        storage: &mut StorageProcessor<'_>,
        aggregated_op: &AggregatedOperation,
        contracts_are_pre_boojum: bool,
    ) -> Result<EthTx, ETHSenderError> {
        let mut transaction = storage.start_transaction().await.unwrap();
        let nonce = self.get_next_nonce(&mut transaction).await?;
        let calldata = self.encode_aggregated_op(aggregated_op, contracts_are_pre_boojum);
        let l1_batch_number_range = aggregated_op.l1_batch_range();
        let op_type = aggregated_op.get_action_type();

        let predicted_gas_for_batches = transaction
            .blocks_dal()
            .get_l1_batches_predicted_gas(l1_batch_number_range.clone(), op_type)
            .await
            .unwrap();
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
            .await
            .unwrap();

        transaction
            .blocks_dal()
            .set_eth_tx_id(l1_batch_number_range, eth_tx.id, op_type)
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        Ok(eth_tx)
    }

    async fn get_next_nonce(
        &self,
        storage: &mut StorageProcessor<'_>,
    ) -> Result<u64, ETHSenderError> {
        let db_nonce = storage
            .eth_sender_dal()
            .get_next_nonce()
            .await
            .unwrap()
            .unwrap_or(0);
        // Between server starts we can execute some txs using operator account or remove some txs from the database
        // At the start we have to consider this fact and get the max nonce.
        Ok(db_nonce.max(self.base_nonce))
    }
}

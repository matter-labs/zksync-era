use std::{convert::TryInto, sync::Arc};

use tokio::sync::watch;
use zksync_config::configs::eth_sender::SenderConfig;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_eth_client::{BoundEthInterface, CallFunctionArgs};
use zksync_l1_contract_interface::{
    i_executor::commit::kzg::{KzgInfo, ZK_SYNC_BYTES_PER_BLOB},
    multicall3::{Multicall3Call, Multicall3Result},
    Detokenize, Tokenizable, Tokenize,
};
use zksync_types::{
    aggregated_operations::AggregatedActionType,
    commitment::SerializeCommitment,
    eth_sender::{EthTx, EthTxBlobSidecar, EthTxBlobSidecarV1, SidecarBlobV1},
    ethabi::Token,
    l2_to_l1_log::UserL2ToL1Log,
    protocol_version::{L1VerifierConfig, VerifierParams},
    pubdata_da::PubdataDA,
    web3::{contract::Error as Web3ContractError, types::BlockNumber},
    Address, L2ChainId, ProtocolVersionId, H256, U256,
};

use super::aggregated_operations::AggregatedOperation;
use crate::{
    eth_sender::{
        l1_batch_commit_data_generator::L1BatchCommitDataGenerator,
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
    base_nonce_custom_commit_sender: Option<u64>,
    rollup_chain_id: L2ChainId,
    /// If set to `Some` node is operating in the 4844 mode with two operator
    /// addresses at play: the main one and the custom address for sending commit
    /// transactions. The `Some` then contains the address of this custom operator
    /// address.
    custom_commit_sender_addr: Option<Address>,
    pool: ConnectionPool<Core>,
    l1_commit_data_generator: Arc<dyn L1BatchCommitDataGenerator>,
}

struct TxData {
    calldata: Vec<u8>,
    sidecar: Option<EthTxBlobSidecar>,
}

impl EthTxAggregator {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        pool: ConnectionPool<Core>,
        config: SenderConfig,
        aggregator: Aggregator,
        eth_client: Arc<dyn BoundEthInterface>,
        timelock_contract_address: Address,
        l1_multicall3_address: Address,
        main_zksync_contract_address: Address,
        rollup_chain_id: L2ChainId,
        custom_commit_sender_addr: Option<Address>,
        l1_commit_data_generator: Arc<dyn L1BatchCommitDataGenerator>,
    ) -> Self {
        let functions = ZkSyncFunctions::default();
        let base_nonce = eth_client
            .pending_nonce("eth_sender")
            .await
            .unwrap()
            .as_u64();

        let base_nonce_custom_commit_sender = match custom_commit_sender_addr {
            Some(addr) => Some(
                eth_client
                    .nonce_at_for_account(addr, BlockNumber::Pending, "eth_sender")
                    .await
                    .unwrap()
                    .as_u64(),
            ),
            None => None,
        };
        Self {
            config,
            aggregator,
            eth_client,
            timelock_contract_address,
            l1_multicall3_address,
            main_zksync_contract_address,
            functions,
            base_nonce,
            base_nonce_custom_commit_sender,
            rollup_chain_id,
            custom_commit_sender_addr,
            pool,
            l1_commit_data_generator,
        }
    }

    pub async fn run(mut self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let pool = self.pool.clone();
        loop {
            let mut storage = pool.connection_tagged("eth_sender").await.unwrap();

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
    ) -> Result<H256, ETHSenderError> {
        let get_vk_hash = &self.functions.verification_key_hash;
        let args = CallFunctionArgs::new(&get_vk_hash.name, ())
            .for_contract(verifier_address, self.functions.verifier_contract.clone());
        let vk_hash = self.eth_client.call_contract_function(args).await?;
        Ok(H256::from_tokens(vk_hash)?)
    }

    #[tracing::instrument(skip(self, storage))]
    async fn loop_iteration(
        &mut self,
        storage: &mut Connection<'_, Core>,
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
        let contracts_are_pre_shared_bridge = protocol_version_id.is_pre_shared_bridge();

        let recursion_scheduler_level_vk_hash = self
            .get_recursion_scheduler_level_vk_hash(verifier_address)
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
                .save_eth_tx(storage, &agg_op, contracts_are_pre_shared_bridge)
                .await?;
            Self::report_eth_tx_saving(storage, agg_op, &tx).await;
        }
        Ok(())
    }

    async fn report_eth_tx_saving(
        storage: &mut Connection<'_, Core>,
        aggregated_op: AggregatedOperation,
        tx: &EthTx,
    ) {
        let l1_batch_number_range = aggregated_op.l1_batch_range();
        tracing::info!(
            "eth_tx with ID {} for op {} was saved for L1 batches {l1_batch_number_range:?}",
            tx.id,
            aggregated_op.get_action_caption()
        );

        if let AggregatedOperation::Commit(_, l1_batches, _) = &aggregated_op {
            for batch in l1_batches {
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
        contracts_are_pre_shared_bridge: bool,
    ) -> TxData {
        let operation_is_pre_shared_bridge = op.protocol_version().is_pre_shared_bridge();
        assert_eq!(
            contracts_are_pre_shared_bridge,
            operation_is_pre_shared_bridge
        );

        let mut args = vec![Token::Uint(self.rollup_chain_id.as_u64().into())];

        let (calldata, sidecar) = match op.clone() {
            AggregatedOperation::Commit(last_committed_l1_batch, l1_batches, pubdata_da) => {
                let commit_data = self.l1_commit_data_generator.l1_commit_batches(
                    &last_committed_l1_batch,
                    &l1_batches,
                    &pubdata_da,
                );
                if contracts_are_pre_shared_bridge {
                    if let PubdataDA::Blobs = self.aggregator.pubdata_da() {
                        let calldata = self
                            .functions
                            .pre_shared_bridge_commit
                            .encode_input(&commit_data)
                            .expect("Failed to encode commit transaction data");

                        let side_car = l1_batches[0]
                            .header
                            .pubdata_input
                            .clone()
                            .unwrap()
                            .chunks(ZK_SYNC_BYTES_PER_BLOB)
                            .map(|blob| {
                                let kzg_info = KzgInfo::new(blob);
                                SidecarBlobV1 {
                                    blob: kzg_info.blob.to_vec(),
                                    commitment: kzg_info.kzg_commitment.to_vec(),
                                    proof: kzg_info.blob_proof.to_vec(),
                                    versioned_hash: kzg_info.versioned_hash.to_vec(),
                                }
                            })
                            .collect::<Vec<SidecarBlobV1>>();

                        let eth_tx_sidecar = EthTxBlobSidecarV1 { blobs: side_car };
                        (calldata, Some(eth_tx_sidecar.into()))
                    } else {
                        let calldata = self
                            .functions
                            .pre_shared_bridge_commit
                            .encode_input(&commit_data)
                            .expect("Failed to encode commit transaction data");
                        (calldata, None)
                    }
                } else {
                    args.extend(commit_data);
                    let calldata = self
                        .functions
                        .post_shared_bridge_commit
                        .as_ref()
                        .expect("Missing ABI for commitBatchesSharedBridge")
                        .encode_input(&args)
                        .expect("Failed to encode commit transaction data");
                    (calldata, None)
                }
            }
            AggregatedOperation::PublishProofOnchain(op) => {
                let calldata = if contracts_are_pre_shared_bridge {
                    self.functions
                        .pre_shared_bridge_prove
                        .encode_input(&op.into_tokens())
                        .expect("Failed to encode prove transaction data")
                } else {
                    args.extend(op.into_tokens());
                    self.functions
                        .post_shared_bridge_prove
                        .as_ref()
                        .expect("Missing ABI for proveBatchesSharedBridge")
                        .encode_input(&args)
                        .expect("Failed to encode prove transaction data")
                };
                (calldata, None)
            }
            AggregatedOperation::Execute(op) => {
                let calldata = if contracts_are_pre_shared_bridge {
                    self.functions
                        .pre_shared_bridge_execute
                        .encode_input(&op.into_tokens())
                        .expect("Failed to encode execute transaction data")
                } else {
                    args.extend(op.into_tokens());
                    self.functions
                        .post_shared_bridge_execute
                        .as_ref()
                        .expect("Missing ABI for executeBatchesSharedBridge")
                        .encode_input(&args)
                        .expect("Failed to encode execute transaction data")
                };
                (calldata, None)
            }
        };
        TxData { calldata, sidecar }
    }

    pub(super) async fn save_eth_tx(
        &self,
        storage: &mut Connection<'_, Core>,
        aggregated_op: &AggregatedOperation,
        contracts_are_pre_shared_bridge: bool,
    ) -> Result<EthTx, ETHSenderError> {
        let mut transaction = storage.start_transaction().await.unwrap();
        let op_type = aggregated_op.get_action_type();
        // We may be using a custom sender for commit transactions, so use this
        // var whatever it actually is: a `None` for single-addr operator or `Some`
        // for multi-addr operator in 4844 mode.
        let sender_addr = match op_type {
            AggregatedActionType::Commit => self.custom_commit_sender_addr,
            _ => None,
        };
        let nonce = self.get_next_nonce(&mut transaction, sender_addr).await?;
        let encoded_aggregated_op =
            self.encode_aggregated_op(aggregated_op, contracts_are_pre_shared_bridge);
        let l1_batch_number_range = aggregated_op.l1_batch_range();

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
                encoded_aggregated_op.calldata,
                op_type,
                self.timelock_contract_address,
                eth_tx_predicted_gas,
                sender_addr,
                encoded_aggregated_op.sidecar,
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
        storage: &mut Connection<'_, Core>,
        from_addr: Option<Address>,
    ) -> Result<u64, ETHSenderError> {
        let db_nonce = storage
            .eth_sender_dal()
            .get_next_nonce(from_addr)
            .await
            .unwrap()
            .unwrap_or(0);
        // Between server starts we can execute some txs using operator account or remove some txs from the database
        // At the start we have to consider this fact and get the max nonce.
        Ok(if from_addr.is_none() {
            db_nonce.max(self.base_nonce)
        } else {
            db_nonce.max(
                self.base_nonce_custom_commit_sender
                    .expect("custom base nonce is expected to be initialized; qed"),
            )
        })
    }
}

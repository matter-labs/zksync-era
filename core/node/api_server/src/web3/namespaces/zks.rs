use zksync_crypto_primitives::hasher::{keccak::KeccakHasher, Hasher};
use zksync_dal::{Connection, Core, CoreDal, DalError};
use zksync_mini_merkle_tree::MiniMerkleTree;
use zksync_shared_resources::tree::TreeApiError;
use zksync_system_constants::DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE;
use zksync_types::{
    api::{
        state_override::StateOverride, BlockDetails, BridgeAddresses, InteropMode, L1BatchDetails,
        L2ToL1LogProof, Proof, ProtocolVersion, StorageProof, TransactionDetails,
    },
    fee::Fee,
    fee_model::{FeeParams, PubdataIndependentBatchFeeModelInput},
    l1::L1Tx,
    l2::L2Tx,
    l2_to_l1_log::{l2_to_l1_logs_tree_size, L2ToL1Log, LOG_PROOF_SUPPORTED_METADATA_VERSION},
    transaction_request::CallRequest,
    AccountTreeId, L1BatchNumber, L2BlockNumber, ProtocolVersionId, StorageKey, Transaction,
    REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE, U256, U64,
};
use zksync_web3_decl::{
    error::{ClientRpcContext, Web3Error},
    namespaces::ZksNamespaceClient,
    types::{Address, H256},
};

use crate::{
    execution_sandbox::BlockArgs,
    tx_sender::BinarySearchKind,
    utils::open_readonly_transaction,
    web3::{backend_jsonrpsee::MethodTracer, RpcState},
};

#[derive(Debug)]
pub(crate) struct ZksNamespace {
    state: RpcState,
}

impl ZksNamespace {
    pub fn new(state: RpcState) -> Self {
        Self { state }
    }

    pub(crate) fn current_method(&self) -> &MethodTracer {
        &self.state.current_method
    }

    pub async fn estimate_fee_impl(
        &self,
        request: CallRequest,
        state_override: Option<StateOverride>,
    ) -> Result<Fee, Web3Error> {
        self.current_method()
            .observe_state_override(state_override.as_ref());

        let mut request_with_gas_per_pubdata_overridden = request;
        self.state
            .set_nonce_for_call_request(&mut request_with_gas_per_pubdata_overridden)
            .await?;

        if let Some(ref mut eip712_meta) = request_with_gas_per_pubdata_overridden.eip712_meta {
            eip712_meta.gas_per_pubdata = U256::from(DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE);
        }

        let mut connection = self.state.acquire_connection().await?;
        let block_args = BlockArgs::pending(
            &mut connection,
            self.state.api_config.settlement_layer.settlement_layer(),
        )
        .await?;
        drop(connection);
        let mut tx = L2Tx::from_request(
            request_with_gas_per_pubdata_overridden.into(),
            self.state.api_config.max_tx_size,
            block_args.use_evm_emulator(),
        )?;

        // When we're estimating fee, we are trying to deduce values related to fee, so we should
        // not consider provided ones.
        tx.common_data.fee.max_priority_fee_per_gas = 0u64.into();
        tx.common_data.fee.gas_per_pubdata_limit = U256::from(DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE);
        self.estimate_fee(tx.into(), block_args, state_override)
            .await
    }

    pub async fn estimate_l1_to_l2_gas_impl(
        &self,
        request: CallRequest,
        state_override: Option<StateOverride>,
    ) -> Result<U256, Web3Error> {
        self.current_method()
            .observe_state_override(state_override.as_ref());

        let mut request_with_gas_per_pubdata_overridden = request;
        // When we're estimating fee, we are trying to deduce values related to fee, so we should
        // not consider provided ones.
        if let Some(ref mut eip712_meta) = request_with_gas_per_pubdata_overridden.eip712_meta {
            if eip712_meta.gas_per_pubdata == U256::zero() {
                eip712_meta.gas_per_pubdata = REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE.into();
            }
        }

        let mut connection = self.state.acquire_connection().await?;
        let block_args = BlockArgs::pending(
            &mut connection,
            self.state.api_config.settlement_layer.settlement_layer(),
        )
        .await?;
        drop(connection);
        let tx = L1Tx::from_request(
            request_with_gas_per_pubdata_overridden,
            block_args.use_evm_emulator(),
        )
        .map_err(Web3Error::SerializationError)?;

        let fee = self
            .estimate_fee(tx.into(), block_args, state_override)
            .await?;
        Ok(fee.gas_limit)
    }

    async fn estimate_fee(
        &self,
        tx: Transaction,
        block_args: BlockArgs,
        state_override: Option<StateOverride>,
    ) -> Result<Fee, Web3Error> {
        self.current_method()
            .observe_state_override(state_override.as_ref());

        let scale_factor = self.state.api_config.estimate_gas_scale_factor;
        let acceptable_overestimation =
            self.state.api_config.estimate_gas_acceptable_overestimation;
        let search_kind = BinarySearchKind::new(self.state.api_config.estimate_gas_optimize_search);

        self.state
            .tx_sender
            .get_txs_fee_in_wei(
                tx,
                block_args,
                scale_factor,
                acceptable_overestimation as u64,
                state_override,
                search_kind,
            )
            .await
            .map_err(|err| self.current_method().map_submit_err(err))
    }

    pub fn get_bridgehub_contract_impl(&self) -> Option<Address> {
        self.state
            .api_config
            .l1_ecosystem_contracts
            .bridgehub_proxy_addr
    }

    pub fn get_main_l1_contract_impl(&self) -> Address {
        self.state.api_config.l1_diamond_proxy_addr
    }

    pub fn get_testnet_paymaster_impl(&self) -> Option<Address> {
        self.state.api_config.l2_testnet_paymaster_addr
    }

    pub async fn get_bridge_contracts_impl(&self) -> Result<BridgeAddresses, Web3Error> {
        self.state
            .bridge_addresses_handle
            .read()
            .await
            .ok_or_else(|| anyhow::anyhow!("bridge addresses are not initialized").into())
    }

    pub fn get_timestamp_asserter_impl(&self) -> Option<Address> {
        self.state.api_config.timestamp_asserter_address
    }

    pub fn l1_chain_id_impl(&self) -> U64 {
        U64::from(*self.state.api_config.l1_chain_id)
    }

    async fn get_l2_to_l1_log_proof_inner(
        &self,
        storage: &mut Connection<'_, Core>,
        l1_batch_number: L1BatchNumber,
        index_in_filtered_logs: usize,
        log_filter: impl Fn(&L2ToL1Log) -> bool,
        interop_mode: Option<InteropMode>,
    ) -> Result<Option<L2ToL1LogProof>, Web3Error> {
        let all_l1_logs_in_batch = storage
            .blocks_web3_dal()
            .get_l2_to_l1_logs(l1_batch_number)
            .await
            .map_err(DalError::generalize)?;

        let Some((l1_log_index, _)) = all_l1_logs_in_batch
            .iter()
            .enumerate()
            .filter(|(_, log)| log_filter(log))
            .nth(index_in_filtered_logs)
        else {
            return Ok(None);
        };

        let Some(batch_with_metadata) = storage
            .blocks_dal()
            .get_l1_batch_metadata(l1_batch_number)
            .await
            .map_err(DalError::generalize)?
        else {
            return Ok(None);
        };

        let merkle_tree_leaves = all_l1_logs_in_batch.iter().map(L2ToL1Log::to_bytes);

        let protocol_version = batch_with_metadata
            .header
            .protocol_version
            .unwrap_or_else(ProtocolVersionId::last_potentially_undefined);
        let tree_size = l2_to_l1_logs_tree_size(protocol_version);
        let (local_root, proof) = MiniMerkleTree::new(merkle_tree_leaves, Some(tree_size))
            .merkle_root_and_path(l1_log_index);

        if protocol_version.is_pre_gateway() {
            return Ok(Some(L2ToL1LogProof {
                proof,
                root: local_root,
                id: l1_log_index as u32,
                batch_number: l1_batch_number,
            }));
        }

        let aggregated_root = batch_with_metadata
            .metadata
            .aggregation_root
            .expect("`aggregation_root` must be present for post-gateway branch");
        let root = KeccakHasher.compress(&local_root, &aggregated_root);

        let mut log_leaf_proof = proof;
        log_leaf_proof.push(aggregated_root);

        let Some(sl_chain_id) = storage
            .eth_sender_dal()
            .get_batch_execute_chain_id(l1_batch_number)
            .await
            .map_err(DalError::generalize)?
        else {
            return Ok(None);
        };

        let (batch_proof_len, batch_chain_proof, is_final_node) =
            if sl_chain_id.0 != self.state.api_config.l1_chain_id.0 {
                let batch_chain_proof = if interop_mode == Some(InteropMode::ProofBasedGateway) {
                    // Serve a proof to Gateway's MessageRoot
                    storage
                        .blocks_dal()
                        .get_batch_chain_merkle_path_until_msg_root(l1_batch_number)
                        .await
                        .map_err(DalError::generalize)
                } else {
                    // Serve a proof to Gateway's ChainBatchRoot, used for withdrawals
                    storage
                        .blocks_dal()
                        .get_l1_batch_chain_merkle_path(l1_batch_number)
                        .await
                        .map_err(DalError::generalize)
                };

                if let Ok(Some(batch_chain_proof)) = batch_chain_proof {
                    (
                        batch_chain_proof.batch_proof_len,
                        batch_chain_proof.proof,
                        false,
                    )
                } else {
                    return Ok(None);
                }
            } else {
                (0, Vec::new(), true)
            };

        let proof = {
            let mut metadata = [0u8; 32];
            metadata[0] = LOG_PROOF_SUPPORTED_METADATA_VERSION;
            metadata[1] = log_leaf_proof.len() as u8;
            metadata[2] = batch_proof_len as u8;
            metadata[3] = if is_final_node { 1 } else { 0 };

            let mut result = vec![H256(metadata)];

            result.extend(log_leaf_proof);
            result.extend(batch_chain_proof);

            result
        };

        Ok(Some(L2ToL1LogProof {
            proof,
            root,
            id: l1_log_index as u32,
            batch_number: l1_batch_number,
        }))
    }

    pub async fn get_l2_to_l1_log_proof_impl(
        &self,
        tx_hash: H256,
        index: Option<usize>,
        interop_mode: Option<InteropMode>,
    ) -> Result<Option<L2ToL1LogProof>, Web3Error> {
        if let Some(handler) = &self.state.l2_l1_log_proof_handler {
            return handler
                .get_l2_to_l1_log_proof(tx_hash, index, interop_mode)
                .rpc_context("get_l2_to_l1_log_proof")
                .await
                .map_err(Into::into);
        }

        let mut storage = self.state.acquire_connection().await?;
        // kl todo for precommit based, we need it based on blocks.
        let Some((l1_batch_number, l1_batch_tx_index)) = storage
            .blocks_web3_dal()
            .get_l1_batch_info_for_tx(tx_hash)
            .await
            .map_err(DalError::generalize)?
        else {
            return Ok(None);
        };

        self.state
            .start_info
            .ensure_not_pruned(l1_batch_number, &mut storage)
            .await?;

        let log_proof = self
            .get_l2_to_l1_log_proof_inner(
                &mut storage,
                l1_batch_number,
                index.unwrap_or(0),
                |log| log.tx_number_in_block == l1_batch_tx_index,
                interop_mode,
            )
            .await?;
        Ok(log_proof)
    }

    pub async fn get_l1_batch_number_impl(&self) -> Result<U64, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        let l1_batch_number = storage
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await
            .map_err(DalError::generalize)?
            .ok_or(Web3Error::NoBlock)?;
        Ok(l1_batch_number.0.into())
    }

    pub async fn get_l2_block_range_impl(
        &self,
        batch: L1BatchNumber,
    ) -> Result<Option<(U64, U64)>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        self.state
            .start_info
            .ensure_not_pruned(batch, &mut storage)
            .await?;
        let range = storage
            .blocks_web3_dal()
            .get_l2_block_range_of_l1_batch(batch)
            .await
            .map_err(DalError::generalize)?;
        Ok(range.map(|(min, max)| (U64::from(min.0), U64::from(max.0))))
    }

    pub async fn get_block_details_impl(
        &self,
        block_number: L2BlockNumber,
    ) -> Result<Option<BlockDetails>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        self.state
            .start_info
            .ensure_not_pruned(block_number, &mut storage)
            .await?;

        Ok(storage
            .blocks_web3_dal()
            .get_block_details(block_number)
            .await
            .map_err(DalError::generalize)?)
    }

    pub async fn get_raw_block_transactions_impl(
        &self,
        block_number: L2BlockNumber,
    ) -> Result<Vec<Transaction>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        self.state
            .start_info
            .ensure_not_pruned(block_number, &mut storage)
            .await?;

        Ok(storage
            .transactions_web3_dal()
            .get_raw_l2_block_transactions(block_number)
            .await
            .map_err(DalError::generalize)?)
    }

    pub async fn get_transaction_details_impl(
        &self,
        hash: H256,
    ) -> Result<Option<TransactionDetails>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        // Open a readonly transaction to have a consistent view of Postgres
        let mut storage = open_readonly_transaction(&mut storage).await?;
        let mut tx_details = storage
            .transactions_web3_dal()
            .get_transaction_details(hash)
            .await
            .map_err(DalError::generalize)?;

        if tx_details.is_none() {
            tx_details = self
                .state
                .tx_sink()
                .lookup_tx_details(&mut storage, hash)
                .await?;
        }
        Ok(tx_details)
    }

    pub async fn get_l1_batch_details_impl(
        &self,
        batch_number: L1BatchNumber,
    ) -> Result<Option<L1BatchDetails>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        self.state
            .start_info
            .ensure_not_pruned(batch_number, &mut storage)
            .await?;

        Ok(storage
            .blocks_web3_dal()
            .get_l1_batch_details(batch_number)
            .await
            .map_err(DalError::generalize)?)
    }

    pub async fn get_bytecode_by_hash_impl(
        &self,
        hash: H256,
    ) -> Result<Option<Vec<u8>>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        Ok(storage
            .factory_deps_dal()
            .get_sealed_factory_dep(hash)
            .await
            .map_err(DalError::generalize)?)
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_fee_params_impl(&self) -> FeeParams {
        self.state
            .tx_sender
            .0
            .batch_fee_input_provider
            .get_fee_model_params()
            .await
    }

    #[deprecated]
    #[allow(deprecated)]
    pub async fn get_protocol_version_impl(
        &self,
        version_id: Option<u16>,
    ) -> Result<Option<ProtocolVersion>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        let protocol_version = if let Some(id) = version_id {
            storage
                .protocol_versions_web3_dal()
                .get_protocol_version_by_id(id)
                .await
                .map_err(DalError::generalize)?
        } else {
            Some(
                storage
                    .protocol_versions_web3_dal()
                    .get_latest_protocol_version()
                    .await
                    .map_err(DalError::generalize)?,
            )
        };
        Ok(protocol_version)
    }

    pub async fn get_proofs_impl(
        &self,
        address: Address,
        keys: Vec<H256>,
        l1_batch_number: L1BatchNumber,
    ) -> Result<Option<Proof>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        self.state
            .start_info
            .ensure_not_pruned(l1_batch_number, &mut storage)
            .await?;
        let hashed_keys = keys
            .iter()
            .map(|key| StorageKey::new(AccountTreeId::new(address), *key).hashed_key_u256())
            .collect();
        let tree_api = self
            .state
            .tree_api
            .as_deref()
            .ok_or(Web3Error::MethodNotImplemented)?;
        let proofs_result = tree_api.get_proofs(l1_batch_number, hashed_keys).await;
        let proofs = match proofs_result {
            Ok(proofs) => proofs,
            Err(TreeApiError::NotReady(_)) => return Err(Web3Error::TreeApiUnavailable),
            Err(TreeApiError::NoVersion {
                missing_version,
                version_count,
            }) => {
                return if missing_version > version_count {
                    Ok(None)
                } else {
                    Err(Web3Error::InternalError(anyhow::anyhow!(
                        "L1 batch #{l1_batch_number} is pruned in Merkle tree, but not in Postgres"
                    )))
                };
            }
            Err(TreeApiError::Internal(err)) => return Err(Web3Error::InternalError(err)),
            Err(_) => {
                // This branch is not expected to be executed, but has to be provided since the error is non-exhaustive.
                return Err(Web3Error::InternalError(anyhow::anyhow!(
                    "Unspecified tree API error"
                )));
            }
        };

        let storage_proof = proofs
            .into_iter()
            .zip(keys)
            .map(|(proof, key)| StorageProof {
                key,
                proof: proof.merkle_path,
                value: proof.value,
                index: proof.index,
            })
            .collect();

        Ok(Some(Proof {
            address,
            storage_proof,
        }))
    }

    pub fn get_base_token_l1_address_impl(&self) -> Result<Address, Web3Error> {
        self.state
            .api_config
            .base_token_address
            .ok_or(Web3Error::MethodNotImplemented)
    }

    pub fn get_l2_multicall3_impl(&self) -> Result<Option<Address>, Web3Error> {
        Ok(self.state.api_config.l2_multicall3)
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_batch_fee_input_impl(
        &self,
    ) -> Result<PubdataIndependentBatchFeeModelInput, Web3Error> {
        Ok(self
            .state
            .tx_sender
            .scaled_batch_fee_input()
            .await?
            .into_pubdata_independent())
    }

    pub async fn gas_per_pubdata_impl(&self) -> Result<U256, Web3Error> {
        let (_, gas_per_pubdata) = self.state.tx_sender.gas_price_and_gas_per_pubdata().await?;
        // We don't accept transactions with `gas_per_pubdata=0` so API should always return 1 at the
        // bare minimum.
        let gas_per_pubdata = gas_per_pubdata.max(1);
        Ok(gas_per_pubdata.into())
    }
}

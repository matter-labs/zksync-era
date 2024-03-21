use std::{collections::HashMap, convert::TryInto};

use anyhow::Context as _;
use zksync_dal::{Connection, Core, CoreDal};
use zksync_mini_merkle_tree::MiniMerkleTree;
use zksync_system_constants::DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE;
use zksync_types::{
    api::{
        BlockDetails, BridgeAddresses, GetLogsFilter, L1BatchDetails, L2ToL1LogProof, Proof,
        ProtocolVersion, StorageProof, TransactionDetails,
    },
    fee::Fee,
    fee_model::FeeParams,
    l1::L1Tx,
    l2::L2Tx,
    l2_to_l1_log::{l2_to_l1_logs_tree_size, L2ToL1Log},
    tokens::ETHEREUM_ADDRESS,
    transaction_request::CallRequest,
    utils::storage_key_for_standard_token_balance,
    AccountTreeId, L1BatchNumber, MiniblockNumber, ProtocolVersionId, StorageKey, Transaction,
    L1_MESSENGER_ADDRESS, L2_ETH_TOKEN_ADDRESS, REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE, U256, U64,
};
use zksync_utils::{address_to_h256, h256_to_u256};
use zksync_web3_decl::{
    error::Web3Error,
    types::{Address, Token, H256},
};

use crate::api_server::{
    tree::TreeApiError,
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

    async fn connection(&self) -> Result<Connection<'_, Core>, Web3Error> {
        Ok(self.state.connection_pool.connection_tagged("api").await?)
    }

    #[tracing::instrument(skip(self, request))]
    pub async fn estimate_fee_impl(&self, request: CallRequest) -> Result<Fee, Web3Error> {
        let mut request_with_gas_per_pubdata_overridden = request;
        self.state
            .set_nonce_for_call_request(&mut request_with_gas_per_pubdata_overridden)
            .await?;

        if let Some(ref mut eip712_meta) = request_with_gas_per_pubdata_overridden.eip712_meta {
            eip712_meta.gas_per_pubdata = U256::from(DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE);
        }

        let mut tx = L2Tx::from_request(
            request_with_gas_per_pubdata_overridden.into(),
            self.state.api_config.max_tx_size,
        )?;

        // When we're estimating fee, we are trying to deduce values related to fee, so we should
        // not consider provided ones.
        tx.common_data.fee.max_priority_fee_per_gas = 0u64.into();
        tx.common_data.fee.gas_per_pubdata_limit = U256::from(DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE);
        self.estimate_fee(tx.into()).await
    }

    #[tracing::instrument(skip(self, request))]
    pub async fn estimate_l1_to_l2_gas_impl(
        &self,
        request: CallRequest,
    ) -> Result<U256, Web3Error> {
        let mut request_with_gas_per_pubdata_overridden = request;
        // When we're estimating fee, we are trying to deduce values related to fee, so we should
        // not consider provided ones.
        if let Some(ref mut eip712_meta) = request_with_gas_per_pubdata_overridden.eip712_meta {
            if eip712_meta.gas_per_pubdata == U256::zero() {
                eip712_meta.gas_per_pubdata = REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE.into();
            }
        }

        let tx: L1Tx = request_with_gas_per_pubdata_overridden
            .try_into()
            .map_err(Web3Error::SerializationError)?;

        let fee = self.estimate_fee(tx.into()).await?;
        Ok(fee.gas_limit)
    }

    async fn estimate_fee(&self, tx: Transaction) -> Result<Fee, Web3Error> {
        let scale_factor = self.state.api_config.estimate_gas_scale_factor;
        let acceptable_overestimation =
            self.state.api_config.estimate_gas_acceptable_overestimation;

        Ok(self
            .state
            .tx_sender
            .get_txs_fee_in_wei(tx, scale_factor, acceptable_overestimation)
            .await?)
    }

    #[tracing::instrument(skip(self))]
    pub fn get_bridgehub_contract_impl(&self) -> Option<Address> {
        self.state.api_config.bridgehub_proxy_addr
    }

    #[tracing::instrument(skip(self))]
    pub fn get_main_contract_impl(&self) -> Address {
        self.state.api_config.diamond_proxy_addr
    }

    #[tracing::instrument(skip(self))]
    pub fn get_testnet_paymaster_impl(&self) -> Option<Address> {
        self.state.api_config.l2_testnet_paymaster_addr
    }

    #[tracing::instrument(skip(self))]
    pub fn get_bridge_contracts_impl(&self) -> BridgeAddresses {
        self.state.api_config.bridge_addresses.clone()
    }

    #[tracing::instrument(skip(self))]
    pub fn l1_chain_id_impl(&self) -> U64 {
        U64::from(*self.state.api_config.l1_chain_id)
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_confirmed_tokens_impl(
        &self,
        from: u32,
        limit: u8,
    ) -> Result<Vec<Token>, Web3Error> {
        let mut storage = self.connection().await?;
        let tokens = storage
            .tokens_web3_dal()
            .get_well_known_tokens()
            .await
            .context("get_well_known_tokens")?;

        let tokens = tokens
            .into_iter()
            .skip(from as usize)
            .take(limit.into())
            .map(|token_info| Token {
                l1_address: token_info.l1_address,
                l2_address: token_info.l2_address,
                name: token_info.metadata.name,
                symbol: token_info.metadata.symbol,
                decimals: token_info.metadata.decimals,
            })
            .collect();
        Ok(tokens)
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_all_account_balances_impl(
        &self,
        address: Address,
    ) -> Result<HashMap<Address, U256>, Web3Error> {
        let mut storage = self.connection().await?;
        let tokens = storage
            .tokens_dal()
            .get_all_l2_token_addresses()
            .await
            .context("get_all_l2_token_addresses")?;
        let hashed_balance_keys = tokens.iter().map(|&token_address| {
            let token_account = AccountTreeId::new(if token_address == ETHEREUM_ADDRESS {
                L2_ETH_TOKEN_ADDRESS
            } else {
                token_address
            });
            let hashed_key =
                storage_key_for_standard_token_balance(token_account, &address).hashed_key();
            (hashed_key, (hashed_key, token_address))
        });
        let (hashed_balance_keys, hashed_key_to_token_address): (Vec<_>, HashMap<_, _>) =
            hashed_balance_keys.unzip();

        let balance_values = storage
            .storage_web3_dal()
            .get_values(&hashed_balance_keys)
            .await
            .context("get_values")?;

        let balances = balance_values
            .into_iter()
            .filter_map(|(hashed_key, balance)| {
                let balance = h256_to_u256(balance);
                if balance.is_zero() {
                    return None;
                }
                Some((hashed_key_to_token_address[&hashed_key], balance))
            })
            .collect();
        Ok(balances)
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_l2_to_l1_msg_proof_impl(
        &self,
        block_number: MiniblockNumber,
        sender: Address,
        msg: H256,
        l2_log_position: Option<usize>,
    ) -> Result<Option<L2ToL1LogProof>, Web3Error> {
        let mut storage = self.connection().await?;
        self.state
            .start_info
            .ensure_not_pruned(block_number, &mut storage)
            .await?;

        let Some(l1_batch_number) = storage
            .blocks_web3_dal()
            .get_l1_batch_number_of_miniblock(block_number)
            .await
            .context("get_l1_batch_number_of_miniblock")?
        else {
            return Ok(None);
        };
        let (first_miniblock_of_l1_batch, _) = storage
            .blocks_web3_dal()
            .get_miniblock_range_of_l1_batch(l1_batch_number)
            .await
            .context("get_miniblock_range_of_l1_batch")?
            .context("L1 batch should contain at least one miniblock")?;

        // Position of l1 log in L1 batch relative to logs with identical data
        let l1_log_relative_position = if let Some(l2_log_position) = l2_log_position {
            let logs = storage
                .events_web3_dal()
                .get_logs(
                    GetLogsFilter {
                        from_block: first_miniblock_of_l1_batch,
                        to_block: block_number,
                        addresses: vec![L1_MESSENGER_ADDRESS],
                        topics: vec![(2, vec![address_to_h256(&sender)]), (3, vec![msg])],
                    },
                    self.state.api_config.req_entities_limit,
                )
                .await
                .context("get_logs")?;
            let maybe_pos = logs.iter().position(|event| {
                event.block_number == Some(block_number.0.into())
                    && event.log_index == Some(l2_log_position.into())
            });
            match maybe_pos {
                Some(pos) => pos,
                None => return Ok(None),
            }
        } else {
            0
        };

        let log_proof = self
            .get_l2_to_l1_log_proof_inner(
                &mut storage,
                l1_batch_number,
                l1_log_relative_position,
                |log| {
                    log.sender == L1_MESSENGER_ADDRESS
                        && log.key == address_to_h256(&sender)
                        && log.value == msg
                },
            )
            .await?;
        Ok(log_proof)
    }

    async fn get_l2_to_l1_log_proof_inner(
        &self,
        storage: &mut Connection<'_, Core>,
        l1_batch_number: L1BatchNumber,
        index_in_filtered_logs: usize,
        log_filter: impl Fn(&L2ToL1Log) -> bool,
    ) -> Result<Option<L2ToL1LogProof>, Web3Error> {
        let all_l1_logs_in_batch = storage
            .blocks_web3_dal()
            .get_l2_to_l1_logs(l1_batch_number)
            .await
            .context("get_l2_to_l1_logs")?;

        let Some((l1_log_index, _)) = all_l1_logs_in_batch
            .iter()
            .enumerate()
            .filter(|(_, log)| log_filter(log))
            .nth(index_in_filtered_logs)
        else {
            return Ok(None);
        };

        let Some(batch) = storage
            .blocks_dal()
            .get_l1_batch_header(l1_batch_number)
            .await
            .context("get_l1_batch_header")?
        else {
            return Ok(None);
        };

        let merkle_tree_leaves = all_l1_logs_in_batch.iter().map(L2ToL1Log::to_bytes);

        let protocol_version = batch
            .protocol_version
            .unwrap_or_else(ProtocolVersionId::last_potentially_undefined);
        let tree_size = l2_to_l1_logs_tree_size(protocol_version);

        let (root, proof) = MiniMerkleTree::new(merkle_tree_leaves, Some(tree_size))
            .merkle_root_and_path(l1_log_index);
        Ok(Some(L2ToL1LogProof {
            proof,
            root,
            id: l1_log_index as u32,
        }))
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_l2_to_l1_log_proof_impl(
        &self,
        tx_hash: H256,
        index: Option<usize>,
    ) -> Result<Option<L2ToL1LogProof>, Web3Error> {
        let mut storage = self.connection().await?;
        let Some((l1_batch_number, l1_batch_tx_index)) = storage
            .blocks_web3_dal()
            .get_l1_batch_info_for_tx(tx_hash)
            .await
            .context("get_l1_batch_info_for_tx")?
        else {
            return Ok(None);
        };

        let log_proof = self
            .get_l2_to_l1_log_proof_inner(
                &mut storage,
                l1_batch_number,
                index.unwrap_or(0),
                |log| log.tx_number_in_block == l1_batch_tx_index,
            )
            .await?;
        Ok(log_proof)
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_l1_batch_number_impl(&self) -> Result<U64, Web3Error> {
        let mut storage = self.connection().await?;
        let l1_batch_number = storage
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await
            .context("get_sealed_l1_batch_number")?
            .ok_or(Web3Error::NoBlock)?;
        Ok(l1_batch_number.0.into())
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_miniblock_range_impl(
        &self,
        batch: L1BatchNumber,
    ) -> Result<Option<(U64, U64)>, Web3Error> {
        let mut storage = self.connection().await?;
        self.state
            .start_info
            .ensure_not_pruned(batch, &mut storage)
            .await?;
        let range = storage
            .blocks_web3_dal()
            .get_miniblock_range_of_l1_batch(batch)
            .await
            .context("get_miniblock_range_of_l1_batch")?;
        Ok(range.map(|(min, max)| (U64::from(min.0), U64::from(max.0))))
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_block_details_impl(
        &self,
        block_number: MiniblockNumber,
    ) -> Result<Option<BlockDetails>, Web3Error> {
        let mut storage = self.connection().await?;
        self.state
            .start_info
            .ensure_not_pruned(block_number, &mut storage)
            .await?;

        Ok(storage
            .blocks_web3_dal()
            .get_block_details(block_number)
            .await
            .context("get_block_details")?)
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_raw_block_transactions_impl(
        &self,
        block_number: MiniblockNumber,
    ) -> Result<Vec<Transaction>, Web3Error> {
        let mut storage = self.connection().await?;
        self.state
            .start_info
            .ensure_not_pruned(block_number, &mut storage)
            .await?;

        Ok(storage
            .transactions_web3_dal()
            .get_raw_miniblock_transactions(block_number)
            .await
            .context("get_raw_miniblock_transactions")?)
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_transaction_details_impl(
        &self,
        hash: H256,
    ) -> Result<Option<TransactionDetails>, Web3Error> {
        let mut storage = self.connection().await?;
        let mut tx_details = storage
            .transactions_web3_dal()
            .get_transaction_details(hash)
            .await
            .context("get_transaction_details")?;
        drop(storage);

        if tx_details.is_none() {
            tx_details = self.state.tx_sink().lookup_tx_details(hash).await?;
        }
        Ok(tx_details)
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_l1_batch_details_impl(
        &self,
        batch_number: L1BatchNumber,
    ) -> Result<Option<L1BatchDetails>, Web3Error> {
        let mut storage = self.connection().await?;
        self.state
            .start_info
            .ensure_not_pruned(batch_number, &mut storage)
            .await?;

        Ok(storage
            .blocks_web3_dal()
            .get_l1_batch_details(batch_number)
            .await
            .context("get_l1_batch_details")?)
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_bytecode_by_hash_impl(
        &self,
        hash: H256,
    ) -> Result<Option<Vec<u8>>, Web3Error> {
        let mut storage = self.connection().await?;
        Ok(storage
            .factory_deps_dal()
            .get_factory_dep(hash)
            .await
            .context("get_factory_dep")?)
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_l1_gas_price_impl(&self) -> U64 {
        let gas_price = self
            .state
            .tx_sender
            .0
            .batch_fee_input_provider
            .get_batch_fee_input()
            .await
            .l1_gas_price();
        gas_price.into()
    }

    #[tracing::instrument(skip(self))]
    pub fn get_fee_params_impl(&self) -> FeeParams {
        self.state
            .tx_sender
            .0
            .batch_fee_input_provider
            .get_fee_model_params()
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_protocol_version_impl(
        &self,
        version_id: Option<u16>,
    ) -> Result<Option<ProtocolVersion>, Web3Error> {
        let mut storage = self.connection().await?;
        let protocol_version = match version_id {
            Some(id) => {
                storage
                    .protocol_versions_web3_dal()
                    .get_protocol_version_by_id(id)
                    .await
            }
            None => Some(
                storage
                    .protocol_versions_web3_dal()
                    .get_latest_protocol_version()
                    .await,
            ),
        };
        Ok(protocol_version)
    }

    #[tracing::instrument(skip_all)]
    pub async fn get_proofs_impl(
        &self,
        address: Address,
        keys: Vec<H256>,
        l1_batch_number: L1BatchNumber,
    ) -> Result<Option<Proof>, Web3Error> {
        let mut storage = self.connection().await?;
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
            .ok_or(Web3Error::TreeApiUnavailable)?;
        let proofs_result = tree_api.get_proofs(l1_batch_number, hashed_keys).await;
        let proofs = match proofs_result {
            Ok(proofs) => proofs,
            Err(TreeApiError::NotReady) => return Err(Web3Error::TreeApiUnavailable),
            Err(TreeApiError::NoVersion(err)) => {
                return if err.missing_version > err.version_count {
                    Ok(None)
                } else {
                    Err(Web3Error::InternalError(anyhow::anyhow!(
                        "L1 batch #{l1_batch_number} is pruned in Merkle tree, but not in Postgres"
                    )))
                };
            }
            Err(TreeApiError::Internal(err)) => return Err(Web3Error::InternalError(err)),
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
}

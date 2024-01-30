use std::{collections::HashMap, convert::TryInto};

use bigdecimal::{BigDecimal, Zero};
use zksync_dal::StorageProcessor;
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
    l2_to_l1_log::L2ToL1Log,
    tokens::ETHEREUM_ADDRESS,
    transaction_request::CallRequest,
    AccountTreeId, L1BatchNumber, MiniblockNumber, StorageKey, Transaction, L1_MESSENGER_ADDRESS,
    L2_ETH_TOKEN_ADDRESS, REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE, U256, U64,
};
use zksync_utils::{address_to_h256, ratio_to_big_decimal_normalized};
use zksync_web3_decl::{
    error::Web3Error,
    types::{Address, Token, H256},
};

use crate::api_server::{
    tree::TreeApiClient,
    web3::{backend_jsonrpsee::internal_error, metrics::API_METRICS, RpcState},
};

#[derive(Debug)]
pub struct ZksNamespace {
    pub state: RpcState,
}

impl ZksNamespace {
    pub fn new(state: RpcState) -> Self {
        Self { state }
    }

    async fn access_storage(
        &self,
        method_name: &'static str,
    ) -> Result<StorageProcessor<'_>, Web3Error> {
        self.state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .map_err(|err| internal_error(method_name, err))
    }

    #[tracing::instrument(skip(self, request))]
    pub async fn estimate_fee_impl(&self, request: CallRequest) -> Result<Fee, Web3Error> {
        const METHOD_NAME: &str = "estimate_fee";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
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

        let fee = self.estimate_fee(tx.into(), METHOD_NAME).await?;
        method_latency.observe();
        Ok(fee)
    }

    #[tracing::instrument(skip(self, request))]
    pub async fn estimate_l1_to_l2_gas_impl(
        &self,
        request: CallRequest,
    ) -> Result<U256, Web3Error> {
        const METHOD_NAME: &str = "estimate_gas_l1_to_l2";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
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

        let fee = self.estimate_fee(tx.into(), METHOD_NAME).await?;
        method_latency.observe();
        Ok(fee.gas_limit)
    }

    async fn estimate_fee(
        &self,
        tx: Transaction,
        method_name: &'static str,
    ) -> Result<Fee, Web3Error> {
        let scale_factor = self.state.api_config.estimate_gas_scale_factor;
        let acceptable_overestimation =
            self.state.api_config.estimate_gas_acceptable_overestimation;

        self.state
            .tx_sender
            .get_txs_fee_in_wei(tx, scale_factor, acceptable_overestimation)
            .await
            .map_err(|err| err.into_web3_error(method_name))
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
        const METHOD_NAME: &str = "get_confirmed_tokens";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        let mut storage = self.access_storage(METHOD_NAME).await?;
        let tokens = storage
            .tokens_web3_dal()
            .get_well_known_tokens()
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?;

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
        method_latency.observe();
        Ok(tokens)
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_token_price_impl(&self, l2_token: Address) -> Result<BigDecimal, Web3Error> {
        const METHOD_NAME: &str = "get_token_price";

        /// Amount of possible symbols after the decimal dot in the USD.
        /// Used to convert `Ratio<BigUint>` to `BigDecimal`.
        const USD_PRECISION: usize = 100;
        /// Minimum amount of symbols after the decimal dot in the USD.
        /// Used to convert `Ratio<BigUint>` to `BigDecimal`.
        const MIN_PRECISION: usize = 2;

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        let token_price_result = {
            let mut storage = self.access_storage(METHOD_NAME).await?;
            storage.tokens_web3_dal().get_token_price(&l2_token).await
        };

        let result = match token_price_result {
            Ok(Some(price)) => Ok(ratio_to_big_decimal_normalized(
                &price.usd_price,
                USD_PRECISION,
                MIN_PRECISION,
            )),
            Ok(None) => Ok(BigDecimal::zero()),
            Err(err) => Err(internal_error(METHOD_NAME, err)),
        };

        method_latency.observe();
        result
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_all_account_balances_impl(
        &self,
        address: Address,
    ) -> Result<HashMap<Address, U256>, Web3Error> {
        const METHOD_NAME: &str = "get_all_balances";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        let mut storage = self.access_storage(METHOD_NAME).await?;
        let balances = storage
            .accounts_dal()
            .get_balances_for_address(address)
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?;

        let balances = balances
            .into_iter()
            .map(|(address, balance)| {
                if address == L2_ETH_TOKEN_ADDRESS {
                    (ETHEREUM_ADDRESS, balance)
                } else {
                    (address, balance)
                }
            })
            .collect();
        method_latency.observe();
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
        const METHOD_NAME: &str = "get_l2_to_l1_msg_proof";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        self.state.start_info.ensure_not_pruned(block_number)?;
        let mut storage = self.access_storage(METHOD_NAME).await?;
        let Some(l1_batch_number) = storage
            .blocks_web3_dal()
            .get_l1_batch_number_of_miniblock(block_number)
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?
        else {
            return Ok(None);
        };
        let (first_miniblock_of_l1_batch, _) = storage
            .blocks_web3_dal()
            .get_miniblock_range_of_l1_batch(l1_batch_number)
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?
            .expect("L1 batch should contain at least one miniblock");

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
                .map_err(|err| internal_error(METHOD_NAME, err))?;
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
                METHOD_NAME,
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

        method_latency.observe();
        Ok(log_proof)
    }

    async fn get_l2_to_l1_log_proof_inner(
        &self,
        method_name: &'static str,
        storage: &mut StorageProcessor<'_>,
        l1_batch_number: L1BatchNumber,
        index_in_filtered_logs: usize,
        log_filter: impl Fn(&L2ToL1Log) -> bool,
    ) -> Result<Option<L2ToL1LogProof>, Web3Error> {
        let all_l1_logs_in_batch = storage
            .blocks_web3_dal()
            .get_l2_to_l1_logs(l1_batch_number)
            .await
            .map_err(|err| internal_error(method_name, err))?;

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
            .map_err(|err| internal_error(method_name, err))?
        else {
            return Ok(None);
        };

        let merkle_tree_leaves = all_l1_logs_in_batch.iter().map(L2ToL1Log::to_bytes);

        let min_tree_size = if batch
            .protocol_version
            .map(|v| v.is_pre_boojum())
            .unwrap_or(true)
        {
            Some(L2ToL1Log::PRE_BOOJUM_MIN_L2_L1_LOGS_TREE_SIZE)
        } else {
            Some(L2ToL1Log::MIN_L2_L1_LOGS_TREE_SIZE)
        };

        let (root, proof) = MiniMerkleTree::new(merkle_tree_leaves, min_tree_size)
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
        const METHOD_NAME: &str = "get_l2_to_l1_msg_proof";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        let mut storage = self.access_storage(METHOD_NAME).await?;
        let Some((l1_batch_number, l1_batch_tx_index)) = storage
            .blocks_web3_dal()
            .get_l1_batch_info_for_tx(tx_hash)
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?
        else {
            return Ok(None);
        };

        let log_proof = self
            .get_l2_to_l1_log_proof_inner(
                METHOD_NAME,
                &mut storage,
                l1_batch_number,
                index.unwrap_or(0),
                |log| log.tx_number_in_block == l1_batch_tx_index,
            )
            .await?;

        method_latency.observe();
        Ok(log_proof)
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_l1_batch_number_impl(&self) -> Result<U64, Web3Error> {
        const METHOD_NAME: &str = "get_l1_batch_number";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        let mut storage = self.access_storage(METHOD_NAME).await?;
        let l1_batch_number = storage
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?
            .ok_or(Web3Error::NoBlock)?;

        method_latency.observe();
        Ok(l1_batch_number.0.into())
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_miniblock_range_impl(
        &self,
        batch: L1BatchNumber,
    ) -> Result<Option<(U64, U64)>, Web3Error> {
        const METHOD_NAME: &str = "get_miniblock_range";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        self.state.start_info.ensure_not_pruned(batch)?;
        let mut storage = self.access_storage(METHOD_NAME).await?;
        let minmax = storage
            .blocks_web3_dal()
            .get_miniblock_range_of_l1_batch(batch)
            .await
            .map(|minmax| minmax.map(|(min, max)| (U64::from(min.0), U64::from(max.0))))
            .map_err(|err| internal_error(METHOD_NAME, err));

        method_latency.observe();
        minmax
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_block_details_impl(
        &self,
        block_number: MiniblockNumber,
    ) -> Result<Option<BlockDetails>, Web3Error> {
        const METHOD_NAME: &str = "get_block_details";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        self.state.start_info.ensure_not_pruned(block_number)?;
        let mut storage = self.access_storage(METHOD_NAME).await?;
        let block_details = storage
            .blocks_web3_dal()
            .get_block_details(block_number)
            .await
            .map_err(|err| internal_error(METHOD_NAME, err));

        method_latency.observe();
        block_details
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_raw_block_transactions_impl(
        &self,
        block_number: MiniblockNumber,
    ) -> Result<Vec<Transaction>, Web3Error> {
        const METHOD_NAME: &str = "get_raw_block_transactions";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        self.state.start_info.ensure_not_pruned(block_number)?;
        let mut storage = self.access_storage(METHOD_NAME).await?;
        let transactions = storage
            .transactions_web3_dal()
            .get_raw_miniblock_transactions(block_number)
            .await
            .map_err(|err| internal_error(METHOD_NAME, err));

        method_latency.observe();
        transactions
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_transaction_details_impl(
        &self,
        hash: H256,
    ) -> Result<Option<TransactionDetails>, Web3Error> {
        const METHOD_NAME: &str = "get_transaction_details";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        let mut storage = self.access_storage(METHOD_NAME).await?;
        let mut tx_details = storage
            .transactions_web3_dal()
            .get_transaction_details(hash)
            .await
            .map_err(|err| internal_error(METHOD_NAME, err));
        drop(storage);

        if let Some(proxy) = &self.state.tx_sender.0.proxy {
            // We're running an external node - we should query the main node directly
            // in case the transaction was proxied but not yet synced back to us
            if matches!(tx_details, Ok(None)) {
                // If the transaction is not in the db, query main node for details
                tx_details = proxy
                    .request_tx_details(hash)
                    .await
                    .map_err(|err| internal_error(METHOD_NAME, err));
            }
        }

        method_latency.observe();
        tx_details
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_l1_batch_details_impl(
        &self,
        batch_number: L1BatchNumber,
    ) -> Result<Option<L1BatchDetails>, Web3Error> {
        const METHOD_NAME: &str = "get_l1_batch";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        self.state.start_info.ensure_not_pruned(batch_number)?;
        let mut storage = self.access_storage(METHOD_NAME).await?;
        let l1_batch = storage
            .blocks_web3_dal()
            .get_l1_batch_details(batch_number)
            .await
            .map_err(|err| internal_error(METHOD_NAME, err));

        method_latency.observe();
        l1_batch
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_bytecode_by_hash_impl(
        &self,
        hash: H256,
    ) -> Result<Option<Vec<u8>>, Web3Error> {
        const METHOD_NAME: &str = "get_bytecode_by_hash";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        let mut storage = self.access_storage(METHOD_NAME).await?;
        let bytecode = storage.storage_dal().get_factory_dep(hash).await;

        method_latency.observe();
        Ok(bytecode)
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_l1_gas_price_impl(&self) -> U64 {
        const METHOD_NAME: &str = "get_l1_gas_price";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        let gas_price = self
            .state
            .tx_sender
            .0
            .batch_fee_input_provider
            .get_batch_fee_input()
            .await
            .l1_gas_price();

        method_latency.observe();
        gas_price.into()
    }

    #[tracing::instrument(skip(self))]
    pub fn get_fee_params_impl(&self) -> FeeParams {
        const METHOD_NAME: &str = "get_fee_params";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        let fee_model_params = self
            .state
            .tx_sender
            .0
            .batch_fee_input_provider
            .get_fee_model_params();

        method_latency.observe();

        fee_model_params
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_protocol_version_impl(
        &self,
        version_id: Option<u16>,
    ) -> Result<Option<ProtocolVersion>, Web3Error> {
        const METHOD_NAME: &str = "get_protocol_version";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        let mut storage = self.access_storage(METHOD_NAME).await?;
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

        method_latency.observe();
        Ok(protocol_version)
    }

    #[tracing::instrument(skip_all)]
    pub async fn get_proofs_impl(
        &self,
        address: Address,
        keys: Vec<H256>,
        l1_batch_number: L1BatchNumber,
    ) -> Result<Proof, Web3Error> {
        const METHOD_NAME: &str = "get_proofs";

        self.state.start_info.ensure_not_pruned(l1_batch_number)?;
        let hashed_keys = keys
            .iter()
            .map(|key| StorageKey::new(AccountTreeId::new(address), *key).hashed_key_u256())
            .collect();
        let storage_proof = self
            .state
            .tree_api
            .as_ref()
            .ok_or(Web3Error::TreeApiUnavailable)?
            .get_proofs(l1_batch_number, hashed_keys)
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?
            .into_iter()
            .zip(keys)
            .map(|(proof, key)| StorageProof {
                key,
                proof: proof.merkle_path,
                value: proof.value,
                index: proof.index,
            })
            .collect();

        Ok(Proof {
            address,
            storage_proof,
        })
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_batch_pubdata_impl(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> Result<Vec<u8>, Web3Error> {
        const METHOD_NAME: &str = "get_batch_pubdata";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        self.state.start_info.ensure_not_pruned(l1_batch_number)?;
        let mut storage = self.access_storage(METHOD_NAME).await?;
        let pubdata = storage
            .blocks_dal()
            .get_batch_pubdata(l1_batch_number)
            .await
            .map_err(|_err| Web3Error::PubdataNotFound)?
            .unwrap_or_default();

        method_latency.observe();
        Ok(pubdata)
    }
}

use std::{collections::HashMap, convert::TryInto};

use bigdecimal::{BigDecimal, Zero};
use zksync_dal::StorageProcessor;

use zksync_mini_merkle_tree::MiniMerkleTree;
use zksync_types::{
    api::{
        BlockDetails, BridgeAddresses, GetLogsFilter, L1BatchDetails, L2ToL1LogProof, Proof,
        ProtocolVersion, StorageProof, TransactionDetails,
    },
    fee::Fee,
    l1::L1Tx,
    l2::L2Tx,
    l2_to_l1_log::L2ToL1Log,
    tokens::ETHEREUM_ADDRESS,
    transaction_request::CallRequest,
    AccountTreeId, L1BatchNumber, MiniblockNumber, StorageKey, Transaction, L1_MESSENGER_ADDRESS,
    L2_ETH_TOKEN_ADDRESS, MAX_GAS_PER_PUBDATA_BYTE, REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE, U256,
    U64,
};
use zksync_utils::{address_to_h256, ratio_to_big_decimal_normalized};
use zksync_web3_decl::{
    error::Web3Error,
    types::{Address, Filter, Log, Token, H256},
};

use crate::api_server::{
    tree::TreeApiClient,
    web3::{backend_jsonrpc::error::internal_error, metrics::API_METRICS, RpcState},
};
use crate::l1_gas_price::L1GasPriceProvider;

#[derive(Debug)]
pub struct ZksNamespace<G> {
    pub state: RpcState<G>,
}

impl<G> Clone for ZksNamespace<G> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
        }
    }
}

impl<G: L1GasPriceProvider> ZksNamespace<G> {
    pub fn new(state: RpcState<G>) -> Self {
        Self { state }
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
            eip712_meta.gas_per_pubdata = MAX_GAS_PER_PUBDATA_BYTE.into();
        }

        let mut tx = L2Tx::from_request(
            request_with_gas_per_pubdata_overridden.into(),
            self.state.api_config.max_tx_size,
        )?;

        // When we're estimating fee, we are trying to deduce values related to fee, so we should
        // not consider provided ones.
        tx.common_data.fee.max_priority_fee_per_gas = 0u64.into();
        tx.common_data.fee.gas_per_pubdata_limit = MAX_GAS_PER_PUBDATA_BYTE.into();

        let fee = self.estimate_fee(tx.into()).await?;
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

        let fee = self.estimate_fee(tx.into()).await?;
        method_latency.observe();
        Ok(fee.gas_limit)
    }

    async fn estimate_fee(&self, tx: Transaction) -> Result<Fee, Web3Error> {
        let scale_factor = self.state.api_config.estimate_gas_scale_factor;
        let acceptable_overestimation =
            self.state.api_config.estimate_gas_acceptable_overestimation;

        let fee = self
            .state
            .tx_sender
            .get_txs_fee_in_wei(tx, scale_factor, acceptable_overestimation)
            .await
            .map_err(|err| Web3Error::SubmitTransactionError(err.to_string(), err.data()))?;

        Ok(fee)
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
        let tokens = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap()
            .tokens_web3_dal()
            .get_well_known_tokens()
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?
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
            let mut storage = self
                .state
                .connection_pool
                .access_storage_tagged("api")
                .await
                .unwrap();
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
        let balances = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap()
            .accounts_dal()
            .get_balances_for_address(address)
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?
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
        let mut storage = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap();
        let l1_batch_number = match storage
            .blocks_web3_dal()
            .get_l1_batch_number_of_miniblock(block_number)
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?
        {
            Some(number) => number,
            None => return Ok(None),
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
                        to_block: Some(block_number.0.into()),
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

        let merkle_tree_leaves = all_l1_logs_in_batch.iter().map(L2ToL1Log::to_bytes);
        let min_tree_size = Some(L2ToL1Log::MIN_L2_L1_LOGS_TREE_SIZE);
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
        let mut storage = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap();
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
        let l1_batch_number = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap()
            .blocks_web3_dal()
            .get_sealed_l1_batch_number()
            .await
            .map(|n| U64::from(n.0))
            .map_err(|err| internal_error(METHOD_NAME, err));

        method_latency.observe();
        l1_batch_number
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_miniblock_range_impl(
        &self,
        batch: L1BatchNumber,
    ) -> Result<Option<(U64, U64)>, Web3Error> {
        const METHOD_NAME: &str = "get_miniblock_range";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        let minmax = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap()
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
        let block_details = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap()
            .blocks_web3_dal()
            .get_block_details(
                block_number,
                self.state.tx_sender.0.sender_config.fee_account_addr,
            )
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
        let transactions = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap()
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
        let mut tx_details = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap()
            .transactions_web3_dal()
            .get_transaction_details(hash)
            .await
            .map_err(|err| internal_error(METHOD_NAME, err));

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
        let l1_batch = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap()
            .blocks_web3_dal()
            .get_l1_batch_details(batch_number)
            .await
            .map_err(|err| internal_error(METHOD_NAME, err));

        method_latency.observe();
        l1_batch
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_bytecode_by_hash_impl(&self, hash: H256) -> Option<Vec<u8>> {
        const METHOD_NAME: &str = "get_bytecode_by_hash";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        let bytecode = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap()
            .storage_dal()
            .get_factory_dep(hash)
            .await;

        method_latency.observe();
        bytecode
    }

    #[tracing::instrument(skip(self))]
    pub fn get_l1_gas_price_impl(&self) -> U64 {
        const METHOD_NAME: &str = "get_l1_gas_price";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        let gas_price = self
            .state
            .tx_sender
            .0
            .l1_gas_price_source
            .estimate_effective_gas_price();

        method_latency.observe();
        gas_price.into()
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_protocol_version_impl(
        &self,
        version_id: Option<u16>,
    ) -> Option<ProtocolVersion> {
        const METHOD_NAME: &str = "get_protocol_version";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        let protocol_version = match version_id {
            Some(id) => {
                self.state
                    .connection_pool
                    .access_storage()
                    .await
                    .unwrap()
                    .protocol_versions_web3_dal()
                    .get_protocol_version_by_id(id)
                    .await
            }
            None => Some(
                self.state
                    .connection_pool
                    .access_storage()
                    .await
                    .unwrap()
                    .protocol_versions_web3_dal()
                    .get_latest_protocol_version()
                    .await,
            ),
        };

        method_latency.observe();
        protocol_version
    }

    #[tracing::instrument(skip_all)]
    pub async fn get_logs_with_virtual_blocks_impl(
        &self,
        filter: Filter,
    ) -> Result<Vec<Log>, Web3Error> {
        self.state.translate_get_logs(filter).await
    }

    #[tracing::instrument(skip_all)]
    pub async fn get_proofs_impl(
        &self,
        address: Address,
        keys: Vec<H256>,
        l1_batch_number: L1BatchNumber,
    ) -> Result<Proof, Web3Error> {
        const METHOD_NAME: &str = "get_proofs";

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
}

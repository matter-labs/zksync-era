use std::time::Instant;
use std::{collections::HashMap, convert::TryInto};

use bigdecimal::{BigDecimal, Zero};

use zksync_mini_merkle_tree::MiniMerkleTree;

use zksync_types::l2::L2Tx;
use zksync_types::{
    api::{
        BlockDetails, BridgeAddresses, GetLogsFilter, L1BatchDetails, L2ToL1LogProof,
        ProtocolVersion, TransactionDetails,
    },
    commitment::SerializeCommitment,
    fee::Fee,
    l1::L1Tx,
    l2_to_l1_log::L2ToL1Log,
    tokens::ETHEREUM_ADDRESS,
    transaction_request::CallRequest,
    L1BatchNumber, MiniblockNumber, Transaction, L1_MESSENGER_ADDRESS, L2_ETH_TOKEN_ADDRESS,
    MAX_GAS_PER_PUBDATA_BYTE, REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE, U256, U64,
};
use zksync_utils::address_to_h256;
use zksync_web3_decl::{
    error::Web3Error,
    types::{Address, Token, H256},
};

use crate::api_server::web3::{backend_jsonrpc::error::internal_error, RpcState};
use crate::fee_ticker::FeeTicker;
use crate::fee_ticker::{error::TickerError, TokenPriceRequestType};
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
        let start = Instant::now();
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

        metrics::histogram!("api.web3.call", start.elapsed(), "method" => "estimate_fee");
        Ok(fee)
    }

    #[tracing::instrument(skip(self, request))]
    pub async fn estimate_l1_to_l2_gas_impl(
        &self,
        request: CallRequest,
    ) -> Result<U256, Web3Error> {
        let start = Instant::now();
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

        metrics::histogram!("api.web3.call", start.elapsed(), "method" => "estimate_gas_l1_to_l2");
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

        let start = Instant::now();
        let tokens = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
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

        metrics::histogram!("api.web3.call", start.elapsed(), "method" => METHOD_NAME);
        Ok(tokens)
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_token_price_impl(&self, l2_token: Address) -> Result<BigDecimal, Web3Error> {
        const METHOD_NAME: &str = "get_token_price";

        let start = Instant::now();
        let token_price_result = {
            let mut storage = self
                .state
                .connection_pool
                .access_storage_tagged("api")
                .await;
            let mut tokens_web3_dal = storage.tokens_web3_dal();
            FeeTicker::get_l2_token_price(
                &mut tokens_web3_dal,
                TokenPriceRequestType::USDForOneToken,
                &l2_token,
            )
            .await
        };

        let result = match token_price_result {
            Ok(price) => Ok(price),
            Err(TickerError::PriceNotTracked(_)) => Ok(BigDecimal::zero()),
            Err(err) => Err(internal_error(METHOD_NAME, err)),
        };

        metrics::histogram!("api.web3.call", start.elapsed(), "method" => METHOD_NAME);
        result
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_all_account_balances_impl(
        &self,
        address: Address,
    ) -> Result<HashMap<Address, U256>, Web3Error> {
        const METHOD_NAME: &str = "get_all_balances";

        let start = Instant::now();
        let balances = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
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

        metrics::histogram!("api.web3.call", start.elapsed(), "method" => METHOD_NAME);
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

        let start = Instant::now();
        let mut storage = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await;
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

        let all_l1_logs_in_batch = storage
            .blocks_web3_dal()
            .get_l2_to_l1_logs(l1_batch_number)
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?;

        // Position of l1 log in L1 batch relative to logs with identical data
        let l1_log_relative_position = if let Some(l2_log_position) = l2_log_position {
            let pos = storage
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
                .map_err(|err| internal_error(METHOD_NAME, err))?
                .iter()
                .position(|event| {
                    event.block_number == Some(block_number.0.into())
                        && event.log_index == Some(l2_log_position.into())
                });
            match pos {
                Some(pos) => pos,
                None => {
                    return Ok(None);
                }
            }
        } else {
            0
        };

        let l1_log_index = match all_l1_logs_in_batch
            .iter()
            .enumerate()
            .filter(|(_, log)| {
                log.sender == L1_MESSENGER_ADDRESS
                    && log.key == address_to_h256(&sender)
                    && log.value == msg
            })
            .nth(l1_log_relative_position)
        {
            Some(nth_elem) => nth_elem.0,
            None => {
                return Ok(None);
            }
        };

        let merkle_tree_leaves = all_l1_logs_in_batch.iter().map(L2ToL1Log::to_bytes);
        let (root, proof) = MiniMerkleTree::new(merkle_tree_leaves, L2ToL1Log::LIMIT_PER_L1_BATCH)
            .merkle_root_and_path(l1_log_index);
        let msg_proof = L2ToL1LogProof {
            proof,
            root,
            id: l1_log_index as u32,
        };
        metrics::histogram!("api.web3.call", start.elapsed(), "method" => METHOD_NAME);
        Ok(Some(msg_proof))
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_l2_to_l1_log_proof_impl(
        &self,
        tx_hash: H256,
        index: Option<usize>,
    ) -> Result<Option<L2ToL1LogProof>, Web3Error> {
        const METHOD_NAME: &str = "get_l2_to_l1_msg_proof";

        let start = Instant::now();
        let mut storage = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await;
        let (l1_batch_number, l1_batch_tx_index) = match storage
            .blocks_web3_dal()
            .get_l1_batch_info_for_tx(tx_hash)
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?
        {
            Some(x) => x,
            None => return Ok(None),
        };

        let all_l1_logs_in_batch = storage
            .blocks_web3_dal()
            .get_l2_to_l1_logs(l1_batch_number)
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?;

        let l1_log_index = match all_l1_logs_in_batch
            .iter()
            .enumerate()
            .filter(|(_, log)| log.tx_number_in_block == l1_batch_tx_index)
            .nth(index.unwrap_or(0))
        {
            Some(nth_elem) => nth_elem.0,
            None => {
                return Ok(None);
            }
        };

        let merkle_tree_leaves = all_l1_logs_in_batch.iter().map(L2ToL1Log::to_bytes);
        let (root, proof) = MiniMerkleTree::new(merkle_tree_leaves, L2ToL1Log::LIMIT_PER_L1_BATCH)
            .merkle_root_and_path(l1_log_index);
        let msg_proof = L2ToL1LogProof {
            proof,
            root,
            id: l1_log_index as u32,
        };

        metrics::histogram!("api.web3.call", start.elapsed(), "method" => METHOD_NAME);
        Ok(Some(msg_proof))
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_l1_batch_number_impl(&self) -> Result<U64, Web3Error> {
        const METHOD_NAME: &str = "get_l1_batch_number";

        let start = Instant::now();
        let l1_batch_number = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .blocks_web3_dal()
            .get_sealed_l1_batch_number()
            .await
            .map(|n| U64::from(n.0))
            .map_err(|err| internal_error(METHOD_NAME, err));

        metrics::histogram!("api.web3.call", start.elapsed(), "endpoint" => METHOD_NAME);
        l1_batch_number
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_miniblock_range_impl(
        &self,
        batch: L1BatchNumber,
    ) -> Result<Option<(U64, U64)>, Web3Error> {
        const METHOD_NAME: &str = "get_miniblock_range";

        let start = Instant::now();
        let minmax = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .blocks_web3_dal()
            .get_miniblock_range_of_l1_batch(batch)
            .await
            .map(|minmax| minmax.map(|(min, max)| (U64::from(min.0), U64::from(max.0))))
            .map_err(|err| internal_error(METHOD_NAME, err));

        metrics::histogram!("api.web3.call", start.elapsed(), "endpoint" => METHOD_NAME);
        minmax
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_block_details_impl(
        &self,
        block_number: MiniblockNumber,
    ) -> Result<Option<BlockDetails>, Web3Error> {
        const METHOD_NAME: &str = "get_block_details";

        let start = Instant::now();
        let block_details = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .blocks_web3_dal()
            .get_block_details(
                block_number,
                self.state.tx_sender.0.sender_config.fee_account_addr,
            )
            .await
            .map_err(|err| internal_error(METHOD_NAME, err));

        metrics::histogram!("api.web3.call", start.elapsed(), "method" => METHOD_NAME);
        block_details
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_raw_block_transactions_impl(
        &self,
        block_number: MiniblockNumber,
    ) -> Result<Vec<Transaction>, Web3Error> {
        const METHOD_NAME: &str = "get_raw_block_transactions";

        let start = Instant::now();
        let transactions = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .transactions_web3_dal()
            .get_raw_miniblock_transactions(block_number)
            .await
            .map_err(|err| internal_error(METHOD_NAME, err));

        metrics::histogram!("api.web3.call", start.elapsed(), "method" => METHOD_NAME);
        transactions
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_transaction_details_impl(
        &self,
        hash: H256,
    ) -> Result<Option<TransactionDetails>, Web3Error> {
        const METHOD_NAME: &str = "get_transaction_details";

        let start = Instant::now();
        let mut tx_details = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
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

        metrics::histogram!("api.web3.call", start.elapsed(), "method" => METHOD_NAME);
        tx_details
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_l1_batch_details_impl(
        &self,
        batch_number: L1BatchNumber,
    ) -> Result<Option<L1BatchDetails>, Web3Error> {
        const METHOD_NAME: &str = "get_l1_batch";

        let start = Instant::now();
        let l1_batch = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .blocks_web3_dal()
            .get_l1_batch_details(batch_number)
            .await
            .map_err(|err| internal_error(METHOD_NAME, err));

        metrics::histogram!("api.web3.call", start.elapsed(), "method" => METHOD_NAME);
        l1_batch
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_bytecode_by_hash_impl(&self, hash: H256) -> Option<Vec<u8>> {
        const METHOD_NAME: &str = "get_bytecode_by_hash";

        let start = Instant::now();
        let bytecode = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .storage_dal()
            .get_factory_dep(hash)
            .await;

        metrics::histogram!("api.web3.call", start.elapsed(), "method" => METHOD_NAME);
        bytecode
    }

    #[tracing::instrument(skip(self))]
    pub fn get_l1_gas_price_impl(&self) -> U64 {
        const METHOD_NAME: &str = "get_l1_gas_price";

        let start = Instant::now();
        let gas_price = self
            .state
            .tx_sender
            .0
            .l1_gas_price_source
            .estimate_effective_gas_price();

        metrics::histogram!("api.web3.call", start.elapsed(), "method" => METHOD_NAME);
        gas_price.into()
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_protocol_version_impl(
        &self,
        version_id: Option<u16>,
    ) -> Option<ProtocolVersion> {
        let start = Instant::now();
        const METHOD_NAME: &str = "get_protocol_version";

        let protocol_version = match version_id {
            Some(id) => {
                self.state
                    .connection_pool
                    .access_storage()
                    .await
                    .protocol_versions_web3_dal()
                    .get_protocol_version_by_id(id)
                    .await
            }
            None => Some(
                self.state
                    .connection_pool
                    .access_storage()
                    .await
                    .protocol_versions_web3_dal()
                    .get_latest_protocol_version()
                    .await,
            ),
        };
        metrics::histogram!("api.web3.call", start.elapsed(), "method" => METHOD_NAME);

        protocol_version
    }
}

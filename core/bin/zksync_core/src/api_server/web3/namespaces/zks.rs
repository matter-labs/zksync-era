use bigdecimal::{BigDecimal, Zero};
use std::time::Instant;
use std::{collections::HashMap, convert::TryInto};

use zksync_mini_merkle_tree::mini_merkle_tree_proof;
use zksync_types::{
    api::{BridgeAddresses, GetLogsFilter, L2ToL1LogProof, TransactionDetails, U64},
    commitment::CommitmentSerializable,
    explorer_api::{BlockDetails, L1BatchDetails},
    fee::Fee,
    l1::L1Tx,
    l2_to_l1_log::L2ToL1Log,
    tokens::ETHEREUM_ADDRESS,
    transaction_request::{l2_tx_from_call_req, CallRequest},
    vm_trace::{ContractSourceDebugInfo, VmDebugTrace},
    L1BatchNumber, MiniblockNumber, Transaction, L1_MESSENGER_ADDRESS, L2_ETH_TOKEN_ADDRESS,
    MAX_GAS_PER_PUBDATA_BYTE, REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE, U256,
};
use zksync_utils::address_to_h256;
use zksync_web3_decl::{
    error::Web3Error,
    types::{Address, Token, H256},
};

use crate::api_server::web3::{backend_jsonrpc::error::internal_error, RpcState};
use crate::fee_ticker::{error::TickerError, TokenPriceRequestType};

#[cfg(feature = "openzeppelin_tests")]
use zksync_types::Bytes;

#[derive(Debug, Clone)]
pub struct ZksNamespace {
    pub state: RpcState,
}

impl ZksNamespace {
    pub fn new(state: RpcState) -> Self {
        Self { state }
    }

    #[tracing::instrument(skip(self, request))]
    pub fn estimate_fee_impl(&self, request: CallRequest) -> Result<Fee, Web3Error> {
        let start = Instant::now();

        let mut tx = l2_tx_from_call_req(request, self.state.config.api.web3_json_rpc.max_tx_size)?;

        // When we're estimating fee, we are trying to deduce values related to fee, so we should
        // not consider provided ones.
        let fair_l2_gas_price = self.state.tx_sender.0.state_keeper_config.fair_l2_gas_price;
        tx.common_data.fee.max_fee_per_gas = fair_l2_gas_price.into();
        tx.common_data.fee.max_priority_fee_per_gas = fair_l2_gas_price.into();
        tx.common_data.fee.gas_per_pubdata_limit = MAX_GAS_PER_PUBDATA_BYTE.into();

        let fee = self.estimate_fee(tx.into())?;

        metrics::histogram!("api.web3.call", start.elapsed(), "method" => "estimate_fee");
        Ok(fee)
    }

    #[tracing::instrument(skip(self, request))]
    pub fn estimate_l1_to_l2_gas_impl(&self, request: CallRequest) -> Result<U256, Web3Error> {
        let start = Instant::now();

        let mut tx: L1Tx = request.try_into().map_err(Web3Error::SerializationError)?;

        // When we're estimating fee, we are trying to deduce values related to fee, so we should
        // not consider provided ones.
        let fair_l2_gas_price = self.state.tx_sender.0.state_keeper_config.fair_l2_gas_price;
        tx.common_data.max_fee_per_gas = fair_l2_gas_price.into();
        if tx.common_data.gas_per_pubdata_limit == U256::zero() {
            tx.common_data.gas_per_pubdata_limit = REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE.into();
        }

        let fee = self.estimate_fee(tx.into())?;

        metrics::histogram!("api.web3.call", start.elapsed(), "method" => "estimate_gas_l1_to_l2");
        Ok(fee.gas_limit)
    }

    fn estimate_fee(&self, tx: Transaction) -> Result<Fee, Web3Error> {
        let scale_factor = self
            .state
            .config
            .api
            .web3_json_rpc
            .estimate_gas_scale_factor;
        let acceptable_overestimation = self
            .state
            .config
            .api
            .web3_json_rpc
            .estimate_gas_acceptable_overestimation;

        let fee = self
            .state
            .tx_sender
            .get_txs_fee_in_wei(tx, scale_factor, acceptable_overestimation)
            .map_err(|err| Web3Error::SubmitTransactionError(err.to_string()))?;

        Ok(fee)
    }

    #[tracing::instrument(skip(self))]
    pub fn get_main_contract_impl(&self) -> Address {
        self.state.config.contracts.diamond_proxy_addr
    }

    #[tracing::instrument(skip(self))]
    pub fn get_testnet_paymaster_impl(&self) -> Option<Address> {
        self.state.config.contracts.l2_testnet_paymaster_addr
    }

    #[tracing::instrument(skip(self))]
    pub fn get_bridge_contracts_impl(&self) -> BridgeAddresses {
        BridgeAddresses {
            l1_erc20_default_bridge: self.state.config.contracts.l1_erc20_bridge_proxy_addr,
            l2_erc20_default_bridge: self.state.config.contracts.l2_erc20_bridge_addr,
        }
    }

    #[tracing::instrument(skip(self))]
    pub fn l1_chain_id_impl(&self) -> U64 {
        U64::from(*self.state.config.chain.eth.network.chain_id())
    }

    #[tracing::instrument(skip(self))]
    pub fn get_confirmed_tokens_impl(&self, from: u32, limit: u8) -> Result<Vec<Token>, Web3Error> {
        let start = Instant::now();
        let endpoint_name = "get_confirmed_tokens";

        let tokens = self
            .state
            .connection_pool
            .access_storage_blocking()
            .tokens_web3_dal()
            .get_well_known_tokens()
            .map_err(|err| internal_error(endpoint_name, err))?
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

        metrics::histogram!("api.web3.call", start.elapsed(), "method" => endpoint_name);
        Ok(tokens)
    }

    #[tracing::instrument(skip(self))]
    pub fn get_token_price_impl(&self, l2_token: Address) -> Result<BigDecimal, Web3Error> {
        let start = Instant::now();
        let endpoint_name = "get_token_price";

        let result = match self
            .state
            .tx_sender
            .token_price(TokenPriceRequestType::USDForOneToken, l2_token)
        {
            Ok(price) => Ok(price),
            Err(TickerError::PriceNotTracked(_)) => Ok(BigDecimal::zero()),
            Err(err) => Err(internal_error(endpoint_name, err)),
        };

        metrics::histogram!("api.web3.call", start.elapsed(), "method" => endpoint_name);
        result
    }

    // This method is currently to be used for internal debug purposes only.
    // It should be reworked for being public (validate contract info and maybe store it elsewhere).
    #[tracing::instrument(skip(self, info))]
    pub fn set_contract_debug_info_impl(
        &self,
        address: Address,
        info: ContractSourceDebugInfo,
    ) -> bool {
        let start = Instant::now();

        self.state
            .connection_pool
            .access_storage_blocking()
            .storage_dal()
            .set_contract_source(address, info);

        metrics::histogram!("api.web3.call", start.elapsed(), "method" => "set_contract_debug_info");
        true
    }

    #[tracing::instrument(skip(self))]
    pub fn get_contract_debug_info_impl(
        &self,
        address: Address,
    ) -> Option<ContractSourceDebugInfo> {
        let start = Instant::now();

        let info = self
            .state
            .connection_pool
            .access_storage_blocking()
            .storage_dal()
            .get_contract_source(address);

        metrics::histogram!("api.web3.call", start.elapsed(), "method" => "get_contract_debug_info");
        info
    }

    #[tracing::instrument(skip(self))]
    pub fn get_transaction_trace_impl(&self, hash: H256) -> Option<VmDebugTrace> {
        let start = Instant::now();
        let mut storage = self.state.connection_pool.access_storage_blocking();
        let trace = storage.transactions_dal().get_trace(hash);
        let result = trace.map(|trace| {
            let mut storage_dal = storage.storage_dal();
            let mut sources = HashMap::new();
            for address in trace.contracts {
                let source = storage_dal.get_contract_source(address);
                sources.insert(address, source);
            }
            VmDebugTrace {
                steps: trace.steps,
                sources,
            }
        });

        metrics::histogram!("api.web3.call", start.elapsed(), "method" => "get_transaction_trace");
        result
    }

    #[tracing::instrument(skip(self))]
    pub fn get_all_account_balances_impl(
        &self,
        address: Address,
    ) -> Result<HashMap<Address, U256>, Web3Error> {
        let start = Instant::now();
        let endpoint_name = "get_all_balances";

        let balances = self
            .state
            .connection_pool
            .access_storage_blocking()
            .explorer()
            .accounts_dal()
            .get_balances_for_address(address)
            .map_err(|err| internal_error(endpoint_name, err))?
            .into_iter()
            .map(|(address, balance_item)| {
                if address == L2_ETH_TOKEN_ADDRESS {
                    (ETHEREUM_ADDRESS, balance_item.balance)
                } else {
                    (address, balance_item.balance)
                }
            })
            .collect();

        metrics::histogram!("api.web3.call", start.elapsed(), "method" => endpoint_name);
        Ok(balances)
    }

    #[tracing::instrument(skip(self))]
    pub fn get_l2_to_l1_msg_proof_impl(
        &self,
        block_number: MiniblockNumber,
        sender: Address,
        msg: H256,
        l2_log_position: Option<usize>,
    ) -> Result<Option<L2ToL1LogProof>, Web3Error> {
        let start = Instant::now();
        let endpoint_name = "get_l2_to_l1_msg_proof";

        let mut storage = self.state.connection_pool.access_storage_blocking();
        let l1_batch_number = match storage
            .blocks_web3_dal()
            .get_l1_batch_number_of_miniblock(block_number)
            .map_err(|err| internal_error(endpoint_name, err))?
        {
            Some(number) => number,
            None => return Ok(None),
        };
        let (first_miniblock_of_l1_batch, _) = storage
            .blocks_web3_dal()
            .get_miniblock_range_of_l1_batch(l1_batch_number)
            .map_err(|err| internal_error(endpoint_name, err))?
            .expect("L1 batch should contain at least one miniblock");

        let all_l1_logs_in_block = storage
            .blocks_web3_dal()
            .get_l2_to_l1_logs(l1_batch_number)
            .map_err(|err| internal_error(endpoint_name, err))?;

        // Position of l1 log in block relative to logs with identical data
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
                    self.state.req_entities_limit,
                )
                .map_err(|err| internal_error(endpoint_name, err))?
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

        let l1_log_index = match all_l1_logs_in_block
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
        let values: Vec<Vec<u8>> = all_l1_logs_in_block
            .into_iter()
            .map(|a| a.to_bytes())
            .collect();
        let mut proof: Vec<H256> = mini_merkle_tree_proof(
            values,
            l1_log_index,
            L2ToL1Log::SERIALIZED_SIZE,
            L2ToL1Log::limit_per_block(),
        )
        .into_iter()
        .map(|elem| H256::from_slice(&elem))
        .collect();
        let root = proof.pop().unwrap();
        let msg_proof = L2ToL1LogProof {
            proof,
            root,
            id: l1_log_index as u32,
        };
        metrics::histogram!("api.web3.call", start.elapsed(), "method" => endpoint_name);
        Ok(Some(msg_proof))
    }

    #[tracing::instrument(skip(self))]
    pub fn get_l2_to_l1_log_proof_impl(
        &self,
        tx_hash: H256,
        index: Option<usize>,
    ) -> Result<Option<L2ToL1LogProof>, Web3Error> {
        let start = Instant::now();
        let endpoint_name = "get_l2_to_l1_msg_proof";

        let mut storage = self.state.connection_pool.access_storage_blocking();
        let (l1_batch_number, l1_batch_tx_index) = match storage
            .blocks_web3_dal()
            .get_l1_batch_info_for_tx(tx_hash)
            .map_err(|err| internal_error(endpoint_name, err))?
        {
            Some(x) => x,
            None => return Ok(None),
        };

        let all_l1_logs_in_block = storage
            .blocks_web3_dal()
            .get_l2_to_l1_logs(l1_batch_number)
            .map_err(|err| internal_error(endpoint_name, err))?;

        let l1_log_index = match all_l1_logs_in_block
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

        let values: Vec<Vec<u8>> = all_l1_logs_in_block
            .into_iter()
            .map(|a| a.to_bytes())
            .collect();
        let mut proof: Vec<H256> = mini_merkle_tree_proof(
            values,
            l1_log_index,
            L2ToL1Log::SERIALIZED_SIZE,
            L2ToL1Log::limit_per_block(),
        )
        .into_iter()
        .map(|elem| H256::from_slice(&elem))
        .collect();
        let root = proof.pop().unwrap();

        let msg_proof = L2ToL1LogProof {
            proof,
            root,
            id: l1_log_index as u32,
        };

        metrics::histogram!("api.web3.call", start.elapsed(), "method" => endpoint_name);
        Ok(Some(msg_proof))
    }

    #[tracing::instrument(skip(self))]
    pub fn get_l1_batch_number_impl(&self) -> Result<U64, Web3Error> {
        let start = Instant::now();
        let endpoint_name = "get_l1_batch_number";

        let l1_batch_number = self
            .state
            .connection_pool
            .access_storage_blocking()
            .blocks_web3_dal()
            .get_sealed_l1_batch_number()
            .map(|n| U64::from(n.0))
            .map_err(|err| internal_error(endpoint_name, err));

        metrics::histogram!("api.web3.call", start.elapsed(), "endpoint" => endpoint_name);
        l1_batch_number
    }

    #[tracing::instrument(skip(self))]
    pub fn get_miniblock_range_impl(
        &self,
        batch: L1BatchNumber,
    ) -> Result<Option<(U64, U64)>, Web3Error> {
        let start = Instant::now();
        let endpoint_name = "get_miniblock_range";

        let minmax = self
            .state
            .connection_pool
            .access_storage_blocking()
            .blocks_web3_dal()
            .get_miniblock_range_of_l1_batch(batch)
            .map(|minmax| minmax.map(|(min, max)| (U64::from(min.0), U64::from(max.0))))
            .map_err(|err| internal_error(endpoint_name, err));

        metrics::histogram!("api.web3.call", start.elapsed(), "endpoint" => endpoint_name);
        minmax
    }

    #[tracing::instrument(skip(self))]
    pub fn get_block_details_impl(
        &self,
        block_number: MiniblockNumber,
    ) -> Result<Option<BlockDetails>, Web3Error> {
        let start = Instant::now();
        let endpoint_name = "get_block_details";

        let block_details = self
            .state
            .connection_pool
            .access_storage_blocking()
            .explorer()
            .blocks_dal()
            .get_block_details(block_number)
            .map_err(|err| internal_error(endpoint_name, err));

        metrics::histogram!("api.web3.call", start.elapsed(), "method" => endpoint_name);

        block_details
    }

    #[tracing::instrument(skip(self))]
    pub fn get_raw_block_transactions_impl(
        &self,
        block_number: MiniblockNumber,
    ) -> Result<Vec<zksync_types::Transaction>, Web3Error> {
        let start = Instant::now();
        let endpoint_name = "get_raw_block_transactions";

        let transactions = self
            .state
            .connection_pool
            .access_storage_blocking()
            .transactions_web3_dal()
            .get_raw_miniblock_transactions(block_number)
            .map_err(|err| internal_error(endpoint_name, err));

        metrics::histogram!("api.web3.call", start.elapsed(), "method" => endpoint_name);

        transactions
    }

    #[tracing::instrument(skip(self))]
    pub fn get_transaction_details_impl(
        &self,
        hash: H256,
    ) -> Result<Option<TransactionDetails>, Web3Error> {
        let start = Instant::now();
        let endpoint_name = "get_transaction_details";

        let tx_details = self
            .state
            .connection_pool
            .access_storage_blocking()
            .transactions_web3_dal()
            .get_transaction_details(hash)
            .map_err(|err| internal_error(endpoint_name, err));

        metrics::histogram!("api.web3.call", start.elapsed(), "method" => endpoint_name);

        tx_details
    }

    #[tracing::instrument(skip(self))]
    pub fn get_l1_batch_details_impl(
        &self,
        batch_number: L1BatchNumber,
    ) -> Result<Option<L1BatchDetails>, Web3Error> {
        let start = Instant::now();
        let endpoint_name = "get_l1_batch";

        let l1_batch = self
            .state
            .connection_pool
            .access_storage_blocking()
            .explorer()
            .blocks_dal()
            .get_l1_batch_details(batch_number)
            .map_err(|err| internal_error(endpoint_name, err));

        metrics::histogram!("api.web3.call", start.elapsed(), "method" => endpoint_name);

        l1_batch
    }

    #[cfg(feature = "openzeppelin_tests")]
    /// Saves contract bytecode to memory.
    pub fn set_known_bytecode_impl(&self, bytecode: Bytes) -> bool {
        let mut lock = self.state.known_bytecodes.write().unwrap();
        lock.insert(bytecode.0.clone());

        true
    }
}

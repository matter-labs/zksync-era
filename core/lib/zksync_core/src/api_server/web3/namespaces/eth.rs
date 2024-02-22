use zksync_system_constants::DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE;
use zksync_types::{
    api::{
        BlockId, BlockNumber, GetLogsFilter, Transaction, TransactionId, TransactionReceipt,
        TransactionVariant,
    },
    l2::{L2Tx, TransactionType},
    transaction_request::CallRequest,
    utils::decompose_full_nonce,
    web3::{
        self,
        types::{FeeHistory, SyncInfo, SyncState},
    },
    AccountTreeId, Bytes, MiniblockNumber, StorageKey, H256, L2_ETH_TOKEN_ADDRESS, U256,
};
use zksync_utils::u256_to_h256;
use zksync_web3_decl::{
    error::Web3Error,
    types::{Address, Block, Filter, FilterChanges, Log, U64},
};

use crate::api_server::web3::{
    backend_jsonrpsee::internal_error,
    metrics::{BlockCallObserver, API_METRICS},
    state::RpcState,
    TypedFilter,
};

pub const EVENT_TOPIC_NUMBER_LIMIT: usize = 4;
pub const PROTOCOL_VERSION: &str = "zks/1";

#[derive(Debug)]
pub struct EthNamespace {
    state: RpcState,
}

impl EthNamespace {
    pub fn new(state: RpcState) -> Self {
        Self { state }
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_block_number_impl(&self) -> Result<U64, Web3Error> {
        const METHOD_NAME: &str = "get_block_number";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        let mut storage = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?;
        let block_number = storage
            .blocks_dal()
            .get_sealed_miniblock_number()
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?
            .ok_or(Web3Error::NoBlock)?;

        method_latency.observe();
        Ok(block_number.0.into())
    }

    #[tracing::instrument(skip(self, request, block_id))]
    pub async fn call_impl(
        &self,
        request: CallRequest,
        block_id: Option<BlockId>,
    ) -> Result<Bytes, Web3Error> {
        const METHOD_NAME: &str = "call";

        let block_id = block_id.unwrap_or(BlockId::Number(BlockNumber::Pending));
        let method_latency = API_METRICS.start_block_call(METHOD_NAME, block_id);
        let mut connection = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?;
        let block_args = self
            .state
            .resolve_block_args(&mut connection, block_id, METHOD_NAME)
            .await?;

        drop(connection);

        let tx = L2Tx::from_request(request.into(), self.state.api_config.max_tx_size)?;

        let call_result = self.state.tx_sender.eth_call(block_args, tx).await;
        let res_bytes = call_result.map_err(|err| err.into_web3_error(METHOD_NAME))?;

        let block_diff = self
            .state
            .last_sealed_miniblock
            .diff_with_block_args(&block_args);
        method_latency.observe(block_diff);
        Ok(res_bytes.into())
    }

    #[tracing::instrument(skip(self, request, _block))]
    pub async fn estimate_gas_impl(
        &self,
        request: CallRequest,
        _block: Option<BlockNumber>,
    ) -> Result<U256, Web3Error> {
        const METHOD_NAME: &str = "estimate_gas";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        let mut request_with_gas_per_pubdata_overridden = request;
        self.state
            .set_nonce_for_call_request(&mut request_with_gas_per_pubdata_overridden)
            .await?;

        if let Some(eip712_meta) = &mut request_with_gas_per_pubdata_overridden.eip712_meta {
            if eip712_meta.gas_per_pubdata == U256::zero() {
                eip712_meta.gas_per_pubdata = DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE.into();
            }
        }

        let is_eip712 = request_with_gas_per_pubdata_overridden
            .eip712_meta
            .is_some();

        let mut tx: L2Tx = L2Tx::from_request(
            request_with_gas_per_pubdata_overridden.into(),
            self.state.api_config.max_tx_size,
        )?;

        // The user may not include the proper transaction type during the estimation of
        // the gas fee. However, it is needed for the bootloader checks to pass properly.
        if is_eip712 {
            tx.common_data.transaction_type = TransactionType::EIP712Transaction;
        }

        // When we're estimating fee, we are trying to deduce values related to fee, so we should
        // not consider provided ones.
        let gas_price = self.state.tx_sender.gas_price().await;
        let gas_price = gas_price.map_err(|err| internal_error(METHOD_NAME, err))?;
        tx.common_data.fee.max_fee_per_gas = gas_price.into();
        tx.common_data.fee.max_priority_fee_per_gas = tx.common_data.fee.max_fee_per_gas;

        // Modify the l1 gas price with the scale factor
        let scale_factor = self.state.api_config.estimate_gas_scale_factor;
        let acceptable_overestimation =
            self.state.api_config.estimate_gas_acceptable_overestimation;

        let fee = self
            .state
            .tx_sender
            .get_txs_fee_in_wei(tx.into(), scale_factor, acceptable_overestimation)
            .await
            .map_err(|err| err.into_web3_error(METHOD_NAME))?;
        method_latency.observe();
        Ok(fee.gas_limit)
    }

    #[tracing::instrument(skip(self))]
    pub async fn gas_price_impl(&self) -> Result<U256, Web3Error> {
        const METHOD_NAME: &str = "gas_price";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        let gas_price = self.state.tx_sender.gas_price().await;
        let gas_price = gas_price.map_err(|err| internal_error(METHOD_NAME, err))?;
        method_latency.observe();
        Ok(gas_price.into())
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_balance_impl(
        &self,
        address: Address,
        block_id: Option<BlockId>,
    ) -> Result<U256, Web3Error> {
        const METHOD_NAME: &str = "get_balance";

        let block_id = block_id.unwrap_or(BlockId::Number(BlockNumber::Pending));
        let method_latency = API_METRICS.start_block_call(METHOD_NAME, block_id);
        let mut connection = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?;
        let block_number = self
            .state
            .resolve_block(&mut connection, block_id, METHOD_NAME)
            .await?;
        let balance = connection
            .storage_web3_dal()
            .standard_token_historical_balance(
                AccountTreeId::new(L2_ETH_TOKEN_ADDRESS),
                AccountTreeId::new(address),
                block_number,
            )
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?;
        self.report_latency_with_block_id(method_latency, block_number);

        Ok(balance)
    }

    fn report_latency_with_block_id(
        &self,
        observer: BlockCallObserver<'_>,
        block_number: MiniblockNumber,
    ) {
        let block_diff = self.state.last_sealed_miniblock.diff(block_number);
        observer.observe(block_diff);
    }

    #[tracing::instrument(skip(self, filter))]
    pub async fn get_logs_impl(&self, mut filter: Filter) -> Result<Vec<Log>, Web3Error> {
        const METHOD_NAME: &str = "get_logs";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        self.state.resolve_filter_block_hash(&mut filter).await?;
        let (from_block, to_block) = self.state.resolve_filter_block_range(&filter).await?;

        filter.to_block = Some(BlockNumber::Number(to_block.0.into()));
        let changes = self
            .filter_changes(&mut TypedFilter::Events(filter, from_block))
            .await?;
        method_latency.observe();
        Ok(match changes {
            FilterChanges::Logs(list) => list,
            _ => unreachable!("Unexpected `FilterChanges` type, expected `Logs`"),
        })
    }

    pub async fn get_filter_logs_impl(&self, idx: U256) -> Result<FilterChanges, Web3Error> {
        const METHOD_NAME: &str = "get_filter_logs";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        let installed_filters = self
            .state
            .installed_filters
            .as_ref()
            .ok_or(Web3Error::NotImplemented)?;
        // We clone the filter to not hold the filter lock for an extended period of time.
        let maybe_filter = installed_filters.lock().await.get_and_update_stats(idx);

        let Some(TypedFilter::Events(filter, _)) = maybe_filter else {
            return Err(Web3Error::FilterNotFound);
        };

        let from_block = self
            .state
            .resolve_filter_block_number(filter.from_block)
            .await?;
        let logs = self
            .filter_changes(&mut TypedFilter::Events(filter, from_block))
            .await?;

        // We are not updating the filter, since that is the purpose of `get_filter_changes` method,
        // which is getting changes happened from the last poll and moving the cursor forward.

        method_latency.observe();
        Ok(logs)
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_block_impl(
        &self,
        block_id: BlockId,
        full_transactions: bool,
    ) -> Result<Option<Block<TransactionVariant>>, Web3Error> {
        let method_name = if full_transactions {
            "get_block_with_txs"
        } else {
            "get_block"
        };
        let method_latency = API_METRICS.start_block_call(method_name, block_id);

        self.state.start_info.ensure_not_pruned(block_id)?;
        let block = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .map_err(|err| internal_error(method_name, err))?
            .blocks_web3_dal()
            .get_block_by_web3_block_id(
                block_id,
                full_transactions,
                self.state.api_config.l2_chain_id,
            )
            .await
            .map_err(|err| internal_error(method_name, err));

        if let Ok(Some(block)) = &block {
            let block_number = MiniblockNumber(block.number.as_u32());
            self.report_latency_with_block_id(method_latency, block_number);
        } else {
            method_latency.observe_without_diff();
        }
        block
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_block_transaction_count_impl(
        &self,
        block_id: BlockId,
    ) -> Result<Option<U256>, Web3Error> {
        const METHOD_NAME: &str = "get_block_transaction_count";

        let method_latency = API_METRICS.start_block_call(METHOD_NAME, block_id);

        self.state.start_info.ensure_not_pruned(block_id)?;
        let tx_count = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?
            .blocks_web3_dal()
            .get_block_tx_count(block_id)
            .await
            .map_err(|err| internal_error(METHOD_NAME, err));

        if let Ok(Some((block_number, _))) = &tx_count {
            self.report_latency_with_block_id(method_latency, *block_number);
        } else {
            method_latency.observe_without_diff();
        }
        Ok(tx_count?.map(|(_, count)| count))
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_block_receipts_impl(
        &self,
        block_id: BlockId,
    ) -> Result<Vec<TransactionReceipt>, Web3Error> {
        const METHOD_NAME: &str = "get_block_receipts";

        let method_latency = API_METRICS.start_block_call(METHOD_NAME, block_id);

        self.state.start_info.ensure_not_pruned(block_id)?;

        let block = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?
            .blocks_web3_dal()
            .get_block_by_web3_block_id(block_id, false, self.state.api_config.l2_chain_id)
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?;

        let transactions: &[TransactionVariant] =
            block.as_ref().map_or(&[], |block| &block.transactions);
        let hashes: Vec<_> = transactions
            .iter()
            .map(|tx| match tx {
                TransactionVariant::Full(tx) => tx.hash,
                TransactionVariant::Hash(hash) => *hash,
            })
            .collect();

        let mut receipts = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?
            .transactions_web3_dal()
            .get_transaction_receipts(&hashes)
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?;

        receipts.sort_unstable_by_key(|receipt| receipt.transaction_index);

        if let Some(block) = block {
            self.report_latency_with_block_id(method_latency, block.number.as_u32().into());
        } else {
            method_latency.observe_without_diff();
        }

        Ok(receipts)
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_code_impl(
        &self,
        address: Address,
        block_id: Option<BlockId>,
    ) -> Result<Bytes, Web3Error> {
        const METHOD_NAME: &str = "get_code";

        let block_id = block_id.unwrap_or(BlockId::Number(BlockNumber::Pending));
        let method_latency = API_METRICS.start_block_call(METHOD_NAME, block_id);
        let mut connection = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap();
        let block_number = self
            .state
            .resolve_block(&mut connection, block_id, METHOD_NAME)
            .await?;
        let contract_code = connection
            .storage_web3_dal()
            .get_contract_code_unchecked(address, block_number)
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?;

        self.report_latency_with_block_id(method_latency, block_number);
        Ok(contract_code.unwrap_or_default().into())
    }

    #[tracing::instrument(skip(self))]
    pub fn chain_id_impl(&self) -> U64 {
        self.state.api_config.l2_chain_id.as_u64().into()
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_storage_at_impl(
        &self,
        address: Address,
        idx: U256,
        block_id: Option<BlockId>,
    ) -> Result<H256, Web3Error> {
        const METHOD_NAME: &str = "get_storage_at";

        let block_id = block_id.unwrap_or(BlockId::Number(BlockNumber::Pending));
        let method_latency = API_METRICS.start_block_call(METHOD_NAME, block_id);
        let storage_key = StorageKey::new(AccountTreeId::new(address), u256_to_h256(idx));
        let mut connection = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap();
        let block_number = self
            .state
            .resolve_block(&mut connection, block_id, METHOD_NAME)
            .await?;
        let value = connection
            .storage_web3_dal()
            .get_historical_value_unchecked(&storage_key, block_number)
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?;

        self.report_latency_with_block_id(method_latency, block_number);
        Ok(value)
    }

    /// Account nonce.
    #[tracing::instrument(skip(self))]
    pub async fn get_transaction_count_impl(
        &self,
        address: Address,
        block_id: Option<BlockId>,
    ) -> Result<U256, Web3Error> {
        let block_id = block_id.unwrap_or(BlockId::Number(BlockNumber::Pending));
        let method_name = match block_id {
            BlockId::Number(BlockNumber::Pending) => "get_pending_transaction_count",
            _ => "get_historical_transaction_count",
        };
        let method_latency = API_METRICS.start_block_call(method_name, block_id);

        let mut connection = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap();

        let block_number = self
            .state
            .resolve_block(&mut connection, block_id, method_name)
            .await?;
        let full_nonce = connection
            .storage_web3_dal()
            .get_address_historical_nonce(address, block_number)
            .await
            .map_err(|err| internal_error(method_name, err))?;

        // TODO (SMA-1612): currently account nonce is returning always, but later we will
        //  return account nonce for account abstraction and deployment nonce for non account abstraction.
        //  Strip off deployer nonce part.
        let (mut account_nonce, _) = decompose_full_nonce(full_nonce);

        if matches!(block_id, BlockId::Number(BlockNumber::Pending)) {
            let account_nonce_u64 = u64::try_from(account_nonce)
                .map_err(|err| internal_error(method_name, anyhow::anyhow!(err)))?;
            account_nonce = if let Some(account_nonce) = self
                .state
                .tx_sender
                .0
                .tx_sink
                .lookup_pending_nonce(method_name, address, account_nonce_u64 as u32)
                .await?
            {
                account_nonce.0.into()
            } else {
                // No nonce hint in the sink: get pending nonces from the mempool
                connection
                    .transactions_web3_dal()
                    .next_nonce_by_initiator_account(address, account_nonce_u64)
                    .await
                    .map_err(|err| internal_error(method_name, err))?
            };
        }

        let block_diff = self.state.last_sealed_miniblock.diff(block_number);
        method_latency.observe(block_diff);
        Ok(account_nonce)
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_transaction_impl(
        &self,
        id: TransactionId,
    ) -> Result<Option<Transaction>, Web3Error> {
        const METHOD_NAME: &str = "get_transaction";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        let mut transaction = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap()
            .transactions_web3_dal()
            .get_transaction(id, self.state.api_config.l2_chain_id)
            .await
            .map_err(|err| internal_error(METHOD_NAME, err));

        if let Ok(None) = transaction {
            transaction = self
                .state
                .tx_sender
                .0
                .tx_sink
                .lookup_tx(METHOD_NAME, id)
                .await;
        }

        method_latency.observe();
        transaction
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_transaction_receipt_impl(
        &self,
        hash: H256,
    ) -> Result<Option<TransactionReceipt>, Web3Error> {
        const METHOD_NAME: &str = "get_transaction_receipt";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        let receipts = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap()
            .transactions_web3_dal()
            .get_transaction_receipts(&[hash])
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?;

        method_latency.observe();

        Ok(receipts.into_iter().next())
    }

    #[tracing::instrument(skip(self))]
    pub async fn new_block_filter_impl(&self) -> Result<U256, Web3Error> {
        const METHOD_NAME: &str = "new_block_filter";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        let installed_filters = self
            .state
            .installed_filters
            .as_ref()
            .ok_or(Web3Error::NotImplemented)?;
        let mut storage = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?;
        let last_block_number = storage
            .blocks_dal()
            .get_sealed_miniblock_number()
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?
            .ok_or_else(|| internal_error(METHOD_NAME, "no miniblocks in storage"))?;
        let next_block_number = last_block_number + 1;
        drop(storage);

        let idx = installed_filters
            .lock()
            .await
            .add(TypedFilter::Blocks(next_block_number));
        method_latency.observe();
        Ok(idx)
    }

    #[tracing::instrument(skip(self, filter))]
    pub async fn new_filter_impl(&self, mut filter: Filter) -> Result<U256, Web3Error> {
        const METHOD_NAME: &str = "new_filter";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        let installed_filters = self
            .state
            .installed_filters
            .as_ref()
            .ok_or(Web3Error::NotImplemented)?;
        if let Some(topics) = filter.topics.as_ref() {
            if topics.len() > EVENT_TOPIC_NUMBER_LIMIT {
                return Err(Web3Error::TooManyTopics);
            }
        }

        self.state.resolve_filter_block_hash(&mut filter).await?;
        let from_block = self.state.get_filter_from_block(&filter).await?;
        let idx = installed_filters
            .lock()
            .await
            .add(TypedFilter::Events(filter, from_block));
        method_latency.observe();
        Ok(idx)
    }

    #[tracing::instrument(skip(self))]
    pub async fn new_pending_transaction_filter_impl(&self) -> Result<U256, Web3Error> {
        const METHOD_NAME: &str = "new_pending_transaction_filter";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        let installed_filters = self
            .state
            .installed_filters
            .as_ref()
            .ok_or(Web3Error::NotImplemented)?;
        let idx = installed_filters
            .lock()
            .await
            .add(TypedFilter::PendingTransactions(
                chrono::Utc::now().naive_utc(),
            ));
        method_latency.observe();
        Ok(idx)
    }

    #[tracing::instrument(skip(self))]
    pub async fn get_filter_changes_impl(&self, idx: U256) -> Result<FilterChanges, Web3Error> {
        const METHOD_NAME: &str = "get_filter_changes";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        let installed_filters = self
            .state
            .installed_filters
            .as_ref()
            .ok_or(Web3Error::NotImplemented)?;
        let mut filter = installed_filters
            .lock()
            .await
            .get_and_update_stats(idx)
            .ok_or(Web3Error::FilterNotFound)?;

        let result = match self.filter_changes(&mut filter).await {
            Ok(changes) => {
                installed_filters.lock().await.update(idx, filter);
                Ok(changes)
            }
            Err(Web3Error::LogsLimitExceeded(..)) => {
                // The filter was not being polled for a long time, so we remove it.
                installed_filters.lock().await.remove(idx);
                Err(Web3Error::FilterNotFound)
            }
            Err(err) => Err(err),
        };
        method_latency.observe();
        result
    }

    #[tracing::instrument(skip(self))]
    pub async fn uninstall_filter_impl(&self, idx: U256) -> Result<bool, Web3Error> {
        const METHOD_NAME: &str = "uninstall_filter";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        let installed_filters = self
            .state
            .installed_filters
            .as_ref()
            .ok_or(Web3Error::NotImplemented)?;
        let removed = installed_filters.lock().await.remove(idx);
        method_latency.observe();
        Ok(removed)
    }

    #[tracing::instrument(skip(self))]
    pub fn protocol_version(&self) -> String {
        // TODO (SMA-838): Versioning of our protocol
        PROTOCOL_VERSION.to_string()
    }

    #[tracing::instrument(skip(self, tx_bytes))]
    pub async fn send_raw_transaction_impl(&self, tx_bytes: Bytes) -> Result<H256, Web3Error> {
        const METHOD_NAME: &str = "send_raw_transaction";

        let method_latency = API_METRICS.start_call(METHOD_NAME);
        let (mut tx, hash) = self.state.parse_transaction_bytes(&tx_bytes.0)?;
        tx.set_input(tx_bytes.0, hash);

        let submit_result = self.state.tx_sender.submit_tx(tx).await;
        let submit_result = submit_result.map(|_| hash).map_err(|err| {
            tracing::debug!("Send raw transaction error: {err}");
            API_METRICS.submit_tx_error[&err.prom_error_code()].inc();
            err.into_web3_error(METHOD_NAME)
        });

        method_latency.observe();
        submit_result
    }

    #[tracing::instrument(skip(self))]
    pub fn accounts_impl(&self) -> Vec<Address> {
        Vec::new()
    }

    #[tracing::instrument(skip(self))]
    pub fn syncing_impl(&self) -> SyncState {
        if let Some(state) = &self.state.sync_state {
            // Node supports syncing process (i.e. not the main node).
            if state.is_synced() {
                SyncState::NotSyncing
            } else {
                SyncState::Syncing(SyncInfo {
                    starting_block: 0u64.into(), // We always start syncing from genesis right now.
                    current_block: state.get_local_block().0.into(),
                    highest_block: state.get_main_node_block().0.into(),
                })
            }
        } else {
            // If there is no sync state, then the node is the main node and it's always synced.
            SyncState::NotSyncing
        }
    }

    #[tracing::instrument(skip(self))]
    pub async fn fee_history_impl(
        &self,
        block_count: U64,
        newest_block: BlockNumber,
        reward_percentiles: Vec<f32>,
    ) -> Result<FeeHistory, Web3Error> {
        const METHOD_NAME: &str = "fee_history";

        let method_latency =
            API_METRICS.start_block_call(METHOD_NAME, BlockId::Number(newest_block));
        // Limit `block_count`.
        let block_count = block_count
            .as_u64()
            .min(self.state.api_config.fee_history_limit)
            .max(1);

        let mut connection = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap();
        let newest_miniblock = self
            .state
            .resolve_block(&mut connection, BlockId::Number(newest_block), METHOD_NAME)
            .await?;

        let mut base_fee_per_gas = connection
            .blocks_web3_dal()
            .get_fee_history(newest_miniblock, block_count)
            .await
            .map_err(|err| internal_error(METHOD_NAME, err))?;
        // DAL method returns fees in DESC order while we need ASC.
        base_fee_per_gas.reverse();

        let oldest_block = newest_miniblock.0 + 1 - base_fee_per_gas.len() as u32;
        // We do not store gas used ratio for blocks, returns array of zeroes as a placeholder.
        let gas_used_ratio = vec![0.0; base_fee_per_gas.len()];
        // Effective priority gas price is currently 0.
        let reward = Some(vec![
            vec![U256::zero(); reward_percentiles.len()];
            base_fee_per_gas.len()
        ]);

        // `base_fee_per_gas` for next miniblock cannot be calculated, appending last fee as a placeholder.
        base_fee_per_gas.push(*base_fee_per_gas.last().unwrap());

        self.report_latency_with_block_id(method_latency, newest_miniblock);
        Ok(FeeHistory {
            oldest_block: web3::types::BlockNumber::Number(oldest_block.into()),
            base_fee_per_gas,
            gas_used_ratio,
            reward,
        })
    }

    #[tracing::instrument(skip(self, typed_filter))]
    async fn filter_changes(
        &self,
        typed_filter: &mut TypedFilter,
    ) -> Result<FilterChanges, Web3Error> {
        const METHOD_NAME: &str = "filter_changes";

        let res = match typed_filter {
            TypedFilter::Blocks(from_block) => {
                let mut conn = self
                    .state
                    .connection_pool
                    .access_storage_tagged("api")
                    .await
                    .map_err(|err| internal_error(METHOD_NAME, err))?;
                let (block_hashes, last_block_number) = conn
                    .blocks_web3_dal()
                    .get_block_hashes_since(*from_block, self.state.api_config.req_entities_limit)
                    .await
                    .map_err(|err| internal_error(METHOD_NAME, err))?;

                *from_block = match last_block_number {
                    Some(last_block_number) => last_block_number + 1,
                    None => *from_block,
                };

                FilterChanges::Hashes(block_hashes)
            }

            TypedFilter::PendingTransactions(from_timestamp_excluded) => {
                let mut conn = self
                    .state
                    .connection_pool
                    .access_storage_tagged("api")
                    .await
                    .map_err(|err| internal_error(METHOD_NAME, err))?;
                let (tx_hashes, last_timestamp) = conn
                    .transactions_web3_dal()
                    .get_pending_txs_hashes_after(
                        *from_timestamp_excluded,
                        Some(self.state.api_config.req_entities_limit),
                    )
                    .await
                    .map_err(|err| internal_error(METHOD_NAME, err))?;

                *from_timestamp_excluded = last_timestamp.unwrap_or(*from_timestamp_excluded);

                FilterChanges::Hashes(tx_hashes)
            }

            TypedFilter::Events(filter, from_block) => {
                let addresses = if let Some(addresses) = &filter.address {
                    addresses.0.clone()
                } else {
                    vec![]
                };
                let topics = if let Some(topics) = &filter.topics {
                    if topics.len() > EVENT_TOPIC_NUMBER_LIMIT {
                        return Err(Web3Error::TooManyTopics);
                    }
                    let topics_by_idx = topics.iter().enumerate().filter_map(|(idx, topics)| {
                        Some((idx as u32 + 1, topics.as_ref()?.0.clone()))
                    });
                    topics_by_idx.collect::<Vec<_>>()
                } else {
                    vec![]
                };

                let mut to_block = self
                    .state
                    .resolve_filter_block_number(filter.to_block)
                    .await?;

                if matches!(filter.to_block, Some(BlockNumber::Number(_))) {
                    to_block = to_block.min(
                        self.state
                            .resolve_filter_block_number(Some(BlockNumber::Latest))
                            .await?,
                    );
                }

                let get_logs_filter = GetLogsFilter {
                    from_block: *from_block,
                    to_block,
                    addresses,
                    topics,
                };

                let mut storage = self
                    .state
                    .connection_pool
                    .access_storage_tagged("api")
                    .await
                    .map_err(|err| internal_error(METHOD_NAME, err))?;

                // Check if there is more than one block in range and there are more than `req_entities_limit` logs that satisfies filter.
                // In this case we should return error and suggest requesting logs with smaller block range.
                if *from_block != to_block {
                    if let Some(miniblock_number) = storage
                        .events_web3_dal()
                        .get_log_block_number(
                            &get_logs_filter,
                            self.state.api_config.req_entities_limit,
                        )
                        .await
                        .map_err(|err| internal_error(METHOD_NAME, err))?
                    {
                        return Err(Web3Error::LogsLimitExceeded(
                            self.state.api_config.req_entities_limit,
                            from_block.0,
                            miniblock_number.0 - 1,
                        ));
                    }
                }

                let logs = storage
                    .events_web3_dal()
                    .get_logs(get_logs_filter, i32::MAX as usize)
                    .await
                    .map_err(|err| internal_error(METHOD_NAME, err))?;
                *from_block = to_block + 1;
                FilterChanges::Logs(logs)
            }
        };

        Ok(res)
    }
}

// Bogus methods.
// They are moved into a separate `impl` block so they don't make the actual implementation noisy.
// This `impl` block contains methods that we *have* to implement for compliance, but don't really
// make sense in terms of L2.
impl EthNamespace {
    pub fn coinbase_impl(&self) -> Address {
        // There is no coinbase account.
        Address::default()
    }

    pub fn compilers_impl(&self) -> Vec<String> {
        // This node doesn't support compilation.
        Vec::new()
    }

    pub fn uncle_count_impl(&self, _block: BlockId) -> Option<U256> {
        // We don't have uncles in zkSync.
        Some(0.into())
    }

    pub fn hashrate_impl(&self) -> U256 {
        // zkSync is not a PoW chain.
        U256::zero()
    }

    pub fn mining_impl(&self) -> bool {
        // zkSync is not a PoW chain.
        false
    }

    // List of methods that are not supported at all:
    //
    // - `sign`.
    // - `submit_hashrate`.
    // - `submit_work`.
    // - `compile_lll`.
    // - `compile_solidity`.
    // - `compile_serpent`.
}

use anyhow::Context as _;
use zksync_dal::{CoreDal, DalError};
use zksync_system_constants::DEFAULT_L2_TX_GAS_PER_PUBDATA_BYTE;
use zksync_types::{
    api::{
        state_override::StateOverride, BlockId, BlockNumber, FeeHistory, GetLogsFilter,
        Transaction, TransactionId, TransactionReceipt, TransactionVariant,
    },
    bytecode::{trim_padded_evm_bytecode, BytecodeHash, BytecodeMarker},
    l2::{L2Tx, TransactionType},
    transaction_request::CallRequest,
    u256_to_h256,
    web3::{self, Bytes, SyncInfo, SyncState},
    AccountTreeId, L2BlockNumber, StorageKey, H256, L2_BASE_TOKEN_ADDRESS, U256,
};
use zksync_web3_decl::{
    error::Web3Error,
    types::{Address, Block, Filter, FilterChanges, Log, U64},
};

// #[cfg(feature = "zkos")]
// use {
//     ruint::aliases::B160, zk_ee::common_structs::derive_flat_storage_key,
//     zk_os_basic_system::basic_io_implementer::address_into_special_storage_key,
//     zk_os_basic_system::basic_io_implementer::io_implementer::NOMINAL_TOKEN_BALANCE_STORAGE_ADDRESS,
// };
use crate::{
    execution_sandbox::BlockArgs,
    tx_sender::BinarySearchKind,
    utils::{fill_transaction_receipts, open_readonly_transaction},
    web3::{backend_jsonrpsee::MethodTracer, metrics::API_METRICS, state::RpcState, TypedFilter},
};

#[cfg(feature = "zkos")]
use crate::utils::{AccountType, ExternalAccountType};

pub const EVENT_TOPIC_NUMBER_LIMIT: usize = 4;
pub const PROTOCOL_VERSION: &str = "zks/1";

#[derive(Debug)]
pub(crate) struct EthNamespace {
    state: RpcState,
}

impl EthNamespace {
    pub fn new(state: RpcState) -> Self {
        Self { state }
    }

    pub(crate) fn current_method(&self) -> &MethodTracer {
        &self.state.current_method
    }

    pub async fn get_block_number_impl(&self) -> Result<U64, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        let block_number = storage
            .blocks_dal()
            .get_sealed_l2_block_number()
            .await
            .map_err(DalError::generalize)?
            .ok_or(Web3Error::NoBlock)?;
        Ok(block_number.0.into())
    }

    pub async fn call_impl(
        &self,
        mut request: CallRequest,
        block_id: Option<BlockId>,
        state_override: Option<StateOverride>,
    ) -> Result<Bytes, Web3Error> {
        let block_id = block_id.unwrap_or(BlockId::Number(BlockNumber::Pending));
        self.current_method().set_block_id(block_id);
        self.current_method()
            .observe_state_override(state_override.as_ref());

        let mut connection = self.state.acquire_connection().await?;
        let block_args = self
            .state
            .resolve_block_args(&mut connection, block_id)
            .await?;
        self.current_method().set_block_diff(
            self.state
                .last_sealed_l2_block
                .diff_with_block_args(&block_args),
        );
        if request.gas.is_none() {
            request.gas = Some(block_args.default_eth_call_gas(&mut connection).await?);
        }
        drop(connection);

        let call_overrides = request.get_call_overrides()?;
        let tx = L2Tx::from_request(
            request.clone().into(),
            self.state.api_config.max_tx_size,
            block_args.use_evm_emulator(),
        )?;

        // It is assumed that the previous checks has already enforced that the `max_fee_per_gas` is at most u64.
        let call_result: Vec<u8> = self
            .state
            .tx_sender
            .eth_call(block_args, call_overrides, tx, state_override)
            .await?;

        tracing::info!("result of eth_call ({:?}): {:?}", request, call_result);

        Ok(call_result.into())
    }

    pub async fn estimate_gas_impl(
        &self,
        request: CallRequest,
        _block: Option<BlockNumber>,
        state_override: Option<StateOverride>,
    ) -> Result<U256, Web3Error> {
        self.current_method()
            .observe_state_override(state_override.as_ref());

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
        let mut connection = self.state.acquire_connection().await?;
        let block_args = BlockArgs::pending(&mut connection).await?;
        drop(connection);
        let mut tx: L2Tx = L2Tx::from_request(
            request_with_gas_per_pubdata_overridden.into(),
            self.state.api_config.max_tx_size,
            block_args.use_evm_emulator(),
        )?;

        // The user may not include the proper transaction type during the estimation of
        // the gas fee. However, it is needed for the bootloader checks to pass properly.
        if is_eip712 {
            tx.common_data.transaction_type = TransactionType::EIP712Transaction;
        }

        // When we're estimating fee, we are trying to deduce values related to fee, so we should
        // not consider provided ones.
        let gas_price = self.state.tx_sender.gas_price().await?;
        tx.common_data.fee.max_fee_per_gas = gas_price.into();
        tx.common_data.fee.max_priority_fee_per_gas = tx.common_data.fee.max_fee_per_gas;

        // Modify the l1 gas price with the scale factor
        let scale_factor = self.state.api_config.estimate_gas_scale_factor;
        let acceptable_overestimation =
            self.state.api_config.estimate_gas_acceptable_overestimation;
        let search_kind = BinarySearchKind::new(self.state.api_config.estimate_gas_optimize_search);

        let fee = self
            .state
            .tx_sender
            .get_txs_fee_in_wei(
                tx.into(),
                block_args,
                scale_factor,
                acceptable_overestimation as u64,
                state_override,
                search_kind,
            )
            .await?;
        Ok(fee.gas_limit)
    }

    pub async fn gas_price_impl(&self) -> Result<U256, Web3Error> {
        let gas_price = self.state.tx_sender.gas_price().await?;
        Ok(gas_price.into())
    }

    pub async fn get_balance_impl(
        &self,
        address: Address,
        block_id: Option<BlockId>,
    ) -> Result<U256, Web3Error> {
        let block_id = block_id.unwrap_or(BlockId::Number(BlockNumber::Pending));
        self.current_method().set_block_id(block_id);

        let mut connection = self.state.acquire_connection().await?;
        let block_number = self.state.resolve_block(&mut connection, block_id).await?;

        #[cfg(feature = "zkos")]
        let balance = {
            connection
                .account_properies_dal()
                .get_balance(address, Some(block_number))
                .await
                .map_err(DalError::generalize)?
        };

        #[cfg(not(feature = "zkos"))]
        let balance = {
            connection
                .storage_web3_dal()
                .standard_token_historical_balance(
                    AccountTreeId::new(L2_BASE_TOKEN_ADDRESS),
                    AccountTreeId::new(address),
                    block_number,
                )
                .await
                .map_err(DalError::generalize)?
        };

        self.set_block_diff(block_number);

        Ok(balance)
    }

    fn set_block_diff(&self, block_number: L2BlockNumber) {
        let diff = self.state.last_sealed_l2_block.diff(block_number);
        self.current_method().set_block_diff(diff);
    }

    pub async fn get_logs_impl(&self, mut filter: Filter) -> Result<Vec<Log>, Web3Error> {
        self.state.resolve_filter_block_hash(&mut filter).await?;
        let (from_block, to_block) = self.state.resolve_filter_block_range(&filter).await?;

        filter.to_block = Some(BlockNumber::Number(to_block.0.into()));
        let changes = self
            .filter_changes(&mut TypedFilter::Events(filter, from_block))
            .await?;
        Ok(match changes {
            FilterChanges::Logs(list) => list,
            _ => unreachable!("Unexpected `FilterChanges` type, expected `Logs`"),
        })
    }

    pub async fn get_filter_logs_impl(&self, idx: U256) -> Result<FilterChanges, Web3Error> {
        let installed_filters = self
            .state
            .installed_filters
            .as_ref()
            .ok_or(Web3Error::MethodNotImplemented)?;
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
        Ok(logs)
    }

    pub async fn get_block_impl(
        &self,
        block_id: BlockId,
        full_transactions: bool,
    ) -> Result<Option<Block<TransactionVariant>>, Web3Error> {
        self.current_method().set_block_id(block_id);
        if matches!(block_id, BlockId::Number(BlockNumber::Pending)) {
            // Shortcut here on a somewhat unlikely case of the client requesting a pending block.
            // Otherwise, since we don't read DB data in a transaction,
            // we might resolve a block number to a block that will be inserted to the DB immediately after,
            // and return `Ok(Some(_))`.
            return Ok(None);
        }

        let mut storage = self.state.acquire_connection().await?;
        self.state
            .start_info
            .ensure_not_pruned(block_id, &mut storage)
            .await?;

        let Some(block_number) = self
            .state
            .resolve_block_unchecked(&mut storage, block_id)
            .await?
        else {
            return Ok(None);
        };
        let Some(block) = storage
            .blocks_web3_dal()
            .get_api_block(block_number)
            .await
            .map_err(DalError::generalize)?
        else {
            return Ok(None);
        };
        self.set_block_diff(block_number);

        let transactions = if full_transactions {
            let mut transactions = storage
                .transactions_web3_dal()
                .get_transactions(&block.transactions, self.state.api_config.l2_chain_id)
                .await
                .map_err(DalError::generalize)?;
            if transactions.len() != block.transactions.len() {
                let err = anyhow::anyhow!(
                    "storage inconsistency: get_api_block({block_number}) returned {} tx hashes, but get_transactions({:?}) \
                     returned {} transactions: {transactions:?}",
                    block.transactions.len(),
                    block.transactions,
                    transactions.len()
                );
                return Err(err.into());
            }
            // We need to sort `transactions` by their index in block since `get_transactions()` returns
            // transactions in an arbitrary order.
            transactions.sort_unstable_by_key(|tx| tx.transaction_index);

            transactions
                .into_iter()
                .map(TransactionVariant::Full)
                .collect()
        } else {
            block
                .transactions
                .iter()
                .copied()
                .map(TransactionVariant::Hash)
                .collect()
        };

        Ok(Some(block.with_transactions(transactions)))
    }

    pub async fn get_block_transaction_count_impl(
        &self,
        block_id: BlockId,
    ) -> Result<Option<U256>, Web3Error> {
        self.current_method().set_block_id(block_id);
        if matches!(block_id, BlockId::Number(BlockNumber::Pending)) {
            // See `get_block_impl()` for an explanation why this check is needed.
            return Ok(None);
        }

        let mut storage = self.state.acquire_connection().await?;
        self.state
            .start_info
            .ensure_not_pruned(block_id, &mut storage)
            .await?;

        let Some(block_number) = self
            .state
            .resolve_block_unchecked(&mut storage, block_id)
            .await?
        else {
            return Ok(None);
        };
        let tx_count = storage
            .blocks_web3_dal()
            .get_block_tx_count(block_number)
            .await
            .map_err(DalError::generalize)?;

        if tx_count.is_some() {
            self.set_block_diff(block_number); // only report block diff for existing L2 blocks
        }
        Ok(tx_count.map(Into::into))
    }

    pub async fn get_block_receipts_impl(
        &self,
        block_id: BlockId,
    ) -> Result<Option<Vec<TransactionReceipt>>, Web3Error> {
        self.current_method().set_block_id(block_id);
        if matches!(block_id, BlockId::Number(BlockNumber::Pending)) {
            // See `get_block_impl()` for an explanation why this check is needed.
            return Ok(None);
        }

        let mut storage = self.state.acquire_connection().await?;
        self.state
            .start_info
            .ensure_not_pruned(block_id, &mut storage)
            .await?;

        let Some(block_number) = self
            .state
            .resolve_block_unchecked(&mut storage, block_id)
            .await?
        else {
            return Ok(None);
        };
        let Some(block) = storage
            .blocks_web3_dal()
            .get_api_block(block_number)
            .await
            .map_err(DalError::generalize)?
        else {
            return Ok(None);
        };
        self.set_block_diff(block_number); // only report block diff for existing L2 blocks

        let receipts = storage
            .transactions_web3_dal()
            .get_transaction_receipts(&block.transactions)
            .await
            .with_context(|| format!("get_transaction_receipts({block_number})"))?;
        let receipts = fill_transaction_receipts(&mut storage, receipts).await?;
        Ok(Some(receipts))
    }

    pub async fn get_code_impl(
        &self,
        address: Address,
        block_id: Option<BlockId>,
    ) -> Result<Bytes, Web3Error> {
        let block_id = block_id.unwrap_or(BlockId::Number(BlockNumber::Pending));
        self.current_method().set_block_id(block_id);

        let mut connection = self.state.acquire_connection().await?;
        let block_number = self.state.resolve_block(&mut connection, block_id).await?;
        self.set_block_diff(block_number);

        #[cfg(not(feature = "zkos"))]
        let prepared_bytecode = {
            let contract_code = connection
                .storage_web3_dal()
                .get_contract_code_unchecked(address, block_number)
                .await
                .map_err(DalError::generalize)?;
            let Some(contract_code) = contract_code else {
                return Ok(Bytes::default());
            };
            // Check if the bytecode is an EVM bytecode, and if so, pre-process it correspondingly.
            let marker = BytecodeMarker::new(contract_code.bytecode_hash);
            if marker == Some(BytecodeMarker::Evm) {
                trim_padded_evm_bytecode(
                    BytecodeHash::try_from(contract_code.bytecode_hash).with_context(|| {
                        format!(
                            "Invalid bytecode hash at address {address:?}: {:?}",
                            contract_code.bytecode_hash
                        )
                    })?,
                    &contract_code.bytecode,
                )
                .with_context(|| {
                    format!(
                        "malformed EVM bytecode at address {address:?}, hash = {:?}",
                        contract_code.bytecode_hash
                    )
                })?
                .to_vec()
            } else {
                contract_code.bytecode
            }
        };

        #[cfg(feature = "zkos")]
        let prepared_bytecode = {
            let code = connection
                .account_properies_dal()
                .get_code(address, Some(block_number))
                .await
                .map_err(DalError::generalize)?;

            Bytes(code.unwrap_or_default())
        };

        Ok(prepared_bytecode.into())
    }

    pub fn chain_id_impl(&self) -> U64 {
        self.state.api_config.l2_chain_id.as_u64().into()
    }

    pub async fn get_storage_at_impl(
        &self,
        address: Address,
        idx: U256,
        block_id: Option<BlockId>,
    ) -> Result<H256, Web3Error> {
        let block_id = block_id.unwrap_or(BlockId::Number(BlockNumber::Pending));
        self.current_method().set_block_id(block_id);

        let storage_key = StorageKey::new(AccountTreeId::new(address), u256_to_h256(idx));
        let mut connection = self.state.acquire_connection().await?;
        let block_number = self.state.resolve_block(&mut connection, block_id).await?;
        self.set_block_diff(block_number);
        let value = connection
            .storage_web3_dal()
            .get_historical_value_unchecked(storage_key.hashed_key(), block_number)
            .await
            .map_err(DalError::generalize)?;
        Ok(value)
    }

    /// Account nonce.
    pub async fn get_transaction_count_impl(
        &self,
        address: Address,
        block_id: Option<BlockId>,
    ) -> Result<U256, Web3Error> {
        let block_id = block_id.unwrap_or(BlockId::Number(BlockNumber::Pending));
        self.current_method().set_block_id(block_id);

        let mut connection = self.state.acquire_connection().await?;

        let block_number = self.state.resolve_block(&mut connection, block_id).await?;
        self.set_block_diff(block_number);

        #[cfg(not(feature = "zkos"))]
        let (account_type, mut nonce) = {
            self.state
                .account_types_cache
                .get_with_nonce(&mut connection, address, block_number)
                .await?
        };

        #[cfg(feature = "zkos")]
        let (account_type, mut nonce) = {
            let nonce = connection
                .account_properies_dal()
                .get_nonce(address, Some(block_number))
                .await
                .map_err(DalError::generalize)?;
            // TODO: account type
            (
                AccountType::External(ExternalAccountType::Default),
                U256::from(nonce),
            )
        };

        if account_type.is_external() && matches!(block_id, BlockId::Number(BlockNumber::Pending)) {
            let account_nonce_u64 = u64::try_from(nonce)
                .map_err(|err| anyhow::anyhow!("nonce conversion failed: {err}"))?;
            nonce = if let Some(account_nonce) = self
                .state
                .tx_sink()
                .lookup_pending_nonce(address, account_nonce_u64 as u32)
                .await?
            {
                account_nonce.0.into()
            } else {
                // No nonce hint in the sink: get pending nonces from the mempool
                connection
                    .transactions_web3_dal()
                    .next_nonce_by_initiator_account(address, account_nonce_u64)
                    .await
                    .map_err(DalError::generalize)?
            };
        }

        Ok(nonce)
    }

    pub async fn get_transaction_impl(
        &self,
        id: TransactionId,
    ) -> Result<Option<Transaction>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        // Open a readonly transaction to have a consistent view of Postgres
        let mut storage = open_readonly_transaction(&mut storage).await?;

        let chain_id = self.state.api_config.l2_chain_id;
        let mut transaction = match id {
            TransactionId::Hash(hash) => storage
                .transactions_web3_dal()
                .get_transaction_by_hash(hash, chain_id)
                .await
                .map_err(DalError::generalize)?,

            TransactionId::Block(block_id, idx) => {
                if matches!(block_id, BlockId::Number(BlockNumber::Pending)) {
                    // See `get_block_impl()` for an explanation why this check is needed.
                    return Ok(None);
                }

                let Ok(idx) = u32::try_from(idx) else {
                    return Ok(None); // index overflow means no transaction
                };
                let Some(block_number) = self
                    .state
                    .resolve_block_unchecked(&mut storage, block_id)
                    .await?
                else {
                    return Ok(None);
                };

                storage
                    .transactions_web3_dal()
                    .get_transaction_by_position(block_number, idx, chain_id)
                    .await
                    .map_err(DalError::generalize)?
            }
        };

        if transaction.is_none() {
            transaction = self.state.tx_sink().lookup_tx(&mut storage, id).await?;
        }
        Ok(transaction)
    }

    pub async fn get_transaction_receipt_impl(
        &self,
        hash: H256,
    ) -> Result<Option<TransactionReceipt>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        let receipts = storage
            .transactions_web3_dal()
            .get_transaction_receipts(&[hash])
            .await
            .context("get_transaction_receipts")?;
        let receipts = fill_transaction_receipts(&mut storage, receipts).await?;
        Ok(receipts.into_iter().next())
    }

    pub async fn new_block_filter_impl(&self) -> Result<U256, Web3Error> {
        let installed_filters = self
            .state
            .installed_filters
            .as_ref()
            .ok_or(Web3Error::MethodNotImplemented)?;
        let mut storage = self.state.acquire_connection().await?;
        let last_block_number = storage
            .blocks_dal()
            .get_sealed_l2_block_number()
            .await
            .map_err(DalError::generalize)?
            .context("no L2 blocks in storage")?;
        let next_block_number = last_block_number + 1;
        drop(storage);

        Ok(installed_filters
            .lock()
            .await
            .add(TypedFilter::Blocks(next_block_number)))
    }

    pub async fn new_filter_impl(&self, mut filter: Filter) -> Result<U256, Web3Error> {
        let installed_filters = self
            .state
            .installed_filters
            .as_ref()
            .ok_or(Web3Error::MethodNotImplemented)?;
        if let Some(topics) = filter.topics.as_ref() {
            if topics.len() > EVENT_TOPIC_NUMBER_LIMIT {
                return Err(Web3Error::TooManyTopics);
            }
        }

        self.state.resolve_filter_block_hash(&mut filter).await?;
        let from_block = self.state.get_filter_from_block(&filter).await?;
        Ok(installed_filters
            .lock()
            .await
            .add(TypedFilter::Events(filter, from_block)))
    }

    pub async fn new_pending_transaction_filter_impl(&self) -> Result<U256, Web3Error> {
        let installed_filters = self
            .state
            .installed_filters
            .as_ref()
            .ok_or(Web3Error::MethodNotImplemented)?;
        Ok(installed_filters
            .lock()
            .await
            .add(TypedFilter::PendingTransactions(
                chrono::Utc::now().naive_utc(),
            )))
    }

    pub async fn get_filter_changes_impl(&self, idx: U256) -> Result<FilterChanges, Web3Error> {
        let installed_filters = self
            .state
            .installed_filters
            .as_ref()
            .ok_or(Web3Error::MethodNotImplemented)?;
        let mut filter = installed_filters
            .lock()
            .await
            .get_and_update_stats(idx)
            .ok_or(Web3Error::FilterNotFound)?;

        match self.filter_changes(&mut filter).await {
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
        }
    }

    pub async fn uninstall_filter_impl(&self, idx: U256) -> Result<bool, Web3Error> {
        let installed_filters = self
            .state
            .installed_filters
            .as_ref()
            .ok_or(Web3Error::MethodNotImplemented)?;
        Ok(installed_filters.lock().await.remove(idx))
    }

    pub fn protocol_version(&self) -> String {
        // TODO (SMA-838): Versioning of our protocol
        PROTOCOL_VERSION.to_string()
    }

    pub async fn send_raw_transaction_impl(&self, tx_bytes: Bytes) -> Result<H256, Web3Error> {
        let mut connection = self.state.acquire_connection().await?;
        let block_args = BlockArgs::pending(&mut connection).await?;
        drop(connection);
        let (mut tx, hash) = self
            .state
            .parse_transaction_bytes(&tx_bytes.0, &block_args)?;
        tx.set_input(tx_bytes.0, hash);

        let submit_result = self.state.tx_sender.submit_tx(tx, block_args).await;
        submit_result.map(|_| hash).map_err(|err| {
            tracing::debug!("Send raw transaction error: {err}");
            API_METRICS.submit_tx_error[&err.prom_error_code()].inc();
            err.into()
        })
    }

    pub fn accounts_impl(&self) -> Vec<Address> {
        Vec::new()
    }

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

    pub async fn fee_history_impl(
        &self,
        block_count: u64,
        newest_block: BlockNumber,
        reward_percentiles: Vec<f32>,
    ) -> Result<FeeHistory, Web3Error> {
        self.current_method()
            .set_block_id(BlockId::Number(newest_block));

        // Limit `block_count`.
        let block_count = block_count.clamp(1, self.state.api_config.fee_history_limit);

        let mut connection = self.state.acquire_connection().await?;
        let newest_l2_block = self
            .state
            .resolve_block(&mut connection, BlockId::Number(newest_block))
            .await?;
        self.set_block_diff(newest_l2_block);

        let (mut base_fee_per_gas, mut effective_pubdata_price_history) = connection
            .blocks_web3_dal()
            .get_fee_history(newest_l2_block, block_count)
            .await
            .map_err(DalError::generalize)?;

        // DAL method returns fees in DESC order while we need ASC.
        base_fee_per_gas.reverse();
        effective_pubdata_price_history.reverse();

        let oldest_block = newest_l2_block.0 + 1 - base_fee_per_gas.len() as u32;
        // We do not store gas used ratio for blocks, returns array of zeroes as a placeholder.
        let gas_used_ratio = vec![0.0; base_fee_per_gas.len()];
        // Effective priority gas price is currently 0.
        let reward = Some(vec![
            vec![U256::zero(); reward_percentiles.len()];
            base_fee_per_gas.len()
        ]);

        // `base_fee_per_gas` for next L2 block cannot be calculated, appending last fee as a placeholder.
        base_fee_per_gas.push(*base_fee_per_gas.last().unwrap());

        // We do not support EIP-4844, but per API specification we should return 0 for pre EIP-4844 blocks.
        let base_fee_per_blob_gas = vec![U256::zero(); base_fee_per_gas.len()];
        let blob_gas_used_ratio = vec![0.0; gas_used_ratio.len()];

        Ok(FeeHistory {
            inner: web3::FeeHistory {
                oldest_block: web3::BlockNumber::Number(oldest_block.into()),
                base_fee_per_gas,
                gas_used_ratio,
                reward,
                base_fee_per_blob_gas,
                blob_gas_used_ratio,
            },
            l2_pubdata_price: effective_pubdata_price_history,
        })
    }

    async fn filter_changes(
        &self,
        typed_filter: &mut TypedFilter,
    ) -> Result<FilterChanges, Web3Error> {
        Ok(match typed_filter {
            TypedFilter::Blocks(from_block) => {
                let mut conn = self.state.acquire_connection().await?;
                let (block_hashes, last_block_number) = conn
                    .blocks_web3_dal()
                    .get_block_hashes_since(*from_block, self.state.api_config.req_entities_limit)
                    .await
                    .map_err(DalError::generalize)?;

                *from_block = match last_block_number {
                    Some(last_block_number) => last_block_number + 1,
                    None => *from_block,
                };

                FilterChanges::Hashes(block_hashes)
            }

            TypedFilter::PendingTransactions(from_timestamp_excluded) => {
                // Attempt to get pending transactions from cache.

                let tx_hashes_from_cache = if let Some(cache) = &self.state.mempool_cache {
                    cache.get_tx_hashes_after(*from_timestamp_excluded).await
                } else {
                    None
                };
                let tx_hashes = if let Some(mut result) = tx_hashes_from_cache {
                    result.truncate(self.state.api_config.req_entities_limit);
                    result
                } else {
                    // On cache miss, query the database.
                    let mut conn = self.state.acquire_connection().await?;
                    conn.transactions_web3_dal()
                        .get_pending_txs_hashes_after(
                            *from_timestamp_excluded,
                            Some(self.state.api_config.req_entities_limit),
                        )
                        .await
                        .map_err(DalError::generalize)?
                };

                // It's possible the `tx_hashes` vector is empty,
                // meaning there are no transactions in cache that are newer than `from_timestamp_excluded`.
                // In this case we should return empty result and don't update `from_timestamp_excluded`.
                if let Some((last_timestamp, _)) = tx_hashes.last() {
                    *from_timestamp_excluded = *last_timestamp;
                }

                FilterChanges::Hashes(tx_hashes.into_iter().map(|(_, hash)| hash).collect())
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

                let mut storage = self.state.acquire_connection().await?;

                // Check if there is more than one block in range and there are more than `req_entities_limit` logs that satisfies filter.
                // In this case we should return error and suggest requesting logs with smaller block range.
                if *from_block != to_block {
                    if let Some(l2_block_number) = storage
                        .events_web3_dal()
                        .get_log_block_number(
                            &get_logs_filter,
                            self.state.api_config.req_entities_limit,
                        )
                        .await
                        .map_err(DalError::generalize)?
                    {
                        return Err(Web3Error::LogsLimitExceeded(
                            self.state.api_config.req_entities_limit,
                            from_block.0,
                            from_block.0.max(l2_block_number.0 - 1),
                        ));
                    }
                }

                let logs = storage
                    .events_web3_dal()
                    .get_logs(get_logs_filter, i32::MAX as usize)
                    .await
                    .map_err(DalError::generalize)?;
                *from_block = to_block + 1;
                FilterChanges::Logs(logs)
            }
        })
    }

    pub fn max_priority_fee_per_gas_impl(&self) -> U256 {
        // ZKsync does not require priority fee.
        0u64.into()
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
        // We don't have uncles in ZKsync.
        Some(0.into())
    }

    pub fn hashrate_impl(&self) -> U256 {
        // ZKsync is not a PoW chain.
        U256::zero()
    }

    pub fn mining_impl(&self) -> bool {
        // ZKsync is not a PoW chain.
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

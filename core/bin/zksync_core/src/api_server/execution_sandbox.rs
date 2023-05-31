use std::collections::{HashMap, HashSet};
use std::time::Instant;

use thiserror::Error;
use tracing::{span, Level};

use vm::oracles::tracer::{ValidationError, ValidationTracerParams};
use vm::vm_with_bootloader::{
    derive_base_fee_and_gas_per_pubdata, init_vm, push_transaction_to_bootloader_memory,
    BlockContext, BlockContextMode, BootloaderJobType, DerivedBlockContext, TxExecutionMode,
};
use vm::zk_evm::block_properties::BlockProperties;
use vm::{
    storage::Storage, utils::ETH_CALL_GAS_LIMIT, TxRevertReason, VmExecutionResult, VmInstance,
};
use vm::{HistoryDisabled, HistoryMode};
use zksync_config::constants::ZKPORTER_IS_AVAILABLE;
use zksync_contracts::BaseSystemContracts;
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_db_storage_provider::DbStorageProvider;
use zksync_state::storage_view::StorageView;
use zksync_types::api::BlockId;
use zksync_types::utils::storage_key_for_eth_balance;
use zksync_types::{
    api,
    event::{extract_long_l2_to_l1_messages, extract_published_bytecodes},
    fee::TransactionExecutionMetrics,
    get_nonce_key,
    l2::L2Tx,
    storage_writes_deduplicator::StorageWritesDeduplicator,
    utils::{decompose_full_nonce, nonces_to_full_nonce},
    AccountTreeId, MiniblockNumber, Nonce, StorageKey, Transaction, H256, U256,
};
use zksync_types::{PUBLISH_BYTECODE_OVERHEAD, TRUSTED_ADDRESS_SLOTS, TRUSTED_TOKEN_SLOTS};
use zksync_utils::bytecode::{bytecode_len_in_bytes, hash_bytecode, CompressedBytecodeInfo};
use zksync_utils::time::millis_since_epoch;
use zksync_utils::{h256_to_u256, u256_to_h256};
use zksync_web3_decl::error::Web3Error;

use crate::api_server::web3::backend_jsonrpc::error::internal_error;

use super::tx_sender::SubmitTxError;

#[derive(Debug, Error)]
pub enum SandboxExecutionError {
    #[error("Account validation failed: {0}")]
    AccountValidationFailed(String),
    #[error("Failed to charge fee: {0}")]
    FailedToChargeFee(String),
    #[error("Paymaster validation failed: {0}")]
    PaymasterValidationFailed(String),
    #[error("Pre-paymaster preparation failed: {0}")]
    PrePaymasterPreparationFailed(String),
    #[error("From is not an account")]
    FromIsNotAnAccount,
    #[error("Bootloader failure: {0}")]
    BootloaderFailure(String),
    #[error("Revert: {0}")]
    Revert(String, Vec<u8>),
    #[error("Failed to pay for the transaction: {0}")]
    FailedToPayForTransaction(String),
    #[error("Bootloader-based tx failed")]
    InnerTxError,
    #[error(
    "Virtual machine entered unexpected state. Please contact developers and provide transaction details \
        that caused this error. Error description: {0}"
    )]
    UnexpectedVMBehavior(String),
    #[error("Transaction is unexecutable. Reason: {0}")]
    Unexecutable(String),
}

#[allow(clippy::too_many_arguments)]
pub fn execute_tx_eth_call(
    connection_pool: &ConnectionPool,
    mut tx: L2Tx,
    block_id: api::BlockId,
    l1_gas_price: u64,
    fair_l2_gas_price: u64,
    enforced_base_fee: Option<u64>,
    base_system_contract: &BaseSystemContracts,
    vm_execution_cache_misses_limit: Option<usize>,
    trace_call: bool,
) -> Result<VmExecutionResult, Web3Error> {
    let mut storage = connection_pool.access_storage_blocking();
    let resolved_block_number = storage
        .blocks_web3_dal()
        .resolve_block_id(block_id)
        .map_err(|err| internal_error("eth_call", err))??;
    let block_timestamp_s = storage
        .blocks_web3_dal()
        .get_block_timestamp(resolved_block_number)
        .unwrap();

    // Protection against infinite-loop eth_calls and alike:
    // limiting the amount of gas the call can use.
    // We can't use BLOCK_ERGS_LIMIT here since the VM itself has some overhead.
    tx.common_data.fee.gas_limit = ETH_CALL_GAS_LIMIT.into();
    let vm_result = execute_tx_in_sandbox(
        storage,
        tx.into(),
        TxExecutionMode::EthCall {
            missed_storage_invocation_limit: vm_execution_cache_misses_limit.unwrap_or(usize::MAX),
        },
        AccountTreeId::default(),
        block_id,
        resolved_block_number,
        block_timestamp_s,
        None,
        U256::zero(),
        BootloaderJobType::TransactionExecution,
        l1_gas_price,
        fair_l2_gas_price,
        enforced_base_fee,
        base_system_contract,
        trace_call,
        &mut Default::default(),
    )
    .1
    .map_err(|err| {
        let submit_tx_error: SubmitTxError = err.into();
        Web3Error::SubmitTransactionError(submit_tx_error.to_string(), submit_tx_error.data())
    })?;
    Ok(vm_result)
}

fn get_pending_state(
    connection_pool: &ConnectionPool,
) -> (BlockId, StorageProcessor<'_>, MiniblockNumber) {
    let block_id = api::BlockId::Number(api::BlockNumber::Pending);
    let mut connection = connection_pool.access_storage_blocking();
    let resolved_block_number = connection
        .blocks_web3_dal()
        .resolve_block_id(block_id)
        .unwrap()
        .expect("Pending block should be present");

    (block_id, connection, resolved_block_number)
}

#[tracing::instrument(skip(
    connection_pool,
    tx,
    operator_account,
    enforced_nonce,
    base_system_contracts,
    storage_read_cache
))]
#[allow(clippy::too_many_arguments)]
pub fn execute_tx_with_pending_state(
    connection_pool: &ConnectionPool,
    tx: Transaction,
    operator_account: AccountTreeId,
    execution_mode: TxExecutionMode,
    enforced_nonce: Option<Nonce>,
    added_balance: U256,
    l1_gas_price: u64,
    fair_l2_gas_price: u64,
    enforced_base_fee: Option<u64>,
    base_system_contracts: &BaseSystemContracts,
    storage_read_cache: &mut HashMap<StorageKey, H256>,
) -> (
    TransactionExecutionMetrics,
    Result<VmExecutionResult, SandboxExecutionError>,
) {
    let (block_id, connection, resolved_block_number) = get_pending_state(connection_pool);

    // In order for execution to pass smoothlessly, we need to ensure that block's required gasPerPubdata will be
    // <= to the one in the transaction itself.
    let l1_gas_price = adjust_l1_gas_price_for_tx(
        l1_gas_price,
        fair_l2_gas_price,
        tx.gas_per_pubdata_byte_limit(),
    );

    execute_tx_in_sandbox(
        connection,
        tx,
        execution_mode,
        operator_account,
        block_id,
        resolved_block_number,
        None,
        enforced_nonce,
        added_balance,
        BootloaderJobType::TransactionExecution,
        l1_gas_price,
        fair_l2_gas_price,
        enforced_base_fee,
        base_system_contracts,
        false,
        storage_read_cache,
    )
}

// Returns the number of the pubdata that the transaction will spend on factory deps
pub fn get_pubdata_for_factory_deps(
    connection_pool: &ConnectionPool,
    factory_deps: &Option<Vec<Vec<u8>>>,
) -> u32 {
    let (_, connection, block_number) = get_pending_state(connection_pool);
    let db_storage_provider = DbStorageProvider::new(connection, block_number, false);
    let mut storage_view = StorageView::new(db_storage_provider);

    factory_deps
        .as_ref()
        .map(|deps| {
            deps.iter()
                .filter_map(|bytecode| {
                    if storage_view.is_bytecode_known(&hash_bytecode(bytecode)) {
                        return None;
                    }

                    let length = if let Ok(compressed) =
                        CompressedBytecodeInfo::from_original(bytecode.clone())
                    {
                        compressed.compressed.len()
                    } else {
                        bytecode.len()
                    };

                    Some(length as u32 + PUBLISH_BYTECODE_OVERHEAD)
                })
                .sum()
        })
        .unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
pub fn validate_tx_with_pending_state(
    connection_pool: &ConnectionPool,
    tx: L2Tx,
    operator_account: AccountTreeId,
    execution_mode: TxExecutionMode,
    enforced_nonce: Option<Nonce>,
    added_balance: U256,
    l1_gas_price: u64,
    fair_l2_gas_price: u64,
    enforced_base_fee: Option<u64>,
    base_system_contracts: &BaseSystemContracts,
    computational_gas_limit: u32,
) -> Result<(), ValidationError> {
    let (block_id, connection, resolved_block_number) = get_pending_state(connection_pool);

    // In order for validation to pass smoothlessly, we need to ensure that block's required gasPerPubdata will be
    // <= to the one in the transaction itself.
    let l1_gas_price = adjust_l1_gas_price_for_tx(
        l1_gas_price,
        fair_l2_gas_price,
        tx.common_data.fee.gas_per_pubdata_limit,
    );

    validate_tx_in_sandbox(
        connection,
        tx,
        execution_mode,
        operator_account,
        base_system_contracts,
        block_id,
        resolved_block_number,
        None,
        enforced_nonce,
        added_balance,
        l1_gas_price,
        fair_l2_gas_price,
        enforced_base_fee,
        computational_gas_limit,
    )
}

pub(crate) fn adjust_l1_gas_price_for_tx(
    l1_gas_price: u64,
    fair_l2_gas_price: u64,
    tx_gas_per_pubdata_limit: U256,
) -> u64 {
    let current_pubdata_price =
        derive_base_fee_and_gas_per_pubdata(l1_gas_price, fair_l2_gas_price).1;
    if U256::from(current_pubdata_price) <= tx_gas_per_pubdata_limit {
        // The current pubdata price is small enough
        l1_gas_price
    } else {
        // gasPerPubdata = ceil(17 * l1gasprice / fair_l2_gas_price)
        // gasPerPubdata <= 17 * l1gasprice / fair_l2_gas_price + 1
        // fair_l2_gas_price(gasPerPubdata - 1) / 17 <= l1gasprice
        let l1_gas_price = U256::from(fair_l2_gas_price)
            * (tx_gas_per_pubdata_limit - U256::from(1u32))
            / U256::from(17);

        l1_gas_price.as_u64()
    }
}

/// This method assumes that (block with number `resolved_block_number` is present in DB)
/// or (`block_id` is `pending` and block with number `resolved_block_number - 1` is present in DB)
#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip(
    connection,
    tx,
    operator_account,
    block_timestamp_s,
    base_system_contract,
    storage_read_cache
))]
fn execute_tx_in_sandbox(
    connection: StorageProcessor<'_>,
    tx: Transaction,
    execution_mode: TxExecutionMode,
    operator_account: AccountTreeId,
    block_id: api::BlockId,
    resolved_block_number: zksync_types::MiniblockNumber,
    block_timestamp_s: Option<u64>,
    enforced_nonce: Option<Nonce>,
    added_balance: U256,
    job_type: BootloaderJobType,
    l1_gas_price: u64,
    fair_l2_gas_price: u64,
    enforced_base_fee: Option<u64>,
    base_system_contract: &BaseSystemContracts,
    trace_call: bool,
    storage_read_cache: &mut HashMap<StorageKey, H256>,
) -> (
    TransactionExecutionMetrics,
    Result<VmExecutionResult, SandboxExecutionError>,
) {
    let span = span!(Level::DEBUG, "execute_in_sandbox").entered();

    let total_factory_deps = tx
        .execute
        .factory_deps
        .as_ref()
        .map_or(0, |deps| deps.len() as u16);

    let execution_result = apply_vm_in_sandbox(
        connection,
        tx,
        execution_mode,
        base_system_contract,
        operator_account,
        block_id,
        resolved_block_number,
        block_timestamp_s,
        enforced_nonce,
        added_balance,
        l1_gas_price,
        fair_l2_gas_price,
        enforced_base_fee,
        storage_read_cache,
        |vm, tx| {
            push_transaction_to_bootloader_memory(vm, &tx, execution_mode, None);
            let result = if trace_call {
                vm.execute_till_block_end_with_call_tracer(job_type)
            } else {
                vm.execute_till_block_end(job_type)
            };

            span.exit();
            result.full_result
        },
    );

    let tx_execution_metrics = collect_tx_execution_metrics(total_factory_deps, &execution_result);

    (
        tx_execution_metrics,
        match execution_result.revert_reason {
            None => Ok(execution_result),
            Some(revert) => Err(revert.revert_reason.into()),
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn apply_vm_in_sandbox<T>(
    mut connection: StorageProcessor<'_>,
    tx: Transaction,
    execution_mode: TxExecutionMode,
    base_system_contracts: &BaseSystemContracts,
    operator_account: AccountTreeId,
    block_id: api::BlockId,
    resolved_block_number: zksync_types::MiniblockNumber,
    block_timestamp_s: Option<u64>,
    enforced_nonce: Option<Nonce>,
    added_balance: U256,
    l1_gas_price: u64,
    fair_l2_gas_price: u64,
    enforced_base_fee: Option<u64>,
    storage_read_cache: &mut HashMap<StorageKey, H256>,
    apply: impl FnOnce(&mut Box<VmInstance<'_, HistoryDisabled>>, Transaction) -> T,
) -> T {
    let stage_started_at = Instant::now();
    let span = span!(Level::DEBUG, "initialization").entered();

    let (state_block_number, vm_block_number) = match block_id {
        api::BlockId::Number(api::BlockNumber::Pending) => {
            let sealed_l1_batch_number = connection
                .blocks_web3_dal()
                .get_sealed_l1_batch_number()
                .unwrap();
            let sealed_miniblock_number = connection
                .blocks_web3_dal()
                .get_sealed_miniblock_number()
                .unwrap();
            (sealed_miniblock_number, sealed_l1_batch_number + 1)
        }
        _ => {
            let l1_batch_number = match connection
                .blocks_web3_dal()
                .get_l1_batch_number_of_miniblock(resolved_block_number)
                .unwrap()
            {
                Some(l1_batch_number) => l1_batch_number,
                None => {
                    connection
                        .blocks_web3_dal()
                        .get_sealed_l1_batch_number()
                        .unwrap()
                        + 1
                }
            };
            (resolved_block_number, l1_batch_number)
        }
    };

    let db_storage_provider = DbStorageProvider::new(connection, state_block_number, false);

    // Moving `storage_read_cache` to `storage_view`. It will be moved back once execution is finished and `storage_view` is not needed.
    let mut storage_view =
        StorageView::new_with_read_keys(db_storage_provider, std::mem::take(storage_read_cache));

    let block_timestamp_ms = match block_id {
        api::BlockId::Number(api::BlockNumber::Pending) => millis_since_epoch(),
        _ => {
            let block_timestamp_s = block_timestamp_s.unwrap_or_else(|| {
                panic!(
                    "Block timestamp is `None`, `block_id`: {:?}, `resolved_block_number`: {}",
                    block_id, resolved_block_number.0
                )
            });
            (block_timestamp_s as u128) * 1000
        }
    };

    if let Some(nonce) = enforced_nonce {
        let nonce_key = get_nonce_key(&tx.initiator_account());
        let full_nonce = storage_view.get_value(&nonce_key);
        let (_, deployment_nonce) = decompose_full_nonce(h256_to_u256(full_nonce));

        let enforced_full_nonce = nonces_to_full_nonce(U256::from(nonce.0), deployment_nonce);

        storage_view.set_value(&nonce_key, u256_to_h256(enforced_full_nonce));
    }

    {
        let payer = tx.payer();
        let balance_key = storage_key_for_eth_balance(&payer);

        let current_balance = h256_to_u256(storage_view.get_value(&balance_key));
        storage_view.set_value(&balance_key, u256_to_h256(current_balance + added_balance));
    }

    let mut oracle_tools =
        vm::OracleTools::new(&mut storage_view as &mut dyn Storage, HistoryDisabled);
    let block_properties = BlockProperties {
        default_aa_code_hash: h256_to_u256(base_system_contracts.default_aa.hash),
        zkporter_is_available: ZKPORTER_IS_AVAILABLE,
    };

    let block_context = DerivedBlockContext {
        context: BlockContext {
            block_number: vm_block_number.0,
            block_timestamp: (block_timestamp_ms / 1000) as u64,
            l1_gas_price,
            fair_l2_gas_price,
            operator_address: *operator_account.address(),
        },
        base_fee: enforced_base_fee.unwrap_or_else(|| {
            derive_base_fee_and_gas_per_pubdata(l1_gas_price, fair_l2_gas_price).0
        }),
    };

    // Since this method assumes that the block vm_block_number-1 is present in the DB, it means that its hash
    // has already been stored in the VM.
    let block_context_properties = BlockContextMode::OverrideCurrent(block_context);

    let mut vm = init_vm(
        &mut oracle_tools,
        block_context_properties,
        &block_properties,
        execution_mode,
        base_system_contracts,
    );

    metrics::histogram!("api.web3.sandbox", stage_started_at.elapsed(), "stage" => "initialization");
    span.exit();

    let tx_id = format!(
        "{:?}-{} ",
        tx.initiator_account(),
        tx.nonce().unwrap_or(Nonce(0))
    );

    let stage_started_at = Instant::now();
    let result = apply(&mut vm, tx);
    let vm_execution_took = stage_started_at.elapsed();
    metrics::histogram!("api.web3.sandbox", vm_execution_took, "stage" => "execution");

    let oracles_sizes = record_vm_memory_metrics(vm);
    let storage_view_cache = storage_view.get_cache_size();
    metrics::histogram!(
        "runtime_context.memory.storage_view_cache_size",
        storage_view_cache as f64
    );
    metrics::histogram!(
        "runtime_context.memory",
        (oracles_sizes + storage_view_cache) as f64
    );

    let total_storage_invocations =
        storage_view.get_value_storage_invocations + storage_view.set_value_storage_invocations;
    let total_time_spent_in_storage =
        storage_view.time_spent_on_get_value + storage_view.time_spent_on_set_value;

    metrics::histogram!("runtime_context.storage_interaction.amount", storage_view.storage_invocations_missed as f64, "interaction" => "missed");
    metrics::histogram!("runtime_context.storage_interaction.amount", storage_view.get_value_storage_invocations as f64, "interaction" => "get_value");
    metrics::histogram!("runtime_context.storage_interaction.amount", storage_view.set_value_storage_invocations as f64, "interaction" => "set_value");
    metrics::histogram!("runtime_context.storage_interaction.amount", (total_storage_invocations) as f64, "interaction" => "total");

    metrics::histogram!("runtime_context.storage_interaction.duration", storage_view.time_spent_on_storage_missed, "interaction" => "missed");
    metrics::histogram!("runtime_context.storage_interaction.duration", storage_view.time_spent_on_get_value, "interaction" => "get_value");
    metrics::histogram!("runtime_context.storage_interaction.duration", storage_view.time_spent_on_set_value, "interaction" => "set_value");
    metrics::histogram!("runtime_context.storage_interaction.duration", total_time_spent_in_storage, "interaction" => "total");

    if total_storage_invocations > 0 {
        metrics::histogram!(
            "runtime_context.storage_interaction.duration_per_unit",
            total_time_spent_in_storage.div_f64(total_storage_invocations as f64),
            "interaction" => "total"
        );
    }
    if storage_view.storage_invocations_missed > 0 {
        metrics::histogram!(
            "runtime_context.storage_interaction.duration_per_unit",
            storage_view.time_spent_on_storage_missed.div_f64(storage_view.storage_invocations_missed as f64),
            "interaction" => "missed"
        );
    }

    metrics::histogram!(
        "runtime_context.storage_interaction.ratio",
        total_time_spent_in_storage.as_secs_f64() / vm_execution_took.as_secs_f64(),
    );

    const STORAGE_INVOCATIONS_DEBUG_THRESHOLD: usize = 1000;

    if total_storage_invocations > STORAGE_INVOCATIONS_DEBUG_THRESHOLD {
        vlog::info!(
            "Tx {} resulted in {} storage_invocations, {} new_storage_invocations, {} get_value_storage_invocations, {} set_value_storage_invocations, vm execution tool {:?}, storage interaction took {:?} (missed: {:?} get: {:?} set: {:?})",
            tx_id,
            total_storage_invocations,
            storage_view.storage_invocations_missed,
            storage_view.get_value_storage_invocations,
            storage_view.set_value_storage_invocations,
            vm_execution_took,
            total_time_spent_in_storage,
            storage_view.time_spent_on_storage_missed,
            storage_view.time_spent_on_get_value,
            storage_view.time_spent_on_set_value,
        );
    }

    // Move `read_storage_keys` from `storage_view` back to cache.
    *storage_read_cache = storage_view.take_read_storage_keys();

    result
}

// Some slots can be marked as "trusted". That is needed for slots which can not be
// trusted to change between validation and execution in general case, but
// sometimes we can safely rely on them to not change often.
fn get_validation_params(
    connection: &mut StorageProcessor<'_>,
    tx: &L2Tx,
    computational_gas_limit: u32,
) -> ValidationTracerParams {
    let user_address = tx.common_data.initiator_address;
    let paymaster_address = tx.common_data.paymaster_params.paymaster;

    // This method assumes that the number of "well-known" tokens is relatively low. When it grows
    // we may need to introduce some kind of caching.
    let well_known_tokens: Vec<_> = connection
        .tokens_dal()
        .get_well_known_token_addresses()
        .into_iter()
        .map(|token| token.1)
        .collect();

    let trusted_slots: HashSet<_> = well_known_tokens
        .clone()
        .into_iter()
        .flat_map(|token| {
            TRUSTED_TOKEN_SLOTS
                .clone()
                .into_iter()
                .map(move |slot| (token, slot))
        })
        .collect();

    // We currently don't support any specific trusted addresses.
    let trusted_addresses = HashSet::new();

    // The slots the value of which will be added as allowed address on the fly.
    // Required for working with transparent proxies.
    let trusted_address_slots: HashSet<_> = well_known_tokens
        .into_iter()
        .flat_map(|token| {
            TRUSTED_ADDRESS_SLOTS
                .clone()
                .into_iter()
                .map(move |slot| (token, slot))
        })
        .collect();

    ValidationTracerParams {
        user_address,
        paymaster_address,
        trusted_slots,
        trusted_addresses,
        trusted_address_slots,
        computational_gas_limit,
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_tx_in_sandbox(
    mut connection: StorageProcessor<'_>,
    tx: L2Tx,
    execution_mode: TxExecutionMode,
    operator_account: AccountTreeId,
    base_system_contracts: &BaseSystemContracts,
    block_id: api::BlockId,
    resolved_block_number: zksync_types::MiniblockNumber,
    block_timestamp_s: Option<u64>,
    enforced_nonce: Option<Nonce>,
    added_balance: U256,
    l1_gas_price: u64,
    fair_l2_gas_price: u64,
    enforced_base_fee: Option<u64>,
    computational_gas_limit: u32,
) -> Result<(), ValidationError> {
    let stage_started_at = Instant::now();
    let span = span!(Level::DEBUG, "validate_in_sandbox").entered();
    let validation_params = get_validation_params(&mut connection, &tx, computational_gas_limit);

    let tx: Transaction = tx.into();

    let validation_result = apply_vm_in_sandbox(
        connection,
        tx,
        execution_mode,
        base_system_contracts,
        operator_account,
        block_id,
        resolved_block_number,
        block_timestamp_s,
        enforced_nonce,
        added_balance,
        l1_gas_price,
        fair_l2_gas_price,
        enforced_base_fee,
        &mut Default::default(),
        |vm, tx| {
            let stage_started_at = Instant::now();
            let span = span!(Level::DEBUG, "validation").entered();

            push_transaction_to_bootloader_memory(vm, &tx, execution_mode, None);
            let result = vm.execute_validation(validation_params);

            metrics::histogram!("api.web3.sandbox", stage_started_at.elapsed(), "stage" => "validation");
            span.exit();

            result
        },
    );

    metrics::histogram!("server.api.validation_sandbox", stage_started_at.elapsed(), "stage" => "validate_in_sandbox");
    span.exit();

    validation_result
}

fn collect_tx_execution_metrics(
    contracts_deployed: u16,
    result: &VmExecutionResult,
) -> TransactionExecutionMetrics {
    let event_topics = result
        .events
        .iter()
        .map(|event| event.indexed_topics.len() as u16)
        .sum();

    let l2_l1_long_messages = extract_long_l2_to_l1_messages(&result.events)
        .iter()
        .map(|event| event.len())
        .sum();

    let published_bytecode_bytes = extract_published_bytecodes(&result.events)
        .iter()
        .map(|bytecodehash| bytecode_len_in_bytes(*bytecodehash))
        .sum();

    let writes_metrics =
        StorageWritesDeduplicator::apply_on_empty_state(&result.storage_log_queries);

    TransactionExecutionMetrics {
        initial_storage_writes: writes_metrics.initial_storage_writes,
        repeated_storage_writes: writes_metrics.repeated_storage_writes,
        gas_used: result.gas_used as usize,
        event_topics,
        l2_l1_long_messages,
        published_bytecode_bytes,
        contracts_used: result.contracts_used,
        contracts_deployed,
        l2_l1_logs: result.l2_to_l1_logs.len(),
        vm_events: result.events.len(),
        storage_logs: result.storage_log_queries.len(),
        total_log_queries: result.total_log_queries,
        cycles_used: result.cycles_used,
        computational_gas_used: result.computational_gas_used,
    }
}

impl From<TxRevertReason> for SandboxExecutionError {
    fn from(reason: TxRevertReason) -> Self {
        match reason {
            TxRevertReason::EthCall(reason) => SandboxExecutionError::Revert(
                reason.to_user_friendly_string(),
                reason.encoded_data(),
            ),
            TxRevertReason::TxReverted(reason) => SandboxExecutionError::Revert(
                reason.to_user_friendly_string(),
                reason.encoded_data(),
            ),
            TxRevertReason::FailedToChargeFee(reason) => {
                SandboxExecutionError::FailedToChargeFee(reason.to_string())
            }
            TxRevertReason::FromIsNotAnAccount => SandboxExecutionError::FromIsNotAnAccount,
            TxRevertReason::InnerTxError => SandboxExecutionError::InnerTxError,
            TxRevertReason::Unknown(reason) => {
                SandboxExecutionError::BootloaderFailure(reason.to_string())
            }
            TxRevertReason::ValidationFailed(reason) => {
                SandboxExecutionError::AccountValidationFailed(reason.to_string())
            }
            TxRevertReason::PaymasterValidationFailed(reason) => {
                SandboxExecutionError::PaymasterValidationFailed(reason.to_string())
            }
            TxRevertReason::PrePaymasterPreparationFailed(reason) => {
                SandboxExecutionError::PrePaymasterPreparationFailed(reason.to_string())
            }
            TxRevertReason::UnexpectedVMBehavior(reason) => {
                SandboxExecutionError::UnexpectedVMBehavior(reason)
            }
            TxRevertReason::BootloaderOutOfGas => {
                SandboxExecutionError::UnexpectedVMBehavior("bootloader is out of gas".to_string())
            }
            TxRevertReason::NotEnoughGasProvided => SandboxExecutionError::UnexpectedVMBehavior(
                "The bootloader did not contain enough gas to execute the transaction".to_string(),
            ),
            revert_reason @ TxRevertReason::FailedToMarkFactoryDependencies(_) => {
                SandboxExecutionError::Revert(revert_reason.to_string(), vec![])
            }
            TxRevertReason::PayForTxFailed(reason) => {
                SandboxExecutionError::FailedToPayForTransaction(reason.to_string())
            }
            TxRevertReason::TooBigGasLimit => {
                SandboxExecutionError::Revert(TxRevertReason::TooBigGasLimit.to_string(), vec![])
            }
            TxRevertReason::MissingInvocationLimitReached => SandboxExecutionError::InnerTxError,
        }
    }
}

/// Returns the sum of all oracles' sizes.
fn record_vm_memory_metrics<H: HistoryMode>(vm: Box<VmInstance<'_, H>>) -> usize {
    let event_sink_inner = vm.state.event_sink.get_size();
    let event_sink_history = vm.state.event_sink.get_history_size();
    let memory_inner = vm.state.memory.get_size();
    let memory_history = vm.state.memory.get_history_size();
    let decommittment_processor_inner = vm.state.decommittment_processor.get_size();
    let decommittment_processor_history = vm.state.decommittment_processor.get_history_size();
    let storage_inner = vm.state.storage.get_size();
    let storage_history = vm.state.storage.get_history_size();

    metrics::histogram!("runtime_context.memory.event_sink_size", event_sink_inner as f64, "type" => "inner");
    metrics::histogram!("runtime_context.memory.event_sink_size", event_sink_history as f64, "type" => "history");
    metrics::histogram!("runtime_context.memory.memory_size", memory_inner as f64, "type" => "inner");
    metrics::histogram!("runtime_context.memory.memory_size", memory_history as f64, "type" => "history");
    metrics::histogram!("runtime_context.memory.decommitter_size", decommittment_processor_inner as f64, "type" => "inner");
    metrics::histogram!("runtime_context.memory.decommitter_size", decommittment_processor_history as f64, "type" => "history");
    metrics::histogram!("runtime_context.memory.storage_size", storage_inner as f64, "type" => "inner");
    metrics::histogram!("runtime_context.memory.storage_size", storage_history as f64, "type" => "history");

    [
        event_sink_inner,
        event_sink_history,
        memory_inner,
        memory_history,
        decommittment_processor_inner,
        decommittment_processor_history,
        storage_inner,
        storage_history,
    ]
    .iter()
    .sum::<usize>()
}

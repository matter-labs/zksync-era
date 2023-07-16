use once_cell::sync::Lazy;
use vm::{
    utils::{create_test_block_params, read_bootloader_test_code, BLOCK_GAS_LIMIT},
    vm_with_bootloader::{
        init_vm_inner, push_raw_transaction_to_bootloader_memory, BlockContextMode,
        BootloaderJobType, DerivedBlockContext, TxExecutionMode,
    },
    zk_evm::{aux_structures::Timestamp, zkevm_opcode_defs::BOOTLOADER_HEAP_PAGE},
    HistoryEnabled, OracleTools,
};
use zksync_contracts::{
    load_sys_contract, read_bootloader_code, read_sys_contract_bytecode, BaseSystemContracts,
    ContractLanguage, SystemContractCode,
};
use zksync_state::{InMemoryStorage, StorageView, WriteStorage};
use zksync_types::{
    ethabi::Token,
    fee::Fee,
    l1::L1Tx,
    l2::L2Tx,
    tx::{
        tx_execution_info::{TxExecutionStatus, VmExecutionLogs},
        ExecutionMetrics,
    },
    utils::storage_key_for_eth_balance,
    AccountTreeId, Address, Execute, L1TxCommonData, L2ChainId, Nonce, StorageKey, Transaction,
    BOOTLOADER_ADDRESS, H256, SYSTEM_CONTEXT_ADDRESS, SYSTEM_CONTEXT_GAS_PRICE_POSITION,
    SYSTEM_CONTEXT_TX_ORIGIN_POSITION, U256,
};
use zksync_utils::{bytecode::hash_bytecode, bytes_to_be_words, u256_to_h256};

use crate::intrinsic_costs::VmSpentResourcesResult;

pub static GAS_TEST_SYSTEM_CONTRACTS: Lazy<BaseSystemContracts> = Lazy::new(|| {
    let bytecode = read_bootloader_code("gas_test");
    let hash = hash_bytecode(&bytecode);

    let bootloader = SystemContractCode {
        code: bytes_to_be_words(bytecode),
        hash,
    };

    let bytecode = read_sys_contract_bytecode("", "DefaultAccount", ContractLanguage::Sol);
    let hash = hash_bytecode(&bytecode);
    BaseSystemContracts {
        default_aa: SystemContractCode {
            code: bytes_to_be_words(bytecode),
            hash,
        },
        bootloader,
    }
});

// 100 gwei is base fee large enough for almost any L1 gas price
const BIG_BASE_FEE: u64 = 100_000_000_000;

pub(super) fn get_l2_tx(contract_address: Address, signer: &H256, pubdata_price: u32) -> L2Tx {
    L2Tx::new_signed(
        contract_address,
        vec![],
        Nonce(0),
        Fee {
            gas_limit: U256::from(10000000u32),
            max_fee_per_gas: U256::from(BIG_BASE_FEE),
            max_priority_fee_per_gas: U256::from(0),
            gas_per_pubdata_limit: pubdata_price.into(),
        },
        U256::from(0),
        L2ChainId(270),
        signer,
        None,
        Default::default(),
    )
    .unwrap()
}

pub(super) fn get_l2_txs(number_of_txs: usize) -> (Vec<Transaction>, Vec<Transaction>) {
    let mut txs_with_pubdata_price = vec![];
    let mut txs_without_pubdata_price = vec![];

    for _ in 0..number_of_txs {
        let signer = H256::random();
        let contract_address = Address::random();

        txs_without_pubdata_price.push(get_l2_tx(contract_address, &signer, 0).into());

        txs_with_pubdata_price.push(get_l2_tx(contract_address, &signer, 1).into());
    }

    (txs_with_pubdata_price, txs_without_pubdata_price)
}

pub(super) fn get_l1_tx(
    id: u64,
    sender: Address,
    contract_address: Address,
    pubdata_price: u32,
    custom_gas_limit: Option<U256>,
    custom_calldata: Option<Vec<u8>>,
    factory_deps: Option<Vec<Vec<u8>>>,
) -> L1Tx {
    L1Tx {
        execute: Execute {
            contract_address,
            calldata: custom_calldata.unwrap_or_default(),
            value: U256::from(0),
            factory_deps,
        },
        common_data: L1TxCommonData {
            sender,
            serial_id: id.into(),
            gas_limit: custom_gas_limit.unwrap_or_else(|| U256::from(10000000u32)),
            gas_per_pubdata_limit: pubdata_price.into(),
            ..Default::default()
        },
        received_timestamp_ms: 0,
    }
}

pub(super) fn get_l1_txs(number_of_txs: usize) -> (Vec<Transaction>, Vec<Transaction>) {
    let mut txs_with_pubdata_price = vec![];
    let mut txs_without_pubdata_price = vec![];

    for id in 0..number_of_txs {
        let sender = Address::random();
        let contract_address = Address::random();

        txs_without_pubdata_price
            .push(get_l1_tx(id as u64, sender, contract_address, 0, None, None, None).into());

        txs_with_pubdata_price
            .push(get_l1_tx(id as u64, sender, contract_address, 1, None, None, None).into());
    }

    (txs_with_pubdata_price, txs_without_pubdata_price)
}

/// Executes the "internal transfer test" of the bootloader -- the test that
/// returns the amount of gas needed to perform and internal transfer, assuming no gas price
/// per pubdata, i.e. under assumption that the refund will not touch any new slots.
pub(super) fn execute_internal_transfer_test() -> u32 {
    let (block_context, block_properties) = create_test_block_params();
    let block_context: DerivedBlockContext = block_context.into();

    let raw_storage = InMemoryStorage::with_system_contracts(hash_bytecode);
    let mut storage_view = StorageView::new(raw_storage);
    let bootloader_balance_key = storage_key_for_eth_balance(&BOOTLOADER_ADDRESS);
    storage_view.set_value(bootloader_balance_key, u256_to_h256(U256([0, 0, 1, 0])));
    let mut oracle_tools = OracleTools::new(&mut storage_view, HistoryEnabled);

    let bytecode = read_bootloader_test_code("transfer_test");
    let hash = hash_bytecode(&bytecode);
    let bootloader = SystemContractCode {
        code: bytes_to_be_words(bytecode),
        hash,
    };

    let bytecode = read_sys_contract_bytecode("", "DefaultAccount", ContractLanguage::Sol);
    let hash = hash_bytecode(&bytecode);
    let default_aa = SystemContractCode {
        code: bytes_to_be_words(bytecode),
        hash,
    };

    let base_system_contract = BaseSystemContracts {
        bootloader,
        default_aa,
    };

    let mut vm = init_vm_inner(
        &mut oracle_tools,
        BlockContextMode::NewBlock(block_context, Default::default()),
        &block_properties,
        BLOCK_GAS_LIMIT,
        &base_system_contract,
        TxExecutionMode::VerifyExecute,
    );

    let eth_token_sys_contract = load_sys_contract("L2EthToken");
    let transfer_from_to = &eth_token_sys_contract
        .functions
        .get("transferFromTo")
        .unwrap()[0];
    let input = {
        let mut input = transfer_from_to
            .encode_input(&[
                Token::Address(BOOTLOADER_ADDRESS),
                Token::Address(Address::random()),
                Token::Uint(U256::from(1u32)),
            ])
            .expect("Failed to encode the calldata");

        // Padding input to be divisible by 32
        while input.len() % 32 != 0 {
            input.push(0);
        }
        input
    };
    let input: Vec<_> = bytes_to_be_words(input).into_iter().enumerate().collect();
    vm.state
        .memory
        .populate_page(BOOTLOADER_HEAP_PAGE as usize, input, Timestamp(0));

    let result = vm.execute_till_block_end(BootloaderJobType::BlockPostprocessing);

    assert!(
        result.block_tip_result.revert_reason.is_none(),
        "The internal call has reverted"
    );
    assert!(
        result.full_result.revert_reason.is_none(),
        "The internal call has reverted"
    );

    let value_recorded_from_test = vm.state.memory.read_slot(BOOTLOADER_HEAP_PAGE as usize, 0);

    value_recorded_from_test.value.as_u32()
}

// Executes an array of transactions in the VM.
pub(super) fn execute_user_txs_in_test_gas_vm(
    txs: Vec<Transaction>,
    accept_failure: bool,
) -> VmSpentResourcesResult {
    let total_gas_paid_upfront = txs
        .iter()
        .fold(U256::zero(), |sum, elem| sum + elem.gas_limit());

    let (block_context, block_properties) = create_test_block_params();
    let block_context: DerivedBlockContext = block_context.into();

    let raw_storage = InMemoryStorage::with_system_contracts(hash_bytecode);
    let mut storage_view = StorageView::new(raw_storage);

    for tx in txs.iter() {
        let sender_address = tx.initiator_account();
        let key = storage_key_for_eth_balance(&sender_address);
        storage_view.set_value(key, u256_to_h256(U256([0, 0, 1, 0])));
    }

    // We also set some of the storage slots to non-zero values. This is not how it will be
    // done in production, but it allows to estimate the overhead of the bootloader more correctly
    {
        let bootloader_balance_key = storage_key_for_eth_balance(&BOOTLOADER_ADDRESS);
        let tx_origin_key = StorageKey::new(
            AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
            SYSTEM_CONTEXT_TX_ORIGIN_POSITION,
        );
        let tx_gas_price_key = StorageKey::new(
            AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
            SYSTEM_CONTEXT_GAS_PRICE_POSITION,
        );

        storage_view.set_value(bootloader_balance_key, u256_to_h256(U256([1, 0, 0, 0])));
        storage_view.set_value(tx_origin_key, u256_to_h256(U256([1, 0, 0, 0])));
        storage_view.set_value(tx_gas_price_key, u256_to_h256(U256([1, 0, 0, 0])));
    }

    let mut oracle_tools = OracleTools::new(&mut storage_view, HistoryEnabled);

    let mut vm = init_vm_inner(
        &mut oracle_tools,
        BlockContextMode::NewBlock(block_context, Default::default()),
        &block_properties,
        BLOCK_GAS_LIMIT,
        &GAS_TEST_SYSTEM_CONTRACTS,
        TxExecutionMode::VerifyExecute,
    );

    let mut total_gas_refunded = 0;
    for tx in txs {
        push_raw_transaction_to_bootloader_memory(
            &mut vm,
            tx.clone().into(),
            TxExecutionMode::VerifyExecute,
            0,
            None,
        );
        let tx_execution_result = vm
            .execute_next_tx(u32::MAX, false)
            .expect("Bootloader failed while processing transaction");

        total_gas_refunded += tx_execution_result.gas_refunded;
        if !accept_failure {
            assert_eq!(
                tx_execution_result.status,
                TxExecutionStatus::Success,
                "A transaction has failed"
            );
        }
    }

    let result = vm.execute_till_block_end(BootloaderJobType::BlockPostprocessing);
    let execution_logs = VmExecutionLogs {
        storage_logs: result.full_result.storage_log_queries,
        events: result.full_result.events,
        l2_to_l1_logs: result.full_result.l2_to_l1_logs,
        total_log_queries_count: result.full_result.total_log_queries,
    };

    let metrics = ExecutionMetrics::new(
        &execution_logs,
        result.full_result.gas_used as usize,
        0, // The number of contracts deployed is irrelevant for our needs
        result.full_result.contracts_used,
        result.full_result.cycles_used,
        result.full_result.computational_gas_used,
    );

    VmSpentResourcesResult {
        gas_consumed: vm.gas_consumed(),
        total_gas_paid: total_gas_paid_upfront.as_u32() - total_gas_refunded,
        pubdata_published: metrics.size() as u32,
        total_pubdata_paid: 0,
    }
}

// Denotes a function that should return a tuple of arrays transactions.
// The first array should be with transactions with pubdata price 1.
// The second array should be with transactions with pubdata price 0.
pub type TransactionGenerator = dyn Fn(usize) -> (Vec<Transaction>, Vec<Transaction>);

// The easiest way to retrieve the amount of gas the user has spent
// on public data is by comparing the results for the same transaction, but
// with different pubdata price (0 vs 1 respectively). The difference in gas
// paid by the users will be the number of gas spent on pubdata.
pub(crate) fn metrics_from_txs(
    number_of_txs: usize,
    tx_generator: &TransactionGenerator,
) -> VmSpentResourcesResult {
    let (txs_with_pubdata_price, txs_without_pubdata_price) = tx_generator(number_of_txs);

    let tx_results_with_pubdata_price =
        execute_user_txs_in_test_gas_vm(txs_with_pubdata_price, false);
    let tx_results_without_pubdata_price =
        execute_user_txs_in_test_gas_vm(txs_without_pubdata_price, false);

    // Sanity check
    assert_eq!(
        tx_results_with_pubdata_price.pubdata_published,
        tx_results_without_pubdata_price.pubdata_published,
        "The transactions should have identical pubdata published"
    );

    // We will use the results from the zero pubdata price block as the basis for the results
    // but we will use the difference in gas spent as the number of pubdata compensated by the users
    VmSpentResourcesResult {
        total_pubdata_paid: tx_results_with_pubdata_price.total_gas_paid
            - tx_results_without_pubdata_price.total_gas_paid,
        ..tx_results_without_pubdata_price
    }
}

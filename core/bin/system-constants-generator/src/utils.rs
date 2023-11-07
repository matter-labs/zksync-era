use once_cell::sync::Lazy;
use std::cell::RefCell;
use std::rc::Rc;

use multivm::interface::{
    dyn_tracers::vm_1_4_0::DynTracer, tracer::VmExecutionStopReason, L1BatchEnv, L2BlockEnv,
    SystemEnv, TxExecutionMode, VmExecutionMode, VmInterface,
};
use multivm::vm_latest::{
    constants::{BLOCK_GAS_LIMIT, BOOTLOADER_HEAP_PAGE},
    BootloaderState, HistoryEnabled, HistoryMode, SimpleMemory, ToTracerPointer, Vm, VmTracer,
    ZkSyncVmState,
};
use zksync_contracts::{
    load_sys_contract, read_bootloader_code, read_sys_contract_bytecode, read_zbin_bytecode,
    BaseSystemContracts, ContractLanguage, SystemContractCode,
};
use zksync_state::{InMemoryStorage, StorageView, WriteStorage};
use zksync_types::{
    block::legacy_miniblock_hash, ethabi::Token, fee::Fee, l1::L1Tx, l2::L2Tx,
    utils::storage_key_for_eth_balance, AccountTreeId, Address, Execute, L1BatchNumber,
    L1TxCommonData, L2ChainId, MiniblockNumber, Nonce, ProtocolVersionId, StorageKey, Timestamp,
    Transaction, BOOTLOADER_ADDRESS, H256, SYSTEM_CONTEXT_ADDRESS,
    SYSTEM_CONTEXT_GAS_PRICE_POSITION, SYSTEM_CONTEXT_TX_ORIGIN_POSITION, U256,
    ZKPORTER_IS_AVAILABLE,
};
use zksync_utils::{bytecode::hash_bytecode, bytes_to_be_words, u256_to_h256};

use crate::intrinsic_costs::VmSpentResourcesResult;

/// Tracer for setting the data for bootloader with custom input
/// and receive an output from this custom bootloader
struct SpecialBootloaderTracer {
    input: Vec<(usize, U256)>,
    output: Rc<RefCell<u32>>,
}

impl<S: WriteStorage, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for SpecialBootloaderTracer {}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for SpecialBootloaderTracer {
    fn initialize_tracer(&mut self, state: &mut ZkSyncVmState<S, H>) {
        state.memory.populate_page(
            BOOTLOADER_HEAP_PAGE as usize,
            self.input.clone(),
            Timestamp(0),
        );
    }
    fn after_vm_execution(
        &mut self,
        state: &mut ZkSyncVmState<S, H>,
        _bootloader_state: &BootloaderState,
        _stop_reason: VmExecutionStopReason,
    ) {
        let value_recorded_from_test = state.memory.read_slot(BOOTLOADER_HEAP_PAGE as usize, 0);
        let mut res = self.output.borrow_mut();
        *res = value_recorded_from_test.value.as_u32();
    }
}

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
        L2ChainId::from(270),
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

fn read_bootloader_test_code(test: &str) -> Vec<u8> {
    read_zbin_bytecode(format!(
        "etc/system-contracts/bootloader/tests/artifacts/{}.yul/{}.yul.zbin",
        test, test
    ))
}

fn default_l1_batch() -> L1BatchEnv {
    L1BatchEnv {
        previous_batch_hash: None,
        number: L1BatchNumber(1),
        timestamp: 100,
        l1_gas_price: 50_000_000_000,   // 50 gwei
        fair_l2_gas_price: 250_000_000, // 0.25 gwei
        fee_account: Address::random(),
        enforced_base_fee: None,
        first_l2_block: L2BlockEnv {
            number: 1,
            timestamp: 100,
            prev_block_hash: legacy_miniblock_hash(MiniblockNumber(0)),
            max_virtual_blocks_to_create: 100,
        },
    }
}

/// Executes the "internal transfer test" of the bootloader -- the test that
/// returns the amount of gas needed to perform and internal transfer, assuming no gas price
/// per pubdata, i.e. under assumption that the refund will not touch any new slots.
pub(super) fn execute_internal_transfer_test() -> u32 {
    let raw_storage = InMemoryStorage::with_system_contracts(hash_bytecode);
    let mut storage_view = StorageView::new(raw_storage);
    let bootloader_balance_key = storage_key_for_eth_balance(&BOOTLOADER_ADDRESS);
    storage_view.set_value(bootloader_balance_key, u256_to_h256(U256([0, 0, 1, 0])));
    let bytecode = read_bootloader_test_code("transfer_test");
    let hash = hash_bytecode(&bytecode);
    let bootloader = SystemContractCode {
        code: bytes_to_be_words(bytecode),
        hash,
    };

    let l1_batch = default_l1_batch();

    let bytecode = read_sys_contract_bytecode("", "DefaultAccount", ContractLanguage::Sol);
    let hash = hash_bytecode(&bytecode);
    let default_aa = SystemContractCode {
        code: bytes_to_be_words(bytecode),
        hash,
    };

    let base_system_smart_contracts = BaseSystemContracts {
        bootloader,
        default_aa,
    };

    let system_env = SystemEnv {
        zk_porter_available: ZKPORTER_IS_AVAILABLE,
        version: ProtocolVersionId::latest(),
        base_system_smart_contracts,
        gas_limit: BLOCK_GAS_LIMIT,
        execution_mode: TxExecutionMode::VerifyExecute,
        default_validation_computational_gas_limit: BLOCK_GAS_LIMIT,
        chain_id: L2ChainId::default(),
    };

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

    let tracer_result = Rc::new(RefCell::new(0));
    let tracer = SpecialBootloaderTracer {
        input,
        output: tracer_result.clone(),
    }
    .into_tracer_pointer();
    let mut vm: Vm<_, HistoryEnabled> =
        Vm::new(l1_batch, system_env, Rc::new(RefCell::new(storage_view)));
    let result = vm.inspect(tracer.into(), VmExecutionMode::Bootloader);

    assert!(!result.result.is_failed(), "The internal call has reverted");
    tracer_result.take()
}

// Executes an array of transactions in the VM.
pub(super) fn execute_user_txs_in_test_gas_vm(
    txs: Vec<Transaction>,
    accept_failure: bool,
) -> VmSpentResourcesResult {
    let total_gas_paid_upfront = txs
        .iter()
        .fold(U256::zero(), |sum, elem| sum + elem.gas_limit());

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

    let l1_batch = default_l1_batch();
    let system_env = SystemEnv {
        zk_porter_available: ZKPORTER_IS_AVAILABLE,
        version: ProtocolVersionId::latest(),
        base_system_smart_contracts: GAS_TEST_SYSTEM_CONTRACTS.clone(),
        gas_limit: BLOCK_GAS_LIMIT,
        execution_mode: TxExecutionMode::VerifyExecute,
        default_validation_computational_gas_limit: BLOCK_GAS_LIMIT,
        chain_id: L2ChainId::default(),
    };

    let mut vm: Vm<_, HistoryEnabled> =
        Vm::new(l1_batch, system_env, Rc::new(RefCell::new(storage_view)));

    let mut total_gas_refunded = 0;
    for tx in txs {
        vm.push_transaction(tx);
        let tx_execution_result = vm.execute(VmExecutionMode::OneTx);

        total_gas_refunded += tx_execution_result.refunds.gas_refunded;
        if !accept_failure {
            assert!(
                !tx_execution_result.result.is_failed(),
                "A transaction has failed"
            );
        }
    }

    let result = vm.execute(VmExecutionMode::Bootloader);
    let metrics = result.get_execution_metrics(None);

    VmSpentResourcesResult {
        gas_consumed: result.statistics.gas_used,
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

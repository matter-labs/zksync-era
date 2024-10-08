//! Oneshot executor tests.

use assert_matches::assert_matches;
use once_cell::sync::Lazy;
use test_casing::{test_casing, Product};
use zksync_contracts::BaseSystemContracts;
use zksync_multivm::{
    interface::{storage::InMemoryStorage, L1BatchEnv, L2BlockEnv, SystemEnv},
    utils::derive_base_fee_and_gas_per_pubdata,
    vm_latest::constants::BATCH_COMPUTATIONAL_GAS_LIMIT,
    zk_evm_latest::ethereum_types::Address,
};
use zksync_types::{
    block::L2BlockHasher, fee::Fee, fee_model::BatchFeeInput, transaction_request::PaymasterParams,
    K256PrivateKey, L1BatchNumber, L2BlockNumber, L2ChainId, ProtocolVersionId, H256,
    ZKPORTER_IS_AVAILABLE,
};
use zksync_utils::bytecode::hash_bytecode;

use super::*;

static BASE_SYSTEM_CONTRACTS: Lazy<BaseSystemContracts> =
    Lazy::new(BaseSystemContracts::load_from_disk);

const EXEC_MODES: [TxExecutionMode; 3] = [
    TxExecutionMode::EstimateFee,
    TxExecutionMode::EthCall,
    TxExecutionMode::VerifyExecute,
];
const FAST_VM_MODES: [FastVmMode; 3] = [FastVmMode::Old, FastVmMode::New, FastVmMode::Shadow];

fn default_system_env(execution_mode: TxExecutionMode) -> SystemEnv {
    SystemEnv {
        zk_porter_available: ZKPORTER_IS_AVAILABLE,
        version: ProtocolVersionId::latest(),
        base_system_smart_contracts: BASE_SYSTEM_CONTRACTS.clone(),
        bootloader_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
        execution_mode,
        default_validation_computational_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
        chain_id: L2ChainId::default(),
    }
}

fn default_l1_batch_env(number: u32) -> L1BatchEnv {
    L1BatchEnv {
        previous_batch_hash: Some(H256::zero()),
        number: L1BatchNumber(number),
        timestamp: number.into(),
        fee_account: Address::repeat_byte(0x22),
        enforced_base_fee: None,
        first_l2_block: L2BlockEnv {
            number,
            timestamp: number.into(),
            prev_block_hash: L2BlockHasher::legacy_hash(L2BlockNumber(number - 1)),
            max_virtual_blocks_to_create: 1,
        },
        fee_input: BatchFeeInput::sensible_l1_pegged_default(),
    }
}

fn create_l2_transaction(value: U256, nonce: Nonce) -> L2Tx {
    let (max_fee_per_gas, gas_per_pubdata_limit) = derive_base_fee_and_gas_per_pubdata(
        BatchFeeInput::sensible_l1_pegged_default(),
        ProtocolVersionId::latest().into(),
    );
    let fee = Fee {
        gas_limit: 10_000_000.into(),
        max_fee_per_gas: max_fee_per_gas.into(),
        max_priority_fee_per_gas: 0_u64.into(),
        gas_per_pubdata_limit: gas_per_pubdata_limit.into(),
    };
    L2Tx::new_signed(
        Some(Address::random()),
        vec![],
        nonce,
        fee,
        value,
        L2ChainId::default(),
        &K256PrivateKey::random(),
        vec![],
        PaymasterParams::default(),
    )
    .unwrap()
}

#[test]
fn selecting_vm_for_execution() {
    let mut executor = MainOneshotExecutor::new(usize::MAX);
    executor.set_fast_vm_modes(FastVmMode::New.into());

    for exec_mode in EXEC_MODES {
        let env = OneshotEnv {
            system: default_system_env(exec_mode),
            l1_batch: default_l1_batch_env(1),
            current_block: None,
        };
        let mode = executor.select_fast_vm_mode(&env, &OneshotTracingParams::default());
        assert_matches!(mode, FastVmMode::New);

        // Tracing calls is not supported by the new VM.
        let mode = executor.select_fast_vm_mode(&env, &OneshotTracingParams { trace_calls: true });
        assert_matches!(mode, FastVmMode::Old);

        // Old protocol versions are not supported either.
        let mut old_env = env.clone();
        old_env.system.version = ProtocolVersionId::Version22;
        let mode = executor.select_fast_vm_mode(&old_env, &OneshotTracingParams::default());
        assert_matches!(mode, FastVmMode::Old);
    }

    executor.set_fast_vm_modes(OneshotExecutorVmModes {
        gas_estimation: FastVmMode::New,
        ..OneshotExecutorVmModes::default()
    });

    let mut env = OneshotEnv {
        system: default_system_env(TxExecutionMode::EstimateFee),
        l1_batch: default_l1_batch_env(1),
        current_block: None,
    };
    let mode = executor.select_fast_vm_mode(&env, &OneshotTracingParams::default());
    assert_matches!(mode, FastVmMode::New);

    for exec_mode in [TxExecutionMode::VerifyExecute, TxExecutionMode::EthCall] {
        env.system.execution_mode = exec_mode;
        let mode = executor.select_fast_vm_mode(&env, &OneshotTracingParams::default());
        assert_matches!(mode, FastVmMode::Old);
    }
}

#[test]
fn setting_up_nonce_and_balance_in_storage() {
    let mut storage = StorageWithOverrides::new(InMemoryStorage::default());
    let tx = create_l2_transaction(1_000_000_000.into(), Nonce(1));
    let execution_args = TxExecutionArgs::for_gas_estimate(tx.clone().into());
    VmSandbox::setup_storage(&mut storage, &execution_args, None);

    // Check the overridden nonce and balance.
    let nonce_key = get_nonce_key(&tx.initiator_account());
    assert_eq!(storage.read_value(&nonce_key), H256::from_low_u64_be(1));
    let balance_key = storage_key_for_eth_balance(&tx.initiator_account());
    let expected_added_balance = tx.common_data.fee.gas_limit * tx.common_data.fee.max_fee_per_gas;
    assert_eq!(
        storage.read_value(&balance_key),
        u256_to_h256(expected_added_balance)
    );

    let mut storage = InMemoryStorage::default();
    storage.set_value(balance_key, H256::from_low_u64_be(2_000_000_000));
    let mut storage = StorageWithOverrides::new(storage);
    VmSandbox::setup_storage(&mut storage, &execution_args, None);

    assert_eq!(
        storage.read_value(&balance_key),
        u256_to_h256(expected_added_balance + U256::from(2_000_000_000))
    );
}

#[test_casing(9, Product((EXEC_MODES, FAST_VM_MODES)))]
#[tokio::test]
async fn inspecting_transfer(exec_mode: TxExecutionMode, fast_vm_mode: FastVmMode) {
    let tx = create_l2_transaction(1_000_000_000.into(), Nonce(0));
    let mut storage = InMemoryStorage::with_system_contracts(hash_bytecode);
    storage.set_value(
        storage_key_for_eth_balance(&tx.initiator_account()),
        u256_to_h256(u64::MAX.into()),
    );
    let storage = StorageWithOverrides::new(storage);

    let l1_batch = default_l1_batch_env(1);
    let env = OneshotEnv {
        system: default_system_env(exec_mode),
        current_block: Some(StoredL2BlockEnv {
            number: l1_batch.first_l2_block.number - 1,
            timestamp: l1_batch.first_l2_block.timestamp - 1,
            txs_rolling_hash: H256::zero(),
        }),
        l1_batch,
    };
    let args = TxExecutionArgs::for_gas_estimate(tx.into());
    let tracing = OneshotTracingParams::default();

    let mut executor = MainOneshotExecutor::new(usize::MAX);
    executor.set_fast_vm_modes(fast_vm_mode.into());
    let result = executor
        .inspect_transaction_with_bytecode_compression(storage, env, args, tracing)
        .await
        .unwrap();
    result.compression_result.unwrap();
    let exec_result = result.tx_result.result;
    assert!(!exec_result.is_failed(), "{exec_result:?}");
}

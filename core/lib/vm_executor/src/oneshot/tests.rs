//! Oneshot executor tests.

use assert_matches::assert_matches;
use test_casing::{test_casing, Product};
use zksync_multivm::interface::storage::InMemoryStorage;
use zksync_types::{ProtocolVersionId, H256};

use super::*;
use crate::testonly::{
    create_l2_transaction, default_l1_batch_env, default_system_env, FAST_VM_MODES,
};

const EXEC_MODES: [TxExecutionMode; 3] = [
    TxExecutionMode::EstimateFee,
    TxExecutionMode::EthCall,
    TxExecutionMode::VerifyExecute,
];

#[test]
fn selecting_vm_for_execution() {
    let mut executor = MainOneshotExecutor::new(usize::MAX);
    executor.set_fast_vm_mode(FastVmMode::New);

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
    let mut storage = InMemoryStorage::with_system_contracts();
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
            interop_roots: vec![],
        }),
        l1_batch,
    };
    let args = TxExecutionArgs::for_gas_estimate(tx.into());
    let tracing = OneshotTracingParams::default();

    let mut executor = MainOneshotExecutor::new(usize::MAX);
    executor.set_fast_vm_mode(fast_vm_mode);
    let result = executor
        .inspect_transaction_with_bytecode_compression(storage, env, args, tracing)
        .await
        .unwrap();
    result.compression_result.unwrap();
    let exec_result = result.tx_result.result;
    assert!(!exec_result.is_failed(), "{exec_result:?}");
}

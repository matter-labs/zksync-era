//! Tests for sending raw transactions.

use std::collections::HashMap;

use assert_matches::assert_matches;
use test_casing::test_casing;
use zksync_contracts::get_loadnext_contract;
use zksync_node_fee_model::MockBatchFeeParamsProvider;
use zksync_types::{get_code_key, get_known_code_key, K256PrivateKey, StorageKey};
use zksync_utils::{bytecode::hash_bytecode, u256_to_h256};

use super::*;
use crate::testonly::{TestAccount, LOAD_TEST_ADDRESS};

async fn fund_account(storage: &mut Connection<'_, Core>, address: Address) {
    let balance_key = storage_key_for_eth_balance(&address);
    let storage_log = StorageLog::new_write_log(balance_key, u256_to_h256(U256::one() << 64));
    storage
        .storage_logs_dal()
        .append_storage_logs(L2BlockNumber(0), &[storage_log])
        .await
        .unwrap();
}

/// Emulates contract deployment in the genesis block.
async fn deploy_contract(
    storage: &mut Connection<'_, Core>,
    bytecode: Vec<u8>,
    address: Address,
    state: &[(H256, H256)],
) {
    let bytecode_hash = hash_bytecode(&bytecode);
    let storage_logs = [
        StorageLog::new_write_log(get_code_key(&address), bytecode_hash),
        StorageLog::new_write_log(get_known_code_key(&bytecode_hash), H256::from_low_u64_be(1)),
    ];
    let contract_state_logs = state.iter().map(|&(key, value)| {
        let key = StorageKey::new(AccountTreeId::new(address), key);
        StorageLog::new_write_log(key, value)
    });
    let storage_logs: Vec<_> = storage_logs
        .into_iter()
        .chain(contract_state_logs)
        .collect();

    storage
        .storage_logs_dal()
        .append_storage_logs(L2BlockNumber(0), &storage_logs)
        .await
        .unwrap();
    let factory_deps = HashMap::from([(bytecode_hash, bytecode)]);
    storage
        .factory_deps_dal()
        .insert_factory_deps(L2BlockNumber(0), &factory_deps)
        .await
        .unwrap();
}

#[tokio::test]
async fn submitting_tx_requires_one_connection() {
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();

    let l2_chain_id = L2ChainId::default();
    let fee_input = MockBatchFeeParamsProvider::default()
        .get_batch_fee_input_scaled(1.0, 1.0)
        .await
        .unwrap();
    let (base_fee, gas_per_pubdata) =
        derive_base_fee_and_gas_per_pubdata(fee_input, ProtocolVersionId::latest().into());
    let tx = create_l2_transaction(base_fee, gas_per_pubdata);
    let tx_hash = tx.hash();

    // Manually set sufficient balance for the tx initiator.
    fund_account(&mut storage, tx.initiator_account()).await;
    drop(storage);

    let mut tx_executor = MockOneshotExecutor::default();
    tx_executor.set_tx_responses(move |received_tx, _| {
        assert_eq!(received_tx.hash(), tx_hash);
        ExecutionResult::Success { output: vec![] }
    });
    let tx_executor = SandboxExecutor::mock(tx_executor).await;
    let (tx_sender, _) = create_test_tx_sender(pool.clone(), l2_chain_id, tx_executor).await;

    let submission_result = tx_sender.submit_tx(tx).await.unwrap();
    assert_matches!(submission_result.0, L2TxSubmissionResult::Added);

    let mut storage = pool.connection().await.unwrap();
    storage
        .transactions_web3_dal()
        .get_transaction_by_hash(tx_hash, l2_chain_id)
        .await
        .unwrap()
        .expect("transaction is not persisted");
}

#[tokio::test]
async fn sending_transfer() {
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tx_sender = create_real_tx_sender(pool).await;
    let alice = K256PrivateKey::random();

    // Manually set sufficient balance for the tx initiator.
    let mut storage = tx_sender
        .0
        .replica_connection_pool
        .connection()
        .await
        .unwrap();
    fund_account(&mut storage, alice.address()).await;
    drop(storage);

    let transfer = alice.create_transfer(1_000_000_000.into());
    let (sub_result, vm_result) = tx_sender.submit_tx(transfer).await.unwrap();
    assert_matches!(sub_result, L2TxSubmissionResult::Added);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);
}

#[tokio::test]
async fn sending_transfer_with_insufficient_balance() {
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tx_sender = create_real_tx_sender(pool).await;
    let alice = K256PrivateKey::random();
    let transfer_value = 1_000_000_000.into();

    let transfer = alice.create_transfer(transfer_value);
    let err = tx_sender.submit_tx(transfer).await.unwrap_err();
    assert_matches!(
        err,
        SubmitTxError::NotEnoughBalanceForFeeValue(balance, _, value) if balance.is_zero()
            && value == transfer_value
    );
}

#[tokio::test]
async fn sending_transfer_with_incorrect_signature() {
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tx_sender = create_real_tx_sender(pool).await;
    let alice = K256PrivateKey::random();
    let transfer_value = 1_000_000_000.into();

    let mut storage = tx_sender
        .0
        .replica_connection_pool
        .connection()
        .await
        .unwrap();
    fund_account(&mut storage, alice.address()).await;
    drop(storage);

    let mut transfer = alice.create_transfer(transfer_value);
    transfer.execute.value = transfer_value / 2; // This should invalidate tx signature
    let err = tx_sender.submit_tx(transfer).await.unwrap_err();
    assert_matches!(err, SubmitTxError::ValidationFailed(_));
}

#[test_casing(5, LOAD_TEST_CASES)]
#[tokio::test]
async fn sending_load_test_transaction(tx_params: LoadnextContractExecutionParams) {
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tx_sender = create_real_tx_sender(pool).await;
    let alice = K256PrivateKey::random();

    let mut storage = tx_sender
        .0
        .replica_connection_pool
        .connection()
        .await
        .unwrap();
    deploy_contract(
        &mut storage,
        get_loadnext_contract().bytecode,
        LOAD_TEST_ADDRESS,
        &[(H256::zero(), H256::from_low_u64_be(100))],
    )
    .await;
    fund_account(&mut storage, alice.address()).await;
    drop(storage);

    let tx = alice.create_load_test_tx(tx_params);
    let (sub_result, vm_result) = tx_sender.submit_tx(tx).await.unwrap();
    assert_matches!(sub_result, L2TxSubmissionResult::Added);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);
}

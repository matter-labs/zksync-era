//! Tests for sending raw transactions.

use std::ops::Range;

use assert_matches::assert_matches;
use chrono::NaiveDateTime;
use test_casing::test_casing;
use zksync_multivm::interface::{tracer::ValidationTraces, ExecutionResult};
use zksync_node_fee_model::{BatchFeeModelInputProvider, MockBatchFeeParamsProvider};
use zksync_node_test_utils::create_l2_transaction;
use zksync_test_contracts::Account;

use super::*;
use crate::testonly::{StateBuilder, TestAccount};

#[tokio::test]
async fn submitting_tx_requires_one_connection() {
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();

    let l2_chain_id = L2ChainId::default();
    let fee_params_provider: &dyn BatchFeeModelInputProvider =
        &MockBatchFeeParamsProvider::default();
    let fee_input = fee_params_provider.get_batch_fee_input().await.unwrap();
    let (base_fee, gas_per_pubdata) =
        derive_base_fee_and_gas_per_pubdata(fee_input, ProtocolVersionId::latest().into());
    let tx = create_l2_transaction(base_fee, gas_per_pubdata);
    let tx_hash = tx.hash();

    // Manually set sufficient balance for the tx initiator.
    StateBuilder::default()
        .with_balance(tx.initiator_account(), u64::MAX.into())
        .apply(storage)
        .await;

    let mut tx_executor = MockOneshotExecutor::default();
    tx_executor.set_tx_responses(move |received_tx, _| {
        assert_eq!(received_tx.hash(), tx_hash);
        ExecutionResult::Success { output: vec![] }
    });
    let tx_executor = SandboxExecutor::mock(tx_executor).await;
    let (tx_sender, _) = create_test_tx_sender(pool.clone(), l2_chain_id, tx_executor).await;
    let block_args = pending_block_args(&tx_sender).await;

    tx_sender.submit_tx(tx, block_args).await.unwrap();

    let mut storage = pool.connection().await.unwrap();
    storage
        .transactions_web3_dal()
        .get_transaction_by_hash(tx_hash, l2_chain_id)
        .await
        .unwrap()
        .expect("transaction is not persisted");

    let storage_tx = storage
        .transactions_dal()
        .get_storage_tx_by_hash(tx_hash)
        .await
        .unwrap()
        .expect("transaction is not persisted");
    // verify that no validation traces have been persisted
    assert!(storage_tx.timestamp_asserter_range_start.is_none());
    assert!(storage_tx.timestamp_asserter_range_start.is_none());
}

#[tokio::test]
async fn nonce_validation_errors() {
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();
    drop(storage);

    let l2_chain_id = L2ChainId::default();
    let tx_executor = SandboxExecutor::mock(MockOneshotExecutor::default()).await;
    let (tx_sender, _) = create_test_tx_sender(pool.clone(), l2_chain_id, tx_executor).await;
    let mut tx = create_l2_transaction(55, 555);

    tx_sender.validate_account_nonce(&tx).await.unwrap();
    // There should be some leeway with the nonce validation.
    tx.common_data.nonce = Nonce(1);
    tx_sender.validate_account_nonce(&tx).await.unwrap();

    tx.common_data.nonce = Nonce(10_000);
    let err = tx_sender.validate_account_nonce(&tx).await.unwrap_err();
    assert_matches!(
        err,
        SubmitTxError::NonceIsTooHigh(from, _, actual) if actual == 10_000 && from == 0
    );

    let mut storage = pool.connection().await.unwrap();
    let nonce_key = get_nonce_key(&tx.initiator_account());
    let nonce_log = StorageLog::new_write_log(nonce_key, H256::from_low_u64_be(42));
    storage
        .storage_logs_dal()
        .append_storage_logs(L2BlockNumber(0), &[nonce_log])
        .await
        .unwrap();
    drop(storage);

    let err = tx_sender.validate_account_nonce(&tx).await.unwrap_err();
    assert_matches!(
        err,
        SubmitTxError::NonceIsTooHigh(from, _, actual) if actual == 10_000 && from == 42
    );

    tx.common_data.nonce = Nonce(5);
    let err = tx_sender.validate_account_nonce(&tx).await.unwrap_err();
    assert_matches!(
        err,
        SubmitTxError::NonceIsTooLow(from, _, actual) if actual == 5 && from == 42
    );
}

#[tokio::test]
async fn fee_validation_errors() {
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();

    let l2_chain_id = L2ChainId::default();
    let tx_executor = SandboxExecutor::mock(MockOneshotExecutor::default()).await;
    let (tx_sender, _) = create_test_tx_sender(pool.clone(), l2_chain_id, tx_executor).await;
    let fee_params_provider: &dyn BatchFeeModelInputProvider =
        &MockBatchFeeParamsProvider::default();
    let fee_input = fee_params_provider.get_batch_fee_input().await.unwrap();
    let (base_fee, gas_per_pubdata) =
        derive_base_fee_and_gas_per_pubdata(fee_input, ProtocolVersionId::latest().into());
    let tx = create_l2_transaction(base_fee, gas_per_pubdata);

    StateBuilder::default()
        .with_balance(tx.initiator_account(), u64::MAX.into())
        .apply(storage)
        .await;

    // Sanity check: validation should succeed with reasonable fee params.
    tx_sender
        .validate_tx(&tx, ProtocolVersionId::latest())
        .await
        .unwrap();

    {
        let mut tx = tx.clone();
        tx.common_data.fee.gas_limit = 100.into();
        let err = tx_sender
            .validate_tx(&tx, ProtocolVersionId::latest())
            .await
            .unwrap_err();
        assert_matches!(err, SubmitTxError::IntrinsicGas);
    }
    {
        let mut tx = tx.clone();
        tx.common_data.fee.gas_limit = u64::MAX.into();
        let err = tx_sender
            .validate_tx(&tx, ProtocolVersionId::latest())
            .await
            .unwrap_err();
        assert_matches!(err, SubmitTxError::GasLimitIsTooBig);
    }
    {
        let mut tx = tx.clone();
        tx.common_data.fee.max_fee_per_gas = 1.into();
        let err = tx_sender
            .validate_tx(&tx, ProtocolVersionId::latest())
            .await
            .unwrap_err();
        assert_matches!(err, SubmitTxError::MaxFeePerGasTooLow);
    }
    {
        let mut tx = tx.clone();
        tx.common_data.fee.max_priority_fee_per_gas = tx.common_data.fee.max_fee_per_gas * 2;
        let err = tx_sender
            .validate_tx(&tx, ProtocolVersionId::latest())
            .await
            .unwrap_err();
        assert_matches!(err, SubmitTxError::MaxPriorityFeeGreaterThanMaxFee);
    }
}

#[tokio::test]
async fn sending_transfer() {
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tx_sender = create_real_tx_sender(pool).await;
    let block_args = pending_block_args(&tx_sender).await;
    let mut alice = Account::random();

    // Manually set sufficient balance for the tx initiator.
    let storage = tx_sender.acquire_replica_connection().await.unwrap();
    StateBuilder::default()
        .with_balance(alice.address(), u64::MAX.into())
        .apply(storage)
        .await;

    let transfer = alice.create_transfer(1_000_000_000.into());
    let vm_result = tx_sender.submit_tx(transfer, block_args).await.unwrap();
    assert!(!vm_result.result.is_failed(), "{vm_result:?}");
}

#[tokio::test]
async fn sending_transfer_with_insufficient_balance() {
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tx_sender = create_real_tx_sender(pool).await;
    let block_args = pending_block_args(&tx_sender).await;
    let mut alice = Account::random();
    let transfer_value = 1_000_000_000.into();

    let transfer = alice.create_transfer(transfer_value);
    let err = tx_sender.submit_tx(transfer, block_args).await.unwrap_err();
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
    let block_args = pending_block_args(&tx_sender).await;
    let mut alice = Account::random();
    let transfer_value = 1_000_000_000.into();

    let storage = tx_sender.acquire_replica_connection().await.unwrap();
    StateBuilder::default()
        .with_balance(alice.address(), u64::MAX.into())
        .apply(storage)
        .await;

    let mut transfer = alice.create_transfer(transfer_value);
    transfer.execute.value = transfer_value / 2; // This should invalidate tx signature
    let err = tx_sender.submit_tx(transfer, block_args).await.unwrap_err();
    assert_matches!(err, SubmitTxError::ValidationFailed(_));
}

#[test_casing(5, LOAD_TEST_CASES)]
#[tokio::test]
async fn sending_load_test_transaction(tx_params: LoadnextContractExecutionParams) {
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tx_sender = create_real_tx_sender(pool).await;
    let block_args = pending_block_args(&tx_sender).await;
    let mut alice = Account::random();

    let storage = tx_sender.acquire_replica_connection().await.unwrap();
    StateBuilder::default()
        .with_load_test_contract()
        .with_balance(alice.address(), u64::MAX.into())
        .apply(storage)
        .await;

    let tx = alice.create_load_test_tx(tx_params);
    let vm_result = tx_sender.submit_tx(tx, block_args).await.unwrap();
    assert!(!vm_result.result.is_failed(), "{vm_result:?}");
}

#[tokio::test]
async fn sending_reverting_transaction() {
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tx_sender = create_real_tx_sender(pool).await;
    let block_args = pending_block_args(&tx_sender).await;
    let mut alice = Account::random();

    let storage = tx_sender.acquire_replica_connection().await.unwrap();
    StateBuilder::default()
        .with_counter_contract(None)
        .with_balance(alice.address(), u64::MAX.into())
        .apply(storage)
        .await;

    let tx = alice.create_counter_tx(1.into(), true);
    let vm_result = tx_sender.submit_tx(tx, block_args).await.unwrap();
    assert_matches!(
        vm_result.result,
        ExecutionResult::Revert { output } if output.to_string().contains("This method always reverts")
    );
}

#[tokio::test]
async fn sending_transaction_out_of_gas() {
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tx_sender = create_real_tx_sender(pool).await;
    let block_args = pending_block_args(&tx_sender).await;
    let mut alice = Account::random();

    let storage = tx_sender.acquire_replica_connection().await.unwrap();
    StateBuilder::default()
        .with_infinite_loop_contract()
        .with_balance(alice.address(), u64::MAX.into())
        .apply(storage)
        .await;

    let tx = alice.create_infinite_loop_tx();
    let vm_result = tx_sender.submit_tx(tx, block_args).await.unwrap();
    assert_matches!(vm_result.result, ExecutionResult::Revert { .. });
}

async fn submit_tx_with_validation_traces(actual_range: Range<u64>, expected_range: Range<i64>) {
    // This test verifies that when a transaction produces ValidationTraces,
    // range_start and range_end get persisted in the database
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();

    let l2_chain_id = L2ChainId::default();
    let fee_params_provider: &dyn BatchFeeModelInputProvider =
        &MockBatchFeeParamsProvider::default();
    let fee_input = fee_params_provider.get_batch_fee_input().await.unwrap();
    let (base_fee, gas_per_pubdata) =
        derive_base_fee_and_gas_per_pubdata(fee_input, ProtocolVersionId::latest().into());
    let tx = create_l2_transaction(base_fee, gas_per_pubdata);
    let tx_hash = tx.hash();

    // Manually set sufficient balance for the tx initiator.
    StateBuilder::default()
        .with_balance(tx.initiator_account(), u64::MAX.into())
        .apply(storage)
        .await;

    let mut tx_executor = MockOneshotExecutor::default();
    tx_executor.set_tx_responses(move |received_tx, _| {
        assert_eq!(received_tx.hash(), tx_hash);
        ExecutionResult::Success { output: vec![] }
    });
    tx_executor.set_tx_validation_traces_responses(move |tx, _| {
        assert_eq!(tx.hash(), tx_hash);
        ValidationTraces {
            timestamp_asserter_range: Some(actual_range.clone()),
        }
    });

    let tx_executor = SandboxExecutor::mock(tx_executor).await;
    let (tx_sender, _) = create_test_tx_sender(pool.clone(), l2_chain_id, tx_executor).await;
    let block_args = pending_block_args(&tx_sender).await;

    tx_sender.submit_tx(tx, block_args).await.unwrap();

    let mut storage = pool.connection().await.unwrap();
    let storage_tx = storage
        .transactions_dal()
        .get_storage_tx_by_hash(tx_hash)
        .await
        .unwrap()
        .expect("transaction is not persisted");
    assert_eq!(
        expected_range.start,
        storage_tx
            .timestamp_asserter_range_start
            .unwrap()
            .and_utc()
            .timestamp()
    );
    assert_eq!(
        expected_range.end,
        storage_tx
            .timestamp_asserter_range_end
            .unwrap()
            .and_utc()
            .timestamp()
    );
}

#[tokio::test]
async fn submitting_tx_with_validation_traces() {
    // This test verifies that when a transaction produces ValidationTraces,
    // range_start and range_end get persisted in the database
    submit_tx_with_validation_traces(10..20, 10..20).await;
}

#[tokio::test]
async fn submitting_tx_with_validation_traces_resulting_into_overflow() {
    // This test verifies that the timestamp in ValidationTraces is capped at
    // the maximum value supported by the NaiveDateTime type
    submit_tx_with_validation_traces(10..u64::MAX, 10..NaiveDateTime::MAX.and_utc().timestamp())
        .await;
}

#[tokio::test]
async fn submitting_call_to_evm_contract() {
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tx_sender = create_real_tx_sender(pool).await;
    let block_args = pending_block_args(&tx_sender).await;
    let mut alice = Account::random();

    let storage = tx_sender.acquire_replica_connection().await.unwrap();
    StateBuilder::default()
        .with_balance(alice.address(), u64::MAX.into())
        .with_evm_counter_contract(None)
        .apply(storage)
        .await;

    let tx = alice.create_counter_tx(42.into(), false);
    let vm_result = tx_sender.submit_tx(tx, block_args).await.unwrap();
    assert_matches!(&vm_result.result, ExecutionResult::Success { .. });
}

#[tokio::test]
async fn submitting_evm_deployment() {
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let tx_sender = create_real_tx_sender(pool).await;
    let block_args = pending_block_args(&tx_sender).await;
    let mut alice = Account::random();

    let storage = tx_sender.acquire_replica_connection().await.unwrap();
    StateBuilder::default()
        .with_balance(alice.address(), u64::MAX.into())
        .enable_evm_deployments()
        .apply(storage)
        .await;

    let tx = alice.create_evm_counter_deployment(42.into());
    let vm_result = tx_sender.submit_tx(tx, block_args).await.unwrap();
    assert_matches!(&vm_result.result, ExecutionResult::Success { .. });
}

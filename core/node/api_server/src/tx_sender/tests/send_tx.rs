//! Tests for sending raw transactions.

use assert_matches::assert_matches;
use test_casing::test_casing;
use zksync_node_fee_model::MockBatchFeeParamsProvider;
use zksync_types::K256PrivateKey;

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
    let fee_input = MockBatchFeeParamsProvider::default()
        .get_batch_fee_input_scaled(1.0, 1.0)
        .await
        .unwrap();
    let (base_fee, gas_per_pubdata) =
        derive_base_fee_and_gas_per_pubdata(fee_input, ProtocolVersionId::latest().into());
    let tx = create_l2_transaction(base_fee, gas_per_pubdata);
    let tx_hash = tx.hash();

    // Manually set sufficient balance for the tx initiator.
    StateBuilder::default()
        .with_balance(tx.initiator_account(), u64::MAX.into())
        .apply(&mut storage)
        .await;
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
    StateBuilder::default()
        .with_balance(alice.address(), u64::MAX.into())
        .apply(&mut storage)
        .await;
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
    StateBuilder::default()
        .with_balance(alice.address(), u64::MAX.into())
        .apply(&mut storage)
        .await;
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
    StateBuilder::default()
        .with_load_test_contract()
        .with_balance(alice.address(), u64::MAX.into())
        .apply(&mut storage)
        .await;
    drop(storage);

    let tx = alice.create_load_test_tx(tx_params);
    let (sub_result, vm_result) = tx_sender.submit_tx(tx).await.unwrap();
    assert_matches!(sub_result, L2TxSubmissionResult::Added);
    assert!(!vm_result.result.is_failed(), "{:?}", vm_result.result);
}

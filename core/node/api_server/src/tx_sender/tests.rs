//! Tests for the transaction sender.

use std::time::Duration;

use assert_matches::assert_matches;
use zksync_multivm::interface::ExecutionResult;
use zksync_node_fee_model::MockBatchFeeParamsProvider;
use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
use zksync_node_test_utils::{create_l2_block, create_l2_transaction, prepare_recovery_snapshot};
use zksync_types::{api, get_nonce_key, L1BatchNumber, L2BlockNumber, StorageLog};
use zksync_utils::u256_to_h256;

use super::*;
use crate::{
    execution_sandbox::{testonly::MockOneshotExecutor, BlockStartInfo},
    web3::testonly::create_test_tx_sender,
};

#[tokio::test]
async fn getting_nonce_for_account() {
    let l2_chain_id = L2ChainId::default();
    let test_address = Address::repeat_byte(1);
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();
    // Manually insert a nonce for the address.
    let nonce_key = get_nonce_key(&test_address);
    let nonce_log = StorageLog::new_write_log(nonce_key, H256::from_low_u64_be(123));
    storage
        .storage_logs_dal()
        .append_storage_logs(L2BlockNumber(0), &[nonce_log])
        .await
        .unwrap();

    let tx_executor = MockOneshotExecutor::default().into();
    let (tx_sender, _) = create_test_tx_sender(pool.clone(), l2_chain_id, tx_executor).await;

    let nonce = tx_sender.get_expected_nonce(test_address).await.unwrap();
    assert_eq!(nonce, Nonce(123));

    // Insert another L2 block with a new nonce log.
    storage
        .blocks_dal()
        .insert_l2_block(&create_l2_block(1))
        .await
        .unwrap();
    let nonce_log = StorageLog {
        value: H256::from_low_u64_be(321),
        ..nonce_log
    };
    storage
        .storage_logs_dal()
        .insert_storage_logs(L2BlockNumber(1), &[nonce_log])
        .await
        .unwrap();

    let nonce = tx_sender.get_expected_nonce(test_address).await.unwrap();
    assert_eq!(nonce, Nonce(321));
    let missing_address = Address::repeat_byte(0xff);
    let nonce = tx_sender.get_expected_nonce(missing_address).await.unwrap();
    assert_eq!(nonce, Nonce(0));
}

#[tokio::test]
async fn getting_nonce_for_account_after_snapshot_recovery() {
    const SNAPSHOT_L2_BLOCK_NUMBER: L2BlockNumber = L2BlockNumber(42);

    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let test_address = Address::repeat_byte(1);
    let other_address = Address::repeat_byte(2);
    let nonce_logs = [
        StorageLog::new_write_log(get_nonce_key(&test_address), H256::from_low_u64_be(123)),
        StorageLog::new_write_log(get_nonce_key(&other_address), H256::from_low_u64_be(25)),
    ];
    prepare_recovery_snapshot(
        &mut storage,
        L1BatchNumber(23),
        SNAPSHOT_L2_BLOCK_NUMBER,
        &nonce_logs,
    )
    .await;

    let l2_chain_id = L2ChainId::default();
    let tx_executor = MockOneshotExecutor::default().into();
    let (tx_sender, _) = create_test_tx_sender(pool.clone(), l2_chain_id, tx_executor).await;

    storage
        .blocks_dal()
        .insert_l2_block(&create_l2_block(SNAPSHOT_L2_BLOCK_NUMBER.0 + 1))
        .await
        .unwrap();
    let new_nonce_logs = vec![StorageLog::new_write_log(
        get_nonce_key(&test_address),
        H256::from_low_u64_be(321),
    )];
    storage
        .storage_logs_dal()
        .insert_storage_logs(SNAPSHOT_L2_BLOCK_NUMBER + 1, &new_nonce_logs)
        .await
        .unwrap();

    let nonce = tx_sender.get_expected_nonce(test_address).await.unwrap();
    assert_eq!(nonce, Nonce(321));
    let nonce = tx_sender.get_expected_nonce(other_address).await.unwrap();
    assert_eq!(nonce, Nonce(25));
    let missing_address = Address::repeat_byte(0xff);
    let nonce = tx_sender.get_expected_nonce(missing_address).await.unwrap();
    assert_eq!(nonce, Nonce(0));
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
    let balance_key = storage_key_for_eth_balance(&tx.initiator_account());
    let storage_log = StorageLog::new_write_log(balance_key, u256_to_h256(U256::one() << 64));
    storage
        .storage_logs_dal()
        .append_storage_logs(L2BlockNumber(0), &[storage_log])
        .await
        .unwrap();
    drop(storage);

    let mut tx_executor = MockOneshotExecutor::default();
    tx_executor.set_tx_responses(move |received_tx, _| {
        assert_eq!(received_tx.hash(), tx_hash);
        ExecutionResult::Success { output: vec![] }
    });
    let tx_executor = tx_executor.into();
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
async fn eth_call_requires_single_connection() {
    let pool = ConnectionPool::<Core>::constrained_test_pool(1).await;
    let mut storage = pool.connection().await.unwrap();
    let genesis_params = GenesisParams::mock();
    insert_genesis_batch(&mut storage, &genesis_params)
        .await
        .unwrap();
    let start_info = BlockStartInfo::new(&mut storage, Duration::MAX)
        .await
        .unwrap();
    let block_id = api::BlockId::Number(api::BlockNumber::Latest);
    let block_args = BlockArgs::new(&mut storage, block_id, &start_info)
        .await
        .unwrap();
    drop(storage);

    let tx = create_l2_transaction(10, 100);
    let tx_hash = tx.hash();

    let mut tx_executor = MockOneshotExecutor::default();
    tx_executor.set_call_responses(move |received_tx, _| {
        assert_eq!(received_tx.hash(), tx_hash);
        ExecutionResult::Success {
            output: b"success!".to_vec(),
        }
    });
    let tx_executor = tx_executor.into();
    let (tx_sender, _) = create_test_tx_sender(
        pool.clone(),
        genesis_params.config().l2_chain_id,
        tx_executor,
    )
    .await;
    let call_overrides = CallOverrides {
        enforced_base_fee: None,
    };
    let output = tx_sender
        .eth_call(block_args, call_overrides, tx, None)
        .await
        .unwrap();
    assert_eq!(output, b"success!");
}

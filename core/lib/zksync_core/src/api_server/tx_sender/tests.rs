//! Tests for the transaction sender.

use zksync_types::{get_nonce_key, L1BatchNumber, StorageLog};

use super::*;
use crate::{
    api_server::execution_sandbox::{testonly::MockTransactionExecutor, VmConcurrencyBarrier},
    genesis::{insert_genesis_batch, GenesisParams},
    utils::testonly::{create_miniblock, prepare_recovery_snapshot, MockBatchFeeParamsProvider},
};

pub(crate) async fn create_test_tx_sender(
    pool: ConnectionPool<Core>,
    l2_chain_id: L2ChainId,
    tx_executor: TransactionExecutor,
) -> (TxSender, VmConcurrencyBarrier) {
    let web3_config = Web3JsonRpcConfig::for_tests();
    let state_keeper_config = StateKeeperConfig::for_tests();
    let tx_sender_config = TxSenderConfig::new(&state_keeper_config, &web3_config, l2_chain_id);

    let storage_caches = PostgresStorageCaches::new(1, 1);
    let batch_fee_model_input_provider = Arc::new(MockBatchFeeParamsProvider::default());
    let (mut tx_sender, vm_barrier) = crate::build_tx_sender(
        &tx_sender_config,
        &web3_config,
        &state_keeper_config,
        pool.clone(),
        pool,
        batch_fee_model_input_provider,
        storage_caches,
    )
    .await;

    Arc::get_mut(&mut tx_sender.0).unwrap().executor = tx_executor;
    (tx_sender, vm_barrier)
}

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
        .append_storage_logs(MiniblockNumber(0), &[(H256::default(), vec![nonce_log])])
        .await
        .unwrap();

    let tx_executor = MockTransactionExecutor::default().into();
    let (tx_sender, _) = create_test_tx_sender(pool.clone(), l2_chain_id, tx_executor).await;

    let nonce = tx_sender.get_expected_nonce(test_address).await.unwrap();
    assert_eq!(nonce, Nonce(123));

    // Insert another miniblock with a new nonce log.
    storage
        .blocks_dal()
        .insert_miniblock(&create_miniblock(1))
        .await
        .unwrap();
    let nonce_log = StorageLog {
        value: H256::from_low_u64_be(321),
        ..nonce_log
    };
    storage
        .storage_logs_dal()
        .insert_storage_logs(MiniblockNumber(1), &[(H256::default(), vec![nonce_log])])
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
    const SNAPSHOT_MINIBLOCK_NUMBER: MiniblockNumber = MiniblockNumber(42);

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
        SNAPSHOT_MINIBLOCK_NUMBER,
        &nonce_logs,
    )
    .await;

    let l2_chain_id = L2ChainId::default();
    let tx_executor = MockTransactionExecutor::default().into();
    let (tx_sender, _) = create_test_tx_sender(pool.clone(), l2_chain_id, tx_executor).await;

    let nonce = tx_sender.get_expected_nonce(test_address).await.unwrap();
    assert_eq!(nonce, Nonce(123));
    let nonce = tx_sender.get_expected_nonce(other_address).await.unwrap();
    assert_eq!(nonce, Nonce(25));
    let missing_address = Address::repeat_byte(0xff);
    let nonce = tx_sender.get_expected_nonce(missing_address).await.unwrap();
    assert_eq!(nonce, Nonce(0));

    storage
        .blocks_dal()
        .insert_miniblock(&create_miniblock(SNAPSHOT_MINIBLOCK_NUMBER.0 + 1))
        .await
        .unwrap();
    let new_nonce_logs = vec![StorageLog::new_write_log(
        get_nonce_key(&test_address),
        H256::from_low_u64_be(321),
    )];
    storage
        .storage_logs_dal()
        .insert_storage_logs(
            SNAPSHOT_MINIBLOCK_NUMBER + 1,
            &[(H256::default(), new_nonce_logs)],
        )
        .await
        .unwrap();

    let nonce = tx_sender.get_expected_nonce(test_address).await.unwrap();
    assert_eq!(nonce, Nonce(321));
    let nonce = tx_sender.get_expected_nonce(other_address).await.unwrap();
    assert_eq!(nonce, Nonce(25));
    let nonce = tx_sender.get_expected_nonce(missing_address).await.unwrap();
    assert_eq!(nonce, Nonce(0));
}

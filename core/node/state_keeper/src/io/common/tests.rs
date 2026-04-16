//! Tests for the common I/O utils.
//!
//! `L1BatchParamsProvider` tests are (temporarily?) here because of `testonly` utils in this crate to create L1 batches,
//! L2 blocks, transactions etc.

use std::{collections::HashMap, ops};

use futures::FutureExt;
use zksync_config::GenesisConfig;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{ConnectionPool, Core};
use zksync_multivm::interface::{tracer::ValidationTraces, TransactionExecutionMetrics};
use zksync_node_genesis::{
    insert_genesis_batch, mock_genesis_config, GenesisParams, GenesisParamsInitials,
};
use zksync_node_test_utils::{
    create_l1_batch, create_l2_block, create_l2_transaction, execute_l2_transaction,
    prepare_recovery_snapshot,
};
use zksync_types::{
    block::L2BlockHasher, protocol_version::ProtocolSemanticVersion, L2ChainId, ProtocolVersion,
    ProtocolVersionId,
};
use zksync_vm_executor::storage::L1BatchParamsProvider;

use super::*;

#[test]
#[rustfmt::skip] // One-line formatting looks better here.
fn test_poll_iters() {
    assert_eq!(poll_iters(Duration::from_millis(100), Duration::from_millis(0)), 1);
    assert_eq!(poll_iters(Duration::from_millis(100), Duration::from_millis(100)), 1);
    assert_eq!(poll_iters(Duration::from_millis(100), Duration::from_millis(101)), 2);
    assert_eq!(poll_iters(Duration::from_millis(100), Duration::from_millis(200)), 2);
    assert_eq!(poll_iters(Duration::from_millis(100), Duration::from_millis(201)), 3);
}

#[tokio::test]
async fn creating_io_cursor_with_genesis() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParamsInitials::mock())
        .await
        .unwrap();

    let cursor = IoCursor::new(&mut storage).await.unwrap();
    assert_eq!(cursor.l1_batch, L1BatchNumber(1));
    assert_eq!(cursor.next_l2_block, L2BlockNumber(1));
    assert_eq!(cursor.prev_l2_block_timestamp, 0);
    assert_eq!(cursor.prev_l1_batch_timestamp, 0);
    assert_eq!(
        cursor.prev_l2_block_hash,
        L2BlockHasher::legacy_hash(L2BlockNumber(0))
    );

    let l2_block = create_l2_block(1);
    storage
        .blocks_dal()
        .insert_l2_block(&l2_block)
        .await
        .unwrap();

    let cursor = IoCursor::new(&mut storage).await.unwrap();
    assert_eq!(cursor.l1_batch, L1BatchNumber(1));
    assert_eq!(cursor.next_l2_block, L2BlockNumber(2));
    assert_eq!(cursor.prev_l2_block_timestamp, l2_block.timestamp);
    assert_eq!(cursor.prev_l2_block_hash, l2_block.hash);
}

#[tokio::test]
async fn creating_io_cursor_with_snapshot_recovery() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let snapshot_recovery =
        prepare_recovery_snapshot(&mut storage, L1BatchNumber(23), L2BlockNumber(42), &[]).await;

    let cursor = IoCursor::new(&mut storage).await.unwrap();
    assert_eq!(cursor.l1_batch, L1BatchNumber(24));
    assert_eq!(cursor.next_l2_block, snapshot_recovery.l2_block_number + 1);
    assert_eq!(
        cursor.prev_l2_block_timestamp,
        snapshot_recovery.l2_block_timestamp
    );
    assert_eq!(
        cursor.prev_l1_batch_timestamp,
        snapshot_recovery.l1_batch_timestamp
    );
    assert_eq!(cursor.prev_l2_block_hash, snapshot_recovery.l2_block_hash);

    // Add an L2 block so that we have L2 blocks (but not an L1 batch) in the storage.
    let l2_block = create_l2_block(snapshot_recovery.l2_block_number.0 + 1);
    storage
        .blocks_dal()
        .insert_l2_block(&l2_block)
        .await
        .unwrap();

    let cursor = IoCursor::new(&mut storage).await.unwrap();
    assert_eq!(cursor.l1_batch, L1BatchNumber(24));
    assert_eq!(cursor.next_l2_block, l2_block.number + 1);
    assert_eq!(cursor.prev_l2_block_timestamp, l2_block.timestamp);
    assert_eq!(cursor.prev_l2_block_hash, l2_block.hash);
}

#[tokio::test]
async fn waiting_for_l1_batch_params_with_genesis() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let genesis_batch = insert_genesis_batch(&mut storage, &GenesisParamsInitials::mock())
        .await
        .unwrap();

    let provider = L1BatchParamsProvider::new(&mut storage).await.unwrap();
    let (hash, timestamp) = provider
        .wait_for_l1_batch_params(&mut storage, L1BatchNumber(0))
        .await
        .unwrap();
    assert_eq!(hash, genesis_batch.root_hash);
    assert_eq!(timestamp, 0);

    let new_l1_batch = create_l1_batch(1);
    storage
        .blocks_dal()
        .insert_mock_l1_batch(&new_l1_batch)
        .await
        .unwrap();

    let wait_future = provider.wait_for_l1_batch_params(&mut storage, L1BatchNumber(1));
    futures::pin_mut!(wait_future);
    tokio::task::yield_now().await;
    assert!((&mut wait_future).now_or_never().is_none());

    let expected_hash = H256::repeat_byte(1);
    let mut storage = pool.connection().await.unwrap();
    storage
        .blocks_dal()
        .set_l1_batch_hash(L1BatchNumber(1), expected_hash)
        .await
        .unwrap();
    let (hash, timestamp) = wait_future.await.unwrap();
    assert_eq!(hash, expected_hash);
    assert_eq!(timestamp, new_l1_batch.timestamp);
}

#[tokio::test]
async fn waiting_for_l1_batch_params_after_snapshot_recovery() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let snapshot_recovery =
        prepare_recovery_snapshot(&mut storage, L1BatchNumber(23), L2BlockNumber(42), &[]).await;

    let provider = L1BatchParamsProvider::new(&mut storage).await.unwrap();
    let (hash, timestamp) = provider
        .wait_for_l1_batch_params(&mut storage, snapshot_recovery.l1_batch_number)
        .await
        .unwrap();
    assert_eq!(hash, snapshot_recovery.l1_batch_root_hash);
    assert_eq!(timestamp, snapshot_recovery.l1_batch_timestamp);

    for pruned_l1_batch in [0, 1, snapshot_recovery.l1_batch_number.0 - 1] {
        assert!(provider
            .wait_for_l1_batch_params(&mut storage, L1BatchNumber(pruned_l1_batch))
            .await
            .is_err());
    }

    let new_l1_batch = create_l1_batch(snapshot_recovery.l1_batch_number.0 + 1);
    storage
        .blocks_dal()
        .insert_mock_l1_batch(&new_l1_batch)
        .await
        .unwrap();

    let wait_future =
        provider.wait_for_l1_batch_params(&mut storage, snapshot_recovery.l1_batch_number + 1);
    futures::pin_mut!(wait_future);
    tokio::task::yield_now().await;
    assert!((&mut wait_future).now_or_never().is_none());

    let expected_hash = H256::repeat_byte(1);
    let mut storage = pool.connection().await.unwrap();
    storage
        .blocks_dal()
        .set_l1_batch_hash(new_l1_batch.number, expected_hash)
        .await
        .unwrap();
    let (hash, timestamp) = wait_future.await.unwrap();
    assert_eq!(hash, expected_hash);
    assert_eq!(timestamp, new_l1_batch.timestamp);
}

#[tokio::test]
async fn getting_first_l2_block_in_batch_with_genesis() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParamsInitials::mock())
        .await
        .unwrap();

    let provider = L1BatchParamsProvider::new(&mut storage).await.unwrap();
    let mut batches_and_l2_blocks = HashMap::from([
        (L1BatchNumber(0), Ok(Some(L2BlockNumber(0)))),
        (L1BatchNumber(1), Ok(Some(L2BlockNumber(1)))),
        (L1BatchNumber(2), Ok(None)),
        (L1BatchNumber(100), Ok(None)),
    ]);
    assert_first_l2_block_numbers(&provider, &mut storage, &batches_and_l2_blocks).await;

    let new_l2_block = create_l2_block(1);
    storage
        .blocks_dal()
        .insert_l2_block(&new_l2_block)
        .await
        .unwrap();
    let new_l2_block = create_l2_block(2);
    storage
        .blocks_dal()
        .insert_l2_block(&new_l2_block)
        .await
        .unwrap();

    assert_first_l2_block_numbers(&provider, &mut storage, &batches_and_l2_blocks).await;

    let new_l1_batch = create_l1_batch(1);
    storage
        .blocks_dal()
        .insert_mock_l1_batch(&new_l1_batch)
        .await
        .unwrap();
    storage
        .blocks_dal()
        .mark_l2_blocks_as_executed_in_l1_batch(new_l1_batch.number)
        .await
        .unwrap();

    batches_and_l2_blocks.insert(L1BatchNumber(2), Ok(Some(L2BlockNumber(3))));
    assert_first_l2_block_numbers(&provider, &mut storage, &batches_and_l2_blocks).await;
}

async fn assert_first_l2_block_numbers(
    provider: &L1BatchParamsProvider,
    storage: &mut Connection<'_, Core>,
    batches_and_l2_blocks: &HashMap<L1BatchNumber, Result<Option<L2BlockNumber>, ()>>,
) {
    for (&batch, &expected_l2_block) in batches_and_l2_blocks {
        let number = provider
            .load_number_of_first_l2_block_in_batch(storage, batch)
            .await;
        match expected_l2_block {
            Ok(expected) => {
                assert_eq!(
                    number.unwrap(),
                    expected,
                    "load_number_of_first_l2_block_in_batch({batch})"
                );
            }
            Err(()) => {
                number.unwrap_err();
            }
        }
    }
}

#[tokio::test]
async fn getting_first_l2_block_in_batch_after_snapshot_recovery() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let snapshot_recovery =
        prepare_recovery_snapshot(&mut storage, L1BatchNumber(23), L2BlockNumber(42), &[]).await;

    let provider = L1BatchParamsProvider::new(&mut storage).await.unwrap();
    let mut batches_and_l2_blocks = HashMap::from([
        (L1BatchNumber(1), Err(())),
        (snapshot_recovery.l1_batch_number, Err(())),
        (
            snapshot_recovery.l1_batch_number + 1,
            Ok(Some(snapshot_recovery.l2_block_number + 1)),
        ),
        (snapshot_recovery.l1_batch_number + 2, Ok(None)),
        (L1BatchNumber(100), Ok(None)),
    ]);
    assert_first_l2_block_numbers(&provider, &mut storage, &batches_and_l2_blocks).await;

    let new_l2_block = create_l2_block(snapshot_recovery.l2_block_number.0 + 1);
    storage
        .blocks_dal()
        .insert_l2_block(&new_l2_block)
        .await
        .unwrap();

    assert_first_l2_block_numbers(&provider, &mut storage, &batches_and_l2_blocks).await;

    let new_l1_batch = create_l1_batch(snapshot_recovery.l1_batch_number.0 + 1);
    storage
        .blocks_dal()
        .insert_mock_l1_batch(&new_l1_batch)
        .await
        .unwrap();
    storage
        .blocks_dal()
        .mark_l2_blocks_as_executed_in_l1_batch(new_l1_batch.number)
        .await
        .unwrap();

    batches_and_l2_blocks.insert(
        snapshot_recovery.l1_batch_number + 2,
        Ok(Some(new_l2_block.number + 1)),
    );
    assert_first_l2_block_numbers(&provider, &mut storage, &batches_and_l2_blocks).await;
}

#[tokio::test]
async fn loading_pending_batch_with_genesis() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let genesis_params = GenesisParamsInitials::mock();
    insert_genesis_batch(&mut storage, &genesis_params)
        .await
        .unwrap();
    store_pending_l2_blocks(
        &mut storage,
        1..=2,
        genesis_params.base_system_contracts.hashes(),
        L1BatchNumber(1),
    )
    .await;

    let provider = L1BatchParamsProvider::new(&mut storage).await.unwrap();
    let restored_l1_batch_env = provider
        .load_l1_batch_env(
            &mut storage,
            L1BatchNumber(1),
            u32::MAX,
            L2ChainId::default(),
        )
        .await
        .unwrap()
        .expect("no L1 batch");

    assert_eq!(restored_l1_batch_env.l1_batch_env.first_l2_block.number, 1);

    let pending_batch = load_pending_batch(&mut storage, restored_l1_batch_env)
        .await
        .unwrap();

    assert_eq!(pending_batch.pending_l2_blocks.len(), 2);
    assert_eq!(pending_batch.l1_batch_env.number, L1BatchNumber(1));
    assert_eq!(pending_batch.l1_batch_env.timestamp, 1);
    assert_eq!(pending_batch.l1_batch_env.first_l2_block.number, 1);
    assert_eq!(pending_batch.l1_batch_env.first_l2_block.timestamp, 1);
    assert_eq!(
        pending_batch.l1_batch_env.first_l2_block.prev_block_hash,
        L2BlockHasher::legacy_hash(L2BlockNumber(0))
    );
}

async fn store_pending_l2_blocks(
    storage: &mut Connection<'_, Core>,
    l2_block_numbers: ops::RangeInclusive<u32>,
    contract_hashes: BaseSystemContractsHashes,
    l1_batch_number: L1BatchNumber,
) {
    let mut l1_batch_header = create_l1_batch(l1_batch_number.0);
    l1_batch_header.timestamp = *l2_block_numbers.start() as u64; // Set timestamp to match the first L2 block.
    storage
        .blocks_dal()
        .insert_l1_batch(l1_batch_header.to_unsealed_header())
        .await
        .unwrap();
    for l2_block_number in l2_block_numbers {
        let tx = create_l2_transaction(10, 100);
        storage
            .transactions_dal()
            .insert_transaction_l2(
                &tx,
                TransactionExecutionMetrics::default(),
                ValidationTraces::default(),
            )
            .await
            .unwrap();
        let mut new_l2_block = create_l2_block(l2_block_number);
        new_l2_block.base_system_contracts_hashes = contract_hashes;
        storage
            .blocks_dal()
            .insert_l2_block(&new_l2_block)
            .await
            .unwrap();
        let tx_result = execute_l2_transaction(tx);
        storage
            .transactions_dal()
            .mark_txs_as_executed_in_l2_block(
                new_l2_block.number,
                &[tx_result],
                1.into(),
                ProtocolVersionId::latest(),
                false,
            )
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn loading_pending_batch_after_snapshot_recovery() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let snapshot_recovery =
        prepare_recovery_snapshot(&mut storage, L1BatchNumber(23), L2BlockNumber(42), &[]).await;

    let starting_l2_block_number = snapshot_recovery.l2_block_number.0 + 1;
    store_pending_l2_blocks(
        &mut storage,
        starting_l2_block_number..=starting_l2_block_number + 1,
        GenesisParams::mock().base_system_contracts().hashes(),
        snapshot_recovery.l1_batch_number + 1,
    )
    .await;

    let provider = L1BatchParamsProvider::new(&mut storage).await.unwrap();
    let restored_l1_batch_env = provider
        .load_l1_batch_env(
            &mut storage,
            snapshot_recovery.l1_batch_number + 1,
            u32::MAX,
            L2ChainId::default(),
        )
        .await
        .unwrap()
        .expect("no L1 batch");
    let pending_batch = load_pending_batch(&mut storage, restored_l1_batch_env)
        .await
        .unwrap();

    let expected_timestamp = u64::from(snapshot_recovery.l2_block_number.0) + 1;
    assert_eq!(pending_batch.pending_l2_blocks.len(), 2);
    assert_eq!(
        pending_batch.l1_batch_env.number,
        snapshot_recovery.l1_batch_number + 1
    );
    assert_eq!(pending_batch.l1_batch_env.timestamp, expected_timestamp);
    assert_eq!(
        pending_batch.l1_batch_env.first_l2_block.number,
        snapshot_recovery.l2_block_number.0 + 1
    );
    assert_eq!(
        pending_batch.l1_batch_env.first_l2_block.timestamp,
        expected_timestamp
    );
    assert_eq!(
        pending_batch.l1_batch_env.first_l2_block.prev_block_hash,
        snapshot_recovery.l2_block_hash
    );
}

#[tokio::test]
async fn getting_batch_version_with_genesis() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let genesis_params = GenesisParams::load_genesis_params(GenesisConfig {
        protocol_version: Some(ProtocolSemanticVersion {
            minor: ProtocolVersionId::Version5,
            patch: 0.into(),
        }),
        ..mock_genesis_config()
    })
    .unwrap();

    insert_genesis_batch(&mut storage, &genesis_params.clone().into())
        .await
        .unwrap();

    let provider = L1BatchParamsProvider::new(&mut storage).await.unwrap();
    let version = provider
        .load_l1_batch_protocol_version(&mut storage, L1BatchNumber(0))
        .await
        .unwrap();
    assert_eq!(version, Some(genesis_params.minor_protocol_version()));

    assert!(provider
        .load_l1_batch_protocol_version(&mut storage, L1BatchNumber(1))
        .await
        .unwrap()
        .is_none());

    storage
        .protocol_versions_dal()
        .save_protocol_version_with_tx(&ProtocolVersion::default())
        .await
        .unwrap();
    let new_l1_batch = create_l1_batch(1);
    storage
        .blocks_dal()
        .insert_mock_l1_batch(&new_l1_batch)
        .await
        .unwrap();

    let version = provider
        .load_l1_batch_protocol_version(&mut storage, L1BatchNumber(1))
        .await
        .unwrap();
    assert_eq!(version, new_l1_batch.protocol_version);
}

#[tokio::test]
async fn getting_batch_version_after_snapshot_recovery() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let snapshot_recovery =
        prepare_recovery_snapshot(&mut storage, L1BatchNumber(23), L2BlockNumber(42), &[]).await;

    let provider = L1BatchParamsProvider::new(&mut storage).await.unwrap();
    let version = provider
        .load_l1_batch_protocol_version(&mut storage, snapshot_recovery.l1_batch_number)
        .await
        .unwrap();
    assert_eq!(version, Some(snapshot_recovery.protocol_version));

    assert!(provider
        .load_l1_batch_protocol_version(&mut storage, L1BatchNumber(1))
        .await
        .is_err());
    assert!(provider
        .load_l1_batch_protocol_version(&mut storage, snapshot_recovery.l1_batch_number + 1)
        .await
        .unwrap()
        .is_none());

    storage
        .protocol_versions_dal()
        .save_protocol_version_with_tx(&ProtocolVersion {
            version: ProtocolSemanticVersion {
                minor: ProtocolVersionId::next(),
                patch: 0.into(),
            },
            ..ProtocolVersion::default()
        })
        .await
        .unwrap();
    let mut new_l1_batch = create_l1_batch(snapshot_recovery.l1_batch_number.0 + 1);
    new_l1_batch.protocol_version = Some(ProtocolVersionId::next());
    storage
        .blocks_dal()
        .insert_mock_l1_batch(&new_l1_batch)
        .await
        .unwrap();

    let version = provider
        .load_l1_batch_protocol_version(&mut storage, snapshot_recovery.l1_batch_number + 1)
        .await
        .unwrap();
    assert_eq!(version, new_l1_batch.protocol_version);
}

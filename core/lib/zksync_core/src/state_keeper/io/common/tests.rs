//! Tests for the common I/O utils.
//!
//! `L1BatchParamsProvider` tests are (temporarily?) here because of `testonly` utils in this crate to create L1 batches,
//! miniblocks, transactions etc.

use std::{collections::HashMap, ops};

use futures::FutureExt;
use vm_utils::storage::L1BatchParamsProvider;
use zksync_config::GenesisConfig;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{ConnectionPool, Core};
use zksync_types::{
    block::MiniblockHasher, fee::TransactionExecutionMetrics, L2ChainId, ProtocolVersion,
    ProtocolVersionId,
};

use super::*;
use crate::{
    genesis::{insert_genesis_batch, mock_genesis_config, GenesisParams},
    utils::testonly::{
        create_l1_batch, create_l2_transaction, create_miniblock, execute_l2_transaction,
        prepare_recovery_snapshot,
    },
};

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
    insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();

    let cursor = IoCursor::new(&mut storage).await.unwrap();
    assert_eq!(cursor.l1_batch, L1BatchNumber(1));
    assert_eq!(cursor.next_miniblock, MiniblockNumber(1));
    assert_eq!(cursor.prev_miniblock_timestamp, 0);
    assert_eq!(
        cursor.prev_miniblock_hash,
        MiniblockHasher::legacy_hash(MiniblockNumber(0))
    );

    let miniblock = create_miniblock(1);
    storage
        .blocks_dal()
        .insert_miniblock(&miniblock)
        .await
        .unwrap();

    let cursor = IoCursor::new(&mut storage).await.unwrap();
    assert_eq!(cursor.l1_batch, L1BatchNumber(1));
    assert_eq!(cursor.next_miniblock, MiniblockNumber(2));
    assert_eq!(cursor.prev_miniblock_timestamp, miniblock.timestamp);
    assert_eq!(cursor.prev_miniblock_hash, miniblock.hash);
}

#[tokio::test]
async fn creating_io_cursor_with_snapshot_recovery() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let snapshot_recovery =
        prepare_recovery_snapshot(&mut storage, L1BatchNumber(23), MiniblockNumber(42), &[]).await;

    let cursor = IoCursor::new(&mut storage).await.unwrap();
    assert_eq!(cursor.l1_batch, L1BatchNumber(24));
    assert_eq!(
        cursor.next_miniblock,
        snapshot_recovery.miniblock_number + 1
    );
    assert_eq!(
        cursor.prev_miniblock_timestamp,
        snapshot_recovery.miniblock_timestamp
    );
    assert_eq!(cursor.prev_miniblock_hash, snapshot_recovery.miniblock_hash);

    // Add a miniblock so that we have miniblocks (but not an L1 batch) in the storage.
    let miniblock = create_miniblock(snapshot_recovery.miniblock_number.0 + 1);
    storage
        .blocks_dal()
        .insert_miniblock(&miniblock)
        .await
        .unwrap();

    let cursor = IoCursor::new(&mut storage).await.unwrap();
    assert_eq!(cursor.l1_batch, L1BatchNumber(24));
    assert_eq!(cursor.next_miniblock, miniblock.number + 1);
    assert_eq!(cursor.prev_miniblock_timestamp, miniblock.timestamp);
    assert_eq!(cursor.prev_miniblock_hash, miniblock.hash);
}

#[tokio::test]
async fn waiting_for_l1_batch_params_with_genesis() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let genesis_batch = insert_genesis_batch(&mut storage, &GenesisParams::mock())
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
        prepare_recovery_snapshot(&mut storage, L1BatchNumber(23), MiniblockNumber(42), &[]).await;

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
async fn getting_first_miniblock_in_batch_with_genesis() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();

    let provider = L1BatchParamsProvider::new(&mut storage).await.unwrap();
    let mut batches_and_miniblocks = HashMap::from([
        (L1BatchNumber(0), Ok(Some(MiniblockNumber(0)))),
        (L1BatchNumber(1), Ok(Some(MiniblockNumber(1)))),
        (L1BatchNumber(2), Ok(None)),
        (L1BatchNumber(100), Ok(None)),
    ]);
    assert_first_miniblock_numbers(&provider, &mut storage, &batches_and_miniblocks).await;

    let new_miniblock = create_miniblock(1);
    storage
        .blocks_dal()
        .insert_miniblock(&new_miniblock)
        .await
        .unwrap();
    let new_miniblock = create_miniblock(2);
    storage
        .blocks_dal()
        .insert_miniblock(&new_miniblock)
        .await
        .unwrap();

    assert_first_miniblock_numbers(&provider, &mut storage, &batches_and_miniblocks).await;

    let new_l1_batch = create_l1_batch(1);
    storage
        .blocks_dal()
        .insert_mock_l1_batch(&new_l1_batch)
        .await
        .unwrap();
    storage
        .blocks_dal()
        .mark_miniblocks_as_executed_in_l1_batch(new_l1_batch.number)
        .await
        .unwrap();

    batches_and_miniblocks.insert(L1BatchNumber(2), Ok(Some(MiniblockNumber(3))));
    assert_first_miniblock_numbers(&provider, &mut storage, &batches_and_miniblocks).await;
}

async fn assert_first_miniblock_numbers(
    provider: &L1BatchParamsProvider,
    storage: &mut Connection<'_, Core>,
    batches_and_miniblocks: &HashMap<L1BatchNumber, Result<Option<MiniblockNumber>, ()>>,
) {
    for (&batch, &expected_miniblock) in batches_and_miniblocks {
        let number = provider
            .load_number_of_first_miniblock_in_batch(storage, batch)
            .await;
        match expected_miniblock {
            Ok(expected) => {
                assert_eq!(
                    number.unwrap(),
                    expected,
                    "load_number_of_first_miniblock_in_batch({batch})"
                );
            }
            Err(()) => {
                number.unwrap_err();
            }
        }
    }
}

#[tokio::test]
async fn getting_first_miniblock_in_batch_after_snapshot_recovery() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let snapshot_recovery =
        prepare_recovery_snapshot(&mut storage, L1BatchNumber(23), MiniblockNumber(42), &[]).await;

    let provider = L1BatchParamsProvider::new(&mut storage).await.unwrap();
    let mut batches_and_miniblocks = HashMap::from([
        (L1BatchNumber(1), Err(())),
        (snapshot_recovery.l1_batch_number, Err(())),
        (
            snapshot_recovery.l1_batch_number + 1,
            Ok(Some(snapshot_recovery.miniblock_number + 1)),
        ),
        (snapshot_recovery.l1_batch_number + 2, Ok(None)),
        (L1BatchNumber(100), Ok(None)),
    ]);
    assert_first_miniblock_numbers(&provider, &mut storage, &batches_and_miniblocks).await;

    let new_miniblock = create_miniblock(snapshot_recovery.miniblock_number.0 + 1);
    storage
        .blocks_dal()
        .insert_miniblock(&new_miniblock)
        .await
        .unwrap();

    assert_first_miniblock_numbers(&provider, &mut storage, &batches_and_miniblocks).await;

    let new_l1_batch = create_l1_batch(snapshot_recovery.l1_batch_number.0 + 1);
    storage
        .blocks_dal()
        .insert_mock_l1_batch(&new_l1_batch)
        .await
        .unwrap();
    storage
        .blocks_dal()
        .mark_miniblocks_as_executed_in_l1_batch(new_l1_batch.number)
        .await
        .unwrap();

    batches_and_miniblocks.insert(
        snapshot_recovery.l1_batch_number + 2,
        Ok(Some(new_miniblock.number + 1)),
    );
    assert_first_miniblock_numbers(&provider, &mut storage, &batches_and_miniblocks).await;
}

#[tokio::test]
async fn loading_pending_batch_with_genesis() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let genesis_params = GenesisParams::mock();
    insert_genesis_batch(&mut storage, &genesis_params)
        .await
        .unwrap();
    store_pending_miniblocks(
        &mut storage,
        1..=2,
        genesis_params.base_system_contracts().hashes(),
    )
    .await;

    let provider = L1BatchParamsProvider::new(&mut storage).await.unwrap();
    let first_miniblock_in_batch = provider
        .load_first_miniblock_in_batch(&mut storage, L1BatchNumber(1))
        .await
        .unwrap()
        .expect("no first miniblock");
    assert_eq!(first_miniblock_in_batch.number(), MiniblockNumber(1));

    let (system_env, l1_batch_env) = provider
        .load_l1_batch_params(
            &mut storage,
            &first_miniblock_in_batch,
            u32::MAX,
            L2ChainId::default(),
        )
        .await
        .unwrap();
    let pending_batch = load_pending_batch(&mut storage, system_env, l1_batch_env)
        .await
        .unwrap();

    assert_eq!(pending_batch.pending_miniblocks.len(), 2);
    assert_eq!(pending_batch.l1_batch_env.number, L1BatchNumber(1));
    assert_eq!(pending_batch.l1_batch_env.timestamp, 1);
    assert_eq!(pending_batch.l1_batch_env.first_l2_block.number, 1);
    assert_eq!(pending_batch.l1_batch_env.first_l2_block.timestamp, 1);
    assert_eq!(
        pending_batch.l1_batch_env.first_l2_block.prev_block_hash,
        MiniblockHasher::legacy_hash(MiniblockNumber(0))
    );
}

async fn store_pending_miniblocks(
    storage: &mut Connection<'_, Core>,
    numbers: ops::RangeInclusive<u32>,
    contract_hashes: BaseSystemContractsHashes,
) {
    for miniblock_number in numbers {
        let tx = create_l2_transaction(10, 100);
        storage
            .transactions_dal()
            .insert_transaction_l2(tx.clone(), TransactionExecutionMetrics::default())
            .await
            .unwrap();
        let mut new_miniblock = create_miniblock(miniblock_number);
        new_miniblock.base_system_contracts_hashes = contract_hashes;
        storage
            .blocks_dal()
            .insert_miniblock(&new_miniblock)
            .await
            .unwrap();
        let tx_result = execute_l2_transaction(tx);
        storage
            .transactions_dal()
            .mark_txs_as_executed_in_miniblock(new_miniblock.number, &[tx_result], 1.into())
            .await;
    }
}

#[tokio::test]
async fn loading_pending_batch_after_snapshot_recovery() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let snapshot_recovery =
        prepare_recovery_snapshot(&mut storage, L1BatchNumber(23), MiniblockNumber(42), &[]).await;

    let starting_miniblock_number = snapshot_recovery.miniblock_number.0 + 1;
    store_pending_miniblocks(
        &mut storage,
        starting_miniblock_number..=starting_miniblock_number + 1,
        GenesisParams::mock().base_system_contracts().hashes(),
    )
    .await;

    let provider = L1BatchParamsProvider::new(&mut storage).await.unwrap();
    let first_miniblock_in_batch = provider
        .load_first_miniblock_in_batch(&mut storage, snapshot_recovery.l1_batch_number + 1)
        .await
        .unwrap()
        .expect("no first miniblock");
    assert_eq!(
        first_miniblock_in_batch.number(),
        snapshot_recovery.miniblock_number + 1
    );

    let (system_env, l1_batch_env) = provider
        .load_l1_batch_params(
            &mut storage,
            &first_miniblock_in_batch,
            u32::MAX,
            L2ChainId::default(),
        )
        .await
        .unwrap();
    let pending_batch = load_pending_batch(&mut storage, system_env, l1_batch_env)
        .await
        .unwrap();

    let expected_timestamp = u64::from(snapshot_recovery.miniblock_number.0) + 1;
    assert_eq!(pending_batch.pending_miniblocks.len(), 2);
    assert_eq!(
        pending_batch.l1_batch_env.number,
        snapshot_recovery.l1_batch_number + 1
    );
    assert_eq!(pending_batch.l1_batch_env.timestamp, expected_timestamp);
    assert_eq!(
        pending_batch.l1_batch_env.first_l2_block.number,
        snapshot_recovery.miniblock_number.0 + 1
    );
    assert_eq!(
        pending_batch.l1_batch_env.first_l2_block.timestamp,
        expected_timestamp
    );
    assert_eq!(
        pending_batch.l1_batch_env.first_l2_block.prev_block_hash,
        snapshot_recovery.miniblock_hash
    );
}

#[tokio::test]
async fn getting_batch_version_with_genesis() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let genesis_params = GenesisParams::load_genesis_params(GenesisConfig {
        protocol_version: ProtocolVersionId::Version5 as u16,
        ..mock_genesis_config()
    })
    .unwrap();

    insert_genesis_batch(&mut storage, &genesis_params)
        .await
        .unwrap();

    let provider = L1BatchParamsProvider::new(&mut storage).await.unwrap();
    let version = provider
        .load_l1_batch_protocol_version(&mut storage, L1BatchNumber(0))
        .await
        .unwrap();
    assert_eq!(version, Some(genesis_params.protocol_version()));

    assert!(provider
        .load_l1_batch_protocol_version(&mut storage, L1BatchNumber(1))
        .await
        .unwrap()
        .is_none());

    storage
        .protocol_versions_dal()
        .save_protocol_version_with_tx(ProtocolVersion::default())
        .await;
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
        prepare_recovery_snapshot(&mut storage, L1BatchNumber(23), MiniblockNumber(42), &[]).await;

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
        .save_protocol_version_with_tx(ProtocolVersion {
            id: ProtocolVersionId::next(),
            ..ProtocolVersion::default()
        })
        .await;
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

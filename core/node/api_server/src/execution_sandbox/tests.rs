//! Tests for the VM execution sandbox.

use assert_matches::assert_matches;
use zksync_dal::ConnectionPool;
use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
use zksync_node_test_utils::{create_l2_block, create_l2_transaction, prepare_recovery_snapshot};

use super::*;
use crate::{execution_sandbox::apply::apply_vm_in_sandbox, tx_sender::ApiContracts};

#[tokio::test]
async fn creating_block_args() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();
    let l2_block = create_l2_block(1);
    storage
        .blocks_dal()
        .insert_l2_block(&l2_block)
        .await
        .unwrap();

    let pending_block_args = BlockArgs::pending(&mut storage).await.unwrap();
    assert_eq!(
        pending_block_args.block_id,
        api::BlockId::Number(api::BlockNumber::Pending)
    );
    assert_eq!(pending_block_args.resolved_block_number, L2BlockNumber(2));
    assert_eq!(pending_block_args.l1_batch_timestamp_s, None);

    let start_info = BlockStartInfo::new(&mut storage, Duration::MAX)
        .await
        .unwrap();
    assert_eq!(
        start_info.first_l2_block(&mut storage).await.unwrap(),
        L2BlockNumber(0)
    );
    assert_eq!(
        start_info.first_l1_batch(&mut storage).await.unwrap(),
        L1BatchNumber(0)
    );

    let latest_block = api::BlockId::Number(api::BlockNumber::Latest);
    let latest_block_args = BlockArgs::new(&mut storage, latest_block, &start_info)
        .await
        .unwrap();
    assert_eq!(latest_block_args.block_id, latest_block);
    assert_eq!(latest_block_args.resolved_block_number, L2BlockNumber(1));
    assert_eq!(
        latest_block_args.l1_batch_timestamp_s,
        Some(l2_block.timestamp)
    );

    let earliest_block = api::BlockId::Number(api::BlockNumber::Earliest);
    let earliest_block_args = BlockArgs::new(&mut storage, earliest_block, &start_info)
        .await
        .unwrap();
    assert_eq!(earliest_block_args.block_id, earliest_block);
    assert_eq!(earliest_block_args.resolved_block_number, L2BlockNumber(0));
    assert_eq!(earliest_block_args.l1_batch_timestamp_s, Some(0));

    let missing_block = api::BlockId::Number(100.into());
    let err = BlockArgs::new(&mut storage, missing_block, &start_info)
        .await
        .unwrap_err();
    assert_matches!(err, BlockArgsError::Missing);
}

#[tokio::test]
async fn creating_block_args_after_snapshot_recovery() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let snapshot_recovery =
        prepare_recovery_snapshot(&mut storage, L1BatchNumber(23), L2BlockNumber(42), &[]).await;

    let pending_block_args = BlockArgs::pending(&mut storage).await.unwrap();
    assert_eq!(
        pending_block_args.block_id,
        api::BlockId::Number(api::BlockNumber::Pending)
    );
    assert_eq!(
        pending_block_args.resolved_block_number,
        snapshot_recovery.l2_block_number + 1
    );
    assert_eq!(pending_block_args.l1_batch_timestamp_s, None);

    let start_info = BlockStartInfo::new(&mut storage, Duration::MAX)
        .await
        .unwrap();
    assert_eq!(
        start_info.first_l2_block(&mut storage).await.unwrap(),
        snapshot_recovery.l2_block_number + 1
    );
    assert_eq!(
        start_info.first_l1_batch(&mut storage).await.unwrap(),
        snapshot_recovery.l1_batch_number + 1
    );

    let latest_block = api::BlockId::Number(api::BlockNumber::Latest);
    let err = BlockArgs::new(&mut storage, latest_block, &start_info)
        .await
        .unwrap_err();
    assert_matches!(err, BlockArgsError::Missing);

    let pruned_blocks = [
        api::BlockNumber::Earliest,
        0.into(),
        snapshot_recovery.l2_block_number.0.into(),
    ];
    for pruned_block in pruned_blocks {
        let pruned_block = api::BlockId::Number(pruned_block);
        let err = BlockArgs::new(&mut storage, pruned_block, &start_info)
            .await
            .unwrap_err();
        assert_matches!(err, BlockArgsError::Pruned(_));
    }

    let missing_blocks = [
        api::BlockNumber::from(snapshot_recovery.l2_block_number.0 + 2),
        100.into(),
    ];
    for missing_block in missing_blocks {
        let missing_block = api::BlockId::Number(missing_block);
        let err = BlockArgs::new(&mut storage, missing_block, &start_info)
            .await
            .unwrap_err();
        assert_matches!(err, BlockArgsError::Missing);
    }

    let l2_block = create_l2_block(snapshot_recovery.l2_block_number.0 + 1);
    storage
        .blocks_dal()
        .insert_l2_block(&l2_block)
        .await
        .unwrap();

    let latest_block_args = BlockArgs::new(&mut storage, latest_block, &start_info)
        .await
        .unwrap();
    assert_eq!(latest_block_args.block_id, latest_block);
    assert_eq!(latest_block_args.resolved_block_number, l2_block.number);
    assert_eq!(
        latest_block_args.l1_batch_timestamp_s,
        Some(l2_block.timestamp)
    );

    for pruned_block in pruned_blocks {
        let pruned_block = api::BlockId::Number(pruned_block);
        let err = BlockArgs::new(&mut storage, pruned_block, &start_info)
            .await
            .unwrap_err();
        assert_matches!(err, BlockArgsError::Pruned(_));
    }
    for missing_block in missing_blocks {
        let missing_block = api::BlockId::Number(missing_block);
        let err = BlockArgs::new(&mut storage, missing_block, &start_info)
            .await
            .unwrap_err();
        assert_matches!(err, BlockArgsError::Missing);
    }
}

#[tokio::test]
async fn instantiating_vm() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();

    let block_args = BlockArgs::pending(&mut storage).await.unwrap();
    test_instantiating_vm(pool.clone(), block_args).await;
    let start_info = BlockStartInfo::new(&mut storage, Duration::MAX)
        .await
        .unwrap();
    let block_args = BlockArgs::new(&mut storage, api::BlockId::Number(0.into()), &start_info)
        .await
        .unwrap();
    test_instantiating_vm(pool.clone(), block_args).await;
}

async fn test_instantiating_vm(pool: ConnectionPool<Core>, block_args: BlockArgs) {
    let (vm_concurrency_limiter, _) = VmConcurrencyLimiter::new(1);
    let vm_permit = vm_concurrency_limiter.acquire().await.unwrap();
    let transaction = create_l2_transaction(10, 100).into();
    let estimate_gas_contracts = ApiContracts::load_from_disk().await.unwrap().estimate_gas;
    tokio::task::spawn_blocking(move || {
        apply_vm_in_sandbox(
            vm_permit,
            TxSharedArgs::mock(estimate_gas_contracts),
            true,
            &TxExecutionArgs::for_gas_estimate(None, &transaction, 123),
            &pool,
            transaction.clone(),
            block_args,
            |_, received_tx, _| {
                assert_eq!(received_tx, transaction);
            },
        )
    })
    .await
    .expect("VM instantiation panicked")
    .expect("VM instantiation errored");
}

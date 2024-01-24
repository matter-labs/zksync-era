//! Tests for the VM execution sandbox.

use assert_matches::assert_matches;

use super::*;
use crate::{
    genesis::{ensure_genesis_state, GenesisParams},
    utils::testonly::{create_miniblock, prepare_empty_recovery_snapshot},
};

#[tokio::test]
async fn creating_block_args() {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.access_storage().await.unwrap();
    ensure_genesis_state(&mut storage, L2ChainId::default(), &GenesisParams::mock())
        .await
        .unwrap();
    let miniblock = create_miniblock(1);
    storage
        .blocks_dal()
        .insert_miniblock(&miniblock)
        .await
        .unwrap();

    let pending_block_args = BlockArgs::pending(&mut storage).await.unwrap();
    assert_eq!(
        pending_block_args.block_id,
        api::BlockId::Number(api::BlockNumber::Pending)
    );
    assert_eq!(pending_block_args.resolved_block_number, MiniblockNumber(2));
    assert_eq!(pending_block_args.l1_batch_timestamp_s, None);

    let start_info = BlockStartInfo::new(&mut storage).await.unwrap();
    assert_eq!(start_info.first_miniblock, MiniblockNumber(0));
    assert_eq!(start_info.first_l1_batch, L1BatchNumber(0));

    let latest_block = api::BlockId::Number(api::BlockNumber::Latest);
    let latest_block_args = BlockArgs::new(&mut storage, latest_block, start_info)
        .await
        .unwrap();
    assert_eq!(latest_block_args.block_id, latest_block);
    assert_eq!(latest_block_args.resolved_block_number, MiniblockNumber(1));
    assert_eq!(
        latest_block_args.l1_batch_timestamp_s,
        Some(miniblock.timestamp)
    );

    let earliest_block = api::BlockId::Number(api::BlockNumber::Earliest);
    let earliest_block_args = BlockArgs::new(&mut storage, earliest_block, start_info)
        .await
        .unwrap();
    assert_eq!(earliest_block_args.block_id, earliest_block);
    assert_eq!(
        earliest_block_args.resolved_block_number,
        MiniblockNumber(0)
    );
    assert_eq!(earliest_block_args.l1_batch_timestamp_s, Some(0));

    let missing_block = api::BlockId::Number(100.into());
    let err = BlockArgs::new(&mut storage, missing_block, start_info)
        .await
        .unwrap_err();
    assert_matches!(err, BlockArgsError::Missing);
}

#[tokio::test]
async fn creating_block_args_after_snapshot_recovery() {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.access_storage().await.unwrap();
    let snapshot_recovery = prepare_empty_recovery_snapshot(&mut storage, 23).await;

    let pending_block_args = BlockArgs::pending(&mut storage).await.unwrap();
    assert_eq!(
        pending_block_args.block_id,
        api::BlockId::Number(api::BlockNumber::Pending)
    );
    assert_eq!(
        pending_block_args.resolved_block_number,
        snapshot_recovery.miniblock_number + 1
    );
    assert_eq!(pending_block_args.l1_batch_timestamp_s, None);

    let start_info = BlockStartInfo::new(&mut storage).await.unwrap();
    assert_eq!(
        start_info.first_miniblock,
        snapshot_recovery.miniblock_number + 1
    );
    assert_eq!(
        start_info.first_l1_batch,
        snapshot_recovery.l1_batch_number + 1
    );

    let latest_block = api::BlockId::Number(api::BlockNumber::Latest);
    let err = BlockArgs::new(&mut storage, latest_block, start_info)
        .await
        .unwrap_err();
    assert_matches!(err, BlockArgsError::Missing);

    let pruned_blocks = [
        api::BlockNumber::Earliest,
        0.into(),
        snapshot_recovery.miniblock_number.0.into(),
    ];
    for pruned_block in pruned_blocks {
        let pruned_block = api::BlockId::Number(pruned_block);
        let err = BlockArgs::new(&mut storage, pruned_block, start_info)
            .await
            .unwrap_err();
        assert_matches!(err, BlockArgsError::Pruned(_));
    }

    let missing_blocks = [
        api::BlockNumber::from(snapshot_recovery.miniblock_number.0 + 2),
        100.into(),
    ];
    for missing_block in missing_blocks {
        let missing_block = api::BlockId::Number(missing_block);
        let err = BlockArgs::new(&mut storage, missing_block, start_info)
            .await
            .unwrap_err();
        assert_matches!(err, BlockArgsError::Missing);
    }

    let miniblock = create_miniblock(snapshot_recovery.miniblock_number.0 + 1);
    storage
        .blocks_dal()
        .insert_miniblock(&miniblock)
        .await
        .unwrap();

    let latest_block_args = BlockArgs::new(&mut storage, latest_block, start_info)
        .await
        .unwrap();
    assert_eq!(latest_block_args.block_id, latest_block);
    assert_eq!(latest_block_args.resolved_block_number, miniblock.number);
    assert_eq!(
        latest_block_args.l1_batch_timestamp_s,
        Some(miniblock.timestamp)
    );

    for pruned_block in pruned_blocks {
        let pruned_block = api::BlockId::Number(pruned_block);
        let err = BlockArgs::new(&mut storage, pruned_block, start_info)
            .await
            .unwrap_err();
        assert_matches!(err, BlockArgsError::Pruned(_));
    }
    for missing_block in missing_blocks {
        let missing_block = api::BlockId::Number(missing_block);
        let err = BlockArgs::new(&mut storage, missing_block, start_info)
            .await
            .unwrap_err();
        assert_matches!(err, BlockArgsError::Missing);
    }
}

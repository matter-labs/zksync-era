//! Tests for Postgres storage implementation.

use rand::{thread_rng, Rng};
use zksync_concurrency::{scope, testonly::abort_on_panic};
use zksync_consensus_roles::validator;
use zksync_types::L2ChainId;

use super::*;
use crate::{
    genesis::{ensure_genesis_state, GenesisParams},
    sync_layer::{
        gossip::tests::{
            add_consensus_fields, assert_first_block_actions, assert_second_block_actions,
            block_payload, create_genesis_block, load_final_block,
        },
        tests::{run_state_keeper_with_multiple_miniblocks, OPERATOR_ADDRESS},
        ActionQueue,
    },
};

const TEST_TIMEOUT: time::Duration = time::Duration::seconds(10);

#[tokio::test]
async fn block_store_basics_for_postgres() {
    abort_on_panic();
    let pool = ConnectionPool::test_pool().await;
    run_state_keeper_with_multiple_miniblocks(pool.clone()).await;

    let mut storage = pool.access_storage().await.unwrap();
    add_consensus_fields(&mut storage, &thread_rng().gen(), 0..3).await;
    let cursor = FetcherCursor::new(&mut storage).await.unwrap();
    drop(storage);
    let (actions_sender, _) = ActionQueue::new();
    let storage = PostgresBlockStorage::new_unchecked(
        pool.clone(),
        MiniblockNumber(0),
        actions_sender,
        cursor,
        OPERATOR_ADDRESS,
    );

    let ctx = &ctx::test_root(&ctx::RealClock);
    let genesis_block = BlockStore::first_block(&storage, ctx).await.unwrap();
    assert_eq!(genesis_block.header.number, BlockNumber(0));
    let head_block = BlockStore::head_block(&storage, ctx).await.unwrap();
    assert_eq!(head_block.header.number, BlockNumber(2));
    let last_contiguous_block_number = storage.last_contiguous_block_number(ctx).await.unwrap();
    assert_eq!(last_contiguous_block_number, BlockNumber(2));

    let block = storage
        .block(ctx, BlockNumber(1))
        .await
        .unwrap()
        .expect("no block #1");
    assert_eq!(block.header.number, BlockNumber(1));
    let missing_block = storage.block(ctx, BlockNumber(3)).await.unwrap();
    assert!(missing_block.is_none(), "{missing_block:?}");
}

#[tokio::test]
async fn subscribing_to_block_updates_for_postgres() {
    abort_on_panic();
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.access_storage().await.unwrap();
    if storage.blocks_dal().is_genesis_needed().await.unwrap() {
        ensure_genesis_state(&mut storage, L2ChainId::default(), &GenesisParams::mock())
            .await
            .unwrap();
    }
    let cursor = FetcherCursor::new(&mut storage).await.unwrap();
    // ^ This is logically incorrect (the storage should not be updated other than using
    // `ContiguousBlockStore`), but for testing subscriptions this is fine.
    drop(storage);
    let (actions_sender, _) = ActionQueue::new();
    let storage = PostgresBlockStorage::new_unchecked(
        pool.clone(),
        MiniblockNumber(0),
        actions_sender,
        cursor,
        OPERATOR_ADDRESS,
    );
    let mut subscriber = storage.subscribe_to_block_writes();

    let ctx = &ctx::test_root(&ctx::RealClock);
    scope::run!(&ctx.with_timeout(TEST_TIMEOUT), |ctx, s| async {
        s.spawn_bg(storage.run_background_tasks(ctx));
        s.spawn(async {
            run_state_keeper_with_multiple_miniblocks(pool.clone()).await;
            Ok(())
        });

        loop {
            let block = *sync::changed(ctx, &mut subscriber).await?;
            if block == BlockNumber(2) {
                // We should receive at least the last update.
                break;
            }
        }
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn processing_new_blocks() {
    abort_on_panic();
    let pool = ConnectionPool::test_pool().await;
    run_state_keeper_with_multiple_miniblocks(pool.clone()).await;

    let mut storage = pool.access_storage().await.unwrap();
    add_consensus_fields(&mut storage, &thread_rng().gen(), 0..3).await;
    let first_block = load_final_block(&mut storage, 1).await;
    let second_block = load_final_block(&mut storage, 2).await;
    storage
        .transactions_dal()
        .reset_transactions_state(MiniblockNumber(0))
        .await;
    storage
        .blocks_dal()
        .delete_miniblocks(MiniblockNumber(0))
        .await
        .unwrap();
    let cursor = FetcherCursor::new(&mut storage).await.unwrap();
    drop(storage);

    let (actions_sender, mut actions) = ActionQueue::new();
    let storage = PostgresBlockStorage::new_unchecked(
        pool.clone(),
        MiniblockNumber(0),
        actions_sender,
        cursor,
        OPERATOR_ADDRESS,
    );
    let ctx = &ctx::test_root(&ctx::RealClock);
    let ctx = &ctx.with_timeout(TEST_TIMEOUT);
    storage
        .schedule_next_block(ctx, &first_block)
        .await
        .unwrap();
    assert_first_block_actions(&mut actions).await;

    storage
        .schedule_next_block(ctx, &second_block)
        .await
        .unwrap();
    assert_second_block_actions(&mut actions).await;
}

#[tokio::test]
async fn ensuring_consensus_fields_for_genesis_block() {
    abort_on_panic();
    let ctx = &ctx::test_root(&ctx::RealClock);
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.access_storage().await.unwrap();
    if storage.blocks_dal().is_genesis_needed().await.unwrap() {
        ensure_genesis_state(&mut storage, L2ChainId::default(), &GenesisParams::mock())
            .await
            .unwrap();
    }
    let cursor = FetcherCursor::new(&mut storage).await.unwrap();
    let block_payload = block_payload(&mut storage, 0).await.encode();
    drop(storage);

    let validator_key = validator::SecretKey::generate(&mut ctx.rng());
    let genesis_block = create_genesis_block(&validator_key, 0, block_payload.clone());

    let (actions_sender, _) = ActionQueue::new();
    PostgresBlockStorage::new(
        ctx,
        pool.clone(),
        actions_sender,
        cursor,
        &genesis_block,
        OPERATOR_ADDRESS,
    )
    .await
    .unwrap();

    // Check that the consensus fields are persisted for the genesis block.
    let mut storage = pool.access_storage().await.unwrap();
    let sync_block = storage
        .sync_dal()
        .sync_block(MiniblockNumber(0), Address::default(), false)
        .await
        .unwrap()
        .expect("No genesis block");
    assert!(sync_block.consensus.is_some());
    let cursor = FetcherCursor::new(&mut storage).await.unwrap();
    let other_cursor = FetcherCursor::new(&mut storage).await.unwrap();
    drop(storage);

    // Check that the storage can be initialized again.
    let (actions_sender, _) = ActionQueue::new();
    PostgresBlockStorage::new(
        ctx,
        pool.clone(),
        actions_sender,
        cursor,
        &genesis_block,
        OPERATOR_ADDRESS,
    )
    .await
    .unwrap();

    // Create a genesis block with another validator.
    let validator_key = validator::SecretKey::generate(&mut ctx.rng());
    let other_genesis_block = create_genesis_block(&validator_key, 0, block_payload);

    // Storage should not be able to initialize with other genesis block.
    let (actions_sender, _) = ActionQueue::new();
    PostgresBlockStorage::new(
        ctx,
        pool,
        actions_sender,
        other_cursor,
        &other_genesis_block,
        OPERATOR_ADDRESS,
    )
    .await
    .unwrap_err();
}

#[tokio::test]
async fn genesis_block_payload_mismatch() {
    abort_on_panic();
    let ctx = &ctx::test_root(&ctx::RealClock);
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.access_storage().await.unwrap();
    if storage.blocks_dal().is_genesis_needed().await.unwrap() {
        ensure_genesis_state(&mut storage, L2ChainId::default(), &GenesisParams::mock())
            .await
            .unwrap();
    }
    let cursor = FetcherCursor::new(&mut storage).await.unwrap();
    let other_cursor = FetcherCursor::new(&mut storage).await.unwrap();

    let bogus_block_payload = validator::Payload(vec![]);
    let validator_key = validator::SecretKey::generate(&mut ctx.rng());
    let genesis_block = create_genesis_block(&validator_key, 0, bogus_block_payload);

    let (actions_sender, _) = ActionQueue::new();
    PostgresBlockStorage::new(
        ctx,
        pool.clone(),
        actions_sender,
        cursor,
        &genesis_block,
        OPERATOR_ADDRESS,
    )
    .await
    .unwrap_err();

    let mut bogus_block_payload = block_payload(&mut storage, 0).await;
    bogus_block_payload.timestamp += 1;
    let genesis_block = create_genesis_block(&validator_key, 0, bogus_block_payload.encode());

    let (actions_sender, _) = ActionQueue::new();
    PostgresBlockStorage::new(
        ctx,
        pool.clone(),
        actions_sender,
        other_cursor,
        &genesis_block,
        OPERATOR_ADDRESS,
    )
    .await
    .unwrap_err();
}

#[tokio::test]
async fn missing_genesis_block() {
    abort_on_panic();
    let ctx = &ctx::test_root(&ctx::RealClock);
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.access_storage().await.unwrap();
    if storage.blocks_dal().is_genesis_needed().await.unwrap() {
        ensure_genesis_state(&mut storage, L2ChainId::default(), &GenesisParams::mock())
            .await
            .unwrap();
    }
    let cursor = FetcherCursor::new(&mut storage).await.unwrap();
    let block_payload = block_payload(&mut storage, 0).await.encode();
    drop(storage);

    // Create a genesis block for the (non-existing) block #2.
    let validator_key = validator::SecretKey::generate(&mut ctx.rng());
    let genesis_block = create_genesis_block(&validator_key, 2, block_payload.clone());

    let (actions_sender, _) = ActionQueue::new();
    PostgresBlockStorage::new(
        ctx,
        pool,
        actions_sender,
        cursor,
        &genesis_block,
        OPERATOR_ADDRESS,
    )
    .await
    .unwrap_err();
}

#[tokio::test]
async fn using_non_zero_genesis_block() {
    abort_on_panic();
    let ctx = &ctx::test_root(&ctx::RealClock);
    let pool = ConnectionPool::test_pool().await;
    run_state_keeper_with_multiple_miniblocks(pool.clone()).await;

    let mut storage = pool.access_storage().await.unwrap();
    let cursor = FetcherCursor::new(&mut storage).await.unwrap();
    let block_payload = block_payload(&mut storage, 2).await.encode();
    drop(storage);

    let validator_key = validator::SecretKey::generate(&mut ctx.rng());
    let genesis_block = create_genesis_block(&validator_key, 2, block_payload.clone());

    let (actions_sender, _) = ActionQueue::new();
    let store = PostgresBlockStorage::new(
        ctx,
        pool,
        actions_sender,
        cursor,
        &genesis_block,
        OPERATOR_ADDRESS,
    )
    .await
    .unwrap();

    let head_block = store.head_block(ctx).await.unwrap();
    assert_eq!(head_block.header.number, BlockNumber(2));
    assert_eq!(
        head_block.header.parent,
        validator::BlockHeaderHash::from_bytes([0; 32])
    );
    let first_block = store.first_block(ctx).await.unwrap();
    assert_eq!(first_block.header.number, BlockNumber(2));
    let last_contiguous_block_number = store.last_contiguous_block_number(ctx).await.unwrap();
    assert_eq!(last_contiguous_block_number, BlockNumber(2));

    let block = store.block(ctx, BlockNumber(2)).await.unwrap();
    assert_eq!(block, Some(head_block));
    for number in [0, 1, 3] {
        let missing_block = store.block(ctx, BlockNumber(number)).await.unwrap();
        assert!(missing_block.is_none());
    }

    let missing_blocks = store
        .missing_block_numbers(ctx, BlockNumber(0)..BlockNumber(5))
        .await
        .unwrap();
    assert_eq!(
        missing_blocks,
        [
            BlockNumber(0),
            BlockNumber(1),
            BlockNumber(3),
            BlockNumber(4)
        ]
    );
    let missing_blocks = store
        .missing_block_numbers(ctx, BlockNumber(0)..BlockNumber(2))
        .await
        .unwrap();
    assert_eq!(missing_blocks, [BlockNumber(0), BlockNumber(1)]);
    let missing_blocks = store
        .missing_block_numbers(ctx, BlockNumber(2)..BlockNumber(5))
        .await
        .unwrap();
    assert_eq!(missing_blocks, [BlockNumber(3), BlockNumber(4)]);
    let missing_blocks = store
        .missing_block_numbers(ctx, BlockNumber(2)..BlockNumber(3))
        .await
        .unwrap();
    assert_eq!(missing_blocks, []);
}

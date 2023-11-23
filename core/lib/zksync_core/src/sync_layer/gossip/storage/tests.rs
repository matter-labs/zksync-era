//! Tests for Postgres storage implementation.

use rand::{thread_rng, Rng};

use zksync_concurrency::scope;
use zksync_types::L2ChainId;

use super::*;
use crate::{
    genesis::{ensure_genesis_state, GenesisParams},
    sync_layer::{
        gossip::tests::{
            add_consensus_fields, assert_first_block_actions, assert_second_block_actions,
            load_final_block,
        },
        tests::run_state_keeper_with_multiple_miniblocks,
        ActionQueue,
    },
};

const TEST_TIMEOUT: time::Duration = time::Duration::seconds(10);

#[tokio::test]
async fn block_store_basics_for_postgres() {
    let pool = ConnectionPool::test_pool().await;
    run_state_keeper_with_multiple_miniblocks(pool.clone()).await;

    let mut storage = pool.access_storage().await.unwrap();
    add_consensus_fields(&mut storage, &thread_rng().gen(), 3).await;
    let cursor = FetcherCursor::new(&mut storage).await.unwrap();
    drop(storage);
    let (actions_sender, _) = ActionQueue::new();
    let storage = PostgresBlockStorage::new(pool.clone(), actions_sender, cursor);

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
    let storage = PostgresBlockStorage::new(pool.clone(), actions_sender, cursor);
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
    let pool = ConnectionPool::test_pool().await;
    run_state_keeper_with_multiple_miniblocks(pool.clone()).await;

    let mut storage = pool.access_storage().await.unwrap();
    add_consensus_fields(&mut storage, &thread_rng().gen(), 3).await;
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
    let storage = PostgresBlockStorage::new(pool.clone(), actions_sender, cursor);
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

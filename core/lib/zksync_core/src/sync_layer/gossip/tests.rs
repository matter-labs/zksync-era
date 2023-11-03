//! Tests for consensus adapters for EN synchronization logic.

use assert_matches::assert_matches;
use test_casing::{test_casing, Product};

use std::future::Future;

use zksync_concurrency::{ctx, scope, time};
use zksync_consensus_executor::testonly::FullValidatorConfig;
use zksync_consensus_roles::validator::FinalBlock;
use zksync_consensus_storage::{InMemoryStorage, WriteBlockStore};
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_types::{Address, L1BatchNumber, MiniblockNumber};

use super::*;
use crate::sync_layer::{
    sync_action::SyncAction,
    tests::{
        mock_l1_batch_hash_computation, run_state_keeper_with_multiple_l1_batches,
        run_state_keeper_with_multiple_miniblocks, StateKeeperHandles,
    },
    ActionQueue,
};

const CLOCK_SPEEDUP: i64 = 20;
const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(50 * CLOCK_SPEEDUP);

/// Loads a block from the storage and converts it to a `FinalBlock`.
pub(super) async fn load_final_block(
    storage: &mut StorageProcessor<'_>,
    number: u32,
) -> FinalBlock {
    let sync_block = storage
        .sync_dal()
        .sync_block(MiniblockNumber(number), Address::repeat_byte(1), true)
        .await
        .unwrap()
        .unwrap_or_else(|| panic!("no sync block #{number}"));
    conversions::sync_block_to_consensus_block(sync_block).unwrap()
}

pub(super) async fn assert_first_block_actions(actions: &mut ActionQueue) -> Vec<SyncAction> {
    let mut received_actions = vec![];
    while !matches!(received_actions.last(), Some(SyncAction::SealMiniblock(_))) {
        received_actions.push(actions.recv_action().await);
    }
    assert_matches!(
        received_actions.as_slice(),
        [
            SyncAction::OpenBatch {
                number: L1BatchNumber(1),
                timestamp: 1,
                first_miniblock_info: (MiniblockNumber(1), 1),
                ..
            },
            SyncAction::Tx(_),
            SyncAction::Tx(_),
            SyncAction::Tx(_),
            SyncAction::Tx(_),
            SyncAction::Tx(_),
            SyncAction::SealMiniblock(_),
        ]
    );
    received_actions
}

pub(super) async fn assert_second_block_actions(actions: &mut ActionQueue) -> Vec<SyncAction> {
    let mut received_actions = vec![];
    while !matches!(received_actions.last(), Some(SyncAction::SealMiniblock(_))) {
        received_actions.push(actions.recv_action().await);
    }
    assert_matches!(
        received_actions.as_slice(),
        [
            SyncAction::Miniblock {
                number: MiniblockNumber(2),
                timestamp: 2,
                virtual_blocks: 1,
            },
            SyncAction::Tx(_),
            SyncAction::Tx(_),
            SyncAction::Tx(_),
            SyncAction::SealMiniblock(_),
        ]
    );
    received_actions
}

/// Wraps a background task so that it returns `Ok(())` if it's canceled.
async fn wrap_bg_task(task: impl Future<Output = anyhow::Result<()>>) -> anyhow::Result<()> {
    match task.await {
        Ok(()) => Ok(()),
        Err(err) if err.root_cause().is::<ctx::Canceled>() => Ok(()),
        Err(err) => Err(err),
    }
}

#[test_casing(4, Product(([false, true], [false, true])))]
#[tokio::test]
async fn syncing_via_gossip_fetcher(delay_first_block: bool, delay_second_block: bool) {
    zksync_concurrency::testonly::abort_on_panic();
    let pool = ConnectionPool::test_pool().await;
    let tx_hashes = run_state_keeper_with_multiple_miniblocks(pool.clone()).await;

    let storage = pool.access_storage().await.unwrap();
    let (genesis_block, blocks) = get_blocks_and_reset_storage(storage).await;
    let [first_block, second_block] = blocks.as_slice() else {
        unreachable!("Unexpected blocks in storage: {blocks:?}");
    };
    tracing::trace!("Node storage reset");

    let ctx = &ctx::test_root(&ctx::AffineClock::new(CLOCK_SPEEDUP as f64));
    let rng = &mut ctx.rng();
    let mut validator =
        FullValidatorConfig::for_single_validator(rng, genesis_block.payload.clone()).await;
    let external_node = validator.connect_external_node(rng).await;

    let validator_storage = Arc::new(InMemoryStorage::new(genesis_block));
    if !delay_first_block {
        validator_storage.put_block(ctx, first_block).await.unwrap();
        if !delay_second_block {
            validator_storage
                .put_block(ctx, second_block)
                .await
                .unwrap();
        }
    }
    let validator = Executor::new(
        validator.node_config,
        validator.node_key,
        validator_storage.clone(),
    )
    .unwrap();
    // ^ We intentionally do not run consensus on the validator node, since it'll produce blocks
    // with payloads that cannot be parsed by the external node.

    let (actions_sender, mut actions) = ActionQueue::new();
    let (keeper_actions_sender, keeper_actions) = ActionQueue::new();
    let state_keeper = StateKeeperHandles::new(pool.clone(), keeper_actions, &[&tx_hashes]).await;
    scope::run!(ctx, |ctx, s| async {
        s.spawn_bg(wrap_bg_task(validator.run(ctx)));
        s.spawn_bg(wrap_bg_task(start_gossip_fetcher_inner(
            ctx,
            pool,
            actions_sender,
            external_node.node_config,
            external_node.node_key,
        )));

        if delay_first_block {
            ctx.sleep(POLL_INTERVAL).await?;
            validator_storage.put_block(ctx, first_block).await.unwrap();
            if !delay_second_block {
                validator_storage
                    .put_block(ctx, second_block)
                    .await
                    .unwrap();
            }
        }

        let received_actions = assert_first_block_actions(&mut actions).await;
        // Manually replicate actions to the state keeper.
        keeper_actions_sender.push_actions(received_actions).await;

        if delay_second_block {
            validator_storage
                .put_block(ctx, second_block)
                .await
                .unwrap();
        }

        let received_actions = assert_second_block_actions(&mut actions).await;
        keeper_actions_sender.push_actions(received_actions).await;
        state_keeper
            .wait(|state| state.get_local_block() == MiniblockNumber(2))
            .await;
        Ok(())
    })
    .await
    .unwrap();
}

async fn get_blocks_and_reset_storage(
    mut storage: StorageProcessor<'_>,
) -> (FinalBlock, Vec<FinalBlock>) {
    let genesis_block = load_final_block(&mut storage, 0).await;
    let sealed_miniblock_number = storage
        .blocks_dal()
        .get_sealed_miniblock_number()
        .await
        .unwrap();

    let mut blocks = Vec::with_capacity(sealed_miniblock_number.0 as usize);
    for number in 1..=sealed_miniblock_number.0 {
        blocks.push(load_final_block(&mut storage, number).await);
    }

    storage
        .transactions_dal()
        .reset_transactions_state(MiniblockNumber(0))
        .await;
    storage
        .blocks_dal()
        .delete_miniblocks(MiniblockNumber(0))
        .await
        .unwrap();
    storage
        .blocks_dal()
        .delete_l1_batches(L1BatchNumber(0))
        .await
        .unwrap();
    (genesis_block, blocks)
}

#[test_casing(4, [3, 2, 1, 0])]
#[tokio::test]
async fn syncing_via_gossip_fetcher_with_multiple_l1_batches(initial_block_count: usize) {
    assert!(initial_block_count <= 3);
    zksync_concurrency::testonly::abort_on_panic();

    let pool = ConnectionPool::test_pool().await;
    let tx_hashes = run_state_keeper_with_multiple_l1_batches(pool.clone()).await;
    let tx_hashes: Vec<_> = tx_hashes.iter().map(Vec::as_slice).collect();

    let storage = pool.access_storage().await.unwrap();
    let (genesis_block, blocks) = get_blocks_and_reset_storage(storage).await;
    assert_eq!(blocks.len(), 3); // 2 real + 1 fictive blocks
    tracing::trace!("Node storage reset");
    let (initial_blocks, delayed_blocks) = blocks.split_at(initial_block_count);

    let ctx = &ctx::test_root(&ctx::AffineClock::new(CLOCK_SPEEDUP as f64));
    let rng = &mut ctx.rng();
    let mut validator =
        FullValidatorConfig::for_single_validator(rng, genesis_block.payload.clone()).await;
    let external_node = validator.connect_external_node(rng).await;

    let validator_storage = Arc::new(InMemoryStorage::new(genesis_block));
    for block in initial_blocks {
        validator_storage.put_block(ctx, block).await.unwrap();
    }
    let validator = Executor::new(
        validator.node_config,
        validator.node_key,
        validator_storage.clone(),
    )
    .unwrap();

    let (actions_sender, actions) = ActionQueue::new();
    let state_keeper = StateKeeperHandles::new(pool.clone(), actions, &tx_hashes).await;
    scope::run!(ctx, |ctx, s| async {
        s.spawn_bg(wrap_bg_task(validator.run(ctx)));
        s.spawn_bg(async {
            for block in delayed_blocks {
                ctx.sleep(POLL_INTERVAL).await?;
                validator_storage.put_block(ctx, block).await?;
            }
            Ok(())
        });

        let cloned_pool = pool.clone();
        s.spawn_bg(async {
            mock_l1_batch_hash_computation(cloned_pool, 1).await;
            Ok(())
        });
        s.spawn_bg(wrap_bg_task(start_gossip_fetcher_inner(
            ctx,
            pool,
            actions_sender,
            external_node.node_config,
            external_node.node_key,
        )));

        state_keeper
            .wait(|state| state.get_local_block() == MiniblockNumber(3))
            .await;
        Ok(())
    })
    .await
    .unwrap();
}

//! Tests for consensus adapters for EN synchronization logic.

use assert_matches::assert_matches;
use async_trait::async_trait;
use rand::{rngs::StdRng, seq::SliceRandom, Rng};
use test_casing::test_casing;

use std::{future::Future, iter, ops};

use zksync_concurrency::{
    ctx::{self, channel},
    scope,
    sync::{self, watch},
    time,
};
use zksync_consensus_roles::validator::{BlockHeader, BlockNumber, FinalBlock, Payload};
use zksync_consensus_storage::{
    BlockStore, InMemoryStorage, StorageError, StorageResult, WriteBlockStore,
};
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_executor::testonly::FullValidatorConfig;
use zksync_types::{Address, L1BatchNumber, MiniblockNumber};

use super::{
    buffered::{BufferedStorage, BufferedStorageEvent, ContiguousBlockStore},
    *,
};
use crate::sync_layer::tests::StateKeeperHandles;
use crate::sync_layer::{
    sync_action::SyncAction, tests::run_state_keeper_with_multiple_miniblocks, ActionQueue,
};

pub(super) const TEST_TIMEOUT: time::Duration = time::Duration::seconds(1);
pub(super) const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(50);

fn init_store(rng: &mut impl Rng) -> (FinalBlock, InMemoryStorage) {
    let payload = Payload(vec![]);
    let genesis_block = FinalBlock {
        header: BlockHeader::genesis(payload.hash()),
        payload,
        justification: rng.gen(),
    };
    let block_store = InMemoryStorage::new(genesis_block.clone());
    (genesis_block, block_store)
}

fn gen_blocks(rng: &mut impl Rng, genesis_block: FinalBlock, count: usize) -> Vec<FinalBlock> {
    let blocks = iter::successors(Some(genesis_block), |parent| {
        let payload = Payload(vec![]);
        let header = BlockHeader {
            parent: parent.header.hash(),
            number: parent.header.number.next(),
            payload: payload.hash(),
        };
        Some(FinalBlock {
            header,
            payload,
            justification: rng.gen(),
        })
    });
    blocks.skip(1).take(count).collect()
}

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
    conversions::sync_block_to_consensus_block(sync_block)
}

#[derive(Debug)]
struct MockContiguousStore {
    inner: InMemoryStorage,
    block_sender: channel::Sender<FinalBlock>,
}

impl MockContiguousStore {
    fn new(inner: InMemoryStorage) -> (Self, channel::Receiver<FinalBlock>) {
        let (block_sender, block_receiver) = channel::bounded(1);
        let this = Self {
            inner,
            block_sender,
        };
        (this, block_receiver)
    }

    async fn run_updates(
        &self,
        ctx: &ctx::Ctx,
        mut block_receiver: channel::Receiver<FinalBlock>,
    ) -> StorageResult<()> {
        let rng = &mut ctx.rng();
        while let Ok(block) = block_receiver.recv(ctx).await {
            let sleep_duration = time::Duration::milliseconds(rng.gen_range(0..5));
            ctx.sleep(sleep_duration).await?;
            self.inner.put_block(ctx, &block).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl BlockStore for MockContiguousStore {
    async fn head_block(&self, ctx: &ctx::Ctx) -> StorageResult<FinalBlock> {
        self.inner.head_block(ctx).await
    }

    async fn first_block(&self, ctx: &ctx::Ctx) -> StorageResult<FinalBlock> {
        self.inner.first_block(ctx).await
    }

    async fn last_contiguous_block_number(&self, ctx: &ctx::Ctx) -> StorageResult<BlockNumber> {
        self.inner.last_contiguous_block_number(ctx).await
    }

    async fn block(
        &self,
        ctx: &ctx::Ctx,
        number: BlockNumber,
    ) -> StorageResult<Option<FinalBlock>> {
        self.inner.block(ctx, number).await
    }

    async fn missing_block_numbers(
        &self,
        ctx: &ctx::Ctx,
        range: ops::Range<BlockNumber>,
    ) -> StorageResult<Vec<BlockNumber>> {
        self.inner.missing_block_numbers(ctx, range).await
    }

    fn subscribe_to_block_writes(&self) -> watch::Receiver<BlockNumber> {
        self.inner.subscribe_to_block_writes()
    }
}

#[async_trait]
impl ContiguousBlockStore for MockContiguousStore {
    async fn schedule_next_block(&self, ctx: &ctx::Ctx, block: &FinalBlock) -> StorageResult<()> {
        let head_block_number = self.head_block(ctx).await?.header.number;
        assert_eq!(block.header.number, head_block_number.next());
        self.block_sender
            .try_send(block.clone())
            .expect("BufferedStorage is rushing");
        Ok(())
    }
}

#[tracing::instrument(level = "trace", skip(shuffle_blocks))]
async fn test_buffered_storage(
    initial_block_count: usize,
    block_count: usize,
    block_interval: time::Duration,
    shuffle_blocks: impl FnOnce(&mut StdRng, &mut [FinalBlock]),
) {
    let ctx = &ctx::test_root(&ctx::RealClock);
    let rng = &mut ctx.rng();

    let (genesis_block, block_store) = init_store(rng);
    let mut initial_blocks = gen_blocks(rng, genesis_block.clone(), initial_block_count);
    for block in &initial_blocks {
        block_store.put_block(ctx, block).await.unwrap();
    }
    initial_blocks.insert(0, genesis_block.clone());

    let (block_store, block_receiver) = MockContiguousStore::new(block_store);
    let mut buffered_store = BufferedStorage::new(block_store);
    let (events_sender, mut events_receiver) = channel::unbounded();
    buffered_store.set_events_sender(events_sender);

    // Check initial values returned by the store.
    let last_initial_block = initial_blocks.last().unwrap().clone();
    assert_eq!(
        buffered_store.head_block(ctx).await.unwrap(),
        last_initial_block
    );
    for block in &initial_blocks {
        let block_result = buffered_store.block(ctx, block.header.number).await;
        assert_eq!(block_result.unwrap().as_ref(), Some(block));
    }
    let mut subscriber = buffered_store.subscribe_to_block_writes();
    assert_eq!(
        *subscriber.borrow(),
        BlockNumber(initial_block_count as u64)
    );

    let mut blocks = gen_blocks(rng, last_initial_block, block_count);
    shuffle_blocks(rng, &mut blocks);
    let last_block_number = BlockNumber((block_count + initial_block_count) as u64);

    scope::run!(ctx, |ctx, s| async {
        s.spawn_bg(buffered_store.inner().run_updates(ctx, block_receiver));
        s.spawn_bg(async {
            let err = buffered_store.listen_to_updates(ctx).await.unwrap_err();
            match &err {
                StorageError::Canceled(_) => Ok(()), // Test has successfully finished
                StorageError::Database(_) => Err(err),
            }
        });

        for (idx, block) in blocks.iter().enumerate() {
            buffered_store.put_block(ctx, block).await?;
            let new_block_number = *sync::changed(ctx, &mut subscriber).await?;
            assert_eq!(new_block_number, block.header.number);

            // Check that all written blocks are immediately accessible.
            for existing_block in initial_blocks.iter().chain(&blocks[0..=idx]) {
                let number = existing_block.header.number;
                assert_eq!(
                    buffered_store.block(ctx, number).await?.as_ref(),
                    Some(existing_block)
                );
            }
            assert_eq!(buffered_store.first_block(ctx).await?, genesis_block);

            let expected_head_block = blocks[0..=idx]
                .iter()
                .max_by_key(|block| block.header.number)
                .unwrap();
            assert_eq!(buffered_store.head_block(ctx).await?, *expected_head_block);

            let expected_last_contiguous_block = blocks[(idx + 1)..]
                .iter()
                .map(|block| block.header.number)
                .min()
                .map_or(last_block_number, BlockNumber::prev);
            assert_eq!(
                buffered_store.last_contiguous_block_number(ctx).await?,
                expected_last_contiguous_block
            );

            ctx.sleep(block_interval).await?;
        }

        let mut inner_subscriber = buffered_store.inner().subscribe_to_block_writes();
        while buffered_store
            .inner()
            .last_contiguous_block_number(ctx)
            .await?
            < last_block_number
        {
            sync::changed(ctx, &mut inner_subscriber).await?;
        }

        // Check events emitted by the buffered storage. This also ensures that all underlying storage
        // updates are processed before proceeding to the following checks.
        let expected_numbers = (initial_block_count as u64 + 1)..=last_block_number.0;
        for expected_number in expected_numbers.map(BlockNumber) {
            assert_matches!(
                events_receiver.recv(ctx).await?,
                BufferedStorageEvent::UpdateReceived(number) if number == expected_number
            );
        }

        assert_eq!(buffered_store.buffer_len().await, 0);
        Ok(())
    })
    .await
    .unwrap();
}

// Choose intervals so that they are both smaller and larger than the sleep duration in
// `MockContiguousStore::run_updates()`.
const BLOCK_INTERVALS: [time::Duration; 4] = [
    time::Duration::ZERO,
    time::Duration::milliseconds(3),
    time::Duration::milliseconds(5),
    time::Duration::milliseconds(10),
];

#[test_casing(4, BLOCK_INTERVALS)]
#[tokio::test]
async fn buffered_storage_with_sequential_blocks(block_interval: time::Duration) {
    test_buffered_storage(0, 30, block_interval, |_, _| {
        // Do not perform shuffling
    })
    .await;
}

#[test_casing(4, BLOCK_INTERVALS)]
#[tokio::test]
async fn buffered_storage_with_random_blocks(block_interval: time::Duration) {
    test_buffered_storage(0, 30, block_interval, |rng, blocks| blocks.shuffle(rng)).await;
}

#[test_casing(4, BLOCK_INTERVALS)]
#[tokio::test]
async fn buffered_storage_with_slightly_shuffled_blocks(block_interval: time::Duration) {
    test_buffered_storage(0, 30, block_interval, |rng, blocks| {
        for chunk in blocks.chunks_mut(4) {
            chunk.shuffle(rng);
        }
    })
    .await;
}

#[test_casing(4, BLOCK_INTERVALS)]
#[tokio::test]
async fn buffered_storage_with_initial_blocks(block_interval: time::Duration) {
    test_buffered_storage(10, 20, block_interval, |_, _| {
        // Do not perform shuffling
    })
    .await;
}

#[test_casing(4, BLOCK_INTERVALS)]
#[tokio::test]
async fn buffered_storage_with_initial_blocks_and_slight_shuffling(block_interval: time::Duration) {
    test_buffered_storage(10, 20, block_interval, |rng, blocks| {
        for chunk in blocks.chunks_mut(5) {
            chunk.shuffle(rng);
        }
    })
    .await;
}

pub(super) async fn assert_first_block_actions(
    ctx: &ctx::Ctx,
    actions: &mut ActionQueue,
) -> ctx::OrCanceled<Vec<SyncAction>> {
    let mut received_actions = vec![];
    while !matches!(received_actions.last(), Some(SyncAction::SealMiniblock)) {
        let Some(action) = actions.pop_action() else {
            ctx.sleep(POLL_INTERVAL).await?;
            continue;
        };
        received_actions.push(action);
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
            SyncAction::SealMiniblock,
        ]
    );
    Ok(received_actions)
}

pub(super) async fn assert_second_block_actions(
    ctx: &ctx::Ctx,
    actions: &mut ActionQueue,
) -> ctx::OrCanceled<Vec<SyncAction>> {
    let mut received_actions = vec![];
    while !matches!(received_actions.last(), Some(SyncAction::SealMiniblock)) {
        let Some(action) = actions.pop_action() else {
            ctx.sleep(POLL_INTERVAL).await?;
            continue;
        };
        received_actions.push(action);
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
            SyncAction::SealMiniblock,
        ]
    );
    Ok(received_actions)
}

async fn wrap_bg_task(task: impl Future<Output = anyhow::Result<()>>) -> anyhow::Result<()> {
    match task.await {
        Ok(()) => Ok(()),
        Err(err) if err.root_cause().is::<ctx::Canceled>() => Ok(()),
        Err(err) => Err(err),
    }
}

#[tokio::test]
async fn syncing_via_gossip_fetcher() {
    zksync_concurrency::testonly::abort_on_panic();
    let pool = ConnectionPool::test_pool().await;
    let tx_hashes = run_state_keeper_with_multiple_miniblocks(pool.clone()).await;

    let mut storage = pool.access_storage().await.unwrap();
    let genesis_block = load_final_block(&mut storage, 0).await;
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
    drop(storage);

    let ctx = &ctx::test_root(&ctx::AffineClock::new(20.0));
    let rng = &mut ctx.rng();
    let mut validator =
        FullValidatorConfig::for_single_validator(rng, genesis_block.payload.clone());
    let external_node = validator.connect_external_node(rng);

    let validator_storage = Arc::new(InMemoryStorage::new(genesis_block));
    validator_storage
        .put_block(ctx, &first_block)
        .await
        .unwrap();
    validator_storage
        .put_block(ctx, &second_block)
        .await
        .unwrap();
    let validator =
        Executor::new(validator.node_config, validator.node_key, validator_storage).unwrap();
    // ^ We intentionally do not run consensus on the validator node, since it'll produce blocks
    // with payloads that cannot be parsed by the external node.

    let (actions_sender, mut actions) = ActionQueue::new();
    let state_keeper = StateKeeperHandles::new(pool.clone(), &[&tx_hashes]).await;
    scope::run!(ctx, |ctx, s| async {
        s.spawn_bg(wrap_bg_task(validator.run(ctx)));
        s.spawn_bg(wrap_bg_task(start_gossip_fetcher_inner(
            ctx,
            pool,
            actions_sender,
            external_node.node_config,
            external_node.node_key,
        )));

        let received_actions = assert_first_block_actions(ctx, &mut actions).await?;
        // Manually replicate actions to the state keeper.
        state_keeper
            .actions_sender
            .push_actions(received_actions)
            .await;

        let received_actions = assert_second_block_actions(ctx, &mut actions).await?;
        state_keeper
            .actions_sender
            .push_actions(received_actions)
            .await;
        state_keeper
            .wait(|state| state.get_local_block() == MiniblockNumber(2))
            .await;
        Ok(())
    })
    .await
    .unwrap();
}

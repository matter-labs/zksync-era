//! Tests for buffered storage.

use std::{iter, ops};

use assert_matches::assert_matches;
use async_trait::async_trait;
use rand::{rngs::StdRng, seq::SliceRandom, Rng};
use test_casing::test_casing;
use zksync_concurrency::{
    ctx::{self, channel},
    scope,
    sync::{self, watch},
    testonly::abort_on_panic,
    time,
};
use zksync_consensus_roles::validator::{BlockHeader, BlockNumber, FinalBlock, Payload};
use zksync_consensus_storage::{BlockStore, InMemoryStorage, WriteBlockStore};

use super::*;

fn init_store(rng: &mut impl Rng) -> (FinalBlock, InMemoryStorage) {
    let payload = Payload(vec![]);
    let genesis_block = FinalBlock {
        header: BlockHeader::genesis(payload.hash(), BlockNumber(0)),
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

#[derive(Debug)]
struct MockContiguousStore {
    inner: InMemoryStorage,
    block_sender: channel::UnboundedSender<FinalBlock>,
}

impl MockContiguousStore {
    fn new(inner: InMemoryStorage) -> (Self, channel::UnboundedReceiver<FinalBlock>) {
        let (block_sender, block_receiver) = channel::unbounded();
        let this = Self {
            inner,
            block_sender,
        };
        (this, block_receiver)
    }

    async fn run_updates(
        &self,
        ctx: &ctx::Ctx,
        mut block_receiver: channel::UnboundedReceiver<FinalBlock>,
    ) -> ctx::Result<()> {
        let rng = &mut ctx.rng();
        while let Ok(block) = block_receiver.recv(ctx).await {
            let head_block_number = self.head_block(ctx).await?.header.number;
            assert_eq!(block.header.number, head_block_number.next());

            let sleep_duration = time::Duration::milliseconds(rng.gen_range(0..5));
            ctx.sleep(sleep_duration).await?;
            self.inner.put_block(ctx, &block).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl BlockStore for MockContiguousStore {
    async fn head_block(&self, ctx: &ctx::Ctx) -> ctx::Result<FinalBlock> {
        self.inner.head_block(ctx).await
    }

    async fn first_block(&self, ctx: &ctx::Ctx) -> ctx::Result<FinalBlock> {
        self.inner.first_block(ctx).await
    }

    async fn last_contiguous_block_number(&self, ctx: &ctx::Ctx) -> ctx::Result<BlockNumber> {
        self.inner.last_contiguous_block_number(ctx).await
    }

    async fn block(&self, ctx: &ctx::Ctx, number: BlockNumber) -> ctx::Result<Option<FinalBlock>> {
        self.inner.block(ctx, number).await
    }

    async fn missing_block_numbers(
        &self,
        ctx: &ctx::Ctx,
        range: ops::Range<BlockNumber>,
    ) -> ctx::Result<Vec<BlockNumber>> {
        self.inner.missing_block_numbers(ctx, range).await
    }

    fn subscribe_to_block_writes(&self) -> watch::Receiver<BlockNumber> {
        self.inner.subscribe_to_block_writes()
    }
}

#[async_trait]
impl ContiguousBlockStore for MockContiguousStore {
    async fn schedule_next_block(&self, _ctx: &ctx::Ctx, block: &FinalBlock) -> ctx::Result<()> {
        tracing::trace!(block_number = block.header.number.0, "Scheduled next block");
        self.block_sender.send(block.clone());
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
    abort_on_panic();
    let ctx = &ctx::test_root(&ctx::RealClock);
    let rng = &mut ctx.rng();

    let (genesis_block, block_store) = init_store(rng);
    let mut initial_blocks = gen_blocks(rng, genesis_block.clone(), initial_block_count);
    for block in &initial_blocks {
        block_store.put_block(ctx, block).await.unwrap();
    }
    initial_blocks.insert(0, genesis_block.clone());

    let (block_store, block_receiver) = MockContiguousStore::new(block_store);
    let mut buffered_store = Buffered::new(block_store);
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
        s.spawn_bg(buffered_store.run_background_tasks(ctx));

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

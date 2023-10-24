//! Tests for consensus adapters for EN synchronization logic.

use assert_matches::assert_matches;
use async_trait::async_trait;
use rand::{rngs::StdRng, seq::SliceRandom, Rng};
use tempfile::TempDir;
use test_casing::test_casing;

use std::{iter, ops};

use zksync_concurrency::{
    ctx::{self, channel},
    scope,
    sync::{self, watch},
    time,
};
use zksync_consensus_roles::validator::{Block, BlockNumber, FinalBlock};
use zksync_consensus_storage::{
    BlockStore, RocksdbStorage, StorageError, StorageResult, WriteBlockStore,
};

use super::buffered::{BufferedStorage, BufferedStorageEvent, ContiguousBlockStore};

async fn init_store<R: Rng>(ctx: &ctx::Ctx, rng: &mut R) -> (FinalBlock, RocksdbStorage, TempDir) {
    let genesis_block = FinalBlock {
        block: Block::genesis(vec![]),
        justification: rng.gen(),
    };
    let temp_dir = TempDir::new().unwrap();
    let block_store = RocksdbStorage::new(ctx, &genesis_block, temp_dir.path())
        .await
        .unwrap();
    (genesis_block, block_store, temp_dir)
}

fn gen_blocks(rng: &mut impl Rng, genesis_block: FinalBlock, count: usize) -> Vec<FinalBlock> {
    let blocks = iter::successors(Some(genesis_block), |parent| {
        let block = Block {
            parent: parent.block.hash(),
            number: parent.block.number.next(),
            payload: Vec::new(),
        };
        Some(FinalBlock {
            block,
            justification: rng.gen(),
        })
    });
    blocks.skip(1).take(count).collect()
}

#[derive(Debug)]
struct MockContiguousStore {
    inner: RocksdbStorage,
    block_sender: channel::Sender<FinalBlock>,
}

impl MockContiguousStore {
    fn new(inner: RocksdbStorage) -> (Self, channel::Receiver<FinalBlock>) {
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
        let head_block_number = self.head_block(ctx).await?.block.number;
        assert_eq!(block.block.number, head_block_number.next());
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

    let (genesis_block, block_store, _temp_dir) = init_store(ctx, rng).await;
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
        let block_result = buffered_store.block(ctx, block.block.number).await;
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
        s.spawn_bg(buffered_store.as_ref().run_updates(ctx, block_receiver));
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
            assert_eq!(new_block_number, block.block.number);

            // Check that all written blocks are immediately accessible.
            for existing_block in initial_blocks.iter().chain(&blocks[0..=idx]) {
                let number = existing_block.block.number;
                assert_eq!(
                    buffered_store.block(ctx, number).await?.as_ref(),
                    Some(existing_block)
                );
            }
            assert_eq!(buffered_store.first_block(ctx).await?, genesis_block);

            let expected_head_block = blocks[0..=idx]
                .iter()
                .max_by_key(|block| block.block.number)
                .unwrap();
            assert_eq!(buffered_store.head_block(ctx).await?, *expected_head_block);

            let expected_last_contiguous_block = blocks[(idx + 1)..]
                .iter()
                .map(|block| block.block.number)
                .min()
                .map_or(last_block_number, BlockNumber::prev);
            assert_eq!(
                buffered_store.last_contiguous_block_number(ctx).await?,
                expected_last_contiguous_block
            );

            ctx.sleep(block_interval).await?;
        }

        let mut inner_subscriber = buffered_store.as_ref().subscribe_to_block_writes();
        while buffered_store
            .as_ref()
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

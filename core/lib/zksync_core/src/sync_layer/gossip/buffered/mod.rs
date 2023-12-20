//! Buffered [`BlockStore`] implementation.

use std::{collections::BTreeMap, ops, time::Instant};

use async_trait::async_trait;
#[cfg(test)]
use zksync_concurrency::ctx::channel;
use zksync_concurrency::{
    ctx, scope,
    sync::{self, watch, Mutex},
};
use zksync_consensus_roles::validator::{BlockNumber, FinalBlock, Payload};
use zksync_consensus_storage::{BlockStore, WriteBlockStore};

use super::{
    metrics::{BlockResponseKind, METRICS},
    utils::MissingBlockNumbers,
};

#[cfg(test)]
mod tests;

/// In-memory buffer or [`FinalBlock`]s received from peers, but not executed and persisted locally yet.
///
/// Unlike with executed / persisted blocks, there may be gaps between blocks in the buffer.
/// These blocks are shared with peers using the gossip network, but are not persisted and lost
/// on the node restart.
#[derive(Debug)]
struct BlockBuffer {
    store_block_number: BlockNumber,
    blocks: BTreeMap<BlockNumber, FinalBlock>,
}

impl BlockBuffer {
    fn new(store_block_number: BlockNumber) -> Self {
        Self {
            store_block_number,
            blocks: BTreeMap::new(),
        }
    }

    fn head_block(&self) -> Option<FinalBlock> {
        self.blocks.values().next_back().cloned()
    }

    #[tracing::instrument(level = "trace", skip(self))]
    fn set_store_block(&mut self, store_block_number: BlockNumber) {
        assert!(
            store_block_number > self.store_block_number,
            "`ContiguousBlockStore` invariant broken: unexpected new head block number"
        );

        self.store_block_number = store_block_number;
        let old_len = self.blocks.len();
        self.blocks = self.blocks.split_off(&store_block_number.next());
        // ^ Removes all entries up to and including `store_block_number`
        tracing::debug!("Removed {} blocks from buffer", old_len - self.blocks.len());
        METRICS.buffer_size.set(self.blocks.len());
    }

    fn last_contiguous_block_number(&self) -> BlockNumber {
        // By design, blocks in the underlying store are always contiguous.
        let mut last_number = self.store_block_number;
        for &number in self.blocks.keys() {
            if number > last_number.next() {
                return last_number;
            }
            last_number = number;
        }
        last_number
    }

    fn missing_block_numbers(&self, mut range: ops::Range<BlockNumber>) -> Vec<BlockNumber> {
        // Clamp the range start so we don't produce extra missing blocks.
        range.start = range.start.max(self.store_block_number.next());
        if range.is_empty() {
            return vec![]; // Return early to not trigger panic in `BTreeMap::range()`
        }

        let keys = self.blocks.range(range.clone()).map(|(&num, _)| num);
        MissingBlockNumbers::new(range, keys).collect()
    }

    fn put_block(&mut self, block: FinalBlock) {
        let block_number = block.header().number;
        assert!(block_number > self.store_block_number);
        // ^ Must be checked previously
        self.blocks.insert(block_number, block);
        tracing::debug!(%block_number, "Inserted block in buffer");
        METRICS.buffer_size.set(self.blocks.len());
    }
}

/// Events emitted by [`Buffered`] storage.
#[cfg(test)]
#[derive(Debug)]
pub(super) enum BufferedStorageEvent {
    /// Update was received from the underlying storage.
    UpdateReceived(BlockNumber),
}

struct Buffer {
    last: sync::watch::Sender<BlockNumber>,
    last_continuous: sync::watch::Sender<BlockNumber>,
    last_inner: BlockNumber,
    blocks: HashMap<BlockNumber,FinalBlock>,
}

impl Buffer {
    pub fn new(last_inner: BlockNumber) -> Self {
        Self {
            last: last_inner,
            last_continuous: last_inner,
            last_inner,
        }
    }

    pub fn push(&mut self, block: FinalBlock) {
        let number = block.header().number;
        if self.last_inner > number { return }
        self.last = std::cmp::max(self.last,number.next());
        self.blocks.insert(number,block);
        while self.blocks.contains_key(self.last_continuous.next()) {
            self.last_continuous = self.last_continuous.next();
        }
    }

    pub fn pop(&mut self) -> Option<FinalBlock> {
        let block = self.blocks.remove(self.first)?;
        self.begin = self.begin.next();
        Some(block)
    }
}

/// [`BlockStore`] with an in-memory buffer for pending blocks.
///
/// # Data flow
///
/// The store is plugged into the `SyncBlocks` actor, so that it can receive new blocks
/// from peers over the gossip network and to share blocks with peers. Received blocks are stored
/// in a [`BlockBuffer`]. The `SyncBlocks` actor doesn't guarantee that blocks are received in order,
/// so we have a background task that waits for successive blocks and feeds them to
/// the underlying storage ([`ContiguousBlockStore`]). The underlying storage executes and persists
/// blocks using the state keeper; see [`PostgresBlockStorage`](super::PostgresBlockStorage) for more details.
/// This logic is largely shared with the old syncing logic using JSON-RPC; the only differing part
/// is producing block data.
///
/// Once a block is processed and persisted by the state keeper, it can be removed from the [`BlockBuffer`];
/// we do this in another background task. Removing blocks from the buffer ensures that it doesn't
/// grow infinitely; it also allows to track syncing progress via metrics.
#[derive(Debug)]
pub(super) struct Buffered {
    inner: Arc<dyn WriteBlockStore + BlockStore>,
    buffer: Mutex<Buffer>,
    head_send: watch::Sender<BlockNumber>,
}

impl Buffered {
    /// Creates a new buffered storage. The buffer is initially empty.
    pub async fn new(ctx: &ctx::Ctx, inner: Arc<dyn WriteBlockStore + BlockStore>) -> ctx::Result<Self> {
        let head = inner.first_block_number(ctx).await?,
        Self {
            head: inner.block(ctx,head).await?,
            buffer: Buffer::new(head),
            head_send: watch::channel(head).0,
            inner,
        }
    }
    
    /// Schedules blocks in the underlying store as they are pushed to this store.
    async fn run_background_tasks(&self, ctx: &ctx::Ctx) -> anyhow::Result<()> {
        let buffer = &mut self.buffer.subscribe();
        loop {
            let block = sync::wait_for(ctx, buffer, |b| b.begin < b.continuous_end).await?.first().unwrap();
            if let Err(err) = self.inner.put_block(ctx,&block).await {
                return match err {
                    ctx::Error::Internal(err) => Err(err),
                    ctx::Error::Canceled(_) => Ok(()),
                };
            }
        }
    }
}

#[async_trait]
impl<T: WriteBlockStore> BlockStore for Buffered<T> {
    async fn head_block(&self, ctx: &ctx::Ctx) -> ctx::Result<FinalBlock> {
        { 
            let buffer = self.buffer.lock.unwrap();
            if Some(block) = buffer.blocks.get(*buffer.head.borrow()) {
                return Ok(block.clone());
            }
        }
        self.inner.head_block(ctx).await
    }

    async fn first_block(&self, ctx: &ctx::Ctx) -> ctx::Result<FinalBlock> {
        // First block is always situated in the underlying store
        self.inner.first_block(ctx).await
    }

    async fn last_contiguous_block_number(&self, ctx: &ctx::Ctx) -> ctx::Result<BlockNumber> {
        Ok(self.buffer.borrow().continuous_end.prev())
    }

    async fn block(&self, ctx: &ctx::Ctx, number: BlockNumber) -> ctx::Result<Option<FinalBlock>> {
        if let Some(block) = self.buffer.borrow().blocks.get(&number) {
            return Ok(Some(block.clone()));
        }
        self.inner.block(ctx,number).await
    }

    async fn missing_block_numbers(
        &self,
        ctx: &ctx::Ctx,
        range: ops::Range<BlockNumber>,
    ) -> ctx::Result<Vec<BlockNumber>> {
        // By design, the underlying store has no missing blocks.
        Ok(sync::lock(ctx, &self.buffer)
            .await?
            .missing_block_numbers(range))
    }

    fn subscribe_to_block_writes(&self) -> watch::Receiver<BlockNumber> {
        self.head_send.subscribe()
    }
}

#[async_trait]
impl<T: WriteBlockStore> WriteBlockStore for Buffered<T> {
    async fn verify_payload(&self, ctx: &ctx::Ctx, block_number: validator::BlockNumber, payload: &validator::Payload) -> ctx::Result<()> {
        self.inner.verify_payload(ctx,block_number,payload).await
    }

    async fn put_block(&self, ctx: &ctx::Ctx, block: &FinalBlock) -> ctx::Result<()> {
        let head = self.head.lock().unwrap();
        self.buffer.send_modify(|b| b.push(block.clone());
        if block.header().number > head.header().number {
            head = block.clone();
            self.head_send.send_replace(head.header().number);
        }
        Ok(())
    }
}

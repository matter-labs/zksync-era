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

/// [`BlockStore`] variation that upholds additional invariants as to how blocks are processed.
///
/// The invariants are as follows:
///
/// - Stored blocks always have contiguous numbers; there are no gaps.
/// - Blocks can be scheduled to be added using [`Self::schedule_next_block()`] only. New blocks do not
///   appear in the store otherwise.
#[async_trait]
pub(super) trait ContiguousBlockStore: BlockStore {
    /// Schedules a block to be added to the store. Unlike [`WriteBlockStore::put_block()`],
    /// there is no expectation that the block is added to the store *immediately*. It's
    /// expected that it will be added to the store eventually, which will be signaled via
    /// a subscriber returned from [`BlockStore::subscribe_to_block_writes()`].
    ///
    /// [`Buffered`] guarantees that this method will only ever be called:
    ///
    /// - with the next block (i.e., one immediately after [`BlockStore::head_block()`])
    /// - sequentially (i.e., multiple blocks cannot be scheduled at once)
    async fn schedule_next_block(&self, ctx: &ctx::Ctx, block: &FinalBlock) -> ctx::Result<()>;
}

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
        let block_number = block.header.number;
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
pub(super) struct Buffered<T> {
    inner: T,
    inner_subscriber: watch::Receiver<BlockNumber>,
    block_writes_sender: watch::Sender<BlockNumber>,
    buffer: Mutex<BlockBuffer>,
    #[cfg(test)]
    events_sender: channel::UnboundedSender<BufferedStorageEvent>,
}

impl<T: ContiguousBlockStore> Buffered<T> {
    /// Creates a new buffered storage. The buffer is initially empty.
    pub fn new(store: T) -> Self {
        let inner_subscriber = store.subscribe_to_block_writes();
        let store_block_number = *inner_subscriber.borrow();
        tracing::debug!(
            store_block_number = store_block_number.0,
            "Initialized buffer storage"
        );
        Self {
            inner: store,
            inner_subscriber,
            block_writes_sender: watch::channel(store_block_number).0,
            buffer: Mutex::new(BlockBuffer::new(store_block_number)),
            #[cfg(test)]
            events_sender: channel::unbounded().0,
        }
    }

    #[cfg(test)]
    fn set_events_sender(&mut self, sender: channel::UnboundedSender<BufferedStorageEvent>) {
        self.events_sender = sender;
    }

    pub(super) fn inner(&self) -> &T {
        &self.inner
    }

    #[cfg(test)]
    async fn buffer_len(&self) -> usize {
        self.buffer.lock().await.blocks.len()
    }

    /// Listens to the updates in the underlying storage.
    #[tracing::instrument(level = "trace", skip_all)]
    async fn listen_to_updates(&self, ctx: &ctx::Ctx) {
        let mut subscriber = self.inner_subscriber.clone();
        loop {
            let store_block_number = {
                let Ok(number) = sync::changed(ctx, &mut subscriber).await else {
                    return; // Do not propagate cancellation errors
                };
                *number
            };
            tracing::debug!(
                store_block_number = store_block_number.0,
                "Underlying block number updated"
            );

            let Ok(mut buffer) = sync::lock(ctx, &self.buffer).await else {
                return; // Do not propagate cancellation errors
            };
            buffer.set_store_block(store_block_number);
            #[cfg(test)]
            self.events_sender
                .send(BufferedStorageEvent::UpdateReceived(store_block_number));
        }
    }

    /// Schedules blocks in the underlying store as they are pushed to this store.
    #[tracing::instrument(level = "trace", skip_all, err)]
    async fn schedule_blocks(&self, ctx: &ctx::Ctx) -> ctx::Result<()> {
        let mut blocks_subscriber = self.block_writes_sender.subscribe();

        let mut next_scheduled_block_number = {
            let Ok(buffer) = sync::lock(ctx, &self.buffer).await else {
                return Ok(()); // Do not propagate cancellation errors
            };
            buffer.store_block_number.next()
        };
        loop {
            loop {
                let block = match self.buffered_block(ctx, next_scheduled_block_number).await {
                    Err(ctx::Canceled) => return Ok(()), // Do not propagate cancellation errors
                    Ok(None) => break,
                    Ok(Some(block)) => block,
                };
                self.inner.schedule_next_block(ctx, &block).await?;
                next_scheduled_block_number = next_scheduled_block_number.next();
            }
            // Wait until some more blocks are pushed into the buffer.
            let Ok(number) = sync::changed(ctx, &mut blocks_subscriber).await else {
                return Ok(()); // Do not propagate cancellation errors
            };
            tracing::debug!(block_number = number.0, "Received new block");
        }
    }

    async fn buffered_block(
        &self,
        ctx: &ctx::Ctx,
        number: BlockNumber,
    ) -> ctx::OrCanceled<Option<FinalBlock>> {
        Ok(sync::lock(ctx, &self.buffer)
            .await?
            .blocks
            .get(&number)
            .cloned())
    }

    /// Runs background tasks for this store. This method **must** be spawned as a background task
    /// which should be running as long at the [`Buffered`] is in use; otherwise, it will function incorrectly.
    pub async fn run_background_tasks(&self, ctx: &ctx::Ctx) -> ctx::Result<()> {
        scope::run!(ctx, |ctx, s| {
            s.spawn(async {
                self.listen_to_updates(ctx).await;
                Ok(())
            });
            self.schedule_blocks(ctx)
        })
        .await
    }
}

#[async_trait]
impl<T: ContiguousBlockStore> BlockStore for Buffered<T> {
    async fn head_block(&self, ctx: &ctx::Ctx) -> ctx::Result<FinalBlock> {
        let buffered_head_block = sync::lock(ctx, &self.buffer).await?.head_block();
        if let Some(block) = buffered_head_block {
            return Ok(block);
        }
        self.inner.head_block(ctx).await
    }

    async fn first_block(&self, ctx: &ctx::Ctx) -> ctx::Result<FinalBlock> {
        // First block is always situated in the underlying store
        self.inner.first_block(ctx).await
    }

    async fn last_contiguous_block_number(&self, ctx: &ctx::Ctx) -> ctx::Result<BlockNumber> {
        Ok(sync::lock(ctx, &self.buffer)
            .await?
            .last_contiguous_block_number())
    }

    async fn block(&self, ctx: &ctx::Ctx, number: BlockNumber) -> ctx::Result<Option<FinalBlock>> {
        let started_at = Instant::now();
        {
            let buffer = sync::lock(ctx, &self.buffer).await?;
            if number > buffer.store_block_number {
                let block = buffer.blocks.get(&number).cloned();
                METRICS.get_block_latency[&BlockResponseKind::InMemory]
                    .observe(started_at.elapsed());
                return Ok(block);
            }
        }
        let block = self.inner.block(ctx, number).await?;
        METRICS.get_block_latency[&BlockResponseKind::Persisted].observe(started_at.elapsed());
        Ok(block)
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
        self.block_writes_sender.subscribe()
    }
}

#[async_trait]
impl<T: ContiguousBlockStore> WriteBlockStore for Buffered<T> {
    /// Verify that `payload` is a correct proposal for the block `block_number`.
    async fn verify_payload(
        &self,
        _ctx: &ctx::Ctx,
        _block_number: BlockNumber,
        _payload: &Payload,
    ) -> ctx::Result<()> {
        // This is storage for non-validator nodes (aka full nodes),
        // so `verify_payload()` won't be called.
        // Still, it probably would be better to either
        // * move `verify_payload()` to `BlockStore`, so that Buffered can just forward the call
        // * create another separate trait for `verify_payload`.
        // It will be clear what needs to be done when we implement multi-validator consensus for
        // zksync-era.
        unimplemented!()
    }

    async fn put_block(&self, ctx: &ctx::Ctx, block: &FinalBlock) -> ctx::Result<()> {
        let buffer_block_latency = METRICS.buffer_block_latency.start();
        {
            let mut buffer = sync::lock(ctx, &self.buffer).await?;
            let block_number = block.header.number;
            if block_number <= buffer.store_block_number {
                return Err(anyhow::anyhow!(
                    "Cannot replace a block #{block_number} since it is already present in the underlying storage",
                ).into());
            }
            buffer.put_block(block.clone());
        }
        self.block_writes_sender.send_replace(block.header.number);
        buffer_block_latency.observe();
        Ok(())
    }
}

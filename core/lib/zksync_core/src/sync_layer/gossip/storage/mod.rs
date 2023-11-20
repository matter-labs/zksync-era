//! Storage implementation based on DAL.

use anyhow::Context as _;
use async_trait::async_trait;

use std::ops;

use zksync_concurrency::{
    ctx,
    sync::{self, watch, Mutex},
    time,
};
use zksync_consensus_roles::validator::{BlockNumber, FinalBlock};
use zksync_consensus_storage::{BlockStore, StorageError, StorageResult};
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_types::{Address, MiniblockNumber};

#[cfg(test)]
mod tests;

use super::{buffered::ContiguousBlockStore, conversions::sync_block_to_consensus_block};
use crate::sync_layer::{
    fetcher::{FetchedBlock, FetcherCursor},
    sync_action::{ActionQueueSender, SyncAction},
};

#[derive(Debug)]
struct CursorWithCachedBlock {
    inner: FetcherCursor,
    maybe_last_block_in_batch: Option<FetchedBlock>,
}

impl From<FetcherCursor> for CursorWithCachedBlock {
    fn from(inner: FetcherCursor) -> Self {
        Self {
            inner,
            maybe_last_block_in_batch: None,
        }
    }
}

impl CursorWithCachedBlock {
    fn advance(&mut self, block: FetchedBlock) -> Vec<Vec<SyncAction>> {
        let mut actions = Vec::with_capacity(2);
        if let Some(mut prev_block) = self.maybe_last_block_in_batch.take() {
            prev_block.last_in_batch = prev_block.l1_batch_number != block.l1_batch_number;
            actions.push(self.inner.advance(prev_block));
        }

        // We take advantage of the fact that the last block in a batch is a *fictive* block that
        // does not contain transactions. Thus, any block with transactions cannot be last in an L1 batch.
        let can_be_last_in_batch = block.transactions.is_empty();
        if can_be_last_in_batch {
            self.maybe_last_block_in_batch = Some(block);
            // We cannot convert the block into actions yet, since we don't know whether it seals an L1 batch.
        } else {
            actions.push(self.inner.advance(block));
        }
        actions
    }
}

/// Postgres-based [`BlockStore`] implementation. New blocks are scheduled to be written via
/// [`ContiguousBlockStore`] trait, which internally uses an [`ActionQueueSender`].
#[derive(Debug)]
pub(super) struct PostgresBlockStorage {
    pool: ConnectionPool,
    actions: ActionQueueSender,
    block_sender: watch::Sender<BlockNumber>,
    cursor: Mutex<CursorWithCachedBlock>,
}

impl PostgresBlockStorage {
    /// Creates a new storage handle. `pool` should have multiple connections to work efficiently.
    pub fn new(pool: ConnectionPool, actions: ActionQueueSender, cursor: FetcherCursor) -> Self {
        let current_block_number = cursor.next_miniblock.0.saturating_sub(1).into();
        Self {
            pool,
            actions,
            block_sender: watch::channel(BlockNumber(current_block_number)).0,
            cursor: Mutex::new(cursor.into()),
        }
    }

    /// Runs background tasks for this store. This method **must** be spawned as a background task
    /// which should be running as long at the [`PostgresBlockStorage`] is in use; otherwise,
    /// it will function incorrectly.
    pub async fn run_background_tasks(&self, ctx: &ctx::Ctx) -> StorageResult<()> {
        const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(50);
        loop {
            let sealed_miniblock_number = match self.sealed_miniblock_number(ctx).await {
                Ok(number) => number,
                Err(err @ StorageError::Database(_)) => return Err(err),
                Err(StorageError::Canceled(_)) => return Ok(()), // Do not propagate cancellation errors
            };
            self.block_sender.send_if_modified(|number| {
                if *number != sealed_miniblock_number {
                    *number = sealed_miniblock_number;
                    true
                } else {
                    false
                }
            });
            if let Err(ctx::Canceled) = ctx.sleep(POLL_INTERVAL).await {
                return Ok(()); // Do not propagate cancellation errors
            }
        }
    }

    async fn storage(&self, ctx: &ctx::Ctx) -> StorageResult<StorageProcessor<'_>> {
        ctx.wait(self.pool.access_storage_tagged("sync_layer"))
            .await?
            .context("Failed to connect to Postgres")
            .map_err(StorageError::Database)
    }

    async fn block(
        ctx: &ctx::Ctx,
        storage: &mut StorageProcessor<'_>,
        number: MiniblockNumber,
    ) -> StorageResult<Option<FinalBlock>> {
        let operator_address = Address::default(); // FIXME: where to get this address from?
        let Some(block) = ctx
            .wait(
                storage
                    .sync_dal()
                    .sync_block(number, operator_address, true),
            )
            .await?
            .with_context(|| format!("Failed getting miniblock #{number} from Postgres"))
            .map_err(StorageError::Database)?
        else {
            return Ok(None);
        };
        let block = sync_block_to_consensus_block(block).map_err(StorageError::Database)?;
        Ok(Some(block))
    }

    async fn sealed_miniblock_number(&self, ctx: &ctx::Ctx) -> StorageResult<BlockNumber> {
        let mut storage = self.storage(ctx).await?;
        let number = ctx
            .wait(storage.blocks_dal().get_sealed_miniblock_number())
            .await?
            .context("Failed getting sealed miniblock number")
            .map_err(StorageError::Database)?;
        Ok(BlockNumber(number.0.into()))
    }
}

#[async_trait]
impl BlockStore for PostgresBlockStorage {
    async fn head_block(&self, ctx: &ctx::Ctx) -> StorageResult<FinalBlock> {
        let mut storage = self.storage(ctx).await?;
        let miniblock_number = ctx
            .wait(storage.blocks_dal().get_sealed_miniblock_number())
            .await?
            .context("Failed getting sealed miniblock number")
            .map_err(StorageError::Database)?;
        // ^ The number can get stale, but it's OK for our purposes
        Ok(Self::block(ctx, &mut storage, miniblock_number)
            .await?
            .with_context(|| format!("Miniblock #{miniblock_number} disappeared from Postgres"))
            .map_err(StorageError::Database)?)
    }

    async fn first_block(&self, ctx: &ctx::Ctx) -> StorageResult<FinalBlock> {
        let mut storage = self.storage(ctx).await?;
        Self::block(ctx, &mut storage, MiniblockNumber(0))
            .await?
            .context("Genesis miniblock not present in Postgres")
            .map_err(StorageError::Database)
    }

    async fn last_contiguous_block_number(&self, ctx: &ctx::Ctx) -> StorageResult<BlockNumber> {
        self.sealed_miniblock_number(ctx).await
    }

    async fn block(
        &self,
        ctx: &ctx::Ctx,
        number: BlockNumber,
    ) -> StorageResult<Option<FinalBlock>> {
        let number = u32::try_from(number.0)
            .context("block number is too large")
            .map_err(StorageError::Database)?;
        let mut storage = self.storage(ctx).await?;
        Self::block(ctx, &mut storage, MiniblockNumber(number)).await
    }

    async fn missing_block_numbers(
        &self,
        _ctx: &ctx::Ctx,
        _range: ops::Range<BlockNumber>,
    ) -> StorageResult<Vec<BlockNumber>> {
        Ok(vec![]) // The storage never has missing blocks by construction
    }

    fn subscribe_to_block_writes(&self) -> watch::Receiver<BlockNumber> {
        self.block_sender.subscribe()
    }
}

#[async_trait]
impl ContiguousBlockStore for PostgresBlockStorage {
    async fn schedule_next_block(&self, ctx: &ctx::Ctx, block: &FinalBlock) -> StorageResult<()> {
        // last_in_batch` is always set to `false` by this call; it is properly set by `CursorWithCachedBlock`.
        let fetched_block =
            FetchedBlock::from_gossip_block(block, false).map_err(StorageError::Database)?;
        let actions = sync::lock(ctx, &self.cursor).await?.advance(fetched_block);
        for actions_chunk in actions {
            // We don't wrap this in `ctx.wait()` because `PostgresBlockStorage` will get broken
            // if it gets reused after context cancellation.
            self.actions.push_actions(actions_chunk).await;
        }
        Ok(())
    }
}

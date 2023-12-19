//! Storage implementation based on DAL.
use std::ops;

use crate::consensus;
use anyhow::Context as _;
use zksync_concurrency::{
    ctx,
    error::Wrap as _,
    sync::{self, watch, Mutex},
    time,
};
use zksync_consensus_roles::validator;
use zksync_consensus_storage::BlockStore;
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_types::{api::en::SyncBlock, Address, MiniblockNumber};

#[cfg(test)]
mod tests;

use super::buffered::ContiguousBlockStore;
use crate::{
    sync_layer::{
        fetcher::{FetchedBlock, FetcherCursor},
        sync_action::{ActionQueueSender, SyncAction},
    },
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
/// [`ContiguousBlockStore`] trait, which internally uses an [`ActionQueueSender`] to queue
/// block data (miniblock and L1 batch parameters, transactions) for the state keeper. Block data processing
/// is shared with JSON-RPC-based syncing.
#[derive(Debug)]
pub(super) struct PostgresBlockStorage {
    inner: consensus::storage::SignedBlockStorage,
    actions: ActionQueueSender,
    cursor: Mutex<CursorWithCachedBlock>,
}

/*
impl PostgresBlockStorage {
    /// Creates a new storage handle. `pool` should have multiple connections to work efficiently.
    pub async fn new(
        ctx: &ctx::Ctx,
        pool: ConnectionPool,
        actions: ActionQueueSender,
        cursor: FetcherCursor,
        genesis_block: validator::BlockNumber,
        operator_address: Address,
    ) -> ctx::Result<Self> {
        let genesis_block =
            u32::try_from(genesis_block.0).context("Block number overflow for genesis block")?;
        let genesis_block = MiniblockNumber(genesis_block);
        {
            let mut storage = ctx.wait(pool.access_storage_tagged("sync_layer")).await??;
            if !ctx
                .wait(storage.blocks_dal().has_consensus_fields(genesis_block))
                .await?
                .context("has_consensus_fields()")?
            {
                return Err(
                    anyhow::anyhow!("genesis block doesn't have consensus fields set").into(),
                );
            }
        }
        Ok(Self::new_unchecked(
            pool,
            genesis_block,
            actions,
            cursor,
            operator_address,
        ))
    }

    fn new_unchecked(
        pool: ConnectionPool,
        first_block_number: MiniblockNumber,
        actions: ActionQueueSender,
        cursor: FetcherCursor,
        operator_address: Address,
    ) -> Self {
        let current_block_number = cursor.next_miniblock.0.saturating_sub(1).into();
        Self {
            pool,
            first_block_number,
            actions,
            block_sender: watch::channel(validator::BlockNumber(current_block_number)).0,
            cursor: Mutex::new(cursor.into()),
            operator_address,
        }
    }

    /// Runs background tasks for this store. This method **must** be spawned as a background task
    /// which should be running as long at the [`PostgresBlockStorage`] is in use; otherwise,
    /// it will function incorrectly.
    pub async fn run_background_tasks(&self, ctx: &ctx::Ctx) -> anyhow::Result<()> {
        const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(50);
        loop {
            let sealed_miniblock_number = match self.sealed_miniblock_number(ctx).await {
                Ok(number) => number,
                Err(ctx::Error::Internal(err)) => return Err(err),
                Err(ctx::Error::Canceled(_)) => return Ok(()), // Do not propagate cancellation errors
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

    async fn storage(&self, ctx: &ctx::Ctx) -> ctx::Result<StorageProcessor<'_>> {
        Ok(ctx
            .wait(self.pool.access_storage_tagged("sync_layer"))
            .await?
            .context("Failed to connect to Postgres")?)
    }

    async fn sync_block(
        ctx: &ctx::Ctx,
        storage: &mut StorageProcessor<'_>,
        number: MiniblockNumber,
        operator_address: Address,
    ) -> ctx::Result<Option<SyncBlock>> {
        Ok(ctx
            .wait(
                storage
                    .sync_dal()
                    .sync_block(number, operator_address, true),
            )
            .await?
            .with_context(|| format!("Failed getting miniblock #{number} from Postgres"))?)
    }

    async fn block(
        ctx: &ctx::Ctx,
        storage: &mut StorageProcessor<'_>,
        number: MiniblockNumber,
        operator_address: Address,
    ) -> ctx::Result<Option<validator::FinalBlock>> {
        let Some(block) = Self::sync_block(ctx, storage, number, operator_address)
            .await
            .wrap("Self::sync_block()")?
        else {
            return Ok(None);
        };
        let block =
            sync_block_to_consensus_block(block).context("sync_block_to_consensus_block()")?;
        Ok(Some(block))
    }

    async fn sealed_miniblock_number(&self, ctx: &ctx::Ctx) -> ctx::Result<validator::BlockNumber> {
        let mut storage = self.storage(ctx).await.wrap("storage()")?;
        let number = ctx
            .wait(storage.blocks_dal().get_sealed_miniblock_number())
            .await?
            .context("Failed getting sealed miniblock number")?;
        Ok(validator::BlockNumber(number.0.into()))
    }
}*/

#[async_trait::async_trait]
impl BlockStore for PostgresBlockStorage {
    async fn head_block(&self, ctx: &ctx::Ctx) -> ctx::Result<validator::FinalBlock> {
        let mut storage = self.storage(ctx).await.wrap("storage()")?;
        let miniblock_number = ctx
            .wait(storage.blocks_dal().get_sealed_miniblock_number())
            .await?
            .context("Failed getting sealed miniblock number")?;
        // ^ The number can get stale, but it's OK for our purposes
        Ok(
            Self::block(ctx, &mut storage, miniblock_number, self.operator_address)
                .await
                .wrap("Self::block()")?
                .with_context(|| {
                    format!("Miniblock #{miniblock_number} disappeared from Postgres")
                })?,
        )
    }

    async fn first_block(&self, ctx: &ctx::Ctx) -> ctx::Result<validator::FinalBlock> {
        let mut storage = self.storage(ctx).await.wrap("storage()")?;
        Ok(Self::block(
            ctx,
            &mut storage,
            self.first_block_number,
            self.operator_address,
        )
        .await
        .wrap("Self::block()")?
        .context("Genesis miniblock not present in Postgres")?)
    }

    async fn last_contiguous_block_number(
        &self,
        ctx: &ctx::Ctx,
    ) -> ctx::Result<validator::BlockNumber> {
        self.sealed_miniblock_number(ctx)
            .await
            .wrap("sealed_miniblock_number()")
    }

    async fn block(
        &self,
        ctx: &ctx::Ctx,
        number: validator::BlockNumber,
    ) -> ctx::Result<Option<validator::FinalBlock>> {
        let Ok(number) = u32::try_from(number.0) else {
            return Ok(None);
        };
        let number = MiniblockNumber(number);
        if number < self.first_block_number {
            return Ok(None);
        }
        let mut storage = self.storage(ctx).await.wrap("storage()")?;
        Self::block(ctx, &mut storage, number, self.operator_address)
            .await
            .wrap("Self::block()")
    }

    async fn missing_block_numbers(
        &self,
        ctx: &ctx::Ctx,
        range: ops::Range<validator::BlockNumber>,
    ) -> ctx::Result<Vec<validator::BlockNumber>> {
        let mut output = vec![];
        let first_block_number = u64::from(self.first_block_number.0);
        let numbers_before_first_block =
            (range.start.0..first_block_number).map(validator::BlockNumber);
        output.extend(numbers_before_first_block);

        let last_block_number = self
            .sealed_miniblock_number(ctx)
            .await
            .wrap("sealed_miniblock_number()")?;
        let numbers_after_last_block =
            (last_block_number.next().0..range.end.0).map(validator::BlockNumber);
        output.extend(numbers_after_last_block);

        // By design, no blocks are missing in the `first_block_number..=last_block_number` range.
        Ok(output)
    }

    fn subscribe_to_block_writes(&self) -> watch::Receiver<validator::BlockNumber> {
        self.block_sender.subscribe()
    }
}

#[async_trait::async_trait]
impl ContiguousBlockStore for PostgresBlockStorage {
    async fn schedule_next_block(
        &self,
        ctx: &ctx::Ctx,
        block: &validator::FinalBlock,
    ) -> ctx::Result<()> {
        // last_in_batch` is always set to `false` by this call; it is properly set by `CursorWithCachedBlock`.
        let fetched_block = FetchedBlock::from_gossip_block(block, false).context("from_gossip_block()")?;
        let actions = sync::lock(ctx, &self.cursor).await?.advance(fetched_block);
        for actions_chunk in actions {
            // We don't wrap this in `ctx.wait()` because `PostgresBlockStorage` will get broken
            // if it gets reused after context cancellation.
            self.actions.push_actions(actions_chunk).await;
        }
        Ok(())
    }
}

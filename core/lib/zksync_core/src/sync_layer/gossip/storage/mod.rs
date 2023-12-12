//! Storage implementation based on DAL.

use std::ops;

use anyhow::Context as _;
use async_trait::async_trait;
use zksync_concurrency::{
    ctx,
    sync::{self, watch, Mutex},
    time,
};
use zksync_consensus_roles::validator::{BlockNumber, FinalBlock};
use zksync_consensus_storage::{BlockStore, StorageError, StorageResult};
use zksync_dal::{blocks_dal::ConsensusBlockFields, ConnectionPool, StorageProcessor};
use zksync_types::{api::en::SyncBlock, MiniblockNumber};

use super::{buffered::ContiguousBlockStore, conversions::sync_block_to_consensus_block};
use crate::{
    consensus,
    sync_layer::{
        fetcher::{FetchedBlock, FetcherCursor},
        sync_action::{ActionQueueSender, SyncAction},
    },
};

#[cfg(test)]
mod tests;

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
    pool: ConnectionPool,
    first_block_number: MiniblockNumber,
    actions: ActionQueueSender,
    block_sender: watch::Sender<BlockNumber>,
    cursor: Mutex<CursorWithCachedBlock>,
}

impl PostgresBlockStorage {
    /// Creates a new storage handle. `pool` should have multiple connections to work efficiently.
    pub async fn new(
        ctx: &ctx::Ctx,
        pool: ConnectionPool,
        actions: ActionQueueSender,
        cursor: FetcherCursor,
        genesis_block: &FinalBlock,
    ) -> StorageResult<Self> {
        let mut storage = ctx
            .wait(pool.access_storage_tagged("sync_layer"))
            .await?
            .map_err(StorageError::Database)?;
        Self::ensure_genesis_block(ctx, &mut storage, genesis_block).await?;
        drop(storage);

        let first_block_number = u32::try_from(genesis_block.header.number.0)
            .context("Block number overflow for genesis block")
            .map_err(StorageError::Database)?;
        let first_block_number = MiniblockNumber(first_block_number);

        Ok(Self::new_unchecked(
            pool,
            first_block_number,
            actions,
            cursor,
        ))
    }

    fn new_unchecked(
        pool: ConnectionPool,
        first_block_number: MiniblockNumber,
        actions: ActionQueueSender,
        cursor: FetcherCursor,
    ) -> Self {
        let current_block_number = cursor.next_miniblock.0.saturating_sub(1).into();
        Self {
            pool,
            first_block_number,
            actions,
            block_sender: watch::channel(BlockNumber(current_block_number)).0,
            cursor: Mutex::new(cursor.into()),
        }
    }

    async fn ensure_genesis_block(
        ctx: &ctx::Ctx,
        storage: &mut StorageProcessor<'_>,
        genesis_block: &FinalBlock,
    ) -> StorageResult<()> {
        let block_number = u32::try_from(genesis_block.header.number.0)
            .context("Block number overflow for genesis block")
            .map_err(StorageError::Database)?;
        let block = Self::sync_block(ctx, storage, MiniblockNumber(block_number)).await?;
        let block = block
            .with_context(|| {
                format!("Genesis block #{block_number} (first block with consensus data) is not present in Postgres")
            })
            .map_err(StorageError::Database)?;
        let actual_consensus_fields = block.consensus.clone();

        // Some of the following checks are duplicated in `Executor` initialization, but it's necessary
        // to run them if the genesis consensus block is not present locally.
        let expected_payload = consensus::Payload::decode(&genesis_block.payload)
            .context("Cannot decode genesis block payload")
            .map_err(StorageError::Database)?;
        let actual_payload: consensus::Payload =
            block.try_into().map_err(StorageError::Database)?;
        if actual_payload != expected_payload {
            let err = anyhow::anyhow!(
                "Genesis block payload from Postgres {actual_payload:?} does not match the configured one \
                 {expected_payload:?}"
            );
            return Err(StorageError::Database(err));
        }

        let expected_consensus_fields = ConsensusBlockFields {
            parent: genesis_block.header.parent,
            justification: genesis_block.justification.clone(),
        };
        if let Some(actual_consensus_fields) = &actual_consensus_fields {
            let actual_consensus_fields = ConsensusBlockFields::decode(actual_consensus_fields)
                .context("ConsensusBlockFields::decode()")
                .map_err(StorageError::Database)?;
            // While justifications may differ among nodes for an arbitrary block, we assume that
            // the genesis block has a hardcoded justification.
            if actual_consensus_fields != expected_consensus_fields {
                let err = anyhow::anyhow!(
                    "Genesis block consensus fields in Postgres {actual_consensus_fields:?} do not match \
                     the configured ones {expected_consensus_fields:?}"
                );
                return Err(StorageError::Database(err));
            }
        } else {
            tracing::info!(
                "Postgres doesn't have consensus fields for genesis block; saving {expected_consensus_fields:?}"
            );
            ctx.wait(storage.blocks_dal().set_miniblock_consensus_fields(
                MiniblockNumber(block_number),
                &expected_consensus_fields,
            ))
            .await?
            .context("Failed saving consensus fields for genesis block")
            .map_err(StorageError::Database)?;
        }
        Ok(())
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

    async fn sync_block(
        ctx: &ctx::Ctx,
        storage: &mut StorageProcessor<'_>,
        number: MiniblockNumber,
    ) -> StorageResult<Option<SyncBlock>> {
        ctx.wait(storage.sync_dal().sync_block(number, true))
            .await?
            .with_context(|| format!("Failed getting miniblock #{number} from Postgres"))
            .map_err(StorageError::Database)
    }

    async fn block(
        ctx: &ctx::Ctx,
        storage: &mut StorageProcessor<'_>,
        number: MiniblockNumber,
    ) -> StorageResult<Option<FinalBlock>> {
        let Some(block) = Self::sync_block(ctx, storage, number).await? else {
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
        Self::block(ctx, &mut storage, self.first_block_number)
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
        let Ok(number) = u32::try_from(number.0) else {
            return Ok(None);
        };
        let number = MiniblockNumber(number);
        if number < self.first_block_number {
            return Ok(None);
        }
        let mut storage = self.storage(ctx).await?;
        Self::block(ctx, &mut storage, number).await
    }

    async fn missing_block_numbers(
        &self,
        ctx: &ctx::Ctx,
        range: ops::Range<BlockNumber>,
    ) -> StorageResult<Vec<BlockNumber>> {
        let mut output = vec![];
        let first_block_number = u64::from(self.first_block_number.0);
        let numbers_before_first_block = (range.start.0..first_block_number).map(BlockNumber);
        output.extend(numbers_before_first_block);

        let last_block_number = self.sealed_miniblock_number(ctx).await?;
        let numbers_after_last_block = (last_block_number.next().0..range.end.0).map(BlockNumber);
        output.extend(numbers_after_last_block);

        // By design, no blocks are missing in the `first_block_number..=last_block_number` range.
        Ok(output)
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

//! Storage implementation based on DAL.
use std::ops;

use crate::consensus::storage::CtxStorage;
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
use zksync_dal::{consensus_dal::Payload,ConnectionPool, StorageProcessor};
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

struct Block {
    fetched: FetchedBlock,
    certificate: validator::CommitQC,
}

struct Cursor {
    
}

impl PostgresBlockStorage { 
}

/// Postgres-based [`BlockStore`] implementation. New blocks are scheduled to be written via
/// [`ContiguousBlockStore`] trait, which internally uses an [`ActionQueueSender`] to queue
/// block data (miniblock and L1 batch parameters, transactions) for the state keeper. Block data processing
/// is shared with JSON-RPC-based syncing.
#[derive(Debug)]
pub(super) struct PostgresBlockStorage {
    inner: consensus::storage::SignedBlockStorage,
    cursor: FetcherCursor,
    actions: ActionQueueSender,
}

impl PostgresBlockStorage {
    /// Creates a new storage handle. `pool` should have multiple connections to work efficiently.
    pub async fn new(
        ctx: &ctx::Ctx,
        pool: ConnectionPool,
        actions: ActionQueueSender,
        operator_address: Address,
    ) -> ctx::Result<Self> {
        let cursor = FetcherCursor::new(&mut storage)
            .await
            .context("FetcherCursor::new()")?;
        // TODO: check genesis
    }

    pub async fn process_block(&mut self, ctx: &ctx::Ctx, block: validator::FinalBlock) -> ctx::Result<()> {
        let number = MiniblockNumber(u32::try_from(block.header().number.0)
            .context("Integer overflow converting block number")?);
        let payload = Payload::decode(&block.payload)
            .context("Failed deserializing block payload")?;
        if self.cursor.next_miniblock <= number {
            let block = FetchedBlock {
                number,
                l1_batch_number: payload.l1_batch_number,
                last_in_batch: payload.last_in_batch,
                protocol_version: payload.protocol_version,
                timestamp: payload.timestamp,
                reference_hash: Some(payload.hash),
                l1_gas_price: payload.l1_gas_price,
                l2_fair_gas_price: payload.l2_fair_gas_price,
                virtual_blocks: payload.virtual_blocks,
                operator_address: payload.operator_address,
                transactions: payload.transactions,
            };
            self.actions.push_actions(self.cursor.advance(block)).await;
        }
        const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(50);
        loop {
            let mut storage = CtxStorage::access(ctx, &self.pool).await.wrap("access()")?;
            let number = storage.last_miniblock_number(ctx).await.wrap("last_miniblock_number()")?;
            if number >= block.justification.message.proposal.number {
                storage.insert_certificate(ctx, &block.justification, payload.operator_address).await.wrap("insert_certificate()")?;
                return Ok(());
            }
            ctx.sleep(POLL_INTERVAL).await?;
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
}

#[async_trait::async_trait]
impl BlockStore for PostgresBlockStorage {
    async fn head_block(&self, ctx: &ctx::Ctx) -> ctx::Result<validator::FinalBlock> {
        // TODO: consider head.
        self.inner.head_block(ctx).await
    }

    async fn first_block(&self, ctx: &ctx::Ctx) -> ctx::Result<validator::FinalBlock> {
        self.inner.first_block(ctx).await
    }

    async fn last_contiguous_block_number(&self, ctx: &ctx::Ctx) -> ctx::Result<validator::BlockNumber> {
        self.inner.last_contiguous_block_number(ctx).await
    }

    async fn block(&self, ctx: &ctx::Ctx, number: validator::BlockNumber) -> ctx::Result<Option<validator::FinalBlock>> {
        // TODO: consider head.
        self.inner.block(ctx,number).await 
    }

    async fn missing_block_numbers(&self, ctx: &ctx::Ctx, range: ops::Range<validator::BlockNumber>) -> ctx::Result<Vec<validator::BlockNumber>> {
        self.inner.missing_block_numbers(ctx,range).await
    }

    fn subscribe_to_block_writes(&self) -> watch::Receiver<validator::BlockNumber> {
        self.inner.subscribe_to_block_writes()
    }
}

#[async_trait::async_trait]
impl WriteBlockStore for PostgresBlockStorage {
    async fn verify_payload(&self, ctx: &ctx::Ctx, block_number: validator::BlockNumber, payload: &validator::Payload) -> ctx::Result<()> {
        self.inner.verify_payload(ctx,block_number,payload).await
    }

    async fn put_block(&self, ctx: &ctx::Ctx, block: &validator::FinalBlock) -> ctx::Result<()> {
        // last_in_batch` is always set to `false` by this call; it is properly set by `CursorWithCachedBlock`.
        let fetched_block = FetchedBlock::from_gossip_block(block).context("from_gossip_block()")?;
        let actions = sync::lock(ctx, &self.cursor).await?.advance(fetched_block);
        for actions_chunk in actions {
            // We don't wrap this in `ctx.wait()` because `PostgresBlockStorage` will get broken
            // if it gets reused after context cancellation.
        }
        Ok(())
    }
}

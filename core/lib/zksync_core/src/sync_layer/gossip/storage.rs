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

use super::{buffered::ContiguousBlockStore, conversions::sync_block_to_consensus_block};
use crate::sync_layer::{
    fetcher::{FetchedBlock, FetcherCursor},
    sync_action::ActionQueueSender,
};

#[derive(Debug)]
pub(super) struct PostgresBlockStore {
    pool: ConnectionPool,
    actions: ActionQueueSender,
    block_sender: watch::Sender<BlockNumber>,
    cursor: Mutex<FetcherCursor>,
}

impl PostgresBlockStore {
    pub fn new(pool: ConnectionPool, actions: ActionQueueSender, cursor: FetcherCursor) -> Self {
        Self {
            pool,
            actions,
            block_sender: watch::channel(BlockNumber(cursor.miniblock.0.into())).0,
            cursor: Mutex::new(cursor),
        }
    }

    pub async fn listen_to_updates(&self, ctx: &ctx::Ctx) -> StorageResult<()> {
        const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(50);
        loop {
            let sealed_miniblock_number = self
                .sealed_miniblock_number()
                .await
                .map_err(StorageError::Database)?;
            self.block_sender.send_if_modified(|number| {
                if *number != sealed_miniblock_number {
                    *number = sealed_miniblock_number;
                    true
                } else {
                    false
                }
            });
            ctx.sleep(POLL_INTERVAL).await?;
        }
    }

    async fn head_block(&self) -> anyhow::Result<FinalBlock> {
        let mut storage = self.storage().await?;
        let miniblock_number = storage
            .blocks_dal()
            .get_sealed_miniblock_number()
            .await
            .context("Failed getting sealed miniblock number")?;
        // ^ The number can get stale, but it's OK for our purposes
        Self::block(&mut storage, miniblock_number)
            .await?
            .with_context(|| format!("Miniblock #{miniblock_number} disappeared from Postgres"))
    }

    async fn storage(&self) -> anyhow::Result<StorageProcessor<'_>> {
        self.pool
            .access_storage_tagged("sync_layer")
            .await
            .context("Failed to connect to Postgres")
    }

    async fn block(
        storage: &mut StorageProcessor<'_>,
        number: MiniblockNumber,
    ) -> anyhow::Result<Option<FinalBlock>> {
        let Some(block) = storage
            .sync_dal()
            .sync_block(number, Address::default(), true)
            .await
            .with_context(|| format!("Failed getting miniblock #{number} from Postgres"))?
        else {
            return Ok(None);
        };
        Ok(Some(sync_block_to_consensus_block(block)))
    }

    async fn first_block(&self) -> anyhow::Result<FinalBlock> {
        let mut storage = self.storage().await?;
        Self::block(&mut storage, MiniblockNumber(0))
            .await?
            .context("Genesis miniblock not present in Postgres")
    }

    async fn sealed_miniblock_number(&self) -> anyhow::Result<BlockNumber> {
        let mut storage = self.storage().await?;
        let number = storage
            .blocks_dal()
            .get_sealed_miniblock_number()
            .await
            .context("Failed getting sealed miniblock number")?;
        Ok(BlockNumber(number.0.into()))
    }

    async fn schedule_block(&self, ctx: &ctx::Ctx, block: &FinalBlock) -> StorageResult<()> {
        let fetched_block =
            FetchedBlock::from_gossip_block(block).map_err(StorageError::Database)?;
        let actions = sync::lock(ctx, &self.cursor).await?.advance(fetched_block);
        tokio::select! {
            () = ctx.canceled() => Err(ctx::Canceled.into()),
            () = self.actions.push_actions(actions) => Ok(()),
        }
    }
}

#[async_trait]
impl BlockStore for PostgresBlockStore {
    async fn head_block(&self, ctx: &ctx::Ctx) -> StorageResult<FinalBlock> {
        tokio::select! {
            () = ctx.canceled() => Err(ctx::Canceled.into()),
            result = self.head_block() => result.map_err(StorageError::Database),
        }
    }

    async fn first_block(&self, ctx: &ctx::Ctx) -> StorageResult<FinalBlock> {
        tokio::select! {
            () = ctx.canceled() => Err(ctx::Canceled.into()),
            result = self.first_block() => result.map_err(StorageError::Database),
        }
    }

    async fn last_contiguous_block_number(&self, ctx: &ctx::Ctx) -> StorageResult<BlockNumber> {
        tokio::select! {
            () = ctx.canceled() => Err(ctx::Canceled.into()),
            result = self.sealed_miniblock_number() => result.map_err(StorageError::Database),
        }
    }

    async fn block(
        &self,
        ctx: &ctx::Ctx,
        number: BlockNumber,
    ) -> StorageResult<Option<FinalBlock>> {
        let get_block = async {
            let number = u32::try_from(number.0).context("block number is too large")?;
            let mut storage = self.storage().await?;
            Self::block(&mut storage, MiniblockNumber(number)).await
        };
        tokio::select! {
            () = ctx.canceled() => Err(ctx::Canceled.into()),
            result = get_block => result.map_err(StorageError::Database),
        }
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
impl ContiguousBlockStore for PostgresBlockStore {
    async fn schedule_next_block(&self, ctx: &ctx::Ctx, block: &FinalBlock) -> StorageResult<()> {
        self.schedule_block(ctx, block).await.map_err(Into::into)
    }
}

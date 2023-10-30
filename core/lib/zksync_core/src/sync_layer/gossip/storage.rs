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

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use zksync_concurrency::{scope, time};
    use zksync_types::{L1BatchNumber, L2ChainId};

    use super::*;
    use crate::{
        genesis::{ensure_genesis_state, GenesisParams},
        sync_layer::{
            sync_action::SyncAction, tests::run_state_keeper_with_multiple_miniblocks, ActionQueue,
        },
    };

    const TEST_TIMEOUT: time::Duration = time::Duration::seconds(1);
    const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(50);

    #[tokio::test]
    async fn block_store_basics_for_postgres() {
        let pool = ConnectionPool::test_pool().await;
        run_state_keeper_with_multiple_miniblocks(pool.clone()).await;

        let mut storage = pool.access_storage().await.unwrap();
        let cursor = FetcherCursor::new(&mut storage).await.unwrap();
        drop(storage);
        let (actions_sender, _) = ActionQueue::new();
        let storage = PostgresBlockStore::new(pool.clone(), actions_sender, cursor);

        let ctx = &ctx::test_root(&ctx::RealClock);
        let genesis_block = BlockStore::first_block(&storage, ctx).await.unwrap();
        assert_eq!(genesis_block.header.number, BlockNumber(0));
        let head_block = BlockStore::head_block(&storage, ctx).await.unwrap();
        assert_eq!(head_block.header.number, BlockNumber(2));
        let last_contiguous_block_number = storage.last_contiguous_block_number(ctx).await.unwrap();
        assert_eq!(last_contiguous_block_number, BlockNumber(2));

        let block = storage
            .block(ctx, BlockNumber(1))
            .await
            .unwrap()
            .expect("no block #1");
        assert_eq!(block.header.number, BlockNumber(1));
        let missing_block = storage.block(ctx, BlockNumber(3)).await.unwrap();
        assert!(missing_block.is_none(), "{missing_block:?}");
    }

    #[tokio::test]
    async fn subscribing_to_block_updates_for_postgres() {
        let pool = ConnectionPool::test_pool().await;
        let mut storage = pool.access_storage().await.unwrap();
        if storage.blocks_dal().is_genesis_needed().await.unwrap() {
            ensure_genesis_state(&mut storage, L2ChainId::default(), &GenesisParams::mock())
                .await
                .unwrap();
        }
        let cursor = FetcherCursor::new(&mut storage).await.unwrap();
        // ^ This is logically incorrect (the storage should not be updated other than using
        // `ContiguousBlockStore`), but for testing subscriptions this is fine.
        drop(storage);
        let (actions_sender, _) = ActionQueue::new();
        let storage = PostgresBlockStore::new(pool.clone(), actions_sender, cursor);
        let mut subscriber = storage.subscribe_to_block_writes();

        let ctx = &ctx::test_root(&ctx::RealClock);
        scope::run!(&ctx.with_timeout(TEST_TIMEOUT), |ctx, s| async {
            s.spawn_bg(async {
                match storage.listen_to_updates(ctx).await {
                    Ok(()) | Err(StorageError::Canceled(_)) => Ok(()),
                    Err(err) => Err(err.into()),
                }
            });
            s.spawn(async {
                run_state_keeper_with_multiple_miniblocks(pool.clone()).await;
                Ok(())
            });

            loop {
                let block = *sync::changed(ctx, &mut subscriber).await?;
                if block == BlockNumber(2) {
                    // We should receive at least the last update.
                    break;
                }
            }
            anyhow::Ok(())
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn processing_new_blocks() {
        let pool = ConnectionPool::test_pool().await;
        run_state_keeper_with_multiple_miniblocks(pool.clone()).await;

        let mut storage = pool.access_storage().await.unwrap();
        let first_block = storage
            .sync_dal()
            .sync_block(MiniblockNumber(1), Address::repeat_byte(1), true)
            .await
            .unwrap()
            .expect("no sync block #1");
        let first_block = sync_block_to_consensus_block(first_block);
        let second_block = storage
            .sync_dal()
            .sync_block(MiniblockNumber(2), Address::repeat_byte(1), true)
            .await
            .unwrap()
            .expect("no sync block #2");
        let second_block = sync_block_to_consensus_block(second_block);
        storage
            .transactions_dal()
            .reset_transactions_state(MiniblockNumber(0))
            .await;
        storage
            .blocks_dal()
            .delete_miniblocks(MiniblockNumber(0))
            .await
            .unwrap();
        let cursor = FetcherCursor::new(&mut storage).await.unwrap();
        drop(storage);

        let (actions_sender, mut actions) = ActionQueue::new();
        let storage = PostgresBlockStore::new(pool.clone(), actions_sender, cursor);
        let ctx = &ctx::test_root(&ctx::RealClock);
        storage.schedule_block(ctx, &first_block).await.unwrap();

        scope::run!(&ctx.with_timeout(TEST_TIMEOUT), |ctx, _| async {
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
            anyhow::Ok(())
        })
        .await
        .unwrap();

        storage.schedule_block(ctx, &second_block).await.unwrap();

        scope::run!(&ctx.with_timeout(TEST_TIMEOUT), |ctx, _| async {
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
            anyhow::Ok(())
        })
        .await
        .unwrap();
    }
}

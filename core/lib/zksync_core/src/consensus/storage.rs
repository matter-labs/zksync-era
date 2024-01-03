//! Storage implementation based on DAL.
use anyhow::Context as _;
use zksync_concurrency::{ctx, error::Wrap as _, sync, time};
use zksync_consensus_bft::PayloadManager;
use zksync_consensus_roles::validator;
use zksync_consensus_storage::{BlockStoreState, PersistentBlockStore, ReplicaState, ReplicaStore};
use zksync_dal::{consensus_dal::Payload, ConnectionPool};
use zksync_types::{Address, MiniblockNumber};

use crate::{
    consensus,
    sync_layer::{
        fetcher::{FetchedBlock, FetcherCursor},
        sync_action::ActionQueueSender,
    },
};

/// Context-aware `zksync_dal::StorageProcessor` wrapper.
pub(super) struct CtxStorage<'a>(zksync_dal::StorageProcessor<'a>);

impl<'a> CtxStorage<'a> {
    /// Wrapper for `access_storage_tagged()`.
    pub async fn access(ctx: &ctx::Ctx, pool: &'a ConnectionPool) -> ctx::Result<CtxStorage<'a>> {
        Ok(Self(
            ctx.wait(pool.access_storage_tagged("consensus")).await??,
        ))
    }

    /// Wrapper for `start_transaction()`.
    pub async fn start_transaction<'b, 'c: 'b>(
        &'c mut self,
        ctx: &ctx::Ctx,
    ) -> ctx::Result<CtxStorage<'b>> {
        Ok(CtxStorage(
            ctx.wait(self.0.start_transaction())
                .await?
                .context("sqlx")?,
        ))
    }

    /// Wrapper for `blocks_dal().get_sealed_miniblock_number()`.
    pub async fn last_miniblock_number(
        &mut self,
        ctx: &ctx::Ctx,
    ) -> ctx::Result<validator::BlockNumber> {
        let number = ctx
            .wait(self.0.blocks_dal().get_sealed_miniblock_number())
            .await?
            .context("sqlx")?;
        Ok(validator::BlockNumber(number.0.into()))
    }

    /// Wrapper for `commit()`.
    pub async fn commit(self, ctx: &ctx::Ctx) -> ctx::Result<()> {
        Ok(ctx.wait(self.0.commit()).await?.context("sqlx")?)
    }

    /// Wrapper for `consensus_dal().block_payload()`.
    pub async fn payload(
        &mut self,
        ctx: &ctx::Ctx,
        number: validator::BlockNumber,
        operator_addr: Address,
    ) -> ctx::Result<Option<Payload>> {
        Ok(ctx
            .wait(self.0.consensus_dal().block_payload(number, operator_addr))
            .await??)
    }

    /// Wrapper for `consensus_dal().first_certificate()`.
    pub async fn first_certificate(
        &mut self,
        ctx: &ctx::Ctx,
    ) -> ctx::Result<Option<validator::CommitQC>> {
        Ok(ctx
            .wait(self.0.consensus_dal().first_certificate())
            .await??)
    }

    /// Wrapper for `consensus_dal().last_certificate()`.
    pub async fn last_certificate(
        &mut self,
        ctx: &ctx::Ctx,
    ) -> ctx::Result<Option<validator::CommitQC>> {
        Ok(ctx
            .wait(self.0.consensus_dal().last_certificate())
            .await??)
    }

    /// Wrapper for `consensus_dal().certificate()`.
    pub async fn certificate(
        &mut self,
        ctx: &ctx::Ctx,
        number: validator::BlockNumber,
    ) -> ctx::Result<Option<validator::CommitQC>> {
        Ok(ctx
            .wait(self.0.consensus_dal().certificate(number))
            .await??)
    }

    /// Wrapper for `consensus_dal().insert_certificate()`.
    pub async fn insert_certificate(
        &mut self,
        ctx: &ctx::Ctx,
        cert: &validator::CommitQC,
        operator_addr: Address,
    ) -> ctx::Result<()> {
        Ok(ctx
            .wait(
                self.0
                    .consensus_dal()
                    .insert_certificate(cert, operator_addr),
            )
            .await??)
    }

    /// Wrapper for `consensus_dal().replica_state()`.
    pub async fn replica_state(&mut self, ctx: &ctx::Ctx) -> ctx::Result<Option<ReplicaState>> {
        Ok(ctx.wait(self.0.consensus_dal().replica_state()).await??)
    }

    /// Wrapper for `consensus_dal().set_replica_state()`.
    pub async fn set_replica_state(
        &mut self,
        ctx: &ctx::Ctx,
        state: &ReplicaState,
    ) -> ctx::Result<()> {
        Ok(ctx
            .wait(self.0.consensus_dal().set_replica_state(state))
            .await?
            .context("sqlx")?)
    }

    /// Wrapper for `FetcherCursor::new()`.
    pub async fn new_fetcher_cursor(&mut self, ctx: &ctx::Ctx) -> ctx::Result<FetcherCursor> {
        Ok(ctx.wait(FetcherCursor::new(&mut self.0)).await??)
    }
}

#[derive(Debug)]
struct Cursor {
    inner: FetcherCursor,
    actions: ActionQueueSender,
}

/// Wrapper of `ConnectionPool` implementing `ReplicaStore` and `PayloadManager`.
#[derive(Clone, Debug)]
pub(super) struct Store {
    pool: ConnectionPool,
    operator_addr: Address,
}

/// Wrapper of `ConnectionPool` implementing `PersistentBlockStore`.
#[derive(Debug)]
pub(super) struct BlockStore {
    inner: Store,
    cursor: sync::Mutex<Option<Cursor>>,
}

impl Store {
    /// Creates a `Store`. `pool` should have multiple connections to work efficiently.
    pub fn new(pool: ConnectionPool, operator_addr: Address) -> Self {
        Self {
            pool,
            operator_addr,
        }
    }

    /// Converts `Store` into a `BlockStore`.
    pub fn into_block_store(self) -> BlockStore {
        BlockStore {
            inner: self,
            cursor: sync::Mutex::new(None),
        }
    }
}

impl BlockStore {
    /// Generates and stores the genesis cert (signed by `validator_key`) for the last sealed miniblock.
    /// Noop if db already contains a genesis cert.
    pub async fn try_init_genesis(
        &mut self,
        ctx: &ctx::Ctx,
        validator_key: &validator::SecretKey,
    ) -> ctx::Result<()> {
        let mut storage = CtxStorage::access(ctx, &self.inner.pool)
            .await
            .wrap("access()")?;
        // Fetch last miniblock number outside of the transaction to avoid taking a lock.
        let number = storage
            .last_miniblock_number(ctx)
            .await
            .wrap("last_miniblock_number()")?;

        let mut txn = storage
            .start_transaction(ctx)
            .await
            .wrap("start_transaction()")?;
        if txn
            .first_certificate(ctx)
            .await
            .wrap("first_certificate()")?
            .is_some()
        {
            return Ok(());
        }
        let payload = txn
            .payload(ctx, number, self.inner.operator_addr)
            .await
            .wrap("payload()")?
            .context("miniblock disappeared")?;
        let (genesis, _) = zksync_consensus_bft::testonly::make_genesis(
            &[validator_key.clone()],
            payload.encode(),
            number,
        );
        txn.insert_certificate(ctx, &genesis.justification, self.inner.operator_addr)
            .await
            .wrap("insert_certificate()")?;
        txn.commit(ctx).await.wrap("commit()")
    }

    /// Sets an `ActionQueueSender` in the `BlockStore`. See `store_next_block()` for details.
    pub async fn set_actions_queue(
        &mut self,
        ctx: &ctx::Ctx,
        actions: ActionQueueSender,
    ) -> ctx::Result<()> {
        let mut storage = CtxStorage::access(ctx, &self.inner.pool)
            .await
            .wrap("access()")?;
        let inner = storage
            .new_fetcher_cursor(ctx)
            .await
            .wrap("new_fetcher_cursor()")?;
        *sync::lock(ctx, &self.cursor).await? = Some(Cursor { inner, actions });
        Ok(())
    }
}

#[async_trait::async_trait]
impl PersistentBlockStore for BlockStore {
    async fn state(&self, ctx: &ctx::Ctx) -> ctx::Result<Option<BlockStoreState>> {
        // Ensure that genesis block has consensus field set in postgres.
        let mut storage = CtxStorage::access(ctx, &self.inner.pool)
            .await
            .wrap("access()")?;
        let Some(first) = storage
            .first_certificate(ctx)
            .await
            .wrap("first_certificate()")?
        else {
            return Ok(None);
        };
        let last = storage
            .last_certificate(ctx)
            .await
            .wrap("last_certificate()")?
            .context("genesis block disappeared from db")?;
        Ok(Some(BlockStoreState { first, last }))
    }

    async fn block(
        &self,
        ctx: &ctx::Ctx,
        number: validator::BlockNumber,
    ) -> ctx::Result<Option<validator::FinalBlock>> {
        let storage = &mut CtxStorage::access(ctx, &self.inner.pool)
            .await
            .wrap("access()")?;
        let Some(justification) = storage
            .certificate(ctx, number)
            .await
            .wrap("certificate()")?
        else {
            return Ok(None);
        };
        let payload = storage
            .payload(ctx, number, self.inner.operator_addr)
            .await
            .wrap("payload()")?
            .context("miniblock disappeared from storage")?;
        Ok(Some(validator::FinalBlock {
            payload: payload.encode(),
            justification,
        }))
    }

    /// If actions queue is set (and the block has not been stored yet),
    /// the block will be translated into a sequence of actions.
    /// The received actions should be fed
    /// to `ExternalIO`, so that `StateKeeper` will store the corresponding miniblock in the db.
    ///
    /// `store_next_block()` call will wait synchronously for the miniblock.
    /// Once miniblock is observed in storage, `store_next_block()` will store a cert for this
    /// miniblock.
    async fn store_next_block(
        &self,
        ctx: &ctx::Ctx,
        block: &validator::FinalBlock,
    ) -> ctx::Result<()> {
        // This mutex prevents concurrent store_next_block calls.
        let mut cursor = ctx.wait(self.cursor.lock()).await?;
        if let Some(cursor) = &mut *cursor {
            let number = MiniblockNumber(
                u32::try_from(block.header().number.0)
                    .context("Integer overflow converting block number")?,
            );
            let payload =
                Payload::decode(&block.payload).context("Failed deserializing block payload")?;
            if cursor.inner.next_miniblock <= number {
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
                cursor
                    .actions
                    .push_actions(cursor.inner.advance(block))
                    .await;
            }
        }
        const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(50);
        loop {
            let mut storage = CtxStorage::access(ctx, &self.inner.pool)
                .await
                .wrap("access()")?;
            let number = storage
                .last_miniblock_number(ctx)
                .await
                .wrap("last_miniblock_number()")?;
            if number >= block.header().number {
                storage
                    .insert_certificate(ctx, &block.justification, self.inner.operator_addr)
                    .await
                    .wrap("insert_certificate()")?;
                return Ok(());
            }
            drop(storage);
            ctx.sleep(POLL_INTERVAL).await?;
        }
    }
}

#[async_trait::async_trait]
impl ReplicaStore for Store {
    async fn state(&self, ctx: &ctx::Ctx) -> ctx::Result<Option<ReplicaState>> {
        let storage = &mut CtxStorage::access(ctx, &self.pool).await.wrap("access()")?;
        storage.replica_state(ctx).await.wrap("replica_state()")
    }

    async fn set_state(&self, ctx: &ctx::Ctx, state: &ReplicaState) -> ctx::Result<()> {
        let storage = &mut CtxStorage::access(ctx, &self.pool).await.wrap("access()")?;
        storage
            .set_replica_state(ctx, state)
            .await
            .wrap("set_replica_state()")
    }
}

#[async_trait::async_trait]
impl PayloadManager for Store {
    /// Currently (for the main node) proposing is implemented as just converting a miniblock from db (without a cert) into a
    /// payload.
    async fn propose(
        &self,
        ctx: &ctx::Ctx,
        block_number: validator::BlockNumber,
    ) -> ctx::Result<validator::Payload> {
        const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(50);
        let mut storage = CtxStorage::access(ctx, &self.pool).await.wrap("access()")?;
        storage
            .certificate(ctx, block_number.prev())
            .await
            .wrap("certificate()")?
            .with_context(|| format!("parent of {block_number:?} is missing"))?;
        drop(storage);
        loop {
            let mut storage = CtxStorage::access(ctx, &self.pool).await.wrap("access()")?;
            if let Some(payload) = storage
                .payload(ctx, block_number, self.operator_addr)
                .await
                .wrap("payload()")?
            {
                return Ok(payload.encode());
            }
            drop(storage);
            ctx.sleep(POLL_INTERVAL).await?;
        }
    }

    /// Verify that `payload` is a correct proposal for the block `block_number`.
    /// Currently (for the main node) it is implemented as checking whether the received payload
    /// matches the miniblock in the db.
    async fn verify(
        &self,
        ctx: &ctx::Ctx,
        block_number: validator::BlockNumber,
        payload: &validator::Payload,
    ) -> ctx::Result<()> {
        let want = self.propose(ctx, block_number).await?;
        let want = consensus::Payload::decode(&want).context("Payload::decode(want)")?;
        let got = consensus::Payload::decode(payload).context("Payload::decode(got)")?;
        if got != want {
            return Err(anyhow::anyhow!("unexpected payload: got {got:?} want {want:?}").into());
        }
        Ok(())
    }
}

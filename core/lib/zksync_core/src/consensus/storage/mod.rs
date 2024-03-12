//! Storage implementation based on DAL.

use anyhow::Context as _;
use zksync_concurrency::{ctx, error::Wrap as _, sync, time};
use zksync_consensus_bft::PayloadManager;
use zksync_consensus_roles::validator;
use zksync_consensus_storage::{PersistentBlockStore, ReplicaState, ReplicaStore};
use zksync_dal::{consensus_dal::Payload, ConnectionPool};
use zksync_types::MiniblockNumber;

#[cfg(test)]
mod testonly;

use crate::{
    state_keeper::io::common::IoCursor,
    sync_layer::{
        fetcher::{FetchedBlock, FetchedTransaction},
        sync_action::ActionQueueSender,
    },
};

/// Context-aware `zksync_dal::StorageProcessor` wrapper.
pub(super) struct Connection<'a>(pub(super) zksync_dal::StorageProcessor<'a>);

impl<'a> Connection<'a> {
    /// Wrapper for `start_transaction()`.
    pub async fn start_transaction<'b, 'c: 'b>(
        &'c mut self,
        ctx: &ctx::Ctx,
    ) -> ctx::Result<Connection<'b>> {
        Ok(Connection(
            ctx.wait(self.0.start_transaction())
                .await?
                .context("sqlx")?,
        ))
    }

    /// Wrapper for `commit()`.
    pub async fn commit(self, ctx: &ctx::Ctx) -> ctx::Result<()> {
        Ok(ctx.wait(self.0.commit()).await?.context("sqlx")?)
    }

    /// Wrapper for `blocks_dal().get_sealed_miniblock_number()`.
    pub async fn last_miniblock_number(
        &mut self,
        ctx: &ctx::Ctx,
    ) -> ctx::Result<Option<validator::BlockNumber>> {
        Ok(ctx
            .wait(self.0.blocks_dal().get_sealed_miniblock_number())
            .await?
            .context("sqlx")?
            .map(|n| validator::BlockNumber(n.0.into())))
    }

    /// Wrapper for `consensus_dal().block_payload()`.
    pub async fn payload(
        &mut self,
        ctx: &ctx::Ctx,
        number: validator::BlockNumber,
    ) -> ctx::Result<Option<Payload>> {
        Ok(ctx
            .wait(self.0.consensus_dal().block_payload(number))
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
    ) -> ctx::Result<()> {
        Ok(ctx
            .wait(self.0.consensus_dal().insert_certificate(cert))
            .await??)
    }

    /// Wrapper for `consensus_dal().replica_state()`.
    pub async fn replica_state(&mut self, ctx: &ctx::Ctx) -> ctx::Result<ReplicaState> {
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
    pub async fn new_fetcher_cursor(
        &mut self,
        ctx: &ctx::Ctx,
        actions: ActionQueueSender,
    ) -> ctx::Result<Cursor> {
        Ok(Cursor {
            inner: ctx.wait(IoCursor::for_fetcher(&mut self.0)).await??,
            actions,
        })
    }

    pub async fn genesis(&mut self, ctx: &ctx::Ctx) -> ctx::Result<Option<validator::Genesis>> {
        Ok(ctx.wait(self.0.consensus_dal().genesis()).await??)
    }

    pub async fn try_update_genesis(
        &mut self,
        ctx: &ctx::Ctx,
        genesis: &validator::Genesis,
    ) -> ctx::Result<()> {
        Ok(ctx
            .wait(self.0.consensus_dal().try_update_genesis(genesis))
            .await??)
    }
}

#[derive(Debug)]
pub(super) struct Cursor {
    inner: IoCursor,
    actions: ActionQueueSender,
}

impl Cursor {
    pub(super) fn next(&self) -> validator::BlockNumber {
        validator::BlockNumber(self.inner.next_miniblock.0.into())
    }

    /// Advances the cursor by converting the block into actions and pushing them
    /// to the actions queue.
    /// Does nothing and returns Ok() if the block has been already processed.
    /// Returns an error if a block with an earlier block number was expected.
    pub(super) async fn advance(&mut self, block: FetchedBlock) -> anyhow::Result<()> {
        let want = self.inner.next_miniblock;
        // Some blocks are missing.
        if block.number > want {
            anyhow::bail!("expected {want:?}, got {:?}", block.number);
        }
        // Block already processed.
        if block.number < want {
            return Ok(());
        }
        self.actions.push_actions(self.inner.advance(block)).await;
        Ok(())
    }
}

/// Wrapper of `ConnectionPool` implementing `ReplicaStore` and `PayloadManager`.
#[derive(Clone, Debug)]
pub struct Store(pub ConnectionPool);

/// Wrapper of `ConnectionPool` implementing `PersistentBlockStore`.
#[derive(Debug)]
pub(super) struct BlockStore {
    inner: Store,
    /// Mutex preventing concurrent execution of `store_next_block` calls.
    store_next_block_mutex: sync::Mutex<Option<Cursor>>,
}

impl Store {
    /// Converts `Store` into a `BlockStore`.
    pub(super) fn into_block_store(self) -> BlockStore {
        BlockStore {
            inner: self,
            store_next_block_mutex: sync::Mutex::new(None),
        }
    }

    /// Wrapper for `access_storage_tagged()`.
    pub(super) async fn access<'a>(&'a self, ctx: &ctx::Ctx) -> ctx::Result<Connection<'a>> {
        Ok(Connection(
            ctx.wait(self.0.access_storage_tagged("consensus"))
                .await??,
        ))
    }
}

impl BlockStore {
    /// Initializes consensus genesis (with 1 validator) to start at the last miniblock in storage.
    /// No-op if db already contains a genesis.
    pub async fn try_init_genesis(
        &mut self,
        ctx: &ctx::Ctx,
        validator_key: &validator::PublicKey,
    ) -> ctx::Result<()> {
        let mut conn = self.inner.access(ctx).await.wrap("access()")?;
        // Fetch last miniblock number before starting the transaction
        // to avoid taking lock on the miniblocks table.
        let first_block = conn
            .last_miniblock_number(ctx)
            .await
            .wrap("last_miniblock_number()")?
            .unwrap_or(validator::BlockNumber(0));
        let mut txn = conn
            .start_transaction(ctx)
            .await
            .wrap("start_transaction()")?;
        if txn.genesis(ctx).await.wrap("genesis()")?.is_some() {
            return Ok(());
        }
        let genesis = validator::Genesis {
            // `ValidatorSet::new()` with a single validator should never fail.
            validators: validator::ValidatorSet::new([validator_key.clone()]).unwrap(),
            fork: validator::Fork {
                number: validator::ForkNumber(0),
                first_block,
                first_parent: None,
            },
        };
        txn.try_update_genesis(ctx, &genesis)
            .await
            .wrap("try_update_genesis()")?;
        txn.commit(ctx).await.wrap("commit()")?;
        Ok(())
    }

    /// Sets a `Cursor` in the `BlockStore`. See `store_next_block()` for details.
    pub fn set_cursor(&mut self, cursor: Cursor) -> anyhow::Result<()> {
        *self.store_next_block_mutex.try_lock()? = Some(cursor);
        Ok(())
    }
}

#[async_trait::async_trait]
impl PersistentBlockStore for BlockStore {
    async fn genesis(&self, ctx: &ctx::Ctx) -> ctx::Result<validator::Genesis> {
        Ok(self
            .inner
            .access(ctx)
            .await
            .wrap("access()")?
            .genesis(ctx)
            .await
            .wrap("genesis()")?
            .context("genesis is missing")?)
    }

    async fn last(&self, ctx: &ctx::Ctx) -> ctx::Result<Option<validator::CommitQC>> {
        self.inner
            .access(ctx)
            .await
            .wrap("access()")?
            .last_certificate(ctx)
            .await
            .wrap("last_certificate()")
    }

    async fn block(
        &self,
        ctx: &ctx::Ctx,
        number: validator::BlockNumber,
    ) -> ctx::Result<validator::FinalBlock> {
        let conn = &mut self.inner.access(ctx).await.wrap("access()")?;
        let justification = conn
            .certificate(ctx, number)
            .await
            .wrap("certificate()")?
            .context("not found")?;
        let payload = conn
            .payload(ctx, number)
            .await
            .wrap("payload()")?
            .context("miniblock disappeared from storage")?;
        Ok(validator::FinalBlock {
            payload: payload.encode(),
            justification,
        })
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
        // This mutex prevents concurrent `store_next_block` calls.
        let mut guard = ctx.wait(self.store_next_block_mutex.lock()).await?;
        if let Some(cursor) = &mut *guard {
            let number = MiniblockNumber(
                block
                    .number()
                    .0
                    .try_into()
                    .context("Integer overflow converting block number")?,
            );
            let payload = Payload::decode(&block.payload).context("Payload::decode()")?;
            let block = FetchedBlock {
                number,
                l1_batch_number: payload.l1_batch_number,
                last_in_batch: payload.last_in_batch,
                protocol_version: payload.protocol_version,
                timestamp: payload.timestamp,
                reference_hash: Some(payload.hash),
                l1_gas_price: payload.l1_gas_price,
                l2_fair_gas_price: payload.l2_fair_gas_price,
                fair_pubdata_price: payload.fair_pubdata_price,
                virtual_blocks: payload.virtual_blocks,
                operator_address: payload.operator_address,
                transactions: payload
                    .transactions
                    .into_iter()
                    .map(FetchedTransaction::new)
                    .collect(),
            };
            cursor.advance(block).await.context("cursor.advance()")?;
        }
        const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(50);
        loop {
            let mut conn = self.inner.access(ctx).await.wrap("access()")?;
            let last = conn
                .last_miniblock_number(ctx)
                .await
                .wrap("last_miniblock_number()")?;
            if let Some(last) = last {
                if last >= block.number() {
                    conn.insert_certificate(ctx, &block.justification)
                        .await
                        .wrap("insert_certificate()")?;
                    return Ok(());
                }
            }
            drop(conn);
            ctx.sleep(POLL_INTERVAL).await?;
        }
    }
}

#[async_trait::async_trait]
impl ReplicaStore for Store {
    async fn state(&self, ctx: &ctx::Ctx) -> ctx::Result<ReplicaState> {
        self.access(ctx)
            .await
            .wrap("access()")?
            .replica_state(ctx)
            .await
            .wrap("replica_state()")
    }

    async fn set_state(&self, ctx: &ctx::Ctx, state: &ReplicaState) -> ctx::Result<()> {
        self.access(ctx)
            .await
            .wrap("access()")?
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
        loop {
            let mut conn = self.access(ctx).await.wrap("access()")?;
            if let Some(payload) = conn.payload(ctx, block_number).await.wrap("payload()")? {
                let encoded_payload = payload.encode();
                if encoded_payload.0.len() > 1 << 20 {
                    tracing::warn!(
                        "large payload ({}B) with {} transactions",
                        encoded_payload.0.len(),
                        payload.transactions.len()
                    );
                }
                return Ok(encoded_payload);
            }
            drop(conn);
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
        let want = Payload::decode(&want).context("Payload::decode(want)")?;
        let got = Payload::decode(payload).context("Payload::decode(got)")?;
        if got != want {
            return Err(anyhow::anyhow!("unexpected payload: got {got:?} want {want:?}").into());
        }
        Ok(())
    }
}

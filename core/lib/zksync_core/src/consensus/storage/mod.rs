//! Storage implementation based on DAL.

use anyhow::Context as _;
use zksync_concurrency::{ctx, error::Wrap as _, sync, time};
use zksync_consensus_bft::PayloadManager;
use zksync_consensus_roles::validator;
use zksync_consensus_storage::{BlockStoreState, PersistentBlockStore, ReplicaState, ReplicaStore};
use zksync_dal::{consensus_dal::Payload, ConnectionPool, Core, CoreDal, DalError};
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

/// Context-aware `zksync_dal::Connection<Core>` wrapper.
pub(super) struct Connection<'a>(pub(super) zksync_dal::Connection<'a, Core>);

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

    /// Wrapper for `consensus_dal().block_range()`.
    pub async fn block_range(
        &mut self,
        ctx: &ctx::Ctx,
    ) -> ctx::Result<std::ops::Range<validator::BlockNumber>> {
        Ok(ctx
            .wait(self.0.consensus_dal().block_range())
            .await?
            .context("sqlx")?)
    }

    /// Wrapper for `consensus_dal().block_payload()`.
    pub async fn payload(
        &mut self,
        ctx: &ctx::Ctx,
        number: validator::BlockNumber,
    ) -> ctx::Result<Option<Payload>> {
        Ok(ctx
            .wait(self.0.consensus_dal().block_payload(number))
            .await?
            .map_err(DalError::generalize)?)
    }

    /// Wrapper for `consensus_dal().first_certificate()`.
    pub async fn first_certificate(
        &mut self,
        ctx: &ctx::Ctx,
    ) -> ctx::Result<Option<validator::CommitQC>> {
        Ok(ctx
            .wait(self.0.consensus_dal().first_certificate())
            .await?
            .map_err(DalError::generalize)?)
    }

    /// Wrapper for `consensus_dal().last_certificate()`.
    pub async fn last_certificate(
        &mut self,
        ctx: &ctx::Ctx,
    ) -> ctx::Result<Option<validator::CommitQC>> {
        Ok(ctx
            .wait(self.0.consensus_dal().last_certificate())
            .await?
            .map_err(DalError::generalize)?)
    }

    /// Wrapper for `consensus_dal().certificate()`.
    pub async fn certificate(
        &mut self,
        ctx: &ctx::Ctx,
        number: validator::BlockNumber,
    ) -> ctx::Result<Option<validator::CommitQC>> {
        Ok(ctx
            .wait(self.0.consensus_dal().certificate(number))
            .await?
            .map_err(DalError::generalize)?)
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
        Ok(ctx
            .wait(self.0.consensus_dal().replica_state())
            .await?
            .map_err(DalError::generalize)?)
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
        Ok(ctx
            .wait(self.0.consensus_dal().genesis())
            .await?
            .map_err(DalError::generalize)?)
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
pub struct Store(pub ConnectionPool<Core>);

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

    /// Wrapper for `connection_tagged()`.
    pub(super) async fn access<'a>(&'a self, ctx: &ctx::Ctx) -> ctx::Result<Connection<'a>> {
        Ok(Connection(
            ctx.wait(self.0.connection_tagged("consensus"))
                .await?
                .map_err(DalError::generalize)?,
        ))
    }

    /// Waits for the `number` miniblock.
    pub async fn wait_for_payload(
        &self,
        ctx: &ctx::Ctx,
        number: validator::BlockNumber,
    ) -> ctx::Result<Payload> {
        const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(50);
        loop {
            if let Some(payload) = self
                .access(ctx)
                .await
                .wrap("access()")?
                .payload(ctx, number)
                .await
                .wrap("payload()")?
            {
                return Ok(payload);
            }
            ctx.sleep(POLL_INTERVAL).await?;
        }
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
        let block_range = conn.block_range(ctx).await.wrap("block_range()")?;
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
                first_block: block_range.end,
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

    async fn state(&self, ctx: &ctx::Ctx) -> ctx::Result<BlockStoreState> {
        let mut conn = self.inner.access(ctx).await.wrap("access()")?;

        // Fetch the range of miniblocks in storage.
        let block_range = conn.block_range(ctx).await.context("block_range")?;

        // Fetch the range of certificates in storage.
        let genesis = conn
            .genesis(ctx)
            .await
            .wrap("genesis()")?
            .context("genesis missing")?;
        let first_expected_cert = genesis.fork.first_block.max(block_range.start);
        let last_cert = conn
            .last_certificate(ctx)
            .await
            .wrap("last_certificate()")?;
        let next_expected_cert = last_cert
            .as_ref()
            .map_or(first_expected_cert, |cert| cert.header().number.next());

        // Check that the first certificate in storage has the expected miniblock number.
        if let Some(got) = conn
            .first_certificate(ctx)
            .await
            .wrap("first_certificate()")?
        {
            if got.header().number != first_expected_cert {
                return Err(anyhow::format_err!(
                    "inconsistent storage: certificates should start at {first_expected_cert}, while they start at {}",
                    got.header().number,
                ).into());
            }
        }
        // Check that the node has all the blocks before the next expected certificate, because
        // the node needs to know the state of the chain up to block `X` to process block `X+1`.
        if block_range.end < next_expected_cert {
            return Err(anyhow::format_err!("inconsistent storage: cannot start consensus for miniblock {next_expected_cert}, because earlier blocks are missing").into());
        }
        let state = BlockStoreState {
            first: first_expected_cert,
            last: last_cert,
        };
        Ok(state)
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
        tracing::info!("storing block {}", block.number());
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
        self.inner
            .wait_for_payload(ctx, block.number())
            .await
            .wrap("wait_for_payload()")?;
        self.inner
            .access(ctx)
            .await
            .wrap("access()")?
            .insert_certificate(ctx, &block.justification)
            .await
            .wrap("insert_certificate()")?;
        tracing::info!("storing block {} DONE", block.number());
        Ok(())
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
        const LARGE_PAYLOAD_SIZE: usize = 1 << 20;
        tracing::info!("proposing block {block_number}");
        let payload = self.wait_for_payload(ctx, block_number).await?;
        let encoded_payload = payload.encode();
        if encoded_payload.0.len() > LARGE_PAYLOAD_SIZE {
            tracing::warn!(
                "large payload ({}B) with {} transactions",
                encoded_payload.0.len(),
                payload.transactions.len()
            );
        }
        tracing::info!("proposing block {block_number} DONE");
        Ok(encoded_payload)
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
        tracing::info!("verifying block {block_number}");
        let got = Payload::decode(payload).context("Payload::decode(got)")?;
        let want = self.wait_for_payload(ctx, block_number).await?;
        if got != want {
            return Err(
                anyhow::format_err!("unexpected payload: got {got:?} want {want:?}").into(),
            );
        }
        tracing::info!("verifying block {block_number} DONE");
        Ok(())
    }
}

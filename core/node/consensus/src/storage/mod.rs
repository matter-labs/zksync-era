//! Storage implementation based on DAL.
use std::sync::Arc;

use anyhow::Context as _;
use zksync_concurrency::{ctx, error::Wrap as _, sync, time};
use zksync_consensus_bft::PayloadManager;
use zksync_consensus_roles::validator;
use zksync_consensus_storage as storage;
use zksync_dal::{consensus_dal::Payload, Core, CoreDal, DalError};
use zksync_node_sync::{
    fetcher::{FetchedBlock, FetchedTransaction, IoCursorExt as _},
    sync_action::ActionQueueSender,
    SyncState,
};
use zksync_state_keeper::io::common::IoCursor;
use zksync_types::{commitment::L1BatchWithMetadata, L1BatchNumber, L2BlockNumber};

use super::config;

#[cfg(test)]
mod testonly;

/// Context-aware `zksync_dal::ConnectionPool<Core>` wrapper.
#[derive(Debug, Clone)]
pub(super) struct ConnectionPool(pub(super) zksync_dal::ConnectionPool<Core>);

impl ConnectionPool {
    /// Wrapper for `connection_tagged()`.
    pub(super) async fn connection<'a>(&'a self, ctx: &ctx::Ctx) -> ctx::Result<Connection<'a>> {
        Ok(Connection(
            ctx.wait(self.0.connection_tagged("consensus"))
                .await?
                .map_err(DalError::generalize)?,
        ))
    }

    /// Waits for the `number` L2 block.
    pub async fn wait_for_payload(
        &self,
        ctx: &ctx::Ctx,
        number: validator::BlockNumber,
    ) -> ctx::Result<Payload> {
        const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(50);
        loop {
            if let Some(payload) = self
                .connection(ctx)
                .await
                .wrap("connection()")?
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

    /// Wrapper for `consensus_dal().block_payloads()`.
    pub async fn payloads(
        &mut self,
        ctx: &ctx::Ctx,
        numbers: std::ops::Range<validator::BlockNumber>,
    ) -> ctx::Result<Vec<Payload>> {
        Ok(ctx
            .wait(self.0.consensus_dal().block_payloads(numbers))
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
    pub async fn replica_state(&mut self, ctx: &ctx::Ctx) -> ctx::Result<storage::ReplicaState> {
        Ok(ctx
            .wait(self.0.consensus_dal().replica_state())
            .await?
            .map_err(DalError::generalize)?)
    }

    /// Wrapper for `consensus_dal().set_replica_state()`.
    pub async fn set_replica_state(
        &mut self,
        ctx: &ctx::Ctx,
        state: &storage::ReplicaState,
    ) -> ctx::Result<()> {
        Ok(ctx
            .wait(self.0.consensus_dal().set_replica_state(state))
            .await?
            .context("sqlx")?)
    }

    /// Wrapper for `consensus_dal().get_l1_batch_metadata()`.
    pub async fn batch(
        &mut self,
        ctx: &ctx::Ctx,
        number: L1BatchNumber,
    ) -> ctx::Result<Option<L1BatchWithMetadata>> {
        Ok(ctx
            .wait(self.0.blocks_dal().get_l1_batch_metadata(number))
            .await?
            .context("get_l1_batch_metadata()")?)
    }

    /// Wrapper for `FetcherCursor::new()`.
    pub async fn new_payload_queue(
        &mut self,
        ctx: &ctx::Ctx,
        actions: ActionQueueSender,
        sync_state: SyncState,
    ) -> ctx::Result<PayloadQueue> {
        Ok(PayloadQueue {
            inner: ctx.wait(IoCursor::for_fetcher(&mut self.0)).await??,
            actions,
            sync_state,
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

    /// Fetches and verifies consistency of certificates in storage.
    async fn certificates_range(
        &mut self,
        ctx: &ctx::Ctx,
    ) -> ctx::Result<storage::BlockStoreState> {
        // Fetch the range of L2 blocks in storage.
        let block_range = self.block_range(ctx).await.context("block_range")?;

        // Fetch the range of certificates in storage.
        let genesis = self
            .genesis(ctx)
            .await
            .wrap("genesis()")?
            .context("genesis missing")?;
        let first_expected_cert = genesis.first_block.max(block_range.start);
        let last_cert = self
            .last_certificate(ctx)
            .await
            .wrap("last_certificate()")?;
        let next_expected_cert = last_cert
            .as_ref()
            .map_or(first_expected_cert, |cert| cert.header().number.next());

        // Check that the first certificate in storage has the expected L2 block number.
        if let Some(got) = self
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
            return Err(anyhow::format_err!("inconsistent storage: cannot start consensus for L2 block {next_expected_cert}, because earlier blocks are missing").into());
        }
        Ok(storage::BlockStoreState {
            first: first_expected_cert,
            last: last_cert,
        })
    }

    /// (Re)initializes consensus genesis to start at the last L2 block in storage.
    /// Noop if `spec` matches the current genesis.
    pub(super) async fn adjust_genesis(
        &mut self,
        ctx: &ctx::Ctx,
        spec: &config::GenesisSpec,
    ) -> ctx::Result<()> {
        let block_range = self.block_range(ctx).await.wrap("block_range()")?;
        let mut txn = self
            .start_transaction(ctx)
            .await
            .wrap("start_transaction()")?;
        let old = txn.genesis(ctx).await.wrap("genesis()")?;
        if let Some(old) = &old {
            if &config::GenesisSpec::from_genesis(old) == spec {
                // Hard fork is not needed.
                return Ok(());
            }
        }
        tracing::info!("Performing a hard fork of consensus.");
        let genesis = validator::GenesisRaw {
            chain_id: spec.chain_id,
            fork_number: old
                .as_ref()
                .map_or(validator::ForkNumber(0), |old| old.fork_number.next()),
            first_block: block_range.end,

            protocol_version: spec.protocol_version,
            committee: spec.validators.clone(),
            leader_selection: spec.leader_selection.clone(),
        }
        .with_hash();
        txn.try_update_genesis(ctx, &genesis)
            .await
            .wrap("try_update_genesis()")?;
        txn.commit(ctx).await.wrap("commit()")?;
        Ok(())
    }

    pub(super) async fn block(
        &mut self,
        ctx: &ctx::Ctx,
        number: validator::BlockNumber,
    ) -> ctx::Result<Option<validator::FinalBlock>> {
        let Some(justification) = self.certificate(ctx, number).await.wrap("certificate()")? else {
            return Ok(None);
        };
        let payload = self
            .payload(ctx, number)
            .await
            .wrap("payload()")?
            .context("L2 block disappeared from storage")?;
        Ok(Some(validator::FinalBlock {
            payload: payload.encode(),
            justification,
        }))
    }
}

#[derive(Debug)]
pub(super) struct PayloadQueue {
    inner: IoCursor,
    actions: ActionQueueSender,
    sync_state: SyncState,
}

impl PayloadQueue {
    pub(super) fn next(&self) -> validator::BlockNumber {
        validator::BlockNumber(self.inner.next_l2_block.0.into())
    }

    /// Advances the cursor by converting the block into actions and pushing them
    /// to the actions queue.
    /// Does nothing and returns Ok() if the block has been already processed.
    /// Returns an error if a block with an earlier block number was expected.
    pub(super) async fn send(&mut self, block: FetchedBlock) -> anyhow::Result<()> {
        let want = self.inner.next_l2_block;
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

fn to_fetched_block(
    number: validator::BlockNumber,
    payload: &validator::Payload,
) -> anyhow::Result<FetchedBlock> {
    let number = L2BlockNumber(
        number
            .0
            .try_into()
            .context("Integer overflow converting block number")?,
    );
    let payload = Payload::decode(payload).context("Payload::decode()")?;
    Ok(FetchedBlock {
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
    })
}

/// Wrapper of `ConnectionPool` implementing `ReplicaStore`, `PayloadManager` and
/// `PersistentBlockStore`.
#[derive(Clone, Debug)]
pub(super) struct Store {
    pub(super) pool: ConnectionPool,
    payloads: Arc<sync::Mutex<Option<PayloadQueue>>>,
    certificates: ctx::channel::UnboundedSender<validator::CommitQC>,
    persisted: sync::watch::Receiver<storage::BlockStoreState>,
}

/// Background task of the `Store`.
pub struct StoreRunner {
    pool: ConnectionPool,
    persisted: sync::watch::Sender<storage::BlockStoreState>,
    certificates: ctx::channel::UnboundedReceiver<validator::CommitQC>,
}

impl Store {
    pub(super) async fn new(
        ctx: &ctx::Ctx,
        pool: ConnectionPool,
        payload_queue: Option<PayloadQueue>,
    ) -> ctx::Result<(Store, StoreRunner)> {
        let persisted = pool
            .connection(ctx)
            .await
            .wrap("connection()")?
            .certificates_range(ctx)
            .await
            .wrap("certificates_range()")?;
        let persisted = sync::watch::channel(persisted).0;
        let (certs_send, certs_recv) = ctx::channel::unbounded();
        Ok((
            Store {
                pool: pool.clone(),
                certificates: certs_send,
                payloads: Arc::new(sync::Mutex::new(payload_queue)),
                persisted: persisted.subscribe(),
            },
            StoreRunner {
                pool,
                persisted,
                certificates: certs_recv,
            },
        ))
    }
}

impl StoreRunner {
    pub async fn run(mut self, ctx: &ctx::Ctx) -> anyhow::Result<()> {
        let res = async {
            loop {
                let cert = self.certificates.recv(ctx).await?;
                self.pool
                    .wait_for_payload(ctx, cert.header().number)
                    .await
                    .wrap("wait_for_payload()")?;
                self.pool
                    .connection(ctx)
                    .await
                    .wrap("connection()")?
                    .insert_certificate(ctx, &cert)
                    .await
                    .wrap("insert_certificate()")?;
                self.persisted.send_modify(|p| p.last = Some(cert));
            }
        }
        .await;
        match res {
            Err(ctx::Error::Canceled(_)) | Ok(()) => Ok(()),
            Err(ctx::Error::Internal(err)) => Err(err),
        }
    }
}

#[async_trait::async_trait]
impl storage::PersistentBlockStore for Store {
    async fn genesis(&self, ctx: &ctx::Ctx) -> ctx::Result<validator::Genesis> {
        Ok(self
            .pool
            .connection(ctx)
            .await
            .wrap("connection")?
            .genesis(ctx)
            .await?
            .context("not found")?)
    }

    fn persisted(&self) -> sync::watch::Receiver<storage::BlockStoreState> {
        self.persisted.clone()
    }

    async fn block(
        &self,
        ctx: &ctx::Ctx,
        number: validator::BlockNumber,
    ) -> ctx::Result<validator::FinalBlock> {
        Ok(self
            .pool
            .connection(ctx)
            .await
            .wrap("connection")?
            .block(ctx, number)
            .await?
            .context("not found")?)
    }

    /// If actions queue is set (and the block has not been stored yet),
    /// the block will be translated into a sequence of actions.
    /// The received actions should be fed
    /// to `ExternalIO`, so that `StateKeeper` will store the corresponding L2 block in the db.
    ///
    /// `store_next_block()` call will wait synchronously for the L2 block.
    /// Once the L2 block is observed in storage, `store_next_block()` will store a cert for this
    /// L2 block.
    async fn queue_next_block(
        &self,
        ctx: &ctx::Ctx,
        block: validator::FinalBlock,
    ) -> ctx::Result<()> {
        let mut payloads = sync::lock(ctx, &self.payloads).await?.into_async();
        if let Some(payloads) = &mut *payloads {
            payloads
                .send(to_fetched_block(block.number(), &block.payload).context("to_fetched_block")?)
                .await
                .context("payload_queue.send()")?;
        }
        self.certificates.send(block.justification);
        Ok(())
    }
}

#[async_trait::async_trait]
impl storage::ReplicaStore for Store {
    async fn state(&self, ctx: &ctx::Ctx) -> ctx::Result<storage::ReplicaState> {
        self.pool
            .connection(ctx)
            .await
            .wrap("connection()")?
            .replica_state(ctx)
            .await
            .wrap("replica_state()")
    }

    async fn set_state(&self, ctx: &ctx::Ctx, state: &storage::ReplicaState) -> ctx::Result<()> {
        self.pool
            .connection(ctx)
            .await
            .wrap("connection()")?
            .set_replica_state(ctx, state)
            .await
            .wrap("set_replica_state()")
    }
}

#[async_trait::async_trait]
impl PayloadManager for Store {
    /// Currently (for the main node) proposing is implemented as just converting an L2 block from db (without a cert) into a payload.
    async fn propose(
        &self,
        ctx: &ctx::Ctx,
        block_number: validator::BlockNumber,
    ) -> ctx::Result<validator::Payload> {
        const LARGE_PAYLOAD_SIZE: usize = 1 << 20;
        let payload = self.pool.wait_for_payload(ctx, block_number).await?;
        let encoded_payload = payload.encode();
        if encoded_payload.0.len() > LARGE_PAYLOAD_SIZE {
            tracing::warn!(
                "large payload ({}B) with {} transactions",
                encoded_payload.0.len(),
                payload.transactions.len()
            );
        }
        Ok(encoded_payload)
    }

    /// Verify that `payload` is a correct proposal for the block `block_number`.
    /// * for the main node it checks whether the same block is already present in storage.
    /// * for the EN validator
    ///   * if the block with this number was already applied, it checks that it was the
    ///     same block. It should always be true, because main node is the only proposer and
    ///     to propose a different block a hard fork is needed.
    ///   * otherwise, EN attempts to apply the received block. If the block was incorrect
    ///     the statekeeper is expected to crash the whole EN. Otherwise OK is returned.
    async fn verify(
        &self,
        ctx: &ctx::Ctx,
        block_number: validator::BlockNumber,
        payload: &validator::Payload,
    ) -> ctx::Result<()> {
        let mut payloads = sync::lock(ctx, &self.payloads).await?.into_async();
        if let Some(payloads) = &mut *payloads {
            let block = to_fetched_block(block_number, payload).context("to_fetched_block")?;
            let n = block.number;
            payloads.send(block).await.context("payload_queue.send()")?;
            // Wait for the block to be processed, without waiting for it to be stored.
            // TODO(BFT-459): this is not ideal, because we don't check here whether the
            // processed block is the same as `payload`. It will work correctly
            // with the current implementation of EN, but we should make it more
            // precise when block reverting support is implemented.
            ctx.wait(payloads.sync_state.wait_for_local_block(n))
                .await?;
        } else {
            let want = self.pool.wait_for_payload(ctx, block_number).await?;
            let got = Payload::decode(payload).context("Payload::decode(got)")?;
            if got != want {
                return Err(
                    anyhow::format_err!("unexpected payload: got {got:?} want {want:?}").into(),
                );
            }
        }
        Ok(())
    }
}

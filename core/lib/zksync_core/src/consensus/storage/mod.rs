//! Storage implementation based on DAL.

use std::sync::Arc;

use anyhow::Context as _;
use zksync_concurrency::{ctx, error::Wrap as _, scope, sync, time};
use zksync_consensus_bft::PayloadManager;
use zksync_consensus_roles::validator;
use zksync_consensus_storage as storage;
use zksync_dal::{consensus_dal::Payload, ConnectionPool, Core, CoreDal, DalError};
use zksync_types::L2BlockNumber;

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

    /// Wrapper for `FetcherCursor::new()`.
    pub async fn new_payload_queue(
        &mut self,
        ctx: &ctx::Ctx,
        actions: ActionQueueSender,
    ) -> ctx::Result<PayloadQueue> {
        Ok(PayloadQueue {
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
pub(super) struct PayloadQueue {
    inner: IoCursor,
    actions: ActionQueueSender,
}

impl PayloadQueue {
    pub(super) fn next(&self) -> validator::BlockNumber {
        validator::BlockNumber(self.inner.next_l2_block.0.into())
    }

    /// Converts the block into actions and pushes them to the actions queue.
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

/// Wrapper of `ConnectionPool` implementing `ReplicaStore` and `PayloadManager`.
#[derive(Clone, Debug)]
pub struct Store(pub ConnectionPool<Core>);

impl Store {
    /// Wrapper for `connection_tagged()`.
    pub(super) async fn access<'a>(&'a self, ctx: &ctx::Ctx) -> ctx::Result<Connection<'a>> {
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

    pub(super) async fn genesis(&self, ctx: &ctx::Ctx) -> ctx::Result<validator::Genesis> {
        Ok(self
            .access(ctx)
            .await
            .wrap("access()")?
            .genesis(ctx)
            .await
            .wrap("genesis()")?
            .context("genesis is missing")?)
    }

    /// Fetches and verifies consistency of certificates in storage.
    async fn certificates_range(&self, ctx: &ctx::Ctx) -> ctx::Result<storage::BlockStoreState> {
        let mut conn = self.access(ctx).await.wrap("access()")?;

        // Fetch the range of L2 blocks in storage.
        let block_range = conn.block_range(ctx).await.context("block_range")?;

        // Fetch the range of certificates in storage.
        let genesis = conn
            .genesis(ctx)
            .await
            .wrap("genesis()")?
            .context("genesis missing")?;
        let first_expected_cert = genesis.first_block.max(block_range.start);
        let last_cert = conn
            .last_certificate(ctx)
            .await
            .wrap("last_certificate()")?;
        let next_expected_cert = last_cert
            .as_ref()
            .map_or(first_expected_cert, |cert| cert.header().number.next());

        // Check that the first certificate in storage has the expected L2 block number.
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
            return Err(anyhow::format_err!("inconsistent storage: cannot start consensus for L2 block {next_expected_cert}, because earlier blocks are missing").into());
        }
        Ok(storage::BlockStoreState {
            first: first_expected_cert,
            last: last_cert,
        })
    }

    pub(super) async fn block(
        &self,
        ctx: &ctx::Ctx,
        number: validator::BlockNumber,
    ) -> ctx::Result<Option<validator::FinalBlock>> {
        let conn = &mut self.access(ctx).await.wrap("access()")?;
        let Some(justification) = conn.certificate(ctx, number).await.wrap("certificate()")? else {
            return Ok(None);
        };
        let payload = conn
            .payload(ctx, number)
            .await
            .wrap("payload()")?
            .context("L2 block disappeared from storage")?;
        Ok(Some(validator::FinalBlock {
            payload: payload.encode(),
            justification,
        }))
    }

    /// Initializes consensus genesis (with 1 validator) to start at the last L2 block in storage.
    /// No-op if db already contains a genesis.
    pub(super) async fn try_init_genesis(
        &self,
        ctx: &ctx::Ctx,
        chain_id: validator::ChainId,
        validator_key: &validator::PublicKey,
    ) -> ctx::Result<()> {
        let mut conn = self.access(ctx).await.wrap("access()")?;
        let block_range = conn.block_range(ctx).await.wrap("block_range()")?;
        let mut txn = conn
            .start_transaction(ctx)
            .await
            .wrap("start_transaction()")?;
        // `Committee::new()` with a single validator should never fail.
        let committee = validator::Committee::new([validator::WeightedValidator {
            key: validator_key.clone(),
            weight: 1,
        }])
        .unwrap();
        let leader_selection = validator::LeaderSelectionMode::Sticky(validator_key.clone());
        let old = txn.genesis(ctx).await.wrap("genesis()")?;
        // Check if the current config of the main node is compatible with the stored genesis.
        if old.as_ref().map_or(false, |old| {
            old.chain_id == chain_id
                && old.protocol_version == validator::ProtocolVersion::CURRENT
                && old.committee == committee
                && old.leader_selection == leader_selection
        }) {
            return Ok(());
        }
        // If not, perform a hard fork.
        tracing::info!("Performing a hard fork of consensus.");
        let genesis = validator::GenesisRaw {
            chain_id,
            fork_number: old
                .as_ref()
                .map_or(validator::ForkNumber(0), |old| old.fork_number.next()),
            first_block: block_range.end,

            protocol_version: validator::ProtocolVersion::CURRENT,
            committee,
            leader_selection,
        }
        .with_hash();
        txn.try_update_genesis(ctx, &genesis)
            .await
            .wrap("try_update_genesis()")?;
        txn.commit(ctx).await.wrap("commit()")?;
        Ok(())
    }

    pub(super) async fn into_block_store(
        self,
        ctx: &ctx::Ctx,
        payload_queue: Option<PayloadQueue>,
    ) -> ctx::Result<(Arc<storage::BlockStore>, BlockStoreRunner)> {
        let persisted = self
            .certificates_range(ctx)
            .await
            .wrap("certificates_range()")?;
        let persisted = sync::watch::channel(persisted).0;
        let (certs_send, certs_recv) = ctx::channel::unbounded();
        let (block_store, runner) = storage::BlockStore::new(
            ctx,
            Box::new(BlockStore {
                inner: self.clone(),
                certificates: certs_send,
                payloads: payload_queue.map(sync::Mutex::new),
                persisted: persisted.subscribe(),
            }),
        )
        .await?;
        Ok((
            block_store,
            BlockStoreRunner {
                store: self,
                persisted,
                certificates: certs_recv,
                inner: runner,
            },
        ))
    }
}

/// Wrapper of `ConnectionPool` implementing `PersistentBlockStore`.
#[derive(Debug)]
struct BlockStore {
    inner: Store,
    payloads: Option<sync::Mutex<PayloadQueue>>,
    certificates: ctx::channel::UnboundedSender<validator::CommitQC>,
    persisted: sync::watch::Receiver<storage::BlockStoreState>,
}

/// Background task of the `BlockStore`.
pub struct BlockStoreRunner {
    store: Store,
    persisted: sync::watch::Sender<storage::BlockStoreState>,
    certificates: ctx::channel::UnboundedReceiver<validator::CommitQC>,
    inner: storage::BlockStoreRunner,
}

impl BlockStoreRunner {
    pub async fn run(mut self, ctx: &ctx::Ctx) -> anyhow::Result<()> {
        let res = scope::run!(ctx, |ctx, s| async {
            s.spawn(async { Ok(self.inner.run(ctx).await?) });
            loop {
                let cert = self.certificates.recv(ctx).await?;
                self.store
                    .wait_for_payload(ctx, cert.header().number)
                    .await
                    .wrap("wait_for_payload()")?;
                self.store
                    .access(ctx)
                    .await
                    .wrap("access()")?
                    .insert_certificate(ctx, &cert)
                    .await
                    .wrap("insert_certificate()")?;
                self.persisted.send_modify(|p| p.last = Some(cert));
            }
        })
        .await;
        match res {
            Err(ctx::Error::Canceled(_)) | Ok(()) => Ok(()),
            Err(ctx::Error::Internal(err)) => Err(err),
        }
    }
}

#[async_trait::async_trait]
impl storage::PersistentBlockStore for BlockStore {
    async fn genesis(&self, ctx: &ctx::Ctx) -> ctx::Result<validator::Genesis> {
        self.inner.genesis(ctx).await
    }

    fn persisted(&self) -> sync::watch::Receiver<storage::BlockStoreState> {
        self.persisted.clone()
    }

    async fn block(
        &self,
        ctx: &ctx::Ctx,
        number: validator::BlockNumber,
    ) -> ctx::Result<validator::FinalBlock> {
        Ok(self.inner.block(ctx, number).await?.context("not found")?)
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
        if let Some(payloads) = &self.payloads {
            let mut payloads = sync::lock(ctx, payloads).await?.into_async();
            let number = L2BlockNumber(
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
            payloads.send(block).await.context("payload_queue.send()")?;
        }
        self.certificates.send(block.justification);
        Ok(())
    }
}

#[async_trait::async_trait]
impl storage::ReplicaStore for Store {
    async fn state(&self, ctx: &ctx::Ctx) -> ctx::Result<storage::ReplicaState> {
        self.access(ctx)
            .await
            .wrap("access()")?
            .replica_state(ctx)
            .await
            .wrap("replica_state()")
    }

    async fn set_state(&self, ctx: &ctx::Ctx, state: &storage::ReplicaState) -> ctx::Result<()> {
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
    /// Currently (for the main node) proposing is implemented as just converting an L2 block from db (without a cert) into a payload.
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
    /// Currently, (for the main node) it is implemented as checking whether the received payload
    /// matches the L2 block in the db.
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

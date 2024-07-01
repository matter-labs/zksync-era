use anyhow::Context as _;
use zksync_concurrency::{ctx, error::Wrap as _, time};
use zksync_consensus_roles::validator;
use zksync_consensus_storage as storage;
use zksync_dal::{consensus_dal::Payload, Core, CoreDal, DalError};
use zksync_node_sync::{fetcher::IoCursorExt as _, ActionQueueSender, SyncState};
use zksync_state_keeper::io::common::IoCursor;
use zksync_types::{commitment::L1BatchWithMetadata, L1BatchNumber};

use super::PayloadQueue;
use crate::config;

/// Context-aware `zksync_dal::ConnectionPool<Core>` wrapper.
#[derive(Debug, Clone)]
pub(crate) struct ConnectionPool(pub(crate) zksync_dal::ConnectionPool<Core>);

impl ConnectionPool {
    /// Wrapper for `connection_tagged()`.
    pub(crate) async fn connection<'a>(&'a self, ctx: &ctx::Ctx) -> ctx::Result<Connection<'a>> {
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
pub(crate) struct Connection<'a>(pub(crate) zksync_dal::Connection<'a, Core>);

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
    pub async fn certificates_range(
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
    pub(crate) async fn adjust_genesis(
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

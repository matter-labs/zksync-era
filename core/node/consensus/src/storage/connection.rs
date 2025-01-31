use anyhow::Context as _;
use zksync_concurrency::{ctx, error::Wrap as _, time};
use zksync_consensus_roles::{attester, attester::BatchNumber, validator};
use zksync_consensus_storage as storage;
use zksync_dal::{
    consensus_dal::{AttestationStatus, BlockMetadata, GlobalConfig, Payload},
    Core, CoreDal, DalError,
};
use zksync_l1_contract_interface::i_executor::structures::StoredBatchInfo;
use zksync_node_sync::{fetcher::IoCursorExt as _, ActionQueueSender, SyncState};
use zksync_state_keeper::io::common::IoCursor;
use zksync_types::{fee_model::BatchFeeInput, L1BatchNumber, L2BlockNumber};
use zksync_vm_executor::oneshot::{BlockInfo, ResolvedBlockInfo};

use super::PayloadQueue;
use crate::config;

/// Context-aware `zksync_dal::ConnectionPool<Core>` wrapper.
#[derive(Debug, Clone)]
pub(crate) struct ConnectionPool(pub(crate) zksync_dal::ConnectionPool<Core>);

impl ConnectionPool {
    /// Wrapper for `connection_tagged()`.
    pub(crate) async fn connection(&self, ctx: &ctx::Ctx) -> ctx::Result<Connection<'static>> {
        Ok(Connection(
            ctx.wait(self.0.connection_tagged("consensus"))
                .await?
                .map_err(DalError::generalize)?,
        ))
    }

    /// Waits for the `number` L2 block.
    #[tracing::instrument(skip_all)]
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
                .with_wrap(|| format!("payload({number})"))?
            {
                return Ok(payload);
            }
            ctx.sleep(POLL_INTERVAL).await?;
        }
    }

    /// Waits for the `number` L1 batch hash.
    #[tracing::instrument(skip_all)]
    pub async fn wait_for_batch_info(
        &self,
        ctx: &ctx::Ctx,
        number: attester::BatchNumber,
        interval: time::Duration,
    ) -> ctx::Result<StoredBatchInfo> {
        loop {
            if let Some(info) = self
                .connection(ctx)
                .await
                .wrap("connection()")?
                .batch_info(ctx, number)
                .await
                .with_wrap(|| format!("batch_info({number})"))?
            {
                return Ok(info);
            }
            ctx.sleep(interval).await?;
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

    pub async fn batch_info(
        &mut self,
        ctx: &ctx::Ctx,
        n: attester::BatchNumber,
    ) -> ctx::Result<Option<StoredBatchInfo>> {
        Ok(ctx.wait(self.0.consensus_dal().batch_info(n)).await??)
    }

    /// Wrapper for `consensus_dal().block_metadata()`.
    pub async fn block_metadata(
        &mut self,
        ctx: &ctx::Ctx,
        number: validator::BlockNumber,
    ) -> ctx::Result<Option<BlockMetadata>> {
        Ok(ctx
            .wait(self.0.consensus_dal().block_metadata(number))
            .await??)
    }

    /// Wrapper for `consensus_dal().block_certificate()`.
    pub async fn block_certificate(
        &mut self,
        ctx: &ctx::Ctx,
        number: validator::BlockNumber,
    ) -> ctx::Result<Option<validator::CommitQC>> {
        Ok(ctx
            .wait(self.0.consensus_dal().block_certificate(number))
            .await??)
    }

    /// Wrapper for `consensus_dal().insert_block_certificate()`.
    #[tracing::instrument(skip_all, fields(l2_block = %cert.message.proposal.number))]
    pub async fn insert_block_certificate(
        &mut self,
        ctx: &ctx::Ctx,
        cert: &validator::CommitQC,
    ) -> Result<(), super::InsertCertificateError> {
        Ok(ctx
            .wait(self.0.consensus_dal().insert_block_certificate(cert))
            .await??)
    }

    /// Wrapper for `consensus_dal().insert_batch_certificate()`,
    /// which additionally verifies that the batch hash matches the stored batch.
    #[tracing::instrument(skip_all, fields(l1_batch = %cert.message.number))]
    pub async fn insert_batch_certificate(
        &mut self,
        ctx: &ctx::Ctx,
        cert: &attester::BatchQC,
    ) -> ctx::Result<()> {
        Ok(ctx
            .wait(self.0.consensus_dal().insert_batch_certificate(cert))
            .await??)
    }

    /// Wrapper for `consensus_dal().upsert_attester_committee()`.
    pub async fn upsert_attester_committee(
        &mut self,
        ctx: &ctx::Ctx,
        number: BatchNumber,
        committee: &attester::Committee,
    ) -> ctx::Result<()> {
        ctx.wait(
            self.0
                .consensus_dal()
                .upsert_attester_committee(number, committee),
        )
        .await??;
        Ok(())
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
        sync_state: SyncState,
    ) -> ctx::Result<PayloadQueue> {
        Ok(PayloadQueue {
            inner: ctx.wait(IoCursor::for_fetcher(&mut self.0)).await??,
            actions,
            sync_state,
        })
    }

    /// Wrapper for `consensus_dal().global_config()`.
    pub async fn global_config(&mut self, ctx: &ctx::Ctx) -> ctx::Result<Option<GlobalConfig>> {
        Ok(ctx.wait(self.0.consensus_dal().global_config()).await??)
    }

    /// Wrapper for `consensus_dal().try_update_global_config()`.
    pub async fn try_update_global_config(
        &mut self,
        ctx: &ctx::Ctx,
        cfg: &GlobalConfig,
    ) -> ctx::Result<()> {
        Ok(ctx
            .wait(self.0.consensus_dal().try_update_global_config(cfg))
            .await??)
    }

    /// Wrapper for `consensus_dal().next_block()`.
    #[tracing::instrument(skip_all)]
    async fn next_block(&mut self, ctx: &ctx::Ctx) -> ctx::Result<validator::BlockNumber> {
        Ok(ctx.wait(self.0.consensus_dal().next_block()).await??)
    }

    /// Wrapper for `consensus_dal().block_store_state()`.
    #[tracing::instrument(skip_all)]
    pub(crate) async fn block_store_state(
        &mut self,
        ctx: &ctx::Ctx,
    ) -> ctx::Result<storage::BlockStoreState> {
        Ok(ctx
            .wait(self.0.consensus_dal().block_store_state())
            .await??)
    }

    /// (Re)initializes consensus genesis to start at the last L2 block in storage.
    /// Noop if `spec` matches the current genesis.
    pub(crate) async fn adjust_global_config(
        &mut self,
        ctx: &ctx::Ctx,
        spec: &config::GenesisSpec,
    ) -> ctx::Result<()> {
        let mut txn = self
            .start_transaction(ctx)
            .await
            .wrap("start_transaction()")?;

        let old = txn.global_config(ctx).await.wrap("genesis()")?;
        if let Some(old) = &old {
            if &config::GenesisSpec::from_global_config(old) == spec {
                // Hard fork is not needed.
                return Ok(());
            }
        }

        tracing::info!("Performing a hard fork of consensus.");
        let new = GlobalConfig {
            genesis: validator::GenesisRaw {
                chain_id: spec.chain_id,
                fork_number: old.as_ref().map_or(validator::ForkNumber(0), |old| {
                    old.genesis.fork_number.next()
                }),
                first_block: txn.next_block(ctx).await.context("next_block()")?,
                protocol_version: spec.protocol_version,
                validators: spec.validators.clone(),
                leader_selection: spec.leader_selection.clone(),
            }
            .with_hash(),
            registry_address: spec.registry_address,
            seed_peers: spec.seed_peers.clone(),
        };

        txn.try_update_global_config(ctx, &new)
            .await
            .wrap("try_update_global_config()")?;
        txn.commit(ctx).await.wrap("commit()")?;
        Ok(())
    }

    /// Fetches a block from storage.
    pub(crate) async fn block(
        &mut self,
        ctx: &ctx::Ctx,
        number: validator::BlockNumber,
    ) -> ctx::Result<Option<validator::Block>> {
        let Some(payload) = self.payload(ctx, number).await.wrap("payload()")? else {
            return Ok(None);
        };

        if let Some(justification) = self
            .block_certificate(ctx, number)
            .await
            .wrap("block_certificate()")?
        {
            return Ok(Some(
                validator::FinalBlock {
                    payload: payload.encode(),
                    justification,
                }
                .into(),
            ));
        }

        Ok(Some(
            validator::PreGenesisBlock {
                number,
                payload: payload.encode(),
                // We won't use justification until it is possible to verify
                // payload against the L1 batch commitment.
                justification: validator::Justification(vec![]),
            }
            .into(),
        ))
    }

    /// Wrapper for `blocks_dal().get_l2_block_range_of_l1_batch()`.
    pub async fn get_l2_block_range_of_l1_batch(
        &mut self,
        ctx: &ctx::Ctx,
        number: attester::BatchNumber,
    ) -> ctx::Result<Option<(validator::BlockNumber, validator::BlockNumber)>> {
        let number = L1BatchNumber(number.0.try_into().context("number")?);

        let range = ctx
            .wait(self.0.blocks_dal().get_l2_block_range_of_l1_batch(number))
            .await?
            .context("get_l2_block_range_of_l1_batch()")?;

        Ok(range.map(|(min, max)| {
            let min = validator::BlockNumber(min.0 as u64);
            let max = validator::BlockNumber(max.0 as u64);
            (min, max)
        }))
    }

    /// Wrapper for `consensus_dal().attestation_status()`.
    pub async fn attestation_status(
        &mut self,
        ctx: &ctx::Ctx,
    ) -> ctx::Result<Option<AttestationStatus>> {
        Ok(ctx
            .wait(self.0.consensus_dal().attestation_status())
            .await?
            .context("attestation_status()")?)
    }

    /// Constructs `BlockArgs` for the last block of the batch.
    pub async fn vm_block_info(
        &mut self,
        ctx: &ctx::Ctx,
        batch: attester::BatchNumber,
    ) -> ctx::Result<(ResolvedBlockInfo, BatchFeeInput)> {
        let (_, block) = self
            .get_l2_block_range_of_l1_batch(ctx, batch)
            .await
            .wrap("get_l2_block_range_of_l1_batch()")?
            .context("batch not sealed")?;
        // `unwrap()` is safe: the block range is returned as `L2BlockNumber`s
        let block = L2BlockNumber(u32::try_from(block.0).unwrap());
        let block_info = ctx
            .wait(BlockInfo::for_existing_block(&mut self.0, block))
            .await?
            .context("BlockInfo")?;
        let resolved_block_info = ctx
            .wait(block_info.resolve(&mut self.0))
            .await?
            .context("resolve()")?;
        let fee_input = ctx
            .wait(block_info.historical_fee_input(&mut self.0))
            .await?
            .context("historical_fee_input()")?;
        Ok((resolved_block_info, fee_input))
    }
}

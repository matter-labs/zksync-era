use anyhow::Context as _;
use zksync_concurrency::{ctx, error::Wrap as _, time};
use zksync_consensus_crypto::keccak256::Keccak256;
use zksync_consensus_roles::{attester, attester::BatchNumber, validator};
use zksync_consensus_storage::{self as storage, BatchStoreState};
use zksync_dal::{consensus_dal, consensus_dal::Payload, Core, CoreDal, DalError};
use zksync_l1_contract_interface::i_executor::structures::StoredBatchInfo;
use zksync_node_api_server::execution_sandbox::{BlockArgs, BlockStartInfo};
use zksync_node_sync::{fetcher::IoCursorExt as _, ActionQueueSender, SyncState};
use zksync_state_keeper::io::common::IoCursor;
use zksync_types::{api, commitment::L1BatchWithMetadata, L1BatchNumber};

use super::{InsertCertificateError, PayloadQueue};
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
    pub async fn wait_for_batch_hash(
        &self,
        ctx: &ctx::Ctx,
        number: attester::BatchNumber,
    ) -> ctx::Result<attester::BatchHash> {
        const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(500);
        loop {
            if let Some(hash) = self
                .connection(ctx)
                .await
                .wrap("connection()")?
                .batch_hash(ctx, number)
                .await
                .with_wrap(|| format!("batch_hash({number})"))?
            {
                return Ok(hash);
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
    ) -> Result<(), InsertCertificateError> {
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
    ) -> Result<(), InsertCertificateError> {
        use consensus_dal::InsertCertificateError as E;
        let want_hash = self
            .batch_hash(ctx, cert.message.number)
            .await
            .wrap("batch_hash()")?
            .ok_or(E::MissingPayload)?;
        if want_hash != cert.message.hash {
            return Err(E::PayloadMismatch.into());
        }
        Ok(ctx
            .wait(self.0.consensus_dal().insert_batch_certificate(cert))
            .await?
            .map_err(E::Other)?)
    }

    /// Wrapper for `consensus_dal().insert_attester_committee()`.
    pub async fn insert_attester_committee(
        &mut self,
        ctx: &ctx::Ctx,
        number: BatchNumber,
        committee: &attester::Committee,
    ) -> ctx::Result<()> {
        ctx.wait(
            self.0
                .consensus_dal()
                .insert_attester_committee(number, committee),
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

    /// Wrapper for `consensus_dal().batch_hash()`.
    pub async fn batch_hash(
        &mut self,
        ctx: &ctx::Ctx,
        number: attester::BatchNumber,
    ) -> ctx::Result<Option<attester::BatchHash>> {
        let n = L1BatchNumber(number.0.try_into().context("overflow")?);
        let Some(meta) = ctx
            .wait(self.0.blocks_dal().get_l1_batch_metadata(n))
            .await?
            .context("get_l1_batch_metadata()")?
        else {
            return Ok(None);
        };
        Ok(Some(attester::BatchHash(Keccak256::from_bytes(
            StoredBatchInfo::from(&meta).hash().0,
        ))))
    }

    /// Wrapper for `blocks_dal().get_l1_batch_metadata()`.
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

    /// Wrapper for `consensus_dal().genesis()`.
    pub async fn genesis(&mut self, ctx: &ctx::Ctx) -> ctx::Result<Option<validator::Genesis>> {
        Ok(ctx
            .wait(self.0.consensus_dal().genesis())
            .await?
            .map_err(DalError::generalize)?)
    }

    /// Wrapper for `consensus_dal().try_update_genesis()`.
    pub async fn try_update_genesis(
        &mut self,
        ctx: &ctx::Ctx,
        genesis: &validator::Genesis,
    ) -> ctx::Result<()> {
        Ok(ctx
            .wait(self.0.consensus_dal().try_update_genesis(genesis))
            .await??)
    }

    /// Wrapper for `consensus_dal().next_block()`.
    #[tracing::instrument(skip_all)]
    async fn next_block(&mut self, ctx: &ctx::Ctx) -> ctx::Result<validator::BlockNumber> {
        Ok(ctx.wait(self.0.consensus_dal().next_block()).await??)
    }

    /// Wrapper for `consensus_dal().block_certificates_range()`.
    #[tracing::instrument(skip_all)]
    pub(crate) async fn block_certificates_range(
        &mut self,
        ctx: &ctx::Ctx,
    ) -> ctx::Result<storage::BlockStoreState> {
        Ok(ctx
            .wait(self.0.consensus_dal().block_certificates_range())
            .await??)
    }

    /// (Re)initializes consensus genesis to start at the last L2 block in storage.
    /// Noop if `spec` matches the current genesis.
    pub(crate) async fn adjust_genesis(
        &mut self,
        ctx: &ctx::Ctx,
        spec: &config::GenesisSpec,
    ) -> ctx::Result<()> {
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
            first_block: txn.next_block(ctx).await.context("next_block()")?,
            protocol_version: spec.protocol_version,
            validators: spec.validators.clone(),
            attesters: spec.attesters.clone(),
            leader_selection: spec.leader_selection.clone(),
        }
        .with_hash();

        txn.try_update_genesis(ctx, &genesis)
            .await
            .wrap("try_update_genesis()")?;
        txn.commit(ctx).await.wrap("commit()")?;
        Ok(())
    }

    /// Fetches a block from storage.
    pub(crate) async fn block(
        &mut self,
        ctx: &ctx::Ctx,
        number: validator::BlockNumber,
    ) -> ctx::Result<Option<validator::FinalBlock>> {
        let Some(justification) = self
            .block_certificate(ctx, number)
            .await
            .wrap("block_certificate()")?
        else {
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

    /// Wrapper for `blocks_dal().get_sealed_l1_batch_number()`.
    #[tracing::instrument(skip_all)]
    pub async fn get_last_batch_number(
        &mut self,
        ctx: &ctx::Ctx,
    ) -> ctx::Result<Option<attester::BatchNumber>> {
        Ok(ctx
            .wait(self.0.blocks_dal().get_sealed_l1_batch_number())
            .await?
            .context("get_sealed_l1_batch_number()")?
            .map(|nr| attester::BatchNumber(nr.0 as u64)))
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

    /// Construct the [attester::SyncBatch] for a given batch number.
    pub async fn get_batch(
        &mut self,
        ctx: &ctx::Ctx,
        number: attester::BatchNumber,
    ) -> ctx::Result<Option<attester::SyncBatch>> {
        let Some((min, max)) = self
            .get_l2_block_range_of_l1_batch(ctx, number)
            .await
            .context("get_l2_block_range_of_l1_batch()")?
        else {
            return Ok(None);
        };

        let payloads = self.payloads(ctx, min..max).await.wrap("payloads()")?;
        let payloads = payloads.into_iter().map(|p| p.encode()).collect();

        // TODO: Fill out the proof when we have the stateless L1 batch validation story finished.
        // It is supposed to be a Merkle proof that the rolling hash of the batch has been included
        // in the L1 system contract state tree. It is *not* the Ethereum state root hash, so producing
        // it can be done without an L1 client, which is only required for validation.
        let batch = attester::SyncBatch {
            number,
            payloads,
            proof: Vec::new(),
        };

        Ok(Some(batch))
    }

    /// Construct the [storage::BatchStoreState] which contains the earliest batch and the last available [attester::SyncBatch].
    #[tracing::instrument(skip_all)]
    pub async fn batches_range(&mut self, ctx: &ctx::Ctx) -> ctx::Result<storage::BatchStoreState> {
        let first = self
            .0
            .blocks_dal()
            .get_earliest_l1_batch_number()
            .await
            .context("get_earliest_l1_batch_number()")?;

        let first = if first.is_some() {
            first
        } else {
            self.0
                .snapshot_recovery_dal()
                .get_applied_snapshot_status()
                .await
                .context("get_earliest_l1_batch_number()")?
                .map(|s| s.l1_batch_number)
        };

        // TODO: In the future when we start filling in the `SyncBatch::proof` field,
        // we can only run `get_batch` expecting `Some` result on numbers where the
        // L1 state root hash is already available, so that we can produce some
        // Merkle proof that the rolling hash of the L2 blocks in the batch has
        // been included in the L1 state tree. At that point we probably can't
        // call `get_last_batch_number` here, but something that indicates that
        // the hashes/commitments on the L1 batch are ready and the thing has
        // been included in L1; that potentially requires an API client as well.
        let last = self
            .get_last_batch_number(ctx)
            .await
            .context("get_last_batch_number()")?;

        Ok(BatchStoreState {
            first: first
                .map(|n| attester::BatchNumber(n.0 as u64))
                .unwrap_or(attester::BatchNumber(0)),
            last,
        })
    }

    /// Wrapper for `consensus_dal().attestation_status()`.
    pub async fn attestation_status(
        &mut self,
        ctx: &ctx::Ctx,
    ) -> ctx::Result<Option<consensus_dal::AttestationStatus>> {
        Ok(ctx
            .wait(self.0.consensus_dal().attestation_status())
            .await?
            .context("attestation_status()")?)
    }

    /// Constructs `BlockArgs` for the last block of the batch`.
    pub async fn block_args(
        &mut self,
        ctx: &ctx::Ctx,
        batch: attester::BatchNumber,
    ) -> ctx::Result<BlockArgs> {
        let (_, block) = self
            .get_l2_block_range_of_l1_batch(ctx, batch)
            .await
            .wrap("get_l2_block_range_of_l1_batch()")?
            .context("batch not sealed")?;
        let block = api::BlockId::Number(api::BlockNumber::Number(block.0.into()));
        let start_info = ctx
            .wait(BlockStartInfo::new(
                &mut self.0,
                /*max_cache_age=*/ std::time::Duration::from_secs(10),
            ))
            .await?
            .context("BlockStartInfo::new()")?;
        Ok(ctx
            .wait(BlockArgs::new(&mut self.0, block, &start_info))
            .await?
            .context("BlockArgs::new")?)
    }
}

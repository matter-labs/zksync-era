//! Storage test helpers.
use anyhow::Context as _;
use zksync_concurrency::{ctx, error::Wrap as _, time};
use zksync_consensus_roles::{attester, validator};
use zksync_contracts::BaseSystemContracts;
use zksync_dal::CoreDal as _;
use zksync_node_genesis::{insert_genesis_batch, mock_genesis_config, GenesisParams};
use zksync_node_test_utils::{recover, snapshot, Snapshot};
use zksync_types::{
    protocol_version::ProtocolSemanticVersion, system_contracts::get_system_smart_contracts,
    L1BatchNumber, L2BlockNumber, ProtocolVersionId,
};

use super::{Connection, ConnectionPool};
use crate::registry;

impl Connection<'_> {
    /// Wrapper for `consensus_dal().batch_of_block()`.
    pub async fn batch_of_block(
        &mut self,
        ctx: &ctx::Ctx,
        block: validator::BlockNumber,
    ) -> ctx::Result<Option<attester::BatchNumber>> {
        Ok(ctx
            .wait(self.0.consensus_dal().batch_of_block(block))
            .await??)
    }

    /// Wrapper for `consensus_dal().last_batch_certificate_number()`.
    pub async fn last_batch_certificate_number(
        &mut self,
        ctx: &ctx::Ctx,
    ) -> ctx::Result<Option<attester::BatchNumber>> {
        Ok(ctx
            .wait(self.0.consensus_dal().last_batch_certificate_number())
            .await??)
    }

    /// Wrapper for `consensus_dal().batch_certificate()`.
    pub async fn batch_certificate(
        &mut self,
        ctx: &ctx::Ctx,
        number: attester::BatchNumber,
    ) -> ctx::Result<Option<attester::BatchQC>> {
        Ok(ctx
            .wait(self.0.consensus_dal().batch_certificate(number))
            .await??)
    }
}

pub(crate) fn mock_genesis_params(protocol_version: ProtocolVersionId) -> GenesisParams {
    let mut cfg = mock_genesis_config();
    cfg.protocol_version = Some(ProtocolSemanticVersion {
        minor: protocol_version,
        patch: 0.into(),
    });
    GenesisParams::from_genesis_config(
        cfg,
        BaseSystemContracts::load_from_disk(),
        get_system_smart_contracts(false),
    )
    .unwrap()
}

impl ConnectionPool {
    pub(crate) async fn test(
        from_snapshot: bool,
        protocol_version: ProtocolVersionId,
    ) -> ConnectionPool {
        match from_snapshot {
            true => {
                ConnectionPool::from_snapshot(Snapshot::new(
                    L1BatchNumber(23),
                    L2BlockNumber(87),
                    vec![],
                    &BaseSystemContracts::load_from_disk(),
                    protocol_version,
                ))
                .await
            }
            false => ConnectionPool::from_genesis(protocol_version).await,
        }
    }

    /// Waits for the `number` L2 block to have a certificate.
    pub async fn wait_for_block_certificate(
        &self,
        ctx: &ctx::Ctx,
        number: validator::BlockNumber,
    ) -> ctx::Result<()> {
        const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(100);
        while self
            .connection(ctx)
            .await
            .wrap("connection()")?
            .block_certificate(ctx, number)
            .await
            .wrap("block_certificate()")?
            .is_none()
        {
            ctx.sleep(POLL_INTERVAL).await?;
        }
        Ok(())
    }

    /// Takes a storage snapshot at the last sealed L1 batch.
    pub(crate) async fn snapshot(&self, ctx: &ctx::Ctx) -> ctx::Result<Snapshot> {
        let mut conn = self.connection(ctx).await.wrap("connection()")?;
        Ok(ctx.wait(snapshot(&mut conn.0)).await?)
    }

    /// Constructs a new db initialized with genesis state.
    pub(crate) async fn from_genesis(protocol_version: ProtocolVersionId) -> Self {
        let pool = zksync_dal::ConnectionPool::test_pool().await;
        {
            let mut storage = pool.connection().await.unwrap();
            insert_genesis_batch(&mut storage, &mock_genesis_params(protocol_version))
                .await
                .unwrap();
        }
        Self(pool)
    }

    /// Recovers storage from a snapshot.
    pub(crate) async fn from_snapshot(snapshot: Snapshot) -> Self {
        let pool = zksync_dal::ConnectionPool::test_pool().await;
        {
            let mut storage = pool.connection().await.unwrap();
            recover(&mut storage, snapshot).await;
        }
        Self(pool)
    }

    /// Waits for `want_last` block then fetches all L2 blocks with certificates.
    pub async fn wait_for_blocks(
        &self,
        ctx: &ctx::Ctx,
        want_last: validator::BlockNumber,
    ) -> ctx::Result<Vec<validator::Block>> {
        const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(100);
        let state = loop {
            let state = self
                .connection(ctx)
                .await
                .wrap("connection()")?
                .block_store_state(ctx)
                .await
                .wrap("block_store_state()")?;
            tracing::info!("state.next() = {}", state.next());
            if state.next() > want_last {
                break state;
            }
            ctx.sleep(POLL_INTERVAL).await?;
        };

        assert_eq!(want_last.next(), state.next());
        let mut conn = self.connection(ctx).await.wrap("connection()")?;
        let mut blocks: Vec<validator::Block> = vec![];
        for i in state.first.0..state.next().0 {
            let i = validator::BlockNumber(i);
            let block = conn.block(ctx, i).await.context("block()")?.unwrap();
            blocks.push(block);
        }
        Ok(blocks)
    }

    /// Same as `wait_for_blocks`, but additionally verifies all certificates.
    pub async fn wait_for_blocks_and_verify_certs(
        &self,
        ctx: &ctx::Ctx,
        want_last: validator::BlockNumber,
    ) -> ctx::Result<Vec<validator::Block>> {
        let blocks = self.wait_for_blocks(ctx, want_last).await?;
        let cfg = self
            .connection(ctx)
            .await
            .wrap("connection()")?
            .global_config(ctx)
            .await
            .wrap("genesis()")?
            .context("genesis is missing")?;
        for block in &blocks {
            if let validator::Block::Final(block) = block {
                block.verify(&cfg.genesis).context(block.number())?;
            }
        }
        Ok(blocks)
    }

    pub async fn wait_for_batch_certificates_and_verify(
        &self,
        ctx: &ctx::Ctx,
        want_last: attester::BatchNumber,
        registry_addr: Option<registry::Address>,
    ) -> ctx::Result<()> {
        // Wait for the last batch to be attested.
        const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(100);
        while self
            .connection(ctx)
            .await
            .wrap("connection()")?
            .last_batch_certificate_number(ctx)
            .await
            .wrap("last_batch_certificate_number()")?
            .map_or(true, |got| got < want_last)
        {
            ctx.sleep(POLL_INTERVAL).await?;
        }
        let mut conn = self.connection(ctx).await.wrap("connection()")?;
        let cfg = conn
            .global_config(ctx)
            .await
            .wrap("global_config()")?
            .context("global config is missing")?;
        let first = conn
            .batch_of_block(ctx, cfg.genesis.first_block)
            .await
            .wrap("batch_of_block()")?
            .context("batch of first_block is missing")?;
        let registry = registry::Registry::new(cfg.genesis.clone(), self.clone()).await;
        for i in first.0..want_last.0 {
            let i = attester::BatchNumber(i);
            let cert = conn
                .batch_certificate(ctx, i)
                .await
                .wrap("batch_certificate")?
                .context("cert missing")?;
            let committee = registry
                .attester_committee_for(ctx, registry_addr, i)
                .await
                .context("attester_committee_for()")?
                .context("committee not specified")?;
            cert.verify(cfg.genesis.hash(), &committee)
                .with_context(|| format!("cert[{i:?}].verify()"))?;
        }
        Ok(())
    }

    pub async fn prune_batches(
        &self,
        ctx: &ctx::Ctx,
        last_batch: attester::BatchNumber,
    ) -> ctx::Result<()> {
        let mut conn = self.connection(ctx).await.context("connection()")?;
        let (_, last_block) = conn
            .get_l2_block_range_of_l1_batch(ctx, last_batch)
            .await
            .wrap("get_l2_block_range_of_l1_batch()")?
            .context("batch not found")?;
        let last_batch = L1BatchNumber(last_batch.0.try_into().context("overflow")?);
        let last_batch_root_hash = ctx
            .wait(conn.0.blocks_dal().get_l1_batch_state_root(last_batch))
            .await?
            .context("get_l1_batch_state_root()")?
            .unwrap_or_default();
        let last_block = L2BlockNumber(last_block.0.try_into().context("overflow")?);
        ctx.wait(
            conn.0
                .pruning_dal()
                .insert_soft_pruning_log(last_batch, last_block),
        )
        .await?
        .context("insert_soft_pruning_log()")?;
        ctx.wait(
            conn.0
                .pruning_dal()
                .hard_prune_batches_range(last_batch, last_block),
        )
        .await?
        .context("hard_prune_batches_range()")?;
        ctx.wait(conn.0.pruning_dal().insert_hard_pruning_log(
            last_batch,
            last_block,
            last_batch_root_hash,
        ))
        .await?
        .context("insert_hard_pruning_log()")?;
        Ok(())
    }
}

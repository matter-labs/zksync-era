//! Storage test helpers.

use anyhow::Context as _;
use zksync_concurrency::{ctx, error::Wrap as _, time};
use zksync_consensus_roles::validator;
use zksync_contracts::BaseSystemContracts;
use zksync_dal::CoreDal as _;
use zksync_node_genesis::{insert_genesis_batch, mock_genesis_config, GenesisParams};
use zksync_node_test_utils::{recover, snapshot, Snapshot};
use zksync_types::{
    commitment::L1BatchWithMetadata, protocol_version::ProtocolSemanticVersion,
    system_contracts::get_system_smart_contracts, L1BatchNumber, L2BlockNumber, ProtocolVersionId,
};

use super::ConnectionPool;

pub(crate) fn mock_genesis_params(protocol_version: ProtocolVersionId) -> GenesisParams {
    let mut cfg = mock_genesis_config();
    cfg.protocol_version = Some(ProtocolSemanticVersion {
        minor: protocol_version,
        patch: 0.into(),
    });
    GenesisParams::from_genesis_config(
        cfg,
        BaseSystemContracts::load_from_disk(),
        get_system_smart_contracts(),
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
                ConnectionPool::from_snapshot(Snapshot::make(
                    L1BatchNumber(23),
                    L2BlockNumber(87),
                    &[],
                    mock_genesis_params(protocol_version),
                ))
                .await
            }
            false => ConnectionPool::from_genesis(protocol_version).await,
        }
    }

    /// Waits for the `number` L2 block to have a certificate.
    pub async fn wait_for_certificate(
        &self,
        ctx: &ctx::Ctx,
        number: validator::BlockNumber,
    ) -> ctx::Result<()> {
        const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(100);
        while self
            .connection(ctx)
            .await
            .wrap("connection()")?
            .certificate(ctx, number)
            .await
            .wrap("certificate()")?
            .is_none()
        {
            ctx.sleep(POLL_INTERVAL).await?;
        }
        Ok(())
    }

    /// Waits for the `number` L1 batch.
    pub async fn wait_for_batch(
        &self,
        ctx: &ctx::Ctx,
        number: L1BatchNumber,
    ) -> ctx::Result<L1BatchWithMetadata> {
        const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(50);
        loop {
            if let Some(payload) = self
                .connection(ctx)
                .await
                .wrap("connection()")?
                .batch(ctx, number)
                .await
                .wrap("batch()")?
            {
                return Ok(payload);
            }
            ctx.sleep(POLL_INTERVAL).await?;
        }
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

    /// Waits for `want_last` block to have certificate then fetches all L2 blocks with certificates.
    pub async fn wait_for_certificates(
        &self,
        ctx: &ctx::Ctx,
        want_last: validator::BlockNumber,
    ) -> ctx::Result<Vec<validator::FinalBlock>> {
        self.wait_for_certificate(ctx, want_last).await?;
        let mut conn = self.connection(ctx).await.wrap("connection()")?;
        let range = conn
            .certificates_range(ctx)
            .await
            .wrap("certificates_range()")?;
        assert_eq!(want_last.next(), range.next());
        let mut blocks: Vec<validator::FinalBlock> = vec![];
        for i in range.first.0..range.next().0 {
            let i = validator::BlockNumber(i);
            let block = conn.block(ctx, i).await.context("block()")?.unwrap();
            blocks.push(block);
        }
        Ok(blocks)
    }

    /// Same as `wait_for_certificates`, but additionally verifies all the blocks against genesis.
    pub async fn wait_for_certificates_and_verify(
        &self,
        ctx: &ctx::Ctx,
        want_last: validator::BlockNumber,
    ) -> ctx::Result<Vec<validator::FinalBlock>> {
        let blocks = self.wait_for_certificates(ctx, want_last).await?;
        let genesis = self
            .connection(ctx)
            .await
            .wrap("connection()")?
            .genesis(ctx)
            .await
            .wrap("genesis()")?
            .context("genesis is missing")?;
        for block in &blocks {
            block.verify(&genesis).context(block.number())?;
        }
        Ok(blocks)
    }

    pub async fn prune_batches(
        &self,
        ctx: &ctx::Ctx,
        last_batch: L1BatchNumber,
    ) -> ctx::Result<()> {
        let mut conn = self.connection(ctx).await.context("connection()")?;
        let (_, last_block) = ctx
            .wait(
                conn.0
                    .blocks_dal()
                    .get_l2_block_range_of_l1_batch(last_batch),
            )
            .await?
            .context("get_l2_block_range_of_l1_batch()")?
            .context("batch not found")?;
        conn.0
            .pruning_dal()
            .soft_prune_batches_range(last_batch, last_block)
            .await
            .context("soft_prune_batches_range()")?;
        conn.0
            .pruning_dal()
            .hard_prune_batches_range(last_batch, last_block)
            .await
            .context("hard_prune_batches_range()")?;
        Ok(())
    }
}

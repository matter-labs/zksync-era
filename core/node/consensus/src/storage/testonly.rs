//! Storage test helpers.
use anyhow::Context as _;
use zksync_concurrency::{ctx, error::Wrap as _, time};
use zksync_consensus_roles::validator;
use zksync_contracts::BaseSystemContracts;
use zksync_dal::CoreDal as _;
use zksync_node_genesis::{insert_genesis_batch, mock_genesis_config, GenesisParams};
use zksync_node_test_utils::{recover, snapshot, Snapshot};
use zksync_types::{
    protocol_version::ProtocolSemanticVersion, system_contracts::get_system_smart_contracts,
    L1BatchNumber, L2BlockNumber, ProtocolVersionId,
};

use super::{Connection, ConnectionPool};

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
            let block = conn.block(ctx, i).await.wrap("block()")?.unwrap();
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
            match block {
                validator::Block::FinalV1(block) => {
                    block
                        .verify(
                            cfg.genesis.hash(),
                            &cfg.genesis.validators_schedule.clone().unwrap(),
                        )
                        .context(block.number())?;
                }
                validator::Block::FinalV2(block) => {
                    // Here we are assuming that we are using a static validators schedule. Hence
                    // getting the epoch number from the genesis config and using epoch 0.
                    block
                        .verify(
                            cfg.genesis.hash(),
                            validator::EpochNumber(0),
                            &cfg.genesis.validators_schedule.clone().unwrap(),
                        )
                        .context(block.number())?;
                }
                validator::Block::PreGenesis(_) => {}
            }
        }
        Ok(blocks)
    }

    pub async fn prune_batches(
        &self,
        ctx: &ctx::Ctx,
        last_batch: L1BatchNumber,
    ) -> ctx::Result<()> {
        let mut conn = self.connection(ctx).await.wrap("connection()")?;

        let (_, last_block) = conn
            .get_l2_block_range_of_l1_batch(ctx, last_batch)
            .await
            .wrap("get_l2_block_range_of_l1_batch()")?
            .context("batch not found")?;
        let last_batch_root_hash = ctx
            .wait(conn.0.blocks_dal().get_l1_batch_state_root(last_batch))
            .await?
            .context("get_l1_batch_state_root()")?
            .unwrap_or_default();

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

    /// Waits for the `number` L1 batch to be stored.
    #[tracing::instrument(skip_all)]
    pub async fn wait_for_batch(
        &self,
        ctx: &ctx::Ctx,
        number: L1BatchNumber,
        interval: time::Duration,
    ) -> ctx::Result<bool> {
        loop {
            if self
                .connection(ctx)
                .await
                .wrap("connection()")?
                .is_batch_stored(ctx, number)
                .await
                .with_wrap(|| format!("is_batch_stored({number})"))?
            {
                return Ok(true);
            }
            ctx.sleep(interval).await?;
        }
    }
}

impl<'a> Connection<'a> {
    /// Wrapper for `blocks_dal().get_l2_block_range_of_l1_batch()`.
    pub async fn get_l2_block_range_of_l1_batch(
        &mut self,
        ctx: &ctx::Ctx,
        number: L1BatchNumber,
    ) -> ctx::Result<Option<(L2BlockNumber, L2BlockNumber)>> {
        Ok(ctx
            .wait(self.0.blocks_dal().get_l2_block_range_of_l1_batch(number))
            .await?
            .context("get_l2_block_range_of_l1_batch()")?)
    }

    /// Wrapper for `consensus_dal().is_batch_stored()`.
    pub async fn is_batch_stored(
        &mut self,
        ctx: &ctx::Ctx,
        number: L1BatchNumber,
    ) -> ctx::Result<bool> {
        Ok(ctx
            .wait(self.0.consensus_dal().is_batch_stored(number))
            .await??)
    }
}

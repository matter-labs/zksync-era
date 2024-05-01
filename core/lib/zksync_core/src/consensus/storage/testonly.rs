//! Storage test helpers.

use anyhow::Context as _;
use zksync_concurrency::{ctx, error::Wrap as _, time};
use zksync_consensus_roles::validator;
use zksync_dal::ConnectionPool;

use super::Store;
use crate::{
    genesis::{insert_genesis_batch, GenesisParams},
    utils::testonly::{recover, snapshot, Snapshot},
};

impl Store {
    /// Waits for the `number` L2 block to have a certificate.
    pub async fn wait_for_certificate(
        &self,
        ctx: &ctx::Ctx,
        number: validator::BlockNumber,
    ) -> ctx::Result<()> {
        const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(100);
        while self
            .access(ctx)
            .await
            .wrap("access()")?
            .certificate(ctx, number)
            .await
            .wrap("certificate()")?
            .is_none()
        {
            ctx.sleep(POLL_INTERVAL).await?;
        }
        Ok(())
    }

    /// Takes a storage snapshot at the last sealed L1 batch.
    pub(crate) async fn snapshot(&self, ctx: &ctx::Ctx) -> ctx::Result<Snapshot> {
        let mut conn = self.access(ctx).await.wrap("access()")?;
        Ok(ctx.wait(snapshot(&mut conn.0)).await?)
    }

    /// Constructs a new db initialized with genesis state.
    pub(crate) async fn from_genesis() -> Self {
        let pool = ConnectionPool::test_pool().await;
        {
            let mut storage = pool.connection().await.unwrap();
            insert_genesis_batch(&mut storage, &GenesisParams::mock())
                .await
                .unwrap();
        }
        Self(pool)
    }

    /// Recovers storage from a snapshot.
    pub(crate) async fn from_snapshot(snapshot: Snapshot) -> Self {
        let pool = ConnectionPool::test_pool().await;
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
        let mut conn = self.access(ctx).await.wrap("access()")?;
        let last_cert = conn
            .last_certificate(ctx)
            .await
            .wrap("last_certificate()")?
            .unwrap();
        let first_cert = conn
            .first_certificate(ctx)
            .await
            .wrap("first_certificate()")?
            .unwrap();
        assert_eq!(want_last, last_cert.header().number);
        let mut blocks: Vec<validator::FinalBlock> = vec![];
        for i in first_cert.header().number.0..=last_cert.header().number.0 {
            let i = validator::BlockNumber(i);
            let block = self.block(ctx, i).await.context("block()")?.unwrap();
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
        let genesis = self.genesis(ctx).await.wrap("genesis()")?;
        for block in &blocks {
            block.verify(&genesis).context(block.number())?;
        }
        Ok(blocks)
    }
}

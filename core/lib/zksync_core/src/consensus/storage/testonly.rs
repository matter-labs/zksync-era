//! Storage test helpers.
use anyhow::Context as _;
use zksync_concurrency::{ctx, error::Wrap as _, time};
use zksync_consensus_roles::validator;
use zksync_consensus_storage as storage;
use zksync_consensus_storage::PersistentBlockStore as _;
use zksync_dal::ConnectionPool;

use super::Store;
use crate::{
    genesis::{insert_genesis_batch, GenesisParams},
    utils::testonly::{recover, snapshot, Snapshot},
};

impl Store {
    /// Waits for the `number` miniblock to have a certificate.
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

    /// Waits for `want_last` block to have certificate, then fetches all miniblocks with certificates
    /// and verifies them.
    pub async fn wait_for_certificates_and_verify(
        &self,
        ctx: &ctx::Ctx,
        want_last: validator::BlockNumber,
    ) -> ctx::Result<Vec<validator::FinalBlock>> {
        self.wait_for_certificate(ctx, want_last).await?;
        let this = self.clone().into_block_store();
        let genesis = this.genesis(ctx).await.wrap("genesis()")?;
        let blocks = storage::testonly::dump(ctx, &this).await;
        let got_last = blocks.last().context("empty store")?.header().number;
        assert_eq!(got_last, want_last);
        for block in &blocks {
            block.verify(&genesis).context(block.header().number)?;
        }
        Ok(blocks)
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
}

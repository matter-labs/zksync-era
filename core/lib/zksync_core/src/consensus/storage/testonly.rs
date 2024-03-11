//! Storage test helpers.
use anyhow::Context as _;
use zksync_concurrency::{ctx, error::Wrap as _, time};
use zksync_consensus_roles::validator;
use zksync_consensus_storage as storage;
use zksync_consensus_storage::PersistentBlockStore as _;
use zksync_dal::consensus_dal::Payload;

use super::Store;

impl Store {
    /// Waits for the `number` miniblock.
    pub async fn wait_for_block(
        &self,
        ctx: &ctx::Ctx,
        number: validator::BlockNumber,
    ) -> ctx::Result<Payload> {
        const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(100);
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

    /// Waits for the `number` miniblock to have a certificate.
    pub async fn wait_for_certificate(
        &self,
        ctx: &ctx::Ctx,
        number: validator::BlockNumber,
    ) -> ctx::Result<()> {
        const POLL_INTERVAL: time::Duration = time::Duration::milliseconds(100);
        loop {
            if self
                .access(ctx)
                .await
                .wrap("access()")?
                .certificate(ctx, number)
                .await
                .wrap("certificate()")?
                .is_some()
            {
                return Ok(());
            }
            ctx.sleep(POLL_INTERVAL).await?;
        }
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
}

//! Consensus adapter for EN synchronization logic.

use std::sync::Arc;

use anyhow::Context as _;
use zksync_concurrency::{ctx, error::Wrap as _, scope};
use zksync_consensus_executor as executor;
use zksync_consensus_roles::validator;
use zksync_dal::ConnectionPool;
use zksync_types::Address;

use self::{buffered::Buffered, storage::PostgresBlockStorage};
use super::{fetcher::FetcherCursor, sync_action::ActionQueueSender};
use crate::consensus;

mod buffered;
mod conversions;
mod metrics;
mod storage;
#[cfg(test)]
mod tests;
mod utils;

pub struct FetcherConfig {
    executor: executor::Config,
    genesis_block_number: validator::BlockNumber,
    operator_address: Address,
}

impl TryFrom<consensus::SerdeConfig> for FetcherConfig {
    type Error = anyhow::Error;
    fn try_from(cfg: consensus::SerdeConfig) -> anyhow::Result<Self> {
        Ok(Self {
            executor: cfg.executor()?,
            genesis_block_number: validator::BlockNumber(cfg.genesis_block_number),
            operator_address: cfg
                .operator_address
                .context("operator_address is required")?,
        })
    }
}

impl FetcherConfig {
    /// Starts fetching L2 blocks using peer-to-peer gossip network.
    pub async fn run(
        self,
        ctx: &ctx::Ctx,
        pool: ConnectionPool,
        actions: ActionQueueSender,
    ) -> anyhow::Result<()> {
        tracing::info!(
            "Starting gossip fetcher with {:?} and node key {:?}",
            self.executor,
            self.executor.node_key.public(),
        );

        let mut storage = pool
            .access_storage_tagged("sync_layer")
            .await
            .context("Failed acquiring Postgres connection for cursor")?;
        let cursor = FetcherCursor::new(&mut storage)
            .await
            .context("FetcherCursor::new()")?;
        drop(storage);

        let store = PostgresBlockStorage::new(
            ctx,
            pool,
            actions,
            cursor,
            self.genesis_block_number,
            self.operator_address,
        )
        .await
        .wrap("PostgresBlockStorage::new()")?;
        let buffered = Arc::new(Buffered::new(store));
        let store = buffered.inner();

        scope::run!(ctx, |ctx, s| async {
            let executor = executor::Executor {
                config: self.executor,
                storage: buffered.clone(),
                validator: None,
            };
            s.spawn_bg(async {
                store
                    .run_background_tasks(ctx)
                    .await
                    .context("`PostgresBlockStorage` background tasks failed")
            });
            s.spawn_bg(async {
                buffered
                    .run_background_tasks(ctx)
                    .await
                    .context("`Buffered` storage background tasks failed")
            });

            executor.run(ctx).await.context("Node executor terminated")
        })
        .await
    }
}

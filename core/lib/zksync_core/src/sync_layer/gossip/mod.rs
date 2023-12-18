//! Consensus adapter for EN synchronization logic.

use std::sync::Arc;

use anyhow::Context as _;
use tokio::sync::watch;
use zksync_concurrency::{ctx, error::Wrap as _, scope};
use zksync_consensus_executor::Executor;
use zksync_consensus_roles::node;
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
    executor: ExecutorConfig,
    node_key: node::SecretKey,
    operator_address: Address,
}

impl TryFrom<consensus::SerdeConfig> for FetcherConfig {
    type Error = anyhow::Error;
    fn try_from(cfg: consensus::SerdeConfig) -> anyhow::Result<Self> {
        Ok(Self {
            executor: cfg.executor(),
            node_key: cfg.node_key,
            operator_address: cfg
                .operator_address
                .context("operator_address is required")?,
        })
    }
}

impl FetcherConfig {
    /// Starts fetching L2 blocks using peer-to-peer gossip network.
    pub async fn run(
        mut self,
        pool: ConnectionPool,
        actions: ActionQueueSender,
        mut stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        scope::run!(&ctx::root(), |ctx, s| async {
            s.spawn_bg(self.run_inner(ctx, pool, actions));
            if stop_receiver.changed().await.is_err() {
                tracing::warn!(
                    "Stop signal sender for gossip fetcher was dropped without sending a signal"
                );
            }
            tracing::info!("Stop signal received, gossip fetcher is shutting down");
            Ok(())
        })
        .await
    }

    async fn run_inner(
        mut self,
        ctx: &ctx::Ctx,
        pool: ConnectionPool,
        actions: ActionQueueSender,
    ) -> anyhow::Result<()> {
        tracing::info!(
            "Starting gossip fetcher with {:?} and node key {:?}",
            self.executor,
            self.node_key.public(),
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
            &self.executor.genesis_block,
            self.operator_address,
        )
        .await
        .wrap("PostgresBlockStorage::new()")?;
        let buffered = Arc::new(Buffered::new(store));
        let store = buffered.inner();

        scope::run!(ctx, |ctx, s| async {
            let executor = Executor::new(ctx, self.executor, self.node_key, buffered.clone())
                .await
                .context("Node executor misconfiguration")?;
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

//! Consensus adapter for EN synchronization logic.

use anyhow::Context as _;
use tokio::sync::watch;

use std::sync::Arc;

use zksync_concurrency::{ctx, scope};
use zksync_consensus_executor::{Executor, ExecutorConfig};
use zksync_consensus_roles::node;
use zksync_dal::ConnectionPool;

mod buffered;
mod conversions;
mod metrics;
mod storage;
#[cfg(test)]
mod tests;
mod utils;

use self::{buffered::Buffered, storage::PostgresBlockStorage};
use super::{fetcher::FetcherCursor, sync_action::ActionQueueSender};

/// Starts fetching L2 blocks using peer-to-peer gossip network.
pub async fn run_gossip_fetcher(
    pool: ConnectionPool,
    actions: ActionQueueSender,
    executor_config: ExecutorConfig,
    node_key: node::SecretKey,
    mut stop_receiver: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    scope::run!(&ctx::root(), |ctx, s| async {
        s.spawn_bg(run_gossip_fetcher_inner(
            ctx,
            pool,
            actions,
            executor_config,
            node_key,
        ));
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

async fn run_gossip_fetcher_inner(
    ctx: &ctx::Ctx,
    pool: ConnectionPool,
    actions: ActionQueueSender,
    executor_config: ExecutorConfig,
    node_key: node::SecretKey,
) -> anyhow::Result<()> {
    tracing::info!(
        "Starting gossip fetcher with {executor_config:?} and node key {:?}",
        node_key.public()
    );

    let mut storage = pool
        .access_storage_tagged("sync_layer")
        .await
        .context("Failed acquiring Postgres connection for cursor")?;
    let cursor = FetcherCursor::new(&mut storage).await?;
    drop(storage);

    let store = PostgresBlockStorage::new(pool, actions, cursor);
    let buffered = Arc::new(Buffered::new(store));
    let store = buffered.inner();
    let executor = Executor::new(executor_config, node_key, buffered.clone())
        .context("Node executor misconfiguration")?;

    scope::run!(ctx, |ctx, s| async {
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

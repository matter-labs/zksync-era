//! Consensus adapter for EN synchronization logic.

use std::sync::Arc;

use anyhow::Context as _;
use tokio::sync::watch;
use zksync_concurrency::{ctx, error::Wrap as _, scope};
use zksync_consensus_executor::{Executor, ExecutorConfig};
use zksync_consensus_roles::node;
use zksync_dal::ConnectionPool;
use zksync_types::Address;

use self::{buffered::Buffered, storage::PostgresBlockStorage};
use super::{fetcher::FetcherCursor, sync_action::ActionQueueSender};

mod buffered;
mod conversions;
mod metrics;
mod storage;
#[cfg(test)]
mod tests;
mod utils;

/// Starts fetching L2 blocks using peer-to-peer gossip network.
pub async fn run_gossip_fetcher(
    pool: ConnectionPool,
    actions: ActionQueueSender,
    executor_config: ExecutorConfig,
    node_key: node::SecretKey,
    mut stop_receiver: watch::Receiver<bool>,
    operator_address: Address,
) -> anyhow::Result<()> {
    scope::run!(&ctx::root(), |ctx, s| async {
        s.spawn_bg(run_gossip_fetcher_inner(
            ctx,
            pool,
            actions,
            executor_config,
            node_key,
            operator_address,
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
    operator_address: Address,
) -> anyhow::Result<()> {
    tracing::info!(
        "Starting gossip fetcher with {executor_config:?} and node key {:?}",
        node_key.public()
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
        &executor_config.genesis_block,
        operator_address,
    )
    .await
    .wrap("PostgresBlockStorage::new()")?;
    let buffered = Arc::new(Buffered::new(store));
    let store = buffered.inner();

    scope::run!(ctx, |ctx, s| async {
        let executor = Executor::new(ctx, executor_config, node_key, buffered.clone())
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

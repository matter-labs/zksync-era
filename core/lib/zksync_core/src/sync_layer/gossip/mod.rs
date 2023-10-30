//! Consensus adapter for EN synchronization logic.

use anyhow::Context as _;

use std::sync::Arc;

use zksync_concurrency::{ctx, scope};
use zksync_consensus_roles::node;
use zksync_dal::ConnectionPool;
use zksync_executor::{Executor, ExecutorConfig};

mod buffered;
mod conversions;
mod storage;
#[cfg(test)]
mod tests;
mod utils;

use self::{buffered::BufferedStorage, storage::PostgresBlockStore};
use super::{fetcher::FetcherCursor, sync_action::ActionQueueSender};

/// Starts fetching L2 blocks using peer-to-peer gossip network.
pub async fn start_gossip_fetcher(
    pool: ConnectionPool,
    actions: ActionQueueSender,
    executor_config: ExecutorConfig,
    node_key: node::SecretKey,
) -> anyhow::Result<()> {
    let mut storage = pool
        .access_storage_tagged("sync_layer")
        .await
        .context("Failed acquiring Postgres connection for cursor")?;
    let cursor = FetcherCursor::new(&mut storage).await?;
    drop(storage);

    let store = PostgresBlockStore::new(pool, actions, cursor);
    let buffered_store = Arc::new(BufferedStorage::new(store));
    let store = buffered_store.inner();
    let executor = Executor::new(executor_config, node_key, buffered_store.clone())
        .context("Node executor misconfiguration")?;

    scope::run!(&ctx::root(), |ctx, s| async {
        s.spawn_bg(async {
            store
                .listen_to_updates(ctx)
                .await
                .context("`PostgresBlockStore` listener failed")
        });
        s.spawn_bg(async {
            buffered_store
                .listen_to_updates(ctx)
                .await
                .context("`BufferedStore` listener failed")
        });

        executor.run(ctx).await.context("Node executor terminated")
    })
    .await
}

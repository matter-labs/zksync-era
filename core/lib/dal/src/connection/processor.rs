use std::{
    panic::Location,
    time::{Duration, Instant},
};

use sqlx::{pool::PoolConnection, Connection, PgConnection, Postgres, Transaction};

use crate::metrics::CONNECTION_METRICS;

/// Tags that can be associated with a connection.
#[derive(Debug)]
pub(super) struct StorageProcessorTags {
    pub requester: &'static str,
    pub location: &'static Location<'static>,
}

#[derive(Debug)]
struct PooledStorageProcessor {
    connection: PoolConnection<Postgres>,
    tags: Option<StorageProcessorTags>,
    created_at: Instant,
}

impl Drop for PooledStorageProcessor {
    fn drop(&mut self) {
        const LONG_CONNECTION_THRESHOLD: Duration = Duration::from_secs(5);

        if let Some(tags) = &self.tags {
            let connection_duration = self.created_at.elapsed();
            CONNECTION_METRICS.connection_lifetime[&tags.requester].observe(connection_duration);

            if connection_duration > LONG_CONNECTION_THRESHOLD {
                let file = tags.location.file();
                let line = tags.location.line();
                tracing::info!(
                    "Long-living connection for `{}` created at {file}:{line}: {connection_duration:?}",
                    tags.requester
                );
            }
        }
    }
}

#[derive(Debug)]
enum StorageProcessorInner<'a> {
    Pooled(PooledStorageProcessor),
    Transaction(Transaction<'a, Postgres>),
}

/// Storage processor is the main storage interaction point.
/// It holds down the connection (either direct or pooled) to the database
/// and provide methods to obtain different storage schema.
#[derive(Debug)]
pub struct StorageProcessor<'a> {
    inner: StorageProcessorInner<'a>,
}

impl<'a> StorageProcessor<'a> {
    pub async fn start_transaction<'c: 'b, 'b>(&'c mut self) -> sqlx::Result<StorageProcessor<'b>> {
        let transaction = self.conn().begin().await?;
        let inner = StorageProcessorInner::Transaction(transaction);
        Ok(StorageProcessor { inner })
    }

    /// Checks if the `StorageProcessor` is currently within database transaction.
    pub fn in_transaction(&self) -> bool {
        matches!(self.inner, StorageProcessorInner::Transaction(_))
    }

    pub async fn commit(self) -> sqlx::Result<()> {
        if let StorageProcessorInner::Transaction(transaction) = self.inner {
            transaction.commit().await
        } else {
            panic!("StorageProcessor::commit can only be invoked after calling StorageProcessor::begin_transaction");
        }
    }

    /// Creates a `StorageProcessor` using a pool of connections.
    /// This method borrows one of the connections from the pool, and releases it
    /// after `drop`.
    pub(super) fn from_pool(
        connection: PoolConnection<Postgres>,
        tags: Option<StorageProcessorTags>,
    ) -> Self {
        let inner = StorageProcessorInner::Pooled(PooledStorageProcessor {
            connection,
            tags,
            created_at: Instant::now(),
        });
        Self { inner }
    }

    pub(crate) fn conn(&mut self) -> &mut PgConnection {
        match &mut self.inner {
            StorageProcessorInner::Pooled(pooled) => &mut pooled.connection,
            StorageProcessorInner::Transaction(transaction) => transaction,
        }
    }
}

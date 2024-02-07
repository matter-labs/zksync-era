use std::{
    collections::HashMap,
    fmt,
    panic::Location,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
    time::{Duration, Instant, SystemTime},
};

use sqlx::{pool::PoolConnection, types::chrono, Connection, PgConnection, Postgres, Transaction};

use crate::metrics::CONNECTION_METRICS;

/// Tags that can be associated with a connection.
#[derive(Debug, Clone, Copy)]
pub(super) struct StorageProcessorTags {
    pub requester: &'static str,
    pub location: &'static Location<'static>,
}

impl fmt::Display for StorageProcessorTags {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "requested by `{}` at {}:{}",
            self.requester,
            self.location.file(),
            self.location.line()
        )
    }
}

struct TracedConnectionInfo {
    tags: Option<StorageProcessorTags>,
    created_at: Instant,
}

impl fmt::Debug for TracedConnectionInfo {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let timestamp: chrono::DateTime<chrono::Utc> =
            (SystemTime::now() - self.created_at.elapsed()).into();
        let tags_display: &dyn fmt::Display = match &self.tags {
            Some(tags) => tags,
            None => &"not tagged",
        };
        write!(formatter, "[{timestamp} - {tags_display}]")
    }
}

/// Traced active connections for a connection pool.
#[derive(Default)]
pub(super) struct TracedConnections {
    connections: Mutex<HashMap<usize, TracedConnectionInfo>>,
    next_id: AtomicUsize,
}

impl fmt::Debug for TracedConnections {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Ok(guard) = self.connections.lock() {
            formatter.debug_set().entries(guard.values()).finish()
        } else {
            formatter.write_str("(poisoned)")
        }
    }
}

impl TracedConnections {
    fn acquire(&self, tags: Option<StorageProcessorTags>, created_at: Instant) -> usize {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let mut guard = self
            .connections
            .lock()
            .expect("`TracedConnections` is poisoned");
        let info = TracedConnectionInfo { tags, created_at };
        guard.insert(id, info);
        id
    }

    fn mark_as_dropped(&self, connection_id: usize) {
        let mut guard = self
            .connections
            .lock()
            .expect("`TracedConnections` is poisoned");
        guard.remove(&connection_id);
    }
}

struct PooledStorageProcessor<'a> {
    connection: PoolConnection<Postgres>,
    tags: Option<StorageProcessorTags>,
    created_at: Instant,
    traced: Option<(&'a TracedConnections, usize)>,
}

impl fmt::Debug for PooledStorageProcessor<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PooledStorageProcessor")
            .field("tags", &self.tags)
            .field("created_at", &self.created_at)
            .finish_non_exhaustive()
    }
}

impl Drop for PooledStorageProcessor<'_> {
    fn drop(&mut self) {
        const LONG_CONNECTION_THRESHOLD: Duration = Duration::from_secs(5);

        if let Some(tags) = &self.tags {
            let lifetime = self.created_at.elapsed();
            CONNECTION_METRICS.lifetime[&tags.requester].observe(lifetime);

            if lifetime > LONG_CONNECTION_THRESHOLD {
                let file = tags.location.file();
                let line = tags.location.line();
                tracing::info!(
                    "Long-living connection for `{}` created at {file}:{line}: {lifetime:?}",
                    tags.requester
                );
            }
        }
        if let Some((connections, id)) = self.traced {
            connections.mark_as_dropped(id);
        }
    }
}

#[derive(Debug)]
enum StorageProcessorInner<'a> {
    Pooled(PooledStorageProcessor<'a>),
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
        traced_connections: Option<&'a TracedConnections>,
    ) -> Self {
        let created_at = Instant::now();
        let inner = StorageProcessorInner::Pooled(PooledStorageProcessor {
            connection,
            tags,
            created_at,
            traced: traced_connections.map(|connections| {
                let id = connections.acquire(tags, created_at);
                (connections, id)
            }),
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

#[cfg(test)]
mod tests {
    use crate::ConnectionPool;

    #[tokio::test]
    async fn tracing_connections() {
        let pool = ConnectionPool::constrained_test_pool(1).await;
        let connection = pool.access_storage_tagged("test").await.unwrap();
        let traced = pool.traced_connections.as_deref().unwrap();
        {
            let traced = traced.connections.lock().unwrap();
            assert_eq!(traced.len(), 1);
            let tags = traced.values().next().unwrap().tags.unwrap();
            assert_eq!(tags.requester, "test");
            assert!(tags.location.file().contains("processor.rs"), "{tags:?}");
        }
        drop(connection);

        {
            let traced = traced.connections.lock().unwrap();
            assert!(traced.is_empty());
        }

        let _connection = pool.access_storage_tagged("test").await.unwrap();
        let err = format!("{:?}", pool.access_storage().await.unwrap_err());
        // Matching strings in error messages is an anti-pattern, but we really want to test DevEx here.
        assert!(err.contains("Active connections"), "{err}");
        assert!(err.contains("requested by `test`"), "{err}");
    }
}

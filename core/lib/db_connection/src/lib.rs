mod metrics;

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
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StorageProcessorTags {
    pub requester: &'static str,
    pub location: &'static Location<'static>,
}

impl StorageProcessorTags {
    pub fn display(this: Option<&Self>) -> &(dyn fmt::Display + Send + Sync) {
        this.map_or(&"not tagged", |tags| tags)
    }
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

pub struct TracedConnectionInfo {
    pub tags: Option<StorageProcessorTags>,
    created_at: Instant,
}

impl fmt::Debug for TracedConnectionInfo {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let timestamp: chrono::DateTime<chrono::Utc> =
            (SystemTime::now() - self.created_at.elapsed()).into();
        let tags_display = StorageProcessorTags::display(self.tags.as_ref());
        write!(formatter, "[{timestamp} - {tags_display}]")
    }
}

/// Traced active connections for a connection pool.
#[derive(Default)]
pub struct TracedConnections {
    pub connections: Mutex<HashMap<usize, TracedConnectionInfo>>,
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

pub struct PooledStorageProcessor<'a> {
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
        if let Some(tags) = &self.tags {
            let lifetime = self.created_at.elapsed();
            CONNECTION_METRICS.lifetime[&tags.requester].observe(lifetime);

            // TODO: value is hardcoded for now, but should be in sync with ConnectionPool::global_config()
            if lifetime > Duration::from_millis(5_000) {
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
pub enum StorageProcessorInner<'a> {
    Pooled(PooledStorageProcessor<'a>),
    Transaction {
        transaction: Transaction<'a, Postgres>,
        tags: Option<&'a StorageProcessorTags>,
    },
}

/// Storage processor is the main storage interaction point.
/// It holds down the connection (either direct or pooled) to the database
/// and provide methods to obtain different storage schema.
#[derive(Debug)]
pub struct BasicStorageProcessor<'a> {
    inner: StorageProcessorInner<'a>,
}

impl<'a> BasicStorageProcessor<'a> {
    pub async fn start_transaction(&mut self) -> sqlx::Result<BasicStorageProcessor<'_>> {
        let (conn, tags) = self.conn_and_tags();
        let inner = StorageProcessorInner::Transaction {
            transaction: conn.begin().await?,
            tags,
        };
        Ok(BasicStorageProcessor { inner })
    }

    /// Checks if the `StorageProcessor` is currently within database transaction.
    pub fn in_transaction(&self) -> bool {
        matches!(self.inner, StorageProcessorInner::Transaction { .. })
    }

    pub async fn commit(self) -> sqlx::Result<()> {
        if let StorageProcessorInner::Transaction {
            transaction: postgres,
            ..
        } = self.inner
        {
            postgres.commit().await
        } else {
            panic!("StorageProcessor::commit can only be invoked after calling StorageProcessor::begin_transaction");
        }
    }

    /// Creates a `StorageProcessor` using a pool of connections.
    /// This method borrows one of the connections from the pool, and releases it
    /// after `drop`.
    pub fn from_pool(
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

    pub fn conn(&mut self) -> &mut PgConnection {
        self.conn_and_tags().0
    }

    pub fn conn_and_tags(&mut self) -> (&mut PgConnection, Option<&StorageProcessorTags>) {
        match &mut self.inner {
            StorageProcessorInner::Pooled(pooled) => (&mut pooled.connection, pooled.tags.as_ref()),
            StorageProcessorInner::Transaction { transaction, tags } => (transaction, *tags),
        }
    }
}

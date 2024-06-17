use std::{
    collections::HashMap,
    fmt, io,
    panic::Location,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
    time::{Instant, SystemTime},
};

use sqlx::{
    pool::PoolConnection, types::chrono, Connection as _, PgConnection, Postgres, Transaction,
};

use crate::{
    connection_pool::ConnectionPool,
    error::{DalConnectionError, DalResult},
    instrument::InstrumentExt,
    metrics::CONNECTION_METRICS,
    utils::InternalMarker,
};

/// Tags that can be associated with a connection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConnectionTags {
    pub requester: &'static str,
    pub location: &'static Location<'static>,
}

impl ConnectionTags {
    pub fn display(this: Option<&Self>) -> &(dyn fmt::Display + Send + Sync) {
        this.map_or(&"not tagged", |tags| tags)
    }
}

impl fmt::Display for ConnectionTags {
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
    tags: Option<ConnectionTags>,
    created_at: Instant,
}

impl fmt::Debug for TracedConnectionInfo {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let timestamp: chrono::DateTime<chrono::Utc> =
            (SystemTime::now() - self.created_at.elapsed()).into();
        let tags_display = ConnectionTags::display(self.tags.as_ref());
        write!(formatter, "[{timestamp} - {tags_display}]")
    }
}

/// Traced active connections for a connection pool.
#[derive(Default)]
pub struct TracedConnections {
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
    fn acquire(&self, tags: Option<ConnectionTags>, created_at: Instant) -> usize {
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

struct PooledConnection<'a> {
    connection: PoolConnection<Postgres>,
    tags: Option<ConnectionTags>,
    created_at: Instant,
    traced: Option<(&'a TracedConnections, usize)>,
}

impl fmt::Debug for PooledConnection<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PooledConnection")
            .field("tags", &self.tags)
            .field("created_at", &self.created_at)
            .finish_non_exhaustive()
    }
}

impl Drop for PooledConnection<'_> {
    fn drop(&mut self) {
        if let Some(tags) = &self.tags {
            let lifetime = self.created_at.elapsed();
            CONNECTION_METRICS.lifetime[&tags.requester].observe(lifetime);

            if lifetime
                > ConnectionPool::<InternalMarker>::global_config().long_connection_threshold()
            {
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
enum ConnectionInner<'a> {
    Pooled(PooledConnection<'a>),
    Transaction {
        transaction: Transaction<'a, Postgres>,
        tags: Option<&'a ConnectionTags>,
    },
}

/// Marker trait for restricting using all possible types as a storage marker.
pub trait DbMarker: 'static + Send + Sync + Clone {}

/// Storage processor is the main storage interaction point.
/// It holds down the connection (either direct or pooled) to the database
/// and provide methods to obtain different storage schema.
#[derive(Debug)]
pub struct Connection<'a, DB: DbMarker> {
    inner: ConnectionInner<'a>,
    _marker: std::marker::PhantomData<DB>,
}

impl<'a, DB: DbMarker> Connection<'a, DB> {
    /// Creates a `Connection` using a pool of connections.
    /// This method borrows one of the connections from the pool, and releases it
    /// after `drop`.
    pub(crate) fn from_pool(
        connection: PoolConnection<Postgres>,
        tags: Option<ConnectionTags>,
        traced_connections: Option<&'a TracedConnections>,
    ) -> Self {
        let created_at = Instant::now();
        let inner = ConnectionInner::Pooled(PooledConnection {
            connection,
            tags,
            created_at,
            traced: traced_connections.map(|connections| {
                let id = connections.acquire(tags, created_at);
                (connections, id)
            }),
        });
        Self {
            inner,
            _marker: Default::default(),
        }
    }

    /// Starts a transaction or a new checkpoint within the current transaction.
    pub async fn start_transaction(&mut self) -> DalResult<Connection<'_, DB>> {
        let (conn, tags) = self.conn_and_tags();
        let inner = ConnectionInner::Transaction {
            transaction: conn
                .begin()
                .await
                .map_err(|err| DalConnectionError::start_transaction(err, tags.cloned()))?,
            tags,
        };
        Ok(Connection {
            inner,
            _marker: Default::default(),
        })
    }

    /// Starts building a new transaction with custom settings. Unlike [`Self::start_transaction()`], this method
    /// will error if called from a transaction; it is a logical error to change transaction settings in the middle of it.
    pub fn transaction_builder(&mut self) -> DalResult<TransactionBuilder<'_, 'a, DB>> {
        if let ConnectionInner::Transaction { tags, .. } = &self.inner {
            let err = io::Error::new(
                io::ErrorKind::Other,
                "`Connection::transaction_builder()` can only be invoked outside of a transaction",
            );
            return Err(
                DalConnectionError::start_transaction(sqlx::Error::Io(err), tags.cloned()).into(),
            );
        }
        Ok(TransactionBuilder {
            connection: self,
            is_readonly: false,
        })
    }

    /// Checks if the `Connection` is currently within database transaction.
    pub fn in_transaction(&self) -> bool {
        matches!(self.inner, ConnectionInner::Transaction { .. })
    }

    /// Commits a transactional connection (one which was created by calling [`Self::start_transaction()`]).
    /// If this connection is not transactional, returns an error.
    pub async fn commit(self) -> DalResult<()> {
        match self.inner {
            ConnectionInner::Transaction {
                transaction: postgres,
                tags,
            } => postgres
                .commit()
                .await
                .map_err(|err| DalConnectionError::commit_transaction(err, tags.cloned()).into()),
            ConnectionInner::Pooled(conn) => {
                let err = io::Error::new(
                    io::ErrorKind::Other,
                    "`Connection::commit()` can only be invoked after calling `Connection::begin_transaction()`",
                );
                Err(DalConnectionError::commit_transaction(sqlx::Error::Io(err), conn.tags).into())
            }
        }
    }

    /// Rolls back a transactional connection (one which was created by calling [`Self::start_transaction()`]).
    /// If this connection is not transactional, returns an error.
    pub async fn rollback(self) -> DalResult<()> {
        match self.inner {
            ConnectionInner::Transaction {
                transaction: postgres,
                tags,
            } => postgres
                .rollback()
                .await
                .map_err(|err| DalConnectionError::rollback_transaction(err, tags.cloned()).into()),
            ConnectionInner::Pooled(conn) => {
                let err = io::Error::new(
                    io::ErrorKind::Other,
                    "`Connection::rollback()` can only be invoked after calling `Connection::begin_transaction()`",
                );
                Err(
                    DalConnectionError::rollback_transaction(sqlx::Error::Io(err), conn.tags)
                        .into(),
                )
            }
        }
    }

    pub fn conn(&mut self) -> &mut PgConnection {
        self.conn_and_tags().0
    }

    pub fn conn_and_tags(&mut self) -> (&mut PgConnection, Option<&ConnectionTags>) {
        match &mut self.inner {
            ConnectionInner::Pooled(pooled) => (&mut pooled.connection, pooled.tags.as_ref()),
            ConnectionInner::Transaction { transaction, tags } => (transaction, *tags),
        }
    }
}

/// Builder of transactions allowing to configure transaction characteristics (for now, just its readonly status).
#[derive(Debug)]
pub struct TransactionBuilder<'a, 'c, DB: DbMarker> {
    connection: &'a mut Connection<'c, DB>,
    is_readonly: bool,
}

impl<'a, DB: DbMarker> TransactionBuilder<'a, '_, DB> {
    /// Sets the readonly status of the created transaction.
    pub fn set_readonly(mut self) -> Self {
        self.is_readonly = true;
        self
    }

    /// Builds the transaction with the provided characteristics.
    pub async fn build(self) -> DalResult<Connection<'a, DB>> {
        let mut transaction = self.connection.start_transaction().await?;
        if self.is_readonly {
            sqlx::query("SET TRANSACTION READ ONLY")
                .instrument("set_transaction_characteristics")
                .execute(&mut transaction)
                .await?;
        }
        Ok(transaction)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn processor_tags_propagate_to_transactions() {
        let pool = ConnectionPool::<InternalMarker>::constrained_test_pool(1).await;
        let mut connection = pool.connection_tagged("test").await.unwrap();
        assert!(!connection.in_transaction());
        let original_tags = *connection.conn_and_tags().1.unwrap();
        assert_eq!(original_tags.requester, "test");

        let mut transaction = connection.start_transaction().await.unwrap();
        let transaction_tags = *transaction.conn_and_tags().1.unwrap();
        assert_eq!(transaction_tags, original_tags);
    }

    #[tokio::test]
    async fn tracing_connections() {
        let pool = ConnectionPool::<InternalMarker>::constrained_test_pool(1).await;
        let connection = pool.connection_tagged("test").await.unwrap();
        let traced = pool.traced_connections.as_deref().unwrap();
        {
            let traced = traced.connections.lock().unwrap();
            assert_eq!(traced.len(), 1);
            let tags = traced.values().next().unwrap().tags.unwrap();
            assert_eq!(tags.requester, "test");
            assert!(tags.location.file().contains("connection.rs"), "{tags:?}");
        }
        drop(connection);

        {
            let traced = traced.connections.lock().unwrap();
            assert!(traced.is_empty());
        }
    }

    #[tokio::test]
    async fn creating_readonly_transaction() {
        let pool = ConnectionPool::<InternalMarker>::constrained_test_pool(1).await;
        let mut connection = pool.connection().await.unwrap();
        let mut readonly_transaction = connection
            .transaction_builder()
            .unwrap()
            .set_readonly()
            .build()
            .await
            .unwrap();
        assert!(readonly_transaction.in_transaction());

        sqlx::query("SELECT COUNT(*) AS \"count?\" FROM miniblocks")
            .instrument("test")
            .fetch_optional(&mut readonly_transaction)
            .await
            .unwrap()
            .expect("no row returned");
        // Check that it's impossible to execute write statements in the transaction.
        sqlx::query("DELETE FROM miniblocks")
            .instrument("test")
            .execute(&mut readonly_transaction)
            .await
            .unwrap_err();
    }
}

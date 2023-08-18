// External imports
use sqlx::{
    pool::PoolConnection,
    postgres::{PgConnectOptions, PgPool, PgPoolOptions, Postgres},
};
// Built-in deps
use std::time::{Duration, Instant};
// Workspace imports
use zksync_utils::parse_env;
// Local imports
use crate::{
    get_master_database_url, get_prover_database_url, get_replica_database_url, StorageProcessor,
};

pub mod holder;
pub mod test_pool;

pub use self::test_pool::TestPool;

#[derive(Debug, Clone, Copy)]
pub enum DbVariant {
    Master,
    Replica,
    Prover,
}

/// Builder for [`ConnectionPool`]s.
#[derive(Debug)]
pub struct ConnectionPoolBuilder {
    db: DbVariant,
    max_size: Option<u32>,
    statement_timeout: Option<Duration>,
}

impl ConnectionPoolBuilder {
    /// Sets the maximum size of the created pool. If not specified, the max pool size will be
    /// taken from the `DATABASE_POOL_SIZE` env variable.
    pub fn set_max_size(&mut self, max_size: Option<u32>) -> &mut Self {
        self.max_size = max_size;
        self
    }

    /// Sets the statement timeout for the pool. See [Postgres docs] for semantics.
    /// If not specified, the statement timeout will not be set.
    ///
    /// [Postgres docs]: https://www.postgresql.org/docs/14/runtime-config-client.html
    pub fn set_statement_timeout(&mut self, timeout: Option<Duration>) -> &mut Self {
        self.statement_timeout = timeout;
        self
    }

    /// Builds a connection pool from this builder.
    pub async fn build(&self) -> ConnectionPool {
        let database_url = match self.db {
            DbVariant::Master => get_master_database_url(),
            DbVariant::Replica => get_replica_database_url(),
            DbVariant::Prover => get_prover_database_url(),
        };
        self.build_inner(&database_url).await
    }

    pub async fn build_inner(&self, database_url: &str) -> ConnectionPool {
        let max_connections = self
            .max_size
            .unwrap_or_else(|| parse_env("DATABASE_POOL_SIZE"));

        let options = PgPoolOptions::new().max_connections(max_connections);
        let mut connect_options: PgConnectOptions = database_url.parse().unwrap_or_else(|err| {
            panic!("Failed parsing {:?} database URL: {}", self.db, err);
        });
        if let Some(timeout) = self.statement_timeout {
            let timeout_string = format!("{}s", timeout.as_secs());
            connect_options = connect_options.options([("statement_timeout", timeout_string)]);
        }
        let pool = options
            .connect_with(connect_options)
            .await
            .unwrap_or_else(|err| {
                panic!("Failed connecting to {:?} database: {}", self.db, err);
            });
        vlog::info!(
            "Created pool for {db:?} database with {max_connections} max connections \
             and {statement_timeout:?} statement timeout",
            db = self.db,
            statement_timeout = self.statement_timeout
        );
        ConnectionPool::Real(pool)
    }
}

#[derive(Debug, Clone)]
pub enum ConnectionPool {
    Real(PgPool),
    Test(TestPool),
}

impl ConnectionPool {
    /// Initializes a builder for connection pools.
    pub fn builder(db: DbVariant) -> ConnectionPoolBuilder {
        ConnectionPoolBuilder {
            db,
            max_size: None,
            statement_timeout: None,
        }
    }

    /// Initializes a builder for connection pools with a single connection. This is equivalent
    /// to calling `Self::builder(db).set_max_size(Some(1))`.
    pub fn singleton(db: DbVariant) -> ConnectionPoolBuilder {
        ConnectionPoolBuilder {
            db,
            max_size: Some(1),
            statement_timeout: None,
        }
    }

    /// Creates a `StorageProcessor` entity over a recoverable connection.
    /// Upon a database outage connection will block the thread until
    /// it will be able to recover the connection (or, if connection cannot
    /// be restored after several retries, this will be considered as
    /// irrecoverable database error and result in panic).
    ///
    /// This method is intended to be used in crucial contexts, where the
    /// database access is must-have (e.g. block committer).
    pub async fn access_storage(&self) -> StorageProcessor<'_> {
        self.access_storage_inner(None).await
    }

    /// A version of `access_storage` that would also expose the duration of the connection
    /// acquisition tagged to the `requester` name.
    ///
    /// WARN: This method should not be used if it will result in too many time series (e.g.
    /// from witness generators or provers), otherwise Prometheus won't be able to handle it.
    pub async fn access_storage_tagged(&self, requester: &'static str) -> StorageProcessor<'_> {
        self.access_storage_inner(Some(requester)).await
    }

    async fn access_storage_inner(&self, requester: Option<&'static str>) -> StorageProcessor<'_> {
        match self {
            ConnectionPool::Real(real_pool) => {
                let start = Instant::now();
                let conn = Self::acquire_connection_retried(real_pool).await;
                metrics::histogram!("sql.connection_acquire", start.elapsed());
                if let Some(requester) = requester {
                    metrics::histogram!("sql.connection_acquire.tagged", start.elapsed(), "requester" => requester);
                }
                StorageProcessor::from_pool(conn)
            }
            ConnectionPool::Test(test) => test.access_storage().await,
        }
    }

    async fn acquire_connection_retried(pool: &PgPool) -> PoolConnection<Postgres> {
        const DB_CONNECTION_RETRIES: u32 = 3;
        const BACKOFF_INTERVAL: Duration = Duration::from_secs(1);

        let mut retry_count = 0;
        while retry_count < DB_CONNECTION_RETRIES {
            metrics::histogram!("sql.connection_pool.size", pool.size() as f64);
            metrics::histogram!("sql.connection_pool.idle", pool.num_idle() as f64);

            let connection = pool.acquire().await;
            let connection_err = match connection {
                Ok(connection) => return connection,
                Err(err) => {
                    retry_count += 1;
                    err
                }
            };

            Self::report_connection_error(&connection_err);
            vlog::warn!(
                "Failed to get connection to DB, backing off for {BACKOFF_INTERVAL:?}: {connection_err}"
            );
            tokio::time::sleep(BACKOFF_INTERVAL).await;
        }

        // Attempting to get the pooled connection for the last time
        pool.acquire().await.unwrap_or_else(|connection_err| {
            Self::report_connection_error(&connection_err);
            panic!(
                "Run out of retries getting a DB connection; last error: {}",
                connection_err
            );
        })
    }

    fn report_connection_error(err: &sqlx::Error) {
        let kind = match err {
            sqlx::Error::PoolTimedOut => "timeout",
            sqlx::Error::Database(_) => "database",
            sqlx::Error::Io(_) => "io",
            _ => "other",
        };
        metrics::increment_counter!("sql.connection_pool.acquire_error", "kind" => kind);
    }

    pub async fn access_test_storage(&self) -> StorageProcessor<'static> {
        match self {
            ConnectionPool::Test(test) => test.access_storage().await,
            ConnectionPool::Real(_) => {
                panic!("Attempt to access test storage with the real pool");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::*;
    use crate::get_test_database_url;

    #[tokio::test]
    async fn setting_statement_timeout() {
        // We cannot use an ordinary test pool here because it isn't created using `ConnectionPoolBuilder`.
        // Since we don't need to mutate the DB for the test, using a real DB connection is OK.
        let database_url = get_test_database_url();
        let pool = ConnectionPool::builder(DbVariant::Master)
            .set_statement_timeout(Some(Duration::from_secs(1)))
            .build_inner(&database_url)
            .await;

        // NB. We must not mutate the database below! Doing so may break other tests.
        let mut conn = pool.access_storage().await;
        let err = sqlx::query("SELECT pg_sleep(2)")
            .map(drop)
            .fetch_optional(conn.conn())
            .await
            .unwrap_err();
        assert_matches!(
            err,
            sqlx::Error::Database(db_err) if db_err.message().contains("statement timeout")
        );
    }
}

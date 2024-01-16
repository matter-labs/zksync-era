use std::{env, fmt, time::Duration};

use anyhow::Context as _;
use sqlx::{
    pool::PoolConnection,
    postgres::{PgConnectOptions, PgPool, PgPoolOptions, Postgres},
};

use crate::{metrics::CONNECTION_METRICS, StorageProcessor};

pub mod holder;

/// Builder for [`ConnectionPool`]s.
#[derive(Clone)]
pub struct ConnectionPoolBuilder {
    database_url: String,
    max_size: u32,
    statement_timeout: Option<Duration>,
}

impl fmt::Debug for ConnectionPoolBuilder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Database URL is potentially sensitive, thus we omit it.
        formatter
            .debug_struct("ConnectionPoolBuilder")
            .field("max_size", &self.max_size)
            .field("statement_timeout", &self.statement_timeout)
            .finish()
    }
}

impl ConnectionPoolBuilder {
    /// Sets the statement timeout for the pool. See [Postgres docs] for semantics.
    /// If not specified, the statement timeout will not be set.
    ///
    /// [Postgres docs]: https://www.postgresql.org/docs/14/runtime-config-client.html
    pub fn set_statement_timeout(&mut self, timeout: Option<Duration>) -> &mut Self {
        self.statement_timeout = timeout;
        self
    }

    /// Builds a connection pool from this builder.
    pub async fn build(&self) -> anyhow::Result<ConnectionPool> {
        let options = PgPoolOptions::new().max_connections(self.max_size);
        let mut connect_options: PgConnectOptions = self
            .database_url
            .parse()
            .context("Failed parsing database URL")?;
        if let Some(timeout) = self.statement_timeout {
            let timeout_string = format!("{}s", timeout.as_secs());
            connect_options = connect_options.options([("statement_timeout", timeout_string)]);
        }
        let pool = options
            .connect_with(connect_options)
            .await
            .context("Failed connecting to database")?;
        tracing::info!(
            "Created pool with {max_connections} max connections \
             and {statement_timeout:?} statement timeout",
            max_connections = self.max_size,
            statement_timeout = self.statement_timeout
        );
        Ok(ConnectionPool {
            database_url: self.database_url.clone(),
            inner: pool,
            max_size: self.max_size,
        })
    }

    /// Builds a connection pool that has a single connection.
    pub async fn build_singleton(&self) -> anyhow::Result<ConnectionPool> {
        let singleton_builder = Self {
            max_size: 1,
            ..self.clone()
        };
        singleton_builder.build().await
    }
}

#[derive(Clone)]
pub struct ConnectionPool {
    pub(crate) inner: PgPool,
    database_url: String,
    max_size: u32,
}

impl fmt::Debug for ConnectionPool {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // We don't print the `database_url`, as is may contain
        // sensitive information (e.g. database password).
        formatter
            .debug_struct("ConnectionPool")
            .field("max_size", &self.max_size)
            .finish_non_exhaustive()
    }
}

pub struct TestTemplate(url::Url);

impl TestTemplate {
    fn db_name(&self) -> &str {
        self.0.path().strip_prefix('/').unwrap()
    }

    fn url(&self, db_name: &str) -> url::Url {
        let mut url = self.0.clone();
        url.set_path(db_name);
        url
    }

    async fn connect_to(db_url: &url::Url) -> sqlx::Result<sqlx::PgConnection> {
        use sqlx::Connection as _;
        let mut attempts = 10;
        loop {
            match sqlx::PgConnection::connect(db_url.as_ref()).await {
                Ok(conn) => return Ok(conn),
                Err(err) => {
                    attempts -= 1;
                    if attempts == 0 {
                        return Err(err);
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    /// Obtains the test database URL from the environment variable.
    pub fn empty() -> anyhow::Result<Self> {
        let db_url = env::var("TEST_DATABASE_URL").context(
            "TEST_DATABASE_URL must be set. Normally, this is done by the 'zk' tool. \
            Make sure that you are running the tests with 'zk test rust' command or equivalent.",
        )?;
        Ok(Self(db_url.parse()?))
    }

    /// Closes the connection pool, disallows connecting to the underlying db,
    /// so that the db can be used as a template.
    pub async fn freeze(pool: ConnectionPool) -> anyhow::Result<Self> {
        use sqlx::Executor as _;
        let mut conn = pool.acquire_connection_retried().await?;
        conn.execute(
            "UPDATE pg_database SET datallowconn = false WHERE datname = current_database()",
        )
        .await
        .context("SET dataallowconn = false")?;
        drop(conn);
        pool.inner.close().await;
        Ok(Self(pool.database_url.parse()?))
    }

    /// Constructs a new temporary database (with a randomized name)
    /// by cloning the database template pointed by TEST_DATABASE_URL env var.
    /// The template is expected to have all migrations from dal/migrations applied.
    /// For efficiency, the Postgres container of TEST_DATABASE_URL should be
    /// configured with option "fsync=off" - it disables waiting for disk synchronization
    /// whenever you write to the DBs, therefore making it as fast as an in-memory Postgres instance.
    /// The database is not cleaned up automatically, but rather the whole Postgres
    /// container is recreated whenever you call "zk test rust".
    pub async fn create_db(&self) -> anyhow::Result<ConnectionPool> {
        use rand::Rng as _;
        use sqlx::Executor as _;

        let mut conn = Self::connect_to(&self.url(""))
            .await
            .context("connect_to()")?;
        let db_old = self.db_name();
        let db_new = format!("test-{}", rand::thread_rng().gen::<u64>());
        conn.execute(format!("CREATE DATABASE \"{db_new}\" WITH TEMPLATE \"{db_old}\"").as_str())
            .await
            .context("CREATE DATABASE")?;

        const TEST_MAX_CONNECTIONS: u32 = 50; // Expected to be enough for any unit test.
        ConnectionPool::builder(self.url(&db_new).as_ref(), TEST_MAX_CONNECTIONS)
            .build()
            .await
            .context("ConnectionPool::builder()")
    }
}

impl ConnectionPool {
    pub async fn test_pool() -> ConnectionPool {
        TestTemplate::empty().unwrap().create_db().await.unwrap()
    }

    /// Initializes a builder for connection pools.
    pub fn builder(database_url: &str, max_pool_size: u32) -> ConnectionPoolBuilder {
        ConnectionPoolBuilder {
            database_url: database_url.to_string(),
            max_size: max_pool_size,
            statement_timeout: None,
        }
    }

    /// Initializes a builder for connection pools with a single connection. This is equivalent
    /// to calling `Self::builder(db_url, 1)`.
    pub fn singleton(database_url: &str) -> ConnectionPoolBuilder {
        Self::builder(database_url, 1)
    }

    /// Returns the maximum number of connections in this pool specified during its creation.
    /// This number may be distinct from the current number of connections in the pool (including
    /// idle ones).
    pub fn max_size(&self) -> u32 {
        self.max_size
    }

    /// Creates a `StorageProcessor` entity over a recoverable connection.
    /// Upon a database outage connection will block the thread until
    /// it will be able to recover the connection (or, if connection cannot
    /// be restored after several retries, this will be considered as
    /// irrecoverable database error and result in panic).
    ///
    /// This method is intended to be used in crucial contexts, where the
    /// database access is must-have (e.g. block committer).
    pub async fn access_storage(&self) -> anyhow::Result<StorageProcessor<'_>> {
        self.access_storage_inner(None).await
    }

    /// A version of `access_storage` that would also expose the duration of the connection
    /// acquisition tagged to the `requester` name.
    ///
    /// WARN: This method should not be used if it will result in too many time series (e.g.
    /// from witness generators or provers), otherwise Prometheus won't be able to handle it.
    pub async fn access_storage_tagged(
        &self,
        requester: &'static str,
    ) -> anyhow::Result<StorageProcessor<'_>> {
        self.access_storage_inner(Some(requester)).await
    }

    async fn access_storage_inner(
        &self,
        requester: Option<&'static str>,
    ) -> anyhow::Result<StorageProcessor<'_>> {
        let acquire_latency = CONNECTION_METRICS.acquire.start();
        let conn = self
            .acquire_connection_retried()
            .await
            .context("acquire_connection_retried()")?;
        let elapsed = acquire_latency.observe();
        if let Some(requester) = requester {
            CONNECTION_METRICS.acquire_tagged[&requester].observe(elapsed);
        }
        Ok(StorageProcessor::from_pool(conn))
    }

    async fn acquire_connection_retried(&self) -> anyhow::Result<PoolConnection<Postgres>> {
        const DB_CONNECTION_RETRIES: u32 = 3;
        const BACKOFF_INTERVAL: Duration = Duration::from_secs(1);

        let mut retry_count = 0;
        while retry_count < DB_CONNECTION_RETRIES {
            CONNECTION_METRICS
                .pool_size
                .observe(self.inner.size() as usize);
            CONNECTION_METRICS.pool_idle.observe(self.inner.num_idle());

            let connection = self.inner.acquire().await;
            let connection_err = match connection {
                Ok(connection) => return Ok(connection),
                Err(err) => {
                    retry_count += 1;
                    err
                }
            };

            Self::report_connection_error(&connection_err);
            tracing::warn!(
                "Failed to get connection to DB, backing off for {BACKOFF_INTERVAL:?}: {connection_err}"
            );
            tokio::time::sleep(BACKOFF_INTERVAL).await;
        }

        // Attempting to get the pooled connection for the last time
        match self.inner.acquire().await {
            Ok(conn) => Ok(conn),
            Err(err) => {
                Self::report_connection_error(&err);
                anyhow::bail!("Run out of retries getting a DB connection, last error: {err}");
            }
        }
    }

    fn report_connection_error(err: &sqlx::Error) {
        CONNECTION_METRICS.pool_acquire_error[&err.into()].inc();
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::*;

    #[tokio::test]
    async fn setting_statement_timeout() {
        let db_url = TestTemplate::empty()
            .unwrap()
            .create_db()
            .await
            .unwrap()
            .database_url;

        let pool = ConnectionPool::singleton(&db_url)
            .set_statement_timeout(Some(Duration::from_secs(1)))
            .build()
            .await
            .unwrap();

        let mut storage = pool.access_storage().await.unwrap();
        let err = sqlx::query("SELECT pg_sleep(2)")
            .map(drop)
            .fetch_optional(storage.conn())
            .await
            .unwrap_err();
        assert_matches!(
            err,
            sqlx::Error::Database(db_err) if db_err.message().contains("statement timeout")
        );
    }
}

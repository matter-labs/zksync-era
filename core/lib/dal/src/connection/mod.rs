use sqlx::{
    pool::PoolConnection,
    postgres::{PgConnectOptions, PgPool, PgPoolOptions, Postgres},
};

use anyhow::Context as _;
use std::fmt;
use std::time::Duration;

use zksync_utils::parse_env;

pub mod holder;
pub mod test_pool;

use holder::Connection;

use crate::{
    get_master_database_url, get_prover_database_url, get_replica_database_url,
    metrics::CONNECTION_METRICS, StorageProcessor,
};

#[derive(Debug, Clone, Copy)]
pub enum DbVariant {
    Master,
    Replica,
    Prover,
    TestTmp,
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
    /// [Postgres docs]: https//www.postgresql.org/docs/14/runtime-config-client.html
    pub fn set_statement_timeout(&mut self, timeout: Option<Duration>) -> &mut Self {
        self.statement_timeout = timeout;
        self
    }

    /// Builds a connection pool from this builder.
    pub async fn build(&self) -> anyhow::Result<ConnectionPool> {
        let database_url = match self.db {
            DbVariant::Master => get_master_database_url()?,
            DbVariant::Replica => get_replica_database_url()?,
            DbVariant::Prover => get_prover_database_url()?,
            DbVariant::TestTmp => test_pool::new_db().await.to_string(),
        };
        self.build_inner(&database_url)
            .await
            .context("build_inner()")
    }

    pub async fn build_inner(&self, database_url: &str) -> anyhow::Result<ConnectionPool> {
        let max_connections = self
            .max_size
            .unwrap_or_else(|| parse_env("DATABASE_POOL_SIZE"));

        let options = PgPoolOptions::new().max_connections(max_connections);
        let mut connect_options: PgConnectOptions = database_url
            .parse()
            .with_context(|| format!("Failed parsing {:?} database URL", self.db))?;
        if let Some(timeout) = self.statement_timeout {
            let timeout_string = format!("{}s", timeout.as_secs());
            connect_options = connect_options.options([("statement_timeout", timeout_string)]);
        }
        let pool = options
            .connect_with(connect_options)
            .await
            .with_context(|| format!("Failed connecting to {:?} database", self.db))?;
        tracing::info!(
            "Created pool for {db:?} database with {max_connections} max connections \
             and {statement_timeout:?} statement timeout",
            db = self.db,
            statement_timeout = self.statement_timeout
        );
        Ok(ConnectionPool::Real(pool))
    }
}

#[derive(Clone)]
pub enum ConnectionPool {
    Real(PgPool),
    Test(test_pool::TestConnection),
}

impl fmt::Debug for ConnectionPool {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Real(_) => write!(f, "Real connection pool"),
            Self::Test(_) => write!(f, "Test connection pool"),
        }
    }
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
    pub async fn access_storage(&self) -> anyhow::Result<StorageProcessor> {
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
    ) -> anyhow::Result<StorageProcessor> {
        self.access_storage_inner(Some(requester)).await
    }

    async fn access_storage_inner(
        &self,
        requester: Option<&'static str>,
    ) -> anyhow::Result<StorageProcessor> {
        Ok(match self {
            ConnectionPool::Real(real_pool) => {
                let acquire_latency = CONNECTION_METRICS.acquire.start();
                let conn = Self::acquire_connection_retried(real_pool)
                    .await
                    .context("acquire_connection_retried()")?;
                let elapsed = acquire_latency.observe();
                if let Some(requester) = requester {
                    CONNECTION_METRICS.acquire_tagged[&requester].observe(elapsed);
                }
                Connection::Pooled(conn).into()
            }
            ConnectionPool::Test(test) => Connection::Test(test.clone()).into(),
        })
    }

    async fn acquire_connection_retried(pool: &PgPool) -> anyhow::Result<PoolConnection<Postgres>> {
        const DB_CONNECTION_RETRIES: u32 = 3;
        const BACKOFF_INTERVAL: Duration = Duration::from_secs(1);

        let mut retry_count = 0;
        while retry_count < DB_CONNECTION_RETRIES {
            CONNECTION_METRICS.pool_size.observe(pool.size() as usize);
            CONNECTION_METRICS.pool_idle.observe(pool.num_idle());

            let connection = pool.acquire().await;
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
        match pool.acquire().await {
            Ok(conn) => Ok(conn),
            Err(err) => {
                Self::report_connection_error(&err);
                anyhow::bail!("Run out of retries getting a DB connetion, last error: {err}");
            }
        }
    }

    fn report_connection_error(err: &sqlx::Error) {
        CONNECTION_METRICS.pool_acquire_error[&err.into()].inc();
    }

    pub async fn access_test_storage(&self) -> StorageProcessor {
        match self {
            ConnectionPool::Test(test) => Connection::Test(test.clone()).into(),
            ConnectionPool::Real(_) => self.access_storage().await.unwrap(),
        }
    }

    pub async fn test_pool() -> ConnectionPool {
        Self::builder(DbVariant::TestTmp).build().await.unwrap()
        //ConnectionPool::Test(test_pool::TestConnection::new().await)
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::*;

    #[tokio::test]
    async fn setting_statement_timeout() {
        let pool = ConnectionPool::builder(DbVariant::TestTmp)
            .set_statement_timeout(Some(Duration::from_secs(1)))
            .build()
            .await
            .unwrap();

        let mut conn = pool.access_storage().await.unwrap();
        let err = sqlx::query("SELECT pg_sleep(2)")
            .map(drop)
            .fetch_optional(conn.acquire().await.as_conn())
            .await
            .unwrap_err();
        assert_matches!(
            err,
            sqlx::Error::Database(db_err) if db_err.message().contains("statement timeout")
        );
    }
}

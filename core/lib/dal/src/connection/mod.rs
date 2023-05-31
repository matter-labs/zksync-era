// Built-in deps
use std::time::{Duration, Instant};
// External imports
use async_std::task::{block_on, sleep};
use sqlx::pool::PoolConnection;
use sqlx::postgres::{PgPool, PgPoolOptions, Postgres};
// Local imports
use crate::{get_master_database_url, get_replica_database_url, StorageProcessor};
use zksync_utils::parse_env;

pub use self::test_pool::TestPool;

pub mod holder;
pub mod test_pool;

#[derive(Clone, Debug)]
pub enum ConnectionPool {
    Real(PgPool),
    Test(TestPool),
}

impl ConnectionPool {
    /// Establishes a pool of the connections to the database and
    /// creates a new `ConnectionPool` object.
    /// pool_max_size - number of connections in pool, if not set env variable "DATABASE_POOL_SIZE" is going to be used.
    pub fn new(pool_max_size: Option<u32>, connect_to_master: bool) -> Self {
        let database_url = if connect_to_master {
            get_master_database_url()
        } else {
            get_replica_database_url()
        };
        let max_connections = pool_max_size.unwrap_or_else(|| parse_env("DATABASE_POOL_SIZE"));

        let options = PgPoolOptions::new().max_connections(max_connections);
        let pool = block_on(options.connect(&database_url)).unwrap();
        Self::Real(pool)
    }

    /// WARNING: this method is intentionally private.
    /// `zksync_dal` crate uses `async-std` runtime, whereas most of our crates use `tokio`.
    /// Calling `async-std` future from `tokio` context may cause deadlocks (and it did happen).
    /// Use blocking counterpart instead.
    ///
    /// Creates a `StorageProcessor` entity over a recoverable connection.
    /// Upon a database outage connection will block the thread until
    /// it will be able to recover the connection (or, if connection cannot
    /// be restored after several retries, this will be considered as
    /// irrecoverable database error and result in panic).
    ///
    /// This method is intended to be used in crucial contexts, where the
    /// database access is must-have (e.g. block committer).
    async fn access_storage(&self) -> StorageProcessor<'_> {
        match self {
            ConnectionPool::Real(real_pool) => {
                let start = Instant::now();
                let conn = Self::acquire_connection_retried(real_pool).await;
                metrics::histogram!("sql.connection_acquire", start.elapsed());
                StorageProcessor::from_pool(conn)
            }
            ConnectionPool::Test(test) => test.access_storage().await,
        }
    }

    async fn acquire_connection_retried(pool: &PgPool) -> PoolConnection<Postgres> {
        const DB_CONNECTION_RETRIES: u32 = 3;

        let mut retry_count = 0;

        while retry_count < DB_CONNECTION_RETRIES {
            metrics::histogram!("sql.connection_pool.size", pool.size() as f64);
            metrics::histogram!("sql.connection_pool.idle", pool.num_idle() as f64);

            let connection = pool.acquire().await;
            match connection {
                Ok(connection) => return connection,
                Err(_) => retry_count += 1,
            }

            // Backing off for one second if facing an error
            vlog::warn!("Failed to get connection to db. Backing off for 1 second");
            sleep(Duration::from_secs(1)).await;
        }

        // Attempting to get the pooled connection for the last time
        pool.acquire().await.unwrap()
    }

    pub fn access_storage_blocking(&self) -> StorageProcessor<'_> {
        block_on(self.access_storage())
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

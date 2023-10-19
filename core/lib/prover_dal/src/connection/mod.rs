use anyhow::Context as _;
use std::time::Duration;

use sqlx::{
    pool::PoolConnection,
    postgres::{PgConnectOptions, PgPool, PgPoolOptions, Postgres},
};

use zksync_dal_utils::metrics::CONNECTION_METRICS;
use zksync_utils::parse_env;

use crate::{get_prover_database_url, ProverStorageProcessor};

/// Builder for [`ProverConnectionPool`]s.
#[derive(Debug)]
pub struct ProverConnectionPoolBuilder {
    max_size: Option<u32>,
    statement_timeout: Option<Duration>,
}

impl ProverConnectionPoolBuilder {
    /// Sets the maximum size of the created pool. If not specified, the max pool size will be
    /// taken from the `HOUSE_KEEPER_PROVER_DB_POOL_SIZE` env variable.
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
    pub async fn build(&self) -> anyhow::Result<ProverConnectionPool> {
        let database_url = get_prover_database_url()?;
        Ok(self.build_inner(&database_url).await)
    }

    pub async fn build_inner(&self, database_url: &str) -> ProverConnectionPool {
        let max_connections = self
            .max_size
            .unwrap_or_else(|| parse_env("HOUSE_KEEPER_PROVER_DB_POOL_SIZE"));

        let options = PgPoolOptions::new().max_connections(max_connections);
        let mut connect_options: PgConnectOptions = database_url.parse().unwrap_or_else(|err| {
            panic!("Failed parsing prover database URL: {}", err);
        });
        if let Some(timeout) = self.statement_timeout {
            let timeout_string = format!("{}s", timeout.as_secs());
            connect_options = connect_options.options([("statement_timeout", timeout_string)]);
        }
        let pool = options
            .connect_with(connect_options)
            .await
            .unwrap_or_else(|err| {
                panic!("Failed connecting to prover database: {}", err);
            });
        tracing::info!(
            "Created pool for prover database with {max_connections} max connections \
             and {statement_timeout:?} statement timeout",
            statement_timeout = self.statement_timeout
        );
        ProverConnectionPool(pool)
    }
}

#[derive(Debug, Clone)]
pub struct ProverConnectionPool(PgPool);

impl ProverConnectionPool {
    /// Initializes a builder for connection pools.
    pub fn builder() -> ProverConnectionPoolBuilder {
        ProverConnectionPoolBuilder {
            max_size: None,
            statement_timeout: None,
        }
    }

    /// Initializes a builder for connection pools with a single connection. This is equivalent
    /// to calling `Self::builder(db).set_max_size(Some(1))`.
    pub fn singleton() -> ProverConnectionPoolBuilder {
        ProverConnectionPoolBuilder {
            max_size: Some(1),
            statement_timeout: None,
        }
    }

    /// Creates a `ProverStorageProcessor` entity over a recoverable connection.
    /// Upon a database outage connection will block the thread until
    /// it will be able to recover the connection (or, if connection cannot
    /// be restored after several retries, this will be considered as
    /// irrecoverable database error and result in panic).
    ///
    /// This method is intended to be used in crucial contexts, where the
    /// database access is must-have (e.g. block committer).
    pub async fn access_storage(&self) -> anyhow::Result<ProverStorageProcessor<'_>> {
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
    ) -> anyhow::Result<ProverStorageProcessor<'_>> {
        self.access_storage_inner(Some(requester)).await
    }

    async fn access_storage_inner(
        &self,
        requester: Option<&'static str>,
    ) -> anyhow::Result<ProverStorageProcessor<'_>> {
        Ok(match self {
            ProverConnectionPool(pool) => {
                let acquire_latency = CONNECTION_METRICS.acquire.start();
                let conn = Self::acquire_connection_retried(pool)
                    .await
                    .context("acquire_connection_retried()")?;
                let elapsed = acquire_latency.observe();
                if let Some(requester) = requester {
                    CONNECTION_METRICS.acquire_tagged[&requester].observe(elapsed);
                }
                ProverStorageProcessor::from_pool(conn)
            }
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
                "Failed to get connection to prover DB, backing off for {BACKOFF_INTERVAL:?}: {connection_err}"
            );
            tokio::time::sleep(BACKOFF_INTERVAL).await;
        }

        // Attempting to get the pooled connection for the last time
        match pool.acquire().await {
            Ok(conn) => Ok(conn),
            Err(err) => {
                Self::report_connection_error(&err);
                anyhow::bail!(
                    "Run out of retries getting a prover DB conneсtion, last error: {err}"
                );
            }
        }
    }

    fn report_connection_error(err: &sqlx::Error) {
        CONNECTION_METRICS.pool_acquire_error[&err.into()].inc();
    }
}

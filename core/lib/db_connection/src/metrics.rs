use std::time::Duration;

use vise::{
    Buckets, Counter, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram, LabeledFamily,
    Metrics,
};

/// Kind of a connection error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "kind", rename_all = "snake_case")]
pub(crate) enum ConnectionErrorKind {
    Timeout,
    Database,
    Io,
    Other,
}

impl From<&sqlx::Error> for ConnectionErrorKind {
    fn from(err: &sqlx::Error) -> Self {
        match err {
            sqlx::Error::PoolTimedOut => Self::Timeout,
            sqlx::Error::Database(_) => Self::Database,
            sqlx::Error::Io(_) => Self::Io,
            _ => Self::Other,
        }
    }
}

const POOL_SIZE_BUCKETS: Buckets = Buckets::linear(0.0..=100.0, 10.0);

/// Connection-related metrics.
#[derive(Debug, Metrics)]
#[metrics(prefix = "sql_connection")]
pub(crate) struct ConnectionMetrics {
    /// Latency of acquiring a DB connection.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub acquire: Histogram<Duration>,
    /// Latency of acquiring a DB connection, tagged with the requester label.
    #[metrics(buckets = Buckets::LATENCIES, labels = ["requester"])]
    pub acquire_tagged: LabeledFamily<&'static str, Histogram<Duration>>,
    /// Current DB pool size.
    #[metrics(buckets = POOL_SIZE_BUCKETS)]
    pub pool_size: Histogram<usize>,
    /// Current number of idle connections in the DB pool.
    #[metrics(buckets = POOL_SIZE_BUCKETS)]
    pub pool_idle: Histogram<usize>,
    /// Number of errors occurred when acquiring a DB connection.
    pub pool_acquire_error: Family<ConnectionErrorKind, Counter>,
    /// Lifetime of a DB connection, tagged with the requester label.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds, labels = ["requester"])]
    pub lifetime: LabeledFamily<&'static str, Histogram<Duration>>,
}

#[vise::register]
pub(crate) static CONNECTION_METRICS: vise::Global<ConnectionMetrics> = vise::Global::new();

#[derive(Debug, Metrics)]
#[metrics(prefix = "postgres")]
pub(crate) struct PostgresMetrics {
    /// Size of the data in a certain table as returned by `pg_table_size` function.
    #[metrics(unit = Unit::Bytes, labels = ["table"])]
    table_data_size: LabeledFamily<String, Gauge<u64>>,
    /// Size of the data in a certain table as returned by `pg_indexes_size` function.
    #[metrics(unit = Unit::Bytes, labels = ["table"])]
    table_indexes_size: LabeledFamily<String, Gauge<u64>>,
    /// Size of the data in a certain table as returned by `pg_relation_size` function.
    #[metrics(unit = Unit::Bytes, labels = ["table"])]
    table_relation_size: LabeledFamily<String, Gauge<u64>>,
    /// Size of the data in a certain table as returned by `pg_total_relation_size` function.
    #[metrics(unit = Unit::Bytes, labels = ["table"])]
    table_total_size: LabeledFamily<String, Gauge<u64>>,
}

#[vise::register]
static POSTGRES_METRICS: vise::Global<PostgresMetrics> = vise::Global::new();

impl PostgresMetrics {
    pub(crate) async fn run_scraping(pool: ConnectionPool, scrape_interval: Duration) {
        let scrape_timeout = Duration::from_secs(1).min(scrape_interval / 2);
        loop {
            match tokio::time::timeout(scrape_timeout, Self::scrape(&pool)).await {
                Err(_) => {
                    tracing::info!("Timed out scraping Postgres metrics after {scrape_timeout:?}");
                }
                Ok(Err(err)) => {
                    tracing::warn!("Error scraping Postgres metrics: {err:?}");
                }
                Ok(Ok(())) => { /* everything went fine */ }
            }
            tokio::time::sleep(scrape_interval).await;
        }
    }

    async fn scrape(pool: &ConnectionPool) -> anyhow::Result<()> {
        let mut storage = pool
            .access_storage_tagged("postgres_metrics")
            .await
            .context("cannot acquire Postgres connection")?;
        let table_sizes = storage
            .system_dal()
            .get_table_sizes()
            .await
            .context("failed getting table sizes")?;
        for (table_name, sizes) in table_sizes {
            POSTGRES_METRICS.table_data_size[&table_name].set(sizes.table_size);
            POSTGRES_METRICS.table_indexes_size[&table_name].set(sizes.indexes_size);
            POSTGRES_METRICS.table_relation_size[&table_name].set(sizes.relation_size);
            POSTGRES_METRICS.table_total_size[&table_name].set(sizes.total_size);
        }
        Ok(())
    }
}

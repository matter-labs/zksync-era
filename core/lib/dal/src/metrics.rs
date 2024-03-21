//! Metrics for the data access layer.
use std::time::Duration;

use anyhow::Context;
use vise::{Gauge, LabeledFamily, Metrics, Unit};
use zksync_db_connection::connection_pool::ConnectionPool;

use crate::{Core, CoreDal};

#[derive(Debug, Metrics)]
#[metrics(prefix = "postgres")]
pub struct PostgresMetrics {
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
    pub async fn run_scraping(pool: ConnectionPool<Core>, scrape_interval: Duration) {
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

    async fn scrape(pool: &ConnectionPool<Core>) -> anyhow::Result<()> {
        let mut storage = pool
            .connection_tagged("postgres_metrics")
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

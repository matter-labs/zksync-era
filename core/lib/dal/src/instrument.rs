//! DAL query instrumentation.

use sqlx::{
    postgres::{PgConnection, PgQueryResult, PgRow},
    query::{Map, Query, QueryAs},
    FromRow, IntoArguments, Postgres,
};
use tokio::time::{Duration, Instant};

use std::{fmt, future::Future, panic::Location};

use crate::metrics::REQUEST_METRICS;

type ThreadSafeDebug<'a> = dyn fmt::Debug + Send + Sync + 'a;

const SLOW_QUERY_TIMEOUT: Duration = Duration::from_millis(100);

/// Logged arguments for an SQL query.
#[derive(Debug, Default)]
struct QueryArgs<'a> {
    inner: Vec<(&'static str, &'a ThreadSafeDebug<'a>)>,
}

impl fmt::Display for QueryArgs<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.inner.is_empty() {
            Ok(())
        } else {
            formatter.write_str("(")?;
            for (i, (name, value)) in self.inner.iter().enumerate() {
                write!(formatter, "{name}={value:?}")?;
                if i + 1 < self.inner.len() {
                    formatter.write_str(", ")?;
                }
            }
            formatter.write_str(")")
        }
    }
}

/// Extension trait for instrumenting `sqlx::query!` outputs.
pub(crate) trait InstrumentExt: Sized {
    /// Instruments a query, assigning it the provided name.
    fn instrument(self, name: &'static str) -> Instrumented<'static, Self>;
}

impl<'q, A> InstrumentExt for Query<'q, Postgres, A>
where
    A: 'q + IntoArguments<'q, Postgres>,
{
    #[track_caller]
    fn instrument(self, name: &'static str) -> Instrumented<'static, Self> {
        Instrumented {
            query: self,
            data: InstrumentedData::new(name, Location::caller()),
        }
    }
}

impl<'q, O, A> InstrumentExt for QueryAs<'q, Postgres, O, A>
where
    A: 'q + IntoArguments<'q, Postgres>,
{
    #[track_caller]
    fn instrument(self, name: &'static str) -> Instrumented<'static, Self> {
        Instrumented {
            query: self,
            data: InstrumentedData::new(name, Location::caller()),
        }
    }
}

impl<'q, F, O, A> InstrumentExt for Map<'q, Postgres, F, A>
where
    F: FnMut(PgRow) -> Result<O, sqlx::Error> + Send,
    O: Send + Unpin,
    A: 'q + Send + IntoArguments<'q, Postgres>,
{
    #[track_caller]
    fn instrument(self, name: &'static str) -> Instrumented<'static, Self> {
        Instrumented {
            query: self,
            data: InstrumentedData::new(name, Location::caller()),
        }
    }
}

#[derive(Debug)]
struct InstrumentedData<'a> {
    name: &'static str,
    location: &'static Location<'static>,
    args: QueryArgs<'a>,
    report_latency: bool,
}

impl<'a> InstrumentedData<'a> {
    fn new(name: &'static str, location: &'static Location<'static>) -> Self {
        Self {
            name,
            location,
            args: QueryArgs::default(),
            report_latency: false,
        }
    }

    async fn fetch<R>(
        self,
        query_future: impl Future<Output = Result<R, sqlx::Error>>,
    ) -> Result<R, sqlx::Error> {
        let Self {
            name,
            location,
            args,
            report_latency,
        } = self;
        let started_at = Instant::now();
        tokio::pin!(query_future);

        let mut is_slow = false;
        let output =
            tokio::time::timeout_at(started_at + SLOW_QUERY_TIMEOUT, &mut query_future).await;
        let output = match output {
            Ok(output) => output,
            Err(_) => {
                tracing::warn!(
                    "Query {name}{args} called at {file}:{line} is executing for more than {SLOW_QUERY_TIMEOUT:?}",
                    file = location.file(),
                    line = location.line()
                );
                REQUEST_METRICS.request_slow[&name].inc();
                is_slow = true;
                query_future.await
            }
        };

        let elapsed = started_at.elapsed();
        if report_latency {
            REQUEST_METRICS.request[&name].observe(elapsed);
        }

        if let Err(err) = &output {
            tracing::warn!(
                "Query {name}{args} called at {file}:{line} has resulted in error: {err}",
                file = location.file(),
                line = location.line()
            );
            REQUEST_METRICS.request_error[&name].inc();
        } else if is_slow {
            tracing::info!(
                "Slow query {name}{args} called at {file}:{line} has finished after {elapsed:?}",
                file = location.file(),
                line = location.line()
            );
        }
        output
    }
}

/// Instrumented `sqlx` query that wraps and can be used as a drop-in replacement for `sqlx::query!` / `query_as!` output
/// (i.e., [`Map`]).
///
/// The following instrumentation logic is included:
///
/// - If the query executes for too long, it is logged with a `WARN` level. The logged info includes
///   the query name, its args provided via [Self::with_arg()`] and the caller location.
/// - If the query returns an error, it is logged with a `WARN` level. The logged info is everything
///   included in the case of a slow query, plus the error info.
/// - Slow and erroneous queries are also reported using metrics (`dal.request.slow` and `dal.request.error`,
///   respectively). The query name is included as a metric label; args are not included for obvious reasons.
#[derive(Debug)]
pub(crate) struct Instrumented<'a, Q> {
    query: Q,
    data: InstrumentedData<'a>,
}

impl<'a, Q> Instrumented<'a, Q> {
    /// Indicates that latency should be reported for all calls.
    pub fn report_latency(mut self) -> Self {
        self.data.report_latency = true;
        self
    }

    /// Adds a traced query argument. The argument will be logged (using `Debug`) if the query executes too slow
    /// or finishes with an error.
    pub fn with_arg(mut self, name: &'static str, value: &'a ThreadSafeDebug) -> Self {
        self.data.args.inner.push((name, value));
        self
    }
}

impl<'q, A> Instrumented<'_, Query<'q, Postgres, A>>
where
    A: 'q + IntoArguments<'q, Postgres>,
{
    /// Executes an SQL statement using this query.
    pub async fn execute(self, conn: &mut PgConnection) -> Result<PgQueryResult, sqlx::Error> {
        self.data.fetch(self.query.execute(conn)).await
    }

    /// Fetches an optional row using this query.
    pub async fn fetch_optional(
        self,
        conn: &mut PgConnection,
    ) -> Result<Option<PgRow>, sqlx::Error> {
        self.data.fetch(self.query.fetch_optional(conn)).await
    }
}

impl<'q, O, A> Instrumented<'_, QueryAs<'q, Postgres, O, A>>
where
    A: 'q + IntoArguments<'q, Postgres>,
    O: Send + Unpin + for<'r> FromRow<'r, PgRow>,
{
    /// Fetches all rows using this query and collects them into a `Vec`.
    pub async fn fetch_all(self, conn: &mut PgConnection) -> Result<Vec<O>, sqlx::Error> {
        self.data.fetch(self.query.fetch_all(conn)).await
    }
}

impl<'q, F, O, A> Instrumented<'_, Map<'q, Postgres, F, A>>
where
    F: FnMut(PgRow) -> Result<O, sqlx::Error> + Send,
    O: Send + Unpin,
    A: 'q + Send + IntoArguments<'q, Postgres>,
{
    /// Fetches an optional row using this query.
    pub async fn fetch_optional(self, conn: &mut PgConnection) -> Result<Option<O>, sqlx::Error> {
        self.data.fetch(self.query.fetch_optional(conn)).await
    }

    /// Fetches a single row using this query.
    pub async fn fetch_one(self, conn: &mut PgConnection) -> Result<O, sqlx::Error> {
        self.data.fetch(self.query.fetch_one(conn)).await
    }

    /// Fetches all rows using this query and collects them into a `Vec`.
    pub async fn fetch_all(self, conn: &mut PgConnection) -> Result<Vec<O>, sqlx::Error> {
        self.data.fetch(self.query.fetch_all(conn)).await
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::{MiniblockNumber, H256};

    use super::*;
    use crate::ConnectionPool;

    #[tokio::test]
    async fn instrumenting_erroneous_query() {
        let pool = ConnectionPool::test_pool().await;
        // Add `vlog::init()` here to debug this test

        let mut conn = pool.access_storage().await.unwrap();
        sqlx::query("WHAT")
            .map(drop)
            .instrument("erroneous")
            .with_arg("miniblock", &MiniblockNumber(1))
            .with_arg("hash", &H256::zero())
            .fetch_optional(conn.conn())
            .await
            .unwrap_err();
    }

    #[tokio::test]
    async fn instrumenting_slow_query() {
        let pool = ConnectionPool::test_pool().await;
        // Add `vlog::init()` here to debug this test

        let mut conn = pool.access_storage().await.unwrap();
        sqlx::query("SELECT pg_sleep(1.5)")
            .map(drop)
            .instrument("slow")
            .with_arg("miniblock", &MiniblockNumber(1))
            .with_arg("hash", &H256::zero())
            .fetch_optional(conn.conn())
            .await
            .unwrap();
    }
}

//! DAL query instrumentation.
//!
//! Query instrumentation allows to:
//!
//! - Report query latency as a metric
//! - Report slow and failing queries as metrics
//! - Log slow and failing queries together with their arguments, which makes it easier to debug.
//!
//! The entry point for instrumentation is the [`InstrumentExt`] trait. After it is imported into the scope,
//! its `instrument()` method can be placed on the output of `query*` functions or macros. You can then call
//! [`Instrumented`] methods on the returned struct, e.g. to [report query latency](Instrumented::report_latency())
//! and/or [to add logged args](Instrumented::with_arg()) for a query.

use std::{fmt, future::Future, panic::Location};

use sqlx::{
    postgres::{PgCopyIn, PgQueryResult, PgRow},
    query::{Map, Query, QueryAs, QueryScalar},
    FromRow, IntoArguments, PgConnection, Postgres,
};
use tokio::time::Instant;

use crate::{
    connection::{Connection, ConnectionTags, DbMarker},
    connection_pool::ConnectionPool,
    error::{DalError, DalRequestError, DalResult},
    metrics::{RequestLabels, RequestMetrics, REQUEST_METRICS},
    utils::InternalMarker,
};

type ThreadSafeDebug<'a> = dyn fmt::Debug + Send + Sync + 'a;

/// Logged arguments for an SQL query.
#[derive(Debug, Clone, Default)]
struct QueryArgs<'a> {
    inner: Vec<(&'static str, &'a ThreadSafeDebug<'a>)>,
}

impl QueryArgs<'_> {
    fn to_owned(&self) -> Vec<(&'static str, String)> {
        self.inner
            .iter()
            .map(|(name, value)| (*name, format!("{value:?}")))
            .collect()
    }
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
pub trait InstrumentExt: Sized {
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

impl<'q, O, A> InstrumentExt for QueryScalar<'q, Postgres, O, A>
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

/// Wrapper for a `COPY` SQL statement. To actually do something on a statement, it should be instrumented.
#[derive(Debug)]
pub struct CopyStatement {
    statement: &'static str,
}

impl CopyStatement {
    /// Creates a new statement wrapping the specified SQL.
    pub fn new(statement: &'static str) -> Self {
        Self { statement }
    }
}

impl InstrumentExt for CopyStatement {
    #[track_caller]
    fn instrument(self, name: &'static str) -> Instrumented<'static, Self> {
        Instrumented {
            query: self,
            data: InstrumentedData::new(name, Location::caller()),
        }
    }
}

/// Result of `start()`ing copying on a [`CopyStatement`].
#[must_use = "Data should be sent to database using `send()`"]
pub struct ActiveCopy<'a> {
    raw: PgCopyIn<&'a mut PgConnection>,
    data: InstrumentedData<'a>,
    tags: Option<&'a ConnectionTags>,
}

impl fmt::Debug for ActiveCopy<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ActiveCopy")
            .field("data", &self.data)
            .field("tags", &self.tags)
            .finish_non_exhaustive()
    }
}

impl ActiveCopy<'_> {
    /// Sends the specified bytes to the database and finishes the copy statement.
    // FIXME: measure latency?
    pub async fn send(mut self, data: &[u8]) -> DalResult<()> {
        let inner_send = async {
            self.raw.send(data).await?;
            self.raw.finish().await.map(drop)
        };
        inner_send.await.map_err(|err| {
            DalRequestError::new(err, self.data.name, self.data.location)
                .with_args(self.data.args.to_owned())
                .with_connection_tags(self.tags.cloned())
                .into()
        })
    }
}

#[derive(Debug, Clone)]
struct InstrumentedData<'a> {
    name: &'static str,
    metrics: vise::LazyItem<'static, RequestLabels, RequestMetrics>,
    location: &'static Location<'static>,
    args: QueryArgs<'a>,
    report_latency: bool,
    slow_query_reporting_enabled: bool,
}

impl<'a> InstrumentedData<'a> {
    fn new(name: &'static str, location: &'static Location<'static>) -> Self {
        Self {
            name,
            metrics: REQUEST_METRICS.get_lazy(name.into()),
            location,
            args: QueryArgs::default(),
            report_latency: false,
            slow_query_reporting_enabled: true,
        }
    }

    fn observe_error(&self, err: &dyn fmt::Display) {
        let InstrumentedData {
            name,
            metrics,
            location,
            args,
            ..
        } = self;
        tracing::warn!(
            "Query {name}{args} called at {file}:{line} has resulted in error: {err}",
            file = location.file(),
            line = location.line()
        );
        metrics.request_error.inc();
    }

    async fn fetch<R>(
        self,
        connection_tags: Option<&ConnectionTags>,
        query_future: impl Future<Output = Result<R, sqlx::Error>>,
    ) -> DalResult<R> {
        let Self {
            name,
            metrics,
            location,
            args,
            report_latency,
            slow_query_reporting_enabled,
        } = self;
        let started_at = Instant::now();
        tokio::pin!(query_future);

        let slow_query_threshold =
            ConnectionPool::<InternalMarker>::global_config().slow_query_threshold();
        let mut is_slow = false;
        let output =
            tokio::time::timeout_at(started_at + slow_query_threshold, &mut query_future).await;
        let output = match output {
            Ok(output) => output,
            Err(_) => {
                let connection_tags = ConnectionTags::display(connection_tags);
                if slow_query_reporting_enabled {
                    tracing::warn!(
                        "Query {name}{args} called at {file}:{line} [{connection_tags}] is executing for more than {slow_query_threshold:?}",
                        file = location.file(),
                        line = location.line()
                    );
                    metrics.request_slow.inc();
                    is_slow = true;
                }
                query_future.await
            }
        };

        let elapsed = started_at.elapsed();
        if report_latency {
            metrics.request.observe(elapsed);
        }

        let connection_tags_display = ConnectionTags::display(connection_tags);
        if let Err(err) = &output {
            tracing::warn!(
                "Query {name}{args} called at {file}:{line} [{connection_tags_display}] has resulted in error: {err}",
                file = location.file(),
                line = location.line()
            );
            metrics.request_error.inc();
        } else if is_slow {
            tracing::info!(
                "Slow query {name}{args} called at {file}:{line} [{connection_tags_display}] has finished after {elapsed:?}",
                file = location.file(),
                line = location.line()
            );
        }

        output.map_err(|err| {
            DalRequestError::new(err, name, location)
                .with_args(args.to_owned())
                .with_connection_tags(connection_tags.cloned())
                .into()
        })
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
#[derive(Debug, Clone)]
pub struct Instrumented<'a, Q> {
    query: Q,
    data: InstrumentedData<'a>,
}

impl<'a> Instrumented<'a, ()> {
    /// Creates an empty instrumentation information. This is useful if you need to validate query arguments
    /// before invoking a query.
    #[track_caller]
    pub fn new(name: &'static str) -> Self {
        Self {
            query: (),
            data: InstrumentedData::new(name, Location::caller()),
        }
    }

    /// Wraps a provided argument validation error. It is assumed that the returned error
    /// will be returned as an error cause (e.g., it is logged as an error and observed using metrics).
    #[must_use]
    pub fn arg_error<E>(&self, arg_name: &str, err: E) -> DalError
    where
        E: Into<anyhow::Error>,
    {
        let err: anyhow::Error = err.into();
        let err = err.context(format!("failed validating query argument `{arg_name}`"));
        let err = DalRequestError::new(
            sqlx::Error::Decode(err.into()),
            self.data.name,
            self.data.location,
        )
        .with_args(self.data.args.to_owned());

        self.data.observe_error(&err);
        err.into()
    }

    /// Wraps a provided application-level data constraint error. It is assumed that the returned error
    /// will be returned as an error cause (e.g., it is logged as an error and observed using metrics).
    #[must_use]
    pub fn constraint_error(&self, err: anyhow::Error) -> DalError {
        let err = err.context("application-level data constraint violation");
        let err = DalRequestError::new(
            sqlx::Error::Decode(err.into()),
            self.data.name,
            self.data.location,
        )
        .with_args(self.data.args.to_owned());

        self.data.observe_error(&err);
        err.into()
    }

    pub fn with<Q>(self, query: Q) -> Instrumented<'a, Q> {
        Instrumented {
            query,
            data: self.data,
        }
    }
}

impl<'a, Q> Instrumented<'a, Q> {
    /// Indicates that latency should be reported for all calls.
    pub fn report_latency(mut self) -> Self {
        self.data.report_latency = true;
        self
    }

    pub fn expect_slow_query(mut self) -> Self {
        self.data.slow_query_reporting_enabled = false;
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
    pub async fn execute<DB: DbMarker>(
        self,
        storage: &mut Connection<'_, DB>,
    ) -> DalResult<PgQueryResult> {
        let (conn, tags) = storage.conn_and_tags();
        self.data.fetch(tags, self.query.execute(conn)).await
    }

    /// Fetches an optional row using this query.
    pub async fn fetch_optional<DB: DbMarker>(
        self,
        storage: &mut Connection<'_, DB>,
    ) -> DalResult<Option<PgRow>> {
        let (conn, tags) = storage.conn_and_tags();
        self.data.fetch(tags, self.query.fetch_optional(conn)).await
    }
}

impl<'q, O, A> Instrumented<'_, QueryAs<'q, Postgres, O, A>>
where
    A: 'q + IntoArguments<'q, Postgres>,
    O: Send + Unpin + for<'r> FromRow<'r, PgRow>,
{
    /// Fetches all rows using this query and collects them into a `Vec`.
    pub async fn fetch_all<DB: DbMarker>(
        self,
        storage: &mut Connection<'_, DB>,
    ) -> DalResult<Vec<O>> {
        let (conn, tags) = storage.conn_and_tags();
        self.data.fetch(tags, self.query.fetch_all(conn)).await
    }
}

impl<'q, O, A> Instrumented<'_, QueryScalar<'q, Postgres, O, A>>
where
    A: 'q + IntoArguments<'q, Postgres>,
    O: Send + Unpin,
    (O,): for<'r> FromRow<'r, PgRow>,
{
    /// Fetches an optional row using this query.
    pub async fn fetch_optional<DB: DbMarker>(
        self,
        storage: &mut Connection<'_, DB>,
    ) -> DalResult<Option<O>> {
        let (conn, tags) = storage.conn_and_tags();
        self.data.fetch(tags, self.query.fetch_optional(conn)).await
    }

    /// Fetches a single row using this query.
    pub async fn fetch_one<DB: DbMarker>(self, storage: &mut Connection<'_, DB>) -> DalResult<O> {
        let (conn, tags) = storage.conn_and_tags();
        self.data.fetch(tags, self.query.fetch_one(conn)).await
    }
}

impl<'q, F, O, A> Instrumented<'_, Map<'q, Postgres, F, A>>
where
    F: FnMut(PgRow) -> Result<O, sqlx::Error> + Send,
    O: Send + Unpin,
    A: 'q + Send + IntoArguments<'q, Postgres>,
{
    /// Fetches an optional row using this query.
    pub async fn fetch_optional<DB: DbMarker>(
        self,
        storage: &mut Connection<'_, DB>,
    ) -> DalResult<Option<O>> {
        let (conn, tags) = storage.conn_and_tags();
        self.data.fetch(tags, self.query.fetch_optional(conn)).await
    }

    /// Fetches a single row using this query.
    pub async fn fetch_one<DB: DbMarker>(self, storage: &mut Connection<'_, DB>) -> DalResult<O> {
        let (conn, tags) = storage.conn_and_tags();
        self.data.fetch(tags, self.query.fetch_one(conn)).await
    }

    /// Fetches all rows using this query and collects them into a `Vec`.
    pub async fn fetch_all<DB: DbMarker>(
        self,
        storage: &mut Connection<'_, DB>,
    ) -> DalResult<Vec<O>> {
        let (conn, tags) = storage.conn_and_tags();
        self.data.fetch(tags, self.query.fetch_all(conn)).await
    }
}

impl<'a> Instrumented<'a, CopyStatement> {
    /// Starts `COPY`ing data using this statement.
    pub async fn start<DB: DbMarker>(
        self,
        storage: &'a mut Connection<'_, DB>,
    ) -> DalResult<ActiveCopy<'a>> {
        let (conn, tags) = storage.conn_and_tags();
        match conn.copy_in_raw(self.query.statement).await {
            Ok(raw) => Ok(ActiveCopy {
                raw,
                data: self.data,
                tags,
            }),
            Err(err) => Err(
                DalRequestError::new(err, self.data.name, self.data.location)
                    .with_args(self.data.args.to_owned())
                    .with_connection_tags(tags.cloned())
                    .into(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use zksync_basic_types::{L2BlockNumber, H256};

    use super::*;
    use crate::{connection_pool::ConnectionPool, utils::InternalMarker};

    #[tokio::test]
    async fn instrumenting_erroneous_query() {
        let pool = ConnectionPool::<InternalMarker>::test_pool().await;
        // Add `zksync_vlog::init()` here to debug this test

        let mut conn = pool.connection().await.unwrap();
        sqlx::query("WHAT")
            .map(drop)
            .instrument("erroneous")
            .with_arg("l2_block", &L2BlockNumber(1))
            .with_arg("hash", &H256::zero())
            .fetch_optional(&mut conn)
            .await
            .unwrap_err();
    }

    #[tokio::test]
    async fn instrumenting_slow_query() {
        let pool = ConnectionPool::<InternalMarker>::test_pool().await;
        // Add `zksync_vlog::init()` here to debug this test

        let mut conn = pool.connection().await.unwrap();
        sqlx::query("SELECT pg_sleep(1.5)")
            .map(drop)
            .instrument("slow")
            .with_arg("l2_block", &L2BlockNumber(1))
            .with_arg("hash", &H256::zero())
            .fetch_optional(&mut conn)
            .await
            .unwrap();
    }
}

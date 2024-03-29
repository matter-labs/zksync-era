use std::{fmt, panic::Location};

use sqlx::error::BoxDynError;

use crate::connection::ConnectionTags;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DalError {
    #[error(transparent)]
    Request(#[from] DalRequestError),
    #[error(transparent)]
    Transaction(#[from] DalTransactionError),
}

impl DalError {
    /// Returns a reference to the underlying `sqlx` error.
    pub fn inner(&self) -> &sqlx::Error {
        match self {
            Self::Request(err) => &err.inner,
            Self::Transaction(err) => &err.inner,
        }
    }

    /// Wraps this error into an `anyhow` wrapper.
    pub fn generalize(self) -> anyhow::Error {
        anyhow::Error::from(self).context("Postgres error")
    }
}

#[derive(Debug, thiserror::Error)]
pub struct DalRequestError {
    #[source]
    inner: sqlx::Error,
    method: &'static str,
    location: &'static Location<'static>,
    args: Vec<(&'static str, String)>,
    connection_tags: Option<ConnectionTags>,
}

pub type DalResult<T> = Result<T, DalError>;

impl DalRequestError {
    pub(crate) fn new(
        inner: sqlx::Error,
        method: &'static str,
        location: &'static Location<'static>,
    ) -> Self {
        Self {
            inner,
            method,
            location,
            args: vec![],
            connection_tags: None,
        }
    }

    pub(crate) fn with_args(mut self, args: Vec<(&'static str, String)>) -> Self {
        self.args = args;
        self
    }

    pub(crate) fn with_connection_tags(mut self, tags: Option<ConnectionTags>) -> Self {
        self.connection_tags = tags;
        self
    }
}

impl fmt::Display for DalRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        struct ArgsFormatter<'a>(&'a [(&'static str, String)]);

        impl fmt::Display for ArgsFormatter<'_> {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                for (i, (name, value)) in self.0.iter().enumerate() {
                    write!(formatter, "{name}={value}")?;
                    if i + 1 < self.0.len() {
                        formatter.write_str(", ")?;
                    }
                }
                Ok(())
            }
        }

        write!(
            formatter,
            "Query {name}({args}) called at {file}:{line} [{connection_tags}] failed: {err}",
            name = self.method,
            args = ArgsFormatter(&self.args),
            file = self.location.file(),
            line = self.location.line(),
            connection_tags = ConnectionTags::display(self.connection_tags.as_ref()),
            err = self.inner
        )
    }
}

#[derive(Debug, Clone, Copy)]
enum TransactionAction {
    Create,
    Commit,
}

impl TransactionAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::Create => "creating",
            Self::Commit => "committing",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub struct DalTransactionError {
    #[source]
    inner: sqlx::Error,
    action: TransactionAction,
    connection_tags: Option<ConnectionTags>,
}

impl fmt::Display for DalTransactionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Failed {action} DB transaction [{connection_tags}]: {err}",
            action = self.action.as_str(),
            connection_tags = ConnectionTags::display(self.connection_tags.as_ref()),
            err = self.inner
        )
    }
}

impl DalTransactionError {
    pub(crate) fn create(inner: sqlx::Error, connection_tags: Option<ConnectionTags>) -> Self {
        Self {
            inner,
            action: TransactionAction::Create,
            connection_tags,
        }
    }

    pub(crate) fn commit(inner: sqlx::Error, connection_tags: Option<ConnectionTags>) -> Self {
        Self {
            inner,
            action: TransactionAction::Commit,
            connection_tags,
        }
    }
}

/// Extension trait to create `sqlx::Result`s, similar to `anyhow::Context`.
pub trait SqlxContext<T> {
    /// Wraps the error variant of a result into a column conversion error.
    fn decode_column(self, column_name: &'static str) -> sqlx::Result<T>;
}

impl<T, E> SqlxContext<T> for Result<T, E>
where
    E: Into<BoxDynError>,
{
    fn decode_column(self, column_name: &'static str) -> sqlx::Result<T> {
        self.map_err(|err| sqlx::Error::ColumnDecode {
            index: column_name.to_string(),
            source: err.into(),
        })
    }
}

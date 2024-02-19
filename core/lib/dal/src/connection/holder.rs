use std::fmt;

use sqlx::{pool::PoolConnection, postgres::Postgres, Transaction};

/// Connection holder unifies the type of underlying connection, which
/// can be either pooled or direct.
pub(crate) enum ConnectionHolder<'a> {
    Pooled(PoolConnection<Postgres>),
    Transaction(Transaction<'a, Postgres>),
}

impl<'a> fmt::Debug for ConnectionHolder<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pooled(_) => write!(f, "Pooled connection"),
            Self::Transaction(_) => write!(f, "Database Transaction"),
        }
    }
}

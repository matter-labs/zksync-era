// Built-in deps
use std::fmt;
// External imports
use sqlx::pool::PoolConnection;
use sqlx::{postgres::Postgres, PgConnection, Transaction};
// Workspace imports
// Local imports
use crate::connection::test_pool::TestPoolLock;

/// Connection holder unifies the type of underlying connection, which
/// can be either pooled or direct.
pub enum ConnectionHolder<'a> {
    Pooled(PoolConnection<Postgres>),
    Direct(PgConnection),
    Transaction(Transaction<'a, Postgres>),
    TestTransaction(TestPoolLock),
}

impl<'a> fmt::Debug for ConnectionHolder<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pooled(_) => write!(f, "Pooled connection"),
            Self::Direct(_) => write!(f, "Direct connection"),
            Self::Transaction(_) => write!(f, "Database Transaction"),
            Self::TestTransaction(_) => write!(f, "Test Database Transaction"),
        }
    }
}

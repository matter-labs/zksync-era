// Built-in deps
use std::fmt;
// External imports
use sqlx::pool::PoolConnection;
use sqlx::{postgres::Postgres, Connection as _, PgConnection};
// Workspace imports
// Local imports
use crate::connection::test_pool::{TestConnection, TestConnectionRef, TestTransaction};

#[async_trait::async_trait]
pub trait Acquire: Send {
    async fn acquire(&mut self) -> ConnectionRef<'_>;
    async fn begin(&mut self) -> sqlx::Result<Transaction<'_>>;
}

/// Connection holder unifies the type of underlying connection, which
/// can be either pooled or direct.
pub enum Connection {
    Pooled(PoolConnection<Postgres>),
    Direct(PgConnection),
    Test(TestConnection),
}

pub enum Transaction<'a> {
    Real(sqlx::Transaction<'a, Postgres>),
    Test(TestTransaction),
}

#[async_trait::async_trait]
impl Acquire for Connection {
    async fn acquire(&mut self) -> ConnectionRef<'_> {
        match self {
            Self::Pooled(c) => ConnectionRef::Real(&mut *c),
            Self::Direct(c) => ConnectionRef::Real(&mut *c),
            Self::Test(c) => ConnectionRef::Test(c.acquire().await),
        }
    }

    async fn begin(&mut self) -> sqlx::Result<Transaction<'_>> {
        Ok(match self {
            Connection::Pooled(c) => Transaction::Real(c.begin().await?),
            Connection::Direct(c) => Transaction::Real(c.begin().await?),
            Connection::Test(c) => Transaction::Test(c.begin().await?),
        })
    }
}

#[async_trait::async_trait]
impl<'a> Acquire for Transaction<'a> {
    async fn acquire(&mut self) -> ConnectionRef<'_> {
        ConnectionRef::Real(match self {
            Self::Real(t) => &mut *t,
            Self::Test(t) => t.as_conn(),
        })
    }

    async fn begin(&mut self) -> sqlx::Result<Transaction<'_>> {
        Ok(Transaction::Real(match self {
            Transaction::Real(t) => t.begin().await?,
            Transaction::Test(t) => t.as_conn().begin().await?,
        }))
    }
}

impl<'a> Transaction<'a> {
    pub async fn commit(self) -> sqlx::Result<()> {
        match self {
            Self::Real(t) => t.commit().await,
            Self::Test(t) => t.commit().await,
        }
    }
}

pub enum ConnectionRef<'a> {
    Real(&'a mut PgConnection),
    Test(TestConnectionRef),
}

impl<'a> ConnectionRef<'a> {
    // TODO: Replace with DerefMut
    pub fn as_conn(&mut self) -> &mut PgConnection {
        match self {
            Self::Real(c) => c,
            Self::Test(c) => c.as_conn(),
        }
    }
}

impl fmt::Debug for Connection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pooled(_) => write!(f, "Pooled connection"),
            Self::Direct(_) => write!(f, "Direct connection"),
            Self::Test(_) => write!(f, "Test Database connection"),
        }
    }
}

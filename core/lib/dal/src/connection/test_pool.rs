//! Implementation of the test/fake connection pool to be used in tests.
//! This implementation works over an established transaction in order to reject
//! any changes made to the database, even if the tested component initiates and commits
//! its own transactions.
//!
//! # How it works
//!
//! Test pool uses an established transaction to be created. Reference to this transaction
//! will be used as a connection to create `StorageProcessor` objects in test.
//!
//! At the same time, using *reference* to the transaction in created `StorageProcessor`
//! objects is also necessary: upon `drop`, transaction gets discarded. It means that if we
//! use transaction and somewhere in test `StorageProcessor` is created, used without
//! transaction and then dropped (which is a normal use case for e.g. test setup) -- such
//! changes would be discarded and test will not execute correctly.

// Built-in deps
use std::sync::Arc;
// External imports
use sqlx::{Connection as _, PgConnection, Postgres};
use tokio::sync::{Mutex, OwnedMutexGuard};

/// Self-referential struct powering [`TestPool`].
// Ideally, we'd want to use a readily available crate like `ouroboros` to define this struct,
// but `ouroboros` in particular doesn't satisfy our needs:
//
// - It doesn't provide mutable access to the tail field (`subtransaction`), only allowing
//   to mutably access it in a closure.
// - There is an error borrowing from `transaction` since it implements `Drop`.
struct StaticTransaction {
    tx: sqlx::Transaction<'static, Postgres>,
    _base: Box<BaseConnection>,
}

enum BaseConnection {
    Root(PgConnection),
    Child(OwnedMutexGuard<StaticTransaction>),
}

impl BaseConnection {
    async fn begin(self: BaseConnection) -> sqlx::Result<StaticTransaction> {
        let mut base = Box::new(self);
        let tx = match &mut *base {
            BaseConnection::Root(conn) => conn.begin().await?,
            BaseConnection::Child(guard) => guard.tx.begin().await?,
        };
        Ok(StaticTransaction {
            tx: unsafe { std::mem::transmute(tx) },
            _base: base,
        })
    }
}

const PREFIX: &str = "test-";

pub async fn new_db() -> url::Url {
    use rand::Rng as _;
    use sqlx::Executor as _;
    let db_url = crate::get_test_database_url().unwrap();
    let mut db_url = url::Url::parse(&db_url).unwrap();
    let db_name = db_url.path()[1..].to_string();
    let db_copy_name = format!("{PREFIX}{}", rand::thread_rng().gen::<u64>());
    db_url.set_path("");
    let mut conn = sqlx::PgConnection::connect(&db_url.to_string())
        .await
        .unwrap();
    conn.execute(
        format!("CREATE DATABASE \"{db_copy_name}\" WITH TEMPLATE \"{db_name}\"").as_str(),
    )
    .await
    .unwrap();
    db_url.set_path(&db_copy_name);
    db_url
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Executor as _;

    #[tokio::test]
    async fn clean_old_dbs() {
        let db_url = crate::get_test_database_url().unwrap();
        let mut db_url = url::Url::parse(&db_url).unwrap();
        db_url.set_path("");
        let mut conn = sqlx::PgConnection::connect(&db_url.to_string())
            .await
            .unwrap();
        let rows = sqlx::query!("SELECT datname as \"datname!\" FROM pg_catalog.pg_database except SELECT datname FROM pg_stat_activity")
            .fetch_all(&mut conn).await.unwrap();
        for row in rows {
            if row.datname.starts_with(PREFIX) {
                conn.execute(format!("DROP DATABASE \"{}\"", row.datname).as_str())
                    .await
                    .unwrap();
            }
        }
    }
}

#[derive(Clone)]
pub struct TestConnection(Arc<Mutex<StaticTransaction>>);
pub struct TestTransaction(StaticTransaction);
pub struct TestConnectionRef(OwnedMutexGuard<StaticTransaction>);

impl TestConnectionRef {
    pub fn as_conn(&mut self) -> &mut PgConnection {
        &mut self.0.tx
    }
}

const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);

impl TestConnection {
    pub async fn acquire(&mut self) -> TestConnectionRef {
        TestConnectionRef(
            tokio::time::timeout(TIMEOUT, self.0.clone().lock_owned())
                .await
                .expect("TestConnection::acquire() timed out"),
        )
    }
}

impl TestTransaction {
    pub fn as_conn(&mut self) -> &mut PgConnection {
        &mut self.0.tx
    }

    pub async fn commit(self) -> sqlx::Result<()> {
        self.0.tx.commit().await
    }
}

impl TestConnection {
    pub async fn new() -> Self {
        let database_url = crate::get_test_database_url().unwrap();
        let conn = sqlx::PgConnection::connect(&database_url).await.unwrap();
        let conn = BaseConnection::Root(conn).begin().await.unwrap();
        Self(Arc::new(Mutex::new(conn)))
    }

    pub async fn begin(&mut self) -> sqlx::Result<TestTransaction> {
        let conn = BaseConnection::Child(
            tokio::time::timeout(TIMEOUT, self.0.clone().lock_owned())
                .await
                .expect("TestConnection::begin() timed out"),
        );
        Ok(TestTransaction(conn.begin().await?))
    }
}

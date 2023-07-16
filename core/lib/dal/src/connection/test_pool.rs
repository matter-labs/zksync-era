// Built-in deps
use std::{fmt, mem, pin::Pin, sync::Arc, time::Duration};
// External imports
use sqlx::{Acquire, Connection, PgConnection, Postgres, Transaction};
use tokio::{
    sync::{Mutex, OwnedMutexGuard},
    time::timeout,
};
// Local imports
use crate::StorageProcessor;

/// Self-referential struct powering [`TestPool`].
// Ideally, we'd want to use a readily available crate like `ouroboros` to define this struct,
// but `ouroboros` in particular doesn't satisfy our needs:
//
// - It doesn't provide mutable access to the tail field (`subtransaction`), only allowing
//   to mutably access it in a closure.
// - There is an error borrowing from `transaction` since it implements `Drop`.
struct TestPoolInner {
    // Mutably references `_transaction`.
    subtransaction: Transaction<'static, Postgres>,
    // Mutably references `_connection`. Must not be used anywhere since it's mutably borrowed!
    _transaction: Pin<Box<Transaction<'static, Postgres>>>,
    // Must not be used anywhere since it's mutably borrowed!
    _connection: Pin<Box<PgConnection>>,
}

impl fmt::Debug for TestPoolInner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Inner")
            .field("subtransaction", &self.subtransaction)
            .finish()
    }
}

impl TestPoolInner {
    async fn new() -> Self {
        let database_url = crate::get_test_database_url();
        let connection = PgConnection::connect(&database_url).await.unwrap();
        let mut connection = Box::pin(connection);

        let transaction = Connection::begin(&mut *connection).await.unwrap();
        let transaction: Transaction<'static, Postgres> = unsafe {
            // SAFETY: We extend `transaction` lifetime to `'static`. This is valid
            // because the transaction borrows from `connection`, which outlives `transaction`
            // (since it's a field in the same struct, and fields in a struct are dropped
            // in the declaration order), is not moved after the borrow
            // (due to being wrapped in a `Pin<Box<_>>`), and is not accessed afterwards.
            mem::transmute(transaction)
        };
        let mut transaction = Box::pin(transaction);

        let subtransaction = transaction.begin().await.unwrap();
        let subtransaction: Transaction<'static, Postgres> = unsafe {
            // SAFETY: We extend `subtransaction` lifetime to `'static`. This is valid
            // for the same reasons that apply for `transaction`.
            mem::transmute(subtransaction)
        };

        Self {
            subtransaction,
            _transaction: transaction,
            _connection: connection,
        }
    }
}

#[derive(Debug)]
pub struct TestPoolLock {
    lock: OwnedMutexGuard<TestPoolInner>,
}

impl TestPoolLock {
    pub fn as_connection(&mut self) -> &mut PgConnection {
        &mut self.lock.subtransaction
    }
}

/// Implementation of the test/fake connection pool to be used in tests.
/// This implementation works over an established transaction in order to reject
/// any changes made to the database, even if the tested component initiates and commits
/// its own transactions.
///
/// # How it works
///
/// Test pool uses an established transaction to be created. This transaction, in its turn,
/// is used to establish a *subtransaction*. Reference to this subtransaction will be used
/// as a connection to create `StorageProcessor` objects in test.
///
/// Having a subtransaction is necessary: even if some component will (mistakenly) will not
/// initiate a transaction and will call `commit` on `StorageProcessor`, changes won't be
/// persisted, since top-level transaction will be dropped.
///
/// At the same time, using *reference* to the subtransaction in created `StorageProcessor`
/// objects is also necessary: upon `drop`, transaction gets discarded. It means that if we
/// use transaction and somewhere in test `StorageProcessor` is created, used without
/// transaction and then dropped (which is a normal use case for e.g. test setup) -- such
/// changes would be discarded and test will not execute correctly.
#[derive(Debug, Clone)]
pub struct TestPool {
    inner: Arc<Mutex<TestPoolInner>>,
}

impl TestPool {
    /// Constructs a new object using an already established transaction to the database.
    /// This method is unsafe, since internally it extends lifetime of the provided `Transaction`.
    pub async fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(TestPoolInner::new().await)),
        }
    }

    pub async fn access_storage(&self) -> StorageProcessor<'static> {
        const LOCK_TIMEOUT: Duration = Duration::from_secs(1);

        let lock = self.inner.clone().lock_owned();
        let lock = timeout(LOCK_TIMEOUT, lock).await.expect(
            "Timed out waiting to acquire a lock in test `ConnectionPool`. \
             Check the backtrace and make sure that no `StorageProcessor`s are alive",
        );
        StorageProcessor::from_test_transaction(TestPoolLock { lock })
    }
}

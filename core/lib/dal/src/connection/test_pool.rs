// Built-in deps
use std::sync::Arc;
// External imports
use async_std::sync::Mutex;
use sqlx::{PgConnection, Postgres, Transaction};

// Public re-export for proc macro to use `begin` on the connection.
#[doc(hidden)]
pub use sqlx::Connection;

use crate::StorageProcessor;
// Local imports

/// Implementation of the test/fake connection pool to be used in tests.
/// This implementation works over an established transaction in order to reject
/// any changes made to the database, even if the tested component initiates and commits
/// its own transactions.
///
/// ## How does it work
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
///
/// ## Safety
///
/// Test pool relies on unsafe code to work, so it comes with several invariants to be
/// upheld by its user. They are *not* enforced by compiler and breaking *any* of them
/// will result in undefined behavior.
///
/// Usage invariants:
/// - This object should never outlive the transaction used to create it. If, for example,
///   test pool is created and passed to another thread and the thread with the original
///   connection panics (or connection is simply dropped), the behavior is undefined.
/// - Concurrent access to the pool is forbidden. `TestPool` has to be `Sync` in order to
///   not break the interface of the `ConnectionPool`, but since it operates over a single
///   established transaction, it can't be safely accessed from multiple threads.
///   Moving the object to another thread is safe though.
/// - Since we use mutable reference to the subtransaction to create `StorageProcessor`, you
///   should not create and use multiple `StorageProcessor` objects in the same scope.
///
/// This object is meant to be used in unit tests only, any attempt to use it with the real
/// database is on the conscience of the user. I have warned you.
#[derive(Debug, Clone)]
pub struct TestPool {
    // Sub-transaction to be used to instantiate connections.
    //
    // `Arc` is required to keep the pool `Clone` and `Send` and also to pin the transaction
    // location in the memory.
    // `Mutex` is required to keep the object `Sync` and provide mutable access to the transaction
    // from the immutable `access_storage` method.
    subtransaction: Arc<Mutex<Transaction<'static, Postgres>>>,
}

impl TestPool {
    /// Establishes a Postgres connection to the test database.
    pub async fn connect_to_test_db() -> PgConnection {
        let database_url = crate::get_test_database_url();
        PgConnection::connect(&database_url).await.unwrap()
    }

    /// Constructs a new object using an already established transaction to the database.
    /// This method is unsafe, since internally it extends lifetime of the provided `Transaction`.
    ///
    /// ## Safety
    ///
    /// When calling this method, caller must guarantee that resulting object will not live longer
    /// than the transaction to the database used to create this object.
    pub async unsafe fn new(transaction: &mut Transaction<'_, Postgres>) -> Self {
        // Using `std::mem::transmute` to extend the lifetime of an object is an unsafe but
        // valid way to use this method.
        let subtransaction: Transaction<'static, Postgres> =
            std::mem::transmute(transaction.begin().await.unwrap());
        Self {
            subtransaction: Arc::new(Mutex::new(subtransaction)),
        }
    }

    pub async fn access_storage(&self) -> StorageProcessor<'static> {
        let mut lock = self.subtransaction.lock().await;
        let subtransaction = &mut *lock;

        // ## Safety
        //
        // Guarantees held by this method:
        // - memory location: original `transaction` object is behind the smart pointer, so its location don't change.
        //
        // Guarantees held by the caller:
        // - cross-thread access: accessing `TestPool` concurrently is forbidden by the contract of the object.
        // - having multiple `StorageProcessor` objects is forbidden by the contract of the object.
        // - lifetime: we are transmuting lifetime to the static lifetime, so the transaction should never live longer
        //   than the test pool object.
        let subtransaction_ref: &'static mut Transaction<Postgres> =
            unsafe { std::mem::transmute(subtransaction) };

        StorageProcessor::from_test_transaction(subtransaction_ref)
    }
}

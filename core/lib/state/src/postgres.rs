use tokio::{runtime::Handle, sync::mpsc, time::Instant};

use std::{
    mem,
    sync::{Arc, RwLock},
};

use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_types::{L1BatchNumber, MiniblockNumber, StorageKey, StorageValue, H256};

use crate::{
    cache::{Cache, CacheValue},
    ReadStorage,
};

/// Type alias for smart contract source code cache.
type FactoryDepsCache = Cache<H256, Vec<u8>>;

impl CacheValue<H256> for Vec<u8> {
    fn cache_weight(&self) -> u32 {
        self.len().try_into().expect("Cached bytes are too large")
    }
}

/// Type alias for initial writes caches.
type InitialWritesCache = Cache<StorageKey, L1BatchNumber>;

impl CacheValue<StorageKey> for L1BatchNumber {
    #[allow(clippy::cast_possible_truncation)] // doesn't happen in practice
    fn cache_weight(&self) -> u32 {
        const WEIGHT: usize = mem::size_of::<L1BatchNumber>() + mem::size_of::<StorageKey>();
        // ^ Since values are small in size, we want to account for key sizes as well

        WEIGHT as u32
    }
}

impl CacheValue<H256> for StorageValue {
    #[allow(clippy::cast_possible_truncation)] // doesn't happen in practice
    fn cache_weight(&self) -> u32 {
        const WEIGHT: usize = mem::size_of::<StorageValue>() + mem::size_of::<H256>();
        // ^ Since values are small in size, we want to account for key sizes as well

        WEIGHT as u32
    }
}

#[derive(Debug)]
struct ValuesCacheInner {
    /// Miniblock for which `self.values` are valid. Has the same meaning as `miniblock_number`
    /// in `PostgresStorage` (i.e., the latest sealed miniblock for which storage logs should
    /// be taken into account).
    valid_for: MiniblockNumber,
    values: Cache<H256, StorageValue>,
}

/// Cache for the VM storage. Only caches values for a single VM storage snapshot, which logically
/// corresponds to the latest sealed miniblock in Postgres.
///
/// The cached snapshot can be updated, which will load changed storage keys from Postgres and remove
/// the (potentially stale) cached values for these keys.
///
/// # Why wrap the cache in `RwLock`?
///
/// We need to be sure that `valid_for` miniblock of the values cache has not changed while we are
/// loading or storing values in it. This is easiest to achieve using an `RwLock`. Note that
/// almost all cache ops require only shared access to the lock (including cache updates!); we only
/// need exclusive access when we are updating the `valid_for` miniblock. Further, the update itself
/// doesn't grab the lock until *after* the Postgres data has been loaded. (This works because we
/// know statically that there is a single thread updating the cache; hence, we have no contention
/// over updating the cache.) To summarize, `RwLock` should see barely any contention.
#[derive(Debug, Clone)]
struct ValuesCache(Arc<RwLock<ValuesCacheInner>>);

impl ValuesCache {
    fn new(capacity: u64) -> Self {
        let inner = ValuesCacheInner {
            valid_for: MiniblockNumber(0),
            values: Cache::new("values_cache", capacity),
        };
        Self(Arc::new(RwLock::new(inner)))
    }

    /// *NB.* The returned value should be considered immediately stale; at best, it can be
    /// the lower boundary on the current `valid_for` value.
    fn valid_for(&self) -> MiniblockNumber {
        self.0.read().expect("values cache is poisoned").valid_for
    }

    /// Gets the cached value for `key` provided that the cache currently holds values
    /// for `miniblock_number`.
    fn get(&self, miniblock_number: MiniblockNumber, key: &StorageKey) -> Option<StorageValue> {
        let lock = self.0.read().expect("values cache is poisoned");
        if lock.valid_for == miniblock_number {
            lock.values.get(&key.hashed_key())
        } else {
            metrics::increment_counter!("server.state_cache.stale_values", "method" => "get");
            None
        }
    }

    /// Caches `value` for `key`, but only if the cache currently holds values for `miniblock_number`.
    fn insert(&self, miniblock_number: MiniblockNumber, key: StorageKey, value: StorageValue) {
        let lock = self.0.read().expect("values cache is poisoned");
        if lock.valid_for == miniblock_number {
            lock.values.insert(key.hashed_key(), value);
        } else {
            metrics::increment_counter!("server.state_cache.stale_values", "method" => "insert");
        }
    }

    #[allow(clippy::cast_precision_loss)] // acceptable for metrics
    fn update(
        &self,
        from_miniblock: MiniblockNumber,
        to_miniblock: MiniblockNumber,
        rt_handle: &Handle,
        connection: &mut StorageProcessor<'_>,
    ) {
        const MAX_MINIBLOCKS_LAG: u32 = 5;

        vlog::debug!(
            "Updating storage values cache from miniblock {from_miniblock} to {to_miniblock}"
        );

        if to_miniblock.0 - from_miniblock.0 > MAX_MINIBLOCKS_LAG {
            // We can spend too much time loading data from Postgres, so we opt for an easier "update" route:
            // evict *everything* from cache and call it a day. This should not happen too often in practice.
            vlog::info!(
                "Storage values cache is too far behind (current miniblock is {from_miniblock}; \
                 requested update to {to_miniblock}); resetting the cache"
            );
            let mut lock = self.0.write().expect("values cache is poisoned");
            assert_eq!(lock.valid_for, from_miniblock);
            lock.valid_for = to_miniblock;
            lock.values.clear();

            metrics::increment_counter!("server.state_cache.values_emptied");
        } else {
            let stage_started_at = Instant::now();
            let miniblocks = (from_miniblock + 1)..=to_miniblock;
            let modified_keys = rt_handle.block_on(
                connection
                    .storage_web3_dal()
                    .modified_keys_in_miniblocks(miniblocks.clone()),
            );

            let elapsed = stage_started_at.elapsed();
            metrics::histogram!(
                "server.state_cache.values_update",
                elapsed,
                "stage" => "load_keys"
            );
            metrics::histogram!(
                "server.state_cache.values_update.modified_keys",
                modified_keys.len() as f64
            );
            vlog::debug!(
                "Loaded {modified_keys_len} modified storage keys from miniblocks {miniblocks:?}; \
                 took {elapsed:?}",
                modified_keys_len = modified_keys.len()
            );

            let stage_started_at = Instant::now();
            let mut lock = self.0.write().expect("values cache is poisoned");
            // The code below holding onto the write `lock` is the only code that can theoretically poison the `RwLock`
            // (other than emptying the cache above). Thus, it's kept as simple and tight as possible.
            // E.g., we load data from Postgres beforehand.
            assert_eq!(lock.valid_for, from_miniblock);
            lock.valid_for = to_miniblock;
            for modified_key in &modified_keys {
                lock.values.remove(modified_key);
            }
            lock.values.report_size();
            drop(lock);

            metrics::histogram!(
                "server.state_cache.values_update",
                stage_started_at.elapsed(),
                "stage" => "remove_stale_keys"
            );
        }
        metrics::gauge!(
            "server.state_cache.values_valid_for_miniblock",
            f64::from(to_miniblock.0)
        );
    }
}

#[derive(Debug, Clone)]
struct ValuesCacheAndUpdater {
    cache: ValuesCache,
    command_sender: mpsc::UnboundedSender<MiniblockNumber>,
}

/// Caches used during VM execution.
///
/// Currently, this struct includes the following caches:
///
/// - Cache for smart contract bytecodes (never invalidated, since it is content-addressable)
/// - Cache for L1 batch numbers of initial writes for storage keys (never invalidated, except after
///   reverting L1 batch execution)
/// - Cache of the VM storage snapshot corresponding to the latest sealed miniblock
#[derive(Debug, Clone)]
pub struct PostgresStorageCaches {
    factory_deps: FactoryDepsCache,
    initial_writes: InitialWritesCache,
    // Besides L1 batch numbers for initial writes, we also cache information that a certain key
    // was not written to before the certain L1 batch (i.e., this lower boundary is the cached value).
    //
    // This is caused by the observation that a significant part of `is_write_initial()` queries returns `true`
    // (i.e., the corresponding key was not written to).
    // If we don't cache this information, we'll query Postgres multiple times for the same key even if we know
    // it wasn't written to at the point that interests us.
    negative_initial_writes: InitialWritesCache,
    values: Option<ValuesCacheAndUpdater>,
}

impl PostgresStorageCaches {
    const NEG_INITIAL_WRITES_NAME: &'static str = "negative_initial_writes_cache";

    /// Creates caches with the specified capacities measured in bytes.
    pub fn new(factory_deps_capacity: u64, initial_writes_capacity: u64) -> Self {
        vlog::debug!(
            "Initialized VM execution cache with {factory_deps_capacity}B capacity for factory deps, \
             {initial_writes_capacity}B capacity for initial writes"
        );

        Self {
            factory_deps: FactoryDepsCache::new("factory_deps_cache", factory_deps_capacity),
            initial_writes: InitialWritesCache::new(
                "initial_writes_cache",
                initial_writes_capacity / 2,
            ),
            negative_initial_writes: InitialWritesCache::new(
                Self::NEG_INITIAL_WRITES_NAME,
                initial_writes_capacity / 2,
            ),
            values: None,
        }
    }

    /// Configures the VM storage values cache. The returned closure is the background task that will update
    /// the cache according to [`Self::schedule_values_update()`] calls. It should be spawned on a separate thread
    /// or a blocking Tokio task.
    ///
    /// # Panics
    ///
    /// Panics if provided `capacity` is zero. (Check on the caller side beforehand if there is
    /// such possibility.)
    pub fn configure_storage_values_cache(
        &mut self,
        capacity: u64,
        connection_pool: ConnectionPool,
        rt_handle: Handle,
    ) -> impl FnOnce() + Send {
        assert!(
            capacity > 0,
            "Storage values cache capacity must be positive"
        );
        vlog::debug!("Initializing VM storage values cache with {capacity}B capacity");

        let (command_sender, mut command_receiver) = mpsc::unbounded_channel();
        let values_cache = ValuesCache::new(capacity);
        self.values = Some(ValuesCacheAndUpdater {
            cache: values_cache.clone(),
            command_sender,
        });

        // We want to run updates on a single thread in order to not block VM execution on update
        // and keep contention over the `ValuesCache` lock as low as possible. As a downside,
        // `Self::schedule_values_update()` will produce some no-op update commands from concurrently
        // executing VM instances. Due to built-in filtering, this seems manageable.
        move || {
            let mut current_miniblock = values_cache.valid_for();
            while let Some(to_miniblock) = command_receiver.blocking_recv() {
                if to_miniblock <= current_miniblock {
                    continue;
                }
                let mut connection = rt_handle
                    .block_on(connection_pool.access_storage_tagged("values_cache_updater"));
                values_cache.update(current_miniblock, to_miniblock, &rt_handle, &mut connection);
                current_miniblock = to_miniblock;
            }
        }
    }

    /// Schedules an update of the VM storage values cache to the specified miniblock.
    ///
    /// # Panics
    ///
    /// - Panics if the cache wasn't previously configured using [`Self::configure_storage_values_cache()`].
    /// - Panics if the cache update task returned from `configure_storage_values_cache()` has panicked.
    pub fn schedule_values_update(&self, to_miniblock: MiniblockNumber) {
        let values = self
            .values
            .as_ref()
            .expect("`schedule_update()` called without configuring values cache");

        if values.cache.valid_for() < to_miniblock {
            // Filter out no-op updates right away in order to not store lots of them in RAM.
            values
                .command_sender
                .send(to_miniblock)
                .expect("values cache update task failed");
        }
    }
}

/// [`ReadStorage`] implementation backed by the Postgres database.
#[derive(Debug)]
pub struct PostgresStorage<'a> {
    rt_handle: Handle,
    connection: StorageProcessor<'a>,
    miniblock_number: MiniblockNumber,
    l1_batch_number_for_miniblock: L1BatchNumber,
    pending_l1_batch_number: L1BatchNumber,
    consider_new_l1_batch: bool,
    caches: Option<PostgresStorageCaches>,
}

impl<'a> PostgresStorage<'a> {
    /// Creates a new storage using the specified connection.
    pub fn new(
        rt_handle: Handle,
        mut connection: StorageProcessor<'a>,
        block_number: MiniblockNumber,
        consider_new_l1_batch: bool,
    ) -> PostgresStorage<'a> {
        let resolved = rt_handle
            .block_on(
                connection
                    .storage_web3_dal()
                    .resolve_l1_batch_number_of_miniblock(block_number),
            )
            .expect("Failed resolving L1 batch number for miniblock");

        Self {
            rt_handle,
            connection,
            miniblock_number: block_number,
            l1_batch_number_for_miniblock: resolved.expected_l1_batch(),
            pending_l1_batch_number: resolved.pending_l1_batch,
            consider_new_l1_batch,
            caches: None,
        }
    }

    /// Sets the caches to use with the storage.
    #[must_use]
    pub fn with_caches(self, mut caches: PostgresStorageCaches) -> Self {
        let should_use_values_cache = caches.values.as_ref().map_or(false, |values| {
            self.miniblock_number >= values.cache.valid_for()
        });
        // Since "valid for" only increases with time, if `self.miniblock_number < valid_for`,
        // all cache calls are guaranteed to miss.

        metrics::increment_counter!(
            "server.state_cache.values_used",
            "used" => if should_use_values_cache { "true" } else { "false" }
        );
        if !should_use_values_cache {
            caches.values = None;
        }

        Self {
            caches: Some(caches),
            ..self
        }
    }

    /// This method is expected to be called for each write that was found in the database, and it decides
    /// whether the change is initial or not. Even if a change is present in the DB, in some cases we would not consider it.
    /// For example, in API we always represent the state at the beginning of an L1 batch, so we discard all the writes
    /// that happened at the same batch or later (for historical `eth_call` requests).
    fn write_counts(&self, write_l1_batch_number: L1BatchNumber) -> bool {
        if self.consider_new_l1_batch {
            self.l1_batch_number_for_miniblock >= write_l1_batch_number
        } else {
            self.l1_batch_number_for_miniblock > write_l1_batch_number
        }
    }

    fn values_cache(&self) -> Option<&ValuesCache> {
        Some(&self.caches.as_ref()?.values.as_ref()?.cache)
    }
}

impl ReadStorage for PostgresStorage<'_> {
    fn read_value(&mut self, &key: &StorageKey) -> StorageValue {
        let started_at = Instant::now();
        let values_cache = self.values_cache();
        let cached_value = values_cache.and_then(|cache| cache.get(self.miniblock_number, &key));

        let value = cached_value.unwrap_or_else(|| {
            let mut dal = self.connection.storage_web3_dal();
            let value = self
                .rt_handle
                .block_on(dal.get_historical_value_unchecked(&key, self.miniblock_number))
                .expect("Failed executing `read_value`");
            if let Some(cache) = self.values_cache() {
                cache.insert(self.miniblock_number, key, value);
            }
            value
        });

        metrics::histogram!("state.postgres_storage", started_at.elapsed(), "method" => "read_value");
        value
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        let started_at = Instant::now();
        let caches = self.caches.as_ref();
        let cached_value = caches.and_then(|caches| caches.initial_writes.get(key));

        if cached_value.is_none() {
            // Write is absent in positive cache, check whether it's present in the negative cache.
            let cached_value = caches.and_then(|caches| caches.negative_initial_writes.get(key));
            if let Some(min_l1_batch_for_initial_write) = cached_value {
                // We know that this slot was certainly not touched before `min_l1_batch_for_initial_write`.
                // Try to use this knowledge to decide if the change is certainly initial.
                // This is based on the hypothetical worst-case scenario, in which the key was
                // written to at the earliest possible L1 batch (i.e., `min_l1_batch_for_initial_write`).
                if !self.write_counts(min_l1_batch_for_initial_write) {
                    metrics::increment_counter!(
                        "server.state_cache.effective_values",
                        "name" => PostgresStorageCaches::NEG_INITIAL_WRITES_NAME
                    );
                    return true;
                }
            }
        }

        let l1_batch_number = cached_value.or_else(|| {
            let mut dal = self.connection.storage_web3_dal();
            let value = self
                .rt_handle
                .block_on(dal.get_l1_batch_number_for_initial_write(key))
                .expect("Failed executing `is_write_initial`");

            if let Some(caches) = &self.caches {
                if let Some(l1_batch_number) = value {
                    caches.negative_initial_writes.remove(key);
                    caches.initial_writes.insert(*key, l1_batch_number);
                } else {
                    caches
                        .negative_initial_writes
                        .insert(*key, self.pending_l1_batch_number);
                    // The pending L1 batch might have been sealed since its number was requested from Postgres
                    // in `Self::new()`, so this is a somewhat conservative estimate.
                }
            }
            value
        });
        metrics::histogram!("state.postgres_storage", started_at.elapsed(), "method" => "is_write_initial");

        let contains_key = l1_batch_number.map_or(false, |initial_write_l1_batch_number| {
            self.write_counts(initial_write_l1_batch_number)
        });
        !contains_key
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        let started_at = Instant::now();
        let cached_value = self
            .caches
            .as_ref()
            .and_then(|caches| caches.factory_deps.get(&hash));

        let result = cached_value.or_else(|| {
            let mut dal = self.connection.storage_web3_dal();
            let value = self
                .rt_handle
                .block_on(dal.get_factory_dep_unchecked(hash, self.miniblock_number))
                .expect("Failed executing `load_factory_dep`");

            if let Some(caches) = &self.caches {
                // If we receive None, we won't cache it.
                if let Some(dep) = value.clone() {
                    caches.factory_deps.insert(hash, dep);
                }
            };

            value
        });

        metrics::histogram!(
            "state.postgres_storage",
            started_at.elapsed(),
            "method" => "load_factory_dep",
        );
        result
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, mem};

    use db_test_macro::db_test;
    use zksync_dal::ConnectionPool;
    use zksync_types::StorageLog;

    use super::*;
    use crate::test_utils::{
        create_l1_batch, create_miniblock, gen_storage_logs, prepare_postgres,
    };

    fn test_postgres_storage_basics(
        pool: &ConnectionPool,
        rt_handle: Handle,
        cache_initial_writes: bool,
    ) {
        let mut connection = rt_handle.block_on(pool.access_storage());
        rt_handle.block_on(prepare_postgres(&mut connection));
        let mut storage = PostgresStorage::new(rt_handle, connection, MiniblockNumber(0), true);
        if cache_initial_writes {
            let caches = PostgresStorageCaches::new(1_024, 1_024);
            storage = storage.with_caches(caches);
        }
        assert_eq!(storage.l1_batch_number_for_miniblock, L1BatchNumber(0));

        let existing_logs = gen_storage_logs(0..20);
        for log in &existing_logs {
            assert!(!storage.is_write_initial(&log.key));
        }

        let non_existing_logs = gen_storage_logs(20..30);
        for log in &non_existing_logs {
            assert!(storage.is_write_initial(&log.key));
        }

        if cache_initial_writes {
            let caches = storage.caches.as_ref().unwrap();
            assert!(caches.initial_writes.estimated_len() > 0);
        }

        // Add a new miniblock to the storage
        storage.rt_handle.block_on(create_miniblock(
            &mut storage.connection,
            MiniblockNumber(1),
            non_existing_logs.clone(),
        ));

        // Check that the miniblock is not seen by `PostgresStorage` (it's not a part of an L1 batch)
        for log in &non_existing_logs {
            assert!(storage.is_write_initial(&log.key));
        }

        let caches = mem::take(&mut storage.caches);
        let mut storage = PostgresStorage::new(
            storage.rt_handle,
            storage.connection,
            MiniblockNumber(1),
            true,
        );
        storage.caches = caches;

        assert_eq!(storage.l1_batch_number_for_miniblock, L1BatchNumber(1));
        for log in &non_existing_logs {
            assert!(storage.is_write_initial(&log.key));
        }

        // Create an L1 batch for miniblock #1
        storage.rt_handle.block_on(create_l1_batch(
            &mut storage.connection,
            L1BatchNumber(1),
            &non_existing_logs,
        ));

        // Miniblock #1 should not be seen by the "old" storage
        let caches = mem::take(&mut storage.caches);
        let mut storage = PostgresStorage::new(
            storage.rt_handle,
            storage.connection,
            MiniblockNumber(0),
            true,
        );
        storage.caches = caches;

        assert_eq!(storage.l1_batch_number_for_miniblock, L1BatchNumber(0));
        for log in &non_existing_logs {
            assert!(storage.is_write_initial(&log.key));
        }

        // ...but should be seen by the new one
        let caches = mem::take(&mut storage.caches);
        let mut storage = PostgresStorage::new(
            storage.rt_handle,
            storage.connection,
            MiniblockNumber(1),
            true,
        );
        storage.caches = caches;

        assert_eq!(storage.l1_batch_number_for_miniblock, L1BatchNumber(1));
        for log in &non_existing_logs {
            assert!(!storage.is_write_initial(&log.key));
        }

        // ...except if we set `consider_new_l1_batch` to `false`
        let caches = mem::take(&mut storage.caches);
        let mut storage = PostgresStorage::new(
            storage.rt_handle,
            storage.connection,
            MiniblockNumber(1),
            false,
        );
        storage.caches = caches;

        assert_eq!(storage.l1_batch_number_for_miniblock, L1BatchNumber(1));
        for log in &non_existing_logs {
            assert!(storage.is_write_initial(&log.key));
        }
        for log in &existing_logs {
            assert!(!storage.is_write_initial(&log.key));
        }
    }

    #[db_test]
    async fn postgres_storage_basics(pool: ConnectionPool) {
        tokio::task::spawn_blocking(move || {
            test_postgres_storage_basics(&pool, Handle::current(), false);
        })
        .await
        .unwrap();
    }

    #[db_test]
    async fn postgres_storage_with_initial_writes_cache(pool: ConnectionPool) {
        tokio::task::spawn_blocking(move || {
            test_postgres_storage_basics(&pool, Handle::current(), true);
        })
        .await
        .unwrap();
    }

    fn test_postgres_storage_after_sealing_miniblock(
        pool: &ConnectionPool,
        rt_handle: Handle,
        consider_new_l1_batch: bool,
    ) {
        let mut connection = rt_handle.block_on(pool.access_storage());
        rt_handle.block_on(prepare_postgres(&mut connection));
        let new_logs = gen_storage_logs(20..30);

        rt_handle.block_on(create_miniblock(
            &mut connection,
            MiniblockNumber(1),
            new_logs.clone(),
        ));

        let mut storage = PostgresStorage::new(
            rt_handle,
            connection,
            MiniblockNumber(1),
            consider_new_l1_batch,
        );
        assert_eq!(storage.l1_batch_number_for_miniblock, L1BatchNumber(1));

        storage.rt_handle.block_on(create_l1_batch(
            &mut storage.connection,
            L1BatchNumber(1),
            &new_logs,
        ));

        for log in &new_logs {
            assert_eq!(storage.is_write_initial(&log.key), !consider_new_l1_batch);
        }

        // Cross-check with the newly instantiated store with the same params
        let mut storage = PostgresStorage::new(
            storage.rt_handle,
            storage.connection,
            MiniblockNumber(1),
            consider_new_l1_batch,
        );
        assert_eq!(storage.l1_batch_number_for_miniblock, L1BatchNumber(1));
        for log in &new_logs {
            assert_eq!(storage.is_write_initial(&log.key), !consider_new_l1_batch);
        }
    }

    #[db_test]
    async fn postgres_storage_after_sealing_miniblock(pool: ConnectionPool) {
        tokio::task::spawn_blocking(move || {
            println!("Considering new L1 batch");
            test_postgres_storage_after_sealing_miniblock(&pool, Handle::current(), true);
            println!("Not considering new L1 batch");
            test_postgres_storage_after_sealing_miniblock(&pool, Handle::current(), false);
        })
        .await
        .unwrap();
    }

    fn test_factory_deps_cache(pool: &ConnectionPool, rt_handle: Handle) {
        let mut connection = rt_handle.block_on(pool.access_storage());
        rt_handle.block_on(prepare_postgres(&mut connection));

        let caches = PostgresStorageCaches::new(128 * 1_024 * 1_024, 1_024);
        let mut storage = PostgresStorage::new(rt_handle, connection, MiniblockNumber(1), true)
            .with_caches(caches.clone());

        let zero_addr = H256::zero();
        // try load a non-existent contract
        let dep = storage.load_factory_dep(zero_addr);

        assert_eq!(dep, None);
        assert_eq!(caches.factory_deps.get(&zero_addr), None);

        // insert the contracts
        let mut contracts = HashMap::new();
        contracts.insert(H256::zero(), vec![1, 2, 3]);
        storage.rt_handle.block_on(
            storage
                .connection
                .storage_dal()
                .insert_factory_deps(MiniblockNumber(0), &contracts),
        );

        // Create the storage that should have the cache filled.
        let mut storage = PostgresStorage::new(
            storage.rt_handle,
            storage.connection,
            MiniblockNumber(1),
            true,
        )
        .with_caches(caches.clone());

        // Fill the cache
        let dep = storage.load_factory_dep(zero_addr);
        assert_eq!(dep, Some(vec![1, 2, 3]));
        assert_eq!(caches.factory_deps.get(&zero_addr), Some(vec![1, 2, 3]));
    }

    #[db_test]
    async fn using_factory_deps_cache(pool: ConnectionPool) {
        let handle = Handle::current();
        tokio::task::spawn_blocking(move || test_factory_deps_cache(&pool, handle))
            .await
            .unwrap();
    }

    fn test_initial_writes_cache(pool: &ConnectionPool, rt_handle: Handle) {
        let connection = rt_handle.block_on(pool.access_storage());
        let caches = PostgresStorageCaches::new(1_024, 4 * 1_024 * 1_024);
        let mut storage = PostgresStorage::new(rt_handle, connection, MiniblockNumber(0), false)
            .with_caches(caches.clone());
        assert_eq!(storage.pending_l1_batch_number, L1BatchNumber(0));

        storage
            .rt_handle
            .block_on(prepare_postgres(&mut storage.connection));

        let mut logs = gen_storage_logs(100..120);
        let non_existing_key = logs[19].key;
        logs.truncate(10);

        assert!(storage.is_write_initial(&logs[0].key));
        assert!(storage.is_write_initial(&non_existing_key));
        assert_eq!(
            caches.negative_initial_writes.get(&logs[0].key),
            Some(L1BatchNumber(0))
        );
        assert_eq!(
            caches.negative_initial_writes.get(&non_existing_key),
            Some(L1BatchNumber(0))
        );
        assert!(storage.is_write_initial(&logs[0].key));
        assert!(storage.is_write_initial(&non_existing_key));

        storage.rt_handle.block_on(create_miniblock(
            &mut storage.connection,
            MiniblockNumber(1),
            logs.clone(),
        ));
        storage.rt_handle.block_on(create_l1_batch(
            &mut storage.connection,
            L1BatchNumber(1),
            &logs,
        ));

        let mut storage = PostgresStorage::new(
            storage.rt_handle,
            storage.connection,
            MiniblockNumber(1),
            false,
        )
        .with_caches(caches.clone());

        assert!(storage.is_write_initial(&logs[0].key));
        // ^ Since we don't consider the latest L1 batch
        assert!(storage.is_write_initial(&non_existing_key));

        // Check that the cache entries have been updated
        assert_eq!(
            caches.initial_writes.get(&logs[0].key),
            Some(L1BatchNumber(1))
        );
        assert_eq!(caches.negative_initial_writes.get(&logs[0].key), None);
        assert_eq!(
            caches.negative_initial_writes.get(&non_existing_key),
            Some(L1BatchNumber(2))
        );
        assert!(storage.is_write_initial(&logs[0].key));
        assert!(storage.is_write_initial(&non_existing_key));

        let mut storage = PostgresStorage::new(
            storage.rt_handle,
            storage.connection,
            MiniblockNumber(1),
            true,
        )
        .with_caches(caches.clone());
        assert!(!storage.is_write_initial(&logs[0].key));
        assert!(storage.is_write_initial(&non_existing_key));

        // Check that the cache entries are still as expected.
        assert_eq!(
            caches.initial_writes.get(&logs[0].key),
            Some(L1BatchNumber(1))
        );
        assert_eq!(
            caches.negative_initial_writes.get(&non_existing_key),
            Some(L1BatchNumber(2))
        );

        let mut storage = PostgresStorage::new(
            storage.rt_handle,
            storage.connection,
            MiniblockNumber(2),
            false,
        )
        .with_caches(caches);

        // Check that the cached value has been used
        assert!(!storage.is_write_initial(&logs[0].key));
        assert!(storage.is_write_initial(&non_existing_key));
    }

    #[db_test]
    async fn using_initial_writes_cache(pool: ConnectionPool) {
        let handle = Handle::current();
        tokio::task::spawn_blocking(move || test_initial_writes_cache(&pool, handle))
            .await
            .unwrap();
    }

    fn test_values_cache(pool: &ConnectionPool, rt_handle: Handle) {
        let mut caches = PostgresStorageCaches::new(1_024, 1_024);
        let _ =
            caches.configure_storage_values_cache(1_024 * 1_024, pool.clone(), rt_handle.clone());
        // We cannot use an update task since it requires having concurrent DB connections
        // that don't work in tests. We'll update values cache manually instead.
        let values_cache = caches.values.as_ref().unwrap().cache.clone();

        let mut connection = rt_handle.block_on(pool.access_storage());
        rt_handle.block_on(prepare_postgres(&mut connection));

        let mut storage = PostgresStorage::new(rt_handle, connection, MiniblockNumber(0), false)
            .with_caches(caches.clone());
        let existing_key = gen_storage_logs(0..20)[1].key;
        let value = storage.read_value(&existing_key);
        assert!(!value.is_zero());

        // Check that the value is now cached.
        let cached_value = values_cache.get(MiniblockNumber(0), &existing_key).unwrap();
        assert_eq!(cached_value, value);

        let non_existing_key = gen_storage_logs(100..120)[0].key;
        let value = storage.read_value(&non_existing_key);
        assert_eq!(value, StorageValue::zero());

        let cached_value = values_cache
            .get(MiniblockNumber(0), &non_existing_key)
            .unwrap();
        assert_eq!(cached_value, StorageValue::zero());

        let logs = vec![
            StorageLog::new_write_log(existing_key, H256::repeat_byte(1)),
            StorageLog::new_write_log(non_existing_key, H256::repeat_byte(2)),
        ];
        storage.rt_handle.block_on(create_miniblock(
            &mut storage.connection,
            MiniblockNumber(1),
            logs,
        ));

        let mut storage = PostgresStorage::new(
            storage.rt_handle,
            storage.connection,
            MiniblockNumber(1),
            true,
        )
        .with_caches(caches);

        // Cached values should not be updated so far, and they should not be used
        assert_eq!(storage.read_value(&existing_key), H256::repeat_byte(1));
        assert_eq!(storage.read_value(&non_existing_key), H256::repeat_byte(2));

        assert!(values_cache
            .get(MiniblockNumber(1), &existing_key)
            .is_none());
        let cached_value = values_cache.get(MiniblockNumber(0), &existing_key).unwrap();
        assert_ne!(cached_value, H256::repeat_byte(1));
        assert!(values_cache
            .get(MiniblockNumber(1), &non_existing_key)
            .is_none());
        let cached_value = values_cache
            .get(MiniblockNumber(0), &non_existing_key)
            .unwrap();
        assert_ne!(cached_value, H256::repeat_byte(2));

        values_cache.update(
            MiniblockNumber(0),
            MiniblockNumber(1),
            &storage.rt_handle,
            &mut storage.connection,
        );
        assert_eq!(values_cache.0.read().unwrap().valid_for, MiniblockNumber(1));

        assert_eq!(storage.read_value(&existing_key), H256::repeat_byte(1));
        assert_eq!(storage.read_value(&non_existing_key), H256::repeat_byte(2));
        // Check that the values are now cached.
        let cached_value = values_cache.get(MiniblockNumber(1), &existing_key).unwrap();
        assert_eq!(cached_value, H256::repeat_byte(1));
        let cached_value = values_cache
            .get(MiniblockNumber(1), &non_existing_key)
            .unwrap();
        assert_eq!(cached_value, H256::repeat_byte(2));
    }

    #[db_test]
    async fn using_values_cache(pool: ConnectionPool) {
        let handle = Handle::current();
        tokio::task::spawn_blocking(move || test_values_cache(&pool, handle))
            .await
            .unwrap();
    }
}

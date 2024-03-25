use std::{
    mem,
    sync::{Arc, RwLock},
};

use anyhow::Context as _;
use tokio::{
    runtime::Handle,
    sync::{
        mpsc::{self, UnboundedReceiver},
        watch,
    },
};
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_types::{L1BatchNumber, MiniblockNumber, StorageKey, StorageValue, H256};

use self::metrics::{Method, ValuesUpdateStage, CACHE_METRICS, STORAGE_METRICS};
use crate::{
    cache::{lru_cache::LruCache, CacheValue},
    ReadStorage,
};

mod metrics;
#[cfg(test)]
mod tests;

/// Type alias for smart contract source code cache.
type FactoryDepsCache = LruCache<H256, Vec<u8>>;

impl CacheValue<H256> for Vec<u8> {
    fn cache_weight(&self) -> u32 {
        self.len().try_into().expect("Cached bytes are too large")
    }
}

/// Type alias for initial writes caches.
type InitialWritesCache = LruCache<StorageKey, L1BatchNumber>;

impl CacheValue<StorageKey> for L1BatchNumber {
    #[allow(clippy::cast_possible_truncation)] // doesn't happen in practice
    fn cache_weight(&self) -> u32 {
        const WEIGHT: usize = mem::size_of::<L1BatchNumber>() + mem::size_of::<StorageKey>();
        // ^ Since values are small in size, we want to account for key sizes as well

        WEIGHT as u32
    }
}

/// [`StorageValue`] together with a miniblock "timestamp" starting from which it is known to be valid.
///
/// Using timestamped values in [`ValuesCache`] enables using it for past miniblock states. As long as
/// a cached value has a "timestamp" older or equal than the requested miniblock, the value can be used.
///
/// Timestamp is assigned to equal the latest miniblock when a value is fetched from the storage.
/// A value may be valid for earlier miniblocks, but fetching the actual modification "timestamp"
/// would make the relevant Postgres query more complex.
#[derive(Debug, Clone, Copy)]
struct TimestampedStorageValue {
    value: StorageValue,
    loaded_at: MiniblockNumber,
}

impl CacheValue<H256> for TimestampedStorageValue {
    #[allow(clippy::cast_possible_truncation)] // doesn't happen in practice
    fn cache_weight(&self) -> u32 {
        const WEIGHT: usize = mem::size_of::<TimestampedStorageValue>() + mem::size_of::<H256>();
        // ^ Since values are small in size, we want to account for key sizes as well

        WEIGHT as u32
    }
}

#[derive(Debug)]
struct ValuesCacheInner {
    /// Miniblock up to which `self.values` are valid. Has the same meaning as `miniblock_number`
    /// in `PostgresStorage` (i.e., the latest sealed miniblock for which storage logs should
    /// be taken into account).
    valid_for: MiniblockNumber,
    values: LruCache<H256, TimestampedStorageValue>,
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
            values: LruCache::new("values_cache", capacity),
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
        if lock.valid_for < miniblock_number {
            // The request is from the future; we cannot say which values in the cache remain valid,
            // so we don't return *any* cached values.
            return None;
        }

        let timestamped_value = lock.values.get(&key.hashed_key())?;
        if timestamped_value.loaded_at <= miniblock_number {
            Some(timestamped_value.value)
        } else {
            None // The value is from the future
        }
    }

    /// Caches `value` for `key`, but only if the cache currently holds values for `miniblock_number`.
    fn insert(&self, miniblock_number: MiniblockNumber, key: StorageKey, value: StorageValue) {
        let lock = self.0.read().expect("values cache is poisoned");
        if lock.valid_for == miniblock_number {
            lock.values.insert(
                key.hashed_key(),
                TimestampedStorageValue {
                    value,
                    loaded_at: miniblock_number,
                },
            );
        } else {
            CACHE_METRICS.stale_values.inc();
        }
    }

    async fn update(
        &self,
        from_miniblock: MiniblockNumber,
        to_miniblock: MiniblockNumber,
        connection: &mut Connection<'_, Core>,
    ) -> anyhow::Result<()> {
        const MAX_MINIBLOCKS_LAG: u32 = 5;

        tracing::debug!(
            "Updating storage values cache from miniblock {from_miniblock} to {to_miniblock}"
        );

        if to_miniblock.0 - from_miniblock.0 > MAX_MINIBLOCKS_LAG {
            // We can spend too much time loading data from Postgres, so we opt for an easier "update" route:
            // evict *everything* from cache and call it a day. This should not happen too often in practice.
            tracing::info!(
                "Storage values cache is too far behind (current miniblock is {from_miniblock}; \
                 requested update to {to_miniblock}); resetting the cache"
            );
            let mut lock = self
                .0
                .write()
                .map_err(|_| anyhow::anyhow!("values cache is poisoned"))?;
            anyhow::ensure!(
                lock.valid_for == from_miniblock,
                "sanity check failed: values cache was expected to be valid for miniblock #{from_miniblock}, but it's actually \
                 valid for miniblock #{}",
                lock.valid_for
            );
            lock.valid_for = to_miniblock;
            lock.values.clear();

            CACHE_METRICS.values_emptied.inc();
        } else {
            let update_latency = CACHE_METRICS.values_update[&ValuesUpdateStage::LoadKeys].start();
            let miniblocks = (from_miniblock + 1)..=to_miniblock;
            let modified_keys = connection
                .storage_logs_dal()
                .modified_keys_in_miniblocks(miniblocks.clone())
                .await
                .with_context(|| {
                    format!("failed loading modified keys for miniblocks {miniblocks:?}")
                })?;

            let elapsed = update_latency.observe();
            CACHE_METRICS
                .values_update_modified_keys
                .observe(modified_keys.len());
            tracing::debug!(
                "Loaded {modified_keys_len} modified storage keys from miniblocks {miniblocks:?}; \
                 took {elapsed:?}",
                modified_keys_len = modified_keys.len()
            );

            let update_latency =
                CACHE_METRICS.values_update[&ValuesUpdateStage::RemoveStaleKeys].start();
            let mut lock = self
                .0
                .write()
                .map_err(|_| anyhow::anyhow!("values cache is poisoned"))?;
            // The code below holding onto the write `lock` is the only code that can theoretically poison the `RwLock`
            // (other than emptying the cache above). Thus, it's kept as simple and tight as possible.
            // E.g., we load data from Postgres beforehand.
            anyhow::ensure!(
                lock.valid_for == from_miniblock,
                "sanity check failed: values cache was expected to be valid for miniblock #{from_miniblock}, but it's actually \
                 valid for miniblock #{}",
                lock.valid_for
            );
            lock.valid_for = to_miniblock;
            for modified_key in &modified_keys {
                lock.values.remove(modified_key);
            }
            lock.values.report_size();
            drop(lock);
            update_latency.observe();
        }

        CACHE_METRICS
            .values_valid_for_miniblock
            .set(u64::from(to_miniblock.0));
        Ok(())
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
        tracing::debug!(
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
        connection_pool: ConnectionPool<Core>,
    ) -> PostgresStorageCachesTask {
        assert!(
            capacity > 0,
            "Storage values cache capacity must be positive"
        );
        tracing::debug!("Initializing VM storage values cache with {capacity}B capacity");

        let (command_sender, command_receiver) = mpsc::unbounded_channel();
        let values_cache = ValuesCache::new(capacity);
        self.values = Some(ValuesCacheAndUpdater {
            cache: values_cache.clone(),
            command_sender,
        });

        // We want to run updates in a separate task in order to not block VM execution on update
        // and keep contention over the `ValuesCache` lock as low as possible. As a downside,
        // `Self::schedule_values_update()` will produce some no-op update commands from concurrently
        // executing VM instances. Due to built-in filtering, this seems manageable.
        PostgresStorageCachesTask {
            connection_pool,
            values_cache,
            command_receiver,
        }
    }

    /// Schedules an update of the VM storage values cache to the specified miniblock. If the values cache is not configured,
    /// this is a no-op.
    ///
    /// # Panics
    ///
    /// - Panics if the cache update task returned from `configure_storage_values_cache()` has panicked.
    pub fn schedule_values_update(&self, to_miniblock: MiniblockNumber) {
        let Some(values) = &self.values else {
            return;
        };
        if values.cache.valid_for() < to_miniblock {
            // Filter out no-op updates right away in order to not store lots of them in RAM.
            values
                .command_sender
                .send(to_miniblock)
                .expect("values cache update task failed");
        }
    }
}

/// An asynchronous task that updates the VM storage values cache.
#[derive(Debug)]
pub struct PostgresStorageCachesTask {
    connection_pool: ConnectionPool<Core>,
    values_cache: ValuesCache,
    command_receiver: UnboundedReceiver<MiniblockNumber>,
}

impl PostgresStorageCachesTask {
    /// Runs the task.
    ///
    /// ## Errors
    ///
    /// - Propagates Postgres errors.
    /// - Propagates errors from the cache update task.
    pub async fn run(mut self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut current_miniblock = self.values_cache.valid_for();
        loop {
            tokio::select! {
                _ = stop_receiver.changed() => {
                    break;
                }
                Some(to_miniblock) = self.command_receiver.recv() => {
                    if to_miniblock <= current_miniblock {
                        continue;
                    }
                    let mut connection = self
                        .connection_pool
                        .connection_tagged("values_cache_updater")
                        .await?;
                    self.values_cache
                        .update(current_miniblock, to_miniblock, &mut connection)
                        .await?;
                    current_miniblock = to_miniblock;
                }
                else => {
                    // The command sender has been dropped, which means that we must receive the stop signal soon.
                    stop_receiver.changed().await?;
                    break;
                }
            }
        }
        Ok(())
    }
}

/// [`ReadStorage`] implementation backed by the Postgres database.
#[derive(Debug)]
pub struct PostgresStorage<'a> {
    rt_handle: Handle,
    connection: Connection<'a, Core>,
    miniblock_number: MiniblockNumber,
    l1_batch_number_for_miniblock: L1BatchNumber,
    pending_l1_batch_number: L1BatchNumber,
    consider_new_l1_batch: bool,
    caches: Option<PostgresStorageCaches>,
}

impl<'a> PostgresStorage<'a> {
    /// Creates a new storage using the specified connection.
    ///
    /// # Panics
    ///
    /// Panics on Postgres errors.
    pub fn new(
        rt_handle: Handle,
        connection: Connection<'a, Core>,
        block_number: MiniblockNumber,
        consider_new_l1_batch: bool,
    ) -> Self {
        rt_handle
            .clone()
            .block_on(Self::new_async(
                rt_handle,
                connection,
                block_number,
                consider_new_l1_batch,
            ))
            .unwrap()
    }

    /// Asynchronous version of [`Self::new()`] that also propagates errors instead of panicking.
    ///
    /// # Errors
    ///
    /// Propagates Postgres errors.
    pub async fn new_async(
        rt_handle: Handle,
        mut connection: Connection<'a, Core>,
        block_number: MiniblockNumber,
        consider_new_l1_batch: bool,
    ) -> anyhow::Result<PostgresStorage<'a>> {
        let resolved = connection
            .storage_web3_dal()
            .resolve_l1_batch_number_of_miniblock(block_number)
            .await
            .with_context(|| {
                format!("failed resolving L1 batch number for miniblock #{block_number}")
            })?;
        Ok(Self {
            rt_handle,
            connection,
            miniblock_number: block_number,
            l1_batch_number_for_miniblock: resolved.expected_l1_batch(),
            pending_l1_batch_number: resolved.pending_l1_batch,
            consider_new_l1_batch,
            caches: None,
        })
    }

    /// Sets the caches to use with the storage.
    #[must_use]
    pub fn with_caches(self, caches: PostgresStorageCaches) -> Self {
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
        let latency = STORAGE_METRICS.storage[&Method::ReadValue].start();
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

        latency.observe();
        value
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        let latency = STORAGE_METRICS.storage[&Method::IsWriteInitial].start();
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
                    CACHE_METRICS.effective_values.inc();
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
        latency.observe();

        let contains_key = l1_batch_number.map_or(false, |initial_write_l1_batch_number| {
            self.write_counts(initial_write_l1_batch_number)
        });
        !contains_key
    }

    fn load_factory_dep(&mut self, hash: H256) -> Option<Vec<u8>> {
        let latency = STORAGE_METRICS.storage[&Method::LoadFactoryDep].start();

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

        latency.observe();
        result
    }

    fn get_enumeration_index(&mut self, key: &StorageKey) -> Option<u64> {
        let mut dal = self.connection.storage_logs_dedup_dal();
        let value = self
            .rt_handle
            .block_on(dal.get_enumeration_index_for_key(key.hashed_key()));
        value.expect("failed getting enumeration index for key")
    }
}

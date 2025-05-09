use std::{
    mem,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use anyhow::Context as _;
use backon::{BlockingRetryable, ConstantBuilder};
use tokio::{runtime::Handle, sync::watch};
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_types::{L1BatchNumber, L2BlockNumber, StorageKey, StorageValue, H256};
use zksync_vm_interface::storage::ReadStorage;

use self::metrics::{Method, ValuesUpdateStage, CACHE_METRICS, STORAGE_METRICS};
use crate::cache::lru_cache::LruCache;

mod metrics;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq)]
struct TimestampedFactoryDep {
    bytecode: Vec<u8>,
    inserted_at: L2BlockNumber,
}

/// Type alias for smart contract source code cache.
type FactoryDepsCache = LruCache<H256, TimestampedFactoryDep>;

/// Type alias for initial writes caches.
type InitialWritesCache = LruCache<H256, L1BatchNumber>;

/// [`StorageValue`] together with an L2 block "timestamp" starting from which it is known to be valid.
///
/// Using timestamped values in [`ValuesCache`] enables using it for past L2 block states. As long as
/// a cached value has a "timestamp" older or equal than the requested L2 block, the value can be used.
///
/// Timestamp is assigned to equal the latest L2 block when a value is fetched from the storage.
/// A value may be valid for earlier L2 blocks, but fetching the actual modification "timestamp"
/// would make the relevant Postgres query more complex.
#[derive(Debug, Clone, Copy)]
struct TimestampedStorageValue {
    value: StorageValue,
    loaded_at: L2BlockNumber,
}

#[derive(Debug)]
struct ValuesCacheInner {
    /// L2 block up to which `self.values` are valid. Has the same meaning as `l2_block_number`
    /// in `PostgresStorage` (i.e., the latest sealed L2 block for which storage logs should
    /// be taken into account).
    valid_for: L2BlockNumber,
    values: LruCache<H256, TimestampedStorageValue>,
}

/// Cache for the VM storage. Only caches values for a single VM storage snapshot, which logically
/// corresponds to the latest sealed L2 block in Postgres.
///
/// The cached snapshot can be updated, which will load changed storage keys from Postgres and remove
/// the (potentially stale) cached values for these keys.
///
/// # Why wrap the cache in `RwLock`?
///
/// We need to be sure that `valid_for` L2 block of the values cache has not changed while we are
/// loading or storing values in it. This is easiest to achieve using an `RwLock`. Note that
/// almost all cache ops require only shared access to the lock (including cache updates!); we only
/// need exclusive access when we are updating the `valid_for` L2 block. Further, the update itself
/// doesn't grab the lock until *after* the Postgres data has been loaded. (This works because we
/// know statically that there is a single thread updating the cache; hence, we have no contention
/// over updating the cache.) To summarize, `RwLock` should see barely any contention.
#[derive(Debug, Clone)]
struct ValuesCache(Arc<RwLock<ValuesCacheInner>>);

impl ValuesCache {
    fn new(capacity: u64) -> Self {
        let inner = ValuesCacheInner {
            valid_for: L2BlockNumber(0),
            values: LruCache::uniform("values_cache", capacity),
        };
        Self(Arc::new(RwLock::new(inner)))
    }

    fn capacity(&self) -> u64 {
        self.0
            .read()
            .expect("values cache is poisoned")
            .values
            .capacity()
    }

    /// *NB.* The returned value should be considered immediately stale; at best, it can be
    /// the lower boundary on the current `valid_for` value.
    fn valid_for(&self) -> L2BlockNumber {
        self.0.read().expect("values cache is poisoned").valid_for
    }

    /// Gets the cached value for `key` provided that the cache currently holds values
    /// for `l2_block_number`.
    fn get(&self, l2_block_number: L2BlockNumber, hashed_key: H256) -> Option<StorageValue> {
        let lock = self.0.read().expect("values cache is poisoned");
        if lock.valid_for < l2_block_number {
            // The request is from the future; we cannot say which values in the cache remain valid,
            // so we don't return *any* cached values.
            return None;
        }

        let timestamped_value = lock.values.get(&hashed_key)?;
        if timestamped_value.loaded_at <= l2_block_number {
            Some(timestamped_value.value)
        } else {
            None // The value is from the future
        }
    }

    /// Caches `value` for `key`, but only if the cache currently holds values for `l2_block_number`.
    fn insert(&self, l2_block_number: L2BlockNumber, hashed_key: H256, value: StorageValue) {
        let lock = self.0.read().expect("values cache is poisoned");
        if lock.valid_for == l2_block_number {
            lock.values.insert(
                hashed_key,
                TimestampedStorageValue {
                    value,
                    loaded_at: l2_block_number,
                },
            );
        } else {
            CACHE_METRICS.stale_values.inc();
        }
    }

    fn reset(
        &self,
        from_l2_block: L2BlockNumber,
        to_l2_block: L2BlockNumber,
    ) -> anyhow::Result<()> {
        // We can spend too much time loading data from Postgres, so we opt for an easier "update" route:
        // evict *everything* from cache and call it a day. This should not happen too often in practice.
        tracing::info!(
            "Storage values cache is too far behind (current L2 block is {from_l2_block}; \
             requested update to {to_l2_block}); resetting the cache"
        );
        let mut lock = self
            .0
            .write()
            .map_err(|_| anyhow::anyhow!("values cache is poisoned"))?;
        anyhow::ensure!(
            lock.valid_for == from_l2_block,
            "sanity check failed: values cache was expected to be valid for L2 block #{from_l2_block}, but it's actually \
             valid for L2 block #{}",
            lock.valid_for
        );
        lock.valid_for = to_l2_block;
        lock.values.clear();

        CACHE_METRICS.values_emptied.inc();
        CACHE_METRICS
            .values_valid_for_miniblock
            .set(u64::from(to_l2_block.0));
        Ok(())
    }

    async fn update(
        &self,
        from_l2_block: L2BlockNumber,
        to_l2_block: L2BlockNumber,
        receive_latency: Duration,
        connection: &mut Connection<'_, Core>,
    ) -> anyhow::Result<()> {
        if from_l2_block == L2BlockNumber(0) {
            tracing::debug!(
                ?receive_latency,
                "Initializing storage values cache at L2 block {to_l2_block}"
            );
        } else {
            tracing::debug!(
                ?receive_latency,
                "Updating storage values cache from L2 block {from_l2_block} to {to_l2_block}"
            );
        }

        let update_latency = CACHE_METRICS.values_update[&ValuesUpdateStage::LoadKeys].start();
        let l2_blocks = (from_l2_block + 1)..=to_l2_block;
        let modified_keys = connection
            .storage_logs_dal()
            .modified_keys_in_l2_blocks(l2_blocks.clone())
            .await?;

        let elapsed = update_latency.observe();
        CACHE_METRICS
            .values_update_modified_keys
            .observe(modified_keys.len());
        tracing::debug!(
            "Loaded {modified_keys_len} modified storage keys from L2 blocks {l2_blocks:?}; \
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
            lock.valid_for == from_l2_block,
            "sanity check failed: values cache was expected to be valid for L2 block #{from_l2_block}, but it's actually \
             valid for L2 block #{}",
            lock.valid_for
        );
        lock.valid_for = to_l2_block;
        for modified_key in &modified_keys {
            lock.values.remove(modified_key);
        }
        lock.values.report_size();
        drop(lock);
        update_latency.observe();

        CACHE_METRICS
            .values_valid_for_miniblock
            .set(u64::from(to_l2_block.0));
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct ValuesCacheAndUpdater {
    cache: ValuesCache,
    command_sender: Arc<watch::Sender<(L2BlockNumber, Instant)>>,
}

/// Caches used during VM execution.
///
/// Currently, this struct includes the following caches:
///
/// - Cache for smart contract bytecodes (never invalidated, since it is content-addressable)
/// - Cache for L1 batch numbers of initial writes for storage keys (never invalidated, except after
///   reverting L1 batch execution)
/// - Cache of the VM storage snapshot corresponding to the latest sealed L2 block
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
    /// Creates caches with the specified capacities measured in bytes.
    #[allow(clippy::cast_possible_truncation, clippy::missing_panics_doc)] // not triggered in practice
    pub fn new(factory_deps_capacity: u64, initial_writes_capacity: u64) -> Self {
        tracing::debug!(
            "Initialized VM execution cache with {factory_deps_capacity}B capacity for factory deps, \
             {initial_writes_capacity}B capacity for initial writes"
        );

        Self {
            factory_deps: FactoryDepsCache::weighted(
                "factory_deps_cache",
                factory_deps_capacity,
                |_, value| {
                    (value.bytecode.len() + mem::size_of::<L2BlockNumber>())
                        .try_into()
                        .expect("Cached bytes are too large")
                },
            ),
            initial_writes: InitialWritesCache::uniform(
                "initial_writes_cache",
                initial_writes_capacity / 2,
            ),
            negative_initial_writes: InitialWritesCache::uniform(
                "negative_initial_writes_cache",
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
        max_l2_blocks_lag: u32,
        connection_pool: ConnectionPool<Core>,
    ) -> PostgresStorageCachesTask {
        assert!(
            capacity > 0,
            "Storage values cache capacity must be positive"
        );
        tracing::debug!("Initializing VM storage values cache with {capacity}B capacity");

        let (command_sender, command_receiver) = watch::channel((L2BlockNumber(0), Instant::now()));
        let values_cache = ValuesCache::new(capacity);
        self.values = Some(ValuesCacheAndUpdater {
            cache: values_cache.clone(),
            command_sender: Arc::new(command_sender),
        });

        // We want to run updates in a separate task in order to not block VM execution on update
        // and keep contention over the `ValuesCache` lock as low as possible. As a downside,
        // `Self::schedule_values_update()` will produce some no-op update commands from concurrently
        // executing VM instances. Due to built-in filtering, this seems manageable.
        PostgresStorageCachesTask {
            connection_pool,
            values_cache,
            max_l2_blocks_lag,
            command_receiver,
        }
    }

    /// Schedules an update of the VM storage values cache to the specified L2 block. If the values cache is not configured,
    /// this is a no-op.
    ///
    /// # Panics
    ///
    /// - Panics if the cache update task returned from `configure_storage_values_cache()` has panicked.
    pub fn schedule_values_update(&self, to_l2_block: L2BlockNumber) {
        let Some(values) = &self.values else {
            return;
        };

        values.command_sender.send_if_modified(|command| {
            if command.0 < to_l2_block {
                let now = Instant::now();
                let command_interval = now
                    .checked_duration_since(command.1)
                    .unwrap_or(Duration::ZERO);
                CACHE_METRICS
                    .values_command_interval
                    .observe(command_interval);
                tracing::debug!(
                    ?command_interval,
                    "Queued update command from L2 block {} to {to_l2_block}",
                    command.0
                );
                *command = (to_l2_block, now);
                true
            } else {
                // Filter out no-op updates right away in order to not wake up the update task unnecessarily.
                // Since the task updating the values cache (`PostgresStorageCachesTask`) is cancel-aware,
                // it can stop before some of `schedule_values_update()` calls; in this case, it's OK
                // to ignore the updates.
                false
            }
        });
    }
}

/// An asynchronous task that updates the VM storage values cache.
#[derive(Debug)]
pub struct PostgresStorageCachesTask {
    connection_pool: ConnectionPool<Core>,
    values_cache: ValuesCache,
    max_l2_blocks_lag: u32,
    command_receiver: watch::Receiver<(L2BlockNumber, Instant)>,
}

impl PostgresStorageCachesTask {
    /// Runs the task.
    ///
    /// ## Errors
    ///
    /// - Propagates Postgres errors.
    /// - Propagates errors from the cache update task.
    #[tracing::instrument(name = "PostgresStorageCachesTask::run", skip_all)]
    pub async fn run(mut self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        tracing::info!(
            max_l2_blocks_lag = self.max_l2_blocks_lag,
            values_cache.capacity = self.values_cache.capacity(),
            "Starting task"
        );

        let mut current_l2_block = self.values_cache.valid_for();
        loop {
            let (to_l2_block, queued_at) = tokio::select! {
                _ = stop_receiver.changed() => break,
                Ok(()) = self.command_receiver.changed() => {
                    *self.command_receiver.borrow_and_update()
                },
                else => {
                    // The command sender has been dropped, which means that we must receive the stop signal soon.
                    stop_receiver.changed().await?;
                    break;
                }
            };

            let receive_latency = queued_at.elapsed();
            CACHE_METRICS
                .values_receive_latency
                .observe(receive_latency);
            if to_l2_block <= current_l2_block {
                continue;
            }

            if to_l2_block.0 - current_l2_block.0 > self.max_l2_blocks_lag {
                self.values_cache.reset(current_l2_block, to_l2_block)?;
            } else {
                let mut connection = self
                    .connection_pool
                    .connection_tagged("values_cache_updater")
                    .await?;
                self.values_cache
                    .update(
                        current_l2_block,
                        to_l2_block,
                        receive_latency,
                        &mut connection,
                    )
                    .await?;
            }
            current_l2_block = to_l2_block;
        }
        Ok(())
    }
}

/// [`ReadStorage`] implementation backed by the Postgres database.
#[derive(Debug)]
pub struct PostgresStorage<'a> {
    rt_handle: Handle,
    connection: Connection<'a, Core>,
    l2_block_number: L2BlockNumber,
    l1_batch_number_for_l2_block: L1BatchNumber,
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
        block_number: L2BlockNumber,
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
        block_number: L2BlockNumber,
        consider_new_l1_batch: bool,
    ) -> anyhow::Result<Self> {
        let resolved = connection
            .storage_web3_dal()
            .resolve_l1_batch_number_of_l2_block(block_number)
            .await
            .with_context(|| {
                format!("failed resolving L1 batch number for L2 block #{block_number}")
            })?;
        Ok(Self {
            rt_handle,
            connection,
            l2_block_number: block_number,
            l1_batch_number_for_l2_block: resolved.expected_l1_batch(),
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
            self.l1_batch_number_for_l2_block >= write_l1_batch_number
        } else {
            self.l1_batch_number_for_l2_block > write_l1_batch_number
        }
    }

    fn values_cache(&self) -> Option<&ValuesCache> {
        Some(&self.caches.as_ref()?.values.as_ref()?.cache)
    }

    /// Returns the wrapped connection.
    pub fn into_inner(self) -> Connection<'a, Core> {
        self.connection
    }
}

impl ReadStorage for PostgresStorage<'_> {
    fn read_value(&mut self, key: &StorageKey) -> StorageValue {
        let hashed_key = key.hashed_key();
        let latency = STORAGE_METRICS.storage[&Method::ReadValue].start();
        let values_cache = self.values_cache();
        let cached_value =
            values_cache.and_then(|cache| cache.get(self.l2_block_number, hashed_key));

        let value = cached_value.unwrap_or_else(|| {
            const RETRY_INTERVAL: Duration = Duration::from_millis(500);
            const MAX_TRIES: usize = 20;

            let mut dal = self.connection.storage_web3_dal();
            let value = (|| {
                self.rt_handle
                    .block_on(dal.get_historical_value_unchecked(hashed_key, self.l2_block_number))
            })
            .retry(
                &ConstantBuilder::default()
                    .with_delay(RETRY_INTERVAL)
                    .with_max_times(MAX_TRIES),
            )
            .when(|e| {
                e.inner()
                    .as_database_error()
                    .is_some_and(|e| e.message() == "canceling statement due to statement timeout")
            })
            .call()
            .expect("Failed executing `read_value`");
            if let Some(cache) = self.values_cache() {
                cache.insert(self.l2_block_number, hashed_key, value);
            }
            value
        });

        latency.observe();
        value
    }

    fn is_write_initial(&mut self, key: &StorageKey) -> bool {
        let hashed_key = key.hashed_key();
        let latency = STORAGE_METRICS.storage[&Method::IsWriteInitial].start();
        let caches = self.caches.as_ref();
        let cached_value = caches.and_then(|caches| caches.initial_writes.get(&hashed_key));

        if cached_value.is_none() {
            // Write is absent in positive cache, check whether it's present in the negative cache.
            let cached_value =
                caches.and_then(|caches| caches.negative_initial_writes.get(&hashed_key));
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
                .block_on(dal.get_l1_batch_number_for_initial_write(hashed_key))
                .expect("Failed executing `is_write_initial`");

            if let Some(caches) = &self.caches {
                if let Some(l1_batch_number) = value {
                    caches.negative_initial_writes.remove(&hashed_key);
                    caches.initial_writes.insert(hashed_key, l1_batch_number);
                } else {
                    caches
                        .negative_initial_writes
                        .insert(hashed_key, self.pending_l1_batch_number);
                    // The pending L1 batch might have been sealed since its number was requested from Postgres
                    // in `Self::new()`, so this is a somewhat conservative estimate.
                }
            }
            value
        });
        latency.observe();

        let contains_key = l1_batch_number.is_some_and(|initial_write_l1_batch_number| {
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

        let value = cached_value.or_else(|| {
            let mut dal = self.connection.storage_web3_dal();
            let value = self
                .rt_handle
                .block_on(dal.get_factory_dep(hash))
                .expect("Failed executing `load_factory_dep`")
                .map(|(bytecode, inserted_at)| TimestampedFactoryDep {
                    bytecode,
                    inserted_at,
                });

            if let Some(caches) = &self.caches {
                // If we receive None, we won't cache it.
                if let Some(value) = value.clone() {
                    caches.factory_deps.insert(hash, value);
                }
            }

            value
        });

        latency.observe();
        Some(
            value
                .filter(|dep| dep.inserted_at <= self.l2_block_number)?
                .bytecode,
        )
    }

    fn get_enumeration_index(&mut self, key: &StorageKey) -> Option<u64> {
        let hashed_key = key.hashed_key();
        let mut dal = self.connection.storage_logs_dedup_dal();
        let value = self.rt_handle.block_on(
            dal.get_enumeration_index_in_l1_batch(hashed_key, self.l1_batch_number_for_l2_block),
        );
        value.expect("failed getting enumeration index for key")
    }
}

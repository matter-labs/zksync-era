use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    ffi::CStr,
    fmt, iter,
    marker::PhantomData,
    num::NonZeroU32,
    ops,
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Condvar, Mutex, Weak,
    },
    thread,
    time::{Duration, Instant},
};

use rocksdb::{
    checkpoint::Checkpoint, perf, properties, BlockBasedOptions, Cache, ColumnFamily,
    ColumnFamilyDescriptor, DBPinnableSlice, Direction, IteratorMode, Options, PrefixRange,
    ReadOptions, WriteOptions, DB,
};
use thread_local::ThreadLocal;
use vise::MetricsFamily;

use crate::metrics::{
    BlockCacheKind, DbLabel, RocksdbLabels, RocksdbProfilingLabels, RocksdbSizeMetrics, METRICS,
    PROF_METRICS,
};

/// Number of active RocksDB instances used to determine if it's safe to exit current process.
/// Not properly dropped RocksDB instances can lead to DB corruption.
static ROCKSDB_INSTANCE_COUNTER: (Mutex<usize>, Condvar) = (Mutex::new(0), Condvar::new());

/// Describes column family used in a [`RocksDB`] instance.
pub trait NamedColumnFamily: 'static + Copy {
    /// Name of the database. Used in metrics reporting.
    const DB_NAME: &'static str;
    /// Lists all column families in the database.
    const ALL: &'static [Self];
    /// Names a column family to access it in `RocksDB`. Also used in metrics reporting.
    fn name(&self) -> &'static str;

    /// Returns whether this CF is so large that it's likely to require special configuration in terms
    /// of compaction / memtables.
    fn requires_tuning(&self) -> bool {
        false
    }
}

/// Thin typesafe wrapper around RocksDB `WriteBatch`.
#[must_use = "Batch should be written to DB"]
pub struct WriteBatch<'a, CF> {
    inner: rocksdb::WriteBatch,
    db: &'a RocksDB<CF>,
}

impl<CF: fmt::Debug> fmt::Debug for WriteBatch<'_, CF> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WriteBatch")
            .field("db", self.db)
            .finish()
    }
}

impl<CF: NamedColumnFamily> WriteBatch<'_, CF> {
    pub fn put_cf(&mut self, cf: CF, key: &[u8], value: &[u8]) {
        let cf = self.db.column_family(cf);
        self.inner.put_cf(cf, key, value);
    }

    pub fn delete_cf(&mut self, cf: CF, key: &[u8]) {
        let cf = self.db.column_family(cf);
        self.inner.delete_cf(cf, key);
    }

    pub fn delete_range_cf(&mut self, cf: CF, keys: ops::Range<&[u8]>) {
        let cf = self.db.column_family(cf);
        self.inner.delete_range_cf(cf, keys.start, keys.end);
    }

    pub fn size_in_bytes(&self) -> usize {
        self.inner.size_in_bytes()
    }
}

struct RocksDBCaches {
    /// LRU block cache shared among all column families.
    shared: Option<Cache>,
}

impl fmt::Debug for RocksDBCaches {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RocksDBCaches")
            .finish_non_exhaustive()
    }
}

impl RocksDBCaches {
    fn new(capacity: Option<usize>) -> Self {
        let shared = capacity.map(Cache::new_lru_cache);
        Self { shared }
    }
}

/// Size statistics for a column family. All sizes are measured in bytes.
#[derive(Debug)]
pub struct SizeStats {
    pub live_data_size: u64,
    pub total_sst_size: u64,
    pub total_mem_table_size: u64,
    pub block_cache_size: u64,
    pub index_and_filters_size: u64,
    pub files_at_level: Vec<u64>,
}

#[derive(Debug)]
pub(crate) struct RocksDBInner {
    db: DB,
    db_name: &'static str,
    cf_names: HashSet<&'static str>,
    _registry_entry: RegistryEntry,
    // Importantly, `Cache`s must be dropped after `DB`, so we place them as the last field
    // (fields in a struct are dropped in the declaration order).
    _caches: RocksDBCaches,
}

impl RocksDBInner {
    pub(crate) fn collect_metrics(
        &self,
        metrics: &MetricsFamily<RocksdbLabels, RocksdbSizeMetrics>,
    ) {
        for &cf_name in &self.cf_names {
            let cf = self.db.cf_handle(cf_name).unwrap();
            // ^ `unwrap()` is safe (CF existence is checked during DB initialization)
            let labels = RocksdbLabels::new(self.db_name, cf_name);
            let metrics = &metrics[&labels];

            let writes_stopped = self.int_property(cf, properties::IS_WRITE_STOPPED);
            let writes_stopped = writes_stopped == Some(1);
            metrics.writes_stopped.set(writes_stopped.into());

            let num_immutable_memtables =
                self.int_property(cf, properties::NUM_IMMUTABLE_MEM_TABLE);
            if let Some(num_immutable_memtables) = num_immutable_memtables {
                metrics.immutable_mem_tables.set(num_immutable_memtables);
            }
            let num_level0_files = self.int_property(cf, &properties::num_files_at_level(0));
            if let Some(num_level0_files) = num_level0_files {
                metrics.level0_files.set(num_level0_files);
            }
            let num_flushes = self.int_property(cf, properties::NUM_RUNNING_FLUSHES);
            if let Some(num_flushes) = num_flushes {
                metrics.running_flushes.set(num_flushes);
            }
            let num_compactions = self.int_property(cf, properties::NUM_RUNNING_COMPACTIONS);
            if let Some(num_compactions) = num_compactions {
                metrics.running_compactions.set(num_compactions);
            }
            let pending_compactions =
                self.int_property(cf, properties::ESTIMATE_PENDING_COMPACTION_BYTES);
            if let Some(pending_compactions) = pending_compactions {
                metrics.pending_compactions.set(pending_compactions);
            }

            let size_stats = self.size_stats(cf);
            metrics.live_data_size.set(size_stats.live_data_size);
            metrics.total_sst_size.set(size_stats.total_sst_size);
            metrics
                .total_mem_table_size
                .set(size_stats.total_mem_table_size);
            metrics.block_cache_size.set(size_stats.block_cache_size);
            metrics
                .index_and_filters_size
                .set(size_stats.index_and_filters_size);

            for (level, files_at_level) in size_stats.files_at_level.into_iter().enumerate() {
                metrics.files_at_level[&level].set(files_at_level);
            }
        }
    }

    fn int_property(&self, cf: &ColumnFamily, name: &CStr) -> Option<u64> {
        let property = self.db.property_int_value_cf(cf, name);
        let property = property.unwrap_or_else(|err| {
            let name_str = name.to_str().unwrap_or("(non-UTF8 string)");
            tracing::warn!(%err, "Failed getting RocksDB property `{name_str}`");
            None
        });
        if property.is_none() {
            let name_str = name.to_str().unwrap_or("(non-UTF8 string)");
            tracing::warn!("Property `{name_str}` is not defined");
        }
        property
    }

    fn size_stats(&self, cf: &ColumnFamily) -> SizeStats {
        const MAX_LEVEL: usize = 6;

        let live_data_size = self
            .int_property(cf, properties::ESTIMATE_LIVE_DATA_SIZE)
            .unwrap_or(0);
        let total_sst_size = self
            .int_property(cf, properties::TOTAL_SST_FILES_SIZE)
            .unwrap_or(0);
        let total_mem_table_size = self
            .int_property(cf, properties::SIZE_ALL_MEM_TABLES)
            .unwrap_or(0);
        let block_cache_size = self
            .int_property(cf, properties::BLOCK_CACHE_USAGE)
            .unwrap_or(0);
        let index_and_filters_size = self
            .int_property(cf, properties::ESTIMATE_TABLE_READERS_MEM)
            .unwrap_or(0);
        let files_at_level = (0..=MAX_LEVEL).map(|level| {
            self.int_property(cf, &properties::num_files_at_level(level))
                .unwrap_or(0)
        });

        SizeStats {
            live_data_size,
            total_sst_size,
            total_mem_table_size,
            block_cache_size,
            index_and_filters_size,
            files_at_level: files_at_level.collect(),
        }
    }

    /// Waits until writes are not stopped for any of the CFs. Writes can stop immediately on DB initialization
    /// if there are too many level-0 SST files; in this case, it may help waiting several seconds until
    /// these files are compacted.
    fn wait_for_writes_to_resume(&self, retries: &StalledWritesRetries) {
        for (retry_idx, retry_interval) in retries.intervals().enumerate() {
            let cfs_with_stopped_writes = self.cf_names.iter().copied().filter(|cf_name| {
                let cf = self.db.cf_handle(cf_name).unwrap();
                // ^ `unwrap()` is safe (CF existence is checked during DB initialization)
                self.int_property(cf, properties::IS_WRITE_STOPPED) == Some(1)
            });
            let cfs_with_stopped_writes: Vec<_> = cfs_with_stopped_writes.collect();
            if cfs_with_stopped_writes.is_empty() {
                return;
            } else {
                tracing::info!(
                    "Writes are stopped for column families {cfs_with_stopped_writes:?} in DB `{}` \
                     (retry #{retry_idx})",
                    self.db_name
                );
                thread::sleep(retry_interval);
            }
        }

        tracing::warn!(
            "Exceeded retries waiting for writes to resume in DB `{}`; proceeding with stopped writes",
            self.db_name
        );
    }
}

impl Drop for RocksDBInner {
    fn drop(&mut self) {
        tracing::debug!(
            "Canceling background compactions / flushes for DB `{}`",
            self.db_name
        );
        self.db.cancel_all_background_work(true);
    }
}

/// Configuration for retries when RocksDB writes are stalled.
#[derive(Debug, Clone, Copy)]
pub struct StalledWritesRetries {
    max_batch_size: usize,
    timeout: Duration,
    start_interval: Duration,
    max_interval: Duration,
    scale_factor: f64,
}

impl StalledWritesRetries {
    /// Creates retries configuration with the specified timeout.
    pub fn new(timeout: Duration) -> Self {
        Self {
            max_batch_size: 128 << 20, // 128 MiB
            timeout,
            start_interval: Duration::from_millis(50),
            max_interval: Duration::from_secs(2),
            scale_factor: 1.5,
        }
    }
}

impl StalledWritesRetries {
    fn intervals(&self) -> impl Iterator<Item = Duration> {
        let &Self {
            timeout,
            start_interval,
            max_interval,
            scale_factor,
            ..
        } = self;
        let started_at = Instant::now();

        iter::successors(Some(start_interval), move |&prev_interval| {
            Some(prev_interval.mul_f64(scale_factor).min(max_interval))
        })
        .take_while(move |_| started_at.elapsed() <= timeout)
    }

    // **NB.** The error message may change between RocksDB versions!
    fn is_write_stall_error(error: &rocksdb::Error) -> bool {
        matches!(error.kind(), rocksdb::ErrorKind::ShutdownInProgress)
            && error.as_ref().ends_with("stalled writes")
    }
}

/// [`RocksDB`] options.
#[derive(Debug, Clone, Copy)]
pub struct RocksDBOptions {
    /// Byte capacity of the block cache (the main RocksDB cache for reads). If not set, default RocksDB
    /// cache options will be used.
    pub block_cache_capacity: Option<usize>,
    /// If specified, RocksDB indices and Bloom filters will be managed by the block cache, rather than
    /// being loaded entirely into RAM on the RocksDB initialization. The block cache capacity should be increased
    /// correspondingly; otherwise, RocksDB performance can significantly degrade.
    pub include_indices_and_filters_in_block_cache: bool,
    /// Byte capacity of memtables (recent, non-persisted changes to RocksDB) set for large CFs
    /// (as defined in [`NamedColumnFamily::requires_tuning()`]).
    /// Setting this to a reasonably large value (order of 512 MiB) is helpful for large DBs that experience
    /// write stalls. If not set, large CFs will not be configured specially.
    pub large_memtable_capacity: Option<usize>,
    /// Timeout to wait for the database to run compaction on stalled writes during startup or
    /// when the corresponding RocksDB error is encountered.
    pub stalled_writes_retries: StalledWritesRetries,
    /// Number of open files that can be used by the DB. Default is None, for no limit.
    pub max_open_files: Option<NonZeroU32>,
}

impl Default for RocksDBOptions {
    fn default() -> Self {
        Self {
            block_cache_capacity: None,
            include_indices_and_filters_in_block_cache: false,
            large_memtable_capacity: None,
            stalled_writes_retries: StalledWritesRetries::new(Duration::from_secs(10)),
            max_open_files: None,
        }
    }
}

/// Thin wrapper around a RocksDB instance.
///
/// The wrapper is cheaply cloneable; internally, it wraps a DB instance in an [`Arc`].
#[derive(Debug)]
pub struct RocksDB<CF> {
    inner: Arc<RocksDBInner>,
    sync_writes: bool,
    stalled_writes_retries: StalledWritesRetries,
    _cf: PhantomData<CF>,
}

impl<CF> Clone for RocksDB<CF> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            sync_writes: self.sync_writes,
            stalled_writes_retries: self.stalled_writes_retries,
            _cf: PhantomData,
        }
    }
}

impl<CF: NamedColumnFamily> RocksDB<CF> {
    pub fn new(path: &Path) -> Result<Self, rocksdb::Error> {
        Self::with_options(path, RocksDBOptions::default())
    }

    pub fn with_options(path: &Path, options: RocksDBOptions) -> Result<Self, rocksdb::Error> {
        let caches = RocksDBCaches::new(options.block_cache_capacity);
        let mut db_options = Self::rocksdb_options(None, None);
        let max_open_files = if let Some(non_zero) = options.max_open_files {
            i32::try_from(non_zero.get()).unwrap_or(i32::MAX)
        } else {
            -1
        };
        db_options.set_max_open_files(max_open_files);
        let existing_cfs = DB::list_cf(&db_options, path).unwrap_or_else(|err| {
            tracing::warn!(
                "Failed getting column families for RocksDB `{}` at `{}`, assuming CFs are empty; {err}",
                CF::DB_NAME,
                path.display()
            );
            vec![]
        });

        let cfs_and_options: HashMap<_, _> = CF::ALL
            .iter()
            .map(|cf| (cf.name(), cf.requires_tuning()))
            .collect();
        let obsolete_cfs: Vec<_> = existing_cfs
            .iter()
            .filter_map(|cf_name| {
                let cf_name = cf_name.as_str();
                // The default CF is created on RocksDB instantiation in any case; it doesn't need
                // to be explicitly opened.
                let is_obsolete = cf_name != rocksdb::DEFAULT_COLUMN_FAMILY_NAME
                    && !cfs_and_options.contains_key(cf_name);
                is_obsolete.then_some(cf_name)
            })
            .collect();
        if !obsolete_cfs.is_empty() {
            tracing::warn!(
                "RocksDB `{}` at `{}` contains extra column families {obsolete_cfs:?} that are not used \
                 in code",
                CF::DB_NAME,
                path.display()
            );
        }

        // Open obsolete CFs as well; RocksDB initialization will panic otherwise.
        let cf_names = cfs_and_options.keys().copied().collect();
        let all_cfs_and_options = cfs_and_options
            .into_iter()
            .chain(obsolete_cfs.into_iter().map(|name| (name, false)));
        let cfs = all_cfs_and_options.map(|(cf_name, requires_tuning)| {
            let mut block_based_options = BlockBasedOptions::default();
            block_based_options.set_bloom_filter(10.0, false);
            if let Some(cache) = &caches.shared {
                block_based_options.set_block_cache(cache);
            }
            if options.include_indices_and_filters_in_block_cache {
                block_based_options.set_cache_index_and_filter_blocks(true);
                block_based_options.set_pin_l0_filter_and_index_blocks_in_cache(true);
            }

            let memtable_capacity = options.large_memtable_capacity.filter(|_| requires_tuning);
            let cf_options = Self::rocksdb_options(memtable_capacity, Some(block_based_options));
            ColumnFamilyDescriptor::new(cf_name, cf_options)
        });

        let db = DB::open_cf_descriptors(&db_options, path, cfs)?;
        let inner = Arc::new(RocksDBInner {
            db,
            db_name: CF::DB_NAME,
            cf_names,
            _registry_entry: RegistryEntry::new(),
            _caches: caches,
        });
        RocksdbSizeMetrics::register(CF::DB_NAME, Arc::downgrade(&inner));

        tracing::info!(
            "Initialized RocksDB `{}` at `{}` with {options:?}",
            CF::DB_NAME,
            path.display()
        );

        inner.wait_for_writes_to_resume(&options.stalled_writes_retries);
        Ok(Self {
            inner,
            sync_writes: false,
            stalled_writes_retries: options.stalled_writes_retries,
            _cf: PhantomData,
        })
    }

    /// Switches on sync writes in [`Self::write()`] and [`Self::put()`]. This has a performance
    /// penalty and is mostly useful for tests.
    #[must_use]
    pub fn with_sync_writes(mut self) -> Self {
        self.sync_writes = true;
        self
    }

    fn rocksdb_options(
        memtable_capacity: Option<usize>,
        block_based_options: Option<BlockBasedOptions>,
    ) -> Options {
        let mut options = Options::default();
        options.create_missing_column_families(true);
        options.create_if_missing(true);

        let num_cpus = num_cpus::get() as i32;
        options.increase_parallelism(num_cpus);
        if let Some(memtable_capacity) = memtable_capacity {
            options.optimize_level_style_compaction(memtable_capacity);
        }
        // Settings below are taken as per PingCAP recommendations:
        // https://www.pingcap.com/blog/how-to-troubleshoot-rocksdb-write-stalls-in-tikv/
        let max_background_jobs = (num_cpus - 1).clamp(1, 8);
        options.set_max_background_jobs(max_background_jobs);

        if let Some(block_based_options) = block_based_options {
            options.set_block_based_table_factory(&block_based_options);
        }
        options
    }

    pub fn downgrade(&self) -> WeakRocksDB<CF> {
        WeakRocksDB {
            inner: Arc::downgrade(&self.inner),
            sync_writes: self.sync_writes,
            stalled_writes_retries: self.stalled_writes_retries,
            _cf: PhantomData,
        }
    }

    pub fn estimated_number_of_entries(&self, cf: CF) -> u64 {
        const ERROR_MSG: &str = "failed to get estimated number of entries";

        let cf = self.inner.db.cf_handle(cf.name()).unwrap();
        self.inner
            .db
            .property_int_value_cf(cf, properties::ESTIMATE_NUM_KEYS)
            .expect(ERROR_MSG)
            .unwrap_or(0)
    }

    pub fn multi_get<K, I>(&self, keys: I) -> Vec<Result<Option<Vec<u8>>, rocksdb::Error>>
    where
        K: AsRef<[u8]>,
        I: IntoIterator<Item = K>,
    {
        self.inner.db.multi_get(keys)
    }

    pub fn multi_get_cf<K: AsRef<[u8]>>(
        &self,
        cf: CF,
        keys: impl Iterator<Item = K>,
    ) -> Vec<Result<Option<DBPinnableSlice<'_>>, rocksdb::Error>> {
        let cf = self.column_family(cf);
        self.inner.db.batched_multi_get_cf(cf, keys, false)
    }

    pub fn new_write_batch(&self) -> WriteBatch<'_, CF> {
        WriteBatch {
            inner: rocksdb::WriteBatch::default(),
            db: self,
        }
    }

    pub fn write<'a>(&'a self, batch: WriteBatch<'a, CF>) -> Result<(), rocksdb::Error> {
        let retries = &self.stalled_writes_retries;
        let mut raw_batch = batch.inner;
        let metrics = &METRICS[&DbLabel::from(CF::DB_NAME)];
        metrics.write_batch_size.observe(raw_batch.size_in_bytes());

        if raw_batch.size_in_bytes() > retries.max_batch_size {
            // The write batch is too large to duplicate in RAM.
            return self.write_inner(raw_batch);
        }

        let raw_batch_bytes = raw_batch.data().to_vec();
        let mut retries = self.stalled_writes_retries.intervals();
        let mut stalled_write_reported = false;
        let started_at = Instant::now();
        loop {
            match self.write_inner(raw_batch) {
                Ok(()) => {
                    if stalled_write_reported {
                        metrics.stalled_write_duration.observe(started_at.elapsed());
                    }
                    return Ok(());
                }
                Err(err) => {
                    let is_stalled_write = StalledWritesRetries::is_write_stall_error(&err);
                    if is_stalled_write && !stalled_write_reported {
                        metrics.write_stalled.inc();
                        stalled_write_reported = true;
                    } else {
                        return Err(err);
                    }

                    if let Some(retry_interval) = retries.next() {
                        tracing::warn!(
                            "Writes stalled when writing to DB `{}`; will retry after {retry_interval:?}",
                            CF::DB_NAME
                        );
                        thread::sleep(retry_interval);
                        raw_batch = rocksdb::WriteBatch::from_data(&raw_batch_bytes);
                    } else {
                        return Err(err);
                    }
                }
            }
        }
    }

    fn write_inner(&self, raw_batch: rocksdb::WriteBatch) -> Result<(), rocksdb::Error> {
        if self.sync_writes {
            let mut options = WriteOptions::new();
            options.set_sync(true);
            self.inner.db.write_opt(raw_batch, &options)
        } else {
            self.inner.db.write(raw_batch)
        }
    }

    fn column_family(&self, cf: CF) -> &ColumnFamily {
        self.inner
            .db
            .cf_handle(cf.name())
            .unwrap_or_else(|| panic!("Column family `{}` doesn't exist", cf.name()))
    }

    /// Returns size stats for the specified column family.
    pub fn size_stats(&self, cf: CF) -> SizeStats {
        let cf = self.column_family(cf);
        self.inner.size_stats(cf)
    }

    pub fn get_cf(&self, cf: CF, key: &[u8]) -> Result<Option<Vec<u8>>, rocksdb::Error> {
        let cf = self.column_family(cf);
        self.inner.db.get_cf(cf, key)
    }

    /// Iterates over key-value pairs in the specified column family `cf` in the lexical
    /// key order. The keys are filtered so that they start from the specified `prefix`.
    pub fn prefix_iterator_cf(
        &self,
        cf: CF,
        prefix: &[u8],
    ) -> impl Iterator<Item = (Box<[u8]>, Box<[u8]>)> + '_ {
        let cf = self.column_family(cf);
        let mut options = ReadOptions::default();
        options.set_iterate_range(PrefixRange(prefix));
        self.inner
            .db
            .iterator_cf_opt(cf, options, IteratorMode::Start)
            .map(Result::unwrap)
            .fuse()
        // ^ The RocksDB docs say that a raw iterator (which is used by the returned ordinary iterator)
        // can become invalid "when it reaches the end of its defined range, or when it encounters an error."
        // We panic on RocksDB errors elsewhere and fuse it to prevent polling after the end of the range.
        // Thus, `unwrap()` should be safe.
    }

    /// Iterates over key-value pairs in the specified column family `cf` in the lexical
    /// key order starting from the given `key_from`.
    pub fn from_iterator_cf(
        &self,
        cf: CF,
        keys: ops::RangeFrom<&[u8]>,
    ) -> impl Iterator<Item = (Box<[u8]>, Box<[u8]>)> + '_ {
        let cf = self.column_family(cf);
        self.inner
            .db
            .iterator_cf(cf, IteratorMode::From(keys.start, Direction::Forward))
            .map(Result::unwrap)
            .fuse()
        // ^ unwrap() is safe for the same reasons as in `prefix_iterator_cf()`.
    }

    /// Iterates over key-value pairs in the specified column family `cf` in the reverse lexical
    /// key order starting from the given `key_from`.
    pub fn to_iterator_cf(
        &self,
        cf: CF,
        keys: ops::RangeToInclusive<&[u8]>,
    ) -> impl Iterator<Item = (Box<[u8]>, Box<[u8]>)> + '_ {
        let cf = self.column_family(cf);
        self.inner
            .db
            .iterator_cf(cf, IteratorMode::From(keys.end, Direction::Reverse))
            .map(Result::unwrap)
            .fuse()
        // ^ unwrap() is safe for the same reasons as in `prefix_iterator_cf()`.
    }

    pub fn raw_iterator(&self, cf: CF, options: ReadOptions) -> rocksdb::DBRawIterator<'_> {
        let cf = self.column_family(cf);
        self.inner.db.raw_iterator_cf_opt(cf, options)
    }

    /// Creates a new profiled operation.
    pub fn new_profiled_operation(&self, name: &'static str) -> ProfiledOperation {
        ProfiledOperation {
            db: CF::DB_NAME,
            name,
            is_profiling: ThreadLocal::new(),
            user_key_comparisons: AtomicU64::new(0),
            block_cache_hits: AtomicU64::new(0),
            block_reads: AtomicU64::new(0),
            index_block_cache_hits: AtomicU64::new(0),
            index_block_reads: AtomicU64::new(0),
            filter_block_cache_hits: AtomicU64::new(0),
            filter_block_reads: AtomicU64::new(0),
            gets_from_memtable: AtomicU64::new(0),
            bloom_sst_hits: AtomicU64::new(0),
            bloom_sst_misses: AtomicU64::new(0),
            block_read_size: AtomicU64::new(0),
            multiget_read_size: AtomicU64::new(0),
        }
    }

    /// Returns the checkpoint handle for the RocksDB instance.
    pub fn checkpoint(&self) -> Result<Checkpoint, rocksdb::Error> {
        Checkpoint::new(&self.inner.db)
    }
}

impl RocksDB<()> {
    /// Awaits termination of all running RocksDB instances.
    ///
    /// This method is blocking and should be wrapped in `spawn_blocking(_)` if run in the async context.
    pub fn await_rocksdb_termination() {
        let (lock, cvar) = &ROCKSDB_INSTANCE_COUNTER;
        let mut num_instances = lock.lock().unwrap();
        while *num_instances != 0 {
            tracing::info!(
                "Waiting for all the RocksDB instances to be dropped, {} remaining",
                *num_instances
            );
            num_instances = cvar.wait(num_instances).unwrap();
        }
        tracing::info!("All the RocksDB instances are dropped");
    }
}

/// Weak reference to a RocksDB instance. Doesn't prevent dropping the underlying instance;
/// to work with it, you should [upgrade](Self::upgrade()) the reference first.
///
/// The wrapper is cheaply cloneable; internally, it wraps a DB instance in a [`Weak`].
#[derive(Debug)]
pub struct WeakRocksDB<CF> {
    inner: Weak<RocksDBInner>,
    sync_writes: bool,
    stalled_writes_retries: StalledWritesRetries,
    _cf: PhantomData<CF>,
}

impl<CF: NamedColumnFamily> Clone for WeakRocksDB<CF> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            sync_writes: self.sync_writes,
            stalled_writes_retries: self.stalled_writes_retries,
            _cf: PhantomData,
        }
    }
}

impl<CF: NamedColumnFamily> WeakRocksDB<CF> {
    /// Tries to upgrade to a strong reference to RocksDB. If the RocksDB instance has been dropped, returns `None`.
    pub fn upgrade(&self) -> Option<RocksDB<CF>> {
        Some(RocksDB {
            inner: self.inner.upgrade()?,
            sync_writes: self.sync_writes,
            stalled_writes_retries: self.stalled_writes_retries,
            _cf: PhantomData,
        })
    }
}

/// Profiling information for a logical I/O operation on RocksDB. Can be used to profile operations
/// distributed in time, including on multiple threads.
#[must_use = "`start_profiling()` should be called one or more times to actually perform profiling"]
#[derive(Debug)]
pub struct ProfiledOperation {
    db: &'static str,
    name: &'static str,
    is_profiling: ThreadLocal<Cell<bool>>,
    user_key_comparisons: AtomicU64,
    block_cache_hits: AtomicU64,
    block_reads: AtomicU64,
    index_block_cache_hits: AtomicU64,
    index_block_reads: AtomicU64,
    filter_block_cache_hits: AtomicU64,
    filter_block_reads: AtomicU64,
    gets_from_memtable: AtomicU64,
    bloom_sst_hits: AtomicU64,
    bloom_sst_misses: AtomicU64,
    block_read_size: AtomicU64,
    multiget_read_size: AtomicU64,
}

impl ProfiledOperation {
    /// Starts profiling RocksDB I/O operations until the returned guard is dropped.
    ///
    /// Returns `None` if operations are already being profiled for this operation on the current thread.
    /// Not checking this would lead to logical errors, like the same operations being profiled multiple times.
    pub fn start_profiling(self: &Arc<Self>) -> Option<ProfileGuard> {
        if self.is_profiling.get_or_default().replace(true) {
            // The profiling was already active on the current thread.
            return None;
        }

        perf::set_perf_stats(perf::PerfStatsLevel::EnableCount);
        let mut context = rocksdb::PerfContext::default();
        context.reset();
        Some(ProfileGuard {
            operation: self.clone(),
            context,
        })
    }

    fn block_cache_hits_and_reads(&self, kind: BlockCacheKind) -> (u64, u64) {
        let (hits, reads) = match kind {
            BlockCacheKind::All => (&self.block_cache_hits, &self.block_reads),
            BlockCacheKind::Filters => (&self.filter_block_cache_hits, &self.filter_block_reads),
            BlockCacheKind::Indices => (&self.index_block_cache_hits, &self.index_block_reads),
        };
        (hits.load(Ordering::Relaxed), reads.load(Ordering::Relaxed))
    }
}

impl Drop for ProfiledOperation {
    fn drop(&mut self) {
        tracing::debug!("Profiled operation finished: {self:?}");

        let labels = RocksdbProfilingLabels {
            db: self.db,
            operation: self.name,
        };
        let metrics = &PROF_METRICS[&labels];
        metrics
            .user_key_comparisons
            .observe(self.user_key_comparisons.load(Ordering::Relaxed));
        metrics
            .gets_from_memtable
            .observe(self.gets_from_memtable.load(Ordering::Relaxed));
        metrics
            .bloom_sst_hits
            .observe(self.bloom_sst_hits.load(Ordering::Relaxed));
        metrics
            .bloom_sst_misses
            .observe(self.bloom_sst_misses.load(Ordering::Relaxed));
        metrics
            .block_read_size
            .observe(self.block_read_size.load(Ordering::Relaxed));
        metrics
            .multiget_read_size
            .observe(self.multiget_read_size.load(Ordering::Relaxed));

        for kind in [
            BlockCacheKind::All,
            BlockCacheKind::Filters,
            BlockCacheKind::Indices,
        ] {
            let (hits, reads) = self.block_cache_hits_and_reads(kind);
            if hits > 0 || reads > 0 {
                // Do not report trivial hit / miss stats.
                metrics.block_cache_hits[&kind].observe(hits);
                metrics.block_reads[&kind].observe(reads);
            }
        }
    }
}

#[must_use = "Guard will report metrics on drop"]
pub struct ProfileGuard {
    context: perf::PerfContext,
    operation: Arc<ProfiledOperation>,
}

impl ProfileGuard {
    // Unfortunately, RocksDB doesn't expose all metrics via its C API, so we use this ugly hack to parse the missing metrics
    // directly from the string representation.
    fn parse_metrics_str(s: &str) -> HashMap<&str, u64> {
        let metrics = s.split(',').filter_map(|part| {
            let part = part.trim();
            let (name, value) = part.split_once('=')?;
            Some((name.trim(), value.trim().parse().ok()?))
        });
        metrics.collect()
    }
}

impl fmt::Debug for ProfileGuard {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProfileGuard")
            .field("operation", &self.operation)
            .finish_non_exhaustive()
    }
}

impl Drop for ProfileGuard {
    fn drop(&mut self) {
        let metrics_string = self.context.report(true);
        tracing::trace!("Obtained metrics: {metrics_string}");
        let parsed_metrics = Self::parse_metrics_str(&metrics_string);

        let count = self
            .context
            .metric(perf::PerfMetric::UserKeyComparisonCount);
        self.operation
            .user_key_comparisons
            .fetch_add(count, Ordering::Relaxed);

        let count = self.context.metric(perf::PerfMetric::BlockReadCount);
        self.operation
            .block_reads
            .fetch_add(count, Ordering::Relaxed);
        let count = self.context.metric(perf::PerfMetric::BlockCacheHitCount);
        self.operation
            .block_cache_hits
            .fetch_add(count, Ordering::Relaxed);

        if let Some(&count) = parsed_metrics.get("block_cache_index_hit_count") {
            self.operation
                .index_block_cache_hits
                .fetch_add(count, Ordering::Relaxed);
        }
        if let Some(&count) = parsed_metrics.get("index_block_read_count") {
            self.operation
                .index_block_reads
                .fetch_add(count, Ordering::Relaxed);
        }
        if let Some(&count) = parsed_metrics.get("block_cache_filter_hit_count") {
            self.operation
                .filter_block_cache_hits
                .fetch_add(count, Ordering::Relaxed);
        }
        if let Some(&count) = parsed_metrics.get("filter_block_read_count") {
            self.operation
                .filter_block_reads
                .fetch_add(count, Ordering::Relaxed);
        }

        let count = self.context.metric(perf::PerfMetric::GetFromMemtableCount);
        self.operation
            .gets_from_memtable
            .fetch_add(count, Ordering::Relaxed);
        let count = self.context.metric(perf::PerfMetric::BloomSstHitCount);
        self.operation
            .bloom_sst_hits
            .fetch_add(count, Ordering::Relaxed);
        let count = self.context.metric(perf::PerfMetric::BloomSstMissCount);
        self.operation
            .bloom_sst_misses
            .fetch_add(count, Ordering::Relaxed);

        let size = self.context.metric(perf::PerfMetric::BlockReadByte);
        self.operation
            .block_read_size
            .fetch_add(size, Ordering::Relaxed);
        let size = self.context.metric(perf::PerfMetric::MultigetReadBytes);
        self.operation
            .multiget_read_size
            .fetch_add(size, Ordering::Relaxed);

        self.operation.is_profiling.get_or_default().set(false);
    }
}

/// Empty struct used to register RocksDB instance
#[derive(Debug)]
struct RegistryEntry;

impl RegistryEntry {
    fn new() -> Self {
        let (lock, cvar) = &ROCKSDB_INSTANCE_COUNTER;
        let mut num_instances = lock.lock().unwrap();
        *num_instances += 1;
        cvar.notify_all();
        Self
    }
}

impl Drop for RegistryEntry {
    fn drop(&mut self) {
        let (lock, cvar) = &ROCKSDB_INSTANCE_COUNTER;
        let mut num_instances = lock.lock().unwrap();
        *num_instances -= 1;
        cvar.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn retry_interval_computation() {
        let retries = StalledWritesRetries::new(Duration::from_secs(10));
        let intervals: Vec<_> = retries.intervals().take(20).collect();
        assert_close(intervals[0], Duration::from_millis(50));
        assert_close(intervals[1], Duration::from_millis(75));
        assert_close(intervals[2], Duration::from_micros(112_500));
        assert_close(intervals[19], retries.max_interval);
    }

    #[test]
    fn retries_iterator_is_finite() {
        let retries = StalledWritesRetries::new(Duration::from_millis(10));
        let mut retry_count = 0;
        for _ in retries.intervals() {
            thread::sleep(Duration::from_millis(5));
            retry_count += 1;
        }
        assert!(retry_count <= 2);
    }

    fn assert_close(lhs: Duration, rhs: Duration) {
        let lhs_millis = (lhs.as_secs_f64() * 1_000.0).round() as u64;
        let rhs_millis = (rhs.as_secs_f64() * 1_000.0).round() as u64;
        assert_eq!(lhs_millis, rhs_millis);
    }

    #[derive(Debug, Clone, Copy)]
    enum OldColumnFamilies {
        Default,
        Junk,
    }

    impl NamedColumnFamily for OldColumnFamilies {
        const DB_NAME: &'static str = "test";
        const ALL: &'static [Self] = &[Self::Default, Self::Junk];

        fn name(&self) -> &'static str {
            match self {
                Self::Default => "default",
                Self::Junk => "junk",
            }
        }
    }

    #[derive(Debug, Clone, Copy)]
    enum NewColumnFamilies {
        Default,
        Other,
    }

    impl NamedColumnFamily for NewColumnFamilies {
        const DB_NAME: &'static str = "test";
        const ALL: &'static [Self] = &[Self::Default, Self::Other];

        fn name(&self) -> &'static str {
            match self {
                Self::Default => "default",
                Self::Other => "other",
            }
        }
    }

    #[test]
    fn changing_column_families() {
        let temp_dir = TempDir::new().unwrap();
        let db = RocksDB::<OldColumnFamilies>::new(temp_dir.path())
            .unwrap()
            .with_sync_writes();
        let mut batch = db.new_write_batch();
        batch.put_cf(OldColumnFamilies::Default, b"test", b"value");
        db.write(batch).unwrap();
        drop(db);

        let db = RocksDB::<NewColumnFamilies>::new(temp_dir.path()).unwrap();
        let value = db.get_cf(NewColumnFamilies::Default, b"test").unwrap();
        assert_eq!(value.unwrap(), b"value");
    }

    #[derive(Debug, Clone, Copy)]
    struct JunkColumnFamily;

    impl NamedColumnFamily for JunkColumnFamily {
        const DB_NAME: &'static str = "test";
        const ALL: &'static [Self] = &[Self];

        fn name(&self) -> &'static str {
            "junk"
        }
    }

    #[test]
    fn default_column_family_does_not_need_to_be_explicitly_opened() {
        let temp_dir = TempDir::new().unwrap();
        let db = RocksDB::<OldColumnFamilies>::new(temp_dir.path())
            .unwrap()
            .with_sync_writes();
        let mut batch = db.new_write_batch();
        batch.put_cf(OldColumnFamilies::Junk, b"test", b"value");
        db.write(batch).unwrap();
        drop(db);

        let db = RocksDB::<JunkColumnFamily>::new(temp_dir.path()).unwrap();
        let value = db.get_cf(JunkColumnFamily, b"test").unwrap();
        assert_eq!(value.unwrap(), b"value");
    }

    #[test]
    fn write_batch_can_be_restored_from_bytes() {
        let temp_dir = TempDir::new().unwrap();
        let db = RocksDB::<NewColumnFamilies>::new(temp_dir.path())
            .unwrap()
            .with_sync_writes();
        let mut batch = db.new_write_batch();
        batch.put_cf(NewColumnFamilies::Default, b"test", b"value");
        batch.put_cf(NewColumnFamilies::Default, b"test2", b"value2");
        let batch = WriteBatch {
            db: &db,
            inner: rocksdb::WriteBatch::from_data(batch.inner.data()),
        };
        db.write(batch).unwrap();

        let value = db
            .get_cf(NewColumnFamilies::Default, b"test")
            .unwrap()
            .unwrap();
        assert_eq!(value, b"value");
        let value = db
            .get_cf(NewColumnFamilies::Default, b"test2")
            .unwrap()
            .unwrap();
        assert_eq!(value, b"value2");
    }

    #[test]
    fn profiling_basics() {
        let temp_dir = TempDir::new().unwrap();
        let db = RocksDB::<NewColumnFamilies>::new(temp_dir.path())
            .unwrap()
            .with_sync_writes();

        let mut batch = db.new_write_batch();
        batch.put_cf(NewColumnFamilies::Default, b"test", b"value");
        batch.put_cf(NewColumnFamilies::Default, b"test2", b"value2");
        db.write(batch).unwrap();

        let profiled_op = Arc::new(db.new_profiled_operation("test"));
        assert_eq!(profiled_op.is_profiling.get().map(Cell::get), None);

        {
            let _guard = profiled_op.start_profiling();
            assert_eq!(profiled_op.is_profiling.get().map(Cell::get), Some(true));
            db.get_cf(NewColumnFamilies::Default, b"test")
                .unwrap()
                .unwrap();
        }
        assert_eq!(profiled_op.is_profiling.get().map(Cell::get), Some(false));
        let key_comparisons = profiled_op.user_key_comparisons.load(Ordering::Relaxed);
        assert!(key_comparisons > 0, "{key_comparisons}");

        // Check enabling profiling on another thread.
        let profiled_op_clone = profiled_op.clone();
        thread::spawn(move || {
            let _guard = profiled_op_clone.start_profiling();
            db.get_cf(NewColumnFamilies::Default, b"test2")
                .unwrap()
                .unwrap();
        })
        .join()
        .unwrap();

        assert_eq!(profiled_op.is_profiling.get().map(Cell::get), Some(false));
        let new_key_comparisons = profiled_op.user_key_comparisons.load(Ordering::Relaxed);
        assert!(
            new_key_comparisons > key_comparisons,
            "{key_comparisons}, {new_key_comparisons}"
        );
    }

    #[test]
    fn parsing_metrics_str() {
        let metrics_str = "\
            user_key_comparison_count = 3309113, block_cache_hit_count = 1625, block_read_count = 70834, \
            block_read_byte = 6425766745, block_cache_index_hit_count = 180, index_block_read_count = 3978, \
            block_cache_filter_hit_count = 105, filter_block_read_count = 8234, multiget_read_bytes = 30942097, \
            get_from_memtable_count = 4300, bloom_sst_hit_count = 85326, bloom_sst_miss_count = 179904\
        ";
        let parsed = ProfileGuard::parse_metrics_str(metrics_str);

        assert_eq!(parsed["block_cache_index_hit_count"], 180);
        assert_eq!(parsed["index_block_read_count"], 3_978);
        assert_eq!(parsed["block_cache_filter_hit_count"], 105);
        assert_eq!(parsed["filter_block_read_count"], 8_234);
    }
}

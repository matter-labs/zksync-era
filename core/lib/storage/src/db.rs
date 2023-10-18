use rocksdb::{
    properties, BlockBasedOptions, Cache, ColumnFamily, ColumnFamilyDescriptor, DBPinnableSlice,
    Direction, IteratorMode, Options, PrefixRange, ReadOptions, WriteOptions, DB,
};

use std::{
    collections::HashSet,
    ffi::CStr,
    fmt,
    marker::PhantomData,
    ops,
    path::Path,
    sync::{Arc, Condvar, Mutex},
    thread,
    time::Duration,
};

use crate::metrics::{RocksdbLabels, RocksdbSizeMetrics, METRICS};

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
    pub(crate) fn report_sizes(&self, metrics: &RocksdbSizeMetrics) {
        for &cf_name in &self.cf_names {
            let cf = self.db.cf_handle(cf_name).unwrap();
            // ^ `unwrap()` is safe (CF existence is checked during DB initialization)
            let labels = RocksdbLabels::new(self.db_name, cf_name);

            let live_data_size = self.int_property(cf, properties::ESTIMATE_LIVE_DATA_SIZE);
            if let Some(size) = live_data_size {
                metrics.live_data_size[&labels].set(size);
            }
            let total_sst_file_size = self.int_property(cf, properties::TOTAL_SST_FILES_SIZE);
            if let Some(size) = total_sst_file_size {
                metrics.total_sst_size[&labels].set(size);
            }
            let total_mem_table_size = self.int_property(cf, properties::SIZE_ALL_MEM_TABLES);
            if let Some(size) = total_mem_table_size {
                metrics.total_mem_table_size[&labels].set(size);
            }
            let block_cache_size = self.int_property(cf, properties::BLOCK_CACHE_USAGE);
            if let Some(size) = block_cache_size {
                metrics.block_cache_size[&labels].set(size);
            }
            let index_and_filters_size =
                self.int_property(cf, properties::ESTIMATE_TABLE_READERS_MEM);
            if let Some(size) = index_and_filters_size {
                metrics.index_and_filters_size[&labels].set(size);
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
}

#[derive(Debug, Clone, Copy)]
struct StalledWritesRetries {
    max_batch_size: usize,
    retry_count: usize,
    interval: Duration,
}

impl Default for StalledWritesRetries {
    fn default() -> Self {
        Self {
            max_batch_size: 128 << 20, // 128 MiB
            retry_count: 3,
            interval: Duration::from_millis(100),
        }
    }
}

impl StalledWritesRetries {
    // **NB.** The error message may change between RocksDB versions!
    fn is_write_stall_error(error: &rocksdb::Error) -> bool {
        matches!(error.kind(), rocksdb::ErrorKind::ShutdownInProgress)
            && error.as_ref().ends_with("stalled writes")
    }
}

/// Thin wrapper around a RocksDB instance.
///
/// The wrapper is cheaply cloneable (internally, it wraps a DB instance in an [`Arc`]).
#[derive(Debug, Clone)]
pub struct RocksDB<CF> {
    inner: Arc<RocksDBInner>,
    sync_writes: bool,
    stalled_writes_retries: StalledWritesRetries,
    _cf: PhantomData<CF>,
}

impl<CF: NamedColumnFamily> RocksDB<CF> {
    pub fn new(path: &Path, tune_options: bool) -> Self {
        Self::with_cache(path, tune_options, None)
    }

    pub fn with_cache(
        path: &Path,
        tune_options: bool,
        block_cache_capacity: Option<usize>,
    ) -> Self {
        let caches = RocksDBCaches::new(block_cache_capacity);
        let options = Self::rocksdb_options(tune_options, None);
        let existing_cfs = DB::list_cf(&options, path).unwrap_or_else(|err| {
            tracing::warn!(
                "Failed getting column families for RocksDB `{}` at `{}`, assuming CFs are empty; {err}",
                CF::DB_NAME,
                path.display()
            );
            vec![]
        });

        let cf_names: HashSet<_> = CF::ALL.iter().map(|cf| cf.name()).collect();
        let obsolete_cfs: Vec<_> = existing_cfs
            .iter()
            .filter_map(|cf_name| {
                let cf_name = cf_name.as_str();
                // The default CF is created on RocksDB instantiation in any case; it doesn't need
                // to be explicitly opened.
                let is_obsolete =
                    cf_name != rocksdb::DEFAULT_COLUMN_FAMILY_NAME && !cf_names.contains(cf_name);
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
        let all_cf_names = cf_names.iter().copied().chain(obsolete_cfs);
        let cfs = all_cf_names.map(|cf_name| {
            let mut block_based_options = BlockBasedOptions::default();
            if tune_options {
                block_based_options.set_bloom_filter(10.0, false);
            }
            if let Some(cache) = &caches.shared {
                block_based_options.set_block_cache(cache);
            }
            let cf_options = Self::rocksdb_options(tune_options, Some(block_based_options));
            ColumnFamilyDescriptor::new(cf_name, cf_options)
        });

        let db = DB::open_cf_descriptors(&options, path, cfs).expect("failed to init rocksdb");
        let inner = Arc::new(RocksDBInner {
            db,
            db_name: CF::DB_NAME,
            cf_names,
            _registry_entry: RegistryEntry::new(),
            _caches: caches,
        });
        RocksdbSizeMetrics::register(CF::DB_NAME, Arc::downgrade(&inner));

        Self {
            inner,
            sync_writes: false,
            stalled_writes_retries: StalledWritesRetries::default(),
            _cf: PhantomData,
        }
    }

    /// Switches on sync writes in [`Self::write()`] and [`Self::put()`]. This has a performance
    /// penalty and is mostly useful for tests.
    #[must_use]
    pub fn with_sync_writes(mut self) -> Self {
        self.sync_writes = true;
        self
    }

    fn rocksdb_options(
        tune_options: bool,
        block_based_options: Option<BlockBasedOptions>,
    ) -> Options {
        let mut options = Options::default();
        options.create_missing_column_families(true);
        options.create_if_missing(true);
        if tune_options {
            let num_cpus = num_cpus::get() as i32;
            options.increase_parallelism(num_cpus);
            // Settings below are taken as per PingCAP recommendations:
            // https://www.pingcap.com/blog/how-to-troubleshoot-rocksdb-write-stalls-in-tikv/
            options.set_max_write_buffer_number(5);
            let max_background_jobs = (num_cpus - 1).clamp(1, 8);
            options.set_max_background_jobs(max_background_jobs);
        }
        if let Some(block_based_options) = block_based_options {
            options.set_block_based_table_factory(&block_based_options);
        }
        options
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

    pub fn multi_get_cf(
        &self,
        cf: CF,
        keys: impl Iterator<Item = Vec<u8>>,
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
        METRICS.report_batch_size(CF::DB_NAME, raw_batch.size_in_bytes());

        if raw_batch.size_in_bytes() > retries.max_batch_size {
            // The write batch is too large to duplicate in RAM.
            return self.write_inner(raw_batch);
        }

        let raw_batch_bytes = raw_batch.data().to_vec();
        let mut retry_count = 0;
        loop {
            match self.write_inner(raw_batch) {
                Ok(()) => return Ok(()),
                Err(err) => {
                    let should_retry = StalledWritesRetries::is_write_stall_error(&err)
                        && retry_count < retries.retry_count;
                    if should_retry {
                        tracing::warn!(
                            "Writes stalled when writing to DB `{}`; will retry after a delay",
                            CF::DB_NAME
                        );
                        thread::sleep(retries.interval);
                        retry_count += 1;
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
        // ^ The rocksdb docs say that a raw iterator (which is used by the returned ordinary iterator)
        // can become invalid "when it reaches the end of its defined range, or when it encounters an error."
        // We panic on RocksDB errors elsewhere and fuse it to prevent polling after the end of the range.
        // Thus, `unwrap()` should be safe.
    }

    /// Iterates over key-value pairs in the specified column family `cf` in the lexical
    /// key order starting from the given `key_from`.
    pub fn from_iterator_cf(
        &self,
        cf: CF,
        key_from: &[u8],
    ) -> impl Iterator<Item = (Box<[u8]>, Box<[u8]>)> + '_ {
        let cf = self.column_family(cf);
        self.inner
            .db
            .iterator_cf(cf, IteratorMode::From(key_from, Direction::Forward))
            .map(Result::unwrap)
            .fuse()
        // ^ unwrap() is safe for the same reasons as in `prefix_iterator_cf()`.
    }
}

impl RocksDB<()> {
    /// Awaits termination of all running rocksdb instances.
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

impl<CF> Drop for RocksDB<CF> {
    fn drop(&mut self) {
        self.inner.db.cancel_all_background_work(true);
    }
}

/// Empty struct used to register rocksdb instance
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
        let db = RocksDB::<OldColumnFamilies>::new(temp_dir.path(), true).with_sync_writes();
        let mut batch = db.new_write_batch();
        batch.put_cf(OldColumnFamilies::Default, b"test", b"value");
        db.write(batch).unwrap();
        drop(db);

        let db = RocksDB::<NewColumnFamilies>::new(temp_dir.path(), true);
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
        let db = RocksDB::<OldColumnFamilies>::new(temp_dir.path(), true).with_sync_writes();
        let mut batch = db.new_write_batch();
        batch.put_cf(OldColumnFamilies::Junk, b"test", b"value");
        db.write(batch).unwrap();
        drop(db);

        let db = RocksDB::<JunkColumnFamily>::new(temp_dir.path(), true);
        let value = db.get_cf(JunkColumnFamily, b"test").unwrap();
        assert_eq!(value.unwrap(), b"value");
    }

    #[test]
    fn write_batch_can_be_restored_from_bytes() {
        let temp_dir = TempDir::new().unwrap();
        let db = RocksDB::<NewColumnFamilies>::new(temp_dir.path(), true).with_sync_writes();
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
}

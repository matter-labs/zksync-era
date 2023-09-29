use rocksdb::{
    properties, BlockBasedOptions, Cache, ColumnFamily, ColumnFamilyDescriptor, DBPinnableSlice,
    IteratorMode, Options, PrefixRange, ReadOptions, WriteOptions, DB,
};

use std::{
    collections::HashSet,
    fmt,
    marker::PhantomData,
    ops,
    path::Path,
    sync::{Condvar, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

use crate::metrics::{describe_metrics, RocksDBSizeStats, WriteMetrics};

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

/// Thin wrapper around a RocksDB instance.
#[derive(Debug)]
pub struct RocksDB<CF> {
    db: DB,
    sync_writes: bool,
    sizes_reported_at: Mutex<Option<Instant>>,
    _registry_entry: RegistryEntry,
    _cf: PhantomData<CF>,
    // Importantly, `Cache`s must be dropped after `DB`, so we place them as the last field
    // (fields in a struct are dropped in the declaration order).
    _caches: RocksDBCaches,
}

impl<CF: NamedColumnFamily> RocksDB<CF> {
    const SIZE_REPORT_INTERVAL: Duration = Duration::from_secs(1);

    pub fn new<P: AsRef<Path>>(path: P, tune_options: bool) -> Self {
        Self::with_cache(path, tune_options, None)
    }

    pub fn with_cache<P: AsRef<Path>>(
        path: P,
        tune_options: bool,
        block_cache_capacity: Option<usize>,
    ) -> Self {
        describe_metrics();

        let caches = RocksDBCaches::new(block_cache_capacity);
        let options = Self::rocksdb_options(tune_options, None);
        let existing_cfs = DB::list_cf(&options, path.as_ref()).unwrap_or_else(|err| {
            vlog::warn!(
                "Failed getting column families for RocksDB `{}` at `{}`, assuming CFs are empty; {err}",
                CF::DB_NAME,
                path.as_ref().display()
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
            vlog::warn!(
                "RocksDB `{}` at `{}` contains extra column families {obsolete_cfs:?} that are not used \
                 in code",
                CF::DB_NAME,
                path.as_ref().display()
            );
        }

        // Open obsolete CFs as well; RocksDB initialization will panic otherwise.
        let cfs = cf_names.into_iter().chain(obsolete_cfs).map(|cf_name| {
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

        Self {
            db,
            sync_writes: false,
            sizes_reported_at: Mutex::new(None),
            _registry_entry: RegistryEntry::new(),
            _cf: PhantomData,
            _caches: caches,
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
            options.increase_parallelism(num_cpus::get() as i32);
        }
        if let Some(block_based_options) = block_based_options {
            options.set_block_based_table_factory(&block_based_options);
        }
        options
    }

    pub fn estimated_number_of_entries(&self, cf: CF) -> u64 {
        const ERROR_MSG: &str = "failed to get estimated number of entries";

        let cf = self.db.cf_handle(cf.name()).unwrap();
        self.db
            .property_int_value_cf(cf, properties::ESTIMATE_NUM_KEYS)
            .expect(ERROR_MSG)
            .unwrap_or(0)
    }

    fn size_stats(&self, cf: CF) -> Option<RocksDBSizeStats> {
        const ERROR_MSG: &str = "failed to get RocksDB size property";

        let cf = self.db.cf_handle(cf.name()).unwrap();
        let estimated_live_data_size = self
            .db
            .property_int_value_cf(cf, properties::ESTIMATE_LIVE_DATA_SIZE)
            .expect(ERROR_MSG)?;
        let total_sst_file_size = self
            .db
            .property_int_value_cf(cf, properties::TOTAL_SST_FILES_SIZE)
            .expect(ERROR_MSG)?;
        let total_mem_table_size = self
            .db
            .property_int_value_cf(cf, properties::SIZE_ALL_MEM_TABLES)
            .expect(ERROR_MSG)?;
        let block_cache_size = self
            .db
            .property_int_value_cf(cf, properties::BLOCK_CACHE_USAGE)
            .expect(ERROR_MSG)?;
        let index_and_filters_size = self
            .db
            .property_int_value_cf(cf, properties::ESTIMATE_TABLE_READERS_MEM)
            .expect(ERROR_MSG)?;

        Some(RocksDBSizeStats {
            estimated_live_data_size,
            total_sst_file_size,
            total_mem_table_size,
            block_cache_size,
            index_and_filters_size,
        })
    }

    pub fn multi_get<K, I>(&self, keys: I) -> Vec<Result<Option<Vec<u8>>, rocksdb::Error>>
    where
        K: AsRef<[u8]>,
        I: IntoIterator<Item = K>,
    {
        self.db.multi_get(keys)
    }

    pub fn multi_get_cf(
        &self,
        cf: CF,
        keys: impl Iterator<Item = Vec<u8>>,
    ) -> Vec<Result<Option<DBPinnableSlice<'_>>, rocksdb::Error>> {
        let cf = self.column_family(cf);
        self.db.batched_multi_get_cf(cf, keys, false)
    }

    pub fn new_write_batch(&self) -> WriteBatch<'_, CF> {
        WriteBatch {
            inner: rocksdb::WriteBatch::default(),
            db: self,
        }
    }

    pub fn write<'a>(&'a self, batch: WriteBatch<'a, CF>) -> Result<(), rocksdb::Error> {
        let raw_batch = batch.inner;
        let write_metrics = WriteMetrics {
            batch_size: raw_batch.size_in_bytes() as u64,
        };

        if self.sync_writes {
            let mut options = WriteOptions::new();
            options.set_sync(true);
            self.db.write_opt(raw_batch, &options)?;
        } else {
            self.db.write(raw_batch)?;
        }

        write_metrics.report(CF::DB_NAME);
        // Since getting size stats may take some time, we throttle their reporting.
        let should_report_sizes = self
            .sizes_reported_at()
            .map_or(true, |ts| ts.elapsed() >= Self::SIZE_REPORT_INTERVAL);
        if should_report_sizes {
            for &column_family in CF::ALL {
                if let Some(stats) = self.size_stats(column_family) {
                    stats.report(CF::DB_NAME, column_family.name());
                }
            }
            *self.sizes_reported_at() = Some(Instant::now());
        }
        Ok(())
    }

    fn sizes_reported_at(&self) -> MutexGuard<'_, Option<Instant>> {
        self.sizes_reported_at
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
        // ^ It's really difficult to screw up writing `Option<Instant>`, so we assume that
        // the mutex can recover from poisoning.
    }

    fn column_family(&self, cf: CF) -> &ColumnFamily {
        self.db
            .cf_handle(cf.name())
            .unwrap_or_else(|| panic!("Column family `{}` doesn't exist", cf.name()))
    }

    pub fn get_cf(&self, cf: CF, key: &[u8]) -> Result<Option<Vec<u8>>, rocksdb::Error> {
        let cf = self.column_family(cf);
        self.db.get_cf(cf, key)
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
        self.db
            .iterator_cf_opt(cf, options, IteratorMode::Start)
            .map(Result::unwrap)
            .fuse()
        // ^ The rocksdb docs say that a raw iterator (which is used by the returned ordinary iterator)
        // can become invalid "when it reaches the end of its defined range, or when it encounters an error."
        // We panic on RocksDB errors elsewhere and fuse it to prevent polling after the end of the range.
        // Thus, `unwrap()` should be safe.
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
            vlog::info!(
                "Waiting for all the RocksDB instances to be dropped, {} remaining",
                *num_instances
            );
            num_instances = cvar.wait(num_instances).unwrap();
        }
        vlog::info!("All the RocksDB instances are dropped");
    }
}

impl<CF> Drop for RocksDB<CF> {
    fn drop(&mut self) {
        self.db.cancel_all_background_work(true);
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
}
